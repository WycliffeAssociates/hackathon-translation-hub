import { lintUsfm, tally } from "../check.js";
import { counts, failure, html, render } from "../ui.js";

const SAMPLE = String.raw`\id JUD
\c 1
\p
\v 1 Jude, a servant of Jesus Christ \add and brother of James,
\v 3 Beloved, I was very eager to write to you \f + \ft about our common salvation.
\v 3 I found it necessary to write to you.
`;

// Paste any USFM and check it. Needs no network once the page has loaded,
// so it is the fallback when WACS is throttling.
export function checkPage(el) {
  render(el, html`
    <h1>Check USFM</h1>
    <p class="lede">Paste a book or a chapter. scripture-kitchen's structural lint runs here in the browser — the same engine the hub runs in Rust on the server.</p>
    <textarea class="usfm-input" spellcheck="false">${SAMPLE}</textarea>
    <p><button data-run>Check</button> <span class="small">Checks as you type, too.</span></p>
    <div data-out></div>`);
  const input = el.querySelector("textarea");
  const out = el.querySelector("[data-out]");
  const run = async () => {
    try {
      const rows = await lintUsfm(input.value);
      const t = tally(rows);
      render(out, html`<p>${counts(t)}</p>
        <table class="findings"><tbody>${rows.map((r) => html`<tr><td><span class="sev ${r.severity}">${r.severity}</span></td>
          <td class="mono">${r.chapter ? `${r.chapter}:${r.verse}` : `line ${r.line}`}</td><td>${r.message}<div class="small mono">${r.name}</div></td></tr>`)}</tbody></table>`);
    } catch (e) {
      render(out, failure(e));
    }
  };
  let timer;
  input.addEventListener("input", () => (clearTimeout(timer), (timer = setTimeout(run, 300))));
  el.querySelector("[data-run]").addEventListener("click", run);
  run();
}
