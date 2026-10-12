# Translation Hub — clickable wireframe

A no-dependency prototype of the screens we talked about, on live public data.
Nothing to install.

```sh
cd translation-hub-proto
python3 -m http.server 8000      # or: npx serve, php -S localhost:8000, …
# open http://localhost:8000
```

(Browsers block JS modules from `file://`, so it needs any static server.)

## What's here

| Route | Page |
|---|---|
| `#/` | Recently updated repos, filter by version / track / status |
| `#/search?q=gal` | Live search, filter by version (reg/ulb/…) and track (text/audio) |
| `#/l/gl` | Language: every version, a 66-book coverage shelf per track |
| `#/l/gl/reg/text` | Version · Track: per book the Official repo (Sefer's rule) and other candidates |
| `#/r/kpass/tmf_3jn_text_reg` | Repo: file tree, chapter grid, scripture-kitchen check and verse coverage vs the declared source, manifest health (sketch) |
| `#/conflicts` | Books shown twice on BIEL (both Primary and show_on_biel), workTracker's rule |
| `#/check` | Paste USFM and check it |
| `#/sitemap` | Live vs wireframe vs proposed pages, with the /v1 endpoint each would use |
| `#/stack` | Suggested build and links |
| `#/schema` | Suggested SQLite schema; over-time tables as a later idea |

Dashed boxes are wireframes: drawn, not built.

## Files

```
index.html  styles.css
js/app.js          hash router
js/api.js          the two data sources (public data API, WACS REST)
js/domain.js       Language → Version → Track → Book, and the Official rule
js/ui.js           escaping html`` template, shelf, chips
js/books.js        66 books, chapter counts
js/usfm.js         tS chunk stitching (simplified)
js/check.js        scripture-kitchen lint wrapper
js/pages/*.js      one file per page
vendor/onion/      scripture-kitchen v0.1.10 onion-wasm (pkg-web) + reader.js (types stripped from reader.ts)
```

## Stand-ins

The real hub reads Forgejo's repos off disk and keeps its own SQLite. Here:

- **Catalog** — the BIEL public data API (`api.bibleineverylanguage.org/v1/graphql`).
  Version = `resource_type`, Track = `type`, status = `wa_content_metadata.status`.
- **Files** — WACS's REST API, blobs by SHA. WACS sits behind Cloudflare and
  rate-limits bursts (429), so large repos are checked a chapter at a time. If you
  hit the limit, wait a minute. On disk none of this applies.
- **tS stitching** is simplified; Sefer's is the real one.
