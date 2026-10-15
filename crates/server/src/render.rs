//! `/v1/repos/{owner}/{name}/render/{*path}`: a markdown or TSV file as
//! sanitised HTML whose links stay in the hub (`hub-markdown`). This module
//! is the caller that knows the repos: it resolves `rc://` links to the repo
//! that holds the target (this repo's owner first, then WA-Catalog, then the
//! newest) and reads the target's title for a bare `[[rc://…]]`.

use std::cell::RefCell;
use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use hub_markdown::{Links, Rc, render_markdown, render_tsv};
use hub_repo_read::RepoReader;
use hub_store::Connection;
use hub_store::read;
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};

use crate::api::{AppState, Failure};

#[derive(OpenApi)]
#[openapi(paths(render_file), components(schemas(Rendered)))]
pub struct RenderDoc;

pub fn router() -> axum::Router<AppState> {
    axum::Router::new().route(
        "/v1/repos/{owner}/{name}/render/{*path}",
        axum::routing::get(render_file),
    )
}

/// Rows of a TSV shown at most; the rest are counted, not drawn.
const TSV_ROWS: usize = 2000;

#[derive(Serialize, ToSchema)]
pub struct Rendered {
    /// `markdown` or `tsv`.
    pub kind: String,
    /// Sanitised HTML, safe to put in the page as is.
    pub html: String,
    /// A TSV longer than the rows shown.
    pub truncated: bool,
}

/// Path segments percent-encoded, slashes kept: a URL path.
fn encode_path(path: &str) -> String {
    path.split('/').map(encode).collect::<Vec<_>>().join("/")
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// A repo a resource link resolves to: `(owner, name, head_sha)`.
type Target = Option<(String, String, String)>;

struct HubLinks<'a> {
    state: &'a AppState,
    conn: &'a Connection,
    owner: String,
    name: String,
    here: String,
    /// For one render: a word page links dozens of notes in one repo, so the
    /// repo is looked up and opened once (it was ~2 ms a link).
    repos: RefCell<HashMap<String, Target>>,
    readers: RefCell<HashMap<String, RepoReader>>,
}

impl HubLinks<'_> {
    /// The repo that holds `rc`'s resource in its language.
    fn repo_for(&self, rc: &Rc) -> Option<(String, String, String)> {
        let key = format!("{}/{}", rc.lang, rc.resource);
        if let Some(hit) = self.repos.borrow().get(&key) {
            return hit.clone();
        }
        let found = self.lookup(rc);
        self.repos.borrow_mut().insert(key, found.clone());
        found
    }

    fn lookup(&self, rc: &Rc) -> Option<(String, String, String)> {
        let lang = rc.lang.to_lowercase();
        // WA names the manual `tm` as often as `ta`.
        let names: Vec<String> = match rc.resource.as_str() {
            "ta" | "tm" => vec![format!("{lang}_ta"), format!("{lang}_tm")],
            r => vec![format!("{lang}_{r}")],
        };
        names
            .iter()
            .find_map(|n| read::repo_named(self.conn, n, &self.owner).ok().flatten())
    }
}

