# REUSABLE — making Sefer's `src/editor` importable by another app

Goal: the Translation Hub (or anything else) imports Sefer's editor directly
instead of forking it, and everything the editor now takes from
`src/core` is either passed in by the host or comes from a small shared leaf.

Based on Sefer `50aa9cd` and on what the fork in this spike actually needed
(SEAM.md). The short version: **the editor is already close.** Sefer injects
the engine (`analyzer` facet) and observability as parameters, and
`src/editor` has no Solid, `#app` or `#platform` imports. What is left is
compile-time coupling (the `#core/*` alias), one Effect value class
(`Refusal`), two import-time side effects, and a bundling problem
(`compose.ts`).

Each step below can land on its own, keeps Sefer green, and changes no
behaviour Sefer can observe unless the step says so.

---

## Seam by seam

| # | Seam | Proposed interface | Sefer passes | Hub viewer passes | Read-only relevant? |
|---|---|---|---|---|---|
| 1 | Engine parse | `type Analyze = (text: string) => Analysis` through the existing `analyzer` facet (`core/analyzer.ts`). **No change.** | `galley.memoize(id)` | `engine.memoize()` (`src/host/engine.onion.ts`: onion `parse` → `deserialize`) | **Yes**, the one capability it needs |
| 2 | Engine vocabulary (`Analysis`, `TOKEN`, `MARKERS`, `NOTE_PART`, `sameSource`, `diagnostic*`, …) | Not injectable: these are constants and pure functions over the wire format. Move `core/galley/analysis.ts` to a leaf that both core and editor import (see step 4), plus a shared builder: `makeAnalyze(parse: (text: string, opts: ParseOptions) => Uint8Array): Analyze` | `core/galley` re-exports the leaf (nothing changes for core callers); `galley.ts` builds `analyze` with `makeAnalyze(t => handle.parseText(t, PARSE_ALL))` | `makeAnalyze(onion.parse)` | **Yes** |
| 3 | Book/Source vocabulary types (`Change`, `Source`, `SourceStamp`, `Origin`, `Trust`, `Receipt`, `History`, `Book`, `BookId`) | Editor-owned `src/editor/ports.ts` declaring the same shapes. Types are structural, so core's types satisfy them with no adapter. | Nothing new; core values flow in as before | Nothing (write side) | No |
| 4 | `Refusal` (Effect `Data.TaggedError`) | Write side only. `satellite.ts`: `outcome instanceof Refusal` → `isRefusal(outcome)` = `outcome._tag === "Refusal"`. `book.ts` keeps constructing core's `Refusal`, but `book.ts` + `funnel.ts` + `satellite.ts` move behind a separate entry (`src/editor/index.ts`, unchanged) and out of the read entry. | Unchanged | Not imported | No |
| 5 | Observability | Already a parameter. Narrow the type to what the editor calls: `interface EditorTelemetry { level(): Level; operation(name: string, detail?: string, attrs?: Attrs): { note(...): void; end(...): void }; note(name: string, verdict: Verdict, detail?: string, attrs?: Attrs): void }` in `ports.ts`. Remove the import-time `onOrphanDerived(repaint)` in `observability.ts` → `export const installRepaintTelemetry = (): () => void`. | `composition.observability` (as now) and one `installRepaintTelemetry()` call in `services.ts` | Nothing (no-op) | No |
| 6 | Footnote clicks | Today: module global `setNoteToggler(fn)` in `decorations.ts`, set by *importing* `noteEditor.ts`. Proposed: `export const noteGestures = Facet.define<NoteToggler, NoteToggler>({ combine: v => v[0] ?? noop })`; `decorations.ts` reads `view.state.facet(noteGestures)`. Split the handler into `noteFollowing()` (caller → apparatus, back to caller; read-only-safe) and `noteEditing()` (adds "edit"). | `noteEditing()` (already in `mountable`, so no call-site change) | `noteFollowing()` | **Yes** — in the fork, footnote callers are inert because noteEditor isn't imported |
| 7 | Diagnostics on a read-only surface | `usfmLinter()` gains nothing new in its signature; inside, omit `actions` when `view.state.readOnly`. | Unchanged (its editor is not read-only) | `usfmLinter()` | **Yes** — today the viewer shows a "Fix:" button that does nothing (the spike blocks it with a change filter) |
| 8 | Theme | CSS custom properties. Document the contract at the top of `editor.css` (the ~25 `--surface-*`, `--on-surface-*`, `--brand-*`, `--font-*`, `--text-*`, `--radius-*`, `--shadow-large` names) and ship `src/editor/tokens.default.css` with Sefer's values. | `src/app/ui/tokens.css` (unchanged) | `tokens.default.css` or its own palette | **Yes** |
| 9 | Corpus findings (sous) | Already push-based: `showCorpusFindings(view, list)`. No change. | `ProjectAnalysis` output | Nothing, or findings from the hub's own pipeline later | Optional |
| 10 | Chapter / mode | Already pure functions over state: `modeView(name)`, `pickChapter(state, ordinal)`, `structureAt(state).chapters`. No change. | — | — | **Yes** |

