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
});

// The chapter navigator: ‹ [Chapter n of N ▾] › plus a grid, following the scroll.
const nav = {
  now: () => page.locator(".chnav-open .chnav-now").innerText(),
  waitNow: (want) => page.waitForFunction((w) => {
    const t = document.querySelector(".chnav-open .chnav-now")?.textContent ?? "";
    return typeof w === "string" ? t === w : /^\d+$/.test(t) && Number(t) > w;
  }, want),
  open: async () => {
    await page.getByRole("button", { name: /Choose a chapter$/ }).click();
    const pop = page.getByRole("dialog", { name: "Chapters" });
    await pop.waitFor();
    return pop;
  },
  pane: () => page.locator(".viewer-pane .cm-scroller"),
};

await step("chapter-nav", async () => {
  const pop = await nav.open();
  expect((await pop.locator(".chnav-grid .chn").count()) === 28, "28 chapter cells for MAT");
  await pop.getByRole("button", { name: /^Chapter 5( ·|$)/ }).click();
  await pop.waitFor({ state: "detached" });
  await nav.waitNow("5");
  await page.waitForTimeout(400);
  const top = await nav.pane().evaluate((el) => el.scrollTop);
  expect(top > 1000, `the pane scrolled to chapter 5 (scrollTop ${top})`);
  // A nudge ends the "where you asked to go" pin; the scroll watcher must agree it is 5.
  await nav.pane().evaluate((el) => el.scrollBy(0, 8));
  await page.waitForTimeout(200);
  expect((await nav.now()) === "5", `the scroll watcher reads chapter 5 (got ${await nav.now()})`);
  await shot("04-repo-en_ulb-MAT-chapter5");
  // Read on: the indicator follows the scroll.
  await nav.pane().evaluate((el) => el.scrollBy(0, 6000));
  await nav.waitNow(5);
  const after = await nav.now();
  results.push({ name: `  scrolled 6000px from MAT 5 → indicator says ${after}`, ok: true, ms: 0, notes: [] });
  // Keyboard, with focus in the text: ] and [ step; Alt+← steps back too.
  await page.locator(".viewer-pane").focus();
  await page.keyboard.press("]");
  await nav.waitNow(String(Number(after) + 1));
  await page.keyboard.press("[");
  await nav.waitNow(after);
  await page.keyboard.press("Alt+ArrowLeft");
  await nav.waitNow(String(Number(after) - 1));
  await page.getByRole("button", { name: "Next chapter" }).click();
  await nav.waitNow(after);
  // The grid by keyboard: Escape closes and gives focus back to the button.
  const again = await nav.open();
  await page.keyboard.press("Escape");
  await again.waitFor({ state: "detached" });
  expect(await page.locator(".chnav-open").evaluate((el) => el === document.activeElement), "focus back on the chapter button");
});

/** The picker open on a book, at desktop width and at 390px. */
async function pickerShots(name, url, chapters) {
  await page.goto(`${base}${url}`);
  await page.locator(".chnav-open").waitFor();
  for (const [w, h, tag] of [[1280, 900, "desktop"], [390, 844, "390"]]) {
    await page.setViewportSize({ width: w, height: h });
    await page.waitForTimeout(150);
    // The bar just under the sticky header, so the grid below it is in the shot.
    await page.locator(".viewer-bar").evaluate((el) => scrollTo(0, el.getBoundingClientRect().top + scrollY - document.querySelector("header.top").offsetHeight - 40));
    const pop = await nav.open();
    expect((await pop.locator(".chnav-grid .chn").count()) === chapters, `${chapters} chapter cells`);
    const box = await pop.boundingBox();
    expect(box && box.x >= 0 && box.x + box.width <= w, `the grid inside the ${w}px viewport`);
    await shot(`${name}-${tag}`);
    await page.keyboard.press("Escape");
  }
  await page.setViewportSize({ width: 1280, height: 900 });
}

