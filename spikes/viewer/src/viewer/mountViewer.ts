/**
 * The hub's read-only USFM viewer, assembled ONLY from the forked editor's
 * public pieces (src/editor-fork/index.ts) — the same recipe Sefer's
 * `mountReference` uses (recipes/reference.ts), plus the engine linter, minus
 * the pairing/alignment apparatus.
 *
 * Inputs the host injects: the text, and an `Analyze` (src/host/engine.ts).
 * Nothing else from the hub reaches the editor.
 */

import { forEachDiagnostic } from "@codemirror/lint";
import { Compartment, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

// Imported module-by-module rather than through the fork's index.ts, so the
// one switch below (`virtual:viewer-layers`) decides whether compose.ts — and
// with it every editing command — is in the bundle. See vite.config.ts.
import { analyzer, type Analyze } from "../editor-fork/core/analyzer";
import { anchorFrom } from "../editor-fork/core/clip";
import { structureAt, type ChapterRow } from "../editor-fork/core/docStructure";
import { flash, flashing } from "../editor-fork/recipes/flash";
import { usfmLinter } from "../editor-fork/recipes/lint";
import { modeView, pickChapter, type ProjectionName } from "../editor-fork/views";
import { readingLayer, viewLayer } from "virtual:viewer-layers";

export interface ViewerOptions {
  readonly parent: HTMLElement;
  readonly text: string;
  readonly analyze: Analyze;
  readonly mode?: ProjectionName;
  /** Draw the engine's diagnostics inline (default true). */
  readonly diagnostics?: boolean;
}

/** A diagnostic as the viewer draws it: engine lint (`onion/…`) or a corpus finding (`sous/…`). */
export interface Shown {
  readonly from: number;
  readonly to: number;
  readonly severity: "error" | "warning" | "info" | "hint";
  /** `onion/<rule>` or `sous/<code>`. */
  readonly source: string;
  readonly message: string;
}

export interface ViewerMount {
  readonly view: EditorView;
  chapters(): readonly ChapterRow[];
  setMode(mode: ProjectionName): void;
  /** Clip to one chapter (ordinal into `chapters()`), or `null` for the whole book. */
  clipTo(ordinal: number | null): void;
  /** Whole book stays visible; scroll the chapter to the top. */
  scrollTo(ordinal: number): void;
  /** Select `from`–`to`, put it a third of the way down the pane, and light it up. */
  reveal(from: number, to: number): void;
  /** What the linter is drawing now, in document order. */
  diagnostics(): readonly Shown[];
  /** Called whenever that set changes (the linter runs ~150 ms after a change). */
  onDiagnostics(listener: (shown: readonly Shown[]) => void): () => void;
  destroy(): void;
}

export function mountViewer(options: ViewerOptions): ViewerMount {
  const projection = new Compartment();
  const listeners = new Set<(shown: readonly Shown[]) => void>();
  const diagnostics = (state: EditorState): Shown[] => {
    const out: Shown[] = [];
    forEachDiagnostic(state, (d, from, to) =>
      out.push({ from, to, severity: d.severity, source: d.source ?? "", message: d.message }),
    );
    return out;
  };
  // The lint plugin publishes by dispatching an effect, so recount on effects;
  // but scrolls, flashes and selections are effects too, so tell listeners only
  // when the set really changed.
  let last = "";
  const announce = EditorView.updateListener.of((update) => {
    if (listeners.size === 0 || !update.transactions.some((tr) => tr.effects.length > 0)) return;
    const shown = diagnostics(update.state);
    const key = shown.map((d) => `${d.from}-${d.to}-${d.source}`).join(",");
    if (key === last) return;
    last = key;
    for (const listener of listeners) listener(shown);
  });
  let stopFlash = () => {};
  const state = EditorState.create({
    doc: options.text,
    extensions: [
      analyzer.of(options.analyze),
      readingLayer,
      viewLayer(),
      projection.of(modeView(options.mode ?? "regular")),
      options.diagnostics === false ? [] : usfmLinter(),
      flashing(),
      announce,
      // Read-only, three ways: commands (`readOnly`), the DOM (`editable`),
      // and — because the linter's Fix actions dispatch changes directly —
      // a change filter that refuses every document change.
      EditorState.readOnly.of(true),
      EditorView.editable.of(false),
      EditorState.changeFilter.of(() => false),
    ],
  });
  const view = new EditorView({ state, parent: options.parent });

  return {
    view,
    chapters: () => structureAt(view.state).chapters,
    setMode: (mode) => view.dispatch({ effects: projection.reconfigure(modeView(mode)) }),
    clipTo: (ordinal) => {
      view.dispatch(pickChapter(view.state, ordinal));
      view.scrollDOM.scrollTop = 0;
    },
    scrollTo: (ordinal) => {
      const row = structureAt(view.state).chapters[ordinal];
      if (row === undefined) return;
      view.dispatch({ effects: EditorView.scrollIntoView(anchorFrom(row), { y: "start" }) });
    },
    reveal: (from, to) => {
      const length = view.state.doc.length;
      const anchor = Math.max(0, Math.min(from, length));
      const head = Math.max(anchor, Math.min(to, length));
      view.dispatch({
        selection: { anchor, head },
        effects: EditorView.scrollIntoView(anchor, { y: "start", yMargin: view.scrollDOM.clientHeight / 3 }),
      });
      stopFlash();
      stopFlash = flash(view, { from: anchor, to: head }, 1600);
    },
    diagnostics: () => diagnostics(view.state),
    onDiagnostics: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    destroy: () => {
      stopFlash();
      listeners.clear();
      view.destroy();
    },
  };
}
