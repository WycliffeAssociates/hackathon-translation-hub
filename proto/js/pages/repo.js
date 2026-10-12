import { blobText, contentForRepo, repoMeta, repoTree, WACS } from "../api.js";
import { bookByUsfm, bookFromRepoName } from "../books.js";
import { coverage, lintUsfm, ranges, tally, versesByChapter } from "../check.js";
import { MANIFEST_FILES, parseManifest, resolveSource } from "../manifest.js";
import { RESOURCE_NAMES, statusOf } from "../domain.js";
import { ago, chip, counts, failure, html, loading, render, wire } from "../ui.js";
import { isWriterTextPath, repoShape, treeFromFiles, tsChunkPaths, writerBookToUsfm } from "../usfm.js";

// Checking a whole book means one blob request per tS chunk. WACS throttles
// bursts, so past this many files the page checks a chapter at a time.
const WHOLE_BOOK_LIMIT = 60;
const TEXT_FILE = /\.(usfm|sfm|txt|md|json|ya?ml|tsv|csv)$/i;

export async function repoPage(el, { owner, repo }) {
  render(el, loading(`${owner}/${repo}`));
  let meta, row, tree;
  try {
    [meta, row] = await Promise.all([repoMeta(owner, repo), contentForRepo(owner, repo).catch(() => null)]);
    tree = meta.empty ? [] : await repoTree(owner, repo, meta.default_branch);
  } catch (e) {
    return render(el, html`<nav class="crumbs"><a href="#/search">Search</a> › ${owner}/${repo}</nav>${failure(e)}`);
  }
  const blobs = tree.filter((e) => e.type === "blob");
  const paths = blobs.map((e) => e.path);
  const sha = new Map(blobs.map((e) => [e.path, e.sha]));
  const shape = repoShape(paths);
  const book = bookFromRepoName(repo);
  const ctx = { owner, repo, sha, book, source: null };

  render(el, html`
    <nav class="crumbs"><a href="#/search">Search</a> ›
      ${row?.language ? html`<a href="#/l/${row.language.ietf_code}">${row.language.english_name}</a> › <a href="#/l/${row.language.ietf_code}/${row.resource_type}/${row.type}?book=${book?.slug ?? ""}">${RESOURCE_NAMES[row.resource_type] ?? row.resource_type} · ${row.type}</a> › ` : ""}
      ${book?.name ?? repo}</nav>
    <header class="page-head">
      <h1 class="mono">${owner}/${repo}</h1>
      <p class="facts">
        ${row ? chip(statusOf(row)) : html`<span class="tag">not in the catalog</span>`}
        <span>${shape === "ts" ? "translationStudio" : shape === "usfm" ? "USFM files" : "other layout"}</span>
        <span>${blobs.length} files</span>
        <span>branch <code>${meta.default_branch}</code></span>
        <span>updated ${ago(meta.updated_at)}</span>
        <a href="${WACS}/${owner}/${repo}" target="_blank" rel="noopener">Open in WACS ↗</a>
      </p>
    </header>
    ${meta.empty ? html`<p>This repo is empty.</p>` : html`
      <section class="checks" data-checks>
        <div class="source-line small" data-source>${loading("manifest")}</div>
        ${checksPanel(shape, paths, book)}
      </section>
      <section class="browser">
        <div class="tree" data-tree>${treeView(paths)}</div>
        <div class="viewer"><p class="mock-note">Stand-in: plain text. Proposed: a read-only CodeMirror with Sefer's regular mode — diagnostics drawn in the text, infinite scroll. No editing; that is Sefer's job.</p><div data-viewer><p class="small">Pick a file.</p></div></div>
      </section>
      <div data-health></div>`}
    ${wire("history", html`<h3>History</h3><p class="small">Master only, read on demand from the repo, like GitHub. Who changed which chapters, and when.</p><div class="wf-rows"><div></div><div></div></div>`)}`);

  if (meta.empty) return;
  el.querySelector("[data-tree]").addEventListener("click", (e) => {
    const f = e.target.closest("[data-file]");
    if (f) showFile(el, ctx, f.dataset.file);
  });
  el.querySelector("[data-viewer]").addEventListener("click", (e) => {
    const b = e.target.closest("[data-check-file]");
    if (!b) return;
    b.dataset.check = "file";
    b.dataset.path = b.dataset.checkFile;
    runCheck(el, ctx, shape, paths, b);
    el.querySelector("[data-checks]").scrollIntoView({ behavior: "smooth" });
  });
  el.querySelector("[data-checks]").addEventListener("click", (e) => {
    const b = e.target.closest("[data-check]");
    if (b) runCheck(el, ctx, shape, paths, b);
  });
  readManifest(el, ctx, shape, paths, row);
}

