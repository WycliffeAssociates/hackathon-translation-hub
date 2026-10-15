//! `/v1/languages/{ietf}/sources?resource=`: every copy of every book of a
//! language and brand, grouped into versions by content, so a publisher
//! decides by book (proposal, 2026-10-15).
//!
//! tStudio and Writer cannot be retired, so the same book lives in many repos:
//! several Writer projects, a merged repo that repeats them, a catalog repo,
//! the official one. Each copy's verses are fingerprinted (`hub_domain::
//! fingerprint`); copies with the same words in every verse are one version.
//! Each version is compared with the book's pick, and the official repo's
//! copy with the copy it was pulled from (the hub's pointer, written by pull
//! and consolidate; for a repo Sefer consolidated, its `.upstreams.json`),
//! which says whether the official text is current, behind the repo it
//! follows, or edited since.

use std::collections::{BTreeMap, HashMap};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use hub_domain::fingerprint::{Fingerprint, VerseDiff, fingerprint};
use hub_repo_read::RepoReader;
use hub_store::publishing::{self, BookCopy};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};

use crate::api::{AppState, Failure};

#[derive(OpenApi)]
#[openapi(
    paths(sources, set_ignored),
    components(schemas(
        Sources,
        SourceBook,
        Version,
        Copy,
        OfficialBook,
        Linked,
        Diff,
        IgnoreBody
    ))
)]
pub struct SourcesDoc;

pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/v1/languages/{ietf}/sources", axum::routing::get(sources))
        .route(
            "/v1/languages/{ietf}/sources/ignore",
            axum::routing::post(set_ignored),
        )
}

