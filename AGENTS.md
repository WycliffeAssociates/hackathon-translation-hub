# AGENTS.md — Translation Hub

Instructions for anyone (agent or human) who changes this repo. Read this first;
README.md is the overview, OUTSTANDING.md is what is left.

## What this is

A read-only service beside WACS (Forgejo). It reads Forgejo's bare repos off
disk and Forgejo's repo list (MariaDB, read-only), copies facts from the BIEL
public data API, keeps its own SQLite, and serves `/v1` JSON plus a thin web
app. Rust does the work; the web app only calls the API.

```
crates/
  manifest  repo-read  stitch  synthetic  checks  domain   ← pure pieces, no IO policy
  ingest     one repo → RepoScan; Forgejo's repo list (dump or MariaDB)
  proof      sous-chef proofreading over one repo's books
  biel       public data API client
  store      SQLite: migrations/, writes, read models (= API bodies)
  server     hub-server: /v1, soft poll, catalog refresh, wide-event logs
  hub-scan   CLI: the pipeline over a root → JSON or SQLite
web/         Vite + Solid + Effect; types generated from the OpenAPI
spikes/viewer  Sefer's editor forked as a read-only USFM viewer (the web app uses it)
scripts/     verify.py (end-to-end), latency.py (response times from logs)
```

## Hard rules

- **Never write** to Forgejo's repos, Forgejo's database, or `dev-data/`. Everything the hub owns is its SQLite and its logs. `scripts/verify.py` checks `dev-data/` was not written; keep it passing.
- **Public repos only.** The repo list comes from Forgejo's `repository` table (`is_private = 0`); the disk also holds private repos.
- **scripture-kitchen by git tag**, never by path (`Cargo.toml`, `crates/proof/Cargo.toml`, `spikes/viewer/package.json`). Bump all of them together, along with `KITCHEN_TAG` in `crates/checks` and `crates/proof`.
- **Sefer is read-only** (`../Sefer`): port from its committed HEAD and name the file and commit you ported from.
- No editing features in the hub: Sefer is the editor.
- Do not push, and do not run anything on the wacsdev server without asking.

## Run it

Prerequisites: Rust (stable), libgit2 1.9 on macOS via Homebrew (or build with
`--features vendored-libgit2`), Node 22 + pnpm for the web app, Python 3 for the
scripts. `dev-data/` (repositories + `sql/repository.sql.gz`) must exist.

```sh
# API + poll on dev-data (first poll ~11 s, catalog ~20 s; watch the logs)
HUB_LOG_FORMAT=pretty cargo run --release -p hub-server
#   → http://127.0.0.1:8787/v1/status   ·   /v1/openapi.json

# Web app (dev server proxies /v1 to the API; see web/README.md)
cd web && pnpm install && pnpm dev

# Everything in one process, as deployed: build the web app, then
HUB_WEB_DIR=web/dist cargo run --release -p hub-server   # → http://127.0.0.1:8787/

# Docker, on dev-data
docker compose up --build                                # → http://localhost:8787/
```

Configuration is environment variables, all optional in development:

| Variable | Default | Meaning |
|---|---|---|
| `HUB_REPO_ROOT` | `dev-data/repositories` | Forgejo's repository dir (read-only) |
| `HUB_FORGEJO_DUMP` | `dev-data/sql/repository.sql.gz` | dev: a mysqldump of `repository` |
| `HUB_FORGEJO_DB` | — | prod: `mysql://dashboard_ro:…@host/gitea` (build `--features mariadb`) |
| `HUB_DB` | `hub.sqlite` | the hub's SQLite |
| `HUB_LISTEN` | `127.0.0.1:8787` | |
| `HUB_POLL_SECS` | `300` | soft poll interval |
| `HUB_CATALOG_SECS` | `3600` | public data API refresh; `HUB_CATALOG_URL=off` to skip (offline) |
| `HUB_WEB_DIR` | — | serve the built web app at `/` |
| `HUB_LOG_DIR`, `HUB_LOG_KEEP` | —, `14` | JSON log files, rotated daily, kept N days |
| `HUB_LOG_FORMAT` | `json` | `pretty` for humans |
| `RUST_LOG` | hub crates `info` | standard filter |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | — | export spans over OTLP/HTTP (build `--features otel`) |