// --- manifest, source, health ------------------------------------------------

async function readManifest(el, ctx, shape, paths, row) {
  const path = MANIFEST_FILES.find((p) => paths.includes(p));
  let manifest = null;
  try {
    manifest = path ? parseManifest(path, await blobText(ctx.owner, ctx.repo, ctx.sha.get(path))) : null;
    ctx.source = manifest?.sources.length ? await resolveSource(manifest.sources, repoMeta) : null;
  } catch (e) {
    render(el.querySelector("[data-source]"), failure(e));
  }
  render(el.querySelector("[data-source]"), sourceLine(manifest, ctx.source));
  render(el.querySelector("[data-health]"), healthView(path, manifest, ctx, shape, paths, row));
}

function sourceLine(manifest, source) {
  if (source)
    return html`<b>Translating from</b> ${source.declared.language} / ${source.declared.resource} → <a class="mono" href="#/r/${source.owner}/${source.repo}">${source.owner}/${source.repo}</a> · coverage against it shows after a check`;
  const declared = manifest?.sources ?? [];
  if (declared.some((s) => s.opaque)) return html`<b>Source</b> declared as <code>${declared[0].opaque}</code>, which does not name a catalog repo · no coverage`;
  if (declared.length) return html`<b>Source</b> declared as ${declared.map((s) => `${s.language}/${s.resource}`).join(", ")}, not found on WACS · no coverage`;
  return html`<b>Source</b> none declared · no coverage`;
}

// A sketch of what manifest health could check. Computed from the real
// manifest, but the list itself is a proposal.
function healthView(path, m, ctx, shape, paths, row) {
  const rows = [];
  const add = (status, name, detail) => rows.push({ status, name, detail });
  add(path ? "pass" : "fail", "Manifest present", path ?? "no manifest.json, manifest.yaml or metadata.json");
  if (path) add(m?.error ? "fail" : "pass", "Manifest parses", m?.error ?? `${m.kind}`);
  if (m && !m.error) {
    const lang = row?.language?.ietf_code;
    if (m.language && lang) add(m.language.toLowerCase() === lang.toLowerCase() ? "pass" : "warn", "Language matches catalog", `manifest ${m.language} · catalog ${lang}`);
    else add("skip", "Language matches catalog", "not in both");
    if (m.book && ctx.book) add(m.book.toLowerCase() === ctx.book.slug ? "pass" : "warn", "Book matches repo name", `manifest ${m.book} · name ${ctx.book.slug}`);
    else add("skip", "Book matches repo name", "not in both");
    if (m.resource && row?.resource_type) add(m.resource === row.resource_type ? "pass" : "warn", "Resource matches catalog", `manifest ${m.resource} · catalog ${row.resource_type}`);
    else add("skip", "Resource matches catalog", "not in both");
    add(m.sources.length ? "pass" : "warn", "Source declared", m.sources.length ? `${m.sources.length}` : "none");
    if (m.sources.length) add(ctx.source ? "pass" : "warn", "Source resolves to a repo", ctx.source ? `${ctx.source.owner}/${ctx.source.repo}` : "not found");
    if (shape === "ts" && m.finishedChunks) {
      const files = tsChunkPaths(paths).length;
      add(m.finishedChunks.length >= files ? "pass" : "warn", "Chunks marked finished", `${m.finishedChunks.length} of ${files} chunk files`);
    }
  }
  return html`<section class="wire"><span class="wire-tag">sketch · manifest health</span>
    <h3>Manifest health</h3>
    <p class="small">Computed from this repo's real manifest. Which checks belong here is a proposal.</p>
    <table class="health"><tbody>${rows.map((r) => html`<tr><td><span class="hs hs-${r.status}">${r.status}</span></td><td>${r.name}</td><td class="small mono">${r.detail}</td></tr>`)}</tbody></table>
  </section>`;
}

/** The source's verses for one book, fetched once per page. */
const sourceCache = new Map();
function sourceVerses(source, usfm) {
  const key = `${source.owner}/${source.repo}/${usfm}`;
  if (!sourceCache.has(key))
    sourceCache.set(key, (async () => {
      const tree = await repoTree(source.owner, source.repo, source.ref);
      const file = tree.find((e) => e.type === "blob" && new RegExp(`(^|[-_/])${usfm}\\.usfm$`, "i").test(e.path));
      if (!file) return null;
      return versesByChapter(await blobText(source.owner, source.repo, file.sha));
    })().catch((e) => (sourceCache.delete(key), Promise.reject(e))));
  return sourceCache.get(key);
}

