// The read-only USFM viewer: Sefer's editor, installed as a package
// (`@sefer/editor` and `@sefer/text`, from Sefer by git; see web/package.json)
// and mounted read-only. The engine is the hub's (src/engine/onion.ts); the
// package draws its diagnostics inline. This component adds the hub's own UI
// around it: the mode toggle, a chapter navigator that follows the scroll,
// proofreading findings as underlines, "go to this range", and the rail
// beside the text listing every mark it draws. It never touches CodeMirror:
// everything goes through the package's handle.
import { Show, createEffect, createMemo, createSignal, on, onCleanup, onMount } from "solid-js";
import "../engine/tokens.css";
import "@sefer/editor/editor.css";
import {
  anchorFrom,
  editorPolicy,
  mountBookEditor,
  seatBook,
  type AlignedGroup,
  type BookEditorMount,
  type CorpusFinding,
  type DrawnDiagnostic,
  type EditorBook,
} from "@sefer/editor";
import { makeBook } from "@sefer/text/book";
import type { Address } from "@sefer/text/location/address";
import { decode, offsetMap, type Source } from "@sefer/text/source";
import { Result } from "effect";
import { bootEngine } from "../engine/onion";
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

/** `3:16`, `3` for a chapter, `intro` before the first chapter: how the rail names a place. */
const label = (a: Address | undefined): string => {
  if (a === undefined || a.kind === "book" || a.kind === "intro") return "intro";
  if (a.kind === "chapters") return String(a.from);
  return `${a.from.chapter}:${a.from.verse}`;
};

/** Format characters (zero-width space, joiners, marks) as `⟨200B⟩`, so a row can show what it is about. */
const visible = (s: string) =>
  s.replace(/[\n\t]+/g, " ").replace(/\p{Cf}/gu, (c) => `⟨${c.codePointAt(0)!.toString(16).toUpperCase().padStart(4, "0")}⟩`);

