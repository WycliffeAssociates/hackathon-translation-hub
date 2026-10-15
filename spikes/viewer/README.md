# Spike: Sefer's CodeMirror editor as a read-only USFM viewer

> **Retired 2026-10-15.** The hub now installs Sefer's editor as packages
> (`@sefer/editor`, `@sefer/text`, from Sefer by git; see `web/package.json`
> and `EDITOR-PIECES.md`), so the fork and this spike's code are gone. The
> engine host moved to `web/src/engine/onion.ts`. This write-up, `REUSABLE.md`
> and `SEAM.md` stay as the record of why; the code is in git at `709b467`.

**Question:** can Sefer's `src/editor` be forked into a standalone, read-only
USFM viewer for the Translation Hub, and what does it cost to ship?

**Answer: yes, and with little effort.** The fork is Sefer's `src/editor`
byte-for-byte. The host supplies 3 copied pure modules, 3 type-only stubs, and
a 60-line engine adapter. It renders Psalms in regular and USFM modes, with
chapter clip/jump, engine diagnostics drawn inline, no editing, and smooth
scrolling. It ships with **no Solid and no Effect.**

- Forked from **Sefer `50aa9cd0dad961952ad573407bfe7f7e0dcaf67c`** (the committed HEAD; uncommitted work was not used). Exported with `git archive HEAD src/editor`.
- Engine: `@wycliffeassociates/scripture-kitchen` `github:WycliffeAssociates/scripture-kitchen#v0.1.9`, the same reference Sefer uses.
- Samples: `en_ulb` from `dev-data/repositories/wa-catalog/en_ulb.git` (`19-PSA.usfm`, 266 KB, 150 psalms; `08-RUT.usfm`, 14 KB). `08-RUT-broken.usfm` is Ruth with injected errors, used to exercise diagnostics.

Deliverables: **[SEAM.md](SEAM.md)** (import → copied/stubbed/injected table),
**[REUSABLE.md](REUSABLE.md)** (how to refactor Sefer so the hub can import the
editor instead of forking it).

## Layout

```
src/editor-fork/   Sefer src/editor @ 50aa9cd, verbatim (testing/ and *.test.ts dropped)
                   + core/reading.ts (spike addition: the proposed compose.ts split)
src/host/          what the `#core/*` alias resolves to here: the seam
  galley/analysis.ts, galley/location.ts, location/{address,locate}.ts   copied, cmp-identical
  galley/index.ts                    shim: re-exports analysis+location only
  book/book.ts, source/source.ts, observability.ts   type stubs (+ plain-class Refusal)
  engine.galley.ts / engine.onion.ts   the injected Analyze (no Effect)
  tokens.css                         CSS custom properties editor.css reads
