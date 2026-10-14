// Why a source-compared finding fired: the verse as this text has it beside
// the same verse in the source it was compared with, lengths drawn to scale,
// and how this verse's length ratio sits against the book's usual one.
//
// Sous Chef measured in graphemes over its own verse model; this card re-reads
// both texts in the browser (markers, notes and cross-references stripped,
// graphemes by Intl.Segmenter) so a reader can see the verses, and takes the
// verdict and the stored lengths from the finding itself.
import { Show, createMemo, createResource } from "solid-js";
import { api } from "../api/client";
import type { PairedEvidence } from "../api/types";

/** Where the run found this book's source: `owner/name@sha` and the file in it. */
export interface SourceRef {
  readonly repo: string;
  readonly path: string;
}

/** What the card needs from a finding. */
export interface Compared {
  readonly code: string;
  readonly chapter: number;
  readonly verse: number | null | undefined;
  readonly paired: PairedEvidence;
}

/** One verse's readable text, by chapter and verse number (a bridge `1-2` answers both). */
type Verses = Map<string, string>;

const NOTES = /\\(f|fe|x|ef|ex)\s[\s\S]*?\\\1\*/g;
const FIGURES = /\\fig[\s\S]*?\\fig\*/g;
const MARKERS = /\\\+?[A-Za-z0-9]+\*?/g;

/** Every verse's text in a USFM book, notes and markers removed. */
export function versesOf(usfm: string): Verses {
  const out: Verses = new Map();
  const text = usfm.replace(/\r\n?/g, "\n").replace(NOTES, " ").replace(FIGURES, " ");
  let chapter = "0";
  const re = /\\(c|v)\s+(\d+)(?:-(\d+))?[a-z]?/g;
  let open: { keys: string[]; from: number } | null = null;
  const close = (to: number) => {
    if (!open) return;
    const body = text
      .slice(open.from, to)
      .replace(/\\(s\d?|ms\d?|mr|r|d|cl|cp|sp|toc\d|h|mt\d?|ca|va|vp)\b[^\n]*/g, " ")
      .replace(MARKERS, " ")
      .replace(/\s+/g, " ")
      .trim();
    for (const k of open.keys) out.set(k, body);
    open = null;
  };
  for (const m of text.matchAll(re)) {
    close(m.index);
    if (m[1] === "c") {
      chapter = m[2]!;
      continue;
    }
    const from = Number(m[2]);
    const to = m[3] ? Number(m[3]) : from;
    const keys: string[] = [];
    for (let v = from; v <= to && v - from < 200; v++) keys.push(`${chapter}:${v}`);
    open = { keys, from: m.index + m[0].length };
  }
  close(text.length);
  return out;
}

const segmenter = typeof Intl !== "undefined" && "Segmenter" in Intl ? new Intl.Segmenter() : null;
const graphemes = (s: string) => (segmenter ? [...segmenter.segment(s)].length : [...s].length);

/** The middle target/source length ratio over every verse both texts have. */
function usualRatio(target: Verses, source: Verses): { ratio: number; verses: number } | null {
  const ratios: number[] = [];
  for (const [key, t] of target) {
    const s = source.get(key);
    if (!s || !t) continue;
    ratios.push(graphemes(t) / graphemes(s));
  }
  if (ratios.length < 10) return null;
  ratios.sort((a, b) => a - b);
  return { ratio: ratios[ratios.length >> 1]!, verses: ratios.length };
}

const KIND: Record<string, string> = {
  Long: "much longer than its source",
  Short: "much shorter than its source",
  Missing: "missing: the source has verses here",
  Extra: "extra: the source has no such verse",
  Empty: "empty in this text",
  Copy: "copied from the source",
};

const x = (n: number) => `${n.toFixed(n < 10 ? 2 : 1)}×`;

