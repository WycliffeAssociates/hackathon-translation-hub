//! `/v1`: the hub's read API. Every handler reads the store (or a repo on
//! disk) on the blocking pool and returns JSON; the OpenAPI document at
//! `/v1/openapi.json` is generated from these signatures, and the web app's
//! types are generated from that.

use std::sync::Arc;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use hub_ingest::{Scanner, synthetic_at};
use hub_repo_read::{EntryKind, RepoReader};
use hub_store::Store;
use hub_store::read::{self, RepoFilter};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub scanner: Arc<Scanner>,
    /// Sign-in and writes to the official org, when configured.
    pub publish: Option<Arc<crate::publish::Publisher>>,
}

/// The whole contract: the read API plus sign-in and publishing.
pub fn openapi() -> utoipa::openapi::OpenApi {
    let mut doc = ApiDoc::openapi();
    doc.merge(crate::publish::PublishDoc::openapi());
    doc.merge(crate::progress::ProgressDoc::openapi());
    doc.merge(crate::compare::CompareDoc::openapi());
    doc.merge(crate::render::RenderDoc::openapi());
    doc.merge(crate::sources::SourcesDoc::openapi());
    doc.merge(crate::changes::ChangesDoc::openapi());
    doc.merge(crate::home::HomeDoc::openapi());
    doc
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Translation Hub",
        version = "0.1.0",
        description = "Read-only views of Bible translation work on WACS: repos, books, checks, catalog status."
    ),
    paths(
        list_repos,
        get_repo,
        repo_tree,
        repo_file,
        repo_proof,
        repo_findings,
        hub_findings,
        hub_code,
        book_history,
        list_languages,
        get_language,
        biel_conflicts,
        status
    ),
    components(schemas(
        read::RepoPage,
        read::RepoSummary,
        read::RepoDetail,
        read::BookRow,
        read::LintRow,
        read::CoverageRow,
        read::CatalogRef,
        read::ProofSummary,
        read::LanguageSummary,
        read::LanguageDetail,
        read::BookPick,
        read::BielConflict,
        read::HistoryPoint,
        read::Totals,
        RepoTree,
        TreeEntry,
        ApiError
    ))
)]
pub struct ApiDoc;

pub fn router(state: AppState) -> axum::Router {
    axum::Router::new()
        .route("/v1/repos", get(list_repos))
        .route("/v1/repos/{owner}/{name}", get(get_repo))
        .route("/v1/repos/{owner}/{name}/tree", get(repo_tree))
        .route("/v1/repos/{owner}/{name}/file/{*path}", get(repo_file))
        .route("/v1/repos/{owner}/{name}/proof", get(repo_proof))
        .route("/v1/repos/{owner}/{name}/findings", get(repo_findings))
        .route("/v1/findings", get(hub_findings))
        .route("/v1/findings/{engine}/{code}", get(hub_code))
        .route(
            "/v1/repos/{owner}/{name}/books/{book}/history",
            get(book_history),
        )
        .route("/v1/languages", get(list_languages))
        .route("/v1/languages/{ietf}", get(get_language))
        .route("/v1/biel/conflicts", get(biel_conflicts))
        .route("/v1/status", get(status))
        .route("/v1/openapi.json", get(|| async { Json(openapi()) }))
        .merge(crate::publish::router())
        .merge(crate::progress::router())
        .merge(crate::compare::router())
        .merge(crate::render::router())
        .merge(crate::sources::router())
        .merge(crate::changes::router())
        .merge(crate::home::router())
        .with_state(state)
}

/// Every error body: `{"error": "..."}`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiError {
    pub error: String,
}

pub struct Failure(pub StatusCode, pub String);

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        if self.0.is_server_error() {
            tracing::error!(status = %self.0, error = %self.1, "request failed");
        }
        (self.0, Json(ApiError { error: self.1 })).into_response()
    }
}

fn not_found(what: impl Into<String>) -> Failure {
    Failure(StatusCode::NOT_FOUND, what.into())
}

