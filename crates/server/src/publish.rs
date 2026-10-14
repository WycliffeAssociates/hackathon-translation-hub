//! Sign-in and publishing: a proposal of the model agreed on 2026-10-14.
//!
//! A published translation is a POINTER: one official repo per language and
//! brand, `{official org}/{ietf}_{resource}` (Joe's `OfficialRepoName`
//! template, edition 1), and a release tag in it. Forgejo holds all of it:
//!
//! - **adopt**: a consolidated repo is forked into the official org (whole
//!   history; Forgejo shows "forked from"), when the official repo does not
//!   exist yet;
//! - **pull**: one book of any repo (a Writer project's book is stitched first)
//!   is committed into the official repo, which is created if missing, with
//!   `Source:` and `Pulled-by:` trailers, so `git blame` plus the trailers say
//!   where every line came from;
//! - **publish**: a release tag at the official repo's head.
//!
//! Only a member of the official org's publishers team may do these, checked
//! at sign-in; the service account writes, naming the person. Two `matthew.usfm`
//! files cannot both be official: there is one repo, and one file per book in it.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use base64::Engine as _;
use hub_ingest::synthetic_at;
use hub_repo_read::RepoReader;
use hub_store::publishing::{self, Action, Me, NewAction};
use hub_store::read;
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::api::{AppState, Failure};
use crate::config::Publishing;
use crate::forgejo_api::{FileChange, Forgejo, ForgejoError, Release};

/// Configured publishing: the settings and a client that has passed the write guard.
pub struct Publisher {
    pub config: Publishing,
    pub forgejo: Forgejo,
}

const SESSION_COOKIE: &str = "hub_session";
const STATE_COOKIE: &str = "hub_oauth";
const SESSION_SECS: i64 = 7 * 24 * 3600;

#[derive(OpenApi)]
#[openapi(
    paths(me, official, adopt, pull, publish_release),
    components(schemas(
        MeResponse,
        Me,
        Official,
        OfficialRepo,
        Release,
        Action,
        PullBody,
        ReleaseBody,
        Done
    ))
)]
pub struct PublishDoc;

pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/v1/auth/login", get(login))
        .route("/v1/auth/callback", get(callback))
        .route("/v1/auth/logout", post(logout))
        .route("/v1/me", get(me))
        .route("/v1/repos/{owner}/{name}/official", get(official))
        .route("/v1/repos/{owner}/{name}/adopt", post(adopt))
        .route("/v1/repos/{owner}/{name}/pull", post(pull))
        .route("/v1/repos/{owner}/{name}/releases", post(publish_release))
}

fn fail(status: StatusCode, msg: impl Into<String>) -> Failure {
    Failure(status, msg.into())
}

fn upstream(e: ForgejoError) -> Failure {
    match e {
        ForgejoError::Refused(m) => fail(StatusCode::FORBIDDEN, m),
        other => fail(StatusCode::BAD_GATEWAY, other.to_string()),
    }
}

