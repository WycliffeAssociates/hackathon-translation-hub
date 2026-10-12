// Tiny view helpers: an escaping `html` template and the few shared pieces.
// Pages return strings; the router sets them. No virtual DOM, no framework.
import { BOOKS } from "./books.js";
import { bookState, byStatus, officialOf, repoPath, statusOf } from "./domain.js";

const RAW = Symbol("raw");
export const raw = (s) => ({ [RAW]: String(s) });
const esc = (v) =>
  String(v).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
const part = (v) => (v == null || v === false ? "" : Array.isArray(v) ? v.map(part).join("") : v[RAW] ?? esc(v));

/** Interpolations are escaped unless wrapped in `raw()` or produced by `html`. */
export const html = (strings, ...values) =>
  raw(strings.reduce((out, s, i) => out + s + (i < values.length ? part(values[i]) : ""), ""));

export const render = (el, view) => (el.innerHTML = part(view));

export const chip = (status) =>
  html`<span class="chip chip-${(status ?? "none").toLowerCase()}">${status ?? "unlabelled"}</span>`;

export const wire = (label, body) => html`<section class="wire"><span class="wire-tag">wireframe · ${label}</span>${body}</section>`;

/** An idea from the proposal that nobody has pinned down yet. */
export const unsure = (label, title) =>
  wire(label, html`<h3>${title}</h3><p class="small">From the proposal. Will: unsure what this would entail yet.</p><div class="wf-rows"><div></div><div></div></div>`);

export const loading = (what) => html`<p class="loading">Loading ${what}…</p>`;

export const failure = (e) =>
  html`<div class="failure"><b>${e.source ?? "Error"}</b> ${e.message}</div>`;

export const legend = () => html`<div class="legend">
  <span><i class="cell official"></i>Official (Primary)</span>
  <span><i class="cell draft"></i>Work, nothing Primary</span>
  <span><i class="cell conflict"></i>Two Primaries</span>
  <span><i class="cell inactive"></i>Inactive only</span>
  <span><i class="cell empty"></i>Nothing yet</span>
  <span><i class="cell draft whole"></i>Covered by a whole-Bible repo</span>
</div>`;

/**
 * The coverage shelf: 66 books, OT then NT. Server-renderable as-is — it is
 * only divs. `href(book)` decides where a cell goes; null makes it inert.
 */
export function shelf(track, href, { big = false } = {}) {
  const cell = (b) => {
    const rows = track.books.get(b.slug);
    const state = bookState(rows);
    const off = rows && officialOf(rows);
    const title = `${b.name} · ${state}${rows ? ` · ${rows.length} repo${rows.length > 1 ? "s" : ""}` : ""}${off ? ` · official: ${repoPath(off)}` : ""}`;
    const to = href(b, rows);
    const whole = rows?.some((r) => r.books) ? " whole" : "";
    const body = html`<b>${b.usfm}</b>`;
    return to
      ? html`<a class="cell ${state}${whole}" href="${to}" title="${title}">${body}</a>`
      : html`<span class="cell ${state}${whole}" title="${title}">${body}</span>`;
  };
  return html`<div class="shelf ${big ? "big" : ""}">
    <div class="testament"><span class="t-label">Old Testament</span><div class="cells">${BOOKS.filter((b) => !b.nt).map(cell)}</div></div>
    <div class="testament"><span class="t-label">New Testament</span><div class="cells">${BOOKS.filter((b) => b.nt).map(cell)}</div></div>
  </div>`;
}

export const statusCounts = (rows) => {
  const c = { Primary: 0, Active: 0, Inactive: 0, none: 0 };
  for (const r of rows) c[statusOf(r) ?? "none"]++;
  return c;
};

export const ago = (iso) => {
  if (!iso) return "";
  const minutes = Math.round((Date.now() - new Date(iso.endsWith("Z") || iso.includes("+") ? iso : `${iso}Z`)) / 6e4);
  if (minutes < 60) return `${Math.max(minutes, 1)} min ago`;
  if (minutes < 60 * 24) return `${Math.round(minutes / 60)} h ago`;
  const days = Math.round(minutes / 1440);
  if (days < 60) return `${days} days ago`;
  if (days < 730) return `${Math.round(days / 30)} months ago`;
  return `${Math.round(days / 365)} years ago`;
};

const plural = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;
/** The error / warning / info badges for one check. */
export const counts = (t) =>
  html`<span class="sev error">${plural(t.error, "error")}</span> <span class="sev warning">${plural(t.warning, "warning")}</span> <span class="sev info">${t.info} info</span>`;

/**
 * The two ways a Version arrives: consolidated whole-Bible repos (gateway
 * languages, the catalog) and one repo per book (heart languages from
 * Writer / tS). The strip names the first kind; the shelf shows both.
 */
export const consolidated = (track) => {
  if (!track.whole.length) return "";
  const one = (r) =>
    html`<span class="whole-repo">${r.git_repo ? html`<a class="mono" href="#/r/${repoPath(r)}">${repoPath(r)}</a>` : html`<span class="mono">${r.name}</span>`} ${chip(statusOf(r))} <span>${r.books?.length ? `${r.books.length} books` : "books unknown"}</span></span>`;
  const sorted = track.whole.toSorted(byStatus);
  const shown = sorted.filter((r) => statusOf(r) === "Primary");
  const rest = sorted.filter((r) => !shown.includes(r));
  return html`<div class="consolidated small"><b>Whole-Bible repos</b> ${shown.map(one)}
    ${rest.length ? html`<details><summary>${shown.length ? `${rest.length} more` : `${rest.length} repo${rest.length > 1 ? "s" : ""}, none Primary`}</summary><div class="whole-list">${rest.map(one)}</div></details>` : ""}</div>`;
};