type ApiResult<T> = Result<T, Failure>;

/// Runs `f` on the blocking pool with a fresh read connection, inside the
/// request's span.
async fn with_reader<T: Send + 'static>(
    store: &Arc<Store>,
    f: impl FnOnce(&hub_store::Connection) -> hub_store::Result<T> + Send + 'static,
) -> ApiResult<T> {
    let store = store.clone();
    let span = tracing::Span::current();
    tokio::task::spawn_blocking(move || {
        let _enter = span.enter();
        let started = std::time::Instant::now();
        let conn = store.reader()?;
        let out = f(&conn);
        span.record("db_ms", started.elapsed().as_secs_f64() * 1000.0);
        out
    })
    .await
    .map_err(|e| Failure(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| Failure(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RepoQuery {
    /// Matches owner/name, title or language code.
    q: Option<String>,
    language: Option<String>,
    /// `ts`, `rc`, `burrito`, `usfm`, `other`, `empty`.
    shape: Option<String>,
    /// e.g. `Bible`, `Open Bible Stories`, `Translation Notes`.
    subject: Option<String>,
    /// A Forgejo topic, e.g. `consolidated`.
    topic: Option<String>,
    /// Only synthetic (Writer) repos, or only real ones.
    synthetic: Option<bool>,
    /// Default 50, at most 500.
    limit: Option<u32>,
    offset: Option<u32>,
}

/// Repos, newest HEAD first: the home feed and search.
#[utoipa::path(get, path = "/v1/repos", params(RepoQuery),
    responses((status = 200, body = read::RepoPage)))]
async fn list_repos(
    State(s): State<AppState>,
    Query(q): Query<RepoQuery>,
) -> ApiResult<Json<read::RepoPage>> {
    let filter = RepoFilter {
        q: q.q,
        language: q.language,
        shape: q.shape,
        subject: q.subject,
        topic: q.topic,
        synthetic: q.synthetic,
        limit: q.limit.unwrap_or(50),
        offset: q.offset.unwrap_or(0),
    };
    with_reader(&s.store, move |c| read::repos(c, &filter))
        .await
        .map(Json)
}

/// One repo at HEAD: manifest, books with lint and coverage, catalog rows,
/// the sous-chef summary.
#[utoipa::path(get, path = "/v1/repos/{owner}/{name}",
    params(("owner" = String, Path), ("name" = String, Path)),
    responses((status = 200, body = read::RepoDetail), (status = 404, body = ApiError)))]
async fn get_repo(
    State(s): State<AppState>,
    Path((owner, name)): Path<(String, String)>,
) -> ApiResult<Json<read::RepoDetail>> {
    let (o, n) = (owner.to_lowercase(), name.to_lowercase());
    with_reader(&s.store, move |c| read::repo(c, &o, &n))
        .await?
        .map(Json)
        .ok_or_else(|| not_found(format!("no public repo {owner}/{name}")))
}

/// One file or directory in a repo's tree.
#[derive(Debug, Serialize, ToSchema)]
pub struct TreeEntry {
    /// `/`-separated, relative to the repo root.
    pub path: String,
    /// `blob`, `tree` or `submodule`.
    pub kind: String,
    pub size: Option<u64>,
}

/// A repo's files at HEAD.
#[derive(Debug, Serialize, ToSchema)]
pub struct RepoTree {
    pub head_sha: String,
    /// True for a Writer repo: `files` is the synthetic repo the hub serves
    /// (it is not a real repo), and `source_files` the Writer chunks it was
    /// stitched from.
    pub synthetic: bool,
    pub files: Vec<TreeEntry>,
    pub source_files: Option<Vec<TreeEntry>>,
}

/// The repo's files at HEAD. A Writer repo answers with its synthetic files.
#[utoipa::path(get, path = "/v1/repos/{owner}/{name}/tree",
    params(("owner" = String, Path), ("name" = String, Path)),
    responses((status = 200, body = RepoTree), (status = 404, body = ApiError)))]
