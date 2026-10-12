/**
 * SPIKE ADDITION — not in Sefer. The reading half of `compose.ts`, on its own.
 *
 * `compose.ts` defines `readingLayer`/`viewLayer` next to `commandsLayer`,
 * whose top-level `keymap.of([...usfmKeys()])` is evaluated at import, so any
 * importer of compose.ts pays for every editing command (deletion, input,
 * insert, @codemirror/commands → @codemirror/language → @lezer/*). This file is
 * the proposed split, verbatim bodies, to measure what a read-only consumer
 * saves. See REUSABLE.md step 3.
 */
import { Decoration, EditorView, drawSelection } from "@codemirror/view";
import type { Extension } from "@codemirror/state";

import type { ViewOptions } from "./compose";
import { decoField, pickField, structureField, usfmModeProjection } from "./editorState";
import { renderRangeField, renderWindow } from "./render";
import { scrollGuard } from "./scroll";

export const readingLayer: Extension = [
  structureField,
  pickField,
  usfmModeProjection,
  renderRangeField,
  decoField,
];

export function viewLayer(options: ViewOptions = {}): Extension {
  return [
    drawSelection(),
    EditorView.lineWrapping,
    ...(options.perLineDirection === false ? [] : [EditorView.perLineTextDirection.of(true)]),
    renderWindow,
    scrollGuard,
    EditorView.atomicRanges.of((v) => v.state.field(decoField, false)?.atomic ?? Decoration.none),
    EditorView.bidiIsolatedRanges.of(
      (v) => v.state.field(decoField, false)?.isolates ?? Decoration.none,
    ),
  ];
}
