//! Sessions and the action log (migration 0004): who is signed in, and what
//! the hub wrote to Forgejo on whose behalf. A proposal, like everything on
//! the publishing side.

use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{Connection, Result, Store, now_unix};

/// A signed-in person, as the API shows them.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Me {
    pub username: String,
    pub full_name: Option<String>,
    pub avatar_url: Option<String>,
    /// In the official org's publishers team at sign-in.
    pub publisher: bool,
}

/// One write the hub made.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Action {
    pub at_unix: i64,
    pub username: String,
    /// `adopt`, `pull` or `publish`.
    pub kind: String,
    /// The official repo, `owner/name`.
    pub target: String,
    /// Where it came from, `owner/name@sha`.
    pub source: Option<String>,
    pub book: Option<String>,
    /// The commit (pull) or tag (publish).
    #[serde(rename = "ref")]
    pub reference: Option<String>,
}

/// What to log.
pub struct NewAction<'a> {
    pub username: &'a str,
    pub kind: &'a str,
    pub target: &'a str,
    pub source: Option<&'a str>,
    pub book: Option<&'a str>,
    pub reference: Option<&'a str>,
    pub detail: Option<serde_json::Value>,
}

impl Store {
    pub fn create_session(&self, id: &str, me: &Me, ttl_secs: i64) -> Result<()> {
        let now = now_unix();
        let w = self.writer();
        // Expired sessions go whenever a new one starts; nothing else needs them.
        w.execute("DELETE FROM session WHERE expires < ?1", params![now])?;
        w.execute(
            "INSERT INTO session (id, username, full_name, avatar_url, publisher, created, expires)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                me.username,
                me.full_name,
                me.avatar_url,
                me.publisher,
                now,
                now + ttl_secs
            ],
        )?;
        Ok(())
    }

    pub fn end_session(&self, id: &str) -> Result<()> {
        self.writer()
            .execute("DELETE FROM session WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn record_action(&self, a: &NewAction<'_>) -> Result<()> {
        self.writer().execute(
            "INSERT INTO action (at, username, kind, target, source, book, ref, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                now_unix(),
                a.username,
                a.kind,
                a.target,
                a.source,
                a.book,
                a.reference,
                a.detail.as_ref().map(|d| d.to_string())
            ],
        )?;
        Ok(())
    }
}

/// The session behind a cookie, if it is still live.
pub fn session(conn: &Connection, id: &str) -> Result<Option<Me>> {
    Ok(conn
        .query_row(
            "SELECT username, full_name, avatar_url, publisher FROM session WHERE id = ?1 AND expires >= ?2",
            params![id, now_unix()],
            |r| {
                Ok(Me {
                    username: r.get(0)?,
                    full_name: r.get(1)?,
                    avatar_url: r.get(2)?,
                    publisher: r.get(3)?,
                })
            },
        )
        .optional()?)
}

