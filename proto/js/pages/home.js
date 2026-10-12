import { recent } from "../api.js";
import { bookFromRepoName } from "../books.js";
import { repoPath, statusOf } from "../domain.js";
import { ago, chip, failure, html, loading, render, unsure } from "../ui.js";

const FACETS = [
  ["rt", "Version", ["", "reg", "ulb", "udb", "ult", "bible"]],
  ["track", "Track", ["", "text", "audio"]],
  ["status", "Status", ["", "Primary", "Active", "Inactive"]],
];

// Home is what changed lately, like Forgejo's explore page, but in the Bible's
// shape: a run of pushes by one person to one language collapses to one row.
export function home(el, _params, query) {
  const state = Object.fromEntries(FACETS.map(([k]) => [k, query.get(k) ?? ""]));
  render(el, html`
    <div class="home">
      <div class="home-main">
        <div class="toolbar">
          <h1>Recently updated</h1>
          <form class="search-inline" data-search><input name="q" type="search" placeholder="Search…" autocomplete="off"></form>
        </div>
        <div class="facets" data-facets>${FACETS.map(([key, label, values]) => html`
          <div class="facet"><span class="facet-label">${label}</span>${values.map((v) => html`
            <button type="button" class="pill ${state[key] === v ? "on" : ""}" data-k="${key}" data-v="${v}">${v || "any"}</button>`)}</div>`)}
        </div>
        <div data-feed>${loading("recent changes")}</div>
        <p class="center"><button data-more>Load more</button></p>
      </div>
      <aside class="home-side">
        <p class="small">Live from the catalog: every scripture repo on WACS, newest change first.</p>
        <p class="small">Try <a href="#/l/gl">Galician</a> · <a href="#/l/en">English</a> · <a href="#/l/pov">Kiriol</a></p>
        ${unsure("signed-in home", "Your languages")}
      </aside>
    </div>`);

  el.querySelector("[data-facets]").addEventListener("click", (e) => {
    const b = e.target.closest("button[data-k]");
    if (!b) return;
    const next = new URLSearchParams({ ...state, [b.dataset.k]: b.dataset.v });
    for (const [k, v] of [...next]) if (!v) next.delete(k);
    location.hash = `#/${next.size ? `?${next}` : ""}`;
  });

  const feed = el.querySelector("[data-feed]");
  const more = el.querySelector("[data-more]");
  const rows = [];
  const load = async () => {
    more.disabled = true;
    try {
      const page = await recent({ ...state, offset: rows.length });
      rows.push(...page);
      render(feed, rows.length ? html`<ul class="feed">${groups(rows).map(row)}</ul>` : html`<p>Nothing matches.</p>`);
      more.hidden = page.length < 60;
    } catch (e) {
      render(feed, failure(e));
    } finally {
      more.disabled = false;
    }
  };
  more.addEventListener("click", load);
  load();
}

// Consecutive rows from one owner, language, version and track become one group.
function groups(rows) {
  const out = [];
  for (const r of rows) {
    const last = out.at(-1);
    const same = last && last.owner === r.git_repo?.username && last.lang === r.language?.ietf_code && last.rt === r.resource_type && last.track === r.type;
    if (same) last.rows.push(r);
    else out.push({ owner: r.git_repo?.username ?? r.name.split("/")[0], lang: r.language?.ietf_code, rt: r.resource_type, track: r.type, rows: [r] });
  }
  return out;
}

function row(g) {
  const first = g.rows[0];
  const lang = first.language;
  const statuses = [...new Set(g.rows.map(statusOf))];
  return html`<li class="feed-row">
    <span class="avatar" aria-hidden="true">${g.owner.slice(0, 1).toUpperCase()}</span>
    <div class="feed-body">
      <div class="feed-line">
        <b>${g.owner}</b> updated
        ${lang ? html`<a href="#/l/${lang.ietf_code}">${lang.english_name}</a>` : "an unknown language"}
        <a class="pill" href="#/l/${g.lang}/${g.rt}/${g.track}">${g.rt} · ${g.track}</a>
        ${statuses.map(chip)}
      </div>
      <div class="feed-books">${g.rows.map((r) => {
        const book = bookFromRepoName(r.git_repo?.repo_name ?? r.name);
        const path = repoPath(r);
        const label = book ? book.usfm : (r.git_repo?.repo_name ?? r.name);
        return path ? html`<a class="book-chip" href="#/r/${path}" title="${path}">${label}</a>` : html`<span class="book-chip" title="${r.name}">${label}</span>`;
      })}</div>
    </div>
    <span class="small when" title="${first.modified_on}">${ago(first.modified_on)}</span>
  </li>`;
}

