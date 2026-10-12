# Handoff: wacs-dashboard-2 (rebuild of gitea-dashboard)

**Type:** continuation + design kickoff. **Your first job is to align with Will, not to write code.**

## Goal

Rebuild the WACS ecosystem dashboard as a **sidecar next to Forgejo**. It reads Forgejo's bare repos straight off disk (read-only) and Forgejo's MariaDB through a read-only user. It offers domain-driven (Bible-driven) search and inspection across the whole public catalog, with an **HTTP API as a first-class product**, and the UI is one consumer of that API.

**Done for this first session:** a short architecture doc/plan in this repo that Will has agreed to. It should cover what ports from the old repo and what doesn't, the frontend decision, the crate layout, and the index store. The plan is done when Will signs off on it. No code is needed.

## Why this exists, and why it's a redo

- The *why* is unchanged: read `../gitea-dashboard/plans/VISION.md`. It's a consolidation layer over Forgejo, not a replacement for it, the reader, or the linter.
- The *how* is changing. The old build polled Gitea's HTTP API and kept its own clones in an LRU cache (`plans/CACHE.md`). Will dislikes polling: it makes a whole-ecosystem view expensive and duplicates on disk what Forgejo already stores. Running next to Forgejo removes the network I/O, so full-catalog indexing becomes cheap.

## Settled (from the previous session; don't re-litigate without a reason)

- **Content:** read the bare repos directly with git2 or gix (that choice is open), from a read-only volume mount.
  - Prod: rootless Forgejo; volume `wacs2_gitea_data` is mounted at `/var/lib/gitea`, so repos are at `/var/lib/gitea/git/repositories/<owner>/<repo>.git`.
  - Names on disk are lowercase. Repos use `packed-refs`.
  - The mount uses the SELinux `z` flag and Forgejo runs as uid 1000, so the sidecar mount needs `:ro,z`.
- **Metadata and visibility:** MariaDB 10.11 (`wacs2-db-1`, database `gitea`), with a read-only user `dashboard_ro` that has `SELECT` on `repository`, `topic`, `repo_topic`, `release`, and `attachment` only.
  - **The disk includes private repos.** The public-only rule comes from `is_private=0`.
  - **Releases are mostly not git data.** Tags are in git, but titles, notes, drafts and attachments are rows in `release` and `attachment`. Filter `is_draft=0` alongside `is_private=0`. Attachment files sit in Forgejo's data dir on the same volume, at `<attachments>/<uuid[0]>/<uuid[1]>/<uuid>`.
  - Topics are effectively unused (1 topic, 424 repos), so **metadata comes from the manifests**.
- **Change detection:** each tick, run `SELECT … FROM repository WHERE updated_unix > :watermark`, then compare HEAD SHAs on disk. As a fallback, sweep every repo's HEAD on disk periodically (seconds of local I/O).
  - No webhook, no inotify, and no Forgejo HTTP API.
  - ⚠️ **Not yet verified:** that Forgejo bumps `updated_unix` on push. Check it in prod before relying on it.
- **Robustness:** a DB row with no repo directory, or an empty repo, is normal. Index the metadata only and mark the content unavailable.
- **Deployment:** `docker compose up` as a sidecar in the `wacs2` stack, optionally behind Traefik or Caddy. No writes to Forgejo's volume or DB, ever.

## Dev environment (already working in this repo)

- `docker compose up -d db`: MariaDB 10.11 loaded from `dev-data/sql/*.sql.gz` (32,575 public repo rows) plus the `dashboard_ro`/`dev` user. See `docker-compose.yml` and `dev/db-init/`.
- `dev-data/repositories/`: 1,441 real bare repos copied off the server. All of them match a DB row. They include the whole `wa-catalog` plus complete NT/Bible `_text_reg` sets and a diverse sample.
- `scripts/pull-more.sh <n> [regex]`: pulls more repos from the dev server (`WACS_HOST=<user>@<wacsdev-host>`, host `<wacsdev-host>`, site `content.wacsdev.org`). Your connection is limited to ~3.5 MB/s total.
- `dev-data/all-repos.txt` lists every public repo as `owner/name`.
- Env: `DATABASE_URL=mysql://dashboard_ro:dev@127.0.0.1:3306/gitea`, `REPO_ROOT=./dev-data/repositories`.

## The old repo is a reference, not a template: `../gitea-dashboard`

No git history. Port pieces deliberately; don't bulk-copy.

- **Probably ports:**
  - `backend/src/manifest/` (RC yaml, Burrito, ts manifest → `NormalizedManifest`)
  - `lint.rs`, `render.rs`, `usfm_source.rs`, `proportionality.rs` (check each against what `usfm_onion` / scripture-kitchen already provide)
  - API shapes in `backend/src/api/` (projects, repo, lookups, stats, export) as a starting point for the public API
  - The domain thinking in `plans/proposals/`
- **Goes away:** `cache.rs`, `kv_cache.rs`, `git_transport.rs`, `gitea.rs` (the API poller), `api/webhook.rs`, `plans/CACHE.md`, the content-cache and `head_sha` migrations, and everything about the Cloudflare bypass and rate limits.
- **Frontend:** `frontend/` (Solid + Vanilla Extract + Ark + Effect + TanStack) and `plans/FRONTEND.md` are not assumed to carry over (see open questions).

## Open questions (resolve these with Will; give a recommendation, push back where warranted)

1. **Dioxus vs. staying on Solid.** Will leans toward Dioxus because so much of the stack is already Rust (`../scripture-kitchen`, https://github.com/WycliffeAssociates/scripture-kitchen; `../usfm_onion`). He says he could be talked out of it. Factors to weigh:
   - It's still a web target, so the UTF-16 boundary work remains.
   - **The API must stay first-class.** Dioxus server functions alone don't count; there has to be a stable, documented REST/JSON surface.
2. **Pulling Sefer core functions into shared crates.** Candidates are in `../Sefer`: move some core functions into crates in this workspace and expose them to Sefer as wasm. Name the candidates, the crate boundaries, and what Sefer would consume. Keep it scoped; this is the attractive extra, not the core.
3. **Index store** for derived manifest and search data: SQLite, Postgres, or in-memory, possibly with tantivy for search. Rebuild-from-scratch time across ~32k repos is a key number to estimate.
4. **git2 vs. gix.**
5. **Whether `updated_unix` bumps on push** (see the warning above). It decides whether the sweep is the primary or the fallback mechanism.

## Non-goals

- Forking or modifying Forgejo.
- Any writes to Forgejo.
- Surfacing private repos.
- A new reader or a new linter (see VISION.md).
- Porting the old frontend wholesale.
- Starting implementation before the plan is agreed.

## Suggested skills

- `/engineering` for Will's values.
- `/rails` to start at idea → plan.
- `/html-plan` for the architecture plan.
- `/show-me` or `/visual-explainer` if a diagram helps the frontend and crate-layout discussion.

## Expected return

An alignment conversation first: reflect your understanding back to Will and ask about the open questions. Then a written plan in this repo covering:
- architecture (crates, binaries, data flow from DB and disk to index to API and UI),
- the port and drop list,
- the frontend decision with reasoning,
- the scope of the Sefer extraction,
- the first small, shippable increment.
