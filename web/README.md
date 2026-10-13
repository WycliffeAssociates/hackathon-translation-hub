# Translation Hub web

A thin front end over `/v1`: Vite + Solid + TypeScript, with one Effect service
(`src/api/client.ts`) for the API. Pages: home feed, repo (file tree, viewer,
books, proofreading), language, search, BIEL conflicts, status.

```sh
pnpm install
pnpm gen        # hub-server --openapi → openapi.json → src/api/schema.d.ts (commit both)
pnpm dev        # http://localhost:5173, proxies /v1 to $HUB_API (default http://127.0.0.1:8788)
pnpm typecheck
pnpm build      # → dist/, served by hub-server with HUB_WEB_DIR=web/dist
pnpm smoke [base-url]   # drives the app in headless Chromium; screenshots → smoke-shots/
```

- **Types** come from the server's OpenAPI. Two shapes are patched by hand in
  `src/api/types.ts`: `RepoDetail` (its `books`/`synthetic` clash with
  `RepoSummary`'s in the generated intersection) and the proofreading body,
  which OpenAPI does not describe and is decoded with Schema.
- **Tracing**: every `/v1` call sends `traceparent` (one trace id per page view,
  a new span id per call) and `x-hub-page`. The server logs the rest.
- **The USFM viewer** is imported from `../spikes/viewer/src` through Vite
  aliases (`@viewer`, plus the spike's `#core/*` and `virtual:viewer-layers`),
  in the spike's build C: onion-only engine wasm, reading-only layers. It is
  lazy-loaded with the repo page. The spike's own dependencies resolve from
  `spikes/viewer/node_modules`, so run `pnpm install --ignore-workspace` there
  first.
- **Smoke** borrows Playwright from Sefer's `node_modules` (set
  `PLAYWRIGHT_FROM=/path/to/package.json` to borrow it elsewhere). It starts no
  server: run hub-server with `HUB_WEB_DIR=web/dist` first.
