// Why a source-compared finding fired: the verse as this text has it beside
// the same verse in the source it was compared with, lengths drawn to scale,
// and how this verse's length ratio sits against the book's usual one.
//
// The server does the reading (`/books/{book}/compare`: galley's verses and
// sous's grapheme counts, the length lane's own), so the card shows the
// numbers the finding was computed from; this only draws them.
import { Show, createResource } from "solid-js";
import { api } from "../api/client";
import type { PairedEvidence } from "../api/types";

/** The book whose finding this is; the server knows its source. */
export interface SourceRef {
  readonly owner: string;
  readonly name: string;
  /** `GEN`. */
  readonly book: string;
}

/** What the card needs from a finding. */
export interface Compared {
  readonly code: string;
  readonly chapter: number;
  readonly verse: number | null | undefined;
  readonly paired: PairedEvidence;
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

export default function SourceCompare(props: { source: SourceRef; finding: Compared }) {
  /** The verse keys the finding covers: one, or a run of missing ones. */
  const count = () => Math.min(12, Math.max(1, props.finding.paired.keys ?? 1));
  const [cmp] = createResource(
    () =>
      props.finding.verse == null
        ? false
        : {
            path: { owner: props.source.owner, name: props.source.name, book: props.source.book },
            query: { chapter: props.finding.chapter, verse: props.finding.verse, count: count() },
          },
    (c) => api.json("/v1/repos/{owner}/{name}/books/{book}/compare", c),
  );
  const pairs = () => cmp()?.verses ?? [];
  const here = () => pairs().map((v) => v.target).join(" ");
  const there = () => pairs().map((v) => v.source).join(" ");
  const tLen = () => props.finding.paired.target_len ?? pairs().reduce((n, v) => n + v.target_len, 0);
  const sLen = () => props.finding.paired.source_len ?? pairs().reduce((n, v) => n + v.source_len, 0);
  const ratio = () => (sLen() > 0 ? tLen() / sLen() : null);
  /** Bars share one scale: the longer of the two is full width. */
  const width = (n: number) => `${Math.max(2, (n / Math.max(tLen(), sLen(), 1)) * 100)}%`;
  const isLength = () => props.finding.code === "LengthProportionality";
  const label = () => {
    const f = props.finding;
    return f.verse == null ? `${f.chapter}` : count() > 1 ? `${f.chapter}:${f.verse}–${f.verse + count() - 1}` : `${f.chapter}:${f.verse}`;
  };
  const sourceName = () => cmp()?.source.split("@")[0] ?? "the source";

  return (
    <section class="compare" aria-label="Compared with the source">
      <p class="compare-head">
        <b class="mono">{label()}</b> {KIND[props.finding.paired.kind] ?? props.finding.paired.kind}
      </p>
      <Show when={!cmp.loading} fallback={<p class="small loading">Reading both books…</p>}>
        <Show when={!cmp.error} fallback={<p class="small bad">Could not compare: {String((cmp.error as { message?: string })?.message ?? cmp.error)}</p>}>
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
            <p class="compare-text">{there() || <span class="small muted">(no such verse in the source)</span>}</p>
          </div>
          <Show when={isLength() && ratio() !== null}>
            <p class="small compare-why">
              Here {x(ratio()!)} the source
              <Show when={cmp()?.usual_ratio}>{(u) => <>; this book usually runs <b>{x(u())}</b></>}</Show>.
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
        </Show>
      </Show>
    </section>
  );
}