/// The log for an official repo, or for anything that came from a repo, newest first.
pub fn actions(conn: &Connection, repo: &str) -> Result<Vec<Action>> {
    let like = format!("{repo}@%");
    let mut stmt = conn.prepare(
        "SELECT at, username, kind, target, source, book, ref FROM action
         WHERE target = ?1 OR source LIKE ?2 ORDER BY at DESC, id DESC LIMIT 200",
    )?;
    let rows = stmt.query_map(params![repo, like], |r| {
        Ok(Action {
            at_unix: r.get(0)?,
            username: r.get(1)?,
            kind: r.get(2)?,
            target: r.get(3)?,
            source: r.get(4)?,
            book: r.get(5)?,
            reference: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// One book of a Writer project, a candidate for consolidation.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WriterBook {
    pub owner: String,
    pub name: String,
    /// Upper-case USFM id.
    pub book: String,
    pub chapters: i64,
    pub verses: i64,
    /// Against the declared source, when one resolves.
    pub source_verses: Option<i64>,
    pub present: Option<i64>,
    pub head_sha: String,
    /// Last updated: HEAD's committer time.
    pub head_unix: i64,
    /// The catalog's status for this repo (`Primary`, `Active`, `Inactive`), when listed.
    pub status: Option<String>,
    pub show_on_biel: bool,
}

/// Every Writer project (served as a synthetic repo) of one language and
/// brand, by book, with what the catalog says about each.
pub fn writer_candidates(
    conn: &Connection,
    language: &str,
    resource: &str,
) -> Result<Vec<WriterBook>> {
    let mut stmt = conn.prepare(
        "SELECT r.owner, r.name, b.book, b.chapters, b.verses, b.source_verses, b.present,
                r.head_sha, coalesce(r.head_unix, 0),
                (SELECT c.status FROM catalog_content c WHERE c.owner = r.owner AND c.name = r.name
                 ORDER BY CASE c.status WHEN 'Primary' THEN 0 WHEN 'Active' THEN 1 ELSE 2 END LIMIT 1),
                coalesce((SELECT max(c.show_on_biel) FROM catalog_content c
                          WHERE c.owner = r.owner AND c.name = r.name), 0)
         FROM repo r JOIN repo_book b ON b.repo_id = r.id
         WHERE r.language = ?1 AND r.resource = ?2 AND r.synthetic IS NOT NULL
           AND r.head_sha IS NOT NULL AND r.is_archived = 0 AND b.book != ''
         ORDER BY b.book, r.owner, r.name",
    )?;
    let rows = stmt.query_map(
        params![language.to_lowercase(), resource.to_lowercase()],
        |r| {
            Ok(WriterBook {
                owner: r.get(0)?,
                name: r.get(1)?,
                book: r.get(2)?,
                chapters: r.get(3)?,
                verses: r.get(4)?,
                source_verses: r.get(5)?,
                present: r.get(6)?,
                head_sha: r.get(7)?,
                head_unix: r.get(8)?,
                status: r.get(9)?,
                show_on_biel: r.get::<_, i64>(10)? != 0,
            })
        },
    )?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The default pick among one book's candidates: Primary and shown on BIEL
/// (what BIEL shows today); else Primary; else the newest Active; else the
/// most verses, then the newest. Mirrors Sefer's `defaultCandidate`, with
/// show-on-BIEL first because that is what "published" means today.
pub fn default_pick(candidates: &[&WriterBook]) -> Option<usize> {
    let rank = |b: &WriterBook| match (b.status.as_deref(), b.show_on_biel) {
        (Some("Primary"), true) => 0,
        (Some("Primary"), false) => 1,
        (Some("Active"), _) => 2,
        (None, _) => 3,
        _ => 4,
    };
    (0..candidates.len()).min_by(|&a, &b| {
        let (x, y) = (candidates[a], candidates[b]);
        rank(x)
            .cmp(&rank(y))
            .then(y.verses.cmp(&x.verses))
            .then(y.head_unix.cmp(&x.head_unix))
    })
}

/// One per book, by `default_pick`.
pub fn writer_books(conn: &Connection, language: &str, resource: &str) -> Result<Vec<WriterBook>> {
    let all = writer_candidates(conn, language, resource)?;
    let mut out = Vec::new();
    for group in all.chunk_by(|a, b| a.book == b.book) {
        let refs: Vec<&WriterBook> = group.iter().collect();
        if let Some(i) = default_pick(&refs) {
            out.push(group[i].clone());
        }
    }
    Ok(out)
}

// --- sources by book (migration 0006) ---------------------------------------

/// One copy of a book in a language and brand: a Writer project, a
/// consolidated repo, or the official one.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BookCopy {
    pub owner: String,
    pub name: String,
    /// Upper-case USFM id.
    pub book: String,
    /// The file in the repo (the stitched file for a Writer project).
    pub path: String,
    pub chapters: i64,
    pub verses: i64,
    pub source_verses: Option<i64>,
    pub head_sha: String,
    pub head_unix: i64,
    pub status: Option<String>,
    pub show_on_biel: bool,
    /// A Writer project, served stitched.
    pub writer: bool,
    /// Proofreading findings in this book at the last run.
    pub findings: Option<i64>,
}

/// Every public copy of every book of one language and brand, by book.
pub fn book_copies(conn: &Connection, language: &str, resource: &str) -> Result<Vec<BookCopy>> {
    let mut stmt = conn.prepare(
        "SELECT r.owner, r.name, b.book, b.path, b.chapters, b.verses, b.source_verses,
                r.head_sha, coalesce(r.head_unix, 0),
                (SELECT c.status FROM catalog_content c WHERE c.owner = r.owner AND c.name = r.name
                 ORDER BY CASE c.status WHEN 'Primary' THEN 0 WHEN 'Active' THEN 1 ELSE 2 END LIMIT 1),
                coalesce((SELECT max(c.show_on_biel) FROM catalog_content c
                          WHERE c.owner = r.owner AND c.name = r.name), 0),
                r.synthetic IS NOT NULL,
                (SELECT json_extract(p.by_book, '$.' || b.book) FROM proof p WHERE p.repo_id = r.id)
         FROM repo r JOIN repo_book b ON b.repo_id = r.id
         WHERE r.language = ?1 AND r.resource = ?2 AND r.head_sha IS NOT NULL
           AND r.is_archived = 0 AND b.book != ''
         ORDER BY b.book, r.owner, r.name",
    )?;
    let rows = stmt.query_map(
        params![language.to_lowercase(), resource.to_lowercase()],
        |r| {
            Ok(BookCopy {
                owner: r.get(0)?,
                name: r.get(1)?,
                book: r.get(2)?,
                path: r.get(3)?,
                chapters: r.get(4)?,
                verses: r.get(5)?,
                source_verses: r.get(6)?,
                head_sha: r.get(7)?,
                head_unix: r.get(8)?,
                status: r.get(9)?,
                show_on_biel: r.get::<_, i64>(10)? != 0,
                writer: r.get::<_, i64>(11)? != 0,
                findings: r.get(12)?,
            })
        },
    )?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The copies set aside in a language and brand: `(book or '*', owner/name)`.
pub fn ignored(conn: &Connection, language: &str, resource: &str) -> Result<Vec<(String, String)>> {
    let mut stmt =
        conn.prepare("SELECT book, repo FROM source_ignore WHERE language = ?1 AND resource = ?2")?;
    let rows = stmt.query_map(
        params![language.to_lowercase(), resource.to_lowercase()],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

impl Store {
    /// Set a copy aside (`ignore`), or bring it back. `book` may be `*`.
    pub fn set_ignored(
        &self,
        language: &str,
        resource: &str,
        book: &str,
        repo: &str,
        username: &str,
        ignore: bool,
    ) -> Result<()> {
        let (l, r, b, o) = (
            language.to_lowercase(),
            resource.to_lowercase(),
            book.to_uppercase(),
            repo.to_lowercase(),
        );
        let w = self.writer();
        if ignore {
            w.execute(
                "INSERT OR REPLACE INTO source_ignore (language, resource, book, repo, username, at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![l, r, b, o, username, now_unix()],
            )?;
        } else {
            // Bringing a copy back also lifts a "for every book" set-aside:
            // the page shows one Bring back, whichever way it was set.
            w.execute(
                "DELETE FROM source_ignore WHERE language = ?1 AND resource = ?2 AND book IN (?3, '*') AND repo = ?4",
                params![l, r, b, o],
            )?;
        }
        Ok(())
    }
}
