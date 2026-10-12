import { language } from "../api.js";
import { trackSummary, versionsOf } from "../domain.js";
import { consolidated, failure, html, legend, loading, render, shelf, statusCounts, unsure } from "../ui.js";

export async function languagePage(el, { ietf }) {
  render(el, loading(ietf));
  let data;
  try {
    data = await language(ietf);
  } catch (e) {
    return render(el, failure(e));
  }
  const { language: lang, content } = data;
  if (!lang) return render(el, html`<h1>No language “${ietf}”</h1><p><a href="#/search?q=${ietf}">Search for it</a></p>`);
  const versions = versionsOf(content);
  const gateways = lang.gateway_languages.map((g) => g.gateway_language_ietf);
  const dependents = lang.dependent_languages_aggregate.aggregate.count;
  const c = statusCounts(content);

  render(el, html`
    <nav class="crumbs"><a href="#/search">Search</a> › ${lang.english_name}</nav>
    <header class="page-head" dir="auto">
      <h1>${lang.english_name}${lang.national_name && lang.national_name !== lang.english_name ? html` <span class="native">${lang.national_name}</span>` : ""}</h1>
      <p class="facts">
        <code>${lang.ietf_code}</code>
        ${lang.wa_language_metadata?.is_gateway ? html`<span class="tag">Gateway · ${dependents} languages depend on it</span>` : ""}
        ${gateways.length ? html`<span>Gateway: ${gateways.map((g, i) => html`${i ? ", " : ""}<a href="#/l/${g}">${g}</a>`)}</span>` : ""}
        <span>${content.length} scripture repos · ${c.Primary} Primary · ${c.Active} Active · ${c.Inactive} Inactive</span>
      </p>
    </header>

    ${versions.length ? legend() : html`<p>No scripture repos in the catalog.</p>`}
    ${versions.map((v) => html`
      <section class="version">
        <h2>${v.name} <code>${v.rtype}</code></h2>
        ${[...v.tracks.values()].map((t) => {
          const s = trackSummary(t);
          return html`<div class="track">
            <div class="track-head">
              <a class="track-name" href="#/l/${lang.ietf_code}/${v.rtype}/${t.track}">${t.track} track →</a>
              <span class="small">${s.official} official · ${s.draft} in progress${s.conflict ? html` · <span class="bad">${s.conflict} with two Primaries</span>` : ""}</span>
            </div>
            ${consolidated(t)}
            ${shelf(t, (b, rows) => (rows ? `#/l/${lang.ietf_code}/${v.rtype}/${t.track}?book=${b.slug}` : null))}
          </div>`;
        })}
      </section>`)}

    <div class="grid2">
      ${unsure("bulletins", "Bulletins")}
      ${unsure("releases", "Releases")}
    </div>`);
}
