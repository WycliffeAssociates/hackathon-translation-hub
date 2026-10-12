import { search } from "../api.js";
import { RESOURCE_NAMES, repoPath, statusOf } from "../domain.js";
import { bookFromRepoName } from "../books.js";
import { chip, failure, html, loading, render } from "../ui.js";

// Live search: the input re-queries as you type (debounced) and rewrites the
// URL in place, so a result list is always a link you can send someone.
export function searchPage(el, _params, query) {
  const q = query.get("q") ?? "";
  const rt = query.get("rt") ?? "";
  const track = query.get("track") ?? "";
  render(el, html`
    <h1>Search</h1>
    <form class="search-box" data-live>
      <input name="q" type="search" value="${q}" placeholder="Language name, code, or repo" autocomplete="off" autofocus>
    </form>
    <div class="filters" data-filters>
      ${filter("rt", "", "Any version", rt)}${Object.keys(RESOURCE_NAMES).map((k) => filter("rt", k, k, rt))}
      <span class="sep"></span>
      ${filter("track", "", "Any track", track)}${filter("track", "text", "text", track)}${filter("track", "audio", "audio", track)}
    </div>
    <div data-results>${q ? loading("results") : html`<p class="small">Type at least two letters.</p>`}</div>`);

  const results = el.querySelector("[data-results]");
  const input = el.querySelector("input");
  let timer;
  const run = () => {
    const state = new URLSearchParams({ q: input.value.trim(), rt, track });
    history.replaceState(null, "", `#/search?${state}`);
    if (input.value.trim().length < 2) return render(results, html`<p class="small">Type at least two letters.</p>`);
    const asked = input.value.trim();
    search(asked)
      .then((data) => input.value.trim() === asked && render(results, view(data, asked, { rt, track })))
      .catch((e) => render(results, failure(e)));
  };
  input.addEventListener("input", () => (clearTimeout(timer), (timer = setTimeout(run, 220))));
  el.querySelector("[data-live]").addEventListener("submit", (e) => (e.preventDefault(), run()));
  el.querySelector("[data-filters]").addEventListener("click", (e) => {
    const b = e.target.closest("button[data-k]");
    if (!b) return;
    const next = new URLSearchParams({ q: input.value.trim(), rt, track, [b.dataset.k]: b.dataset.v });
    location.hash = `#/search?${next}`;
  });
  if (q) run();
}

const filter = (key, value, label, current) =>
  html`<button type="button" class="pill ${value === current ? "on" : ""}" data-k="${key}" data-v="${value}">${label}</button>`;

function view(data, q, { rt, track }) {
  const keep = (row) => (!rt || row.resource_type === rt) && (!track || row.type === track);
  const exact = (l) => (l.ietf_code.toLowerCase() === q.toLowerCase() ? 0 : 1);
  const langs = data.language
    .map((l) => ({ ...l, contents: l.contents.filter(keep) }))
    .filter((l) => l.contents.length)
    .sort((a, b) => exact(a) - exact(b));
  const repos = data.content.filter(keep);
  if (!langs.length && !repos.length) return html`<p>No scripture found for “${q}”.</p>`;
  return html`
    ${langs.length ? html`<h2>Languages <span class="count">${langs.length}</span></h2>
      <ul class="results">${langs.map(langRow)}</ul>` : ""}
    ${repos.length ? html`<h2>Repos <span class="count">${repos.length}</span></h2>
      <ul class="results">${repos.map(repoRow)}</ul>` : ""}`;
}

function langRow(l) {
  // Versions this language has, each with its tracks and how many repos.
  const versions = new Map();
  for (const c of l.contents) {
    const key = `${c.resource_type}/${c.type}`;
    const v = versions.get(key) ?? { rtype: c.resource_type, track: c.type, n: 0, primary: 0 };
    v.n++;
    if (statusOf(c) === "Primary") v.primary++;
    versions.set(key, v);
  }
  return html`<li>
    <a class="result-title" href="#/l/${l.ietf_code}">${l.english_name}</a>
    ${l.national_name && l.national_name !== l.english_name ? html`<span class="native">${l.national_name}</span>` : ""}
    <code>${l.ietf_code}</code>
    <div class="versions">${[...versions.values()].map((v) => html`<a class="pill" href="#/l/${l.ietf_code}/${v.rtype}/${v.track}">
      <b>${v.rtype}</b> ${v.track} · ${v.n} repo${v.n > 1 ? "s" : ""}${v.primary ? html` · <span class="ok">${v.primary} primary</span>` : ""}</a>`)}</div>
  </li>`;
}

function repoRow(r) {
  const book = bookFromRepoName(r.git_repo?.repo_name ?? "");
  return html`<li>
    <a class="result-title mono" href="#/r/${repoPath(r)}">${repoPath(r)}</a> ${chip(statusOf(r))}
    <span class="small">${r.language?.english_name} · ${r.resource_type} ${r.type}${book ? ` · ${book.name}` : ""}</span>
  </li>`;
}
