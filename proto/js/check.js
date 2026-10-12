// The scripture-kitchen structural lint (onion), running in the browser.
// Same engine the hub would run server-side in Rust; here it is the committed
// v0.1.10 `pkg-web` build plus its reader, vendored under /vendor/onion.
import init, { parse, setExtensions } from "../vendor/onion/onion_wasm.js";
import { reader, declaredVersion } from "../vendor/onion/reader.js";

let ready = null;
const load = () => (ready ??= init({ module_or_path: new URL("../vendor/onion/onion_wasm_bg.wasm", import.meta.url) }));

/**
 * Findings for one USFM text, as plain rows the page can render.
 * `utf16` offsets so `text.slice` lines up with JS strings.
 */
// Sefer's legacy-marker policy (src/app/legacyMarkers.ts): `\s5`, en_ulb's
// chunk marker, is registered as a standalone ONLY when the text already has
// it alone on a line — so it is not reported a hundred times per book, but a
// stray one in a text that never used it still is. Registration is process-wide.
const LEGACY = [{ name: "s5", category: "standalone", pattern: /^\\s5[ \t]*\r?$/m }];
let installed = "";
function legacyMarkersFor(text) {
  const found = LEGACY.filter((m) => m.pattern.test(text)).map(({ name, category }) => ({ name, category }));
  const list = JSON.stringify(found);
  if (list !== installed) {
    setExtensions(list, { relaxZPrefix: true });
    installed = list;
  }
}

export async function lintUsfm(text) {
  await load();
  legacyMarkersFor(text);
  const dish = reader(parse).parse(text, { diagnostics: true, toc: true, utf16: true });
  const version = declaredVersion(dish);
  const slice = (from, to) => text.slice(from, to);
  const rows = [];
  for (const d of dish.diagnostics) {
    const code = d.code();
    const severity = d.severity(version);
    // "form" codes are formatter hints, not findings — the hub ranks without them.
    if (!severity || severity === "form") continue;
    const at = d.locate(dish.toc);
    const span = d.span();
    rows.push({
      name: code.name,
      category: code.category,
      severity,
      message: d.message(slice),
      chapter: at?.chapter ?? null,
      verse: at?.verse ?? null,
      line: text.slice(0, span.from).split("\n").length,
      excerpt: lineAround(text, span.from),
    });
  }
  return rows;
}

function lineAround(text, at) {
  const start = text.lastIndexOf("\n", at - 1) + 1;
  const end = text.indexOf("\n", at);
  return text.slice(start, end === -1 ? undefined : end).slice(0, 160);
}

/** Verse numbers present per chapter: Map<chapter, Set<verse>>. Bridges count every verse they cover. */
export async function versesByChapter(text) {
  await load();
  const { toc } = reader(parse).parse(text, { toc: true, utf16: true });
  const out = new Map();
  for (const v of toc.verses()) {
    if (!v.chapter) continue;
    const set = out.get(v.chapter) ?? out.set(v.chapter, new Set()).get(v.chapter);
    const members = v.members.length ? v.members : [{ from: v.first, to: v.last }];
    for (const m of members) for (let n = m.from, end = Math.min(m.to || m.from, m.from + 200); n <= end; n++) set.add(n);
  }
  return out;
}

/**
 * Target against source, per chapter: how many of the source's verses the
 * target has, which are missing, and any extra (versification differences).
 */
export function coverage(target, source, chapters) {
  return chapters.map((ch) => {
    const want = source.get(ch) ?? new Set();
    const have = target.get(ch) ?? new Set();
    const missing = [...want].filter((v) => !have.has(v)).sort((a, b) => a - b);
    const extra = [...have].filter((v) => !want.has(v)).length;
    return { chapter: ch, source: want.size, present: want.size - missing.length, missing, extra };
  });
}

/** [1,2,3,7,9,10] → "1–3, 7, 9–10" */
export function ranges(ns) {
  const out = [];
  for (let i = 0; i < ns.length; i++) {
    let j = i;
    while (j + 1 < ns.length && ns[j + 1] === ns[j] + 1) j++;
    out.push(i === j ? `${ns[i]}` : `${ns[i]}–${ns[j]}`);
    i = j;
  }
  return out.join(", ");
}

export function tally(rows) {
  const t = { error: 0, warning: 0, info: 0 };
  for (const r of rows) t[r.severity] = (t[r.severity] ?? 0) + 1;
  return t;
}
