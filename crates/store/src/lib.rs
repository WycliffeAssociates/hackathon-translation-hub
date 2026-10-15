//! The hub's SQLite: migrations, writes from the pipeline and the catalog
//! sync, and the reads the API serves.
//!
//! One writer, many readers: writes go through a single connection behind a
//! mutex (SQLite allows one writer anyway), and each read opens its own
//! connection in WAL mode, so a long rescan never blocks a page.
//!
//! The read models in [`read`] are the API's response bodies, so they derive
//! `utoipa::ToSchema` here rather than being copied into the server.

pub mod publishing;
pub mod read;
mod write;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// Re-exported so callers of [`Store::reader`] need no rusqlite dependency.
pub use rusqlite::Connection;
use rusqlite::OpenFlags;

pub use write::{
    CatalogRow, LanguageRow, ProofMark, ProofRow, ProofRun, ScanMark, StaleRepo, SyncCounts,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Migrations, in order. A database records how many it has applied in
/// `PRAGMA user_version`; each one runs once, in a transaction.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_finding_counts.sql"),
    include_str!("../migrations/0003_proof_source.sql"),
    include_str!("../migrations/0004_publishing.sql"),
    include_str!("../migrations/0005_consolidate_action.sql"),
    include_str!("../migrations/0006_source_ignore.sql"),
    include_str!("../migrations/0007_official_link.sql"),
    include_str!("../migrations/0008_my_language.sql"),
];

pub struct Store {
    path: PathBuf,
    writer: Mutex<Connection>,
}

impl Store {
    /// Opens (creating if needed) and migrates the database at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate(&mut conn)?;
        Ok(Self {
            path: path.to_owned(),
            writer: Mutex::new(conn),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn writer(&self) -> MutexGuard<'_, Connection> {
        self.writer.lock().expect("store writer")
    }

    /// A fresh read-only connection. Cheap (tens of microseconds); one per
    /// request keeps readers independent of the writer.
    pub fn reader(&self) -> Result<Connection> {
        let conn = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(conn)
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let applied: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(applied as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
        tracing::info!(migration = i + 1, "applied");
    }
    Ok(())
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