Read-only-irrelevant (the hub never touches them): `editorBook`, `funnel`
(`fromCanonical`, `ownEcho`), satellites, `commandsLayer`, history,
`frontMatterCard`, `EditorAction`, insert/deletion/input/caret commands,
`keystrokeMeter`, `diffView` and `stamp` (useful later for history views, but
not needed now), `align` and `pairing` (these are what pull in `core/location`).

---

## Implicit state that has to become explicit

| Where | Today | Proposed |
|---|---|---|
| `recipes/noteEditor.ts` → `decorations.ts` | `setNoteToggler(...)` runs at import time; whoever imported last wins | Facet (step 6) |
| `observability.ts` | `onOrphanDerived(repaint)` runs at import time; `repaintInto` is one global for the process | `installRepaintTelemetry()` (step 5) |
| `core/compose.ts` | `commandsLayer = [keymap.of([...usfmKeys()])…]` is evaluated at import, so tree-shaking can't drop editing code for a reader | Split (step 3) |
| `core/timing.ts`, `core/instrument.ts`, `core/analyzer.ts` | Process-wide span ring, trace sequence, parse counter | Keep. These are meters, harmless to a second host. Document them as process-global. |
| `core/docStructure.ts` `recent`, `recipes/reader.ts` `lent`, `recipes/viewPool.ts` | Cross-view caches and pool | Keep (they are correct across hosts) |
| `core/galley/galley.ts` `instantiated` / wasm-bindgen `initSync` | One engine per process | Not the editor's problem once `Analyze` is injected; the host owns it. |

---

## Ordered steps (each one safe by itself)

1. **`src/editor/ports.ts` (types only).** Declare `Change`, `SourceStamp`,
   `Source`, `Origin`, `Trust`, `Receipt`, `History`, `Book`, `BookId`,
   `EditorTelemetry`. Repoint every `import type … from "#core/source/source" | "#core/book/book" | "#core/observability"`
   to `./ports`. No runtime change, so `tsc` is the whole test. *Removes 3 of the 6 `#core` modules from the editor's type graph.*

2. **`isRefusal` instead of `instanceof Refusal`** in `recipes/satellite.ts`.
   One line. `book.ts` still constructs core's `Refusal`; it is Sefer's Book
   adapter and stays core-aware.

3. **Split `core/compose.ts`.** Move `readingLayer` and `viewLayer` into
   `core/reading.ts` and have `compose.ts` re-export them, so existing imports
   keep working. *Measured in this spike*
   (`src/editor-fork/core/reading.ts`, `LAYERS=split`): the reader stops
   bundling `deletion`, `input`, `insert`, `@codemirror/commands`,
   `@codemirror/language`, and `@lezer/*`. That is **−83 KB min / −27 KB gzip
   of JS** (CodeMirror 294 → 230 KB, fork 80 → 61 KB).

