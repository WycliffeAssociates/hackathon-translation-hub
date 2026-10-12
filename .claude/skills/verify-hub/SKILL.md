---
name: verify-hub
description: Launch the Translation Hub on dev-data, check it is healthy, drive its API and web pages like a user, and keep evidence. Use before saying any change to crates/, web/ or the Dockerfile is done, or when asked to check that the hub works.
---

# Verify the Translation Hub

The hub is an HTTP API (`/v1`) with a soft poll behind it and a thin web app in
front. "Works" means: the real binary, on real repos, answers with the right
data, fast, leaves a wide event per request, and writes nothing it does not own.

## Launch

From the repo root:

```sh
scripts/verify.py --keep            # build, launch on a free port, wait, drive, report; leave it up
```

It prints `launched pid N on http://127.0.0.1:PORT`. The server is ready when
`GET /v1/status` answers and `state.polled_unix` is set (first poll ~11 s);
catalog-backed data (languages, BIEL) is ready when `languages > 0` (~20 s).
Each launch gets its own SQLite and logs under `target/verify/<timestamp>/`, so
runs can sit side by side. Offline: add `--no-catalog`.

To launch by hand instead (e.g. to iterate on the web app):

```sh
HUB_DB=target/dev.sqlite HUB_LOG_DIR=target/dev-logs HUB_WEB_DIR=web/dist \
  cargo run --release -p hub-server &      # note the PID; default port 8787
```

Docker: `docker compose up --build -d`, then drive with `scripts/verify.py --base http://localhost:8787`.

## Doctor

Read-only, before driving: is this instance worth testing?

```sh
curl -s $BASE/v1/status        # scanned == repos, synthetic > 0, proofread > 0, state.polled_unix set
```

- `scanned < repos` for long after start → read the `event:"poll"` line; `scan_failures` names the count.
- `languages: 0` after a minute → the catalog fetch failed (`catalog refresh failed` in the log) or `HUB_CATALOG_URL=off`.
- `proofread: 0` → the poll did not reach the proof pass; check for a panic in stdout.

## Drive

`scripts/verify.py` drives every route with assertions (read it; it is the
executable form of this section). For the web app, `web/tools/smoke.mjs <base>`
drives the pages in headless Chromium and saves screenshots (see web/README.md).
For one feature, follow its file in `features/`.

Always send the headers the web app sends, so your requests are findable:
`x-hub-page: verify` and a `traceparent`.

## Evidence

- `target/verify/<ts>/report.txt`: every check, ok or FAIL, with the numbers.
- `target/verify/<ts>/responses/`: each response body, to diff against a previous run.
- `target/verify/<ts>/logs/hub.*.jsonl`: the wide events. `scripts/latency.py <those files>` gives p50/p95 per route and the poll breakdown. Quote these numbers when claiming something is fast.
- `web/smoke-shots/*.png`: look at them; a page that renders an error box still returns 200.
- Side effects: `verify.py` checks nothing under `dev-data/` changed. For Docker, the repo mount is `:ro`.

Do not weaken an assertion to make a run pass. If dev-data changed (a new
dump), update the expected numbers and say so in the commit.

## Cleanup

`verify.py` stops the PID it launched (unless `--keep`). For a server you
started by hand, `kill <the PID you noted>`; never `pkill hub-server`, another
session may be running one. `docker compose down` (add `-v` only to drop the
SQLite volume). Keep `target/verify/` until the work is merged; it is gitignored.

## Helpers

- `scripts/verify.py [--keep] [--base URL] [--no-catalog] [--budget-ms N]`: end to end; exit 0 = pass.
- `scripts/latency.py LOG.jsonl…`: response times from wide events (also works on production logs).
- `cargo run -p hub-scan -- dev-data/repositories --out scan.json`: the pipeline without the server, for checking a parser or the stitch.
- `cargo run -q -p hub-server -- --openapi`: the API contract.

## Feature map

See `features/README.md`. One file per user-facing feature: how to reach it,
how to drive it, and what its end state looks like.
