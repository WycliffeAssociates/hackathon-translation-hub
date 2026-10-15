// Translating a bulletin with NLLB-200 in this browser (opt-in): the language
// tags the hub uses mapped to NLLB's FLORES-200 codes, and one worker shared
// by every request on the page.

/** IETF tag → NLLB code, for the gateway languages and others NLLB knows. */
const FLORES: Record<string, string> = {
  en: "eng_Latn", fr: "fra_Latn", es: "spa_Latn", "es-419": "spa_Latn", pt: "por_Latn", "pt-br": "por_Latn",
  de: "deu_Latn", it: "ita_Latn", ru: "rus_Cyrl", uk: "ukr_Cyrl", ar: "arb_Arab", fa: "pes_Arab", ur: "urd_Arab",
  hi: "hin_Deva", ne: "npi_Deva", mr: "mar_Deva", bn: "ben_Beng", as: "asm_Beng", pa: "pan_Guru", gu: "guj_Gujr",
  or: "ory_Orya", ta: "tam_Taml", te: "tel_Telu", kn: "kan_Knda", ml: "mal_Mlym", si: "sin_Sinh",
  id: "ind_Latn", ms: "zsm_Latn", tl: "tgl_Latn", ceb: "ceb_Latn", ilo: "ilo_Latn", vi: "vie_Latn", th: "tha_Thai",
  km: "khm_Khmr", lo: "lao_Laoo", my: "mya_Mymr", zh: "zho_Hans", "zh-hans": "zho_Hans", "zh-hant": "zho_Hant",
  ja: "jpn_Jpan", ko: "kor_Hang", tr: "tur_Latn", uz: "uzn_Latn", kk: "kaz_Cyrl", ky: "kir_Cyrl", tg: "tgk_Cyrl",
  am: "amh_Ethi", ti: "tir_Ethi", om: "gaz_Latn", so: "som_Latn", sw: "swh_Latn", ha: "hau_Latn", yo: "yor_Latn",
  ig: "ibo_Latn", zu: "zul_Latn", xh: "xho_Latn", st: "sot_Latn", tn: "tsn_Latn", ss: "ssw_Latn", ts: "tso_Latn",
  rw: "kin_Latn", rn: "run_Latn", lg: "lug_Latn", ln: "lin_Latn", kg: "kon_Latn", ny: "nya_Latn", bem: "bem_Latn",
  sn: "sna_Latn", wo: "wol_Latn", ff: "fuv_Latn", ee: "ewe_Latn", tw: "twi_Latn", fon: "fon_Latn", mos: "mos_Latn",
  bm: "bam_Latn", ki: "kik_Latn", luo: "luo_Latn", mg: "plt_Latn", plt: "plt_Latn", tpi: "tpi_Latn", ht: "hat_Latn",
  qu: "quy_Latn", ay: "ayr_Latn", gn: "grn_Latn",
};

export const floresOf = (ietf: string): string | undefined => {
  const t = ietf.trim().toLowerCase();
  return FLORES[t] ?? FLORES[t.split("-")[0]!];
};

type Progress = { file?: string; loaded: number; total: number };
let worker: Worker | undefined;
let next = 0;
const waiting = new Map<number, { done: (t: string) => void; fail: (e: Error) => void }>();
let onProgress: ((p: Progress | "ready") => void) | undefined;

const ensure = () => {
  if (worker) return worker;
  worker = new Worker(new URL("./nllb.worker.ts", import.meta.url), { type: "module" });
  worker.onmessage = (e: MessageEvent) => {
    const m = e.data as { kind: string; id?: number; text?: string; message?: string } & Progress;
    if (m.kind === "progress") onProgress?.(m);
    else if (m.kind === "ready") onProgress?.("ready");
    else if (m.id !== undefined) {
      const w = waiting.get(m.id);
      waiting.delete(m.id);
      if (m.kind === "done") w?.done(m.text ?? "");
      else w?.fail(new Error(m.message ?? "translation failed"));
    }
  };
  return worker;
};

/** Translate `text` from one IETF language to another; `progress` hears the first download. */
export const translate = (text: string, from: string, to: string, progress?: (p: Progress | "ready") => void) =>
  new Promise<string>((done, fail) => {
    const src = floresOf(from);
    const tgt = floresOf(to);
    if (!src || !tgt) return fail(new Error(`NLLB has no ${!src ? from : to}`));
    onProgress = progress;
    const id = next++;
    waiting.set(id, { done, fail });
    ensure().postMessage({ id, text, src, tgt });
  });

/** Whether this browser already agreed to the download (it is cached after). */
const AGREED = "hub.nllb.agreed";
export const agreed = () => {
  try {
    return localStorage.getItem(AGREED) === "1";
  } catch {
    return false;
  }
};
export const agree = () => {
  try {
    localStorage.setItem(AGREED, "1");
  } catch {
    // Storage blocked: ask again next time.
  }
};
