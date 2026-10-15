# Editor pieces: what the hub rebuilt around Sefer's editor

Will wants Sefer's editor to be **installable** (a package other apps mount),
not forked. This is the one list of everything the hub uses, adds or wishes it
had, so the package can be cut from it. Keep it current when the viewer changes.

Today the hub does not install anything: `web/` imports `spikes/viewer/src`
through the `@viewer` alias (`web/vite.config.ts`), and `spikes/viewer` is a fork
of Sefer's editor at Sefer `50aa9cd` (`src/editor-fork/`), read-only, with the
onion-only engine (build C). The engine itself is scripture-kitchen by tag.

## 1. Taken from Sefer as-is (the fork)

What the package would export, because the hub already uses it unchanged:

| Piece | Where (fork) | What the hub uses it for |
|---|---|---|
| Reading half of the editor: `analyzer`, `readingLayer`, `viewLayer()` | `editor-fork/core`, `views.ts` | Paints USFM the way Sefer does |
| Projections: `modeView("regular" \| "usfm")`, `pickChapter` | `editor-fork/views.ts` | Regular/USFM switch; clip to one chapter |
| Document structure: `structureAt(state).chapters` | `editor-fork/core/docStructure.ts` | Chapter list for the navigator |
| Where am I: `watchLocation(view, report)` | `editor-fork/recipes/whereAmI.ts` | Chapter indicator follows the scroll |
| Linter with a second source: `usfmLinter`, `showCorpusFindings` | `editor-fork/recipes/lint.ts` | Engine lint plus Sous Chef's stored rows as one set of marks |
| Arrival mark: `flashing()`, `flash(view, range)` | `editor-fork/recipes/flash.ts` | Lights the span a finding jumps to |
| Popover, gutter and lint panel styles (under `.cm-host`) | `editor-fork/editor.css` | The tooltip; the pane must carry `cm-host` or it is unstyled |
| Tokens (light and dark) | `host/tokens.css` | Theme |
| Engine host: `bootEngine()`, `memoize()` | `host/engine.onion.ts` | Parse + lint in the browser (540 KB wasm) |

## 2. Added in the hub's copy (candidates for the package)

| Piece | Where | Why it belongs in the editor |
|---|---|---|
| `mountViewer({parent, text, analyze, mode})`: a read-only viewer in one call | `spikes/viewer/src/viewer/mountViewer.ts` | Every host that only shows text wants this, not the editing apparatus |
| `reveal(from, to)`: select, scroll to a third of the pane, flash | `mountViewer.ts` | "Go to this finding" is the same in every host |
| `diagnostics()` / `onDiagnostics(cb)`: what the linter draws, announced only when the set changes | `mountViewer.ts` | Lets a host list marks beside the text (the rail) without reaching into CodeMirror's lint state |
| Re-lint on a reconfiguration (mode switch) | `editor-fork/recipes/lint.ts` `needsRefresh` | **Bug fix.** Marks hidden by paint in Regular stayed hidden in USFM. Check Sefer for the same bug |
| Sefer's `\s5` policy in the browser engine (`setExtensions` per text, `^\s5[ \t]*$`) | `host/engine.onion.ts` | **Bug fix.** Without it en_ulb Genesis shows 634 unknown-marker + 384 missing-paragraph the server never counts. The policy should live with the engine host, once |
| CRLF canonicalisation that maps server offsets (UTF-16 into the raw file) to LF offsets | `web/src/viewer/UsfmViewer.tsx` `canonical()` | Any host fed offsets computed on raw text needs it |
| Offset → `3:16` | `UsfmViewer.tsx` `addresses()` | Sefer has a better one (`core/location/locate.ts`); use that |
| Chapter navigator: ‹ [Chapter 5 of 28 ▾] ›, a hundred-chart grid tinted by lint and marked where verses are missing; follows the scroll and stays pinned after a jump (a short last chapter cannot reach the top); `[` `]` and Alt+←/→ | `web/src/viewer/ChapterNav.tsx`, pin logic in `UsfmViewer.tsx` | Sefer's LocationBar does chapter + book; the grid and the marks are the hub's |
| Findings rail: marks grouped by check, worst then noisiest; filters (words, engine, severity); this book / all books; a row selects its span; same-message rows show the marked text with invisible characters spelled out (`⟨200B⟩`); counted-by-scan vs drawn reconciliation | `web/src/viewer/FindingsRail.tsx` | A host-agnostic "problems panel" for the editor |
| Source comparison card: the verse beside its source verse, length bars on one scale, this verse's ratio vs the book's usual (median), versification note for missing/extra | `web/src/viewer/SourceCompare.tsx` (draws; the server pairs and counts) | The reason behind a source-compared finding; Sefer's reference pane could host it |
| Verse pairs with the source, in Sous Chef's terms | `crates/proof/src/compare.rs`, `/v1/repos/{o}/{n}/books/{book}/compare` | Was a regex in the browser; now galley's `OnionBook` verses and sous's `count_atoms`, the length lane's own numbers. An installed editor would call the engine the same way |

## 3. In Sefer, wanted here, not built

| Piece | Where in Sefer (`50aa9cd`) | Notes |
|---|---|---|
| Location bar: Book · Chapter breadcrumb, book picker, ‹ › | `src/app/ui/workspace/LocationBar.tsx`, `Crumbs.tsx` | Solid 2 in Sefer, Solid 1.9 in the hub: a package would need one |
| Address parser: "mark 3", "Lucas 3:1", `ROM 9:30-10:4` | `src/core/location/` (`citation.ts`, `names.ts`, `canon.ts`, `address.ts`; ~735 lines, plain TS) | Also for the "reference in prose → popover" idea |
| Find in book / project | `Toolbar.tsx` search → `/find` | In-book find is CodeMirror's; project-wide needs a server route |
| Reference pane (a second text, painted the same, read-only) | `editor-fork/recipes/reference.ts` (already in the fork, unused) | The source text beside the book; about half a day, chapter-synced |
| Verse pairing between panes | `editor-fork/recipes/pairing.ts`, `align.ts` (in the fork, unused) | Click a verse, its source lights; about a day more |
| Lint Fix actions | `lint.ts` (refused here by the read-only change filter) | For an installable *editor*, these come back |

Not editor, but ported from Sefer too: `crates/history` is `src/core/migrate/writerHistory.ts` (Writer saves replayed as book-file commits) in Rust; Sefer's hand-written pack (`src/core/git/pack.ts`) is replaced by libgit2's in-memory store dumped as one pack.

## 4. Packaging notes for when it is cut

- The engine wasm is an asset (`?url` import); a package must let the host serve it.
- The engine's extensions (`setExtensions`) are process-wide: one policy per page, decided per text.
- CodeMirror must be a peer dependency, or two copies split the lint state (`forEachDiagnostic` sees nothing).
- Styles are keyed on `.cm-host`; export that as the documented host class.
- Solid 1.9 (hub) vs Solid 2 (Sefer): the UI pieces (navigator, rail, compare) need one version, or a framework-free core with thin wrappers.
