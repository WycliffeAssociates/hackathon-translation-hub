# Start here

The Translation Hub proof of concept: a read-only service beside WACS
(Forgejo) that shows Bible translation work by language, version and book,
with lint, proofreading (Sous Chef), verse coverage against the source text,
BIEL catalog status, and Writer projects shown as consolidated "synthetic"
repos. A Rust API with SQLite, plus a thin web app.

Everything here is a proposal. Rewrite any piece you like. The `/v1` API
(`web/openapi.json`) is the seam: the web app only calls it, so either side
can be replaced alone.

## If you are an agent

Read, in order: this file, `AGENTS.md` (how to run, verify, and the rules),
`OUTSTANDING.md` (what is built, what is left, and what is undecided), then
`TODO.md` (fixes that belong in other repos). Run `scripts/verify.py` before
you change anything, so you know the baseline passes on this machine. Do not
say a change is done until the checks in AGENTS.md "Verify your work" pass.

A good first prompt:

> Read START_HERE.md and AGENTS.md. Get the hub running on this machine,
> run scripts/verify.py and the web smoke, show me the app, and tell me what
> OUTSTANDING.md says is undecided.

## Get it running (about 10 minutes, most of it the first build)

You need: Rust (stable, via rustup), Node 22 and pnpm, Python 3, git, and on
macOS `brew install libgit2` (or add `--features vendored-libgit2` to cargo
commands; that needs cmake). Docker is optional.

```sh
# 1. The data. dev-data/ is in the zip (2.8 GB: 1,441 real WACS repos plus
#    a dump of Forgejo's repository table). It is read-only; never edit it.
ls dev-data/repositories dev-data/sql

# 2. The web app's dependencies (the viewer is built from spikes/viewer)
(cd spikes/viewer && pnpm install --ignore-workspace)
(cd web && pnpm install && pnpm build)

# 3. Check everything end to end: builds the server, runs it on dev-data,
#    drives every API route, checks the logs and response times.
python3 scripts/verify.py                # → PASS, evidence in target/verify/<time>/

# 4. Use it
HUB_WEB_DIR=web/dist HUB_LOG_FORMAT=pretty cargo run --release -p hub-server
#    → http://127.0.0.1:8787   (first start: about 15 s scanning, about 20 s
#      fetching the BIEL catalog; later starts reuse hub.sqlite)
```

`var/data/hub.sqlite` in the zip is a database that is already filled. Point
`HUB_DB=var/data/hub.sqlite` at it to skip the first scan. If it is missing
or old, the hub rebuilds it.

With Docker instead: `docker compose up -d db && docker compose up --build hub`
gives the same thing at http://localhost:8787, with MariaDB standing in for
Forgejo's database.

## What to look at

- `/`: recent repos, like Forgejo's explore page. Use the chips to filter.
- `/repos/wa-catalog/en_ulb`: the file tree, the viewer (Regular and USFM
  modes, chapter picker), lint, coverage, and Findings (every check sorted by
  count; expand it to see books, then open one).
- `/repos/embracekenya/ida-x-tiriki_2th_text_reg`: a Writer project shown as a
  synthetic repo. Note the "Not a real repo" notice.
- `/languages/ceb`: catalog facts, and which repo is Official for each book.
- `/biel`: books that BIEL shows twice.
- `/checks`: which rules fire across all repos (the noisy ones are in TODO.md).
- `/v1/openapi.json`: the API.

## Where things are

| Path | What |
|---|---|
| `AGENTS.md` | how to run, verify, observe, deploy; the hard rules |
| `OUTSTANDING.md` | scope: verified, unverified, in progress, undecided |
| `TODO.md` | fixes for scripture-kitchen and Sefer |
| `crates/` | the Rust side (README.md has a table of crates) |
| `web/` | the web app (`web/README.md`) |
| `scripts/verify.py`, `scripts/latency.py` | end-to-end check; response times from the logs |
| `dev/forgejo/check.sh` | the hub against stock Forgejo 11 in Docker |
| `proto/` | the first clickable wireframe (no build; historical) |
| `HANDOFF.md`, `plans/` | the first session's brief and plans (historical) |

## Will: picking this up tomorrow

1. `git log --oneline` on `hub-core` shows everything built tonight. Nothing is pushed.
2. Decide the items in OUTSTANDING.md, "Rules that look noisy":
   - the stored-row cap (200 or 25);
   - `missing-paragraph` (fix in Sefer's template, or count it apart);
   - `delimiter-surplus`.
3. When you get to it, work through TODO.md in scripture-kitchen and Sefer.
4. Run `scripts/verify.py`, then `dev/forgejo/check.sh`. Both should pass.
5. To send this to your coworker: the zip `translation-hub-handoff.zip` next
   to this file (rebuild it with `scripts/handoff-zip.sh`). It holds the repo
   with its git history, dev-data and a filled SQLite, but not node_modules or
   build output.
