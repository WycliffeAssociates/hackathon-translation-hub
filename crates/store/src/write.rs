//! Writes: Forgejo's repo list, a scan's results, the catalog copy, proof runs.

use std::collections::HashSet;

use hub_ingest::RepoScan;
use hub_ingest::forgejo::ForgejoRepo;
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::{Result, Store, now_unix};

/// A repo the poll should scan again.
#[derive(Debug, Clone)]
pub struct StaleRepo {
    pub id: i64,
    pub owner: String,
    pub name: String,
    pub head_sha: Option<String>,
}

/// What the last scan of a repo saw, for the poll to compare with now.
#[derive(Debug, Clone)]
pub struct ScanMark {
    pub repo: StaleRepo,
    pub scanned_tag: Option<String>,
    pub scanned_updated: Option<i64>,
    /// Forgejo's `updated_unix` as last synced.
    pub updated: i64,
}

/// Where a repo's last sous-chef run was made.
#[derive(Debug, Clone)]
pub struct ProofMark {
    pub repo: StaleRepo,
    pub head_sha: Option<String>,
    pub sk_tag: Option<String>,
    /// The repo's language, from its manifest.
    pub language: Option<String>,
    /// The source its last scan resolved (any book's `source_repo`), now.
    pub source_repo: Option<String>,
    /// The source and its HEAD the last run read.
    pub proof_source_repo: Option<String>,
    pub proof_source_sha: Option<String>,
}

/// One language from the public data API, as stored.
#[derive(Debug, Clone)]
pub struct LanguageRow {
    pub ietf: String,
    pub name: Option<String>,
    pub english_name: Option<String>,
    pub direction: Option<String>,
    pub region: Option<String>,
    pub gateway: Option<bool>,
    pub alternate_names: Vec<String>,
}

/// One catalog content row that points at a WACS repo, as stored.
#[derive(Debug, Clone)]
pub struct CatalogRow {
    pub id: String,
    pub owner: Option<String>,
    pub name: Option<String>,
    pub language: Option<String>,
    pub resource_type: Option<String>,
    pub track: Option<String>,
    pub domain: Option<String>,
    pub status: Option<String>,
    pub show_on_biel: bool,
    pub modified_on: Option<String>,
    pub books: Vec<String>,
}

/// One sous-chef run over a repo at a commit. `body` is what the API serves.
#[derive(Debug, Clone)]
pub struct ProofRun<B: Serialize> {
    pub repo_id: i64,
    pub head_sha: String,
    /// The source text the run compared against (`owner/name`) and its HEAD;
    /// `None` when the repo resolves no source.
    pub source_repo: Option<String>,
    pub source_sha: Option<String>,
    pub millis: i64,
    pub findings: usize,
    pub by_book: std::collections::BTreeMap<String, usize>,
    pub by_rule: std::collections::BTreeMap<String, usize>,
    /// The run's books and skipped files (the API's proof summary body).
    pub body: B,
    /// Every finding, in book order. All are counted; the first
    /// `rows_per_book_code` of each (book, code) are stored as rows.
    pub rows: Vec<ProofRow>,
    pub rows_per_book_code: usize,
    pub error: Option<String>,
}

/// One sous-chef finding as stored: its keys, and the JSON the API serves.
#[derive(Debug, Clone)]
pub struct ProofRow {
    pub book: String,
    pub code: String,
    /// `hygiene` or `convention`.
    pub severity: String,
    pub body: String,
}

impl Store {
    /// Copies Forgejo's repo list in. Public repos that `on_disk` accepts are
    /// upserted; every other row (now private, deleted, or not on this disk)
    /// is removed, with its books and history.
    pub fn sync_forgejo(
        &self,
        repos: &[ForgejoRepo],
        on_disk: impl Fn(&ForgejoRepo) -> bool,
    ) -> Result<SyncCounts> {
        let mut conn = self.writer();
        let tx = conn.transaction()?;
        let mut keep = HashSet::new();
        {
            let mut upsert = tx.prepare(
                "INSERT INTO repo (id, owner, name, default_branch, topics, is_mirror, is_archived, updated_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT (id) DO UPDATE SET owner = excluded.owner, name = excluded.name,
                   default_branch = excluded.default_branch, topics = excluded.topics,
                   is_mirror = excluded.is_mirror, is_archived = excluded.is_archived,
                   updated_unix = excluded.updated_unix",
            )?;
            for r in repos.iter().filter(|r| r.is_public() && on_disk(r)) {
                upsert.execute(params![
                    r.id,
                    r.owner,
                    r.name,
                    r.default_branch,
                    serde_json::to_string(&r.topics)?,
                    r.is_mirror,
                    r.is_archived,
                    r.updated_unix,
                ])?;
                keep.insert(r.id);
            }
        }
        let existing: Vec<i64> = tx
            .prepare("SELECT id FROM repo")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut removed = 0;
        for id in existing.into_iter().filter(|id| !keep.contains(id)) {
            removed += tx.execute("DELETE FROM repo WHERE id = ?1", [id])?;
        }
        tx.commit()?;
        Ok(SyncCounts {
            public: keep.len(),
            removed,
        })
    }