fn fail(status: StatusCode, msg: impl Into<String>) -> Failure {
    Failure(status, msg.into())
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SourcesQuery {
    /// The brand (resource code): `reg`, `ulb`.
    resource: String,
    /// More sources to measure progress against, `owner/name,owner/name`
    /// (a reader's pins), after the ones the copies declare (the three most
    /// declared).
    against: Option<String>,
}

/// One book's verses against a source: how many of the source's verses the
/// book has (by number), of how many.
#[derive(Serialize, ToSchema)]
pub struct BookCoverage {
    pub book: String,
    pub present: usize,
    pub total: usize,
}

/// The language and brand's text (the official copy, else the pick) against
/// one source, book by book in the source's order.
#[derive(Serialize, ToSchema)]
pub struct SourceProgress {
    /// `owner/name`.
    pub source: String,
    pub books: Vec<BookCoverage>,
}

/// How one copy differs from another, in verses.
#[derive(Serialize, ToSchema, Clone, Copy)]
pub struct Diff {
    /// Verses in both, with different words.
    pub changed: usize,
    /// Verses this one has and the other lacks.
    pub added: usize,
    /// Verses the other has and this one lacks.
    pub missing: usize,
}

impl From<VerseDiff> for Diff {
    fn from(d: VerseDiff) -> Self {
        Self {
            changed: d.changed,
            added: d.added,
            missing: d.missing,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct Copy {
    #[serde(flatten)]
    pub copy: BookCopy,
    /// Set aside by a publisher, for this book or for every book.
    pub ignored: bool,
    /// The copy in the official repo.
    pub official: bool,
}

/// Copies with the same words in every verse.
#[derive(Serialize, ToSchema)]
pub struct Version {
    /// Verses with text.
    pub verses: usize,
    pub chapters: i64,
    /// Best first: Primary and on BIEL, then Primary, Active, unlisted; then
    /// the most verses, the newest.
    pub members: Vec<Copy>,
    /// Against the book's pick; absent for the pick itself.
    pub vs_pick: Option<Diff>,
    /// Every member is set aside.
    pub ignored: bool,
}

#[derive(Serialize, ToSchema)]
pub struct Linked {
    /// The repo the official book was pulled from, `owner/name`.
    pub repo: String,
    pub commit: String,
    pub taken_at: String,
    /// Its version now, when it still has the book.
    pub version: Option<usize>,
    /// It has commits since the pull.
    pub moved: bool,
}

#[derive(Serialize, ToSchema)]
pub struct OfficialBook {
    /// The official copy's version.
    pub version: usize,
    pub linked: Option<Linked>,
    /// `current` (same words as the repo it follows), `behind` (that repo
    /// changed since), `edited` (the official copy changed, the repo did not),
    /// `unlinked` (no record of where it came from).
    pub state: String,
    /// The followed repo's copy against the official one, when they differ.
    pub vs_linked: Option<Diff>,
}

#[derive(Serialize, ToSchema)]
pub struct SourceBook {
    pub book: String,
    /// Verses in the gateway source, when one resolves.
    pub source_verses: Option<i64>,
    pub versions: Vec<Version>,
    /// The version the hub would take: the one the official copy follows,
    /// else the best version not set aside.
    pub pick: Option<usize>,
    pub official: Option<OfficialBook>,
}

#[derive(Serialize, ToSchema)]
pub struct Sources {
    pub language: String,
    pub resource: String,
    /// `TextTranslation/{ietf}_{resource}`, when publishing is configured.
    pub target: Option<String>,
    /// The official repo has a copy of at least one book.
    pub official_exists: bool,
    /// Copies read, across every book.
    pub copies: usize,
    pub books: Vec<SourceBook>,
    /// Against each source asked for (or declared).
    pub progress: Vec<SourceProgress>,
}

/// Lower rank is better: what BIEL shows today first (as `default_pick`).
fn rank(c: &BookCopy) -> (u8, std::cmp::Reverse<i64>, std::cmp::Reverse<i64>) {
    let status = match (c.status.as_deref(), c.show_on_biel) {
        (Some("Primary"), true) => 0,
        (Some("Primary"), false) => 1,
        (Some("Active"), _) => 2,
        (None, _) => 3,
        _ => 4,
    };
    (
        status,
        std::cmp::Reverse(c.verses),
        std::cmp::Reverse(c.head_unix),
    )
}

/// The official repo's `.upstreams.json`: book → (owner/name, commit, takenAt).
fn upstreams_of(text: &str) -> HashMap<String, (String, String, String)> {
    let mut out = HashMap::new();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return out;
    };
    for u in v["upstreams"].as_array().into_iter().flatten() {
        let url = u["url"]
            .as_str()
            .unwrap_or_default()
            .trim_end_matches(".git");
        let mut tail = url.rsplit('/');
        let (Some(name), Some(owner)) = (tail.next(), tail.next()) else {
            continue;
        };
        for b in u["books"].as_array().into_iter().flatten() {
            if let Some(b) = b.as_str() {
                out.insert(
                    b.to_uppercase(),
                    (
                        format!("{owner}/{name}").to_lowercase(),
                        u["commit"].as_str().unwrap_or_default().to_owned(),
                        u["takenAt"].as_str().unwrap_or_default().to_owned(),
                    ),
                );
            }
        }
    }
    out
}

/// Every copy of every book of a language and brand, grouped into versions by
/// their words, with the pick and the official copy's state.
#[utoipa::path(get, path = "/v1/languages/{ietf}/sources",
    params(("ietf" = String, Path), SourcesQuery),
    responses((status = 200, body = Sources)))]
async fn sources(
    State(s): State<AppState>,
    Path(ietf): Path<String>,
    Query(q): Query<SourcesQuery>,
) -> Result<Json<Sources>, Failure> {
    let (lang, res) = (ietf.to_lowercase(), q.resource.to_lowercase());
    let against = q.against.clone();
    let org = s
        .publish
        .as_ref()
        .map(|p| p.config.official_org.to_lowercase());
    let target = s
        .publish
        .as_ref()
        .map(|p| format!("{}/{lang}_{res}", p.config.official_org));
    let started = std::time::Instant::now();
    let state = s.clone();
    let out = tokio::task::spawn_blocking(move || -> Result<Sources, Failure> {
        use rayon::prelude::*;
        let internal = |e: String| fail(StatusCode::INTERNAL_SERVER_ERROR, e);
        let conn = state.store.reader().map_err(|e| internal(e.to_string()))?;
        let copies =
            publishing::book_copies(&conn, &lang, &res).map_err(|e| internal(e.to_string()))?;
        let ignored =
            publishing::ignored(&conn, &lang, &res).map_err(|e| internal(e.to_string()))?;
        let is_ignored = |c: &BookCopy| {
            let repo = format!("{}/{}", c.owner, c.name).to_lowercase();
            ignored
                .iter()
                .any(|(b, r)| *r == repo && (b == "*" || *b == c.book))
        };
        let is_official = |c: &BookCopy| org.as_deref() == Some(c.owner.to_lowercase().as_str());

        // Each copy's words; a copy that cannot be read is left out.
        let prints: Vec<Option<Fingerprint>> = copies
            .par_iter()
            .map(|c| {
                crate::compare::book_text(
                    &state,
                    &c.owner.to_lowercase(),
                    &c.name.to_lowercase(),
                    &c.head_sha,
                    c.writer,
                    &c.path,
                )
                .ok()
                .map(|t| fingerprint(&t))
            })
            .collect();

        // What the official copies follow: the hub's pointers, else (a repo
        // Sefer consolidated) its `.upstreams.json`.
        let official_repo = copies.iter().find(|c| is_official(c));
        let mut upstreams: HashMap<String, (String, String, String)> = official_repo
            .and_then(|c| {
                let reader = RepoReader::open(
                    &state
                        .scanner
                        .repo_path(&c.owner.to_lowercase(), &c.name.to_lowercase()),
                )
                .ok()?;
                let bytes = reader.file(&c.head_sha, ".upstreams.json").ok()??;
                Some(upstreams_of(&String::from_utf8_lossy(&bytes)))
            })
            .unwrap_or_default();
        if let Some(c) = official_repo {
            let links = publishing::official_links(&conn, &format!("{}/{}", c.owner, c.name))
                .map_err(|e| internal(e.to_string()))?;
            for (book, (repo, commit, at)) in links {
                upstreams.insert(book, (repo, commit, hub_synthetic::date_of(at)));
            }
        }

        let mut books = Vec::new();
        // Per book, the words that stand for the language: the official copy, else the pick.
        let mut standing: Vec<(String, Fingerprint)> = Vec::new();
        let mut start = 0;
        while start < copies.len() {
            let book = copies[start].book.clone();
            let end = start
                + copies[start..]
                    .iter()
                    .take_while(|c| c.book == book)
                    .count();
            // Versions by fingerprint, in first-seen order.
            let mut by_id: BTreeMap<u64, usize> = BTreeMap::new();
            let mut groups: Vec<(Fingerprint, Vec<usize>)> = Vec::new();
            for (i, fp) in prints.iter().enumerate().take(end).skip(start) {
                let Some(fp) = fp else { continue };
                let id = fp.id();
                let g = *by_id.entry(id).or_insert_with(|| {
                    groups.push((fp.clone(), Vec::new()));
                    groups.len() - 1
                });
                groups[g].1.push(i);
            }
            for (_, members) in &mut groups {
                members.sort_by_key(|&i| rank(&copies[i]));
            }
            // Best version first: by its best member, then the most verses.
            groups.sort_by_key(|(fp, m)| (rank(&copies[m[0]]), std::cmp::Reverse(fp.verses.len())));
            let version_of = |repo: &str| {
                groups.iter().position(|(_, m)| {
                    m.iter().any(|&i| {
                        format!("{}/{}", copies[i].owner, copies[i].name).eq_ignore_ascii_case(repo)
                    })
                })
            };
            let all_ignored = |m: &[usize]| {
                m.iter()
                    .all(|&i| is_ignored(&copies[i]) || is_official(&copies[i]))
            };

            // The official copy, and what it follows.
            let official = groups
                .iter()
                .position(|(_, m)| m.iter().any(|&i| is_official(&copies[i])))
                .map(|version| {
                    let linked = upstreams.get(&book).map(|(repo, commit, taken_at)| {
                        let at = (start..end).find(|&i| {
                            format!("{}/{}", copies[i].owner, copies[i].name)
                                .eq_ignore_ascii_case(repo)
                        });
                        Linked {
                            repo: repo.clone(),
                            commit: commit.clone(),
                            taken_at: taken_at.clone(),
                            version: version_of(repo),
                            moved: at
                                .is_some_and(|i| !copies[i].head_sha.eq_ignore_ascii_case(commit)),
                        }
                    });
                    let (state, vs_linked) = match &linked {
                        None => ("unlinked", None),
                        Some(l) if l.version == Some(version) => ("current", None),
                        Some(l) => {
                            let diff = l
                                .version
                                .map(|v| groups[v].0.diff(&groups[version].0).into());
                            (if l.moved { "behind" } else { "edited" }, diff)
                        }
                    };
                    OfficialBook {
                        version,
                        linked,
                        state: state.to_owned(),
                        vs_linked,
                    }
                });

            let pick = official
                .as_ref()
                .and_then(|o| o.linked.as_ref().and_then(|l| l.version))
                .or_else(|| groups.iter().position(|(_, m)| !all_ignored(m)));
            let versions = groups
                .iter()
                .enumerate()
                .map(|(v, (fp, members))| Version {
                    verses: fp.verses.len(),
                    chapters: copies[members[0]].chapters,
                    vs_pick: pick
                        .filter(|&p| p != v)
                        .map(|p| fp.diff(&groups[p].0).into()),
                    ignored: all_ignored(members),
                    members: members
                        .iter()
                        .map(|&i| Copy {
                            copy: copies[i].clone(),
                            ignored: is_ignored(&copies[i]),
                            official: is_official(&copies[i]),
                        })
                        .collect(),
                })
                .collect();
            if let Some(v) = official.as_ref().map(|o| o.version).or(pick) {
                standing.push((book.clone(), groups[v].0.clone()));
            }
            books.push(SourceBook {
                source_verses: copies[start..end].iter().find_map(|c| c.source_verses),
                book,
                versions,
                pick,
                official,
            });
            start = end;
        }
        // Progress against each source: the sources asked for, else those declared.
        let against: Vec<String> = {
            let mut counts: HashMap<String, usize> = HashMap::new();
            for c in &copies {
                if let Some(src) = &c.source_repo {
                    *counts.entry(src.to_lowercase()).or_default() += 1;
                }
            }
            let mut v: Vec<(String, usize)> = counts.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            let mut list: Vec<String> = v.into_iter().take(3).map(|(s, _)| s).collect();
            for extra in against.iter().flat_map(|a| a.split(',')) {
                let extra = extra.trim().to_lowercase();
                if extra.contains('/') && !list.contains(&extra) {
                    list.push(extra);
                }
            }
            list
        };
        let progress = against
            .iter()
            .filter_map(|src| {
                let (o, n) = src.split_once('/')?;
                let detail = hub_store::read::repo(&conn, o, n).ok()??;
                let sha = detail.head_sha.clone()?;
                let synthetic = detail.synthetic.is_some();
                let mut source_books: Vec<(String, Fingerprint)> = detail
                    .books
                    .par_iter()
                    .filter(|b| !b.book.is_empty())
                    .filter_map(|b| {
                        let text =
                            crate::compare::book_text(&state, o, n, &sha, synthetic, &b.path)
                                .ok()?;
                        Some((b.book.clone(), fingerprint(&text)))
                    })
                    .collect();
                source_books
                    .sort_by_key(|(b, _)| hub_stitch::usfm_file_name(b).unwrap_or_default());
                let books = source_books
                    .iter()
                    .map(|(book, fp)| {
                        let present =
                            standing
                                .iter()
                                .find(|(b, _)| b == book)
                                .map_or(0, |(_, ours)| {
                                    fp.verses
                                        .keys()
                                        .filter(|k| ours.verses.contains_key(k))
                                        .count()
                                });
                        BookCoverage {
                            book: book.clone(),
                            present,
                            total: fp.verses.len(),
                        }
                    })
                    .collect();
                Some(SourceProgress {
                    source: src.clone(),
                    books,
                })
            })
            .collect();
        // Canonical order, not alphabetical.
        books.sort_by_key(|b| hub_stitch::usfm_file_name(&b.book).unwrap_or_default());
        let official_exists = books.iter().any(|b| b.official.is_some());
        Ok(Sources {
            target,
            language: lang,
            resource: res,
            official_exists,
            copies: copies.len(),
            books,
            progress,
        })
    })
    .await
    .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
    let span = tracing::Span::current();
    span.record("rows", out.copies);
    span.record("work_ms", started.elapsed().as_secs_f64() * 1000.0);
    Ok(Json(out))
}

#[derive(Deserialize, ToSchema)]
pub struct IgnoreBody {
    pub resource: String,
    /// A USFM id, or `*` for every book the repo holds.
    pub book: String,
    /// `owner/name`.
    pub repo: String,
    /// False brings it back.
    pub ignore: bool,
}

/// Set a copy aside for one book or every book, or bring it back. Publishers only.
#[utoipa::path(post, path = "/v1/languages/{ietf}/sources/ignore",
    params(("ietf" = String, Path)), request_body = IgnoreBody,
    responses((status = 200, body = bool, content_type = "application/json", description = "Whether the copy is now set aside"), (status = 401, body = crate::api::ApiError), (status = 403, body = crate::api::ApiError)))]
async fn set_ignored(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(ietf): Path<String>,
    Json(body): Json<IgnoreBody>,
) -> Result<Json<bool>, Failure> {
    let me = crate::publish::require_publisher(&s, &headers).await?;
    let store = s.store.clone();
    let ignore = body.ignore;
    tokio::task::spawn_blocking(move || {
        store.set_ignored(
            &ietf,
            &body.resource,
            &body.book,
            &body.repo,
            &me.username,
            body.ignore,
        )
    })
    .await
    .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(ignore))
}

#[cfg(test)]
mod tests {
    // Why: a repo Sefer consolidated says what it follows only in Sefer's
    // file; reading it wrong would mark every book "unlinked".
    #[test]
    fn upstreams_are_read_per_book() {
        let u = super::upstreams_of(
            r#"{"v":1,"upstreams":[{"books":["TIT","PHM"],"via":"git","url":"http://127.0.0.1:3400/ana/tzh_tit_text_reg.git","commit":"abc","takenAt":"2026-10-15"}]}"#,
        );
        assert_eq!(u["PHM"].0, "ana/tzh_tit_text_reg");
        assert_eq!(u["TIT"].1, "abc");
    }
}
