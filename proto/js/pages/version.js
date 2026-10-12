import { language } from "../api.js";
import { BOOKS } from "../books.js";
import { bookState, byStatus, officialOf, repoPath, statusOf, versionsOf } from "../domain.js";
import { ago, chip, consolidated, failure, html, legend, loading, render, shelf, unsure } from "../ui.js";

// One Version + Track: the full shelf, and per book every candidate repo with
// the one the hub would call Official.
export async function versionPage(el, { ietf, rtype, track }, query) {
  render(el, loading(`${ietf} ${rtype}`));
  let data;
  try {
    data = await language(ietf);
  } catch (e) {
    return render(el, failure(e));
  }
  const lang = data.language;
  const version = versionsOf(data.content).find((v) => v.rtype === rtype);
  const t = version?.tracks.get(track);
  if (!lang || !t) return render(el, html`<h1>Nothing for ${ietf} · ${rtype} · ${track}</h1><p><a href="#/l/${ietf}">Back to the language</a></p>`);
  const focus = query.get("book");
  const base = `#/l/${ietf}/${rtype}/${track}`;
  const books = BOOKS.filter((b) => t.books.has(b.slug));

  render(el, html`
    <nav class="crumbs"><a href="#/search">Search</a> › <a href="#/l/${ietf}">${lang.english_name}</a> › ${version.name} · ${track}</nav>
    <header class="page-head">
      <h1>${lang.english_name} · ${version.name} <span class="native">${track}</span></h1>
      <p class="facts"><code>${ietf}/${rtype}/${track}</code><span>${books.length} of 66 books have work</span></p>
    </header>
    ${legend()}
    ${consolidated(t)}
    ${shelf(t, (b, rows) => (rows ? `${base}?book=${b.slug}` : null), { big: true })}

    <section class="books">
      <h2>Books</h2>
      <table class="candidates">
        <thead><tr><th>Book</th><th>Official repo</th><th>Other candidates</th></tr></thead>
        <tbody>${books.map((b) => {
          const rows = t.books.get(b.slug);
          const off = officialOf(rows);
          const others = rows.filter((r) => r !== off);
          return html`<tr id="book-${b.slug}" class="${focus === b.slug ? "focus" : ""}">
            <th><span class="cell ${bookState(rows)} mini"></span>${b.name}</th>
            <td>${off ? html`${repoLink(off)} <span class="small">${ago(off.modified_on)}</span>` : html`<span class="small">none — all Inactive</span>`}</td>
            <td>${others.length ? html`<details class="others"><summary>${others.length} other${others.length > 1 ? "s" : ""}${tally(others)}</summary>
              ${others.toSorted(byStatus).map((r) => html`<div>${repoLink(r)}</div>`)}</details>` : html`<span class="small">—</span>`}</td>
          </tr>`;
        })}</tbody>
      </table>
      <p class="small">Official = the only repo; else the Primary; else the newest Active. Same rule as Sefer's <code>defaultCandidate</code>.</p>
    </section>

    <div class="grid2">
      ${unsure("gateway source", "Translating from")}
    </div>`);

  if (focus) el.querySelector(`#book-${CSS.escape(focus)}`)?.scrollIntoView({ block: "center" });
}

// A repo with its status; whole-Bible repos say so. A catalog row can lack a
// repo (deleted or renamed upstream), so fall back to its catalog name.
const repoLink = (r) => {
  const path = repoPath(r);
  return html`${path ? html`<a class="mono" href="#/r/${path}">${path}</a>` : html`<span class="mono small">${r.name} (no repo)</span>`} ${chip(statusOf(r))}${r.books ? html` <span class="tag">whole Bible</span>` : ""}`;
};

const tally = (rows) => {
  const n = (s) => rows.filter((r) => statusOf(r) === s).length;
  const parts = [["Primary", n("Primary")], ["Active", n("Active")], ["Inactive", n("Inactive")]].filter(([, k]) => k);
  return html`<span class="small"> · ${parts.map(([s, k]) => `${k} ${s}`).join(", ")}</span>`;
};