impl Links for HubLinks<'_> {
    fn here(&self) -> &str {
        &self.here
    }

    fn page(&self, path: &str) -> String {
        format!("/repos/{}/{}?file={}", self.owner, self.name, encode(path))
    }

    fn raw(&self, path: &str) -> String {
        format!(
            "/v1/repos/{}/{}/file/{}",
            self.owner,
            self.name,
            encode_path(path)
        )
    }

    fn rc(&self, rc: &Rc) -> Option<(String, Option<String>)> {
        let (owner, name, sha) = self.repo_for(rc)?;
        let mut readers = self.readers.borrow_mut();
        let key = format!("{owner}/{name}");
        if !readers.contains_key(&key) {
            readers.insert(
                key.clone(),
                RepoReader::open(&self.state.scanner.repo_path(&owner, &name)).ok()?,
            );
        }
        let reader = readers.get(&key)?;
        let read = |path: &str| {
            reader
                .file(&sha, path)
                .ok()
                .flatten()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
        };
        let (path, title) = match (rc.kind.as_str(), rc.rest.as_slice()) {
            // tA: an article is a folder; its text is 01.md, its title title.md.
            ("man", [project, article, ..]) => {
                let dir = format!("{project}/{article}");
                (
                    format!("{dir}/01.md"),
                    read(&format!("{dir}/title.md")).map(|t| t.trim().to_owned()),
                )
            }
            // tW: one file; its title is the first heading.
            ("dict", rest) if !rest.is_empty() => {
                let path = format!("{}.md", rest.join("/"));
                let title = read(&path).and_then(|t| {
                    t.lines()
                        .find(|l| l.starts_with('#'))
                        .map(|l| l.trim_matches(|c| c == '#' || c == ' ').to_owned())
                });
                (path, title)
            }
            // tN / tQ: book/chapter/verse.md; a verse inside a chunk has no
            // file, so the nearest one before it in the chapter stands in.
            ("help", [book, chapter, verse, ..]) => {
                let width = verse.len();
                let first: u32 = verse.parse().unwrap_or(1);
                let path = (1..=first.max(1))
                    .rev()
                    .map(|v| format!("{book}/{chapter}/{v:0width$}.md"))
                    .find(|p| read(p).is_some())
                    .unwrap_or_else(|| format!("{book}/{chapter}/{verse}.md"));
                (path, None)
            }
            (_, rest) => (format!("{}.md", rest.join("/")), None),
        };
        Some((
            format!("/repos/{owner}/{name}?file={}", encode(&path)),
            title.filter(|t| !t.is_empty()),
        ))
    }
}

/// A markdown or TSV file as sanitised HTML, links rewritten to the hub's
/// pages (relative paths, `rc://` resource links) or kept (web links).
#[utoipa::path(get, path = "/v1/repos/{owner}/{name}/render/{path}",
    params(("owner" = String, Path), ("name" = String, Path), ("path" = String, Path)),
    responses((status = 200, body = Rendered), (status = 404, body = crate::api::ApiError)))]
async fn render_file(
    State(s): State<AppState>,
    Path((owner, name, path)): Path<(String, String, String)>,
) -> Result<Json<Rendered>, Failure> {
    let (o, n) = (owner.to_lowercase(), name.to_lowercase());
    let started = std::time::Instant::now();
    let state = s.clone();
    let out = tokio::task::spawn_blocking(move || -> Result<(Rendered, usize), Failure> {
        let fail = |status, msg: String| Failure(status, msg);
        let conn = state
            .store
            .reader()
            .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let (sha, synthetic) = match read::repo_head(&conn, &o, &n)
            .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        {
            Some((_, Some(sha), synthetic)) => (sha, synthetic),
            _ => {
                return Err(fail(
                    StatusCode::NOT_FOUND,
                    format!("no public repo {o}/{n} with commits"),
                ));
            }
        };
        let text = crate::compare::book_text(&state, &o, &n, &sha, synthetic, &path)
            .map_err(|e| fail(StatusCode::NOT_FOUND, e))?;
        let links = HubLinks {
            state: &state,
            conn: &conn,
            owner: o.clone(),
            name: n.clone(),
            here: path
                .rsplit_once('/')
                .map_or(String::new(), |(dir, _)| dir.to_owned()),
            repos: RefCell::default(),
            readers: RefCell::default(),
        };
        let bytes = text.len();
        let rendered = if path.to_lowercase().ends_with(".tsv") {
            let (html, truncated) = render_tsv(&text, &links, TSV_ROWS);
            Rendered {
                kind: "tsv".into(),
                html,
                truncated,
            }
        } else {
            Rendered {
                kind: "markdown".into(),
                html: render_markdown(&text, &links),
                truncated: false,
            }
        };
        Ok((rendered, bytes))
    })
    .await
    .map_err(|e| Failure(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
    let span = tracing::Span::current();
    span.record("rows", out.1);
    span.record("work_ms", started.elapsed().as_secs_f64() * 1000.0);
    Ok(Json(out.0))
}
