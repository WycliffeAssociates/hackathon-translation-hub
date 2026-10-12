//! What Forgejo's own database says about each repo: its id, whether it is
//! public, when it last changed, its topics. The hub only indexes public repos,
//! and the disk alone cannot say which those are.
//!
//! Two readers, one row shape:
//! - [`from_dump`]: a `mysqldump` of the `repository` table (`dev-data/sql/
//!   repository.sql.gz`), for development.
//! - `from_mariadb` (feature `mariadb`): the live database, read-only, in
//!   production.

use std::io::Read;
use std::path::Path;

/// One row of Forgejo's `repository` table, as much of it as the hub uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgejoRepo {
    /// `repository.id`; the hub keys its own `repo` table on it.
    pub id: i64,
    /// Lowercase, as the repo sits on disk (`<owner>/<name>.git`).
    pub owner: String,
    pub name: String,
    pub default_branch: Option<String>,
    pub is_private: bool,
    pub is_empty: bool,
    pub is_mirror: bool,
    pub is_archived: bool,
    /// Unix seconds. Forgejo bumps it on push, which is what the soft poll
    /// watches.
    pub updated_unix: i64,
    /// Repo topics, e.g. `consolidated`.
    pub topics: Vec<String>,
}

impl ForgejoRepo {
    /// The only repos the hub may show.
    pub fn is_public(&self) -> bool {
        !self.is_private
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DumpError {
    #[error("reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("no CREATE TABLE `repository` in the dump")]
    NoSchema,
    #[error("column `{0}` missing from the dump's schema")]
    NoColumn(&'static str),
    #[error("malformed row near byte {0}")]
    Malformed(usize),
}

/// Reads a (gzipped or plain) `mysqldump` of the `repository` table. Column
/// positions come from the dump's own `CREATE TABLE`, so a Forgejo upgrade
/// that adds columns does not shift them.
pub fn from_dump(path: &Path) -> Result<Vec<ForgejoRepo>, DumpError> {
    let io = |source| DumpError::Io {
        path: path.display().to_string(),
        source,
    };
    let file = std::fs::File::open(path).map_err(io)?;
    let mut text = String::new();
    if path.extension().is_some_and(|e| e == "gz") {
        flate2::read::GzDecoder::new(file)
            .read_to_string(&mut text)
            .map_err(io)?;
    } else {
        std::io::BufReader::new(file)
            .read_to_string(&mut text)
            .map_err(io)?;
    }
    parse_dump(&text)
}

fn parse_dump(text: &str) -> Result<Vec<ForgejoRepo>, DumpError> {
    let columns = schema_columns(text).ok_or(DumpError::NoSchema)?;
    let at = |name: &'static str| {
        columns
            .iter()
            .position(|c| c == name)
            .ok_or(DumpError::NoColumn(name))
    };
    let (id, lower_name, default_branch, is_private, is_empty, is_mirror, is_archived) = (
        at("id")?,
        at("lower_name")?,
        at("default_branch")?,
        at("is_private")?,
        at("is_empty")?,
        at("is_mirror")?,
        at("is_archived")?,
    );
    let (updated_unix, topics, owner_name) =
        (at("updated_unix")?, at("topics")?, at("owner_name")?);

    let mut out = Vec::new();
    let marker = "INSERT INTO `repository` VALUES";
    let mut rest = text;
    while let Some(start) = rest.find(marker) {
        let base = text.len() - rest.len() + start + marker.len();
        let (rows, consumed) =
            parse_values(&text[base..]).map_err(|at| DumpError::Malformed(base + at))?;
        for row in rows {
            let int = |i: usize| {
                row.get(i)
                    .and_then(|v| v.as_deref())
                    .and_then(|s| s.parse::<i64>().ok())
            };
            let string = |i: usize| row.get(i).cloned().flatten();
            let flag = |i: usize| int(i).unwrap_or(0) != 0;
            let (Some(id), Some(owner), Some(name)) =
                (int(id), string(owner_name), string(lower_name))
            else {
                continue;
            };
            out.push(ForgejoRepo {
                id,
                owner: owner.to_lowercase(),
                name,
                default_branch: string(default_branch).filter(|b| !b.is_empty()),
                is_private: flag(is_private),
                is_empty: flag(is_empty),
                is_mirror: flag(is_mirror),
                is_archived: flag(is_archived),
                updated_unix: int(updated_unix).unwrap_or(0),
                topics: string(topics)
                    .and_then(|t| serde_json::from_str(&t).ok())
                    .unwrap_or_default(),
            });
        }
        rest = &text[base + consumed..];
    }
    Ok(out)
}

/// The backticked column names of `CREATE TABLE \`repository\``, in order.
fn schema_columns(text: &str) -> Option<Vec<String>> {
    let start = text.find("CREATE TABLE `repository` (")?;
    let body = &text[start..];
    let end = body.find("\n)")?;
    Some(
        body[..end]
            .lines()
            .skip(1)
            .filter_map(|line| {
                let line = line.trim_start();
                let name = line.strip_prefix('`')?;
                Some(name[..name.find('`')?].to_owned())
            })
            .collect(),
    )
}

type Row = Vec<Option<String>>;

/// `(1,'a',NULL),(2,'b\'c',3);` → rows, and the bytes consumed through the
/// `;`. MySQL string escapes are decoded; numbers stay text.
fn parse_values(s: &str) -> Result<(Vec<Row>, usize), usize> {
    let b = s.as_bytes();
    let mut i = 0;
    let mut rows = Vec::new();
    let skip_ws = |i: &mut usize| {
        while *i < b.len() && b[*i].is_ascii_whitespace() {
            *i += 1;
        }
    };
    loop {
        skip_ws(&mut i);
        if b.get(i) != Some(&b'(') {
            return Err(i);
        }
        i += 1;
        let mut row = Vec::new();
        loop {
            skip_ws(&mut i);
            match b.get(i) {
                Some(b'\'') => {
                    i += 1;
                    let mut v = Vec::new();
                    loop {
                        match b.get(i) {
                            Some(b'\\') => {
                                v.push(match b.get(i + 1).ok_or(i)? {
                                    b'n' => b'\n',
                                    b'r' => b'\r',
                                    b't' => b'\t',
                                    b'0' => 0,
                                    other => *other,
                                });
                                i += 2;
                            }
                            Some(b'\'') if b.get(i + 1) == Some(&b'\'') => {
                                v.push(b'\'');
                                i += 2;
                            }
                            Some(b'\'') => {
                                i += 1;
                                break;
                            }
                            Some(&c) => {
                                v.push(c);
                                i += 1;
                            }
                            None => return Err(i),
                        }
                    }
                    row.push(Some(String::from_utf8_lossy(&v).into_owned()));
                }
                Some(_) => {
                    let start = i;
                    while i < b.len() && b[i] != b',' && b[i] != b')' {
                        i += 1;
                    }
                    let raw = s[start..i].trim();
                    row.push((raw != "NULL").then(|| raw.to_owned()));
                }
                None => return Err(i),
            }
            skip_ws(&mut i);
            match b.get(i) {
                Some(b',') => i += 1,
                Some(b')') => {
                    i += 1;
                    break;
                }
                _ => return Err(i),
            }
        }
        rows.push(row);
        skip_ws(&mut i);
        match b.get(i) {
            Some(b',') => i += 1,
            Some(b';') => return Ok((rows, i + 1)),
            _ => return Err(i),
        }
    }
}

/// Every repo in Forgejo's live database. Read-only: one SELECT on
/// `repository` alone, which is all the `dashboard_ro` account may read
/// besides topics and releases (HANDOFF.md). `owner_name` is denormalized on
/// the row, so no join on `user` is needed.
#[cfg(feature = "mariadb")]
pub fn from_mariadb(url: &str) -> Result<Vec<ForgejoRepo>, mysql::Error> {
    use mysql::prelude::Queryable;
    let pool = mysql::Pool::new(url)?;
    let mut conn = pool.get_conn()?;
    let rows: Vec<mysql::Row> = conn.query(
        "SELECT id, LOWER(owner_name), lower_name, default_branch, is_private, is_empty,
                is_mirror, is_archived, updated_unix, topics
         FROM repository",
    )?;
    Ok(rows
        .into_iter()
        .filter_map(|mut r| {
            let flag = |v: Option<Option<i64>>| v.flatten().unwrap_or(0) != 0;
            Some(ForgejoRepo {
                id: r.take(0)?,
                owner: r.take::<Option<String>, _>(1).flatten()?,
                name: r.take(2)?,
                default_branch: r
                    .take::<Option<String>, _>(3)
                    .flatten()
                    .filter(|b| !b.is_empty()),
                is_private: flag(r.take(4)),
                is_empty: flag(r.take(5)),
                is_mirror: flag(r.take(6)),
                is_archived: flag(r.take(7)),
                updated_unix: r.take::<Option<i64>, _>(8).flatten().unwrap_or(0),
                topics: r
                    .take::<Option<String>, _>(9)
                    .flatten()
                    .and_then(|t| serde_json::from_str(&t).ok())
                    .unwrap_or_default(),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Why: the dump parser is hand-written, and a quote or backslash it gets
    // wrong silently shifts every later column (a private repo read as
    // public). This pins the escapes and the column lookup together.
    #[test]
    fn reads_escapes_nulls_and_columns_by_name() {
        let dump = "CREATE TABLE `repository` (\n  `id` bigint,\n  `lower_name` varchar,\n  `default_branch` varchar,\n  `is_private` tinyint,\n  `is_empty` tinyint,\n  `is_mirror` tinyint,\n  `is_archived` tinyint,\n  `updated_unix` bigint,\n  `topics` text,\n  `owner_name` varchar,\n  PRIMARY KEY (`id`)\n) ENGINE=InnoDB;\nINSERT INTO `repository` VALUES (1,'a\\'b','master',0,0,0,NULL,5,'[\"consolidated\"]','Alice'),(2,'c,d)','',1,1,0,0,NULL,NULL,'bob');\n";
        let rows = parse_dump(dump).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "a'b");
        assert_eq!(rows[0].owner, "alice");
        assert_eq!(rows[0].topics, ["consolidated"]);
        assert!(rows[0].is_public() && !rows[0].is_archived);
        assert_eq!(rows[1].name, "c,d)");
        assert_eq!(rows[1].default_branch, None);
        assert!(!rows[1].is_public());
        assert_eq!(rows[1].updated_unix, 0);
    }
}
