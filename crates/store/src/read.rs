//! Reads: the API's response bodies, straight from SQL.
//!
//! Each function takes a read connection ([`Store::reader`]) so a handler can
//! run several on one snapshot.

use std::collections::BTreeMap;

use hub_domain::{Candidate, Status, biel_collates, book_state, official_of, shown_twice_on_biel};
use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use crate::Result;

/// One repo in a list: the feed, search, a language page.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RepoSummary {
    pub owner: String,
    pub name: String,
    /// `ts`, `rc`, `burrito`, `usfm`, `other`, `empty`; null before the first scan.
    pub shape: Option<String>,
    pub language: Option<String>,
    pub resource: Option<String>,
    pub subject: Option<String>,
    pub title: Option<String>,
    /// Committer time of HEAD, Unix seconds.
    pub head_unix: Option<i64>,
    pub topics: Vec<String>,
    /// Served as a synthetic repo (a Writer project stitched into one book).
    pub is_synthetic: bool,
    pub book_count: i64,
    pub errors: i64,
    pub warnings: i64,
    /// The catalog's label, when the public data API lists this repo.
    pub status: Option<String>,
    pub show_on_biel: bool,
}

const SUMMARY_COLUMNS: &str = "r.owner, r.name, r.shape, r.language, r.resource, r.subject, r.title,
  r.head_unix, r.topics, r.synthetic IS NOT NULL,
  (SELECT count(*) FROM repo_book b WHERE b.repo_id = r.id),
  (SELECT coalesce(sum(l.errors), 0) FROM repo_book b JOIN lint_summary l
     ON l.content_key = b.content_key AND l.sk_tag = r.scanned_tag WHERE b.repo_id = r.id),
  (SELECT coalesce(sum(l.warnings), 0) FROM repo_book b JOIN lint_summary l
     ON l.content_key = b.content_key AND l.sk_tag = r.scanned_tag WHERE b.repo_id = r.id),
  (SELECT c.status FROM catalog_content c WHERE c.owner = r.owner AND c.name = r.name
     ORDER BY c.status = 'Primary' DESC LIMIT 1),
  coalesce((SELECT max(c.show_on_biel) FROM catalog_content c WHERE c.owner = r.owner AND c.name = r.name), 0)";

fn summary(r: &Row) -> rusqlite::Result<RepoSummary> {
    Ok(RepoSummary {
        owner: r.get(0)?,
        name: r.get(1)?,
        shape: r.get(2)?,
        language: r.get(3)?,
        resource: r.get(4)?,
        subject: r.get(5)?,
        title: r.get(6)?,
        head_unix: r.get(7)?,
        topics: json_col(r.get::<_, String>(8)?),
        is_synthetic: r.get(9)?,
        book_count: r.get(10)?,
        errors: r.get(11)?,
        warnings: r.get(12)?,
        status: r.get(13)?,
        show_on_biel: r.get::<_, i64>(14)? != 0,
    })
}

fn json_col<T: serde::de::DeserializeOwned + Default>(s: String) -> T {
    serde_json::from_str(&s).unwrap_or_default()
}

