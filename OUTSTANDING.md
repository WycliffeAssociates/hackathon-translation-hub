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
| Synthetic repos for Writer projects, with a "not a real repo" notice | ✅ verified (API + browser smoke) | `crates/synthetic`; tree/file serve the synthetic files, `?source=true` the real ones |
| Checks and verse counts against the synthetic book | ✅ verified | lint + coverage run on the stitched book |
| Chapter coverage | ✅ verified | `coverage` per book with per-chapter gaps. Uses onion's TOC (the engine sous-chef also reads), not a sous-chef pass |
| Backend observability: JSON logs, rotation | ✅ verified | wide events per request/poll/catalog; `HUB_LOG_DIR` daily rotation, `HUB_LOG_KEEP` |
| Trace response times | ✅ verified | `duration_ms`/`db_ms` per request; `scripts/latency.py`; verify.py enforces a p95 budget |
| OTel-ish export | ✅ verified locally | `--features otel` + `OTEL_EXPORTER_OTLP_ENDPOINT`. Checked against Jaeger 1.62 (`docker run -p 4318:4318 -p 16686:16686 jaegertracing/all-in-one:1.62.0`): the request span joined the caller's `traceparent` trace, with `route` and `db_ms` |
| Thin front end (Solid + Effect, calls the API) | ✅ verified | `web/`: typecheck, build, `tools/smoke.mjs` 12/12 in Chromium against the real server. ~59 KB gzip, viewer lazy (~338 KB, mostly the onion wasm) |
| Docker to deploy | ✅ verified locally | `Dockerfile` (127 MB, uid 1000, healthy via `hub-server --health`), `docker-compose.yml`. `verify.py --base` passes against the container on dev-data. On macOS the first scan takes 35 s instead of 4.4 s (Docker Desktop file sharing); a native Linux mount should not. Not yet run on the wacs2 host |
| Clear run + verify instructions | ✅ | `AGENTS.md`, `.claude/skills/verify-hub/`, `scripts/verify.py` |

## Rules that look noisy (found with /checks)

- `delimiter-surplus` (lint, info): 35k findings in 414 repos. This is formatter-level (doubled spaces); it could leave the summary counts.
- `missing-paragraph` (lint, warning): fires in 952 of 983 Writer books, once per chapter. Writer never writes `\p`, and Sefer's stitch template adds none. Fix it in Sefer's template (so the hub's stitch stays byte-identical) or count it apart for synthetic books. Will to decide.

## Known gaps in what is built

- **MariaDB reader: verified against stock Forgejo, not yet against wacs2.** `dev/forgejo/check.sh` runs Forgejo 11.0.16 (rootless) on MariaDB 10.11 and reads it as `dashboard_ro` with production's grants. It checks that the public repo is indexed, the private one is not, a push lands within a poll, and a repo made private disappears. wacs2 itself still needs a run (ask first).
- **A push bumps `repository.updated_unix`: confirmed** on Forgejo 11.0.16 (`check.sh`, step 1). The poll still also compares HEAD on disk, which covers writes that skip Forgejo.
- **Sous-chef:**
  - No source-compared lanes (length, presence, source copy); they need `analyze_paired` with the WA-Catalog source.
  - Thai `th_ulb` flags every U+200B (3,221 findings); this is likely a kitchen rule to revisit.
  - A one-book Writer corpus gives noisy `Rarity` findings.
  - The proof pass does not register `\s5`.
  - Every finding is counted per book and check; rows are stored up to 200 per book and check.
  - `MisplacedFormat` produces 971k of 1.05M findings, from writing systems that use format characters on purpose (Khmer and Lao U+200B, Persian U+200C, Hebrew U+2060). The rule should judge them against the corpus, as the convention rules do. That is a kitchen change.
- **An idle poll costs ~1.3 s** on dev-data (re-reading the 32k-row dump plus HEAD on 1,441 repos); with MariaDB it is one SELECT. Measure on production's ~30k repos.
- **The first poll after start rescans everything** (~11 s on dev-data with proofreading; `wa-catalog/hbo_uhb` alone takes ~5 s). Production has ~30k repos: measure the first poll there, and consider proofreading lazily or with a time budget.
- **`book_history` key** is (repo, book, time): two files with the same `\id` in one repo collapse to one history row.
- **scripture-kitchen v0.1.10** is only local. Push the tag, then bump every pin (see AGENTS.md).
- **Linux builds** need `--features vendored-libgit2` (the Dockerfile does this) or a system libgit2 1.9.
- **History is from the day the hub starts.** There is no backfill from git history yet (the proto's "over time" idea).

- **Web:**
  - Book history is per book, so the sparkline sits in the open book's header. A per-repo history route would allow a column in the books table.
  - Audio, image and raw links are plain URLs, so those requests carry no `traceparent`.
  - The app uses Solid 1.9 (the router targets it), while Sefer is on Solid 2 rc.

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
