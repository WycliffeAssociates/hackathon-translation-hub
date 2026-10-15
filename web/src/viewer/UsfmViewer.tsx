// The read-only USFM viewer, imported from the spike (spikes/viewer/src via
// the `@viewer` alias; see vite.config.ts) rather than copied. The spike's
// `mountViewer` is Sefer's editor in read-only mode with the engine's
// diagnostics drawn inline; this component adds the mode toggle, a chapter
// navigator that follows the scroll (the fork's `watchLocation`), proofreading
// findings as underlines, "go to this range", and the rail beside the text
// listing every mark it draws.
import { Show, createEffect, createMemo, createSignal, on, onCleanup, onMount } from "solid-js";
import "@viewer/host/tokens.css";
import "@viewer/editor-fork/editor.css";
import { bootEngine } from "@viewer/host/engine.onion";
import { mountViewer, type Shown, type ViewerMount } from "@viewer/viewer/mountViewer";
import { showCorpusFindings, type CorpusFinding } from "@viewer/editor-fork/recipes/lint";
import { watchLocation } from "@viewer/editor-fork/recipes/whereAmI";
import type { BookRow, CodeCount, PairedEvidence } from "../api/types";
import ChapterNav from "./ChapterNav";
import FindingsRail, { type RailItem } from "./FindingsRail";
import SourceCompare, { type Compared, type SourceRef } from "./SourceCompare";

export interface Range {
  readonly from: number;
  readonly to: number;
}

export interface ViewerFinding extends Range {
  readonly code: string;
  readonly message: string;
  readonly severity: "error" | "warning" | "info";
  readonly chapter?: number;
  readonly verse?: number | null;
  /** Present on source-compared findings: lengths, kind, how unusual. */
  readonly paired?: PairedEvidence | null;
}

type Mode = "regular" | "usfm";

/**
 * The engine reads LF text only. Offsets from the server count UTF-16 units in
 * the raw file, so a file with CRs needs its offsets moved back by the CRs
 * before them.
 */
/**
 * Chapter:verse for an offset into the (LF) text, from its `\c` and `\v`
 * markers: `3:16`, `3` before the chapter's first verse, `intro` before `\c`.
 */
function addresses(text: string): (offset: number) => string {
  const marks: { at: number; c?: string; v?: string }[] = [];
  for (const m of text.matchAll(/\\(c|v)\s+(\d+[a-z]?(?:-\d+[a-z]?)?)/g)) {
    marks.push(m[1] === "c" ? { at: m.index, c: m[2] } : { at: m.index, v: m[2] });
  }
  return (offset) => {
    let lo = 0;
    let hi = marks.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (marks[mid]!.at <= offset) lo = mid + 1;
      else hi = mid;
    }
    let verse: string | undefined;
    for (let i = lo - 1; i >= 0; i--) {
      const m = marks[i]!;
      if (m.v !== undefined) verse ??= m.v;
      if (m.c !== undefined) return verse ? `${m.c}:${verse}` : m.c;
    }
    return "intro";
  };
}