src/viewer/mountViewer.ts   the read-only viewer (~70 lines over the fork's public pieces)
src/demo/                   demo page
tools/measure.mjs           headless Chrome timing (Playwright borrowed from Sefer's node_modules)
tools/sizes.mjs             bundle size by part (source-map-explorer)
tools/probe.mjs             diagnostics probe
```

## Run

```sh
cd spikes/viewer
pnpm install                        # pnpm-workspace.yaml skips msgpackr-extract's build script on purpose
pnpm dev                            # http://localhost:5173/?book=19-PSA  (&mode=usfm, book=08-RUT, 08-RUT-broken)
pnpm typecheck                      # the WHOLE fork typechecks against src/host
pnpm build && pnpm sizes            # default: galley engine, verbatim compose.ts
pnpm build:lean && pnpm sizes       # ENGINE=onion LAYERS=split
pnpm measure                        # needs a build; THROTTLE=4 pnpm measure for 4x CPU slowdown
pnpm probe                          # diagnostics counts + hover screenshots → dist-shots/
```

Build switches (`vite.config.ts`): `ENGINE=onion` boots the onion-only parser
wasm instead of the galley handle. `LAYERS=split` takes `readingLayer`/`viewLayer`
from `core/reading.ts` instead of `core/compose.ts`.

## What works

| Feature | Status | How |
|---|---|---|
| Read-only | ✅ | `EditorState.readOnly` + `EditorView.editable(false)` (as in Sefer's `reference.ts`), plus `EditorState.changeFilter(() => false)`, because the linter's Fix button dispatches changes directly. Measured: typing, Backspace and Enter leave the doc unchanged; `contenteditable="false"`. |
| Regular (rendered) ↔ USFM (raw) | ✅ | `modeView("regular" \| "usfm")` in a compartment. Screenshots: `dist-shots/19-PSA-regular.png`, `-usfm.png`. |
| Chapter navigation | ✅ | Clip: `pickChapter(state, ordinal)` (`19-PSA-clip.png` shows Psalm 23 alone). Jump without clipping: `scrollIntoView(anchorFrom(row))`. The chapter list comes from `structureAt(state).chapters`. |
| Diagnostics drawn in the text | ✅ | `usfmLinter()`: engine diagnostics as underlines with hover tooltips (`08-RUT-broken-*-hover.png`: "verse 5 repeats the verse at 5; expected 6", "\s5 is not a known marker"). In regular mode, findings inside hidden markup are suppressed by design (Psalms: 0 marks in regular, 8 in the first USFM screen, mostly `\s5`). |
| Long book, smooth scroll | ✅ | Sefer's render window decorates only the viewport. Scrolling the full Psalms (328,000 px, 547 frames): p95 frame 18.6 ms, max 19.8 ms, 0 frames over 50 ms. At 4x CPU throttle: p95 23.8 ms, max 28 ms. |
| No Solid, no Effect | ✅ | `src/editor` imports neither Solid nor `#app`. Effect is reachable only from write paths (`book.ts` `Result`, core `Refusal`) and tree-shakes to **0 bytes**. |

## What doesn't (yet)

- **Footnote callers are inert.** Click-to-follow is a module-global callback installed as an import side effect of `recipes/noteEditor.ts` (`setNoteToggler`). The viewer doesn't import the note *editor*, so the default no-op runs. Not seen in the samples (ULB Psalms/Ruth have no `\f`); found by reading the code. Fix: REUSABLE.md step 5.
- **The "Fix: …" button still shows** in diagnostic tooltips. It does nothing (the change filter blocks it). Fix: REUSABLE.md step 6 (omit actions when `readOnly`).
- `\s5` (the ULB chunk marker) reports "not a known marker" in USFM mode. That is correct engine behaviour; `setExtensions('[{"name":"s5",…}]')` could register it if the hub wants silence.
- No lint gutter, `lintHoverGrace` or `watchLocation` (where-am-I) installed. They are available in the fork; this spike didn't wire them in.
- USFM mode uses Sefer's dark code look regardless of page theme (that's editor.css).
- Corpus (sous) findings aren't shown. The hook is there (`showCorpusFindings(view, list)`), but the hub has no corpus pipeline in the browser.

## Measurements

Machine: Apple M1 Max, headless Chromium (Playwright 1.63), `vite preview` on
localhost, fresh browser context per run (cold HTTP cache), median of 5.
Timings run from module start, so they exclude network time for the JS and
wasm. Over a real network the **wasm download is the dominant cost** (see sizes).

### Bundle size

Three builds. "JS" means all emitted chunks. Compressed sizes are gzip -9 / brotli q11 per file.

| Build | JS raw | JS gzip | JS brotli | CSS gzip | wasm raw | wasm gzip | wasm brotli | **Total gzip** | **Total brotli** |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| A. verbatim fork + galley engine (`pnpm build`) | 438 KB | 135.7 KB | 118.2 KB | 6.5 KB | 1,255 KB | 484.7 KB | 354.7 KB | **626.7 KB** | **478.5 KB** |
| B. + compose split (`LAYERS=split`) | 354 KB | 108.3 KB | 94.8 KB | 6.5 KB | 1,255 KB | 484.7 KB | 354.7 KB | 599.4 KB | 455.1 KB |
| C. + onion-only engine (`pnpm build:lean`) | 342 KB | 106.4 KB | 93.1 KB | 6.5 KB | 527 KB | 214.7 KB | 163.8 KB | **327.6 KB** | **262.6 KB** |

Minified JS by part (source-map attribution). Gzip is estimated from each chunk's ratio:

| Part | A (verbatim) min | B/C (split) min | ≈ gzip (B/C) |
|---|---:|---:|---:|
| CodeMirror (`state`, `view`, `lint`; A also has `commands`, `language`, `@lezer/*`) | 294.4 KB | 229.9 KB | 73 KB |
| Effect | **0** | **0** | 0 |
| Solid | **0** (not a dependency) | 0 | 0 |
| Forked editor code (`src/editor-fork`) | 80.3 KB | 61.2 KB | ≈ 18 KB |
| onion reader (Dish decoder, `scripture-kitchen/reader`) | 36.0 KB | 35.9 KB | ≈ 10 KB |
| wasm-bindgen glue (galley in A/B; onion in C) | 21.2 KB | 21.2 / 8.7 KB | 4.6 / 2.7 KB |
| host shims (copied `analysis.ts`, engine adapter) | 2.3 KB | 2.3 KB | < 1 KB |
| viewer + demo page | 2.2 KB | 2.3 KB | 1.6 KB |
| `editor.css` + tokens | 31 KB raw | 31 KB raw | 6.1 KB |
| **wasm** | galley 1,255 KB | galley 1,255 / onion 527 KB | 485 / 215 KB |

Two findings:
1. **The verbatim fork ships every editing command.** `compose.ts` evaluates
   `commandsLayer` at import, so a reader that only wants `readingLayer` still
   gets `deletion`/`input`/`insert` and `@codemirror/commands` → `language` → `@lezer/*`.
   Splitting it (B) saves 83 KB min / 27 KB gzip of JS.
2. **The viewer only needs the onion parser, not the galley handle.** The
   galley wasm adds the corpus/sous/find/toc/mask/diff machinery a viewer never
   calls. The onion `parse` door produces the same Dish: diagnostic counts are
   identical in every probe. Switching saves 270 KB gzip / 191 KB brotli. This
   is the largest single saving available.

### Time to first render

| | Psalms (266 KB, 150 ch.) | Ruth (14 KB) |
|---|---:|---:|
| wasm fetch + instantiate (localhost) | 9–17 ms | 8–13 ms |
| engine parse (`Analyze`, inside mount) | 14–15 ms | 5 ms |
| `new EditorView` (structure + plan + first decorations) | **81 ms** | 27 ms |
| mount → first painted frame | **116 ms** | 37 ms |
| module start → painted | 130 ms | 50 ms |
| navigation start → painted | **157 ms** | 76 ms |
| *4x CPU throttle:* parse / mount / mount→paint / nav→paint | 55 / 307 / 435 / 553 ms | 20 / 120 / 160 / 291 ms |

Interactions on Psalms, in-page, from the call to the next frame
(unthrottled / 4x): USFM toggle 30 / 71 ms, back to regular 15 / 38 ms,
clip to Psalm 23 18 / 44 ms, unclip 15 / 30 ms, jump to Psalm 150 21 / 52 ms.

Where Psalms' first render goes: the wasm parse is about 15 ms. Most of the
~80 ms mount is the editor building its `DocStructure`, block plan and paint
index over the whole book. That is a one-time cost per text; scrolling after
that stays at frame rate. A hub that can tolerate showing one chapter first
could mount a clip (or a chapter-sized `mountReader`) and still pay the
structure cost, because the structure is book-wide. If that ever matters, the
lever is in `docStructure.ts`, not the engine.

## Recommendation

**Fork now; refactor Sefer toward the seams in parallel, and drop the fork
once step 7 of REUSABLE.md lands.**

- The fork costs almost nothing to keep in sync *today*. `src/editor-fork` is
  byte-identical to Sefer's, so updating it is `git archive <new-sha> src/editor`
  plus re-running `pnpm typecheck` against the shims. Only the 3 copied core
  files can drift.
- The refactor is small and low-risk because Sefer already did the hard part:
  the engine and observability are parameters, and `src/editor` has no Solid or
  app imports. The concrete seams, in order:
  1. `src/editor/ports.ts`: editor-owned copies of the Book/Source/observability types (type-only).
  2. `isRefusal()` instead of `instanceof Refusal` in `satellite.ts`.
  3. Split `compose.ts` → `core/reading.ts` (measured: −27 KB gzip for readers).
  4. Move `core/galley/analysis.ts` to a leaf `#engine` layer (or upstream to scripture-kitchen), plus `makeAnalyze(parse)`.
  5. Replace the import side effects with explicit installs: `noteGestures` facet (`noteFollowing()` / `noteEditing()`) and `installRepaintTelemetry()`.
  6. `usfmLinter` omits Fix actions on `readOnly` states.
  7. `src/editor/read.ts` entry + a `mountViewer` recipe, with a boundaries rule that nothing reachable from it imports `#core/*`.
  8. Aliases from tsconfig `paths` → package.json `"imports"`, plus `"exports"` for `./editor/read` and the CSS, so the hub can depend on Sefer by git tag.
- For the hub, ship **build C** regardless of fork vs. refactor: the onion-only
  engine and the reading-only layers. That is about 263 KB brotli total, of
  which 164 KB is wasm (cacheable and immutable per engine tag).