await step("chapter-nav-books", async () => {
  await pickerShots("04b-picker-en_ulb-PSA", "/repos/wa-catalog/en_ulb?file=19-PSA.usfm", 150);
  await pickerShots("04c-picker-en_ulb-OBA", "/repos/wa-catalog/en_ulb?file=31-OBA.usfm", 1);
  // Marks: missing verses (ru_ulb Psalms) and lint tints (a Psalms repo with errors and warnings).
  await pickerShots("04d-picker-ru_ulb-PSA-gaps", "/repos/wa-catalog/ru_ulb?file=19-PSA.usfm", 150);
  const hav = await page.request.get(`${base}/v1/repos/faustin_azaza/hav_psa_text_reg`);
  if (hav.ok()) {
    const book = (await hav.json()).books[0];
    await pickerShots("04e-picker-lint-tints", `/repos/faustin_azaza/hav_psa_text_reg?file=${encodeURIComponent(book.path)}`, book.chapters);
  }
});

/** The findings table's rows: [code, engine, total]. */
const checkRows = () =>
  page.locator(".checks-table tbody tr.check-row").evaluateAll((trs) =>
    trs.map((tr) => [tr.querySelector("th button")?.textContent?.replace(/^[▸▾]\s*/, "").trim(), tr.querySelector(".tag")?.textContent, Number(tr.querySelector(".total")?.textContent?.replace(/\D/g, ""))]),
  );
const sortedByTotal = (rows) => rows.every((r, i) => i === 0 || rows[i - 1][2] >= r[2]);

await step("repo-en_ulb-findings", async () => {
  // Sous Chef runs on the server's first poll; its counts land a few seconds after.
  for (let i = 0; i < 60; i++) {
    const list = await (await page.request.get(`${base}/v1/repos/wa-catalog/en_ulb/findings`)).json();
    if (list.some((c) => c.engine === "sous")) break;
    await page.waitForTimeout(1000);
  }
  // The findings table is the Checks tab's.
  await page.goto(`${base}/repos/wa-catalog/en_ulb?tab=checks`);
  await page.locator(".checks-table tbody tr.check-row").first().waitFor();
  const rows = await checkRows();
  expect(rows.length > 0 && sortedByTotal(rows), `findings sorted by total (${rows.map((r) => r[2]).join(", ")})`);
  expect(rows.some((r) => r[1] === "lint") && rows.some((r) => r[1] === "sous"), "both engines in the table");
  results.push({ name: `  ${rows.length} checks; top ${rows.slice(0, 3).map((r) => `${r[0]} ${r[2]}`).join(", ")}`, ok: true, ms: 0, notes: [] });
  // Open the top Sous Chef check, then its noisiest book.
  const sous = page.locator(".checks-table tbody tr.check-row", { has: page.locator(".tag.eng-sous") }).first();
  const code = (await sous.locator("th button").innerText()).replace(/^[▸▾]\s*/, "").trim();
  await sous.click();
  const chip = page.locator(".checks-table tr.check-books button.book-chip").first();
  const book = (await chip.innerText()).split(/\s/)[0];
  await sous.scrollIntoViewIfNeeded();
  await shot("05a-repo-en_ulb-findings");
  await chip.click();
  await page.waitForURL(new RegExp(`check=sous(%3A|:)${code}`));
  await page.locator(".viewer-pane .cm-content").waitFor();
  // The rail beside the text has the check open; a row lights its exact span.
  const group = page.locator(".rail-group", { has: page.locator(`.rail-check[aria-expanded="true"] .code`, { hasText: new RegExp(`^${code}$`) }) });
  const first = group.locator(".rail-items button").first();
  await first.waitFor();
  const where = (await first.locator(".ref").innerText()).trim();
  await first.click();
  await page.locator(".viewer-pane .usfm-flash").first().waitFor();
  const lit = (await page.locator(".viewer-pane .usfm-flash").first().innerText()).trim();
  expect((await page.locator(".rail-items button.on").count()) === 1, "the picked row is marked");
  const url = new URL(page.url());
  results.push({ name: `  ${code} in ${book} ${where} → ${url.searchParams.get("file")}, lit "${lit.slice(0, 30)}"`, ok: true, ms: 0, notes: [] });
  await shot("05-repo-en_ulb-finding");
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator(".rail").scrollIntoViewIfNeeded();
  await shot("05b-repo-en_ulb-finding-390");
  await page.locator(".checks-table").scrollIntoViewIfNeeded();
  await shot("05c-repo-en_ulb-findings-390");
  await page.setViewportSize({ width: 1280, height: 900 });
  // The books table is on the Files tab.
  await page.goto(`${base}/repos/wa-catalog/en_ulb`);
  await page.locator(".books").scrollIntoViewIfNeeded();
  await shot("06-repo-en_ulb-books");
});

