//! `/v1/languages/{ietf}/sources?resource=`: every copy of every book of a
//! language and brand, grouped into variants by content, so a publisher
//! decides by book (proposal, 2026-10-15).
//!
//! tStudio and Writer cannot be retired, so the same book lives in many repos:
//! several Writer projects, a merged repo that repeats them, a catalog repo,
//! the official one. Each copy's verses are fingerprinted (`hub_domain::
//! fingerprint`); copies with the same words in every verse are one variant.
//! Each version is compared with the book's pick, and the official repo's
//! copy with the copy it was pulled from (the hub's pointer, written by pull
//! and consolidate; for a repo Sefer consolidated, its `.upstreams.json`),
//! which says whether the official text is current, behind the repo it
//! follows, or edited since.

use std::collections::{BTreeMap, HashMap};

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use hub_domain::fingerprint::{Fingerprint, VerseDiff, fingerprint, verse_words};
use hub_repo_read::RepoReader;
use hub_store::publishing::{self, BookCopy};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};

use crate::api::{AppState, Failure};

#[derive(OpenApi)]
#[openapi(
    paths(sources, set_ignored, diff, published, set_published, coverage),
    components(schemas(
        Sources,
        SourceBook,
        Variant,
        Copy,
        PublishedBook,
        PublishedRef,
        Linked,
        Diff,
        IgnoreBody,
        BookDiff,
        VerseChange,
        Published,
        PublishBody,
        Coverage,
        BookChapters,
        ChapterCoverage
    ))
)]
pub struct SourcesDoc;

pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/v1/languages/{ietf}/sources", axum::routing::get(sources))
        .route(
            "/v1/languages/{ietf}/sources/diff",
            axum::routing::get(diff),
        )
        .route(
            "/v1/languages/{ietf}/published",
            axum::routing::get(published).post(set_published),
        )
        .route(
            "/v1/languages/{ietf}/sources/ignore",
            axum::routing::post(set_ignored),
        )
        .route(
            "/v1/repos/{owner}/{name}/coverage",
            axum::routing::get(coverage),
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
}

/// A variant: copies with the same words in every verse. (A *version* is a
/// release, `v1.0`; a variant is just a distinct text.)
#[derive(Serialize, ToSchema)]
pub struct Variant {
    /// Verses with text.
    pub verses: usize,
    pub chapters: i64,
    /// Best first: Primary and on BIEL, then Primary, Active, unlisted; then
    /// the most verses, the newest. The published copy is not listed here:
    /// it is the book's `published`, what the others are compared with.
    pub members: Vec<Copy>,
    /// The published copy has these words.
    pub published: bool,
    /// Against the reference (the published copy, else the pick); absent for the reference itself.
    pub vs_reference: Option<Diff>,
    /// Every member is set aside.
    pub ignored: bool,
}

#[derive(Serialize, ToSchema)]
pub struct Linked {
    /// The repo the published book was pulled from (it tracks), `owner/name`.
    pub repo: String,
    pub commit: String,
    pub taken_at: String,
    /// Its variant now, when it still has the book.
    pub version: Option<usize>,
    /// It has commits since the pull.
    pub moved: bool,
}

/// The published copy of one book, and what it tracks.
#[derive(Serialize, ToSchema)]
pub struct PublishedBook {
    /// Its variant.
    pub version: usize,
    pub chapters: i64,
    pub verses: usize,
    /// The published repo's last commit.
    pub head_unix: i64,
    pub linked: Option<Linked>,
    /// `current` (same words as the repo it tracks), `behind` (that repo
    /// changed since), `edited` (the published copy changed, the repo did not),
    /// `unlinked` (no record of where it came from).
    pub state: String,
    /// The tracked repo's copy against the published one, when they differ.
    pub vs_linked: Option<Diff>,
}

#[derive(Serialize, ToSchema)]
pub struct SourceBook {
    pub book: String,
    /// Verses in the gateway source, when one resolves.
    pub source_verses: Option<i64>,
    pub variants: Vec<Variant>,
    /// The variant the hub would take: the one the published copy tracks,
    /// else the best variant not set aside.
    pub pick: Option<usize>,
    /// What the other variants are compared with: the published copy's
    /// variant, else the pick.
    pub reference: Option<usize>,
    pub published: Option<PublishedBook>,
}

