//! hub-server: the Translation Hub's API, its soft poll, and (optionally) the
//! built web app, in one process.
//!
//!     hub-server              serve (configuration: see config.rs / AGENTS.md)
//!     hub-server --openapi    print the OpenAPI document and exit
//!     hub-server --health     exit 0 if the server on HUB_LISTEN answers (Docker's HEALTHCHECK)
//!
//! Observability is wide events: one JSON line per request (`event:"request"`),
//! per poll round (`event:"poll"`) and per catalog refresh (`event:"catalog"`),
//! each carrying its duration and everything needed to explain a slow one.

mod api;
mod config;
mod forgejo_api;
mod poll;
mod progress;
mod proof;
mod publish;
mod telemetry;

use std::sync::Arc;
use std::time::Instant;

use axum::extract::{MatchedPath, Request};
use axum::middleware::{self, Next};
use axum::response::Response;
use hub_ingest::Scanner;
use hub_store::Store;
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};
use tracing::{Instrument, field::Empty, info, info_span};

#[tokio::main]
async fn main() {
    if std::env::args().any(|a| a == "--openapi") {
        println!(
            "{}",
            api::openapi().to_pretty_json().expect("serialize OpenAPI")
        );
        return;
    }
    if std::env::args().any(|a| a == "--health") {
        std::process::exit(health().await);
    }
    let _guard = telemetry::init();
    let config = config::Config::from_env();
    info!(?config, "starting");

    let store = Arc::new(Store::open(&config.db).expect("open the hub's SQLite (HUB_DB)"));
    let scanner = Arc::new(Scanner::new(hub_ingest::Config {
        root: config.repo_root.clone(),
        base_url: config.base_url.clone(),
    }));
    assert!(
        config.repo_root.is_dir(),
        "HUB_REPO_ROOT {} is not a directory",
        config.repo_root.display()
    );

    // Bind before starting the poll, so a taken port fails fast.
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .expect("bind HUB_LISTEN");

    let poller = Arc::new(poll::Poller {
        config: config.clone(),
        store: store.clone(),
        scanner: scanner.clone(),
    });
    tokio::spawn(poller.clone().run_repos());
    tokio::spawn(poller.run_catalog());

    // Publishing is off unless configured, and refused outright for a host the
    // write guard will not accept (production WACS never is).
    let publish = config
        .publishing
        .as_ref()
        .and_then(|p| match forgejo_api::Forgejo::new(p) {
            Ok(forgejo) => {
                tracing::info!(forgejo = %p.forgejo_url, org = %p.official_org, "publishing on");
                Some(Arc::new(publish::Publisher {
                    config: p.clone(),
                    forgejo,
                }))
            }
            Err(e) => {
                tracing::error!(error = %e, "publishing off");
                None
            }
        });
    let mut app = api::router(api::AppState {
        store,
        scanner,
        publish,
    });
    if let Some(dir) = &config.web_dir {
        // The web app is a single page: unknown paths get index.html.
        let index = dir.join("index.html");
        app = app.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)));
    }
    let app = app
        .layer(middleware::from_fn(wide_event))
        .layer(CompressionLayer::new());

    info!(listen = %config.listen, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            info!("shutting down");
        })
        .await
        .expect("server");
}

/// GET /v1/status on our own listen address. The image has no curl, so the
/// binary checks itself.
async fn health() -> i32 {
    let mut addr = config::Config::from_env().listen;
    if addr.ip().is_unspecified() {
        addr.set_ip(std::net::Ipv4Addr::LOCALHOST.into());
    }
    let ok = reqwest::Client::new()
        .get(format!("http://{addr}/v1/status"))
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
        .is_ok_and(|r| r.status().is_success());
    if ok { 0 } else { 1 }
}

/// One wide event per request. Handlers add to the span (`db_ms`, `rows`);
/// the event at the end carries the span's fields with the outcome, so one
/// line answers "what was slow, for whom, and why".
async fn wide_event(req: Request, next: Next) -> Response {
    let started = Instant::now();
    // Everything read from the request is owned before the await: a borrow of
    // it would make this future non-Send (the body is not Sync).
    let (route, traceparent, page) = {
        let header = |name: &str| {
            req.headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned()
        };
        let route = req
            .extensions()
            .get::<MatchedPath>()
            .map_or("static".to_owned(), |p| p.as_str().to_owned());
        (route, header("traceparent"), header("x-hub-page"))
    };
    let span = info_span!(
        "request",
        method = %req.method(),
        route = %route,
        path = %req.uri().path(),
        // The sign-in callback's query is a one-time OAuth code; it stays out of the logs.
        query = %if route == "/v1/auth/callback" { "<redacted>" } else { req.uri().query().unwrap_or("") },
        traceparent = %traceparent,
        // Set by the web app: which page made the call.
        page = %page,
        db_ms = Empty,
        rows = Empty,
        // Time in the routes' own work off the async pool: reading and
        // replaying git history, and git pushes (progress, assemble, consolidate).
        work_ms = Empty,
        push_ms = Empty,
        // Calls to Forgejo's HTTP API during the request, and their total time.
        forgejo_calls = Empty,
        forgejo_ms = Empty,
    );
    telemetry::link_parent(&span, req.headers());
    let res = forgejo_api::UPSTREAM
        .scope(Default::default(), async {
            let res = next.run(req).instrument(span.clone()).await;
            let (calls, ms) = forgejo_api::UPSTREAM.with(|u| u.get());
            if calls > 0 {
                span.record("forgejo_calls", calls);
                span.record("forgejo_ms", ms);
            }
            res
        })
        .await;
    let bytes = res
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    span.in_scope(|| {
        info!(
            event = "request",
            status = res.status().as_u16(),
            duration_ms = started.elapsed().as_secs_f64() * 1000.0,
            bytes,
            "request"
        )
    });
    res
}
