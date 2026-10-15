// Runs NLLB-200 (distilled, 600M, 8-bit) in the browser, off the page's
// thread, through transformers.js. Loaded only when someone asks to translate
// a bulletin; the model is fetched once from Hugging Face and cached by the
// browser. WASM, not WebGPU: transformers.js reported wrong NLLB output on
// WebGPU (issue 1286), and a bulletin is short enough for the CPU.
import { pipeline, type TranslationPipeline } from "@huggingface/transformers";

type Ask = { id: number; text: string; src: string; tgt: string };

let translator: Promise<TranslationPipeline> | undefined;

const load = () =>
  (translator ??= pipeline("translation", "Xenova/nllb-200-distilled-600M", {
    dtype: "q8",
    device: "wasm",
    progress_callback: (p: { status: string; file?: string; loaded?: number; total?: number }) => {
      if (p.status === "progress" && p.total) postMessage({ kind: "progress", file: p.file, loaded: p.loaded, total: p.total });
    },
  }) as Promise<TranslationPipeline>);

self.onmessage = async (e: MessageEvent<Ask>) => {
  const { id, text, src, tgt } = e.data;
  try {
    const t = await load();
    postMessage({ kind: "ready" });
    // Paragraph by paragraph: the model is trained on sentences, and long input degrades.
    const parts = text.split(/\n{2,}/);
    const out: string[] = [];
    for (const part of parts) {
      if (!part.trim()) {
        out.push(part);
        continue;
      }
      const r = (await t(part, { src_lang: src, tgt_lang: tgt, max_length: 512 } as never)) as { translation_text: string }[];
      out.push(r[0]?.translation_text ?? "");
    }
    postMessage({ kind: "done", id, text: out.join("\n\n") });
  } catch (err) {
    postMessage({ kind: "error", id, message: err instanceof Error ? err.message : String(err) });
  }
};
