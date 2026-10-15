//! A repo's history, read straight from its bare git repository (read-only):
//!
//! - `/v1/repos/{owner}/{name}/commits`: the newest commits, with the hub's
//!   trailers pulled out (`Source:`, `Pulled-by:`, Writer's `Writer-Source:`),
//!   for the History tab;
//! - `/v1/repos/{owner}/{name}/unreleased`: what changed since the newest
//!   release tag, book by book in chapters and verses (verse fingerprints, as
//!   the sources page uses), for the Releases tab's "since v1.2".

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use hub_domain::fingerprint::{Fingerprint, fingerprint};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};

use crate::api::{AppState, Failure};
use crate::sources::Diff;

#[derive(OpenApi)]
#[openapi(
    paths(commits, unreleased),
    components(schemas(CommitRow, Unreleased, BookChange, Size))
)]
pub struct ChangesDoc;

pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route(
            "/v1/repos/{owner}/{name}/commits",
            axum::routing::get(commits),
        )
        .route(
            "/v1/repos/{owner}/{name}/unreleased",
            axum::routing::get(unreleased),
        )
}

fn fail(status: StatusCode, msg: impl Into<String>) -> Failure {
    Failure(status, msg.into())
}

#[derive(Serialize, ToSchema)]
pub struct CommitRow {
    pub sha: String,
    pub summary: String,
    pub author: String,
    pub unix: i64,
    /// Where the change came from: `Source:` (a pull) or `Writer-Source:` (a replayed save).
    pub source: Option<String>,
    /// Who asked the hub for it: `Pulled-by:` or `Consolidated-by:`.
    pub by: Option<String>,
    /// Release tags on this commit.
    pub tags: Vec<String>,
}

#[derive(Serialize, ToSchema, Clone, Copy)]
pub struct Size {
    pub chapters: usize,
    pub verses: usize,
}

#[derive(Serialize, ToSchema)]
pub struct BookChange {
    pub path: String,
    /// Absent: the book is new since the release.
    pub before: Option<Size>,
    /// Absent: the book was removed.
    pub after: Option<Size>,
    /// The book now against the book at the release.
    pub diff: Diff,
}

#[derive(Serialize, ToSchema)]
pub struct Unreleased {
    /// The newest release tag behind HEAD; absent before the first release.
    pub last_tag: Option<String>,
    pub last_unix: Option<i64>,
    /// Commits since it, newest first.
    pub commits: Vec<CommitRow>,
    /// USFM files whose verses changed since it.
    pub books: Vec<BookChange>,
    /// Other files changed (manifest, bulletins).
    pub other: Vec<String>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CommitsQuery {
    /// At most this many (default 100, at most 500).
    limit: Option<usize>,
}

fn trailer<'a>(message: &'a str, keys: &[&str]) -> Option<&'a str> {
    message.lines().find_map(|l| {
        keys.iter()
            .find_map(|k| l.strip_prefix(k).and_then(|v| v.strip_prefix(':')))
            .map(str::trim)
    })
}

fn open(s: &AppState, owner: &str, name: &str) -> Result<git2::Repository, Failure> {
    git2::Repository::open_bare(
        s.scanner
            .repo_path(&owner.to_lowercase(), &name.to_lowercase()),
    )
    .map_err(|e| {
        fail(
            StatusCode::NOT_FOUND,
            format!("{owner}/{name}: {}", e.message()),
        )
    })
}

/// Release tags (`v…`) by the commit they point at.
fn tags_by_commit(repo: &git2::Repository) -> std::collections::HashMap<git2::Oid, Vec<String>> {
    let mut out: std::collections::HashMap<git2::Oid, Vec<String>> = Default::default();
    if let Ok(names) = repo.tag_names(Some("v*")) {
        for name in names.iter().flatten() {
            if let Ok(obj) = repo.revparse_single(&format!("refs/tags/{name}"))
                && let Ok(c) = obj.peel_to_commit()
            {
                out.entry(c.id()).or_default().push(name.to_owned());
            }
        }
    }
    out
}

fn row(c: &git2::Commit, tags: &std::collections::HashMap<git2::Oid, Vec<String>>) -> CommitRow {
    let message = c.message().unwrap_or_default();
    CommitRow {
        sha: c.id().to_string(),
        summary: c.summary().unwrap_or_default().to_owned(),
        author: c.author().name().unwrap_or_default().to_owned(),
        unix: c.time().seconds(),
        source: trailer(message, &["Source", "Writer-Source"]).map(str::to_owned),
        by: trailer(message, &["Pulled-by", "Consolidated-by"]).map(str::to_owned),
        tags: tags.get(&c.id()).cloned().unwrap_or_default(),
    }
}

/// The newest commits on the default branch.
#[utoipa::path(get, path = "/v1/repos/{owner}/{name}/commits",
    params(("owner" = String, Path), ("name" = String, Path), CommitsQuery),
    responses((status = 200, body = Vec<CommitRow>), (status = 404, body = crate::api::ApiError)))]