4. **Move the engine vocabulary to a leaf.** `src/core/galley/analysis.ts` →
   `src/engine/analysis.ts` (the file is already pure; it imports only
   `@wycliffeassociates/scripture-kitchen/reader`), and add
   `makeAnalyze(parse)` there, built from the body of `galley.ts`'s
   `analyze` minus the span. `core/galley/analysis.ts` becomes
   `export * from "#engine/analysis"`, so core callers don't change. Add the
   layer to `tools/boundaries`: `engine` imports nothing from the app;
   `core` and `editor` may import `engine`. (Alternative: upstream it into
   scripture-kitchen as `/analysis`, since it describes their wire format.)
   After steps 1–4 the editor's only `#core` imports are `book.ts` (`Refusal`,
   `UNTRUSTED`, `asChangeList`, `makeListeners`: Sefer's write seat) and
   `recipes/align.ts` (`core/location`: the aligned group). Neither is on the read path.

5. **Remove the import-time side effects.** `noteGestures` facet with
   `noteFollowing()` and `noteEditing()` (step 6 in the table), and
   `installRepaintTelemetry()` called once from `src/app/services.ts`.
   This is the one step that changes wiring in Sefer, and only in two places:
   `mountable` already installs `noteEditing()`, and `services.ts` gets one call.

6. **Read-only linter.** Leave out `actions` when `state.readOnly`.

7. **A read entry: `src/editor/read.ts`.** It exports only the read-side names:
   `analyzer`, `type Analyze`, `readingLayer`, `viewLayer`, `modeView`,
   `editorPolicy`, `policyView`, `pickChapter`, `structureAt`, `anchorFrom`,
   `type ChapterRow`, `usfmLinter`, `showCorpusFindings`, `lintHoverGrace`,
   `noteFollowing`, `flash`, `watchLocation`, `mountReader`, plus a new
   `mountViewer` recipe (this spike's `src/viewer/mountViewer.ts`: about 70 lines,
   i.e. `mountReference` without pairing, plus the linter). Add a boundaries
   rule: **nothing reachable from `read.ts` imports `#core/*`.** After steps
   1–4 that already holds, so the rule only keeps it that way.

8. **Make it resolvable from outside Sefer.** The `#core/*` / `#editor/*`
   aliases live in `tsconfig.json` `paths` and Vite's `tsconfigPaths`, and a
   consumer doesn't have those. Move them to **package.json `"imports"`**
   (`"#engine/*": "./src/engine/*"` …). Node, Vite and TS (`moduleResolution: bundler`)
   resolve subpath imports relative to the *owning* package, so they keep
   working when Sefer is a dependency. Then add
   `"exports": { "./editor/read": "./src/editor/read.ts", "./editor/editor.css": …, "./editor/tokens.default.css": … }`.
   The hub depends on `github:…/Sefer#<tag>` and ships the TS through Vite,
   the same way scripture-kitchen's `.ts` readers already ship.
   (Sefer's runtime deps would install too; they are tree-shaken, but a
   dedicated `packages/editor` workspace package is cleaner once there is a
   second consumer.)

After steps 1–7 the hub's integration is:

```ts
import { mountViewer } from "sefer/editor/read";
import "sefer/editor/editor.css";
import "sefer/editor/tokens.default.css";
import { makeAnalyze } from "sefer/engine/analysis";
import { initSync, parse } from "@wycliffeassociates/scripture-kitchen/onion/web";

initSync({ module: await (await fetch(onionWasmUrl)).arrayBuffer() });
const analyze = makeAnalyze(parse); // + a one-entry memo, as galley.memoize() does
const viewer = mountViewer({ parent, text, analyze, mode: "regular" });
viewer.clipTo(22); viewer.setMode("usfm");
```

## What not to do

- Don't wrap the editor in an Effect service or ask the host for a `Galley`
  Layer. The editor already takes a plain function, and that is what lets the
  viewer ship with **0 bytes of Effect**.
- Don't turn the engine vocabulary into an injected object. It is constants
  that the hot paths (`decorations.ts`, `cst.ts`) read in tight loops. It is a
  dependency on the wire format, not a capability.
- Don't make the hub take `editorBook`. A Book is a write seat with a journal,
  and the reader/reference recipes exist so that read-only surfaces don't need one.