    /// Every repo with what the last scan saw, for the poll to compare
    /// against HEAD on disk and the current kitchen tag.
    pub fn scan_marks(&self) -> Result<Vec<ScanMark>> {
        let conn = self.writer();
        let mut stmt = conn.prepare(
            "SELECT id, owner, name, head_sha, scanned_tag, scanned_updated, updated_unix FROM repo",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ScanMark {
                    repo: StaleRepo {
                        id: r.get(0)?,
                        owner: r.get(1)?,
                        name: r.get(2)?,
                        head_sha: r.get(3)?,
                    },
                    scanned_tag: r.get(4)?,
                    scanned_updated: r.get(5)?,
                    updated: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Stores one scan: the repo's HEAD facts, its books (replacing the old
    /// ones), lint summaries by content, and a history row for each book whose
    /// content or kitchen tag changed.
    pub fn write_scan(&self, repo_id: i64, scan: &RepoScan, sk_tag: &str) -> Result<()> {
        let now = now_unix();
        let mut conn = self.writer();
        let tx = conn.transaction()?;
        let manifest = scan.manifest.as_ref();
        tx.execute(
            "UPDATE repo SET scanned_unix = ?2, scanned_updated = updated_unix, scanned_tag = ?3,
               head_branch = ?4, head_sha = ?5, head_unix = ?6, shape = ?7, language = ?8,
               resource = ?9, subject = ?10, title = ?11, manifest = ?12, synthetic = ?13,
               scan_error = ?14
             WHERE id = ?1",
            params![
                repo_id,
                now,
                sk_tag,
                scan.branch,
                scan.sha,
                scan.head_at,
                scan.shape.as_str(),
                manifest
                    .and_then(|m| m.language.as_deref())
                    .map(str::to_lowercase),
                manifest
                    .and_then(|m| m.resource.as_deref())
                    .map(str::to_lowercase),
                manifest.and_then(|m| m.subject.clone()),
                scan.synthetic
                    .as_ref()
                    .map(|s| s.title.clone())
                    .or_else(|| manifest.and_then(|m| m.title.clone())),
                manifest.map(serde_json::to_string).transpose()?,
                scan.synthetic
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                scan.error,
            ],
        )?;
        tx.execute("DELETE FROM repo_book WHERE repo_id = ?1", [repo_id])?;
        tx.execute(
            "DELETE FROM finding_count WHERE repo_id = ?1 AND engine = 'lint'",
            [repo_id],
        )?;
        {
            let mut count = tx.prepare(
                "INSERT INTO finding_count (repo_id, engine, book, code, severity, n)
                 VALUES (?1, 'lint', ?2, ?3, ?4, ?5)
                 ON CONFLICT (repo_id, engine, book, code) DO UPDATE SET n = n + excluded.n",
            )?;
            for b in &scan.books {
                let book = if b.book.is_empty() { &b.path } else { &b.book };
                for (code, n) in &b.lint.by_code {
                    let severity = b.lint.severity.get(code).copied().unwrap_or("info");
                    count.execute(params![repo_id, book, code, severity, n])?;
                }
            }
        }
        {
            let mut book = tx.prepare(
                "INSERT OR REPLACE INTO repo_book (repo_id, book, path, content_key, chapters, verses,
                   source_repo, source_verses, present, missing, extra, gaps)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;
            let mut lint = tx.prepare(
                "INSERT OR IGNORE INTO lint_summary (content_key, sk_tag, errors, warnings, infos, by_code, by_chapter)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            let mut last = tx.prepare(
                "SELECT content_key, sk_tag FROM book_history WHERE repo_id = ?1 AND book = ?2
                 ORDER BY at_unix DESC LIMIT 1",
            )?;
            let mut history = tx.prepare(
                "INSERT OR REPLACE INTO book_history (repo_id, book, at_unix, head_sha, head_unix, content_key,
                   sk_tag, chapters, verses, errors, warnings, missing)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;
            for b in &scan.books {
                let c = b.coverage.as_ref();
                book.execute(params![
                    repo_id,
                    b.book,
                    b.path,
                    b.content_key,
                    b.chapters,
                    b.verses,
                    c.map(|c| c.source.clone()),
                    c.map(|c| c.source_verses),
                    c.map(|c| c.present),
                    c.map(|c| c.missing),
                    c.map(|c| c.extra),
                    c.map(|c| serde_json::to_string(&c.gaps)).transpose()?,
                ])?;
                lint.execute(params![
                    b.content_key,
                    sk_tag,
                    b.lint.errors,
                    b.lint.warnings,
                    b.lint.infos,
                    serde_json::to_string(&b.lint.by_code)?,
                    serde_json::to_string(&b.lint.by_chapter)?,
                ])?;
                let seen: Option<(String, String)> = last
                    .query_row(params![repo_id, b.book], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()?;
                let changed = seen.is_none_or(|(key, tag)| key != b.content_key || tag != sk_tag);
                if changed && let Some(sha) = &scan.sha {
                    history.execute(params![
                        repo_id,
                        b.book,
                        now,
                        sha,
                        scan.head_at,
                        b.content_key,
                        sk_tag,
                        b.chapters,
                        b.verses,
                        b.lint.errors,
                        b.lint.warnings,
                        c.map(|c| c.missing),
                    ])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Replaces the language copy with a fresh fetch.
    pub fn replace_languages(&self, rows: &[LanguageRow]) -> Result<()> {
        let now = now_unix();
        let mut conn = self.writer();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM language", [])?;
        {
            let mut insert = tx.prepare(
                "INSERT OR REPLACE INTO language (ietf, name, english_name, direction, region, gateway,
                   alternate_names, fetched_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for l in rows {
                insert.execute(params![
                    l.ietf.to_lowercase(),
                    l.name,
                    l.english_name,
                    l.direction,
                    l.region,
                    l.gateway,
                    serde_json::to_string(&l.alternate_names)?,
                    now,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Replaces the catalog copy with a fresh fetch.
    pub fn replace_catalog(&self, rows: &[CatalogRow]) -> Result<()> {
        let now = now_unix();
        let mut conn = self.writer();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM catalog_content", [])?;
        {
            let mut insert = tx.prepare(
                "INSERT OR REPLACE INTO catalog_content (id, owner, name, language, resource_type, track,
                   domain, status, show_on_biel, modified_on, books, fetched_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;
            for c in rows {
                insert.execute(params![
                    c.id,
                    c.owner.as_deref().map(str::to_lowercase),
                    c.name.as_deref().map(str::to_lowercase),
                    c.language.as_deref().map(str::to_lowercase),
                    c.resource_type,
                    c.track,
                    c.domain,
                    c.status,
                    c.show_on_biel,
                    c.modified_on,
                    serde_json::to_string(&c.books)?,
                    now,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Stores one proofreading run: its summary, a count per (book, code), and
    /// the first rows of each (book, code). Replaces the repo's previous run.
    pub fn write_proof<B: Serialize>(&self, run: &ProofRun<B>, sk_tag: &str) -> Result<()> {
        let mut conn = self.writer();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO proof (repo_id, head_sha, sk_tag, ran_unix, millis, findings,
               by_book, by_rule, body, error, source_repo, source_sha)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                run.repo_id,
                run.head_sha,
                sk_tag,
                now_unix(),
                run.millis,
                run.findings as i64,
                serde_json::to_string(&run.by_book)?,
                serde_json::to_string(&run.by_rule)?,
                serde_json::to_string(&run.body)?,
                run.error,
                run.source_repo,
                run.source_sha,
            ],
        )?;
        tx.execute(
            "DELETE FROM proof_finding WHERE repo_id = ?1",
            [run.repo_id],
        )?;
        tx.execute(
            "DELETE FROM finding_count WHERE repo_id = ?1 AND engine = 'sous'",
            [run.repo_id],
        )?;
        {
            let mut counts: std::collections::BTreeMap<(&str, &str), (usize, &str)> =
                std::collections::BTreeMap::new();
            let mut row = tx.prepare(
                "INSERT INTO proof_finding (repo_id, book, code, seq, body) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for f in &run.rows {
                let entry = counts
                    .entry((f.book.as_str(), f.code.as_str()))
                    .or_insert((0, f.severity.as_str()));
                if entry.0 < run.rows_per_book_code {
                    row.execute(params![run.repo_id, f.book, f.code, entry.0 as i64, f.body])?;
                }
                entry.0 += 1;
            }
            let mut count = tx.prepare(
                "INSERT INTO finding_count (repo_id, engine, book, code, severity, n)
                 VALUES (?1, 'sous', ?2, ?3, ?4, ?5)",
            )?;
            for ((book, code), (n, severity)) in counts {
                count.execute(params![run.repo_id, book, code, severity, n as i64])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The HEAD each repo was last proofread at, for repos with books.
    pub fn proof_marks(&self) -> Result<Vec<ProofMark>> {
        let conn = self.writer();
        let mut stmt = conn.prepare(
            "SELECT r.id, r.owner, r.name, r.head_sha, p.head_sha, p.sk_tag, r.language,
               (SELECT b.source_repo FROM repo_book b
                 WHERE b.repo_id = r.id AND b.source_repo IS NOT NULL LIMIT 1),
               p.source_repo, p.source_sha
             FROM repo r
             LEFT JOIN proof p ON p.repo_id = r.id
             WHERE r.head_sha IS NOT NULL AND EXISTS (SELECT 1 FROM repo_book b WHERE b.repo_id = r.id)",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ProofMark {
                    repo: StaleRepo {
                        id: r.get(0)?,
                        owner: r.get(1)?,
                        name: r.get(2)?,
                        head_sha: r.get(3)?,
                    },
                    head_sha: r.get(4)?,
                    sk_tag: r.get(5)?,
                    language: r.get(6)?,
                    source_repo: r.get(7)?,
                    proof_source_repo: r.get(8)?,
                    proof_source_sha: r.get(9)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn set_state(&self, key: &str, value: &str) -> Result<()> {
        self.writer().execute(
            "INSERT OR REPLACE INTO sync_state (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SyncCounts {
    pub public: usize,
    pub removed: usize,
}
