/**
 * The viewer's engine, onion-only: the stateless parser wasm (540 KB) instead
 * of the galley handle (1.28 MB, which adds sous/corpus/find/toc/mask/diff).
 * Same `Analyze` contract as engine.galley.ts; the editor cannot tell them apart.
 * Build with ENGINE=onion.
 */

import wasmUrl from "@wycliffeassociates/scripture-kitchen/onion/web/wasm?url";
import { initSync, parse } from "@wycliffeassociates/scripture-kitchen/onion/web";
import { deserialize, declaredVersion } from "@wycliffeassociates/scripture-kitchen/reader";

import type { Analysis } from "./galley/analysis";
import type { Engine } from "./engine.galley";

export type { Engine };

const PARSE_ALL = { diagnostics: true, toc: true, utf16: true } as const;

let booted: Promise<Engine> | undefined;

export const bootEngine = (): Promise<Engine> =>
  (booted ??= (async () => {
    const t0 = performance.now();
    const response = await fetch(wasmUrl);
    if (!response.ok) throw new Error(`cannot fetch ${wasmUrl}: ${response.status}`);
    const bytes = new Uint8Array(await response.arrayBuffer());
    initSync({ module: bytes });
    let revision = 0;
    const analyze = (text: string): Analysis => {
      if (text.includes("\r")) throw new Error("canonical text is LF; the engine refuses \\r");
      const started = performance.now();
      const dish = deserialize(parse(text, PARSE_ALL));
      revision += 1;
      return {
        dish,
        text,
        docLen: text.length,
        sourceHash: dish.sourceHash,
        revision,
        engineMs: performance.now() - started,
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
