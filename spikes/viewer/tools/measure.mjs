// Headless Chrome measurement of the built viewer.
// Usage: pnpm build && node tools/measure.mjs   (starts `vite preview` itself)
// Playwright is borrowed from Sefer's node_modules (read-only; nothing runs there).
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire("/Users/willkelly/Documents/Work/Code/Sefer/package.json");
const { chromium } = require("playwright");

const root = fileURLToPath(new URL("..", import.meta.url));
const shots = `${root}/dist-shots`;
mkdirSync(shots, { recursive: true });
const PORT = 4174;
const server = spawn("npx", ["vite", "preview", "--port", String(PORT), "--strictPort"], {
  cwd: root,
  stdio: ["ignore", "pipe", "inherit"],
});
await new Promise((resolve) => server.stdout.on("data", (d) => String(d).includes("Local") && resolve()));

const browser = await chromium.launch();
const out = {};
try {
  for (const book of ["19-PSA", "08-RUT"]) {
    const runs = [];
    for (let i = 0; i < 5; i++) {
      const ctx = await browser.newContext({ viewport: { width: 1200, height: 900 } }); // cold cache
      const page = await ctx.newPage();
      if (process.env.THROTTLE) {
        const cdp = await ctx.newCDPSession(page);
        await cdp.send("Emulation.setCPUThrottlingRate", { rate: Number(process.env.THROTTLE) });
      }
      const errors = [];
      page.on("pageerror", (e) => errors.push(String(e)));
      page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
      await page.goto(`http://localhost:${PORT}/?book=${book}`);
      await page.waitForFunction(() => window.__timings !== undefined, null, { timeout: 30000 });
      const t = await page.evaluate(() => window.__timings);
      if (errors.length) t.errors = errors;
      runs.push(t);
      if (i === 0) {
        await page.screenshot({ path: `${shots}/${book}-regular.png` });
        // diagnostics drawn (linter has a 150 ms delay)
        await page.waitForTimeout(600);
        t.lintMarks = await page.locator(".cm-lintRange").count();
        // read-only: try to type
        const before = await page.evaluate(() => window.__viewer.view.state.doc.length);
        await page.locator(".cm-content").click({ position: { x: 50, y: 50 } });
        await page.keyboard.type("XYZ");
        await page.keyboard.press("Backspace");
        await page.keyboard.press("Enter");
        t.docUnchangedAfterTyping =
          before === (await page.evaluate(() => window.__viewer.view.state.doc.length));
        t.contentEditable = await page.locator(".cm-content").getAttribute("contenteditable");
        // usfm mode
        const inPage = (code) =>
          page.evaluate(async (code) => {
            const v = window.__viewer;
            const s = performance.now();
            new Function("v", code)(v);
            await new Promise((r) => requestAnimationFrame(() => setTimeout(r, 0)));
            return performance.now() - s;
          }, code);
        t.inPageToggleToUsfmMs = await inPage("v.setMode('usfm')");
        t.inPageToggleToRegularMs = await inPage("v.setMode('regular')");
        t.inPageClipCh23Ms = await inPage("v.clipTo(Math.min(23, v.chapters().length - 1))");
        t.inPageUnclipMs = await inPage("v.clipTo(null)");
        t.inPageJumpToLastChapterMs = await inPage("v.scrollTo(v.chapters().length - 1)");
        await inPage("v.scrollTo(0)");
        let m = performance.now();
        await page.click("#mode-usfm");
        await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => setTimeout(r, 0))));
        t.toggleToUsfmMs = performance.now() - m;
        await page.screenshot({ path: `${shots}/${book}-usfm.png` });
        m = performance.now();
        await page.click("#mode-regular");
        await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => setTimeout(r, 0))));
        t.toggleToRegularMs = performance.now() - m;
        // chapter clip
        const options = await page.locator("#chapter option").count();
        const pick = Math.min(23, options - 1);
        m = performance.now();
        await page.selectOption("#chapter", { index: pick });
        await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => setTimeout(r, 0))));
        t.clipMs = performance.now() - m;
        t.clipVisibleLines = await page.locator(".cm-line").count();
        await page.screenshot({ path: `${shots}/${book}-clip.png` });
        // jump without clipping (whole book, scroll to chapter)
        await page.uncheck("#clip");
        await page.waitForTimeout(100);
        await page.screenshot({ path: `${shots}/${book}-jump.png` });
        await page.selectOption("#chapter", { index: 0 });
        // scroll smoothness: scroll the whole book in steps, record frame gaps
        t.scroll = await page.evaluate(async () => {
          const sc = document.querySelector(".cm-scroller");
          const gaps = [];
          let last = performance.now();
          const max = sc.scrollHeight;
          const step = 600;
          for (let y = 0; y < max; y += step) {
            sc.scrollTop = y;
            await new Promise((r) => requestAnimationFrame(r));
            const now = performance.now();
            gaps.push(now - last);
            last = now;
          }
          gaps.sort((a, b) => a - b);
          const pct = (p) => gaps[Math.min(gaps.length - 1, Math.floor(gaps.length * p))];
          return {
            frames: gaps.length,
            scrollHeight: max,
            p50: pct(0.5),
            p95: pct(0.95),
            max: gaps[gaps.length - 1],
            over50ms: gaps.filter((g) => g > 50).length,
          };
        });
        await page.screenshot({ path: `${shots}/${book}-end.png` });
      }
      await ctx.close();
    }
    out[book] = runs;
  }
} finally {
  await browser.close();
  server.kill();
}
const med = (xs) => xs.slice().sort((a, b) => a - b)[Math.floor(xs.length / 2)];
for (const [book, runs] of Object.entries(out)) {
  console.log(`\n${book}: first run`, JSON.stringify(runs[0], null, 1));
  for (const k of ["engineBootMs", "fetchMs", "mountMs", "firstPaintAfterMountMs", "sincePageStartMs", "navStartToPaintMs"])
    console.log(`  median ${k}: ${med(runs.map((r) => r[k])).toFixed(1)}`);
}
