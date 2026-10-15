# Editor pieces: what the hub uses of Sefer's editor

Will wanted Sefer's editor to be **installable** (a package other apps mount),
not forked. This list was how the package was cut; it is now the map of what
the hub takes from it, what the hub keeps of its own, and what it still asks.
Keep it current when the viewer changes.

**Installed (2026-10-15).** `web/` depends on `@sefer/editor` and
`@sefer/text`, from Sefer by git at master `082c09c` (no `editor-v*` tag yet;
`web/package.json`, pnpm's `&path:` form). The packages ship TypeScript and
Vite compiles them; their peers (CodeMirror, `effect`, scripture-kitchen) are
`web/`'s own dependencies, one copy each. The fork (`spikes/viewer/src/editor-fork/`,
taken at Sefer `50aa9cd`) and the spike's code are gone; the spike's write-ups
stay in `spikes/viewer/` as the record. The engine host is the hub's:
`web/src/engine/onion.ts`.

## 1. Taken from the package

| Piece | From | What the hub uses it for |
|---|---|---|
| `seatBook(makeBook(path, source), { analyze })`, `mountBookEditor({ parent, book, readOnly: true })` | `@sefer/editor`, `@sefer/text/book` | The read-only viewer (`web/src/viewer/UsfmViewer.tsx`) |
| `decode` and `offsetMap(raw).toCanonical` | `@sefer/text/source` | Canonical text (LF, no mark); the server's raw-file offsets mapped onto it |
| `setPolicy(editorPolicy("regular" \| "usfm"))` | `@sefer/editor` | The Regular/USFM switch; the reader stays put and the marks are redrawn |
| `book.structure().chapters`, `anchorFrom(row)`, `reveal(range, { scroll: "start" })` | `@sefer/editor` | The chapter navigator's list and its jumps |
| `onViewport((where, source) => …)` | `@sefer/editor` | The chapter indicator follows the scroll; `source` ("user" or "reveal") replaces the hub's scrollTop pin |
| `jump(range, "center")` and `reveal(range, { select: true })` | `@sefer/editor` | Go to a finding: select, scroll, flash |
| `diagnostics()` and `onDiagnostics(cb)` | `@sefer/editor` | The rail lists what the linter draws, told only when the set changes |
| `setHostDiagnostics(findings)` with `stamp: book.source().stamp` | `@sefer/editor` | Sous Chef's stored rows drawn with the engine's lint as one set |
| `addressAt(offset)`, `revealAddress(address)` | `@sefer/editor`, `@sefer/text/location/address` | A mark's `3:16` in the rail; the `?v=3:16` link from a chart |
| `createAlignedGroup()` | `@sefer/editor` | The Checks tab: the text and its source kept on the same verse as either scrolls |
| `editor.css` (popover, gutter, lint styles under `.cm-host`) | `@sefer/editor/editor.css` | The pane carries `cm-host` |
| `Analysis` | `@sefer/text/engine/analysis` | The type the hub's engine host returns |

No hub file imports `@codemirror/*`: everything goes through the package's handle.

## 2. The hub's own (and which of its old wishes the package meets)

| Piece | Where | Status |
|---|---|---|
| A read-only viewer in one call | was `mountViewer` | **Met**: `mountBookEditor({ readOnly: true })` |
| Go to a finding: select, scroll, flash | was `mountViewer.reveal` | **Met**: `jump` plus `reveal({ select })` |
| What the linter draws, told when it changes | was `mountViewer.onDiagnostics` | **Met**: `diagnostics()`, `onDiagnostics` |
| Re-lint after a mode switch | was the fork's `needsRefresh` fix | **Met**: in the package |
| CRLF offsets from the server | was `canonical()` | **Met**: `offsetMap` |
| Offset → `3:16` | was `addresses()` | **Met**: `addressAt` |
| The chapter indicator pinned after a jump | was a scrollTop pin | **Met**: `onViewport`'s `source` |
| Verse pairing between panes | wished | **Met**: `createAlignedGroup` (the Checks tab) |
| Sefer's `\s5` policy in the browser engine | `web/src/engine/onion.ts` | The hub's, as the handoff asked; it should live with the engine host once |
| Chapter navigator (‹ [Chapter 5 of 28 ▾] ›, the hundred-chart grid, `[` `]`) | `web/src/viewer/ChapterNav.tsx` | The hub's UI |
| Findings rail (checks grouped, filters, this book / all books, invisible characters spelled out, scan-vs-drawn reconciliation) | `web/src/viewer/FindingsRail.tsx` | The hub's UI |
| Source comparison card | `web/src/viewer/SourceCompare.tsx`; the server pairs and counts | The hub's UI |
| Verse pairs with the source, in Sous Chef's terms | `crates/proof/src/compare.rs` | The hub's server |

## 3. Asks for Sefer (what the package lacks)

- **scripture-kitchen v0.1.10.** `@sefer/text`'s message rendering expects kitchen 0.1.10 to fill `{aux}` itself; the hub still pins 0.1.9 everywhere (Rust and web, bumped together by rule), so a message with `{aux}` ("expected {aux}") shows the placeholder until the hub moves to 0.1.10. Either the hub bumps (its next step), or the package fills `{aux}` when the engine did not.
- **A release tag.** The hub installs by commit SHA; an `editor-v0.1.0` tag would make the pin readable.
- **The reading-only build.** The fork let the hub drop the editing layer (`virtual:viewer-layers` → `layers.split.ts`), for about 263 KB brotli. `seatBook` always adds the commands and note editing; a read-only seat (or a reading-only extension set) would let a viewer ship less.
- **Marker settings are page-wide.** The engine's `setExtensions` is one process-wide registry; two viewers with different marker policies on one page would interfere (already logged with scripture-kitchen).
- **The flash is shorter** than the fork's (about a second against 1.6 s). Fine; noted in case a host wants to choose.

## 4. In Sefer, wanted here, not built

| Piece | Where in Sefer | Notes |
|---|---|---|
| Location bar: Book · Chapter breadcrumb, book picker, ‹ › | `src/app/ui/workspace/LocationBar.tsx`, `Crumbs.tsx` | Solid 2 in Sefer, Solid 1.9 in the hub |
| Address parser: "mark 3", "Lucas 3:1", `ROM 9:30-10:4` | `@sefer/text/location/citation` (now installable) | For the "reference in prose → popover" idea |
| Find in book / project | `Toolbar.tsx` search → `/find` | In-book find is CodeMirror's; project-wide needs a server route |
| Lint Fix actions | the package (`readOnly` hides them) | For an editing host |

Not editor, but ported from Sefer too: `crates/history` is `src/core/migrate/writerHistory.ts` (Writer saves replayed as book-file commits) in Rust; Sefer's hand-written pack (`src/core/git/pack.ts`) is replaced by libgit2's in-memory store dumped as one pack.
