// Drive the built app like a user, against a running hub-server.
// Usage: node tools/smoke.mjs [base-url]     (default http://127.0.0.1:8788)
// Starts nothing. Playwright is borrowed from Sefer's node_modules (read-only),
// as spikes/viewer/tools/measure.mjs does. Screenshots go to web/smoke-shots/.
import { mkdirSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire(process.env.PLAYWRIGHT_FROM ?? "/Users/willkelly/Documents/Work/Code/Sefer/package.json");
const { chromium } = require("playwright");

const base = (process.argv[2] ?? "http://127.0.0.1:8788").replace(/\/$/, "");
const shots = fileURLToPath(new URL("../smoke-shots/", import.meta.url));
mkdirSync(shots, { recursive: true });

const browser = await chromium.launch();
const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
const page = await context.newPage();

const problems = [];
page.on("console", (m) => m.type() === "error" && problems.push(`console: ${m.text()}`));
page.on("pageerror", (e) => problems.push(`pageerror: ${e.message}`));
page.on("response", (r) => {
  const url = new URL(r.url());
  if (url.pathname.startsWith("/v1/") && r.status() >= 400) problems.push(`HTTP ${r.status()} ${url.pathname}${url.search}`);
});
const headers = [];
page.on("request", (r) => r.url().includes("/v1/") && headers.push(r.headers()));

const results = [];
async function step(name, fn) {
  const before = problems.length;
  const t0 = Date.now();
  try {
    await fn();
    const fresh = problems.slice(before);
    results.push({ name, ok: fresh.length === 0, ms: Date.now() - t0, notes: fresh });
  } catch (e) {
    results.push({ name, ok: false, ms: Date.now() - t0, notes: [String(e.message ?? e).split("\n")[0], ...problems.slice(before)] });
    await page.screenshot({ path: `${shots}${name}-FAILED.png`, fullPage: false }).catch(() => {});
  }
}
const expect = (cond, what) => {
  if (!cond) throw new Error(`expected ${what}`);
};
const shot = (name, fullPage = false) => page.screenshot({ path: `${shots}${name}.png`, fullPage });

await step("home", async () => {
  await page.goto(`${base}/`);
  await page.locator(".feed-row").first().waitFor();
  expect((await page.locator(".feed-row").count()) >= 10, "at least 10 repos in the feed");
  await page.getByRole("button", { name: "synthetic only" }).click();
  await page.waitForURL(/synthetic=true/);
  await page.locator(".tag.syn").first().waitFor();
  await shot("01-home");
  await page.getByRole("button", { name: "all", exact: true }).click();
  await page.waitForURL((u) => !u.search.includes("synthetic"));
});

await step("repo-en_ulb", async () => {
  await page.goto(`${base}/repos/wa-catalog/en_ulb`);
  await page.getByRole("heading", { level: 1, name: "Unlocked Literal Bible" }).waitFor();
  await page.locator(".books tbody tr").first().waitFor();
  expect((await page.locator(".books tbody tr").count()) === 66, "66 book rows");
  await page.locator(".tree .file", { hasText: /^41-MAT\.usfm$/ }).click();
  await page.waitForURL(/file=41-MAT\.usfm/);
  await page.locator(".viewer-pane .cm-editor .cm-content").waitFor();
  await page.waitForFunction(() => (document.querySelector(".viewer-pane .cm-content")?.textContent ?? "").includes("Jesus"));
  const regular = await page.locator(".viewer-pane .cm-content").innerText();
  expect(!regular.includes("\\v "), "regular mode to hide markers");
  await shot("02-repo-en_ulb-MAT-regular");
  await page.getByRole("button", { name: "USFM", exact: true }).click();
  await page.waitForFunction(() => (document.querySelector(".viewer-pane .cm-content")?.textContent ?? "").includes("\\v "));
  await shot("03-repo-en_ulb-MAT-usfm");
  await page.getByRole("button", { name: "Regular", exact: true }).click();
  await page.getByRole("combobox").selectOption("5");
  await page.waitForTimeout(300);
  await shot("04-repo-en_ulb-MAT-chapter5");
});

await step("repo-en_ulb-finding", async () => {
  const group = page.locator(".proof-group").first();
  await group.locator("summary").click();
  const link = group.locator("button.linkish").first();
  const where = await link.innerText();
  await link.click();
  await page.waitForURL(/at=\d+-\d+/);
  await page.locator(".viewer-pane .cm-content").waitFor();
  // The finding's range is selected and its underline drawn.
  await page.locator(".viewer-pane .cm-selectionBackground").first().waitFor();
  await page.waitForTimeout(300);
  const url = new URL(page.url());
  results.push({ name: `  finding ${where} → ${url.searchParams.get("file")} @ ${url.searchParams.get("at")}`, ok: true, ms: 0, notes: [] });
  await shot("05-repo-en_ulb-finding");
  await page.locator(".books").scrollIntoViewIfNeeded();
  await shot("06-repo-en_ulb-books");
});

await step("repo-coverage", async () => {
  await page.goto(`${base}/repos/wa-catalog/ru_ulb`);
  const psa = page.locator(".books tbody tr", { has: page.locator("th", { hasText: /^PSA$/ }) });
  await psa.waitFor();
  await psa.locator("details summary").click();
  await psa.evaluate((el) => (el.scrollIntoView({ block: "start" }), scrollBy(0, -120)));
  await shot("07a-repo-coverage-gaps");
});

await step("repo-synthetic", async () => {
  const res = await page.request.get(`${base}/v1/repos?synthetic=true&limit=100`);
  const list = (await res.json()).repos.filter((r) => r.book_count > 0);
  let pick = null;
  for (const r of list) {
    const d = await (await page.request.get(`${base}/v1/repos/${r.owner}/${r.name}`)).json();
    if (d.books.some((b) => b.verses > 0)) {
      pick = d;
      break;
    }
  }
  expect(pick, "a synthetic repo with verses");
  await page.goto(`${base}/repos/${pick.owner}/${pick.name}`);
  await page.getByText("Not a real repo.").waitFor();
  const file = pick.synthetic.file;
  await page.locator(".tree .file", { hasText: file }).click();
  await page.locator(".viewer-pane .cm-content").waitFor();
  await shot("07-synthetic");
  await page.getByLabel("Show the original Writer files").check();
  await page.waitForURL(/source=true/);
  await page.locator(".tree details").first().waitFor();
  await page.locator(".tree details .file", { hasText: /\.txt$/ }).first().click();
  await page.locator(".viewer pre.code").waitFor();
  expect((await page.locator(".viewer pre.code").innerText()).length > 0, "a Writer chunk as text");
  await shot("08-synthetic-source-files");
  results.push({ name: `  synthetic repo: ${pick.owner}/${pick.name} (${file})`, ok: true, ms: 0, notes: [] });
});

await step("language-ceb", async () => {
  await page.goto(`${base}/languages/ceb`);
  await page.getByRole("heading", { level: 1, name: /Cebuano/ }).waitFor();
  await page.locator(".books tbody tr").first().waitFor();
  expect((await page.locator(".feed-row").count()) > 0, "repos listed");
  await shot("09-language-ceb");
});

await step("search", async () => {
  await page.goto(`${base}/search?q=cebuano`);
  await page.locator(".results li").first().waitFor();
  await shot("10-search");
});

await step("biel", async () => {
  await page.goto(`${base}/biel`);
  await page.locator(".conflict").first().waitFor();
  expect((await page.locator(".conflict").count()) > 0, "conflicts grouped by language");
  await shot("11-biel");
});

await step("status", async () => {
  await page.goto(`${base}/status`);
  await page.locator(".stat").first().waitFor();
  expect((await page.locator(".stat").count()) === 9, "nine totals");
  expect(await page.getByRole("link", { name: "/v1/openapi.json" }).isVisible(), "the OpenAPI link");
  await shot("12-status");
});

await browser.close();

// Every /v1 call carries the two headers.
const bad = headers.filter((h) => !/^00-[0-9a-f]{32}-[0-9a-f]{16}-01$/.test(h.traceparent ?? "") || !h["x-hub-page"]);
results.push({ name: `headers on ${headers.length} /v1 calls`, ok: bad.length === 0 && headers.length > 0, ms: 0, notes: bad.slice(0, 3).map((h) => JSON.stringify(h)) });

for (const r of results) console.log(`${r.ok ? "ok  " : "FAIL"} ${r.name}${r.ms ? ` (${r.ms} ms)` : ""}${r.notes.map((n) => `\n       ${n}`).join("")}`);
console.log(`screenshots: ${shots}`);
process.exit(results.every((r) => r.ok) ? 0 : 1);