async fn repo_tree(
    State(s): State<AppState>,
    Path((owner, name)): Path<(String, String)>,
) -> ApiResult<Json<RepoTree>> {
    let (owner, name) = (owner.to_lowercase(), name.to_lowercase());
    let (sha, synthetic) = head_of(&s, &owner, &name).await?;
    let scanner = s.scanner.clone();
    blocking(move || {
        let reader = RepoReader::open(&scanner.repo_path(&owner, &name)).map_err(internal)?;
        let real: Vec<TreeEntry> = reader
            .tree(&sha)
            .map_err(internal)?
            .into_iter()
            .map(|e| TreeEntry {
                path: e.path,
                kind: match e.kind {
                    EntryKind::Blob => "blob",
                    EntryKind::Tree => "tree",
                    EntryKind::Submodule => "submodule",
                }
                .into(),
                size: e.size,
            })
            .collect();
        if !synthetic {
            return Ok(RepoTree {
                head_sha: sha,
                synthetic,
                files: real,
                source_files: None,
            });
        }
        let syn = synthetic_at(&scanner, &owner, &name, &sha)
            .map_err(internal)?
            .ok_or_else(|| not_found("synthetic repo no longer builds"))?;
        Ok(RepoTree {
            head_sha: sha,
            synthetic,
            files: syn
                .files
                .iter()
                .map(|f| TreeEntry {
                    path: f.path.clone(),
                    kind: "blob".into(),
                    size: Some(f.text.len() as u64),
                })
                .collect(),
            source_files: Some(real),
        })
    })
    .await
    .map(Json)
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct FileQuery {
    /// For a synthetic repo, read the real Writer file at `path` instead of the
    /// synthetic one.
    source: Option<bool>,
}

/// One file's bytes at HEAD, with a content type from its extension (text as
/// UTF-8, audio playable in the browser).
#[utoipa::path(get, path = "/v1/repos/{owner}/{name}/file/{path}",
    params(("owner" = String, Path), ("name" = String, Path), ("path" = String, Path), FileQuery),
    responses((status = 200, description = "The file's bytes"), (status = 404, body = ApiError)))]
async fn repo_file(
    State(s): State<AppState>,
    Path((owner, name, path)): Path<(String, String, String)>,
    Query(q): Query<FileQuery>,
) -> ApiResult<Response> {
    let (owner, name) = (owner.to_lowercase(), name.to_lowercase());
    let (sha, synthetic) = head_of(&s, &owner, &name).await?;
    let scanner = s.scanner.clone();
    let bytes = blocking(move || {
        if synthetic && !q.source.unwrap_or(false) {
            let syn = synthetic_at(&scanner, &owner, &name, &sha).map_err(internal)?;
            return syn
                .and_then(|syn| syn.files.into_iter().find(|f| f.path == path))
                .map(|f| (f.path, f.text.into_bytes()))
                .ok_or_else(|| not_found(format!("no file {path}")));
        }
        let reader = RepoReader::open(&scanner.repo_path(&owner, &name)).map_err(internal)?;
        reader
            .file(&sha, &path)
            .map_err(internal)?
            .map(|b| (path.clone(), b))
            .ok_or_else(|| not_found(format!("no file {path}")))
    })
    .await?;
    let (path, bytes) = bytes;
    let mut res = Response::new(Body::from(bytes));
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(&path)),
    );
    // The bytes are those of one commit; the path names HEAD, which moves.
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    Ok(res)
}

