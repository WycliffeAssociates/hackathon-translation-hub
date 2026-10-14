/**
 * The viewer's engine, onion-only: the stateless parser wasm (540 KB) instead
 * of the galley handle (1.28 MB, which adds sous/corpus/find/toc/mask/diff).
 * Same `Analyze` contract as engine.galley.ts; the editor cannot tell them apart.
 * Build with ENGINE=onion.
 */

import wasmUrl from "@wycliffeassociates/scripture-kitchen/onion/web/wasm?url";
import { initSync, parse, setExtensions } from "@wycliffeassociates/scripture-kitchen/onion/web";
import { deserialize, declaredVersion } from "@wycliffeassociates/scripture-kitchen/reader";

import type { Analysis } from "./galley/analysis";
import type { Engine } from "./engine.galley";

export type { Engine };

const PARSE_ALL = { diagnostics: true, toc: true, utf16: true } as const;

/**
 * Sefer's legacy-marker policy, as the hub's server scan applies it
 * (crates/checks/src/lint.rs): a text that has `\s5` alone on a line gets
 * `\s5` registered as a standalone point that leaves its paragraph open;
 * any other text gets no extensions. Without it en_ulb's Genesis alone shows
 * 634 unknown-marker and 384 missing-paragraph warnings the scan never counts.
 * Sefer's regex: `^\\s5[ \t]*\r?$`, per line.
 */
const S5_ALONE = /^\\s5[ \t]*$/m;
const WITH_S5 = JSON.stringify([{ name: "s5", category: "standalone" }]);

let booted: Promise<Engine> | undefined;

export const bootEngine = (): Promise<Engine> =>
  (booted ??= (async () => {
    const t0 = performance.now();
    const response = await fetch(wasmUrl);
    if (!response.ok) throw new Error(`cannot fetch ${wasmUrl}: ${response.status}`);
    const bytes = new Uint8Array(await response.arrayBuffer());
    initSync({ module: bytes });
    let revision = 0;
    // setExtensions is process-wide and invalidates what the engine derived,
    // so it is called only when a text needs the other registry.
    let s5 = false;
    const analyze = (text: string): Analysis => {
      if (text.includes("\r")) throw new Error("canonical text is LF; the engine refuses \\r");
      const wants = S5_ALONE.test(text);
      if (wants !== s5) {
        const refused = setExtensions(wants ? WITH_S5 : "[]", { relaxZPrefix: true });
        if (refused !== "[]") throw new Error(`the engine refused \\s5: ${refused}`);
        s5 = wants;
      }
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