/// What is published for this language and resource.
#[derive(Serialize, ToSchema)]
pub struct PublishedRef {
    /// `owner/name`.
    pub repo: String,
    /// A publisher chose it; false: the official org's `{ietf}_{resource}` by default.
    pub chosen: bool,
    /// In the official org, so the hub may pull into it and release from it.
    pub writable: bool,
}

#[derive(Serialize, ToSchema)]
pub struct Sources {
    pub language: String,
    pub resource: String,
    /// `TextTranslation/{ietf}_{resource}`, where the hub would create one, when publishing is configured.
    pub target: Option<String>,
    pub published: Option<PublishedRef>,
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
pub(crate) fn upstreams_of(text: &str) -> HashMap<String, (String, String, String)> {
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

/// Every copy of every book of a language and brand, grouped into variants by
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
        let published = match &org {
            Some(org) => publishing::published_of(&conn, &lang, &res, org)
                .map_err(|e| internal(e.to_string()))?,
            None => None,
        };
        let published_repo = published
            .as_ref()
            .map(|(r, _)| r.clone())
            .unwrap_or_default();
        let is_official =
            |c: &BookCopy| format!("{}/{}", c.owner, c.name).eq_ignore_ascii_case(&published_repo);

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

        // What the published copies track: the hub's pointers, else (a repo
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
                    let lead = groups[version]
                        .1
                        .iter()
                        .find(|&&i| is_official(&copies[i]))
                        .copied();
                    PublishedBook {
                        version,
                        chapters: lead.map_or(0, |i| copies[i].chapters),
                        verses: groups[version].0.verses.len(),
                        head_unix: lead.map_or(0, |i| copies[i].head_unix),
                        linked,
                        state: state.to_owned(),
                        vs_linked,
                    }
                });

            let pick = official
                .as_ref()
                .and_then(|o| o.linked.as_ref().and_then(|l| l.version))
                .or_else(|| groups.iter().position(|(_, m)| !all_ignored(m)));
            let reference = official.as_ref().map(|o| o.version).or(pick);
            let variants = groups
                .iter()
                .enumerate()
                .map(|(v, (fp, members))| Variant {
                    verses: fp.verses.len(),
                    chapters: copies[members[0]].chapters,
                    vs_reference: reference
                        .filter(|&r| r != v)
                        .map(|r| fp.diff(&groups[r].0).into()),
                    ignored: all_ignored(members),
                    published: members.iter().any(|&i| is_official(&copies[i])),
                    members: members
                        .iter()
                        .filter(|&&i| !is_official(&copies[i]))
                        .map(|&i| Copy {
                            copy: copies[i].clone(),
                            ignored: is_ignored(&copies[i]),
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
                variants,
                pick,
                reference,
                published: official,
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

        Ok(Sources {
            target,
            language: lang,
            resource: res,
            published: published.map(|(repo, chosen)| PublishedRef {
                writable: org
                    .as_deref()
                    .is_some_and(|o| repo.starts_with(&format!("{o}/"))),
                repo,
                chosen,
            }),
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

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DiffQuery {
    resource: String,
    /// USFM id: `MAT`.
    book: String,
    /// `owner/name`: the copy shown on the left (usually the published one).
    a: String,
    /// `owner/name`: the copy compared with it.
    b: String,
}

/// One verse that differs between two copies; a side is absent when that copy lacks the verse.
#[derive(Serialize, ToSchema)]
pub struct VerseChange {
    pub chapter: u16,
    pub verse: u16,
    pub a: Option<String>,
    pub b: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct BookDiff {
    pub book: String,
    pub a: String,
    pub b: String,
    /// Verses that differ, in order (at most 2,000).
    pub verses: Vec<VerseChange>,
    pub same: usize,
}

/// Two copies of a book, verse by verse: the verses whose words differ.
#[utoipa::path(get, path = "/v1/languages/{ietf}/sources/diff",
    params(("ietf" = String, Path), DiffQuery),
    responses((status = 200, body = BookDiff), (status = 404, body = crate::api::ApiError)))]
async fn diff(
    State(s): State<AppState>,
    Path(ietf): Path<String>,
    Query(q): Query<DiffQuery>,
) -> Result<Json<BookDiff>, Failure> {
    let state = s.clone();
    tokio::task::spawn_blocking(move || -> Result<BookDiff, Failure> {
        let internal = |e: String| fail(StatusCode::INTERNAL_SERVER_ERROR, e);
        let conn = state.store.reader().map_err(|e| internal(e.to_string()))?;
        let book = q.book.to_uppercase();
        let copies = publishing::book_copies(&conn, &ietf, &q.resource)
            .map_err(|e| internal(e.to_string()))?;
        let words = |repo: &str| -> Result<BTreeMap<(u16, u16), String>, Failure> {
            let c = copies
                .iter()
                .find(|c| {
                    c.book == book && format!("{}/{}", c.owner, c.name).eq_ignore_ascii_case(repo)
                })
                .ok_or_else(|| fail(StatusCode::NOT_FOUND, format!("no {book} in {repo}")))?;
            let text = crate::compare::book_text(
                &state,
                &c.owner.to_lowercase(),
                &c.name.to_lowercase(),
                &c.head_sha,
                c.writer,
                &c.path,
            )
            .map_err(internal)?;
            Ok(verse_words(&text))
        };
        let (a, b) = rayon::join(|| words(&q.a), || words(&q.b));
        let (a, b) = (a?, b?);
        let mut keys: Vec<&(u16, u16)> = a.keys().chain(b.keys()).collect();
        keys.sort();
        keys.dedup();
        let mut verses = Vec::new();
        let mut same = 0;
        for k in keys {
            let (x, y) = (a.get(k), b.get(k));
            if x == y {
                same += 1;
            } else if verses.len() < 2000 {
                verses.push(VerseChange {
                    chapter: k.0,
                    verse: k.1,
                    a: x.cloned(),
                    b: y.cloned(),
                });
            }
        }
        Ok(BookDiff {
            book,
            a: q.a.to_lowercase(),
            b: q.b.to_lowercase(),
            verses,
            same,
        })
    })
    .await
    .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
}

/// What is published for one resource of a language, or what could be.
#[derive(Serialize, ToSchema)]
pub struct Published {
    pub resource: String,
    /// Repos of this language and resource.
    pub repos: i64,
    /// Of them, Writer projects.
    pub writer: i64,
    pub published: Option<PublishedRef>,
    /// Its newest release tag, and when.
    pub last_tag: Option<String>,
    pub last_tag_unix: Option<i64>,
    pub books: Option<i64>,
    /// With nothing published: the repo to bless first (WA-Catalog's, else
    /// the one with the most books), when there is a consolidated one.
    pub suggest: Option<String>,
}

/// The newest `v…` tag of a repo on disk and its time.
fn last_tag(s: &AppState, repo: &str) -> Option<(String, i64)> {
    let (o, n) = repo.split_once('/')?;
    let r = git2::Repository::open_bare(s.scanner.repo_path(o, n)).ok()?;
    let names = r.tag_names(Some("v*")).ok()?;
    names
        .iter()
        .flatten()
        .filter_map(|t| {
            let c = r
                .revparse_single(&format!("refs/tags/{t}"))
                .ok()?
                .peel_to_commit()
                .ok()?;
            Some((t.to_owned(), c.time().seconds()))
        })
        .max_by_key(|(t, at)| (*at, t.clone()))
}

/// Per resource of a language: what is published (a publisher's choice, or
/// the official org's repo by default), its newest release, or a suggestion.
#[utoipa::path(get, path = "/v1/languages/{ietf}/published",
    params(("ietf" = String, Path)),
    responses((status = 200, body = Vec<Published>)))]
async fn published(
    State(s): State<AppState>,
    Path(ietf): Path<String>,
) -> Result<Json<Vec<Published>>, Failure> {
    let org = s
        .publish
        .as_ref()
        .map(|p| p.config.official_org.to_lowercase())
        .unwrap_or_default();
    let state = s.clone();
    tokio::task::spawn_blocking(move || -> Result<Vec<Published>, Failure> {
        let internal = |e: String| fail(StatusCode::INTERNAL_SERVER_ERROR, e);
        let conn = state.store.reader().map_err(|e| internal(e.to_string()))?;
        let Some((_, resources)) = hub_store::read::language_card(&conn, &ietf, &org)
            .map_err(|e| internal(e.to_string()))?
        else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for r in resources {
            let p = publishing::published_of(&conn, &ietf, &r.resource, &org)
                .map_err(|e| internal(e.to_string()))?;
            let tag = p.as_ref().and_then(|(repo, _)| last_tag(&state, repo));
            let books = match &p {
                Some((repo, _)) => {
                    let (o, n) = repo.split_once('/').unwrap_or_default();
                    hub_store::read::repo(&conn, o, n)
                        .ok()
                        .flatten()
                        .map(|d| d.books.len() as i64)
                }
                None => None,
            };
            let suggest = if p.is_none() {
                publishing::best_consolidated(&conn, &ietf, &r.resource)
                    .map_err(|e| internal(e.to_string()))?
            } else {
                None
            };
            out.push(Published {
                published: p.map(|(repo, chosen)| PublishedRef {
                    writable: !org.is_empty() && repo.starts_with(&format!("{org}/")),
                    repo,
                    chosen,
                }),
                resource: r.resource,
                repos: r.repos,
                writer: r.writer,
                last_tag: tag.as_ref().map(|t| t.0.clone()),
                last_tag_unix: tag.map(|t| t.1),
                books,
                suggest,
            });
        }
        Ok(out)
    })
    .await
    .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
}

#[derive(Deserialize, ToSchema)]
pub struct PublishBody {
    pub resource: String,
    /// `owner/name` to make the published text, or null to clear the choice.
    pub repo: Option<String>,
}

/// Make a repo the published text for a language and resource (a pointer; no
/// copy is made), or clear the choice. Publishers only.
#[utoipa::path(post, path = "/v1/languages/{ietf}/published",
    params(("ietf" = String, Path)), request_body = PublishBody,
    responses((status = 200, body = bool, content_type = "application/json"), (status = 403, body = crate::api::ApiError)))]
async fn set_published(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(ietf): Path<String>,
    Json(body): Json<PublishBody>,
) -> Result<Json<bool>, Failure> {
    let me = crate::publish::require_publisher(&s, &headers).await?;
    let store = s.store.clone();
    let set = body.repo.is_some();
    tokio::task::spawn_blocking(move || -> Result<(), Failure> {
        if let Some(repo) = &body.repo {
            // Only a repo the hub knows, in this language and resource.
            let (o, n) = repo
                .split_once('/')
                .ok_or_else(|| fail(StatusCode::BAD_REQUEST, "owner/name"))?;
            let d = store
                .reader()
                .ok()
                .and_then(|c| hub_store::read::repo(&c, o, n).ok().flatten())
                .ok_or_else(|| fail(StatusCode::NOT_FOUND, format!("no public repo {repo}")))?;
            let same = |a: &Option<String>, b: &str| {
                a.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(b))
            };
            if !same(&d.summary.language, &ietf) || !same(&d.summary.resource, &body.resource) {
                return Err(fail(
                    StatusCode::CONFLICT,
                    format!("{repo} is not {ietf} {}", body.resource),
                ));
            }
        }
        store
            .set_published(&ietf, &body.resource, body.repo.as_deref(), &me.username)
            .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
    })
    .await
    .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
    Ok(Json(set))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CoverageQuery {
    /// The source to measure against, `owner/name`. Default: the one this
    /// repo declares, else the one its language's copies declare most.
    against: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ChapterCoverage {
    pub chapter: u16,
    /// The source's verses this chapter has (by number).
    pub present: usize,
    /// The source's verses in the chapter.
    pub total: usize,
}

#[derive(Serialize, ToSchema)]
pub struct BookChapters {
    pub book: String,
    /// This repo's file for the book; absent when the repo lacks it.
    pub path: Option<String>,
    pub chapters: Vec<ChapterCoverage>,
}

/// A repo against a source, chapter by chapter, in the source's book order.
#[derive(Serialize, ToSchema)]
pub struct Coverage {
    /// `owner/name` measured against; absent when none could be found.
    pub source: Option<String>,
    /// Sources one could pick instead: declared by the repo or its language's copies.
    pub candidates: Vec<String>,
    pub books: Vec<BookChapters>,
}

/// Verses present per chapter, this repo against a source (verse numbers, as
/// the progress strip counts them).
#[utoipa::path(get, path = "/v1/repos/{owner}/{name}/coverage",
    params(("owner" = String, Path), ("name" = String, Path), CoverageQuery),
    responses((status = 200, body = Coverage), (status = 404, body = crate::api::ApiError)))]