fn content_type(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "usfm" | "sfm" | "txt" | "md" | "yaml" | "yml" | "tsv" | "csv" => {
            "text/plain; charset=utf-8"
        }
        "json" => "application/json",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "opus" => "audio/ogg",
        "m4a" => "audio/mp4",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProofQuery {
    /// Only this book (USFM id, e.g. `MAT`).
    book: Option<String>,
    /// Only this check (e.g. `Rarity`).
    code: Option<String>,
    /// Default 2000.
    limit: Option<u32>,
}

/// Documentation only: the shape `repo_proof` assembles from stored JSON.
#[derive(Serialize, ToSchema)]
pub struct ProofPage {
    /// Which books formed the corpus, and which were left out.
    pub corpus: crate::proof::ProofCorpus,
    /// The stored rows that match, in book order. At most
    /// `ROWS_PER_BOOK_CODE` (200) per book and check are stored; the true
    /// counts are in `/findings`.
    pub findings: Vec<hub_proof::Finding>,
}

/// Sous Chef rows from the last run, for one book and/or one check.
#[utoipa::path(get, path = "/v1/repos/{owner}/{name}/proof",
    params(("owner" = String, Path), ("name" = String, Path), ProofQuery),
    responses((status = 200, body = ProofPage), (status = 404, body = ApiError)))]
async fn repo_proof(
    State(s): State<AppState>,
    Path((owner, name)): Path<(String, String)>,
    Query(q): Query<ProofQuery>,
) -> ApiResult<Response> {
    let (o, n) = (owner.to_lowercase(), name.to_lowercase());
    let found = with_reader(&s.store, move |c| {
        let Some(corpus) = read::proof_body(c, &o, &n)? else {
            return Ok(None);
        };
        let book = q.book.map(|b| b.to_uppercase());
        let rows = read::proof_rows(
            c,
            &o,
            &n,
            book.as_deref(),
            q.code.as_deref(),
            q.limit.unwrap_or(2000),
        )?;
        Ok(Some((corpus, rows)))
    })
    .await?;
    let (corpus, rows) =
        found.ok_or_else(|| not_found(format!("no proofreading run for {owner}/{name} yet")))?;
    tracing::Span::current().record("rows", rows.len());
    // The rows are stored as JSON; splice them rather than parse and re-serialize.
    let body = format!("{{\"corpus\":{corpus},\"findings\":[{}]}}", rows.join(","));
    Ok(([(header::CONTENT_TYPE, "application/json")], body).into_response())
}

/// Every check that fired in the repo (lint and Sous Chef), most findings
/// first, each with its count per book.
#[utoipa::path(get, path = "/v1/repos/{owner}/{name}/findings",
    params(("owner" = String, Path), ("name" = String, Path)),
    responses((status = 200, body = Vec<read::CodeCount>)))]
async fn repo_findings(
    State(s): State<AppState>,
    Path((owner, name)): Path<(String, String)>,
) -> ApiResult<Json<Vec<read::CodeCount>>> {
    let (o, n) = (owner.to_lowercase(), name.to_lowercase());
    with_reader(&s.store, move |c| read::repo_findings(c, &o, &n))
        .await
        .map(Json)
}

/// Every check across every repo, most findings first: where a rule is noisy.
#[utoipa::path(get, path = "/v1/findings",
    responses((status = 200, body = Vec<read::HubCode>)))]
async fn hub_findings(State(s): State<AppState>) -> ApiResult<Json<Vec<read::HubCode>>> {
    with_reader(&s.store, read::hub_findings).await.map(Json)
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HubCodeQuery {
    /// Default 50.
    limit: Option<u32>,
}

/// The repos where one check fires most.
#[utoipa::path(get, path = "/v1/findings/{engine}/{code}",
    params(("engine" = String, Path, description = "`lint` or `sous`"), ("code" = String, Path), HubCodeQuery),
    responses((status = 200, body = Vec<read::RepoCodeCount>)))]
async fn hub_code(
    State(s): State<AppState>,
    Path((engine, code)): Path<(String, String)>,
    Query(q): Query<HubCodeQuery>,
) -> ApiResult<Json<Vec<read::RepoCodeCount>>> {
    with_reader(&s.store, move |c| {
        read::hub_code(c, &engine, &code, q.limit.unwrap_or(50))
    })
    .await
    .map(Json)
}

/// What the hub saw each time the book changed: lint and coverage over time.
#[utoipa::path(get, path = "/v1/repos/{owner}/{name}/books/{book}/history",
    params(("owner" = String, Path), ("name" = String, Path), ("book" = String, Path)),
    responses((status = 200, body = Vec<read::HistoryPoint>)))]
