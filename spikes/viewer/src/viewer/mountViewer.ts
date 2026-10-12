/**
 * The hub's read-only USFM viewer, assembled ONLY from the forked editor's
 * public pieces (src/editor-fork/index.ts) — the same recipe Sefer's
 * `mountReference` uses (recipes/reference.ts), plus the engine linter, minus
 * the pairing/alignment apparatus.
 *
 * Inputs the host injects: the text, and an `Analyze` (src/host/engine.ts).
 * Nothing else from the hub reaches the editor.
 */

import { Compartment, EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

// Imported module-by-module rather than through the fork's index.ts, so the
// one switch below (`virtual:viewer-layers`) decides whether compose.ts — and
// with it every editing command — is in the bundle. See vite.config.ts.
import { analyzer, type Analyze } from "../editor-fork/core/analyzer";
import { anchorFrom } from "../editor-fork/core/clip";
import { structureAt, type ChapterRow } from "../editor-fork/core/docStructure";
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

export interface ViewerMount {
  readonly view: EditorView;
  chapters(): readonly ChapterRow[];
  setMode(mode: ProjectionName): void;
  /** Clip to one chapter (ordinal into `chapters()`), or `null` for the whole book. */
  clipTo(ordinal: number | null): void;
  /** Whole book stays visible; scroll the chapter to the top. */
  scrollTo(ordinal: number): void;
  destroy(): void;
}

export function mountViewer(options: ViewerOptions): ViewerMount {
  const projection = new Compartment();
  const state = EditorState.create({
    doc: options.text,
    extensions: [
      analyzer.of(options.analyze),
      readingLayer,
      viewLayer(),
      projection.of(modeView(options.mode ?? "regular")),
      options.diagnostics === false ? [] : usfmLinter(),
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
    destroy: () => view.destroy(),
  };
}
