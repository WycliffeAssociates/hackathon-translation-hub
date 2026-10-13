// The read-only USFM viewer, imported from the spike (spikes/viewer/src via
// the `@viewer` alias; see vite.config.ts) rather than copied. The spike's
// `mountViewer` is Sefer's editor in read-only mode with the engine's
// diagnostics drawn inline; this component adds the mode toggle, a chapter
// picker, proofreading findings as underlines, and "go to this range".
import { For, Show, createEffect, createSignal, on, onCleanup, onMount } from "solid-js";
import "@viewer/host/tokens.css";
import "@viewer/editor-fork/editor.css";
import { bootEngine } from "@viewer/host/engine.onion";
import { mountViewer, type ViewerMount } from "@viewer/viewer/mountViewer";
import { showCorpusFindings, type CorpusFinding } from "@viewer/editor-fork/recipes/lint";

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
  onReady?: () => void;
}) {
  let host!: HTMLDivElement;
  const [mount, setMount] = createSignal<ViewerMount>();
  const [mode, setMode] = createSignal<Mode>("regular");
  const [failed, setFailed] = createSignal<string>();
  const doc = canonical(props.text);

  onMount(() => {
    let alive = true;
    onCleanup(() => {
      alive = false;
      mount()?.destroy();
    });
    bootEngine().then(
      (engine) => {
        if (!alive) return;
        const m = mountViewer({ parent: host, text: doc.text, analyze: engine.memoize(), mode: mode() });
        setMount(m);
        props.onReady?.();
      },
      (e: unknown) => setFailed(String(e)),
    );
  });

  createEffect(on([mount, mode], ([m, md]) => m?.setMode(md)));

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

  const chapters = () => mount()?.chapters().filter((c) => c.label) ?? [];

  const goTo = (label: string) => {
    const m = mount();
    const row = m?.chapters().find((c) => c.label === label);
    if (m && row) m.scrollTo(row.ordinal);
  };
  createEffect(on([mount, () => props.chapter], ([m, ch]) => m && ch && goTo(ch)));

  return (
    <div class="usfm-viewer">
      <div class="viewer-bar">
        <div class="seg" role="group" aria-label="View mode">
          <button type="button" aria-pressed={mode() === "regular"} onClick={() => setMode("regular")}>Regular</button>
          <button type="button" aria-pressed={mode() === "usfm"} onClick={() => setMode("usfm")}>USFM</button>
        </div>
        <Show when={chapters().length}>
          <label class="small">
            Chapter{" "}
            <select onChange={(e) => goTo(e.currentTarget.value)} value={props.chapter ?? ""}>
              <option value="" disabled>…</option>
              <For each={chapters()}>{(c) => <option value={c.label}>{c.label}</option>}</For>
            </select>
          </label>
        </Show>
        <Show when={!mount() && !failed()}><span class="loading small">Starting the viewer…</span></Show>
      </div>
      <Show when={failed()}>{(e) => <div class="failure">Viewer failed: {e()}</div>}</Show>
      <div class="viewer-pane" ref={host} data-mode={mode()} />
    </div>
  );
}