async fn coverage(
    State(s): State<AppState>,
    Path((owner, name)): Path<(String, String)>,
    Query(q): Query<CoverageQuery>,
) -> Result<Json<Coverage>, Failure> {
    let started = std::time::Instant::now();
    let state = s.clone();
    let out = tokio::task::spawn_blocking(move || -> Result<Coverage, Failure> {
        use rayon::prelude::*;
        let internal = |e: String| fail(StatusCode::INTERNAL_SERVER_ERROR, e);
        let conn = state.store.reader().map_err(|e| internal(e.to_string()))?;
        let (o, n) = (owner.to_lowercase(), name.to_lowercase());
        let d = hub_store::read::repo(&conn, &o, &n)
            .map_err(|e| internal(e.to_string()))?
            .ok_or_else(|| fail(StatusCode::NOT_FOUND, format!("no public repo {o}/{n}")))?;
        let sha = d.head_sha.clone().unwrap_or_default();
        // Candidates: what the repo's books declare, then what the language's copies declare most.
        let mut candidates: Vec<String> = d
            .books
            .iter()
            .filter_map(|b| b.coverage.as_ref().map(|c| c.source.to_lowercase()))
            .collect();
        if let (Some(lang), Some(res)) = (&d.summary.language, &d.summary.resource) {
            let mut counts: HashMap<String, usize> = HashMap::new();
            for c in
                publishing::book_copies(&conn, lang, res).map_err(|e| internal(e.to_string()))?
            {
                if let Some(src) = c.source_repo {
                    *counts.entry(src.to_lowercase()).or_default() += 1;
                }
            }
            let mut v: Vec<(String, usize)> = counts.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            candidates.extend(v.into_iter().map(|(s, _)| s));
        }
        let mut seen = std::collections::HashSet::new();
        candidates.retain(|c| seen.insert(c.clone()) && c != &format!("{o}/{n}"));
        let source = q
            .against
            .map(|a| a.to_lowercase())
            .or_else(|| candidates.first().cloned());
        let Some(src) = source.clone() else {
            return Ok(Coverage {
                source: None,
                candidates,
                books: Vec::new(),
            });
        };
        let (so, sn) = src
            .split_once('/')
            .ok_or_else(|| fail(StatusCode::BAD_REQUEST, "owner/name"))?;
        let sd = hub_store::read::repo(&conn, so, sn)
            .map_err(|e| internal(e.to_string()))?
            .ok_or_else(|| fail(StatusCode::NOT_FOUND, format!("no public repo {src}")))?;
        let ssha = sd.head_sha.clone().unwrap_or_default();
        let read = |owner: &str, name: &str, sha: &str, synthetic: bool, path: &str| {
            crate::compare::book_text(&state, owner, name, sha, synthetic, path)
                .ok()
                .map(|t| fingerprint(&t))
        };
        let mut books: Vec<BookChapters> = sd
            .books
            .par_iter()
            .filter(|b| !b.book.is_empty())
            .filter_map(|sb| {
                let theirs = read(so, sn, &ssha, sd.synthetic.is_some(), &sb.path)?;
                let ours_row = d.books.iter().find(|b| b.book == sb.book);
                let ours =
                    ours_row.and_then(|b| read(&o, &n, &sha, d.synthetic.is_some(), &b.path));
                let mut by: BTreeMap<u16, (usize, usize)> = BTreeMap::new();
                for key in theirs.verses.keys() {
                    let e = by.entry(key.0).or_default();
                    e.1 += 1;
                    if ours.as_ref().is_some_and(|f| f.verses.contains_key(key)) {
                        e.0 += 1;
                    }
                }
                Some(BookChapters {
                    book: sb.book.clone(),
                    path: ours_row.map(|b| b.path.clone()),
                    chapters: by
                        .into_iter()
                        .map(|(chapter, (present, total))| ChapterCoverage {
                            chapter,
                            present,
                            total,
                        })
                        .collect(),
                })
            })
            .collect();
        books.sort_by_key(|b| hub_stitch::usfm_file_name(&b.book).unwrap_or_default());
        Ok(Coverage {
            source,
            candidates,
            books,
        })
    })
    .await
    .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
    let span = tracing::Span::current();
    span.record("rows", out.books.len());
    span.record("work_ms", started.elapsed().as_secs_f64() * 1000.0);
    Ok(Json(out))
}
