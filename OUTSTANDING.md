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

- **Stored Sous rows: cap of 200 per book and check, or 25?** Will to decide. On dev-data the rows are 43 MB of a 64 MB database (112k rows); at about 32k repos that is roughly 1 GB. The per-book counts (`finding_count`) are 0.6 MB and stay either way. With 25, the page would say "showing 25 of N" and rows would be about 100 MB in production. The change is `ROWS_PER_BOOK_CODE` in `crates/server/src/proof.rs`, then one re-proofread. The other option is to store no rows and re-run Sous for the repo on click (~0.2 s, Hebrew 5 s).

## Decided 2026-10-12 (night)

- The SQLite and the logs live in host directories on disk (`./var/data`, `./var/logs` in compose), not named volumes.
- Sous Chef's source comparisons are in scope for the POC: done. 983 of 1,136 repos are compared against their declared source (LengthProportionality, Presence, SourceCopy), at about the same proof time.
- Fixes in scripture-kitchen and Sefer are listed in TODO.md for Will.

- **`SourceCopy` between related languages.** Will to decide. It runs only when the repo's language differs from its source's (Sous ships it off because a same-language source matches everywhere: en_udb vs en_ulb gives 59k). Related languages still flood it: tkg vs plt_ulb 1,149, bou vs sw_ulb 1,134. Hub-wide it is 16,465 findings in 267 repos. Options: switch it off (`source_copy()` in `crates/server/src/proof.rs`), or raise Sous's `source_copy_min_run` from 3 (not exposed yet).

## Known gaps in what is built

- **MariaDB reader: verified against stock Forgejo, not yet against wacs2.** `dev/forgejo/check.sh` runs Forgejo 15.0.10 (rootless; WACS dev runs 15.0.6) on MariaDB 10.11 and reads it as `dashboard_ro` with production's grants. It checks that the public repo is indexed, the private one is not, a push lands within a poll, and a repo made private disappears. wacs2 itself still needs a run (ask first).
- **A push bumps `repository.updated_unix`: confirmed** on Forgejo 11.0.16 and 15.0.10 (`check.sh`, step 1). On 15 it happens from a queue a few seconds after the push returns. The poll still also compares HEAD on disk, which covers writes that skip Forgejo.
- **Sous-chef:**
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

## Publishing (proposal, built 2026-10-14 against WACS-local; revert freely)

Will's model after reading Joe's repo: a published translation is a **pointer**,
one official repo per language and brand plus a release tag in it, so two
`matthew.usfm` files cannot both be official. Joe's org/name template and pulls,
with Will's choices: run on WACS dev, read off disk, write only through
Forgejo's API, roles from a Forgejo team.

Built, and driven end to end in a browser against WACS-local (`dev/wacs/`):