async fn commits(
    State(s): State<AppState>,
    Path((owner, name)): Path<(String, String)>,
    Query(q): Query<CommitsQuery>,
) -> Result<Json<Vec<CommitRow>>, Failure> {
    let limit = q.limit.unwrap_or(100).min(500);
    tokio::task::spawn_blocking(move || -> Result<Vec<CommitRow>, Failure> {
        let repo = open(&s, &owner, &name)?;
        let tags = tags_by_commit(&repo);
        let internal =
            |e: git2::Error| fail(StatusCode::INTERNAL_SERVER_ERROR, e.message().to_owned());
        let mut walk = repo.revwalk().map_err(internal)?;
        if walk.push_head().is_err() {
            return Ok(Vec::new()); // empty repo
        }
        walk.set_sorting(git2::Sort::TIME).map_err(internal)?;
        Ok(walk
            .filter_map(Result::ok)
            .filter_map(|oid| repo.find_commit(oid).ok())
            .take(limit)
            .map(|c| row(&c, &tags))
            .collect())
    })
    .await
    .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
}

fn size(fp: &Fingerprint) -> Size {
    let mut chapters: Vec<u16> = fp.verses.keys().map(|(c, _)| *c).collect();
    chapters.dedup();
    Size {
        chapters: chapters.len(),
        verses: fp.verses.len(),
    }
}

/// What changed since the newest release tag: commits, and each book in verses.
#[utoipa::path(get, path = "/v1/repos/{owner}/{name}/unreleased",
    params(("owner" = String, Path), ("name" = String, Path)),
    responses((status = 200, body = Unreleased), (status = 404, body = crate::api::ApiError)))]
async fn unreleased(
    State(s): State<AppState>,
    Path((owner, name)): Path<(String, String)>,
) -> Result<Json<Unreleased>, Failure> {
    let started = std::time::Instant::now();
    let out = tokio::task::spawn_blocking(move || -> Result<Unreleased, Failure> {
        let internal =
            |e: git2::Error| fail(StatusCode::INTERNAL_SERVER_ERROR, e.message().to_owned());
        let repo = open(&s, &owner, &name)?;
        let Ok(head) = repo.head().and_then(|h| h.peel_to_commit()) else {
            return Ok(Unreleased {
                last_tag: None,
                last_unix: None,
                commits: Vec::new(),
                books: Vec::new(),
                other: Vec::new(),
            });
        };
        let tags = tags_by_commit(&repo);
        // The newest tag whose commit is HEAD or behind it.
        let last = tags
            .iter()
            .filter_map(|(oid, names)| {
                let c = repo.find_commit(*oid).ok()?;
                let reachable =
                    *oid == head.id() || repo.graph_descendant_of(head.id(), *oid).unwrap_or(false);
                reachable.then(|| {
                    (
                        c.time().seconds(),
                        names.iter().max().cloned().unwrap_or_default(),
                        c,
                    )
                })
            })
            .max_by_key(|(t, name, _)| (*t, name.clone()));

        let mut walk = repo.revwalk().map_err(internal)?;
        walk.push(head.id()).map_err(internal)?;
        if let Some((_, _, c)) = &last {
            walk.hide(c.id()).map_err(internal)?;
        }
        walk.set_sorting(git2::Sort::TIME).map_err(internal)?;
        let commits: Vec<CommitRow> = walk
            .filter_map(Result::ok)
            .filter_map(|oid| repo.find_commit(oid).ok())
            .take(500)
            .map(|c| row(&c, &tags))
            .collect();

        let old_tree = last.as_ref().and_then(|(_, _, c)| c.tree().ok());
        let new_tree = head.tree().map_err(internal)?;
        let diff = repo
            .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)
            .map_err(internal)?;
        let text = |oid: git2::Oid| -> Option<String> {
            if oid.is_zero() {
                return None;
            }
            repo.find_blob(oid)
                .ok()
                .map(|b| String::from_utf8_lossy(b.content()).into_owned())
        };
        let mut books = Vec::new();
        let mut other = Vec::new();
        for d in diff.deltas() {
            let path = d
                .new_file()
                .path()
                .or_else(|| d.old_file().path())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !path.to_lowercase().ends_with(".usfm") {
                other.push(path);
                continue;
            }
            let before = text(d.old_file().id()).map(|t| fingerprint(&t));
            let after = text(d.new_file().id()).map(|t| fingerprint(&t));
            let empty = Fingerprint::default();
            let change = after
                .as_ref()
                .unwrap_or(&empty)
                .diff(before.as_ref().unwrap_or(&empty));
            // A file rewritten with the same words (re-stitched) is not a change to announce.
            if before.is_some() && after.is_some() && change.is_same() {
                continue;
            }
            books.push(BookChange {
                path,
                before: before.as_ref().map(size),
                after: after.as_ref().map(size),
                diff: change.into(),
            });
        }
        books.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Unreleased {
            last_tag: last.as_ref().map(|(_, n, _)| n.clone()),
            last_unix: last.as_ref().map(|(t, _, _)| *t),
            commits,
            books,
            other,
        })
    })
    .await
    .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
    let span = tracing::Span::current();
    span.record("rows", out.commits.len());
    span.record("work_ms", started.elapsed().as_secs_f64() * 1000.0);
    Ok(Json(out))
}
