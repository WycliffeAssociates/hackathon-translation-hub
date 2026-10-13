// The read-only USFM viewer, imported from the spike (spikes/viewer/src via
// the `@viewer` alias; see vite.config.ts) rather than copied. The spike's
// `mountViewer` is Sefer's editor in read-only mode with the engine's
// diagnostics drawn inline; this component adds the mode toggle, a chapter
// navigator that follows the scroll (the fork's `watchLocation`), proofreading
// findings as underlines, and "go to this range".
import { Show, createEffect, createSignal, on, onCleanup, onMount } from "solid-js";
import "@viewer/host/tokens.css";
import "@viewer/editor-fork/editor.css";
import { bootEngine } from "@viewer/host/engine.onion";
import { mountViewer, type ViewerMount } from "@viewer/viewer/mountViewer";
import { showCorpusFindings, type CorpusFinding } from "@viewer/editor-fork/recipes/lint";
import { watchLocation } from "@viewer/editor-fork/recipes/whereAmI";
import type { BookRow } from "../api/types";
import ChapterNav from "./ChapterNav";

export interface Range {
  readonly from: number;
  readonly to: number;
}

export interface ViewerFinding extends Range {
  readonly code: string;
  readonly message: string;
}

type Mode = "regular" | "usfm";

/**
 * The engine reads LF text only. Offsets from the server count UTF-16 units in
 * the raw file, so a file with CRs needs its offsets moved back by the CRs
 * before them.
 */
function canonical(raw: string): { text: string; at: (offset: number) => number } {
  if (!raw.includes("\r")) return { text: raw, at: (o) => o };
  const crs: number[] = [];
  for (let i = raw.indexOf("\r"); i >= 0; i = raw.indexOf("\r", i + 1)) crs.push(i);
  const text = raw.replace(/\r\n?/g, "\n");
  // A lone \r became \n (same length); only \r\n pairs shrink.
  const removed = crs.filter((i) => raw[i + 1] === "\n");
  return {
    text,
    at: (o) => {
      let n = 0;
      while (n < removed.length && removed[n]! < o) n++;
      return o - n;
    },
  };
}

