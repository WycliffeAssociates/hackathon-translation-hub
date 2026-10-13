# Outstanding

What is built, what is left, and what is someone else's call. Update this file
in the same commit as the work. "Verified" means `scripts/verify.py` (or the
named check) exercised it on dev-data; "unverified" means it compiles and is
reasoned about but has not run against the real thing.

## Will's scope (2026-10-12): agreed, build it

| Item | State | Where / notes |
|---|---|---|
| Text file tree of a repo | ✅ verified | `GET /v1/repos/{o}/{n}/tree`, `/file/{path}`; web repo page |
| Linting (scripture-kitchen) | ✅ verified | `crates/checks`; summary per book keyed by content; full diagnostics drawn by the viewer's wasm engine |
| Sous-chef proofreading | ✅ verified | `crates/proof`, run by the poll, `GET …/proof`. Gaps below |
| In a database | ✅ verified | `crates/store`, SQLite, `migrations/0001_init.sql` |
| Reading local files / querying from local files | ✅ verified | bare repos via `crates/repo-read`; Forgejo's repo list from the dump (dev) |
| Alternate names, regions from the public data API | ✅ verified | `crates/biel` → `language` table; search matches alternate names |
| Is it Primary / shown on BIEL | ✅ verified | `catalog_content` table; `/v1/languages/{ietf}` `books[]` (Official pick, state); `/v1/biel/conflicts` (320 books / 33 languages = workTracker) |
| Source texts resolved against WA-Catalog, as Sefer does | ✅ verified | `wa-catalog/<lang>_<resource>` (Sefer `relationships.ts`). Only the first declared source that exists on disk is used |
| Synthetic repos for Writer projects, with a "not a real repo" notice | ✅ API verified · web in progress | `crates/synthetic`; tree/file serve the synthetic files, `?source=true` the real ones |
| Checks and verse counts against the synthetic book | ✅ verified | lint + coverage run on the stitched book |
| Chapter coverage | ✅ verified | `coverage` per book with per-chapter gaps. Uses onion's TOC (the engine sous-chef also reads), not a sous-chef pass |
| Backend observability: JSON logs, rotation | ✅ verified | wide events per request/poll/catalog; `HUB_LOG_DIR` daily rotation, `HUB_LOG_KEEP` |
| Trace response times | ✅ verified | `duration_ms`/`db_ms` per request; `scripts/latency.py`; verify.py enforces a p95 budget |
| OTel-ish export | ✅ verified locally | `--features otel` + `OTEL_EXPORTER_OTLP_ENDPOINT`. Checked against Jaeger 1.62 (`docker run -p 4318:4318 -p 16686:16686 jaegertracing/all-in-one:1.62.0`): the request span joined the caller's `traceparent` trace, with `route` and `db_ms` |
| Thin front end (Solid + Effect, calls the API) | 🚧 in progress | `web/` |
| Docker to deploy | ⚠️ written, image build not yet run | `Dockerfile`, `docker-compose.yml` |
| Clear run + verify instructions | ✅ | `AGENTS.md`, `.claude/skills/verify-hub/`, `scripts/verify.py` |

## Known gaps in what is built

- **MariaDB reader is unverified.** `--features mariadb`, one SELECT on `repository` (the `dashboard_ro` grants). Needs a run against wacs2's DB (ask before touching the server).
- **Forgejo bumping `updated_unix` on push is unverified** (HANDOFF.md). The poll does not depend on it: it also compares HEAD on disk for every repo each round.
- **Sous-chef:**
  - No source-compared lanes (length, presence, source copy); they need `analyze_paired` with the WA-Catalog source.
  - Thai `th_ulb` flags every U+200B (3,221 findings); this is likely a kitchen rule to revisit.
  - A one-book Writer corpus gives noisy `Rarity` findings.
  - The proof pass does not register `\s5`.
  - At most 500 findings per code are stored.
- **An idle poll costs ~1.3 s** on dev-data (re-reading the 32k-row dump plus HEAD on 1,441 repos); with MariaDB it is one SELECT. Measure on production's ~30k repos.
- **The first poll after start rescans everything** (~11 s on dev-data with proofreading; `wa-catalog/hbo_uhb` alone takes ~5 s). Production has ~30k repos: measure the first poll there, and consider proofreading lazily or with a time budget.
- **`book_history` key** is (repo, book, time): two files with the same `\id` in one repo collapse to one history row.
- **scripture-kitchen v0.1.10** is only local. Push the tag, then bump every pin (see AGENTS.md).
- **Linux builds** need `--features vendored-libgit2` (the Dockerfile does this) or a system libgit2 1.9.
- **History is from the day the hub starts.** There is no backfill from git history yet (the proto's "over time" idea).

## Not built yet, but agreed earlier

- WACS sign-in (needed only when something is not public; today nothing needs it).
- Writes through Forgejo's API: none exist. A person overriding the Official pick (`picked_by = person`) has no table yet.
- Burn-down and velocity charts. `book_history` collects the data from now on; the web app draws a sparkline per book.

## Out of scope here: the coworker's proposals, left for him

- Publications, releases, bulletins (release notes).
- Audio beyond playing a file from the tree.
- The language → version → track grid as a page. The API returns `books[]` picks per language; the grid's shape is his call.
- "Bring in chapters" / the assembled-version view (parked as a funky model).

## Open decisions (decide together; this repo is a proposal)

- The SQLite schema (`crates/store/migrations/0001_init.sql`) and the `/v1` shapes (`web/openapi.json`).
- The web stack. `web/` is thin by design, so it can be replaced by server-rendered pages without touching `/v1`.
- Domain naming (his DDD terms) and provenance granularity.
