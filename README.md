# Translation Hub

A read-only service beside WACS (Forgejo) that shows Bible translation work in the shape translators think in — language → version (resource type) → track → book — instead of as repos. It reads the bare repos off disk and Forgejo's MariaDB read-only, keeps its own SQLite, and serves an API (OpenAPI, generated SDKs) plus light web pages.

**New here?** [START_HERE.md](START_HERE.md). Then [AGENTS.md](AGENTS.md) — how to run it, how to verify a change (`scripts/verify.py`), observability, deploy. [OUTSTANDING.md](OUTSTANDING.md) — what is built, what is left, and what is the coworker's call.

- `proto/` — clickable wireframe on live public data (no install; `python3 -m http.server` in that folder). Zipped as `translation-hub-proto.zip`.
- `plans/` — architecture notes and the combined proposal.
- `crates/` — the Rust side (below).
- `web/` — the thin web app (Vite + Solid + Effect), calling `/v1` only.
- `scripts/` — `verify.py` (run the real thing and check it), `latency.py` (response times from the logs).
- `spikes/viewer/` — Sefer's editor forked into a read-only USFM viewer, with sizes and the seams to make Sefer's editor importable.
- `fixtures/` — `hub-scan` output for 15 repos of every shape, and two synthetic repos as served.

## The Rust workspace

| Crate | Does | Built from |
|---|---|---|
| `manifest` | tS / RC / Burrito manifests → one normalized shape (language, books, sources, finished chunks) | the old gitea-dashboard's parsers |
| `repo-read` | bare repos, read-only: HEAD from ref files, trees, blobs (git2) | — |
| `stitch` | Writer chunks → one USFM book | Sefer `writerBook.ts` @ 50aa9cd, byte-identical on 987 dev repos |
| `synthetic` | a Writer scripture repo served as an RC: `manifest.yaml`, `NN-BOOK.usfm`, `.upstreams.json` | Sefer assemble / writeMetadata / upstreams |
| `checks` | scripture-kitchen lint summary per book, Sefer's `\s5` policy, content keys, verse coverage vs a source | scripture-kitchen by tag |
| `domain` | the Official pick, a book's shelf state, BIEL's shown-twice rule | Sefer `defaultCandidate`; workTracker's BIEL rule |
| `ingest` | one repo → its scan (HEAD, manifest, books, lint, coverage); Forgejo's repo list | hub-scan's pipeline |
| `proof` | sous-chef proofreading over a repo's books, UTF-16 offsets | scripture-kitchen `sous` CLI @ v0.1.9 |
| `biel` | the public data API: languages (names, regions, alternates), catalog status, books | Sefer `catalogue.ts`; workTracker's query |
| `store` | the hub's SQLite: migrations, writes, the read models the API serves | the proto's Schema tab |
| `server` | `hub-server`: `/v1` (OpenAPI), soft poll, catalog refresh, wide-event logs | — |
| `hub-scan` | CLI: the whole pipeline over a repo root → JSON, or `--db` into SQLite | — |

```sh
cargo test --workspace
scripts/verify.py                                   # the real server on dev-data, every route checked
HUB_LOG_FORMAT=pretty cargo run --release -p hub-server    # → http://127.0.0.1:8787/v1/status
cargo run --release -p hub-scan -- dev-data/repositories --out scan.json --synthetic-dir /tmp/synthetic
```

Golden tests that read `dev-data` run when `REPO_ROOT=$PWD/dev-data/repositories` is set.

### What the dev data says (1,441 repos, one `hub-scan`, ~2.8 s on a 10-core Mac)

- Manifests: tS 1,103 · RC 291 · Burrito 8 · none 38 · empty 1 · 0 parse failures. tS subjects: Bible 1,039, notes 30, questions 17, OBS 16, words 1.
- 983 Writer scripture repos become synthetic repos. 4,624 books linted; 2,593 measured against their declared source (`wa-catalog/<lang>_<id>`): 2,258 complete, median 100%.
- Lint over 3,589 RC books alone takes ~140 ms; reading blobs dominates.
- Nothing in `dev-data` is written (checked with a `-newer` marker after every run).

## Decided

Will, 2026-10-12: build the agreed core end to end as a proposal the coworker can rewrite (see OUTSTANDING.md for the list). Rust API + SQLite (kept, not a cache) · soft poll, not webhooks · public repos only · writes only through Forgejo's API · WACS sign-in · scripture-kitchen by git tag · no editing in the hub (Sefer is the editor) · read-only CodeMirror viewer with Sefer's regular/USFM modes · consolidated whole repos are first-class; Writer repos are served as synthetic repos and people are moved onto real ones over time.

## Open — to decide together

- The SQLite schema (sketch in `proto/` → Schema tab) and the `/v1` API shapes.
- The web stack: server-first and light vs. TypeScript pages calling `/v1`.
- Domain naming, provenance granularity, and the parked "assembled version" view.

## Notes for whoever picks this up

- **scripture-kitchen is pinned to `v0.1.9`.** `v0.1.10` exists only in a local clone; its engine is identical. Push the tag, then bump every pin (AGENTS.md lists them).
- **Linux builds:** `git2` links the system libgit2 here (Homebrew). For a server image, enable `git2/vendored-libgit2` (needs cmake + a C compiler) or install libgit2 1.9.x.
- **Default branches** are `master`, `scribe-main` (dcs-mirror) and `main`; nothing assumes `master`.
- **Lint counting:** `hint` findings count under `infos`; only severity-`form` rows are skipped (the form *category* includes some hints and infos, e.g. `delimiter-surplus`, which dominates infos). One line to change if ranking should drop the whole category.
- **Synthetic repos** require subject Bible *and* USFM chunks — Writer also writes notes and questions in markdown, book by book. A book's fallback title is the manifest's project name, else the book id (Sefer uses either, depending on the call site).
- **Remaining synthetic warnings are real:** a literal `\n` typed into text (1,571), `\v18` without a space, `\V`.
- **The viewer** should ship as the spike's build C (onion-only wasm, reading layers only): ~263 KB brotli, no Solid, no Effect. `spikes/viewer/REUSABLE.md` lists eight safe steps to make Sefer's editor importable instead of forked.