fn publisher(s: &AppState) -> Result<&Arc<Publisher>, Failure> {
    s.publish.as_ref().ok_or_else(|| {
        fail(
            StatusCode::NOT_FOUND,
            "publishing is not configured (HUB_FORGEJO_URL, HUB_FORGEJO_TOKEN)",
        )
    })
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

fn random_id() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("os randomness");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The signed-in person behind this request, if any.
pub async fn current(s: &AppState, headers: &HeaderMap) -> Option<Me> {
    let id = cookie(headers, SESSION_COOKIE)?.to_owned();
    let store = s.store.clone();
    tokio::task::spawn_blocking(move || {
        publishing::session(&store.reader().ok()?, &id)
            .ok()
            .flatten()
    })
    .await
    .ok()
    .flatten()
}

async fn require_publisher(s: &AppState, headers: &HeaderMap) -> Result<Me, Failure> {
    match current(s, headers).await {
        Some(me) if me.publisher => Ok(me),
        Some(me) => Err(fail(
            StatusCode::FORBIDDEN,
            format!("{} is not in the publishers team", me.username),
        )),
        None => Err(fail(StatusCode::UNAUTHORIZED, "sign in first")),
    }
}

fn redirect(to: &str, cookies: &[String]) -> Response {
    let mut res = (StatusCode::FOUND, [(header::LOCATION, to.to_owned())]).into_response();
    for c in cookies {
        if let Ok(v) = HeaderValue::from_str(c) {
            res.headers_mut().append(header::SET_COOKIE, v);
        }
    }
    res
}

fn cookie_attrs(p: &Publishing, max_age: i64) -> String {
    let secure = if p.public_url.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    format!("Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}")
}

// --- sign-in ---------------------------------------------------------------

#[derive(Deserialize)]
struct LoginQuery {
    next: Option<String>,
}

/// Off to Forgejo's sign-in page; back at `/v1/auth/callback`.
async fn login(
    State(s): State<AppState>,
    Query(q): Query<LoginQuery>,
) -> Result<Response, Failure> {
    let p = &publisher(&s)?.config;
    // Only a path on this site, so the callback cannot be made to bounce elsewhere.
    let next = q
        .next
        .filter(|n| n.starts_with('/') && !n.starts_with("//"))
        .unwrap_or_else(|| "/".into());
    let state = random_id();
    let packed = format!(
        "{state}.{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&next)
    );
    let to = format!(
        "{}/login/oauth/authorize?client_id={}&redirect_uri={}&response_type=code&state={state}",
        p.forgejo_public_url,
        p.oauth_client_id,
        url_encode(&format!("{}/v1/auth/callback", p.public_url)),
    );
    Ok(redirect(
        &to,
        &[format!("{STATE_COOKIE}={packed}; {}", cookie_attrs(p, 600))],
    ))
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn callback(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Result<Response, Failure> {
    let pb = publisher(&s)?.clone();
    if let Some(e) = q.error {
        return Err(fail(StatusCode::UNAUTHORIZED, format!("Forgejo said: {e}")));
    }
    let (Some(code), Some(state)) = (q.code, q.state) else {
        return Err(fail(StatusCode::BAD_REQUEST, "no code"));
    };
    let packed = cookie(&headers, STATE_COOKIE)
        .ok_or_else(|| fail(StatusCode::BAD_REQUEST, "sign-in expired; try again"))?;
    let (expected, next) = packed.split_once('.').unwrap_or((packed, ""));
    if expected != state {
        return Err(fail(
            StatusCode::BAD_REQUEST,
            "sign-in state does not match; try again",
        ));
    }
    let next = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(next)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .filter(|n| n.starts_with('/') && !n.starts_with("//"))
        .unwrap_or_else(|| "/".into());

    let token = pb
        .forgejo
        .exchange_code(&pb.config, &code)
        .await
        .map_err(upstream)?;
    let user = pb.forgejo.user(&token).await.map_err(upstream)?;
    let is_publisher = pb
        .forgejo
        .in_team(&pb.config.publishers_team, &user.login)
        .await
        .map_err(upstream)?;
    let me = Me {
        username: user.login,
        full_name: Some(user.full_name).filter(|n| !n.is_empty()),
        avatar_url: Some(user.avatar_url).filter(|n| !n.is_empty()),
        publisher: is_publisher,
    };
    let id = random_id();
    {
        let (store, id, me) = (s.store.clone(), id.clone(), me.clone());
        tokio::task::spawn_blocking(move || store.create_session(&id, &me, SESSION_SECS))
            .await
            .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    tracing::info!(event = "sign-in", username = %me.username, publisher = me.publisher, "signed in");
    Ok(redirect(
        &next,
        &[
            format!(
                "{SESSION_COOKIE}={id}; {}",
                cookie_attrs(&pb.config, SESSION_SECS)
            ),
            format!("{STATE_COOKIE}=; {}", cookie_attrs(&pb.config, 0)),
        ],
    ))
}

async fn logout(State(s): State<AppState>, headers: HeaderMap) -> Result<Response, Failure> {
    if let Some(id) = cookie(&headers, SESSION_COOKIE).map(str::to_owned) {
        let store = s.store.clone();
        let _ = tokio::task::spawn_blocking(move || store.end_session(&id)).await;
    }
    let attrs = s
        .publish
        .as_ref()
        .map_or("Path=/; Max-Age=0".into(), |p| cookie_attrs(&p.config, 0));
    let mut res = StatusCode::NO_CONTENT.into_response();
    if let Ok(v) = HeaderValue::from_str(&format!("{SESSION_COOKIE}=; {attrs}")) {
        res.headers_mut().insert(header::SET_COOKIE, v);
    }
    Ok(res)
}

#[derive(Serialize, ToSchema)]
pub struct MeResponse {
    /// Whether this hub can sign people in and publish at all.
    pub publishing: bool,
    /// The official org (`TextTranslation`), when publishing is on.
    pub official_org: Option<String>,
    /// The signed-in person, or null.
    pub me: Option<Me>,
}

/// Who is signed in.
#[utoipa::path(get, path = "/v1/me", responses((status = 200, body = MeResponse)))]
async fn me(State(s): State<AppState>, headers: HeaderMap) -> Json<MeResponse> {
    Json(MeResponse {
        publishing: s.publish.is_some(),
        official_org: s.publish.as_ref().map(|p| p.config.official_org.clone()),
        me: current(&s, &headers).await,
    })
}

// --- the official repo -----------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct OfficialRepo {
    pub owner: String,
    pub name: String,
    /// Whether it exists on Forgejo yet.
    pub exists: bool,
    pub html_url: String,
    /// Its releases, newest first.
    pub releases: Vec<Release>,
}

/// What the official side looks like from one repo's page.
#[derive(Serialize, ToSchema)]
pub struct Official {
    /// This repo is itself in the official org.
    pub is_official: bool,
    /// Where this repo's language and brand publish: `{org}/{ietf}_{resource}`.
    /// Null when the repo declares no language or resource.
    pub target: Option<OfficialRepo>,
    /// Why there is no target.
    pub reason: Option<String>,
    /// The signed-in person may adopt, pull and publish.
    pub can_publish: bool,
    /// Adopt is offered: a consolidated (not Writer) repo, and no official repo yet.
    pub can_adopt: bool,
    /// What the hub wrote from this repo, or to it.
    pub actions: Vec<Action>,
}

/// The official repo name for a language and brand, or why there is none.
/// Joe's template: `{ietf}_{transcode}`, the language tag lower case with
/// hyphens kept, the transcode `[a-z0-9]{2,8}`; edition 1 has no suffix.
fn official_name(language: Option<&str>, resource: Option<&str>) -> Result<String, String> {
    let lang = language
        .ok_or("the repo declares no language")?
        .to_lowercase();
    let code = resource
        .ok_or("the repo declares no resource (brand)")?
        .to_lowercase();
    if !(2..=8).contains(&code.len())
        || !code
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    {
        return Err(format!(
            "`{code}` is not a usable brand code ([a-z0-9]{{2,8}})"
        ));
    }
    if lang.is_empty() || !lang.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err(format!("`{lang}` is not a usable language tag"));
    }
    Ok(format!("{lang}_{code}"))
}

struct Facts {
    detail: read::RepoDetail,
    actions: Vec<Action>,
}

async fn facts(s: &AppState, owner: &str, name: &str) -> Result<Facts, Failure> {
    let (store, o, n) = (s.store.clone(), owner.to_lowercase(), name.to_lowercase());
    tokio::task::spawn_blocking(move || -> hub_store::Result<Option<Facts>> {
        let c = store.reader()?;
        let Some(detail) = read::repo(&c, &o, &n)? else {
            return Ok(None);
        };
        let actions = publishing::actions(&c, &format!("{o}/{n}"))?;
        Ok(Some(Facts { detail, actions }))
    })
    .await
    .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or_else(|| {
        fail(
            StatusCode::NOT_FOUND,
            format!("no public repo {owner}/{name}"),
        )
    })
}

/// The official target for a repo: itself if it is official, else its language and brand's.
fn target_of(pb: &Publisher, d: &read::RepoDetail) -> Result<(String, String), String> {
    let org = &pb.config.official_org;
    if d.summary.owner.eq_ignore_ascii_case(org) {
        return Ok((org.clone(), d.summary.name.clone()));
    }
    official_name(d.summary.language.as_deref(), d.summary.resource.as_deref())
        .map(|n| (org.clone(), n))
}

/// The official repo this one publishes to, its releases, and what the signed-in person may do.
#[utoipa::path(get, path = "/v1/repos/{owner}/{name}/official",
    params(("owner" = String, Path), ("name" = String, Path)),
    responses((status = 200, body = Official), (status = 404, body = crate::api::ApiError)))]
