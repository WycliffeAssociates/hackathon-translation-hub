// Diagnostics probe: counts engine diagnostics vs drawn lint marks per book and mode.
import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
const require = createRequire("/Users/willkelly/Documents/Work/Code/Sefer/package.json");
const { chromium } = require("playwright");
const root = fileURLToPath(new URL("..", import.meta.url));
const server = spawn("npx", ["vite", "preview", "--port", "4175", "--strictPort"], { cwd: root, stdio: ["ignore", "pipe", "inherit"] });
await new Promise((r) => server.stdout.on("data", (d) => String(d).includes("Local") && r()));
const browser = await chromium.launch();
try {
  for (const book of ["08-RUT-broken", "19-PSA", "08-RUT"]) {
    for (const mode of ["regular", "usfm"]) {
      const page = await browser.newPage({ viewport: { width: 1200, height: 900 } });
      await page.goto(`http://localhost:4175/?book=${book}&mode=${mode}`);
      await page.waitForFunction(() => window.__timings !== undefined);
      await page.waitForTimeout(800);
      const r = await page.evaluate(async () => {
        const { view } = window.__viewer;
        // reach the forked linter's state through CodeMirror's own API
        const lint = await import("/node_modules/.vite-none").catch(() => null);
        return {
          marks: document.querySelectorAll(".cm-lintRange").length,
          slots: document.querySelectorAll(".usfm-lint-slot").length,
          classes: [...new Set([...document.querySelectorAll("[class*=lint]")].map((e) => e.className))].slice(0, 8),
        };
      });
      console.log(book, mode, JSON.stringify(r));
      if (book === "08-RUT-broken") {
        await page.screenshot({ path: `${root}/dist-shots/${book}-${mode}.png` });
        const first = page.locator(".cm-lintRange").first();
        if (await first.count()) { await first.hover(); await page.waitForTimeout(700); await page.screenshot({ path: `${root}/dist-shots/${book}-${mode}-hover.png` }); }
      }
      await page.close();
    }
  }
} finally { await browser.close(); server.kill(); }
