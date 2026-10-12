const T0 = performance.now();
import "../host/tokens.css";
import "../editor-fork/editor.css";
import "./demo.css";

import { bootEngine } from "../host/engine";
import { mountViewer, type ViewerMount } from "../viewer/mountViewer";

declare global {
  interface Window {
    __viewer?: ViewerMount;
    __timings?: Record<string, number>;
    __ready?: Promise<void>;
  }
}

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const pane = $("pane");
const bookSel = $<HTMLSelectElement>("book");
const chapterSel = $<HTMLSelectElement>("chapter");
const clipBox = $<HTMLInputElement>("clip");
const stats = $("stats");
const params = new URLSearchParams(location.search);
let mode: "regular" | "usfm" = params.get("mode") === "usfm" ? "usfm" : "regular";
let current: ViewerMount | undefined;

const nextPaint = () => new Promise<void>((r) => requestAnimationFrame(() => setTimeout(r, 0)));

async function open(book: string): Promise<void> {
  const t: Record<string, number> = {};
  const start = performance.now();
  const [engine, text] = await Promise.all([
    bootEngine(),
    fetch(`${import.meta.env.BASE_URL}books/${book}.usfm`).then((r) => r.text()),
  ]);
  t.engineBootMs = engine.bootMs;
  t.fetchMs = performance.now() - start;
  const lf = text.replace(/\r\n?/g, "\n");
  current?.destroy();
  const a = performance.now();
  const memo = engine.memoize();
  let parseMs = 0;
  let parses = 0;
  const analyze = (x: string) => {
    const p0 = performance.now();
    const r = memo(x);
    parseMs += performance.now() - p0;
    parses += 1;
    return r;
  };
  current = mountViewer({ parent: pane, text: lf, analyze, mode });
  t.parseMsDuringMount = parseMs;
  t.analyzeCallsDuringMount = parses;
  t.mountMs = performance.now() - a;
  await nextPaint();
  t.firstPaintAfterMountMs = performance.now() - a;
  t.sincePageStartMs = performance.now() - T0;
  t.navStartToPaintMs = performance.now();
  t.wasmBytes = engine.wasmBytes;
  t.chars = lf.length;
  const chapters = current.chapters();
  t.chapters = chapters.length;
  chapterSel.replaceChildren(new Option("Whole book", ""));
  chapters.forEach((c, i) => {
    if (c.label) chapterSel.append(new Option(`Chapter ${c.label}`, String(i)));
  });
  window.__viewer = current;
  window.__timings = t;
  stats.textContent = `${book}: ${(lf.length / 1024).toFixed(0)} KB, ${chapters.length} rows · wasm boot ${t.engineBootMs.toFixed(0)}ms · mount ${t.mountMs.toFixed(0)}ms · paint ${t.firstPaintAfterMountMs.toFixed(0)}ms · page→paint ${t.sincePageStartMs.toFixed(0)}ms`;
  console.log("timings", t);
}

const setMode = (next: "regular" | "usfm") => {
  mode = next;
  $("mode-regular").setAttribute("aria-pressed", String(next === "regular"));
  $("mode-usfm").setAttribute("aria-pressed", String(next === "usfm"));
  current?.setMode(next);
};
$("mode-regular").onclick = () => setMode("regular");
$("mode-usfm").onclick = () => setMode("usfm");
setMode(mode);

const applyChapter = () => {
  if (!current) return;
  const v = chapterSel.value;
  const ord = v === "" ? null : Number(v);
  if (clipBox.checked || ord === null) current.clipTo(ord);
  else {
    current.clipTo(null);
    current.scrollTo(ord);
  }
};
chapterSel.onchange = applyChapter;
clipBox.onchange = applyChapter;
bookSel.value = params.get("book") ?? "19-PSA";
bookSel.onchange = () => void open(bookSel.value);
window.__ready = open(bookSel.value);