await step("checks", async () => {
  await page.goto(`${base}/checks`);
  await page.locator(".checks-table tbody tr.check-row").first().waitFor();
  const rows = await checkRows();
  expect(sortedByTotal(rows), "checks sorted by total");
  expect(rows[0][0] === "MisplacedFormat", `MisplacedFormat on top (got ${rows[0][0]})`);
  await page.locator(".checks-table tbody tr.check-row").first().click();
  const top = page.locator(".top-repos li a").first();
  await top.waitFor();
  expect((await top.innerText()) === "wa-catalog/km_ulb", `wa-catalog/km_ulb first (got ${await top.innerText()})`);
  await shot("13-checks");
  await page.setViewportSize({ width: 390, height: 844 });
  await page.waitForTimeout(150);
  await shot("13b-checks-390");
  await page.setViewportSize({ width: 1280, height: 900 });
  // Through to the repo: the check is open there, and a book shows its kept rows against the true count.
  await top.click();
  await page.waitForURL(/\/repos\/wa-catalog\/km_ulb\?tab=checks&check=/);
  const chip = page.locator(".checks-table tr.check-books button.book-chip").first();
  await chip.waitFor();
  await chip.click();
  const capped = page.locator(".rail-capped").first();
  await capped.waitFor();
  const note = await capped.innerText();
  expect(/[\d,]+ in this book; the first [\d,]+ are kept/.test(note), `"N in this book; the first M are kept" (got "${note}")`);
  results.push({ name: `  km_ulb: ${note}`, ok: true, ms: 0, notes: [] });
  await page.locator(".rail").scrollIntoViewIfNeeded();
  await shot("14-km_ulb-MisplacedFormat");
});

await step("source-compare", async () => {
  // A length finding opens beside its source: both verses, both lengths, and the book's usual ratio.
  await page.goto(`${base}/repos/wa-catalog/ru_ulb?file=01-GEN.usfm`);
  const check = page.locator(".rail-check", { hasText: "LengthProportionality" });
  await check.waitFor();
  await check.click();
  await page.locator(".rail-group", { has: check }).locator(".rail-items button").first().click();
  const card = page.locator(".compare");
  // The source verse arrives after the source file does.
  await card.locator(".compare-text").nth(1).waitFor();
  await card.locator(".compare-why").first().waitFor();
  const texts = await card.locator(".compare-text").allInnerTexts();
  expect(texts.length === 2 && texts.every((t) => t.trim().length > 0), `both verses shown (got ${JSON.stringify(texts)})`);
  const why = await card.locator(".compare-why").first().innerText();
  expect(/usually runs [\d.]+×/.test(why), `the book's usual ratio (got "${why}")`);
  results.push({ name: `  ${why.slice(0, 90)}`, ok: true, ms: 0, notes: [] });
  await shot("07b-source-compare");
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
  // Folders render their files only once opened (the tree is lazy).
  await page.locator(".tree details > summary").first().click();
  await page.locator(".tree details .file", { hasText: /\.txt$/ }).first().click();
  await page.locator(".viewer pre.code").waitFor();
  expect((await page.locator(".viewer pre.code").innerText()).length > 0, "a Writer chunk as text");
  await shot("08-synthetic-source-files");
  results.push({ name: `  synthetic repo: ${pick.owner}/${pick.name} (${file})`, ok: true, ms: 0, notes: [] });
});

await step("language-ceb", async () => {
  await page.goto(`${base}/languages/ceb`);
  await page.getByRole("heading", { level: 1, name: /Cebuano/ }).waitFor();
  // What is published leads the page (the catalog's books table is gone).
  await page.locator(".published-list li").first().waitFor();
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
