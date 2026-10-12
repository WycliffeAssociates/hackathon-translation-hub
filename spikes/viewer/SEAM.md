# SEAM — what `src/editor` imports from outside itself

Fork source: Sefer `50aa9cd0dad961952ad573407bfe7f7e0dcaf67c` (committed HEAD), `src/editor/**`.
`src/editor-fork/` is byte-identical to that tree except: `testing/` and
`recipes/emptyBlocks.test.ts` dropped (test-only), and one spike file added
(`core/reading.ts`, see REUSABLE.md step 3). Its `#core/*` imports are left
exactly as Sefer writes them. `vite.config.ts` / `tsconfig.json` point the
alias at `src/host/*`, so the alias is the seam: everything under `src/host/`
is what a host has to provide.

**The whole fork, not just the viewer's slice, typechecks against these shims**
(`pnpm typecheck`). Book, funnel, satellite, diffView, align, noteEditor and the
rest all compile. So this table covers the editor's complete outside surface.

Decisions: **copied** = verbatim file from Sefer HEAD (`cmp`-identical);
**stubbed** = cut down or re-implemented to the editor's needs;
**injected** = the host passes it at runtime; nothing is imported.

## Imports from `#core/*` (the only outside layer `src/editor` touches — no `#app`, `#platform`, Solid, or Tauri)

