import { html } from "../ui.js";

const STACK = [
  ["Server", "Rust (axum), one binary beside Forgejo", "Reads the bare repos off disk, read-only, and Forgejo's MariaDB with a read-only user. Fast enough to index every public repo in seconds."],
  ["Hub data", "SQLite", "What Forgejo doesn't know: the Bible shape (language, version, track, book), Official picks, check results per commit. Kept, not a cache."],
  ["Change detection", "Soft poll", "Every few minutes, ask MariaDB which repos changed, compare HEADs, re-index those. Nightly full pass. No webhooks to drop on a restart."],
  ["Checks", "scripture-kitchen, pinned by git tag", "The same Rust engine Sefer uses. This page runs its WASM build; the server runs it natively."],
  ["API", "JSON /v1 + OpenAPI (utoipa)", "The API is a product: generated SDKs for TypeScript and Kotlin, so apps can call hub.listVersions() instead of scraping."],
  ["Writes", "Forgejo's API, as the signed-in user", "Whatever the hub ends up writing goes through the API like any client. The hub never writes to Forgejo's disk or database."],
  ["Sign-in", "WACS accounts", "Forgejo as the OAuth2 provider. One account everywhere."],
  ["Front end", "Plain HTML, CSS, a little JS", "Like this prototype: no framework, no build step. Pages call /v1; JS only for live search and click-to-check. Heatmaps are just HTML. Cache headers make repeat visits nearly free on bad internet."],
  ["Reading scripture", "Read-only CodeMirror + Sefer's regular mode", "Diagnostics drawn in the text, infinite scroll through a book. Loaded only on pages that show text."],
  ["Not doing", "Editing", "The hub views and checks. Sefer is the editor."],
];

const LINKS = [
  ["Public data API (GraphQL)", "https://api.bibleineverylanguage.org/v1/graphql", "Stand-in catalog for this prototype"],
  ["Public data API explorer", "https://api-explorer.bibleineverylanguage.org/", "Try queries by hand"],
  ["WACS", "https://content.bibletranslationtools.org", "The Forgejo the hub sits beside"],
  ["WACS API (Swagger)", "https://content.bibletranslationtools.org/api/swagger", "What this prototype reads for trees and files"],
  ["scripture-kitchen", "https://github.com/WycliffeAssociates/scripture-kitchen", "USFM engine and checks · tag v0.1.10"],
  ["axum", "https://github.com/tokio-rs/axum", "HTTP server"],
  ["utoipa", "https://github.com/juhaku/utoipa", "OpenAPI from Rust types"],
  ["sqlx", "https://github.com/launchbadge/sqlx", "SQLite and MariaDB, checked queries"],
  ["git2", "https://github.com/rust-lang/git2-rs", "Reading the bare repos"],
  ["Hey API", "https://heyapi.dev", "TypeScript SDK from the OpenAPI spec"],
  ["OpenAPI Generator", "https://openapi-generator.tech", "Kotlin SDK from the same spec"],
];

export function stackPage() {
  return html`
    <h1>Stack &amp; links</h1>
    <p class="lede">Will's suggested build. Open for discussion — this prototype only fixes the screens, not the stack.</p>
    <table class="stack"><tbody>${STACK.map(([layer, pick, why]) => html`<tr><th>${layer}</th><td><b>${pick}</b><p>${why}</p></td></tr>`)}</tbody></table>

    <h2>Open</h2>
    <ul class="open">
      <li><b>Front end:</b> server-rendered HTML from Rust templates, or static pages calling /v1 like this one? Both stay light; the second keeps one API for everyone.</li>
      <li><b>Provenance:</b> track where each chapter came from, or only each repo?</li>
      <li><b>Scope:</b> replace the Forgejo UI for translators, or start with the read-only shelf and grow?</li>
    </ul>

    <h2>Links</h2>
    <table class="links"><tbody>${LINKS.map(([name, url, note]) => html`<tr><td><a href="${url}" target="_blank" rel="noopener">${name} ↗</a></td><td class="small">${note}</td></tr>`)}</tbody></table>

    <h2>About this prototype</h2>
    <p class="small">No dependencies, no build: <code>index.html</code>, <code>styles.css</code>, ES modules in <code>js/</code>, and the scripture-kitchen WASM in <code>vendor/onion/</code>. Data: the public data API for the catalog, WACS's REST API for files. The real hub reads disk instead, so WACS's rate limit and Cloudflare go away.</p>`;
}