async fn official(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
) -> Result<Json<Official>, Failure> {
    let pb = publisher(&s)?.clone();
    let f = facts(&s, &owner, &name).await?;
    let me = current(&s, &headers).await;
    let is_official = f
        .detail
        .summary
        .owner
        .eq_ignore_ascii_case(&pb.config.official_org);
    let (target, reason) = match target_of(&pb, &f.detail) {
        Ok((o, n)) => {
            let exists = pb.forgejo.repo_exists(&o, &n).await.map_err(upstream)?;
            let releases = if exists {
                pb.forgejo.releases(&o, &n).await.map_err(upstream)?
            } else {
                Vec::new()
            };
            let html_url = format!("{}/{o}/{n}", pb.config.forgejo_public_url);
            (
                Some(OfficialRepo {
                    owner: o,
                    name: n,
                    exists,
                    html_url,
                    releases,
                }),
                None,
            )
        }
        Err(why) => (None, Some(why)),
    };
    let can_adopt =
        !is_official && f.detail.synthetic.is_none() && target.as_ref().is_some_and(|t| !t.exists);
    let mut actions = f.actions;
    if let Some(t) = &target {
        // The official repo's own log too, when looking from a source.
        if !is_official {
            let (store, key) = (
                s.store.clone(),
                format!("{}/{}", t.owner, t.name).to_lowercase(),
            );
            if let Ok(Ok(more)) =
                tokio::task::spawn_blocking(move || -> hub_store::Result<Vec<Action>> {
                    publishing::actions(&store.reader()?, &key)
                })
                .await
            {
                for a in more {
                    if !actions
                        .iter()
                        .any(|b| b.at_unix == a.at_unix && b.kind == a.kind && b.target == a.target)
                    {
                        actions.push(a);
                    }
                }
                actions.sort_by_key(|a| std::cmp::Reverse(a.at_unix));
            }
        }
    }
    Ok(Json(Official {
        is_official,
        target,
        reason,
        can_publish: me.is_some_and(|m| m.publisher),
        can_adopt,
        actions,
    }))
}

