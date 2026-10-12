/**
 * The viewer's engine: what Sefer's `GalleyLive` Layer does for the editor,
 * minus Effect, minus the corpus half, minus observability.
 *
 * The editor needs exactly one thing from the engine: an `Analyze`
 * (`(text) => Analysis`, src/editor-fork/core/analyzer.ts) handed in through
 * the `analyzer` facet. Sefer builds it with `galley.memoize()`; this file
 * builds the same function straight off the wasm handle's loose-text door
 * (`parseText`), which is the door Sefer's own reference pane uses.
 */

import wasmUrl from "@wycliffeassociates/scripture-kitchen/web/wasm?url";
import { Galley as GalleyHandle, initSync } from "@wycliffeassociates/scripture-kitchen/web";
import { deserialize, declaredVersion } from "@wycliffeassociates/scripture-kitchen/reader";

import type { Analysis } from "./galley/analysis";

/** Same options Sefer's galley.ts passes (PARSE_ALL). */
const PARSE_ALL = { diagnostics: true, toc: true, utf16: true } as const;

export interface Engine {
  /** A fresh memoized analyzer — one per view, as Sefer's `galley.memoize()`. */
  memoize(): (text: string) => Analysis;
  readonly wasmBytes: number;
  readonly bootMs: number;
}

let booted: Promise<Engine> | undefined;

/** Fetch + instantiate the wasm once per page. */
export const bootEngine = (): Promise<Engine> =>
  (booted ??= (async () => {
    const t0 = performance.now();
    const response = await fetch(wasmUrl);
    if (!response.ok) throw new Error(`cannot fetch ${wasmUrl}: ${response.status}`);
    const bytes = new Uint8Array(await response.arrayBuffer());
    initSync({ module: bytes });
    const handle = new GalleyHandle();
    let revision = 0;

    const analyze = (text: string): Analysis => {
      if (text.includes("\r")) throw new Error("canonical text is LF; the engine refuses \\r");
      const started = performance.now();
      const dish = deserialize(handle.parseText(text, PARSE_ALL));
      const engineMs = performance.now() - started;
      revision += 1;
      return {
        dish,
        text,
        docLen: text.length,
        sourceHash: dish.sourceHash,
        revision,
        engineMs,
        usfmVersion: declaredVersion(dish),
      };
    };

    return {
      memoize: () => {
        let last: Analysis | undefined;
        return (text) => {
          if (last !== undefined && last.text === text) return last;
          last = analyze(text);
          return last;
        };
      },
      wasmBytes: bytes.byteLength,
      bootMs: performance.now() - t0,
    };
  })());