/// What the feed and search accept. Every field narrows; none is required.
#[derive(Debug, Clone, Default)]
pub struct RepoFilter {
    /// Matches owner, name, title or language.
    pub q: Option<String>,
    pub language: Option<String>,
    pub shape: Option<String>,
    pub subject: Option<String>,
    pub topic: Option<String>,
    /// Only repos served as synthetic (true) or only real ones (false).
    pub synthetic: Option<bool>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RepoPage {
    pub total: i64,
    pub repos: Vec<RepoSummary>,
}

/// Repos, newest HEAD first.
pub fn repos(conn: &Connection, f: &RepoFilter) -> Result<RepoPage> {
    let mut wh = vec!["r.scanned_unix IS NOT NULL".to_owned()];
    let mut args: Vec<String> = Vec::new();
    // Each clause names its parameter as `?N`, N the value's 1-based index.
    let mut bind = |v: String| {
        args.push(v);
        format!("?{}", args.len())
    };
    if let Some(q) = f.q.as_deref().filter(|q| !q.trim().is_empty()) {
        let p = bind(format!("%{}%", q.trim().to_lowercase()));
        wh.push(format!(
            "(r.owner || '/' || r.name LIKE {p} OR lower(coalesce(r.title, '')) LIKE {p} OR coalesce(r.language, '') LIKE {p}
              OR EXISTS (SELECT 1 FROM language l WHERE l.ietf = r.language
                AND (lower(coalesce(l.name, '')) LIKE {p} OR lower(coalesce(l.english_name, '')) LIKE {p})))"
        ));
    }
    if let Some(v) = &f.language {
        wh.push(format!("r.language = {}", bind(v.to_lowercase())));
    }
    if let Some(v) = &f.shape {
        wh.push(format!("r.shape = {}", bind(v.clone())));
    }
    if let Some(v) = &f.subject {
        wh.push(format!("r.subject = {}", bind(v.clone())));
    }
    if let Some(v) = &f.topic {
        wh.push(format!(
            "EXISTS (SELECT 1 FROM json_each(r.topics) WHERE value = {})",
            bind(v.clone())
        ));
    }
    match f.synthetic {
        Some(true) => wh.push("r.synthetic IS NOT NULL".into()),
        Some(false) => wh.push("r.synthetic IS NULL".into()),
        None => {}
    }
    let wh = wh.join(" AND ");
    let total: i64 = conn.query_row(
        &format!("SELECT count(*) FROM repo r WHERE {wh}"),
        params_from_iter(&args),
        |r| r.get(0),
    )?;
    let sql = format!(
        "SELECT {SUMMARY_COLUMNS} FROM repo r WHERE {wh}
         ORDER BY r.head_unix DESC NULLS LAST, r.owner, r.name LIMIT {} OFFSET {}",
        f.limit.clamp(1, 500),
        f.offset
    );
    let repos = conn
        .prepare(&sql)?
        .query_map(params_from_iter(&args), summary)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(RepoPage { total, repos })
}

