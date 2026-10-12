import { fetchConflicts } from "../../vendor/biel-duplicate-chapters-primary.js";
import { WACS } from "../api.js";
import { failure, html, render } from "../ui.js";

// BIEL shows two sources for one book when both are Primary AND show_on_biel.
// The rule is workTracker's (kept in step with biel-frontend), used as is.
// One query over the whole catalog: ~20 s here; one SQL GROUP BY in the hub.
let cached = null;

export function conflictsPage(el) {
  render(el, html`
    <h1>Shown twice on BIEL</h1>
    <p class="lede">Books where two repos are both <b>Primary</b> and <b>show_on_biel</b>, so BIEL stitches two sources for the same book. Usually a Writer book repo next to a consolidated whole repo.</p>
    <p class="small">Same rule as workTracker's data-inconsistencies report and biel-frontend's collation: non-gateway languages (plus <code>ceb</code>), scripture / gloss / parascriptural, grouped by domain · resource type · track, books from what BIEL rendered.</p>
    <div data-out><p class="loading">Asking the public data API about every Primary + shown repo (about 20 seconds; the hub answers this from its own SQLite in milliseconds)…</p></div>`);
  const out = el.querySelector("[data-out]");
  (cached ??= fetchConflicts("prod").catch((e) => ((cached = null), Promise.reject(e))))
    .then(({ conflicts }) => show(out, conflicts))
    .catch((e) => render(out, failure(e)));
}

function show(out, conflicts) {
  const books = conflicts.reduce((n, c) => n + Object.keys(c.conflictingBooks).length, 0);
  render(out, html`
    <div class="conflict-head">
      <span class="cov-num">${books} books · ${conflicts.length} groups · ${new Set(conflicts.map((c) => c.language)).size} languages</span>
      <input type="search" placeholder="Filter by language or repo" data-filter>
    </div>
    <p class="small">Book chips open that book in BIEL. Repo names open in the hub.</p>
    <div data-list>${conflicts.map(group)}</div>`);
  const input = out.querySelector("[data-filter]");
  input.addEventListener("input", () => {
    const q = input.value.trim().toLowerCase();
    for (const g of out.querySelectorAll("[data-group]")) g.hidden = q && !g.dataset.group.includes(q);
  });
}

// One dropdown per language group. The usual shape is one consolidated repo
// clashing with a different book repo per book, so books are grouped around
// the repo that recurs most (the anchor) and the others fold away.
function group(c) {
  const entries = Object.entries(c.conflictingBooks);
  const freq = new Map();
  for (const [, b] of entries) for (const x of b.sources) freq.set(x.name, (freq.get(x.name) ?? 0) + 1);
  const byAnchor = new Map();
  for (const [slug, b] of entries) {
    const anchor = b.sources.toSorted((x, y) => freq.get(y.name) - freq.get(x.name) || x.name.localeCompare(y.name))[0];
    const row = byAnchor.get(anchor.name) ?? byAnchor.set(anchor.name, { anchor, books: [], others: [] }).get(anchor.name);
    row.books.push([slug, b.seeInBiel]);
    for (const x of b.sources) if (x.name !== anchor.name) row.others.push([slug, x]);
  }
  const repos = new Set(freq.keys());
  const haystack = `${c.language} ${c.group} ${[...repos].join(" ")}`.toLowerCase();
  const ietf = c.language.match(/\(([^)]+)\)$/)?.[1];
  return html`<details class="conflict" data-group="${haystack}">
    <summary><b>${c.language}</b> <code>${c.group}</code> <span class="small">${entries.length} book${entries.length > 1 ? "s" : ""} · ${repos.size} repos</span></summary>
    ${ietf ? html`<p class="small"><a href="#/l/${ietf}">Open ${c.language} in the hub →</a></p>` : ""}
    ${[...byAnchor.values()].map((r) => html`<div class="anchor-row">
      <div>${repoLink(r.anchor)} <span class="small">clashes on ${r.books.length} book${r.books.length > 1 ? "s" : ""} with</span></div>
      <div class="conflict-books">${r.books.map(([slug, url]) => html`<a class="book-chip" href="${url}" target="_blank" rel="noopener" title="See ${slug} in BIEL">${slug}</a>`)}</div>
      <details class="small others"><summary>${new Set(r.others.map(([, x]) => x.name)).size} other repo${r.others.length > 1 ? "s" : ""}</summary>
        ${r.others.map(([slug, x]) => html`<div><span class="book-chip">${slug}</span> ${repoLink(x)}</div>`)}</details>
    </div>`)}
  </details>`;
}

// Repos on WACS open in the hub's repo page; anything else links out.
function repoLink(s) {
  const path = s.repoUrl?.startsWith(WACS) ? s.repoUrl.slice(WACS.length + 1).replace(/\.git$/, "") : null;
  return path ? html`<a class="mono" href="#/r/${path}">${s.name}</a>` : html`<span class="mono">${s.name}</span>`;
}