async fn book_history(
    State(s): State<AppState>,
    Path((owner, name, book)): Path<(String, String, String)>,
) -> ApiResult<Json<Vec<read::HistoryPoint>>> {
    let (o, n) = (owner.to_lowercase(), name.to_lowercase());
    with_reader(&s.store, move |c| read::book_history(c, &o, &n, &book))
        .await
        .map(Json)
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct LanguageQuery {
    /// Matches the code, the names and the alternate names.
    q: Option<String>,
    /// Default 50.
    limit: Option<u32>,
}

/// Languages that have repos here, most repos first.
#[utoipa::path(get, path = "/v1/languages", params(LanguageQuery),
    responses((status = 200, body = Vec<read::LanguageSummary>)))]
async fn list_languages(
    State(s): State<AppState>,
    Query(q): Query<LanguageQuery>,
) -> ApiResult<Json<Vec<read::LanguageSummary>>> {
    with_reader(&s.store, move |c| {
        read::languages(c, q.q.as_deref(), q.limit.unwrap_or(50))
    })
    .await
    .map(Json)
}

/// One language: its facts, its repos, and per book which repo is Official.
#[utoipa::path(get, path = "/v1/languages/{ietf}",
    params(("ietf" = String, Path)),
    responses((status = 200, body = read::LanguageDetail), (status = 404, body = ApiError)))]
async fn get_language(
    State(s): State<AppState>,
    Path(ietf): Path<String>,
) -> ApiResult<Json<read::LanguageDetail>> {
    let code = ietf.clone();
    with_reader(&s.store, move |c| read::language(c, &code))
        .await?
        .map(Json)
        .ok_or_else(|| not_found(format!("nothing known about {ietf}")))
}

/// Books BIEL shows twice: two repos both Primary and shown on BIEL.
#[utoipa::path(get, path = "/v1/biel/conflicts",
    responses((status = 200, body = Vec<read::BielConflict>)))]
async fn biel_conflicts(State(s): State<AppState>) -> ApiResult<Json<Vec<read::BielConflict>>> {
    with_reader(&s.store, read::biel_conflicts).await.map(Json)
}

/// Totals and watermarks: how much is indexed and how fresh it is.
#[utoipa::path(get, path = "/v1/status", responses((status = 200, body = read::Totals)))]
async fn status(State(s): State<AppState>) -> ApiResult<Json<read::Totals>> {
    with_reader(&s.store, read::totals).await.map(Json)
}

async fn head_of(s: &AppState, owner: &str, name: &str) -> ApiResult<(String, bool)> {
    let (o, n) = (owner.to_owned(), name.to_owned());
    let found = with_reader(&s.store, move |c| read::repo_head(c, &o, &n)).await?;
    match found {
        Some((_, Some(sha), synthetic)) => Ok((sha, synthetic)),
        Some((_, None, _)) => Err(not_found(format!("{owner}/{name} is empty"))),
        None => Err(not_found(format!("no public repo {owner}/{name}"))),
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> ApiResult<T> + Send + 'static,
) -> ApiResult<T> {
    let span = tracing::Span::current();
    tokio::task::spawn_blocking(move || span.in_scope(f))
        .await
        .map_err(internal)?
}

fn internal(e: impl std::fmt::Display) -> Failure {
    Failure(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Why: web/openapi.json is the contract between this server and the web
    // app (its types are generated from it). A shape change that is not
    // regenerated and committed would compile on both sides and break at
    // runtime. Fix a failure with `cd web && pnpm gen`, then review the diff.
    #[test]
    fn openapi_matches_the_committed_contract() {
        let committed = include_str!("../../../web/openapi.json");
        let current = openapi().to_pretty_json().expect("serialize");
        assert!(
            committed.trim() == current.trim(),
            "web/openapi.json is stale: run `cd web && pnpm gen` and commit it"
        );
    }
}