/// One book of a repo at HEAD.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BookRow {
    pub book: String,
    pub path: String,
    pub content_key: String,
    pub chapters: i64,
    pub verses: i64,
    pub lint: Option<LintRow>,
    pub coverage: Option<CoverageRow>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LintRow {
    pub errors: i64,
    pub warnings: i64,
    /// `info` and `hint` together.
    pub infos: i64,
    pub by_code: BTreeMap<String, i64>,
    /// Chapter → `[errors, warnings, infos]`; chapter 0 is before the first `\c`.
    pub by_chapter: BTreeMap<String, Vec<i64>>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CoverageRow {
    /// `owner/name` of the source text, e.g. `wa-catalog/en_ulb`.
    pub source: String,
    pub source_verses: i64,
    pub present: i64,
    pub missing: i64,
    /// Verses the source does not have; usually versification, not an error.
    pub extra: i64,
    /// Chapter → missing verse numbers; only chapters with gaps.
    pub gaps: BTreeMap<String, Vec<i64>>,
}

/// A catalog row that points at this repo.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CatalogRef {
    pub content_id: String,
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

const CATALOG_COLUMNS: &str =
    "c.id, c.owner, c.name, c.language, c.resource_type, c.track, c.domain,
  c.status, c.show_on_biel, c.modified_on, c.books";

fn catalog_ref(r: &Row) -> rusqlite::Result<CatalogRef> {
    Ok(CatalogRef {
        content_id: r.get(0)?,
        owner: r.get(1)?,
        name: r.get(2)?,
        language: r.get(3)?,
        resource_type: r.get(4)?,
        track: r.get(5)?,
        domain: r.get(6)?,
        status: r.get(7)?,
        show_on_biel: r.get::<_, i64>(8)? != 0,
        modified_on: r.get(9)?,
        books: json_col(r.get::<_, String>(10)?),
    })
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ProofSummary {
    pub head_sha: String,
    pub sk_tag: String,
    pub ran_unix: i64,
    pub millis: i64,
    pub findings: i64,
    pub by_book: BTreeMap<String, i64>,
    pub by_rule: BTreeMap<String, i64>,
    pub error: Option<String>,
    /// False when HEAD moved since the run; a new run is queued.
    pub current: bool,
}

/// The manifest as the scan read it.
#[derive(Debug, Clone, Serialize, serde::Deserialize, ToSchema)]
pub struct ManifestInfo {
    /// `manifest.yaml`, `manifest.json` or `metadata.json`.
    pub file: String,
    pub language: Option<String>,
    pub resource: Option<String>,
    pub subject: Option<String>,
    pub title: Option<String>,
    pub books: Vec<String>,
    /// Declared source texts, as `lang/resource`.
    pub sources: Vec<String>,
    /// Writer only: chunks marked finished.
    pub finished_chunks: Option<i64>,
    /// Why the manifest did not parse, when it did not.
    pub error: Option<String>,
    /// The whole normalized manifest (format-specific detail).
    #[schema(value_type = Object)]
    pub normalized: Option<Value>,
}

/// What the hub serves in place of a Writer repo's chunk files.
#[derive(Debug, Clone, Serialize, serde::Deserialize, ToSchema)]
pub struct SyntheticInfo {
    /// The stitched book, `NN-BOOK.usfm`.
    pub file: String,
    pub title: String,
    /// Chapters that have at least one chunk.
    pub chapters: Vec<i64>,
    /// How many Writer files it was stitched from.
    pub chunk_files: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RepoDetail {
    /// The same fields the lists show, at the top level.
    #[serde(flatten)]
    pub summary: RepoSummary,
    pub default_branch: Option<String>,
    pub head_branch: Option<String>,
    pub head_sha: Option<String>,
    pub scanned_unix: Option<i64>,
    pub sk_tag: Option<String>,
    pub manifest: Option<ManifestInfo>,
    /// Set when the repo is a Writer project served as a synthetic repo.
    pub synthetic: Option<SyntheticInfo>,
    pub scan_error: Option<String>,
    pub books: Vec<BookRow>,
    pub catalog: Vec<CatalogRef>,
    pub proof: Option<ProofSummary>,
}

pub fn repo(conn: &Connection, owner: &str, name: &str) -> Result<Option<RepoDetail>> {
    let found = conn
        .query_row(
            &format!(
                "SELECT {SUMMARY_COLUMNS}, r.id, r.default_branch, r.head_branch, r.head_sha,
                   r.scanned_unix, r.scanned_tag, r.manifest, r.synthetic, r.scan_error
                 FROM repo r WHERE r.owner = ?1 AND r.name = ?2"
            ),
            params![owner, name],
            |r| {
                Ok((
                    summary(r)?,
                    r.get::<_, i64>(15)?,
                    r.get::<_, Option<String>>(16)?,
                    r.get::<_, Option<String>>(17)?,
                    r.get::<_, Option<String>>(18)?,
                    r.get::<_, Option<i64>>(19)?,
                    r.get::<_, Option<String>>(20)?,
                    r.get::<_, Option<String>>(21)?,
                    r.get::<_, Option<String>>(22)?,
                    r.get::<_, Option<String>>(23)?,
                ))
            },
        )
        .optional()?;
    let Some((
        summary,
        id,
        default_branch,
        head_branch,
        head_sha,
        scanned_unix,
        sk_tag,
        manifest,
        synthetic,
        scan_error,
    )) = found
    else {
        return Ok(None);
    };
    let books = books(conn, id, sk_tag.as_deref())?;
    let catalog = conn
        .prepare(&format!(
            "SELECT {CATALOG_COLUMNS} FROM catalog_content c WHERE c.owner = ?1 AND c.name = ?2"
        ))?
        .query_map(params![owner, name], catalog_ref)?
        .collect::<rusqlite::Result<_>>()?;
    let proof = proof_summary(conn, id, head_sha.as_deref())?;
    Ok(Some(RepoDetail {
        summary,
        default_branch,
        head_branch,
        head_sha,
        scanned_unix,
        sk_tag,
        manifest: manifest.and_then(|m| serde_json::from_str(&m).ok()),
        synthetic: synthetic.and_then(|m| serde_json::from_str(&m).ok()),
        scan_error,
        books,
        catalog,
        proof,
    }))
}

fn books(conn: &Connection, repo_id: i64, sk_tag: Option<&str>) -> Result<Vec<BookRow>> {
    let rows = conn
        .prepare(
            "SELECT b.book, b.path, b.content_key, b.chapters, b.verses,
               l.errors, l.warnings, l.infos, l.by_code, l.by_chapter,
               b.source_repo, b.source_verses, b.present, b.missing, b.extra, b.gaps
             FROM repo_book b
             LEFT JOIN lint_summary l ON l.content_key = b.content_key AND l.sk_tag = ?2
             WHERE b.repo_id = ?1 ORDER BY b.path",
        )?
        .query_map(params![repo_id, sk_tag], |r| {
            let lint = match r.get::<_, Option<i64>>(5)? {
                Some(errors) => Some(LintRow {
                    errors,
                    warnings: r.get(6)?,
                    infos: r.get(7)?,
                    by_code: json_col(r.get::<_, String>(8)?),
                    by_chapter: json_col(r.get::<_, String>(9)?),
                }),
                None => None,
            };
            let coverage = match r.get::<_, Option<String>>(10)? {
                Some(source) => Some(CoverageRow {
                    source,
                    source_verses: r.get(11)?,
                    present: r.get(12)?,
                    missing: r.get(13)?,
                    extra: r.get(14)?,
                    gaps: json_col(r.get::<_, Option<String>>(15)?.unwrap_or_default()),
                }),
                None => None,
            };
            Ok(BookRow {
                book: r.get(0)?,
                path: r.get(1)?,
                content_key: r.get(2)?,
                chapters: r.get(3)?,
                verses: r.get(4)?,
                lint,
                coverage,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

fn proof_summary(
    conn: &Connection,
    repo_id: i64,
    head: Option<&str>,
) -> Result<Option<ProofSummary>> {
    Ok(conn
        .query_row(
            "SELECT head_sha, sk_tag, ran_unix, millis, findings, by_book, by_rule, error
             FROM proof WHERE repo_id = ?1",
            [repo_id],
            |r| {
                let sha: String = r.get(0)?;
                Ok(ProofSummary {
                    current: Some(sha.as_str()) == head,
                    head_sha: sha,
                    sk_tag: r.get(1)?,
                    ran_unix: r.get(2)?,
                    millis: r.get(3)?,
                    findings: r.get(4)?,
                    by_book: json_col(r.get::<_, String>(5)?),
                    by_rule: json_col(r.get::<_, String>(6)?),
                    error: r.get(7)?,
                })
            },
        )
        .optional()?)
}

/// The stored sous-chef findings for a repo, as JSON text (served as is).
pub fn proof_body(conn: &Connection, owner: &str, name: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT p.body FROM proof p JOIN repo r ON r.id = p.repo_id WHERE r.owner = ?1 AND r.name = ?2",
            params![owner, name],
            |r| r.get(0),
        )
        .optional()?)
}

/// The repo's id and HEAD, for handlers that read files.
pub fn repo_head(
    conn: &Connection,
    owner: &str,
    name: &str,
) -> Result<Option<(i64, Option<String>, bool)>> {
    Ok(conn
        .query_row(
            "SELECT id, head_sha, synthetic IS NOT NULL FROM repo WHERE owner = ?1 AND name = ?2",
            params![owner, name],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?)
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LanguageSummary {
    pub ietf: String,
    pub name: Option<String>,
    pub english_name: Option<String>,
    pub direction: Option<String>,
    pub region: Option<String>,
    pub gateway: Option<bool>,
    pub alternate_names: Vec<String>,
    /// Scanned public repos whose manifest names this language.
    pub repos: i64,
}

fn language_summary(r: &Row) -> rusqlite::Result<LanguageSummary> {
    Ok(LanguageSummary {
        ietf: r.get(0)?,
        name: r.get(1)?,
        english_name: r.get(2)?,
        direction: r.get(3)?,
        region: r.get(4)?,
        gateway: r.get(5)?,
        alternate_names: json_col(r.get::<_, Option<String>>(6)?.unwrap_or_default()),
        repos: r.get(7)?,
    })
}

/// Languages that have repos here, matched on code, names or alternate names.
/// A language the catalog does not know still shows, by its code alone.
pub fn languages(conn: &Connection, q: Option<&str>, limit: u32) -> Result<Vec<LanguageSummary>> {
    let like = format!("%{}%", q.unwrap_or("").trim().to_lowercase());
    let rows = conn
        .prepare(
            "WITH counts AS (
               SELECT language AS ietf, count(*) AS repos FROM repo
               WHERE language IS NOT NULL AND scanned_unix IS NOT NULL GROUP BY language)
             SELECT c.ietf, l.name, l.english_name, l.direction, l.region, l.gateway,
                    l.alternate_names, c.repos
             FROM counts c LEFT JOIN language l ON l.ietf = c.ietf
             WHERE c.ietf LIKE ?1 OR lower(coalesce(l.name, '')) LIKE ?1
                OR lower(coalesce(l.english_name, '')) LIKE ?1
                OR lower(coalesce(l.alternate_names, '')) LIKE ?1
             ORDER BY c.ietf = trim(?2) DESC, c.repos DESC LIMIT ?3",
        )?
        .query_map(
            params![like, q.unwrap_or("").to_lowercase(), limit.clamp(1, 1000)],
            language_summary,
        )?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// One book of one version (resource type + track) in a language, with the
/// repos that could be its text and the rules' verdicts on them.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BookPick {
    pub resource_type: String,
    pub track: String,
    pub book: String,
    /// Sefer's `defaultCandidate`: which repo is Official.
    pub official: Option<String>,
    /// `official`, `draft`, `conflict`, `inactive`, `empty`.
    #[schema(value_type = String)]
    pub state: hub_domain::BookState,
    /// BIEL stitches two sources for this book (both Primary and shown).
    pub shown_twice_on_biel: bool,
    pub candidates: Vec<CatalogRef>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct LanguageDetail {
    pub ietf: String,
    pub facts: Option<LanguageSummary>,
    pub repos: Vec<RepoSummary>,
    /// From the catalog: per version and book, which repo is Official.
    pub books: Vec<BookPick>,
}

impl Candidate for CatalogRef {
    fn status(&self) -> Option<Status> {
        self.status.as_deref().and_then(Status::parse)
    }
    fn modified(&self) -> Option<&str> {
        self.modified_on.as_deref()
    }
    fn shown_on_biel(&self) -> bool {
        self.show_on_biel
    }
}

pub fn language(conn: &Connection, ietf: &str) -> Result<Option<LanguageDetail>> {
    let ietf = ietf.to_lowercase();
    let facts = conn
        .query_row(
            "SELECT l.ietf, l.name, l.english_name, l.direction, l.region, l.gateway, l.alternate_names,
               (SELECT count(*) FROM repo r WHERE r.language = l.ietf AND r.scanned_unix IS NOT NULL)
             FROM language l WHERE l.ietf = ?1",
            [&ietf],
            language_summary,
        )
        .optional()?;
    let repos: Vec<RepoSummary> = conn
        .prepare(&format!(
            "SELECT {SUMMARY_COLUMNS} FROM repo r WHERE r.language = ?1 AND r.scanned_unix IS NOT NULL
             ORDER BY r.resource, r.head_unix DESC"
        ))?
        .query_map([&ietf], summary)?
        .collect::<rusqlite::Result<_>>()?;
    let catalog: Vec<CatalogRef> = conn
        .prepare(&format!(
            "SELECT {CATALOG_COLUMNS} FROM catalog_content c WHERE c.language = ?1"
        ))?
        .query_map([&ietf], catalog_ref)?
        .collect::<rusqlite::Result<_>>()?;
    if facts.is_none() && repos.is_empty() && catalog.is_empty() {
        return Ok(None);
    }
    Ok(Some(LanguageDetail {
        books: picks(catalog),
        ietf,
        facts,
        repos,
    }))
}

/// Groups catalog rows by (resource type, track, book) and applies the rules.
fn picks(catalog: Vec<CatalogRef>) -> Vec<BookPick> {
    let mut groups: BTreeMap<(String, String, String), Vec<CatalogRef>> = BTreeMap::new();
    for c in catalog {
        for book in &c.books {
            let key = (
                c.resource_type.clone().unwrap_or_default(),
                c.track.clone().unwrap_or_default(),
                book.clone(),
            );
            groups.entry(key).or_default().push(c.clone());
        }
    }
    groups
        .into_iter()
        .map(|((resource_type, track, book), candidates)| BookPick {
            official: official_of(&candidates).map(|c| {
                format!(
                    "{}/{}",
                    c.owner.as_deref().unwrap_or("?"),
                    c.name.as_deref().unwrap_or("?")
                )
            }),
            state: book_state(&candidates),
            shown_twice_on_biel: shown_twice_on_biel(&candidates),
            resource_type,
            track,
            book,
            candidates,
        })
        .collect()
}

/// A book BIEL shows twice: two repos both Primary and shown, for one
/// language, domain, resource type and track (workTracker's rule).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BielConflict {
    pub language: String,
    pub language_name: Option<String>,
    pub domain: String,
    pub resource_type: String,
    pub track: String,
    pub book: String,
    pub repos: Vec<CatalogRef>,
}

pub fn biel_conflicts(conn: &Connection) -> Result<Vec<BielConflict>> {
    // Only the candidates matter: Primary AND shown, in a stitched domain.
    let rows: Vec<(CatalogRef, Option<bool>, Option<String>)> = conn
        .prepare(&format!(
            "SELECT {CATALOG_COLUMNS}, l.gateway, coalesce(l.english_name, l.name)
             FROM catalog_content c LEFT JOIN language l ON l.ietf = c.language
             WHERE c.status = 'Primary' AND c.show_on_biel = 1"
        ))?
        .query_map([], |r| Ok((catalog_ref(r)?, r.get(11)?, r.get(12)?)))?
        .collect::<rusqlite::Result<_>>()?;
    type Key = (String, String, String, String, String);
    let mut groups: BTreeMap<Key, (Option<String>, Vec<CatalogRef>)> = BTreeMap::new();
    for (c, gateway, language_name) in rows {
        let lang = c.language.clone().unwrap_or_default();
        let domain = c.domain.clone().unwrap_or_default();
        if !hub_domain::STITCHED_DOMAINS.contains(&domain.as_str())
            || !biel_collates(gateway.unwrap_or(false), &lang)
        {
            continue;
        }
        for book in &c.books {
            let key = (
                lang.clone(),
                domain.clone(),
                c.resource_type.clone().unwrap_or_default(),
                c.track.clone().unwrap_or_default(),
                book.clone(),
            );
            let entry = groups
                .entry(key)
                .or_insert_with(|| (language_name.clone(), Vec::new()));
            entry.1.push(c.clone());
        }
    }
    Ok(groups
        .into_iter()
        .filter(|(_, (_, repos))| shown_twice_on_biel(repos))
        .map(
            |((language, domain, resource_type, track, book), (language_name, repos))| {
                BielConflict {
                    language,
                    language_name,
                    domain,
                    resource_type,
                    track,
                    book,
                    repos,
                }
            },
        )
        .collect())
}

/// One point of a book's history: what the hub saw when it changed.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HistoryPoint {
    pub at_unix: i64,
    pub head_sha: String,
    pub head_unix: Option<i64>,
    pub sk_tag: String,
    pub chapters: i64,
    pub verses: i64,
    pub errors: i64,
    pub warnings: i64,
    pub missing: Option<i64>,
}

pub fn book_history(
    conn: &Connection,
    owner: &str,
    name: &str,
    book: &str,
) -> Result<Vec<HistoryPoint>> {
    Ok(conn
        .prepare(
            "SELECT h.at_unix, h.head_sha, h.head_unix, h.sk_tag, h.chapters, h.verses, h.errors,
               h.warnings, h.missing
             FROM book_history h JOIN repo r ON r.id = h.repo_id
             WHERE r.owner = ?1 AND r.name = ?2 AND h.book = ?3 ORDER BY h.at_unix",
        )?
        .query_map(params![owner, name, book.to_uppercase()], |r| {
            Ok(HistoryPoint {
                at_unix: r.get(0)?,
                head_sha: r.get(1)?,
                head_unix: r.get(2)?,
                sk_tag: r.get(3)?,
                chapters: r.get(4)?,
                verses: r.get(5)?,
                errors: r.get(6)?,
                warnings: r.get(7)?,
                missing: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Totals {
    pub repos: i64,
    pub scanned: i64,
    pub synthetic: i64,
    pub books: i64,
    pub errors: i64,
    pub warnings: i64,
    pub languages: i64,
    pub catalog_rows: i64,
    pub proofread: i64,
    /// Watermarks: last poll, last catalog fetch, and so on (Unix seconds).
    pub state: BTreeMap<String, String>,
}

pub fn totals(conn: &Connection) -> Result<Totals> {
    let n = |sql: &str| -> rusqlite::Result<i64> { conn.query_row(sql, [], |r| r.get(0)) };
    let state = conn
        .prepare("SELECT key, value FROM sync_state")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Totals {
        repos: n("SELECT count(*) FROM repo")?,
        scanned: n("SELECT count(*) FROM repo WHERE scanned_unix IS NOT NULL")?,
        synthetic: n("SELECT count(*) FROM repo WHERE synthetic IS NOT NULL")?,
        books: n("SELECT count(*) FROM repo_book")?,
        errors: n("SELECT coalesce(sum(l.errors), 0) FROM repo_book b JOIN repo r ON r.id = b.repo_id
                   JOIN lint_summary l ON l.content_key = b.content_key AND l.sk_tag = r.scanned_tag")?,
        warnings: n("SELECT coalesce(sum(l.warnings), 0) FROM repo_book b JOIN repo r ON r.id = b.repo_id
                     JOIN lint_summary l ON l.content_key = b.content_key AND l.sk_tag = r.scanned_tag")?,
        languages: n("SELECT count(*) FROM language")?,
        catalog_rows: n("SELECT count(*) FROM catalog_content")?,
        proofread: n("SELECT count(*) FROM proof")?,
        state,
    })
}