## Verify your work — required before you say it is done

Tests in isolation are not enough. Run the app and look at it.

1. **`cargo clippy --workspace --all-targets` and `cargo test --workspace`** — clean.
2. **`scripts/verify.py`** — builds and launches the real server on dev-data with a fresh SQLite and a free port, waits for the first poll and the catalog, drives every `/v1` route, asserts on what dev-data must give (en_ulb's 66 books, a synthetic repo's three files, 404s, BIEL conflicts), checks every request left a wide event, checks p95 per route against a budget (`--budget-ms`, default 250), checks `dev-data/` was not written, and stops only the process it started. Evidence (logs, every response body, `report.txt`) is in `target/verify/<timestamp>/`. `--no-catalog` works offline; `--base URL` drives a hub that is already running (e.g. in Docker).
3. **If you touched the web app:** `cd web && pnpm typecheck && pnpm build`, then the browser smoke (`web/README.md`), and **look at the screenshots** it saves.
4. **If you changed an API shape:** regenerate the contract (`cd web && pnpm gen`) and commit `web/openapi.json` with the change; the web typecheck then shows every caller you broke.
5. **If you touched performance-sensitive code** (the poll, a store query): read the numbers, don't guess. `scripts/latency.py target/verify/<ts>/logs/*.jsonl` prints p50/p95/max per route, the slowest requests, and each poll round's breakdown.

The `/verify-hub` skill (`.claude/skills/verify-hub/`) wraps this as launch → doctor → drive → evidence → cleanup, with one file per feature under `features/` saying how to reach it and what its end state looks like.

## Observability: wide events

Each unit of work writes **one JSON line** that has everything needed to explain it:

- `event:"request"`: method, route, path, query, status, `duration_ms`, `db_ms`, `bytes`, `traceparent`, `page` (the web app sends `x-hub-page`).
- `event:"poll"`: repos public/removed/rescanned, `scan_ms`, proofread count, `proof_ms`, the slowest repo, `duration_ms`.
- `event:"catalog"`: languages, catalog rows, `duration_ms`.

They go to stdout, and to `HUB_LOG_DIR/hub.YYYY-MM-DD.jsonl` (rotated daily) when that is set. With `--features otel` and `OTEL_EXPORTER_OTLP_ENDPOINT`, the same spans also go to a collector, and requests join the browser's trace through `traceparent`. To add context to a request's event, record a field on the current span (declare it `Empty` in `wide_event` in `crates/server/src/main.rs`) rather than logging a separate line.

## Tests: few, each on purpose

Rust is test-light by choice. Every test has a comment saying **why it exists** (a rule ported from Sefer or workTracker that is easy to get subtly wrong, an offset convention the UI depends on, a hand-written parser). Do not add tests per route or per handler: `scripts/verify.py` covers the running system. Goldens that read `dev-data` run only when `REPO_ROOT=$PWD/dev-data/repositories` is set.

## Code style

Match the surrounding code: module doc comments say what a crate does and where it came from; comments explain why, next to what they describe. Edition 2024; `cargo fmt`. SQL lives in `crates/store` (migrations are append-only: add `migrations/000N_*.sql` and the line in `MIGRATIONS`, never edit an applied one).

## Deploy

`Dockerfile` builds one image (web + server, `--features mariadb,otel,vendored-libgit2`) that runs as uid 1000 (Forgejo's) and checks its own health (`hub-server --health`). On the wacs2 host, mount Forgejo's repositories read-only with the SELinux flag (`/var/lib/gitea/git/repositories:/repos:ro,z`), set `HUB_FORGEJO_DB` to the `dashboard_ro` account, and keep `/data` and `/logs` on volumes. `docker-compose.yml` is the dev-data version of the same.
