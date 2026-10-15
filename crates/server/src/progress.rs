//! `/v1/languages/{ietf}/progress`: how a language and brand's Writer books
//! grew over time, read from their git history (crates/history). For the
//! burn-up and "who worked when" charts; a proposal, like the page that
//! draws it (/viz).
//!
//! One Writer project per book, the same default pick consolidation uses
//! (Primary and shown on BIEL first). Each book's series is the verses present
//! after each save that changed them; the weekly table counts saves per author.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use hub_store::publishing;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, OpenApi, ToSchema};

use crate::api::{AppState, Failure};

#[derive(OpenApi)]
#[openapi(
    paths(language_progress),
    components(schemas(LanguageProgress, BookProgress, WeekSaves))
)]
pub struct ProgressDoc;

pub fn router() -> axum::Router<AppState> {
    axum::Router::new().route(
        "/v1/languages/{ietf}/progress",
        axum::routing::get(language_progress),
    )
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ProgressQuery {
    /// The brand: `reg`, `ulb`.
    resource: String,
}

/// One book's growth.
#[derive(Serialize, ToSchema)]
pub struct BookProgress {
    pub book: String,
    /// The Writer project it was read from, `owner/name`.
    pub repo: String,
    /// The source's verse count, when a source resolves: the book's target.
    pub source_verses: Option<i64>,
    /// Verses now.
    pub verses: i64,
    /// `[unix seconds, verses present]` after each save that changed the count.
    pub points: Vec<[i64; 2]>,
}

/// Saves by one author in one week.
#[derive(Serialize, ToSchema)]
pub struct WeekSaves {
    /// Start of the week, unix seconds (weeks from the epoch, a Thursday).
    pub week: i64,
    pub author: String,
    pub book: String,
    pub saves: i64,
}

#[derive(Serialize, ToSchema)]
pub struct LanguageProgress {
    pub language: String,
    pub resource: String,
    pub books: Vec<BookProgress>,
    pub weekly: Vec<WeekSaves>,
}

/// A language and brand's Writer books over time: verses per book after each
/// save, and saves per author per week.
#[utoipa::path(get, path = "/v1/languages/{ietf}/progress",
    params(("ietf" = String, Path), ProgressQuery),
    responses((status = 200, body = LanguageProgress)))]
async fn language_progress(
    State(s): State<AppState>,
    Path(ietf): Path<String>,
    Query(q): Query<ProgressQuery>,
) -> Result<Json<LanguageProgress>, Failure> {
    let (store, scanner) = (s.store.clone(), s.scanner.clone());
    let (lang, resource) = (ietf.to_lowercase(), q.resource.to_lowercase());
    let (l, r) = (lang.clone(), resource.clone());
    let out = tokio::task::spawn_blocking(
        move || -> Result<(Vec<BookProgress>, Vec<WeekSaves>), String> {
            use rayon::prelude::*;
            let books =
                publishing::writer_books(&store.reader().map_err(|e| e.to_string())?, &l, &r)
                    .map_err(|e| e.to_string())?;
            let read: Vec<(BookProgress, Vec<(i64, String)>)> = books
                .par_iter()
                .filter_map(|b| {
                    let h = hub_history::read_writer_history(
                        &scanner.repo_path(&b.owner, &b.name),
                        &b.book,
                        &b.book,
                        "",
                    )
                    .ok()?;
                    let mut points: Vec<[i64; 2]> = Vec::new();
                    for (at, verses) in hub_history::progress(&h) {
                        if points.last().is_none_or(|p| p[1] != verses) {
                            points.push([at, verses]);
                        }
                    }
                    let saves = h
                        .saves
                        .iter()
                        .map(|s| (s.author.at, s.author.name.clone()))
                        .collect();
                    Some((
                        BookProgress {
                            book: b.book.clone(),
                            repo: format!("{}/{}", b.owner, b.name),
                            source_verses: b.source_verses,
                            verses: b.verses,
                            points,
                        },
                        saves,
                    ))
                })
                .collect();
            let mut weekly: BTreeMap<(i64, String, String), i64> = BTreeMap::new();
            for (bp, saves) in &read {
                for (at, author) in saves {
                    let week = at - at.rem_euclid(7 * 86_400);
                    *weekly
                        .entry((week, author.clone(), bp.book.clone()))
                        .or_default() += 1;
                }
            }
            let weekly = weekly
                .into_iter()
                .map(|((week, author, book), saves)| WeekSaves {
                    week,
                    author,
                    book,
                    saves,
                })
                .collect();
            Ok((read.into_iter().map(|(b, _)| b).collect(), weekly))
        },
    )
    .await
    .map_err(|e| Failure(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| Failure(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(LanguageProgress {
        language: lang,
        resource,
        books: out.0,
        weekly: out.1,
    }))
}
