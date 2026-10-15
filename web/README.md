# Translation Hub web

A thin front end over `/v1`: Vite + Solid + TypeScript, with one Effect service
(`src/api/client.ts`) for the API. Pages: home feed, repo (file tree, viewer,
books, findings), language, search, BIEL conflicts, checks, status.

```sh
pnpm install
pnpm gen        # hub-server --openapi → openapi.json → src/api/schema.d.ts (commit both)
pnpm dev        # http://localhost:5173, proxies /v1 to $HUB_API (default http://127.0.0.1:8788)
pnpm typecheck
pnpm build      # → dist/, served by hub-server with HUB_WEB_DIR=web/dist
pnpm smoke [base-url]   # drives the app in headless Chromium; screenshots → smoke-shots/
```

- **Types** come from the server's OpenAPI; `src/api/types.ts` only names them.
- **Findings are counts first.** The repo page loads `/findings` (one row per
  check, both engines, per-book counts) and fetches Sous Chef rows from
  `/proof?book=&code=` only for the book picked; lint rows are drawn by the
  viewer's own engine. `/checks` is the same view across all repos.
- **Tracing**: every `/v1` call sends `traceparent` (one trace id per page view,
  a new span id per call) and `x-hub-page`. The server logs the rest.
- **The USFM viewer** is Sefer's editor, installed: `@sefer/editor` and
  `@sefer/text` from Sefer by git (`package.json`; the repo is private, so
  `pnpm install` needs git credentials for github.com, as for
  scripture-kitchen). The engine is the hub's onion-only build
  (`src/engine/onion.ts`). The viewer is lazy-loaded with the file it shows.
  `EDITOR-PIECES.md` at the root maps what comes from the package.
- **Smoke** borrows Playwright from Sefer's `node_modules` (set
  `PLAYWRIGHT_FROM=/path/to/package.json` to borrow it elsewhere). It starts no
  server: run hub-server with `HUB_WEB_DIR=web/dist` first.
