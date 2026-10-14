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