- **Sign in** with Forgejo OAuth (`/v1/auth/login` → Forgejo → `/v1/auth/callback`); sessions in SQLite (migration 0004), a cookie, 7 days. `/v1/me`; the header shows who you are.
- **Who may publish**: members of the official org's `publishers` team, read with the service account at sign-in. Others see the panel and get 403 from the routes.
- **Writes**: only through Forgejo's API, as a service account (`hub-bot`), naming the person; only to `HUB_OFFICIAL_ORG`; only on a host in `HUB_WRITE_HOSTS`, never production WACS (`crates/server/src/forgejo_api.rs`).
- **Adopt**: fork a consolidated repo into `TextTranslation/{ietf}_{resource}` (history kept; Forgejo shows "forked from"). Releases are switched on after the fork (Forgejo forks get only code and pulls).
- **Pull**: one book (a Writer project's book stitched) committed into the official repo, created if missing, with `Source:`, `Source-path:`, `Pulled-by:` trailers. The same commit starts an RC `manifest.yaml` or adds the book to its `projects` (Joe's spec), so the hub and BIEL read the language and books. A manifest that is not YAML is left alone.
- **Publish**: a release tag at the official repo's head (`v1.0`, then `v1.1`).
- **Consolidate from Writer** (a language with no official repo yet): an assembling step lists every Writer project per book with its catalog status (Primary, on BIEL), chapters · verses, coverage of the source, last updated, saves, and a progress line (verses present over time, scaled to the source). The default per book is Primary and shown on BIEL, then Primary, then the newest Active, then the most verses; a person can pick another or leave a book out. Creating replays **every Writer save** into the new repo (`crates/history`, ported from Sefer's `writerHistory.ts` at `50aa9cd`): each save that changed a book is a commit with its own author, date and zone, titled with what it changed (`ROM 5:1–11; titles`) and `Writer-Message`/`Writer-Source` trailers, books interleaved by author time; then one commit adds the RC manifest with every book's source commit. Built locally (rayon across books, libgit2 in memory, one pack) and pushed as the service account. Tawara (twx reg): 9 books, 3,959 commits, built in 2.1 s (the first version wrote loose objects: 175 s).
- The repo page's **Official** panel shows the target, its releases, the buttons for a publisher, and the hub's log of writes (`action` table).

Gaps, in the order they would bite:

- **Running it on wacsdev** (ask first; nothing has run there): create `dashboard_ro` from `dev/db-init/10-readonly-user.sql` with a real password, e.g. `PW=$(openssl rand -hex 16); sed "s/'dev'/'$PW'/" dev/db-init/10-readonly-user.sql | ssh <wacsdev-host> 'docker exec -i wacs2-db-1 sh -c "exec mariadb -uroot -p\"\$MARIADB_ROOT_PASSWORD\""'`; then what `dev/wacs/publishing.py` does locally (a `TextTranslation` org with `publishers`, a service account with a token, an OAuth app); then the hub beside `wacs2-gitea-1` with the `wacs2_gitea_data` volume mounted `:ro` on the compose network.
- **Pushing to wacsdev over https**: git2 is built without its `https` feature (the workspace sets `default-features = false`). The macOS libgit2 has TLS; the Docker image's vendored libgit2 needs `git2/https` (OpenSSL) before consolidate can push to `https://content.wacsdev.org`.
- **Consolidate runs in the request** (2 s for Tawara). A language with many long-lived Writer books may take longer; move it to a job with progress when it does.
- **Port and BIEL** still mark Primary by hand. The pointer could feed them later; not touched.
- **Editions** (`_e2`), overwrite-with-confirmation for a pull that replaces a book: not built. A pull that replaces a book says so in the commit and the log, but does not ask twice.
- **CSRF**: writes rely on `SameSite=Lax` and a JSON body. Enough for a POC; add a token before anyone else's browser can reach it.
- Team membership is read at sign-in; removal takes effect at the next sign-in.

## Next (2026-10-16, demo day)

- **The demo tour:** the machine is built (`web/src/tour/`, `?tour=demo`). Write the real stops with Will, who talks through the what and the why. Add a visible way to start it.
- **Rebuild on wacsdev** after today's changes: `git pull`, the build with `--build-arg HUB_UID=1001 --build-arg HUB_GID=1002`, `export TAG=$(git rev-parse --short HEAD)`, then `compose up -d` (DEPLOY-WACSDEV.md). The database volume keeps its scans.
- **Poll by Forgejo's update time (bookmarked):** the poll reads every repo's HEAD from disk each round (about 30 s idle at wacsdev's 32,575 repos), to catch a push Forgejo did not record. Nobody edits on the server, so trust `repository.updated_unix` from the database and read HEAD only where it moved. Idle polls become milliseconds and the interval can drop to seconds. As the safety net (Will): when a page opens a repo whose HEAD on disk differs from the stored commit, rescan that one repo then.
- **Write scans in batches as the poll goes:** today a round scans every repo in memory and writes them all at the end, so a first poll shows "scanned 0" for its whole run (about 15–30 minutes at 32,575 repos) and holds every scan in memory. Write, say, every 500, so `/status` counts up and memory stays flat.

## Integrating with Joe's version (2026-10-15; built against WACS-local, revert freely)

Will clicked through Joe's hub (cloned to `../translation-hub`, run in Docker on :8080 in fixture mode) and asked for these; each is built, tried in a browser on WACS-local, and committed:

- **Sources by book** (`/languages/{ietf}/sources?resource=`, and the Sources tab of any repo with a language and brand). Every copy of every book (Writer projects, consolidated repos, the official one) grouped into **versions by their words**: each copy's verses are fingerprinted (`hub_domain::fingerprint`: markers and spacing dropped, headings left out), copies with the same words in every verse are one version, and each version says how it differs from the pick ("1,060 changed", "35 fewer"). The official copy is compared with the copy it follows: `current`, `behind` (that copy moved on), `edited` (the official copy changed), `unlinked`. Publishers can set a copy aside (one book or every book; migration 0006), take one repo for all its books, pull a pick, or create the official repo from the picks (Writer picks consolidated with history, the rest pulled). 100–450 ms for 100–140 copies. Joe's link / ignore / pull map onto pick (follow), set aside, pull.
- **What official books follow** is the hub's own pointer (`official_link`, migration 0007), written by pull and consolidate; **no `.upstreams.json` is committed** to official repos (Will, 2026-10-15). Every pull commit still names its source in a `Source:` trailer; a repo Sefer consolidated is still read from its `.upstreams.json`.
- **Progress against gateway sources** at the top of the sources page: a tab per source (the three the copies declare most, plus pins kept in the browser), a strip of the source's books shaded in one hue by the share of its verses present, the overall share, and Writer saves per week over a year.
- **The repo page in tabs**: Files, Checks (findings, the text, and the source text as a second pane that follows the chapter and lights the picked finding's verse), Sources, Releases, History (`GET /v1/repos/{o}/{n}/commits`, trailers pulled out). `?tab=` in the URL.
- **Releases first class**: on an official repo, what changed since the newest tag book by book in chapters and verses (`GET /v1/repos/{o}/{n}/unreleased`), and a new release whose **bulletin** can carry translations, committed as `bulletins/{tag}/{language}.md` before the tag. Translating a bulletin in the browser with NLLB-200 (transformers.js v4, opt-in ~600 MB download, CC-BY-NC weights, gateway languages only) is an idea, not built.
- **Your work** (`/welcome`): languages picked with one multi-select (migration 0008), each with its resources; repos owned by the person or their orgs (Forgejo lists public memberships only); repos starred on WACS (Forgejo's own pin; edit permission alone would list everything for an admin).
- **Open in Sefer** beside Open in WACS: a bare r-link to the r-links dev preview.
- **Quieter**: PRODUCT.md (who, why, how it should feel), neutral surfaces, one type family, status as a dot and a word, zero counts hidden.
- dev-data grew to 3,542 repos (2,889 Writer projects): every language with all 66 books, and the 14 with the most duplicate copies (Swati, Galician, Oxchuc Tzeltal with 6 owners, …), for the by-book cases.

Second pass the same day (Will, walking Oxchuc Tzeltal and pt-br): **published** is a pointer per language and resource (migration 0009), either the official org's repo or an existing repo blessed as is; the language page leads with it; sources compare every copy with the published copy and say what each published book **tracks**, with a verse diff; a distinct text is a **variant** (version is kept for releases); Releases on a repo that is not published offers "make this the published one" and "copy it into the official org" (what Adopt was). Tracking means watched and reported, not pulled automatically (see below).

Still open from this round: who may create the official repo stays "the robot writes, the publishers team decides" (Will agreed; site admins can always write); private org memberships do not count toward Your work; revisortseltal's Tzeltal repos (the catalog's Primary for several books) were not in the pull, so their links 404 until they are pulled; whether a tracked book may be pulled automatically when the published copy has not been edited (Will to decide; the hub only reports today).

## Charts: candidates to keep or cut (`/viz`, 2026-10-14)

Twelve charts on real data, Observable Plot 0.6.17, for Will to cut. The page
says the question each answers. A: a language over time (1 burn-up, 2 each book
toward its source, 3 who worked when, 4 contributor × book), from
`/v1/languages/{ietf}/progress?resource=` (Writer history via `crates/history`).
B: a book against its source (5 verse-length scatter with flags, 6 ratio along
the book), read in the browser. C: a repo (7 coverage book × chapter, 8 findings
book × check). D: WACS (9 which checks are noise, 10 how much is alive, 11 BIEL
shown twice by language, 12 a language's book × version state). The choices in
each card's picker are fixed lists for the demo.

Clicking opens what was clicked (Plot's pointer value, no other library):
1–2 a book's Writer project, 5–6 the verse in the viewer (`?v=31:50`, lit,
with the length check open in the rail when it was flagged), 7 the book at that
chapter, 8 the book with that check, 11 the language, 12 the Official repo.
TanStack Charts (1.1) was considered for this: its scales are thinner (log,
symlog and time need extra modules) and it has no Solid adapter; what it adds
that Plot lacks is animation between states (a picker change). Revisit if that
is wanted.

## Markdown and TSV notes (2026-10-14)

`.md` and `.tsv` files open in Preview (the server's HTML) with a Raw switch.
`crates/markdown` renders CommonMark with GitHub's extras (pulldown-cmark) and
sanitises the result (ammonia): repo markdown is user content. Links stay in
the hub: relative paths resolve in the repo, `rc://` resource links (and bare
`[[rc://…]]`, titled from the target's `title.md` or first heading) resolve
to the repo that holds them (this owner first, then WA-Catalog, then newest;
`ta` also looks for `_tm`), images load from the repo, web links open a tab.
TSV notes render as a table (2,000 rows at most). en_tw `god.md`, 38 links,
renders in 15 ms. Not done: a cache by blob (not needed at that speed);
`rc://` links to a Bible (`book` kind) open the repo, not the verse.

## Demo tour (2026-10-15): the machine is built, the stops are placeholders

`web/src/tour/`, ported from Sefer's tour machine (branch `r-links`, 95fedfd) without the workflow kind or the tours list. One outlet in the layout; `?tour=demo` starts it, `&step=<id or n>` jumps in. Each step: `go` (navigate, then any DOM effect), `ready`, then a ring and card, with an optional "more" (one native `<dialog>`, any JSX). A table of contents on the card jumps to any step; → / ← / Escape (pause); a reload comes back paused at the same step. Fails open like Sefer's. `tours/demo.tsx` has three placeholder stops that prove it on real screens; the real stops and their why wait on Will.

## Performance notes (2026-10-14, profiled with samply)

- Proofreading: books parse in parallel, and findings map to raw bytes by binary search instead of galley's whole-mask walk (`KITCHEN-RFC.md`). hbo_uhb 4 s → 237 ms, km_ulb (301k findings) 436 ms; output byte-identical on en_ulb, hbo_uhb, km_ulb, ru_ulb. A full proof round on dev-data: about 8.3 s → 5–7 s on a loaded laptop.
- What is left in the noisiest repos is sous-core's own analysis (~65%) and the hub converting every finding (~20%, ~100 ms for 301k) although it stores 200 per book and check. Sefer's pattern is to count from the packed rows and convert only what is shown; worth it if the conversion ever shows up again.
- The scan lints a repo's books in parallel; repos were already parallel, so the scan is unchanged (~1.4 s standalone, identical output). The poll's `proof_slowest_ms` is wall time while sharing the pool with every other repo, so it names a repo that waited, not one that is expensive: lo_bible read 3.4 s there and takes 300 ms alone.

## Not built yet, but agreed earlier

- A person overriding the Official pick (`picked_by = person`) has no table yet.
- Burn-down and velocity charts. `book_history` collects the data from now on; the web app draws a sparkline per book.

## Out of scope here: the coworker's proposals, left for him

- Publications and bulletins (release notes). Releases as tags are now proposed above.
- Audio beyond playing a file from the tree.
- The language → version → track grid as a page. The API returns `books[]` picks per language; the grid's shape is his call.
- "Bring in chapters" / the assembled-version view (parked as a funky model).

## Open decisions (decide together; this repo is a proposal)

- The SQLite schema (`crates/store/migrations/0001_init.sql`) and the `/v1` shapes (`web/openapi.json`).
- The web stack. `web/` is thin by design, so it can be replaced by server-rendered pages without touching `/v1`.
- Domain naming (his DDD terms) and provenance granularity.