#[derive(Serialize, ToSchema)]
pub struct Done {
    /// What was written, in a sentence.
    pub message: String,
    /// The official repo.
    pub target: String,
    /// Forgejo's page for what was written.
    pub html_url: String,
}

/// Adopt: fork this consolidated repo into the official org as its language and brand's official repo.
#[utoipa::path(post, path = "/v1/repos/{owner}/{name}/adopt",
    params(("owner" = String, Path), ("name" = String, Path)),
    responses((status = 200, body = Done), (status = 403, body = crate::api::ApiError), (status = 409, body = crate::api::ApiError)))]
async fn adopt(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
) -> Result<Json<Done>, Failure> {
    let pb = publisher(&s)?.clone();
    let me = require_publisher(&s, &headers).await?;
    let f = facts(&s, &owner, &name).await?;
    if f.detail.synthetic.is_some() {
        return Err(fail(
            StatusCode::CONFLICT,
            "a Writer project is one book: pull its book instead",
        ));
    }
    let (org, target) = target_of(&pb, &f.detail).map_err(|e| fail(StatusCode::CONFLICT, e))?;
    if f.detail.summary.owner.eq_ignore_ascii_case(&org) {
        return Err(fail(StatusCode::CONFLICT, "this repo is already official"));
    }
    if pb
        .forgejo
        .repo_exists(&org, &target)
        .await
        .map_err(upstream)?
    {
        return Err(fail(
            StatusCode::CONFLICT,
            format!("{org}/{target} exists; pull books into it instead"),
        ));
    }
    let src = format!("{}/{}", f.detail.summary.owner, f.detail.summary.name);
    pb.forgejo
        .fork(&f.detail.summary.owner, &f.detail.summary.name, &target)
        .await
        .map_err(upstream)?;
    let source = format!("{src}@{}", f.detail.head_sha.clone().unwrap_or_default());
    log(
        &s,
        NewAction {
            username: &me.username,
            kind: "adopt",
            target: &format!("{org}/{target}").to_lowercase(),
            source: Some(&source),
            book: None,
            reference: None,
            detail: None,
        },
    )
    .await;
    Ok(Json(Done {
        message: format!("{src} is now {org}/{target} (forked, with its history)."),
        target: format!("{org}/{target}"),
        html_url: format!("{}/{org}/{target}", pb.config.forgejo_public_url),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct PullBody {
    /// The book to pull: `RUT`.
    pub book: String,
}

/// Pull one book of this repo (a Writer project's stitched book) into the official repo, as one commit.
#[utoipa::path(post, path = "/v1/repos/{owner}/{name}/pull",
    params(("owner" = String, Path), ("name" = String, Path)),
    request_body = PullBody,
    responses((status = 200, body = Done), (status = 403, body = crate::api::ApiError), (status = 409, body = crate::api::ApiError)))]
async fn pull(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Json(body): Json<PullBody>,
) -> Result<Json<Done>, Failure> {
    let pb = publisher(&s)?.clone();
    let me = require_publisher(&s, &headers).await?;
    let f = facts(&s, &owner, &name).await?;
    let book = body.book.to_uppercase();
    let (org, target) = target_of(&pb, &f.detail).map_err(|e| fail(StatusCode::CONFLICT, e))?;
    if f.detail.summary.owner.eq_ignore_ascii_case(&org) {
        return Err(fail(StatusCode::CONFLICT, "this repo is the official one"));
    }
    let row = f
        .detail
        .books
        .iter()
        .find(|b| b.book == book)
        .ok_or_else(|| {
            fail(
                StatusCode::NOT_FOUND,
                format!("no {book} in {owner}/{name}"),
            )
        })?;
    let sha = f
        .detail
        .head_sha
        .clone()
        .ok_or_else(|| fail(StatusCode::CONFLICT, "the repo is empty"))?;
    let file_name = hub_stitch::usfm_file_name(&book).ok_or_else(|| {
        fail(
            StatusCode::CONFLICT,
            format!("{book} is not a book of the canon"),
        )
    })?;

    // The book's text at HEAD: the stitched file for a Writer project, the file itself otherwise.
    let text = {
        let (scanner, o, n, path, sha, synthetic) = (
            s.scanner.clone(),
            f.detail.summary.owner.to_lowercase(),
            f.detail.summary.name.to_lowercase(),
            row.path.clone(),
            sha.clone(),
            f.detail.synthetic.is_some(),
        );
        tokio::task::spawn_blocking(move || -> Result<String, String> {
            if synthetic {
                let syn = synthetic_at(&scanner, &o, &n, &sha).map_err(|e| e.to_string())?;
                return syn
                    .and_then(|s| s.files.into_iter().find(|f| f.path == path))
                    .map(|f| f.text)
                    .ok_or_else(|| format!("no stitched {path}"));
            }
            let reader = RepoReader::open(&scanner.repo_path(&o, &n)).map_err(|e| e.to_string())?;
            let bytes = reader
                .file(&sha, &path)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("no {path}"))?;
            String::from_utf8(bytes).map_err(|_| format!("{path} is not UTF-8"))
        })
        .await
        .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e))?
    };

    let created = if pb
        .forgejo
        .repo_exists(&org, &target)
        .await
        .map_err(upstream)?
    {
        false
    } else {
        let lang = f.detail.summary.language.clone().unwrap_or_default();
        let res = f.detail.summary.resource.clone().unwrap_or_default();
        pb.forgejo
            .create_repo(
                &target,
                &format!("Official {lang} {res}: books pulled in through the Translation Hub"),
            )
            .await
            .map_err(upstream)?;
        true
    };
    let replaces = pb
        .forgejo
        .file_sha(&org, &target, &file_name)
        .await
        .map_err(upstream)?;
    let src = format!("{}/{}", f.detail.summary.owner, f.detail.summary.name);
    let message = format!(
        "{} {book} from {src}\n\nSource: {src}@{sha}\nSource-path: {}\nPulled-by: {}\nPulled-with: translation-hub\n",
        if replaces.is_some() { "Replace" } else { "Add" },
        row.path,
        me.username,
    );
    let commit = pb
        .forgejo
        .commit(
            &org,
            &target,
            &message,
            &[FileChange {
                path: &file_name,
                text: &text,
                replaces: replaces.clone(),
            }],
        )
        .await
        .map_err(upstream)?;
    log(&s, NewAction {
        username: &me.username,
        kind: "pull",
        target: &format!("{org}/{target}").to_lowercase(),
        source: Some(&format!("{src}@{sha}").to_lowercase()),
        book: Some(&book),
        reference: Some(&commit),
        detail: Some(serde_json::json!({ "path": file_name, "replaced": replaces.is_some(), "created_repo": created })),
    })
    .await;
    Ok(Json(Done {
        message: format!(
            "{book} {} {org}/{target}{}.",
            if replaces.is_some() {
                "replaced in"
            } else {
                "added to"
            },
            if created { " (created)" } else { "" }
        ),
        target: format!("{org}/{target}"),
        html_url: format!(
            "{}/{org}/{target}/commit/{commit}",
            pb.config.forgejo_public_url
        ),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct ReleaseBody {
    /// `v1.0`.
    pub tag: String,
    pub title: Option<String>,
    pub notes: Option<String>,
}

/// Publish: a release tag at the head of this official repo (or of this repo's official one).
#[utoipa::path(post, path = "/v1/repos/{owner}/{name}/releases",
    params(("owner" = String, Path), ("name" = String, Path)),
    request_body = ReleaseBody,
    responses((status = 200, body = Done), (status = 403, body = crate::api::ApiError), (status = 409, body = crate::api::ApiError)))]
async fn publish_release(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Json(body): Json<ReleaseBody>,
) -> Result<Json<Done>, Failure> {
    let pb = publisher(&s)?.clone();
    let me = require_publisher(&s, &headers).await?;
    let f = facts(&s, &owner, &name).await?;
    let (org, target) = target_of(&pb, &f.detail).map_err(|e| fail(StatusCode::CONFLICT, e))?;
    let tag = body.tag.trim();
    let ok = tag.strip_prefix('v').is_some_and(|v| {
        !v.is_empty()
            && v.split('.')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    });
    if !ok {
        return Err(fail(
            StatusCode::BAD_REQUEST,
            format!("`{tag}` is not a version tag like v1.0"),
        ));
    }
    if !pb
        .forgejo
        .repo_exists(&org, &target)
        .await
        .map_err(upstream)?
    {
        return Err(fail(
            StatusCode::CONFLICT,
            format!("{org}/{target} does not exist yet: adopt or pull first"),
        ));
    }
    if pb
        .forgejo
        .releases(&org, &target)
        .await
        .map_err(upstream)?
        .iter()
        .any(|r| r.tag_name == tag)
    {
        return Err(fail(
            StatusCode::CONFLICT,
            format!("{org}/{target} already has {tag}"),
        ));
    }
    let title = body
        .title
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| format!("{target} {tag}"));
    let notes = format!(
        "{}\n\nPublished by {} through the Translation Hub.",
        body.notes.unwrap_or_default().trim(),
        me.username
    );
    let release = pb
        .forgejo
        .create_release(&org, &target, tag, &title, notes.trim_start())
        .await
        .map_err(upstream)?;
    log(
        &s,
        NewAction {
            username: &me.username,
            kind: "publish",
            target: &format!("{org}/{target}").to_lowercase(),
            source: None,
            book: None,
            reference: Some(tag),
            detail: Some(serde_json::json!({ "commit": release.target_commitish })),
        },
    )
    .await;
    Ok(Json(Done {
        message: format!("{org}/{target} {tag} is published."),
        target: format!("{org}/{target}"),
        html_url: release.html_url,
    }))
}

/// The hub's log of a write; a failure to log is a warning, not a failed write
/// (Forgejo already holds the write itself).
async fn log(s: &AppState, a: NewAction<'_>) {
    let store = s.store.clone();
    let (u, k, t) = (
        a.username.to_owned(),
        a.kind.to_owned(),
        a.target.to_owned(),
    );
    let (src, book, r, d) = (
        a.source.map(str::to_owned),
        a.book.map(str::to_owned),
        a.reference.map(str::to_owned),
        a.detail,
    );
    tracing::info!(event = "publish", kind = %k, target = %t, username = %u, "wrote to Forgejo");
    let res = tokio::task::spawn_blocking(move || {
        store.record_action(&NewAction {
            username: &u,
            kind: &k,
            target: &t,
            source: src.as_deref(),
            book: book.as_deref(),
            reference: r.as_deref(),
            detail: d,
        })
    })
    .await;
    if !matches!(res, Ok(Ok(()))) {
        tracing::warn!("could not log a write");
    }
}