| Import | What it provides | Used by (files) | Decision | Why |
|---|---|---|---|---|
| `#core/galley` → `analysis.ts` | The engine's vocabulary: `Analysis` type (dish + text + hash + revision); wire constants `TOKEN`, `FLAG`, `NOTE_PART`, `MARKERS`, `NONE`, `NODE_ID_BIT`, `TOKEN_BLANK`, `TOKEN_DELIMITER_FOLDED`, `TOKEN_SPELLING_BIT`, `Category`, `CloseReason`, `ClosingBehavior`, `MarkerKind`, `SpecContext`, `StructuralWhitespaceRequirement`, `TokenKind`, `isMarkerKind`; reader types `NodeView`, `TokenView`, `Tree`, `NotePart`, `EngineStamp`; pure fns `classWordOf`, `describesExactly`, `sameSource`, `stampOf`, `stampMatches`, `versionOf`, `diagnosticName/Message/Severity/FixLabel` | 15: docStructure, cst, mapping, decorations, lineTable, blockTable, plan, owned, fold, notes, analyzer, compose, lint, reader, stamp, diffView, noteEditor | **copied** (`src/host/galley/analysis.ts`) | Pure (382 lines). Depends only on `@wycliffeassociates/scripture-kitchen/reader`. It is wire-format vocabulary, not a capability — there is nothing to inject; the editor genuinely needs the constants. Should become a shared leaf module (or move upstream into scripture-kitchen). |
| `#core/galley` (index) → `galley.ts`, `diff.ts`, `format.ts`, `overlay.ts` | The `Galley` Effect service + Layer, wasm boot, corpus (sous), find, toc, mask, diff/merge/format/overlay doors | — (the editor imports none of these names; they ride in only because `index.ts` re-exports them) | **not copied**; `src/host/galley/index.ts` re-exports only `analysis` + `location` | The editor never calls the engine directly — see "engine" row below. Leaving them out is what keeps Effect and the 1.28 MB galley handle optional. |
| *(implicit)* the engine | `Analyze = (text) => Analysis` — a synchronous, memoised parse | `core/analyzer.ts` facet, read by docStructure, compose, lint, reader, reference, book | **injected** (already, by Sefer's design: `analyzer.of(fn)` / `editorBook({ analyze })` / `mountReference({ analyze })`) | Sefer passes `galley.memoize(id)`. The viewer passes `src/host/engine.*.ts` — 60 lines over the wasm `parseText`/`parse` door, no Effect. Two interchangeable engines measured (galley handle vs onion-only). |
| `#core/galley/location` | `tocViewOf(analysis)` — chapters/verses index view | `recipes/align.ts` | **copied** (`src/host/galley/location.ts`) | Pure, 83 lines. Only the aligned-group recipe uses it; the viewer never reaches it (tree-shaken). |
| `#core/location/address` | `Address` type (book/chapter/verse coordinates) | `recipes/align.ts` (type only) | **copied** | Pure; `locate.ts` needs it. |
| `#core/location/locate` | `addressAt`, `resolve`, `TocView` — offset ↔ address | `recipes/align.ts` | **copied** | Pure (474 lines). Only align uses it; not in the viewer bundle. |
| `#core/book/book` — types | `BookId`, `Origin`, `Trust`, `Receipt`, `History`, `ChangeListener`, `Book` | book.ts, funnel.ts, align.ts | **stubbed** (types copied verbatim into `src/host/book/book.ts`) | The editor-backed Book *implements* core's `Book` interface. Write side only; read-only-irrelevant. Types are structural, so an editor-owned copy is compatible. |
| `#core/book/book` — `Refusal` (value) | An `effect` `Data.TaggedError` | book.ts (`new Refusal`), satellite.ts (`instanceof Refusal`) | **stubbed** (plain `Error` subclass with the same `_tag` and fields) | The only runtime value from core that drags `effect` in. Write path only. A `_tag` check would decouple it. |
| `#core/book/book` — `UNTRUSTED`, `asChangeList`, `makeListeners` | Tiny pure helpers | book.ts | **copied** (into the stub) | 30 lines, pure. |
| `#core/book/book` — `makeBook`, `openBook`, `applyAll` | The plain Book over `Source` (Effect FileSystem) | — (not imported by the editor; test file only) | **not copied** | — |
| `#core/source/source` | `Change`, `Source`, `SourceStamp` | book.ts, funnel.ts, lint.ts | **stubbed** (types only, `src/host/source/source.ts`) | Type-only imports; the module's runtime (`decode`/`encode`/`applyChange`, Effect `Result`) is never called by the editor. |
| `#core/observability` | `ObservabilityService`, `Operation`, `Level`, `Verdict` | editor `observability.ts`, book.ts | **stubbed** (types-only cut, lines 1–273 minus the Effect import) | Type-only. The service itself is **already injected** (`editorBook({ observability })`, `observabilityTracer(service)`, `annotateRepaint({...})`). The viewer passes nothing and none of that code is bundled. |

## Other things the editor depends on that are not `import` statements

| Dependency | What it provides | Decision | Why |
|---|---|---|---|
| CSS custom properties from `src/app/ui/tokens.css` | ~25 tokens `editor.css` reads: `--surface-*`, `--on-surface-*`, `--brand-base/light`, `--font-ui/scripture/mono/sans`, `--text-body/small`, `--radius-*`, `--shadow-large` (the `--editor-*` tokens are defined in editor.css itself) | **copied** (`src/host/tokens.css`: the `:root`/dark blocks + the font/size/radius lines of `@theme`; Tailwind bridge dropped) | This is already a clean injection point — CSS variables. It just isn't documented as a contract. Without it the text renders, but in the browser's default monospace with no colour. |
| `editor.css` | All of the projection's look (regular mode is *made* of CSS: hidden markers, verse pips, poetry indents) | **copied** (part of the fork) | Not optional: regular mode without it is raw markup. |
| `@codemirror/state`, `view`, `lint`, `commands` | CodeMirror | npm deps | `commands` (and through it `language` + `@lezer/*`) is only needed for editing — see REUSABLE.md step 3. |
| `@wycliffeassociates/scripture-kitchen` `#v0.1.9` | `/reader` (Dish decoder, used by analysis.ts) + a wasm | npm dep (same GitHub tag reference as Sefer) | v0.1.9 is the tag Sefer pins. |
| `effect` | `Result` (book.ts), `Data` (core `Refusal`) | npm dep for **typecheck only** | **0 bytes in the viewer bundle** — everything that uses it is unreachable from the read-only path and tree-shakes away. |
| Solid | — | **not used at all** | `src/editor` has no Solid import; the shell (`src/app/ui/BookEditor.tsx`) is the only Solid layer. |
| `__GALLEY_TAG__` (Vite define) | engine tag string | not needed | Only `galley.ts` reads it, which is not copied. |

## Implicit global state inside `src/editor` (not imports, but seams)

| Where | What | Effect on a second host |
|---|---|---|
| `recipes/noteEditor.ts` (import side effect) → `core/decorations.ts` `setNoteToggler` | A module-global callback for footnote clicks (caller → apparatus, back, edit). Installed by *importing* noteEditor. | In the viewer it is never imported, so the default no-op runs: **clicking a footnote caller does nothing**. Should be a facet. |
| `observability.ts` (import side effect) → `core/instrument.ts` `onOrphanDerived(repaint)` | One process-wide handler for the repaint telemetry burst | Only matters if imported; one handler per process means two hosts on a page would fight. |
| `core/timing.ts`, `core/instrument.ts`, `core/analyzer.ts` | Module-level span ring, trace sequence, parse counter | Harmless meters, but process-wide. |
| `core/docStructure.ts` `recent` | Last-built structure, reused when the text fits | A cross-view cache; safe (checked by `fits`). |
| `recipes/viewPool.ts` | A pool of up to 32 `EditorView`s for excerpt cards | Process-wide; fine. |
| `core/compose.ts` `commandsLayer` | Top-level `keymap.of([...usfmKeys()])` evaluated on import | Not state, but makes every editing command non-tree-shakeable for anyone importing `readingLayer`. |
| engine (`initSync`) | wasm-bindgen glue instantiates into module-global state | One engine per page. Sefer guards it with `instantiated`; the viewer with a `booted` promise. |

## Tally

- **Injected: 2** — the engine (`Analyze`, via the `analyzer` facet) and observability (already parameters in Sefer). Plus theming, which is injected via CSS variables but undocumented.
- **Copied: 5 modules** — `galley/analysis.ts`, `galley/location.ts`, `location/address.ts`, `location/locate.ts` (all byte-identical), and the token CSS subset.
- **Stubbed: 3 modules** — `book/book` (types + plain-class `Refusal`), `source/source` (types), `observability` (types).
- **Not needed: 4** — galley.ts/diff/format/overlay (the Effect Layer and the corpus doors).

**Read-only-irrelevant:** `book/book`, `source/source`, `observability`, `Refusal`, and the location trio (aligned group only). A read-only consumer
needs exactly **`analysis.ts` + an `Analyze` + the CSS tokens.**
