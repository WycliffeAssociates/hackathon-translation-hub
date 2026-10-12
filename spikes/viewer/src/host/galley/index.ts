/**
 * SPIKE shim for Sefer's `#core/galley` front door.
 *
 * Sefer's index re-exports analysis, diff, format, overlay, galley, location.
 * The editor only ever imports from `analysis.ts` (the engine reader's
 * vocabulary: TOKEN, MARKERS, NOTE_PART, sameSource, diagnostic* …) and
 * `location.ts`. `galley.ts` (the Effect Layer + wasm handle), `diff.ts`,
 * `format.ts` and `overlay.ts` are deliberately NOT copied — the viewer
 * supplies an `Analyze` function instead (see ../engine.ts).
 */
export * from "./analysis";
export * from "./location";