// --- the check panel ---------------------------------------------------------

function checksPanel(shape, paths, book) {
  if (shape === "ts") {
    const chunks = tsChunkPaths(paths);
    const present = new Set(chunks.map((c) => c.chapter));
    const total = book?.chapters ?? Math.max(0, ...present);
    return html`
      <div class="checks-head">
        <h2>Chapters</h2>
        <span class="small">${present.size} of ${total} present · click a chapter to check it</span>
        ${chunks.length <= WHOLE_BOOK_LIMIT ? html`<button data-check="book">Check whole book</button>` : html`<span class="small">(${chunks.length} chunk files — checking one chapter at a time to stay under WACS's rate limit)</span>`}
      </div>
      <div class="chapters">${Array.from({ length: total }, (_, i) => i + 1).map((n) =>
        present.has(n)
          ? html`<button class="ch present" data-check="chapter" data-n="${n}" title="Chapter ${n}">${n}</button>`
          : html`<span class="ch missing" title="Chapter ${n} missing">${n}</span>`)}</div>
      <div class="ch-legend small"><span><i class="ch present"></i>present</span><span><i class="ch clean"></i>clean</span><span><i class="ch warn"></i>warnings</span><span><i class="ch err"></i>errors</span><span><i class="ch missing"></i>missing</span></div>
      <div data-findings></div>`;
  }
  if (shape === "usfm") {
    const files = paths.filter((p) => /\.(usfm|sfm)$/i.test(p));
    return html`
      <div class="checks-head"><h2>Books</h2><span class="small">${files.length} USFM file${files.length > 1 ? "s" : ""} · open one in the file tree below to check it</span></div>
      <div data-findings></div>`;
  }
  return html`<p class="small">No USFM or tS chunks here, so there is nothing to check.</p>`;
}

async function runCheck(el, ctx, shape, paths, button) {
  const out = el.querySelector("[data-findings]");
  const kind = button.dataset.check;
  let label, text;
  button.disabled = true;
  render(out, loading("files from WACS"));
  try {
    if (kind === "file") {
      label = button.dataset.path;
      text = await blobText(ctx.owner, ctx.repo, ctx.sha.get(label));
    } else {
      const only = kind === "chapter" ? Number(button.dataset.n) : null;
      // The Writer files Sefer's stitch reads: the front title, and the chapter's (or all) titles and chunks.
      const wanted = paths.filter((p) => isWriterTextPath(p) && (only === null || p === "front/title.txt" || Number(p.split("/")[0]) === only));
      label = only === null ? "whole book" : `chapter ${only}`;
      const texts = await Promise.all(wanted.map((p) => blobText(ctx.owner, ctx.repo, ctx.sha.get(p))));
      const usfm = ctx.book?.usfm ?? "XXX";
      text = writerBookToUsfm(treeFromFiles(new Map(wanted.map((p, i) => [p, texts[i]]))), usfm, ctx.book?.name ?? usfm);
    }
    const t0 = performance.now();
    const rows = await lintUsfm(text);
    const ms = Math.round(performance.now() - t0);
    markChapters(el, kind, button, rows);
    const cov = await coverageFor(ctx, kind, button, text).catch(() => null);
    if (cov) markCoverage(el, cov);
    render(out, html`${coverageView(ctx.source, cov)}${findingsView(label, rows, ms, text)}`);
  } catch (e) {
    render(out, failure(e));
  } finally {
    button.disabled = false;
  }
}

async function coverageFor(ctx, kind, button, text) {
  if (!ctx.source) return null;
  const usfm = (kind === "file" ? bookByUsfm(text.match(/\\id\s+(\w{3})/)?.[1] ?? "") : ctx.book)?.usfm;
  if (!usfm) return null;
  const source = await sourceVerses(ctx.source, usfm);
  if (!source) return null;
  const target = await versesByChapter(text);
  const chapters = kind === "chapter" ? [Number(button.dataset.n)] : [...source.keys()].sort((a, b) => a - b);
  return coverage(target, source, chapters);
}

function markCoverage(el, cov) {
  for (const c of cov) {
    const b = el.querySelector(`button.ch[data-n="${c.chapter}"]`);
    if (!b || !c.source) continue;
    const pct = Math.round((c.present / c.source) * 100);
    b.style.setProperty("--cov", `${pct}%`);
    b.classList.add("has-cov");
    b.title = `Chapter ${c.chapter}: ${c.present} of ${c.source} source verses (${pct}%)`;
  }
}

function coverageView(source, cov) {
  if (!source || !cov) return "";
  const want = cov.reduce((n, c) => n + c.source, 0);
  const have = cov.reduce((n, c) => n + c.present, 0);
  const gaps = cov.filter((c) => c.missing.length);
  const extra = cov.reduce((n, c) => n + c.extra, 0);
  return html`<div class="coverage">
    <b>Coverage vs ${source.owner}/${source.repo}</b>
    <span class="cov-num">${have} of ${want} verses · ${want ? Math.round((have / want) * 100) : 0}%</span>
    ${extra ? html`<span class="small">${extra} extra (versification)</span>` : ""}
    ${gaps.length ? html`<details class="small"><summary>${gaps.length} chapter${gaps.length > 1 ? "s" : ""} with missing verses</summary>
      <ul>${gaps.map((c) => html`<li>Chapter ${c.chapter}: missing ${ranges(c.missing)}</li>`)}</ul></details>` : html`<span class="small ok">no missing verses</span>`}
  </div>`;
}

function markChapters(el, kind, button, rows) {
  const grade = (rs) => (rs.some((r) => r.severity === "error") ? "err" : rs.some((r) => r.severity === "warning") ? "warn" : "clean");
  const set = (b, rs) => (b.classList.remove("clean", "warn", "err"), b.classList.add(grade(rs)));
  if (kind === "chapter") set(button, rows);
  if (kind === "book") for (const b of el.querySelectorAll("button.ch")) set(b, rows.filter((r) => r.chapter === Number(b.dataset.n)));
}

function findingsView(label, rows, ms, text) {
  const t = tally(rows);
  return html`
    <div class="findings-head">
      <b>${label}</b>
      ${counts(t)}
      <span class="small">checked ${text.length.toLocaleString()} characters in ${ms} ms, in your browser</span>
      <details class="small"><summary>stitched USFM</summary><pre class="code">${text}</pre></details>
    </div>
    ${rows.length ? html`<table class="findings"><thead><tr><th></th><th>Where</th><th>What</th><th>Text</th></tr></thead><tbody>
      ${rows.map((r) => html`<tr><td><span class="sev ${r.severity}">${r.severity}</span></td>
        <td class="mono">${r.chapter ? `${r.chapter}:${r.verse}` : `line ${r.line}`}</td>
        <td>${r.message}<div class="small mono">${r.name}</div></td>
        <td class="excerpt mono">${r.excerpt}</td></tr>`)}
    </tbody></table>` : html`<p class="clean-note">No structural findings.</p>`}`;
}

// --- file browser ------------------------------------------------------------

function treeView(paths) {
  const root = {};
  for (const p of paths) {
    let node = root;
    const parts = p.split("/");
    parts.forEach((name, i) => (node = node[name] ??= i === parts.length - 1 ? p : {}));
  }
  const walk = (node, depth) =>
    Object.entries(node)
      .sort(([a, x], [b, y]) => (typeof x === "string") - (typeof y === "string") || a.localeCompare(b, undefined, { numeric: true }))
      .map(([name, child]) =>
        typeof child === "string"
          ? html`<button class="file" data-file="${child}">${name}</button>`
          : html`<details ${depth === 0 && Object.keys(node).length < 8 ? "open" : ""}><summary>${name}/</summary>${walk(child, depth + 1)}</details>`);
  return walk(root, 0);
}

async function showFile(el, ctx, path) {
  const viewer = el.querySelector("[data-viewer]");
  for (const b of el.querySelectorAll(".file.on")) b.classList.remove("on");
  el.querySelector(`[data-file="${CSS.escape(path)}"]`)?.classList.add("on");
  if (!TEXT_FILE.test(path)) return render(viewer, html`<h3 class="mono">${path}</h3><p class="small">Not a text file.</p>`);
  render(viewer, loading(path));
  try {
    const text = await blobText(ctx.owner, ctx.repo, ctx.sha.get(path));
    const lines = text.split("\n");
    const checkable = /\.(usfm|sfm)$/i.test(path);
    render(viewer, html`<div class="viewer-head"><h3 class="mono">${path}</h3>${checkable ? html`<button data-check-file="${path}">Check this book</button>` : ""}</div>
      <pre class="code numbered">${lines.map((l, i) => html`<span><i>${i + 1}</i>${l}\n</span>`)}</pre>`);
  } catch (e) {
    render(viewer, failure(e));
  }
}
