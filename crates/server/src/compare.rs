//! `/v1/repos/{owner}/{name}/books/{book}/compare`: a book beside the source
//! its last proofreading run compared it with, verse by verse, in Sous Chef's
//! terms (`hub_proof::compare`). For the source comparison card and the
//! length charts; it replaced the web app's own verse parsing (2026-10-14).

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use hub_ingest::synthetic_at;
use hub_proof::compare::{VersePair, compare};
use hub_repo_read::RepoReader;
use hub_store::read;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};

use crate::api::{AppState, Failure};

#[derive(OpenApi)]
#[openapi(
    paths(book_compare),
    components(schemas(BookCompare, ComparedVerse, VersePair))
)]
pub struct CompareDoc;

pub fn router() -> axum::Router<AppState> {
    axum::Router::new().route(
        "/v1/repos/{owner}/{name}/books/{book}/compare",
        axum::routing::get(book_compare),
    )
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CompareQuery {
    /// Only this chapter's verses (the usual ratio still covers the book).
    chapter: Option<u16>,
    /// With `chapter`: from this verse.
    verse: Option<u16>,
    /// With `verse`: this many verses (default 1).
    count: Option<u16>,
    /// Include the verse texts (default true). Off for charts of the whole book.
    text: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub struct ComparedVerse {
    #[serde(flatten)]
    pub pair: VersePair,
    /// The length lane's verdict on this verse, when it fired: `Long` or `Short`.
    pub flag: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct BookCompare {
    /// `owner/name`.
    pub target: String,
    /// The source the run compared against, `owner/name@sha` as recorded.
    pub source: String,
    /// The source file read (at the source's current HEAD).
    pub source_path: String,
    pub book: String,
    pub usual_ratio: Option<f64>,
    pub verses: Vec<ComparedVerse>,
}

fn fail(status: StatusCode, msg: impl Into<String>) -> Failure {
    Failure(status, msg.into())
}

/// A book file's text at HEAD: the stitched file for a Writer project.
fn book_text(
    s: &AppState,
    owner: &str,
    name: &str,
    sha: &str,
    synthetic: bool,
    path: &str,
) -> Result<String, String> {
    if synthetic {
        return synthetic_at(&s.scanner, owner, name, sha)
            .map_err(|e| e.to_string())?
            .and_then(|syn| syn.files.into_iter().find(|f| f.path == path))
            .map(|f| f.text)
            .ok_or_else(|| format!("no stitched {path} in {owner}/{name}"));
    }
    let reader = RepoReader::open(&s.scanner.repo_path(owner, name)).map_err(|e| e.to_string())?;
    let bytes = reader
        .file(sha, path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no {path} in {owner}/{name}"))?;
    String::from_utf8(bytes).map_err(|_| format!("{path} in {owner}/{name} is not UTF-8"))
}

/// One book beside its source, verse by verse: both texts, both lengths
/// (graphemes, as Sous Chef counts them), the book's usual ratio, and the
/// length lane's flags.
#[utoipa::path(get, path = "/v1/repos/{owner}/{name}/books/{book}/compare",
    params(("owner" = String, Path), ("name" = String, Path), ("book" = String, Path), CompareQuery),
    responses((status = 200, body = BookCompare), (status = 404, body = crate::api::ApiError)))]
async fn book_compare(
    State(s): State<AppState>,
    Path((owner, name, book)): Path<(String, String, String)>,
    Query(q): Query<CompareQuery>,
) -> Result<Json<BookCompare>, Failure> {
    let (o, n, b) = (
        owner.to_lowercase(),
        name.to_lowercase(),
        book.to_uppercase(),
    );
    let started = std::time::Instant::now();
    let state = s.clone();
    let out = tokio::task::spawn_blocking(move || -> Result<BookCompare, Failure> {
        let internal = |e: String| fail(StatusCode::INTERNAL_SERVER_ERROR, e);
        let conn = state.store.reader().map_err(|e| internal(e.to_string()))?;
        let corpus = read::proof_body(&conn, &o, &n)
            .map_err(|e| internal(e.to_string()))?
            .ok_or_else(|| {
                fail(
                    StatusCode::NOT_FOUND,
                    format!("no proofreading run for {o}/{n} yet"),
                )
            })?;
        let corpus: serde_json::Value =
            serde_json::from_str(&corpus).map_err(|e| internal(e.to_string()))?;
        let source = corpus["source"]
            .as_str()
            .ok_or_else(|| {
                fail(
                    StatusCode::NOT_FOUND,
                    format!("{o}/{n} was compared with no source"),
                )
            })?
            .to_owned();
        let entry = corpus["books"]
            .as_array()
            .and_then(|bs| bs.iter().find(|x| x["book"].as_str() == Some(b.as_str())))
            .ok_or_else(|| fail(StatusCode::NOT_FOUND, format!("no {b} in {o}/{n}'s run")))?;
        let target_path = entry["id"].as_str().unwrap_or_default().to_owned();
        let source_path = entry["source"]
            .as_str()
            .ok_or_else(|| fail(StatusCode::NOT_FOUND, format!("the source has no {b}")))?
            .to_owned();
        let (so, sn) = source
            .split('@')
            .next()
            .and_then(|r| r.split_once('/'))
            .map(|(a, c)| (a.to_lowercase(), c.to_lowercase()))
            .ok_or_else(|| internal(format!("unreadable source {source}")))?;
        let head = |owner: &str, name: &str| -> Result<(String, bool), Failure> {
            match read::repo_head(&conn, owner, name).map_err(|e| internal(e.to_string()))? {
                Some((_, Some(sha), synthetic)) => Ok((sha, synthetic)),
                _ => Err(fail(
                    StatusCode::NOT_FOUND,
                    format!("no public repo {owner}/{name} with commits"),
                )),
            }
        };
        let (tsha, tsyn) = head(&o, &n)?;
        let (ssha, ssyn) = head(&so, &sn)?;
        // The length lane's verdicts, by chapter and verse.
        let flags: std::collections::HashMap<(u16, u16), String> =
            read::proof_rows(&conn, &o, &n, Some(&b), Some("LengthProportionality"), 2000)
                .map_err(|e| internal(e.to_string()))?
                .iter()
                .filter_map(|row| serde_json::from_str::<serde_json::Value>(row).ok())
                .filter_map(|f| {
                    let c = u16::try_from(f["chapter"].as_u64()?).ok()?;
                    let v = u16::try_from(f["verse"].as_u64()?).ok()?;
                    Some((
                        (c, v),
                        f["paired"]["kind"].as_str().unwrap_or("?").to_owned(),
                    ))
                })
                .collect();
        let target = book_text(&state, &o, &n, &tsha, tsyn, &target_path).map_err(internal)?;
        let source_text =
            book_text(&state, &so, &sn, &ssha, ssyn, &source_path).map_err(internal)?;
        let (from, to) = match (q.chapter, q.verse) {
            (Some(_), Some(v)) => (v, v.saturating_add(q.count.unwrap_or(1).max(1) - 1)),
            _ => (0, u16::MAX),
        };
        let keep =
            |c: u16, v: u16| q.chapter.is_none_or(|want| want == c) && (from..=to).contains(&v);
        let cmp = compare(&target, &source_text, q.text.unwrap_or(true), keep)
            .ok_or_else(|| internal("the engine could not parse one of the books".into()))?;
        Ok(BookCompare {
            target: format!("{o}/{n}"),
            source,
            source_path,
            book: b,
            usual_ratio: cmp.usual_ratio,
            verses: cmp
                .verses
                .into_iter()
                .map(|pair| {
                    let flag = flags.get(&(pair.chapter, pair.verse)).cloned();
                    ComparedVerse { pair, flag }
                })
                .collect(),
        })
    })
    .await
    .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
    let span = tracing::Span::current();
    span.record("rows", out.verses.len());
    span.record("work_ms", started.elapsed().as_secs_f64() * 1000.0);
    Ok(Json(out))
}
