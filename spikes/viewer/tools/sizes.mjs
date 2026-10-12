// Bundle size by part: raw / gzip / brotli per chunk, and minified bytes per
// source module (source-map-explorer), grouped into the parts the spike asks about.
// Usage: pnpm build && node tools/sizes.mjs
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { brotliCompressSync, gzipSync, constants } from "node:zlib";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const dir = `${root}/dist/assets`;
const kb = (n) => (n / 1024).toFixed(1);
const comp = (buf) => ({
  raw: buf.length,
  gz: gzipSync(buf, { level: 9 }).length,
  br: brotliCompressSync(buf, { params: { [constants.BROTLI_PARAM_QUALITY]: 11 } }).length,
});

console.log("## Chunks (as emitted)\n\n| file | raw KB | gzip KB | brotli KB |\n|---|---:|---:|---:|");
const total = { raw: 0, gz: 0, br: 0 };
for (const f of readdirSync(dir).filter((f) => !f.endsWith(".map")).sort()) {
  const c = comp(readFileSync(`${dir}/${f}`));
  for (const k in total) total[k] += c[k];
  console.log(`| ${f} | ${kb(c.raw)} | ${kb(c.gz)} | ${kb(c.br)} |`);
}
console.log(`| **total** | ${kb(total.raw)} | ${kb(total.gz)} | ${kb(total.br)} |`);

// Per-module attribution of minified JS bytes.
const js = readdirSync(dir).filter((f) => f.endsWith(".js"));
const json = JSON.parse(
  execFileSync(`${root}/node_modules/.bin/source-map-explorer`, [...js.map((f) => `${dir}/${f}`), "--json", "--no-border-checks"], {
    encoding: "utf8",
    maxBuffer: 1 << 26,
  }),
);
const part = (file) => {
  if (/node_modules\/(@codemirror|@lezer|style-mod|w3c-keyname|crelt|@marijn)/.test(file)) return "CodeMirror";
  if (/node_modules\/effect\//.test(file)) return "Effect";
  if (/solid/.test(file)) return "Solid";
  if (/scripture-kitchen\/galley\/pkg-web/.test(file)) return "galley wasm-bindgen glue";
  if (/scripture-kitchen\/onion-wasm\/reader/.test(file)) return "onion reader (Dish decoder)";
  if (/src\/editor-fork\//.test(file)) return "editor fork";
  if (/src\/host\//.test(file)) return "host shims (#core copies + engine)";
  if (/src\/(viewer|demo)\//.test(file)) return "viewer + demo";
  return "other";
};
const parts = new Map();
const modules = [];
for (const r of json.results) {
  for (const [file, { size }] of Object.entries(r.files)) {
    if (file.startsWith("[")) continue;
    const p = part(file);
    parts.set(p, (parts.get(p) ?? 0) + size);
    modules.push([size, file.replace(/.*node_modules\//, "nm/").replace(/.*spikes\/viewer\//, "")]);
  }
}
// Estimate compressed share per part by compressing the concatenated module
// sources is wrong (unminified); instead scale the chunk's gzip ratio. Good
// enough for a spike: report raw minified bytes per part, and the measured
// compressed size of each emitted chunk above.
console.log("\n## Minified JS by part (source-map attribution)\n\n| part | min KB |\n|---|---:|");
for (const [p, s] of [...parts].sort((a, b) => b[1] - a[1])) console.log(`| ${p} | ${kb(s)} |`);
console.log("\n## Editor-fork modules (min bytes)\n");
for (const [s, f] of modules.filter(([, f]) => f.includes("editor-fork")).sort((a, b) => b[0] - a[0]))
  console.log(`${String(s).padStart(7)}  ${f}`);
console.log("\n## Top other modules\n");
for (const [s, f] of modules.filter(([, f]) => !f.includes("editor-fork")).sort((a, b) => b[0] - a[0]).slice(0, 15))
  console.log(`${String(s).padStart(7)}  ${f}`);