/** Format characters (zero-width space, joiners, marks) as `⟨200B⟩`, so a row can show what it is about. */
const visible = (s: string) =>
  s.replace(/[\n\t]+/g, " ").replace(/\p{Cf}/gu, (c) => `⟨${c.codePointAt(0)!.toString(16).toUpperCase().padStart(4, "0")}⟩`);

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
  /** The repo's finding counts, every book (`/findings`), for the rail. */
  counts?: readonly CodeCount[];
  /** `engine:code` for the rail to open first. */
  check?: string | null;
  /** Open another book of the repo with a check (the rail's All books). */
  onBook?: (engine: string, code: string, book: string) => void;
  /** The source text this book was compared with, for source-compared findings. */
  source?: SourceRef | null;
  onReady?: () => void;
}) {
  let host!: HTMLDivElement;
  const [mount, setMount] = createSignal<ViewerMount>();
  /** The chapter label in view (`""` in the front matter). */
  const [current, setCurrent] = createSignal("");
  const [mode, setMode] = createSignal<Mode>("regular");
  const [failed, setFailed] = createSignal<string>();
  const doc = canonical(props.text);
  const where = addresses(doc.text);
  const [shown, setShown] = createSignal<readonly Shown[]>([]);
  /** Sous Chef's source-compared rows by code and start, to find the evidence behind a drawn mark. */
  const comparedAt = createMemo(() => {
    const by = new Map<string, Compared>();
    for (const f of props.findings) {
      if (f.paired && f.chapter !== undefined) {
        by.set(`${f.code}@${doc.at(f.from)}`, { code: f.code, chapter: f.chapter, verse: f.verse, paired: f.paired });
      }
    }
    return by;
  });
  const items = createMemo<RailItem[]>(() =>
    shown().map((d) => {
      const slash = d.source.indexOf("/");
      const engine = d.source.slice(0, slash);
      const code = d.source.slice(slash + 1);
      return {
        compared: engine === "sous" ? comparedAt().get(`${code}@${d.from}`) : undefined,
        from: d.from,
        to: d.to,
        engine: engine === "onion" ? "lint" : engine,
        code: d.source.slice(slash + 1),
        severity: d.severity === "hint" ? ("info" as const) : d.severity,
        message: d.message,
        ref: where(d.from),
        context: visible(doc.text.slice(Math.max(0, d.from - 12), d.to + 12)).trim(),
      };
    }),
  );

  onMount(() => {
    let alive = true;
    let unwatch = () => {};
    let unlisten = () => {};
    onCleanup(() => {
      alive = false;
      unwatch();
      unlisten();
      mount()?.destroy();
    });
    bootEngine().then(
      (engine) => {
        if (!alive) return;
        const m = mountViewer({ parent: host, text: doc.text, analyze: engine.memoize(), mode: mode() });
        unwatch = watchLocation(m.view, (at) => follow(m, at?.label ?? ""));
        unlisten = m.onDiagnostics(setShown);
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
      severity: f.severity,
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
    m.clipTo(null);
    reveal(m, doc.at(f.from), doc.at(f.to));
  });

  /** Lint the scan counted that the viewer draws less of (the two engines' policies differ). */
  const undrawn = () => {
    const drawn = new Map<string, number>();
    for (const i of items()) if (i.engine === "lint") drawn.set(i.code, (drawn.get(i.code) ?? 0) + 1);
    return (props.counts ?? []).flatMap((c) => {
      const counted = c.engine === "lint" ? (c.books.find((b) => b.book === props.book?.book)?.n ?? 0) : 0;
      const n = counted - (drawn.get(c.code) ?? 0);
      return n > 0 ? [{ code: c.code, n }] : [];
    });
  };

  /** Select a range, a third of the way down the pane, lit for a moment; the chapter indicator follows. */
  function reveal(m: ViewerMount, from: number, to: number) {
    pinned = null;
    m.reveal(from, to);
  }

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
      <div class="viewer-body">
        {/* Focusable, so a click in the text puts the [ ] keys here. `cm-host`
            is what the fork's editor.css keys its popover and gutter on. */}
        <div class="viewer-pane cm-host" ref={host} data-mode={mode()} tabIndex={0} aria-label="Book text" />
        <Show when={mount()}>
          {(m) => (
            <FindingsRail
              items={items()}
              counts={props.counts ?? []}
              book={props.book?.book}
              initial={props.check ?? null}
              onPick={(item) => reveal(m(), item.from, item.to)}
              onBook={(engine, code, book) => props.onBook?.(engine, code, book)}
              undrawn={undrawn()}
              mode={mode()}
              onShowMarkup={() => setMode("usfm")}
              compare={(item) => (
                <Show when={props.source} fallback={<p class="small compare">No source text was recorded for this book.</p>}>
                  {(src) => <SourceCompare source={src()} finding={item.compared} />}
                </Show>
              )}
            />
          )}
        </Show>
      </div>
    </div>
  );
}
