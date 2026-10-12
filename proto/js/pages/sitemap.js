import { html } from "../ui.js";

// What exists here, what is only drawn, and what is proposed. The API column
// is the /v1 endpoint the real page would read — the same one an SDK calls.
const MAP = [
  { path: "/", name: "Home", state: "live", note: "Recently updated scripture repos, filtered by version, track and status. Pushes in a row by one person collapse to one line. Signed-in home is a wireframe.", api: "GET /v1/recent?version=&track=&status=", href: "#/" },
  { path: "/search", name: "Search", state: "live", note: "Languages and repos, filtered by version and track. Live as you type.", api: "GET /v1/search?q=&version=&track=", href: "#/search?q=gal" },
  { path: "/l/{lang}", name: "Language", state: "live", note: "Versions, each with a coverage shelf per track. Bulletins and releases are wireframes.", api: "GET /v1/languages/{lang}", href: "#/l/gl" },
  { path: "/l/{lang}/{version}/{track}", name: "Version · Track", state: "live", note: "Full shelf; per book the Official repo and other candidates. Gateway source is a wireframe.", api: "GET /v1/languages/{lang}/versions/{version}/{track}", href: "#/l/gl/reg/text" },
  { path: "/r/{owner}/{repo}", name: "Repo", state: "live", note: "File tree, the scripture-kitchen check, and verse coverage against the source the manifest declares (tS and RC; Burrito sources rarely resolve). Manifest health is a sketch. Viewer is plain text for now (proposed: read-only CodeMirror). History is a wireframe.", api: "GET /v1/repos/{owner}/{repo} · /tree · /lint", href: "#/r/kpass/tmf_3jn_text_reg" },
  { path: "/conflicts", name: "Shown twice on BIEL", state: "live", note: "Books where two repos are both Primary and show_on_biel. workTracker's rule, as is.", api: "GET /v1/conflicts", href: "#/conflicts" },
  { path: "/check", name: "Check USFM", state: "live", note: "Paste USFM, see findings. Runs offline once loaded.", api: "POST /v1/lint", href: "#/check" },
  { path: "/schema", name: "Schema", state: "live", note: "Suggested SQLite schema, including the over-time tables.", api: "—", href: "#/schema" },
  { path: "/me", name: "My languages", state: "proposed", note: "From the proposal; unsure what it would entail yet.", api: "?" },
  { path: "/l/{lang}/{version}/requests", name: "Contribution requests", state: "proposed", note: "From the proposal; unsure what it would entail yet.", api: "?" },
  { path: "/l/{lang}/bulletins", name: "Bulletins", state: "proposed", note: "From the proposal; unsure what it would entail yet.", api: "?" },
  { path: "/l/{lang}/{version}/releases", name: "Releases", state: "proposed", note: "From the proposal; unsure what it would entail yet.", api: "?" },
  { path: "/gateway/{lang}", name: "Gateway view", state: "proposed", note: "From the proposal; unsure what it would entail yet.", api: "?" },
  { path: "/api", name: "API docs", state: "proposed", note: "The OpenAPI spec and generated SDKs (TypeScript, Kotlin).", api: "GET /v1/openapi.json" },
  { path: "/status", name: "Sync status", state: "proposed", note: "Last poll, repos behind, failed reads. For us, not translators.", api: "GET /v1/status" },
  { path: "/l/{lang}/{version}/progress", name: "Progress over time", state: "later", note: "Velocity (verses per week) and lint burn-down from a daily log. Schema sketched; charts mocked.", api: "—", href: "#/schema" },
  { path: "/l/{lang}/{version}/language", name: "Linguistic charts", state: "later", note: "Per book and version: type/token ratio, vocabulary growth, rank–frequency, hapax share, character inventory, verse length vs source. Inputs mostly exist in sous-chef (words and glyph folds).", api: "GET /v1/…/stats" },
  { path: "/r/{owner}/{repo}/export", name: "Export", state: "later", note: "USFM, USX, USJ, HTML, verse-reference text — the formats scripture-kitchen already writes.", api: "GET /v1/repos/{owner}/{repo}/export?format=" },
  { path: "—", name: "Editing scripture", state: "out", note: "Not in the hub. Sefer is the editor; the hub links out to it.", api: "—" },
  { path: "—", name: "Assembled version view", state: "open", note: "Showing per-book Writer repos as one hypothetical joined project (as Sefer assembles) while still seeing each repo. Needs work; the model is funky. First class for now: consolidated whole repos.", api: "?" },
  { path: "—", name: "Chapter provenance", state: "open", note: "Where each chapter of the Official repo came from. Open question: chapter-level or repo-level?", api: "?" },
];

export function sitemapPage() {
  const group = (state, title, blurb) => {
    const rows = MAP.filter((r) => r.state === state);
    return html`<section>
      <h2><span class="state state-${state}">${state}</span> ${title}</h2>
      <p class="small">${blurb}</p>
      <table class="sitemap"><thead><tr><th>Page</th><th>Path</th><th>What it does</th><th>API</th></tr></thead><tbody>
        ${rows.map((r) => html`<tr><td>${r.href ? html`<a href="${r.href}">${r.name}</a>` : r.name}</td><td class="mono">${r.path}</td><td>${r.note}</td><td class="mono small">${r.api}</td></tr>`)}
      </tbody></table></section>`;
  };
  return html`
    <h1>Sitemap</h1>
    <p class="lede">Every page we talked about. Live pages run on real public data; dashed boxes on them are wireframes.</p>
    ${group("live", "In this prototype", "Click through. Data comes from the public data API and WACS, standing in for the hub's own API.")}
    ${group("proposed", "From the proposal, not pinned down", "Names only. Any writes would go through Forgejo's API as the signed-in user.")}
    ${group("later", "Later ideas", "Worth doing after the first slice.")}
    ${group("out", "Out of scope", "Deliberately not built.")}
    ${group("open", "Open questions", "Needs a decision before it has a page.")}`;
}