export default function SourceCompare(props: {
  /** This text, LF. */
  target: string;
  source: SourceRef;
  finding: Compared;
}) {
  const parts = () => {
    const [ownerName] = props.source.repo.split("@");
    const [owner, name] = ownerName!.split("/");
    return { path: { owner: owner!, name: name!, path: props.source.path } };
  };
  const [sourceText] = createResource(parts, (c) => api.text("/v1/repos/{owner}/{name}/file/{path}", c));

  const targetVerses = createMemo(() => versesOf(props.target));
  const sourceVerses = createMemo(() => (sourceText() ? versesOf(sourceText()!) : null));
  const usual = createMemo(() => {
    const s = sourceVerses();
    return s ? usualRatio(targetVerses(), s) : null;
  });

  /** The verse keys the finding covers: one, or a run of missing ones. */
  const keys = () => {
    const f = props.finding;
    if (f.verse == null) return [];
    const n = Math.max(1, f.paired.keys ?? 1);
    return Array.from({ length: Math.min(n, 12) }, (_, i) => `${f.chapter}:${f.verse! + i}`);
  };
  const join = (v: Verses | null, ks: string[]) => (v ? ks.map((k) => v.get(k)).filter(Boolean).join(" ") : "");
  const here = () => join(targetVerses(), keys());
  const there = () => join(sourceVerses(), keys());

  const tLen = () => props.finding.paired.target_len ?? graphemes(here());
  const sLen = () => props.finding.paired.source_len ?? graphemes(there());
  const ratio = () => (sLen() > 0 ? tLen() / sLen() : null);
  /** Bars share one scale: the longer of the two is full width. */
  const width = (n: number) => `${Math.max(2, (n / Math.max(tLen(), sLen(), 1)) * 100)}%`;
  const isLength = () => props.finding.code === "LengthProportionality";
  const label = () => {
    const f = props.finding;
    return f.verse == null ? `${f.chapter}` : keys().length > 1 ? `${f.chapter}:${f.verse}–${f.verse + keys().length - 1}` : `${f.chapter}:${f.verse}`;
  };
  const sourceName = () => props.source.repo.split("@")[0];

  return (
    <section class="compare" aria-label="Compared with the source">
      <p class="compare-head">
        <b class="mono">{label()}</b> {KIND[props.finding.paired.kind] ?? props.finding.paired.kind}
      </p>
      <div class="compare-side">
        <div class="compare-label">
          <span>This text</span>
          <Show when={isLength()}><span class="mono small">{tLen().toLocaleString()}</span></Show>
        </div>
        <Show when={isLength()}><div class="compare-bar here"><i style={{ width: width(tLen()) }} /></div></Show>
        <p class="compare-text">{here() || <span class="small muted">(no such verse here)</span>}</p>
      </div>
      <div class="compare-side">
        <div class="compare-label">
          <span>Source <span class="mono small">{sourceName()}</span></span>
          <Show when={isLength()}><span class="mono small">{sLen().toLocaleString()}</span></Show>
        </div>
        <Show when={isLength()}><div class="compare-bar there"><i style={{ width: width(sLen()) }} /></div></Show>
        <Show when={!sourceText.loading} fallback={<p class="small loading">Reading the source…</p>}>
          <Show when={!sourceText.error} fallback={<p class="small bad">Could not read {props.source.path} from {sourceName()}.</p>}>
            <p class="compare-text">{there() || <span class="small muted">(no such verse in the source)</span>}</p>
          </Show>
        </Show>
      </div>
      <Show when={isLength() && ratio() !== null}>
        <p class="small compare-why">
          Here {x(ratio()!)} the source
          <Show when={usual()}>
            {(u) => (
              <>
                ; this book usually runs <b>{x(u().ratio)}</b> (middle of {u().verses.toLocaleString()} verses)
              </>
            )}
          </Show>
          .
          <Show when={props.finding.paired.z_book != null}>
            {" "}That is {props.finding.paired.z_book!.toFixed(1)} spreads from usual in this book
            <Show when={props.finding.paired.z_project != null}>, {props.finding.paired.z_project!.toFixed(1)} in the whole repo</Show>.
          </Show>
        </p>
      </Show>
      <Show when={props.finding.paired.kind === "Missing" || props.finding.paired.kind === "Extra"}>
        <p class="small compare-why">
          Often versification rather than a gap: Psalm titles numbered as verse 1, or a verse split differently, shift every
          number after them.
        </p>
      </Show>
      <Show when={props.finding.code === "SourceCopy" && props.finding.paired.eligible != null}>
        <p class="small compare-why">{props.finding.paired.eligible} words in this verse could be compared.</p>
      </Show>
    </section>
  );
}