export default function UsfmViewer(props: {
  text: string;
  findings: readonly ViewerFinding[];
  /** A range to select and scroll to. */
  focus: Range | null;
  /** A chapter label (`"3"`) to scroll to. */
  chapter: string | null;
  /** The book's lint and coverage, to mark chapters in the navigator. */
  book?: BookRow | undefined;
  onReady?: () => void;
}) {
  let host!: HTMLDivElement;
  const [mount, setMount] = createSignal<ViewerMount>();
  /** The chapter label in view (`""` in the front matter). */
  const [current, setCurrent] = createSignal("");
  const [mode, setMode] = createSignal<Mode>("regular");
  const [failed, setFailed] = createSignal<string>();
  const doc = canonical(props.text);

  onMount(() => {
    let alive = true;
    let unwatch = () => {};
    onCleanup(() => {
      alive = false;
      unwatch();
      mount()?.destroy();
    });
    bootEngine().then(
      (engine) => {
        if (!alive) return;
        const m = mountViewer({ parent: host, text: doc.text, analyze: engine.memoize(), mode: mode() });
        unwatch = watchLocation(m.view, (where) => follow(m, where?.label ?? ""));
        setMount(m);
        props.onReady?.();
      },
      (e: unknown) => setFailed(String(e)),
    );
  });

  createEffect(
    on([mount, mode], ([m, md]) => {
      if (!m) return;
      m.setMode(md);
      // The text reflows under a still scroller; ask again where we are.
      requestAnimationFrame(() => m.view.scrollDOM.dispatchEvent(new Event("scroll")));
    }),
  );

  createEffect(() => {
    const m = mount();
    if (!m) return;
    const stamp = { revision: 0, length: doc.text.length };
    const list: CorpusFinding[] = props.findings.map((f, i) => ({
      id: `${i}`,
      code: f.code,
      severity: "info",
      message: f.message,
      from: doc.at(f.from),
      to: doc.at(f.to),
      stamp,
    }));
    showCorpusFindings(m.view, list);
  });

  createEffect(() => {
    const m = mount();
    const f = props.focus;
    if (!m || !f) return;
    const anchor = Math.min(doc.at(f.from), doc.text.length);
    const head = Math.min(doc.at(f.to), doc.text.length);
    m.clipTo(null);
    m.view.dispatch({ selection: { anchor, head }, scrollIntoView: true });
    // Centre it: CodeMirror's minimal scroll leaves it at the bottom edge.
    requestAnimationFrame(() => {
      const at = m.view.coordsAtPos(anchor);
      const box = m.view.scrollDOM.getBoundingClientRect();
      if (at) m.view.scrollDOM.scrollTop += at.top - box.top - box.height / 3;
    });
  });

  const labels = () => mount()?.chapters().flatMap((c) => (c.label ? [c.label] : [])) ?? [];

  // After a jump the indicator says where we asked to go, until the reader
  // scrolls: a short last chapter cannot reach the top of the pane, and the
  // scroll position alone would name the one above it (and "next" would stick).
  let pinned: { label: string; top: number | null } | null = null;
  function follow(m: ViewerMount, label: string) {
    if (pinned) {
      if (pinned.top === null || Math.abs(m.view.scrollDOM.scrollTop - pinned.top) < 4) return;
      pinned = null;
    }
    setCurrent(label);
  }

  const goTo = (label: string) => {
    const m = mount();
    const row = m?.chapters().find((c) => c.label === label);
    if (!m || !row) return;
    const pin = { label, top: null as number | null };
    pinned = pin;
    setCurrent(label);
    m.scrollTo(row.ordinal);
    // CodeMirror scrolls on its next measure, and may correct once its height
    // estimates are replaced by real lines; take the position after that.
    setTimeout(() => pinned === pin && (pin.top = m.view.scrollDOM.scrollTop), 200);
  };
  createEffect(on([mount, () => props.chapter], ([m, ch]) => m && ch && goTo(ch)));

  const step = (by: -1 | 1) => {
    const all = labels();
    const next = all[all.indexOf(current()) + by] ?? (current() === "" && by === 1 ? all[0] : undefined);
    if (next !== undefined) goTo(next);
  };

  /** `[` / `]` and Alt+←/→ step chapters while focus is in the viewer. */
  const onKey = (e: KeyboardEvent) => {
    if (e.defaultPrevented || e.metaKey || e.ctrlKey) return;
    if ((e.target as HTMLElement).closest(".chnav-pop")) return;
    const by = e.key === "[" || (e.altKey && e.key === "ArrowLeft") ? -1 : e.key === "]" || (e.altKey && e.key === "ArrowRight") ? 1 : 0;
    if (!by) return;
    e.preventDefault();
    step(by);
  };

  return (
    <div class="usfm-viewer" onKeyDown={onKey}>
      <div class="viewer-bar">
        <div class="seg" role="group" aria-label="View mode">
          <button type="button" aria-pressed={mode() === "regular"} onClick={() => setMode("regular")}>Regular</button>
          <button type="button" aria-pressed={mode() === "usfm"} onClick={() => setMode("usfm")}>USFM</button>
        </div>
        <Show when={labels().length}>
          <ChapterNav
            labels={labels()}
            current={current()}
            book={props.book}
            onStep={step}
            onPick={(label) => {
              goTo(label);
              host.focus({ preventScroll: true });
            }}
          />
        </Show>
        <Show when={!mount() && !failed()}><span class="loading small">Starting the viewer…</span></Show>
      </div>
      <Show when={failed()}>{(e) => <div class="failure">Viewer failed: {e()}</div>}</Show>
      {/* Focusable, so a click in the text puts the [ ] keys here. */}
      <div class="viewer-pane" ref={host} data-mode={mode()} tabIndex={0} aria-label="Book text" />
    </div>
  );
}
