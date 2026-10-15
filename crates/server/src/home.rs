//! A signed-in person's home (`/welcome`, proposal 2026-10-15): the
//! languages they picked, each with its resources and a way into its sources;
//! the repos they own or their organisations own; and the repos they starred
//! on WACS, Forgejo's own pin, so pinning works from the WACS UI as well.
//! Edit permission alone would not do: an admin can edit every repo.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use hub_store::read::{self, LanguageResource, LanguageSummary, RepoSummary};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::api::{AppState, Failure};

#[derive(OpenApi)]
#[openapi(
    paths(home, set_languages, starred, set_star),
    components(schemas(Home, HomeLanguage, LanguagesBody, LanguageResource, Star, StarBody))
)]
pub struct HomeDoc;

pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/v1/me/home", axum::routing::get(home))
        .route("/v1/me/languages", axum::routing::post(set_languages))
        .route(
            "/v1/repos/{owner}/{name}/star",
            axum::routing::get(starred).post(set_star),
        )
}

fn fail(status: StatusCode, msg: impl Into<String>) -> Failure {
    Failure(status, msg.into())
}

#[derive(Serialize, ToSchema)]
pub struct HomeLanguage {
    #[serde(flatten)]
    pub language: LanguageSummary,
    pub resources: Vec<LanguageResource>,
}

#[derive(Serialize, ToSchema)]
pub struct Home {
    pub username: String,
    pub languages: Vec<HomeLanguage>,
    /// Repos the person or one of their organisations owns, newest first.
    pub mine: Vec<RepoSummary>,
    /// The organisations counted in `mine`.
    pub orgs: Vec<String>,
    /// Repos starred on WACS, in Forgejo's order.
    pub starred: Vec<RepoSummary>,
}

#[derive(Deserialize, ToSchema)]
pub struct LanguagesBody {
    /// IETF tags, in the order to show them.
    pub languages: Vec<String>,
}

async fn who(s: &AppState, headers: &HeaderMap) -> Result<String, Failure> {
    crate::publish::current(s, headers)
        .await
        .map(|m| m.username)
        .ok_or_else(|| fail(StatusCode::UNAUTHORIZED, "sign in first"))
}

/// The signed-in person's languages and projects.
#[utoipa::path(get, path = "/v1/me/home",
    responses((status = 200, body = Home), (status = 401, body = crate::api::ApiError)))]
async fn home(State(s): State<AppState>, headers: HeaderMap) -> Result<Json<Home>, Failure> {
    let username = who(&s, &headers).await?;
    let pb = s
        .publish
        .clone()
        .ok_or_else(|| fail(StatusCode::NOT_FOUND, "publishing is off"))?;
    let (orgs, starred) = tokio::join!(pb.forgejo.orgs(&username), pb.forgejo.starred(&username));
    // Forgejo down is a thinner home, not a failed one.
    let orgs = orgs.unwrap_or_default();
    let starred = starred.unwrap_or_default();
    let org = pb.config.official_org.clone();
    let store = s.store.clone();
    let user = username.clone();
    let out = tokio::task::spawn_blocking(move || -> hub_store::Result<Home> {
        let c = store.reader()?;
        let languages = read::my_languages(&c, &user)?
            .iter()
            .filter_map(|ietf| read::language_card(&c, ietf, &org).ok().flatten())
            .map(|(language, resources)| HomeLanguage {
                language,
                resources,
            })
            .collect();
        let owners: Vec<String> = std::iter::once(user.clone())
            .chain(orgs.iter().cloned())
            .collect();
        Ok(Home {
            mine: read::repos_owned_by(&c, &owners, 100)?,
            starred: read::repos_named(&c, &starred)?,
            orgs,
            languages,
            username: user,
        })
    })
    .await
    .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    tracing::Span::current().record("rows", out.mine.len() + out.starred.len());
    Ok(Json(out))
}

/// Replace the signed-in person's languages.
#[utoipa::path(post, path = "/v1/me/languages", request_body = LanguagesBody,
    responses((status = 200, body = Vec<String>), (status = 401, body = crate::api::ApiError)))]
async fn set_languages(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LanguagesBody>,
) -> Result<Json<Vec<String>>, Failure> {
    let username = who(&s, &headers).await?;
    let list: Vec<String> = body
        .languages
        .iter()
        .map(|l| l.trim().to_lowercase())
        .filter(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        .take(50)
        .collect();
    let (store, saved) = (s.store.clone(), list.clone());
    tokio::task::spawn_blocking(move || store.set_my_languages(&username, &saved))
        .await
        .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| fail(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(list))
}

/// Whether the signed-in person starred a repo: Forgejo's own pin, which is
/// how the hub's Your work follows a repo.
#[derive(Serialize, ToSchema)]
pub struct Star {
    pub starred: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct StarBody {
    pub star: bool,
}

/// Whether the signed-in person starred this repo on Forgejo.
#[utoipa::path(get, path = "/v1/repos/{owner}/{name}/star",
    params(("owner" = String, Path), ("name" = String, Path)),
    responses((status = 200, body = Star), (status = 401, body = crate::api::ApiError)))]
async fn starred(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
) -> Result<Json<Star>, Failure> {
    let token = crate::publish::user_token(&s, &headers).await?;
    let pb = s
        .publish
        .clone()
        .ok_or_else(|| fail(StatusCode::NOT_FOUND, "publishing is off"))?;
    let starred = pb
        .forgejo
        .starred_as(&token, &owner, &name)
        .await
        .map_err(|e| fail(StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(Json(Star { starred }))
}

/// Star or unstar this repo on Forgejo, as the signed-in person.
#[utoipa::path(post, path = "/v1/repos/{owner}/{name}/star",
    params(("owner" = String, Path), ("name" = String, Path)), request_body = StarBody,
    responses((status = 200, body = Star), (status = 401, body = crate::api::ApiError)))]
async fn set_star(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path((owner, name)): Path<(String, String)>,
    Json(body): Json<StarBody>,
) -> Result<Json<Star>, Failure> {
    let token = crate::publish::user_token(&s, &headers).await?;
    let pb = s
        .publish
        .clone()
        .ok_or_else(|| fail(StatusCode::NOT_FOUND, "publishing is off"))?;
    pb.forgejo
        .star_as(&token, &owner, &name, body.star)
        .await
        .map_err(|e| fail(StatusCode::BAD_GATEWAY, e.to_string()))?;
    Ok(Json(Star { starred: body.star }))
}
