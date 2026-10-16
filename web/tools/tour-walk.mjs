// Walk a guided tour end to end, as a presenter would, and screenshot each
// stop: the check to run before a demo, on whichever hub it will be given on.
// Usage: node tools/tour-walk.mjs [base-url] [tour-id]   (default http://127.0.0.1:8788 demo)
// Starts nothing, writes nothing on the hub. Playwright is borrowed from
// Sefer's node_modules, as in smoke.mjs. Screenshots go to web/tour-shots/.
import { mkdirSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire(process.env.PLAYWRIGHT_FROM ?? "/Users/willkelly/Documents/Work/Code/Sefer/package.json");
const { chromium } = require("playwright");

const base = (process.argv[2] ?? "http://127.0.0.1:8788").replace(/\/$/, "");
const tour = process.argv[3] ?? "demo";
const shots = fileURLToPath(new URL(`../tour-shots/${new URL(base).host.replace(/[:.]/g, "_")}/`, import.meta.url));
mkdirSync(shots, { recursive: true });

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
const warnings = [];
page.on("console", (m) => m.type() === "warning" && m.text().includes("tour") && warnings.push(m.text()));

await page.goto(`${base}/?tour=${tour}`);
let failed = 0;
for (let n = 1; ; n++) {
  // A step is done waiting when its card shows: its own, or "could not be shown".
  const card = page.locator(".tour-card");
  await card.waitFor({ timeout: 30_000 });
  await page.waitForTimeout(600); // let charts and the ring settle for the picture
  const title = await card.locator("h2").innerText();
  const at = await card.locator(".tour-at").innerText();
  const missed = title === "This step could not be shown";
  if (missed) failed++;
  console.log(`${missed ? "MISS" : "ok  "} ${at.replace(/\s*[▾▴]/, "")}  ${missed ? await card.locator("p").innerText() : title}`);
  await page.screenshot({ path: `${shots}${String(n).padStart(2, "0")}.png` });
  const next = card.locator(".tour-buttons .primary");
  if ((await next.innerText()) === "Done") break;
  await next.click();
  await card.waitFor({ state: "detached", timeout: 5_000 }).catch(() => {});
}
for (const w of warnings) console.log(`  warn: ${w}`);
console.log(`${failed ? `${failed} stop(s) missed` : "every stop shown"} · shots in ${shots}`);
await browser.close();
process.exit(failed ? 1 : 0);