export default function UsfmViewer(props: {
  text: string;
  findings: readonly ViewerFinding[];
  /** A range to select and scroll to. */
  focus: Range | null;
  /** A chapter label (`"3"`) to scroll to. */
  chapter: string | null;
  /** A verse to select and light, `3:16` (a link from a chart). */
  verse?: string | null;
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
  /** Draw the findings rail beside the text (default true). Off for a source pane. */
  rail?: boolean;
  /** The chapter label in view, as it changes: a second pane follows it. */
  onChapter?: (label: string) => void;
  /** A finding was picked in the rail: its place, `3:15`, for a second pane to light. */
  onPicked?: (ref: string) => void;
  /** Panes shown side by side, kept on the same verse as either one scrolls (the package's aligned group). */
  align?: AlignedGroup;
  onReady?: () => void;
}) {
  let host!: HTMLDivElement;
  const [mount, setMount] = createSignal<BookEditorMount>();
  /** The chapter label in view (`""` in the front matter). */
  const [current, setCurrent] = createSignal("");
  const [mode, setMode] = createSignal<Mode>("regular");
  createEffect(on(current, (l) => l && props.onChapter?.(l), { defer: true }));
  const [failed, setFailed] = createSignal<string>();
  // The package's canonical text: LF, no byte-order mark. The server's offsets
  // count the raw file, so they go through the offset map.
  const decoded = decode(new TextEncoder().encode(props.text));
  const source: Source | undefined = Result.isSuccess(decoded) ? decoded.success : undefined;
  const text = source?.text ?? "";
  const raw = offsetMap(props.text);
  let book: EditorBook | undefined;
  const [shown, setShown] = createSignal<readonly DrawnDiagnostic[]>([]);
  /** Sous Chef's source-compared rows by code and start, to find the evidence behind a drawn mark. */
  const comparedAt = createMemo(() => {
    const by = new Map<string, Compared>();
    for (const f of props.findings) {
      if (f.paired && f.chapter !== undefined) {
        by.set(`${f.code}@${raw.toCanonical(f.from)}`, { code: f.code, chapter: f.chapter, verse: f.verse, paired: f.paired });
      }
    }
    return by;
  });
  const items = createMemo<RailItem[]>(() => {
    const m = mount();
    return shown().map((d) => {
      const tag = d.source ?? "";
      const slash = tag.indexOf("/");
      const engine = tag.slice(0, slash);
      const code = tag.slice(slash + 1);
      return {
        compared: engine === "sous" ? comparedAt().get(`${code}@${d.from}`) : undefined,
        from: d.from,
        to: d.to,
        engine: engine === "onion" ? "lint" : engine,
        code,
        severity: d.severity === "hint" ? ("info" as const) : d.severity,
        message: d.message,
        ref: label(m?.addressAt(d.from)),
        context: visible(text.slice(Math.max(0, d.from - 12), d.to + 12)).trim(),
      };
    });
  });

  onMount(() => {
    let alive = true;
    let detach: (() => void)[] = [];
    onCleanup(() => {
      alive = false;
      for (const d of detach) d();
      mount()?.destroy();
      book?.close();
    });
    if (source === undefined) {
      setFailed("the file is not valid UTF-8");
      return;
    }
    bootEngine().then(
      (engine) => {
        if (!alive) return;
        book = seatBook(makeBook(props.book?.path ?? "book.usfm", source), { analyze: engine.memoize() });
        const m = mountBookEditor({ parent: host, book, readOnly: true });
        m.setPolicy(editorPolicy(mode()));
        detach = [
          m.onViewport((where, why) => follow(where?.label ?? "", why)),
          m.onDiagnostics(setShown),
          ...(props.align ? [props.align.join({ editor: m, book: book.id })] : []),
        ];
        setShown(m.diagnostics());
        setMount(m);
        props.onReady?.();
      },
      (e: unknown) => setFailed(String(e)),
    );
  });

  // The package keeps the reader in place across a mode flip and redraws its marks.
  createEffect(on([mount, mode], ([m, md]) => m?.setPolicy(editorPolicy(md))));

  createEffect(() => {
    const m = mount();
    if (!m || !book) return;
    const stamp = book.source().stamp;
    const list: CorpusFinding[] = props.findings.map((f, i) => ({
      id: `${i}`,
      code: f.code,
      severity: f.severity,
      message: f.message,
      from: raw.toCanonical(f.from),
      to: raw.toCanonical(f.to),
      stamp,
    }));
    m.setHostDiagnostics(list);
  });

  createEffect(() => {
    const m = mount();
    const f = props.focus;
    if (!m || !f) return;
    reveal(m, raw.toCanonical(f.from), raw.toCanonical(f.to));
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

  /** Select a range in the middle of the pane, lit for a moment; the chapter indicator follows. */
  function reveal(m: BookEditorMount, from: number, to: number) {
    pinned = null;
    // Select first, then jump (caret, scroll, flash). CodeMirror estimates the
    // height of text it has not drawn, so a jump deep into a book lands near the
    // place; once that part is drawn, a second scroll lands on it (the package's
    // `resume` does the same).
    m.reveal({ from, to }, { select: true, scroll: "none" });
    m.jump({ from, to }, "center");
    requestAnimationFrame(() => m.reveal({ from, to }, { select: true, scroll: "center" }));
    props.align?.lead(m);
  }

  const labels = () => {
    mount();
    return book?.structure().chapters.flatMap((c) => (c.label ? [c.label] : [])) ?? [];
  };

  // After a jump the indicator says where we asked to go, until the reader
  // scrolls: a short last chapter cannot reach the top of the pane, and the
  // scroll position alone would name the one above it (and "next" would stick).
  let pinned: string | null = null;
  function follow(l: string, why: "user" | "reveal") {
    if (pinned !== null) {
      if (why === "reveal") return;
      pinned = null;
    }
    setCurrent(l);
  }

  const goTo = (l: string) => {
    const m = mount();
    const row = book?.structure().chapters.find((c) => c.label === l);
    if (!m || !row) return;
    pinned = l;
    setCurrent(l);
    m.reveal({ from: anchorFrom(row) }, { scroll: "start" });
    props.align?.lead(m);
  };
  createEffect(on([mount, () => props.chapter], ([m, ch]) => m && ch && goTo(ch)));
  createEffect(
    on([mount, () => props.verse], ([m, v]) => {
      const [c, n] = (v ?? "").split(":").map(Number);
      if (!m || !book || !c || !n) return;
      pinned = null;
      m.revealAddress({ kind: "verses", book: book.id, from: { chapter: c, verse: n }, to: { chapter: c, verse: n } }, "center");
    }),
  );

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
            onPick={(l) => {
              goTo(l);
              host.focus({ preventScroll: true });
            }}
          />
        </Show>
        <Show when={!mount() && !failed()}><span class="loading small">Starting the viewer…</span></Show>
      </div>
      <Show when={failed()}>{(e) => <div class="failure">Viewer failed: {e()}</div>}</Show>
      <div class="viewer-body">
        {/* Focusable, so a click in the text puts the [ ] keys here. `cm-host`
            is what the package's editor.css keys its popover and gutter on. */}
        <div class="viewer-pane cm-host" ref={host} data-mode={mode()} tabIndex={0} aria-label="Book text" />
        <Show when={props.rail !== false && mount()}>
          {(m) => (
            <FindingsRail
              items={items()}
              counts={props.counts ?? []}
              book={props.book?.book}
              initial={props.check ?? null}
              onPick={(item) => {
                reveal(m(), item.from, item.to);
                props.onPicked?.(item.ref);
              }}
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
