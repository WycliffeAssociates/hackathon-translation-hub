// The Progress tab of a repo (Will, 2026-10-15): this text against its
// source, made obvious. First the share of the source's verses present and a
// map of every chapter of the source, shaded by how many of its verses this
// repo has; a chapter opens in the viewer. Then the language's burn-up (all
// books together; by book on request), and each verse's length against the
// source for a book the proofreading run compared.
import * as Plot from "@observablehq/plot";
import { useNavigate } from "@solidjs/router";
import { For, Show, createMemo, createResource, createSignal } from "solid-js";
import { api } from "../../api/client";
import type { RepoDetail } from "../../api/types";
import { Async, plural } from "../../ui/bits";
import { Chart, byCanon, time } from "../../charts/kit";

/** Five steps of one hue (none … all), as the progress strip draws them. */
const SHADES = ["#e4e6e8", "#cfe5d8", "#9cc9b0", "#5fa583", "#2f7a57"];
const SHADE_LABELS = ["none", "under half", "half or more", "nearly all", "all"];

export default function Progress(props: { repo: RepoDetail; compared: readonly { book: string; path: string }[] }) {
  const navigate = useNavigate();
  const r = () => props.repo;
  const [against, setAgainst] = createSignal<string>();
  /** Show only books with a chapter missing verses. */
  const [incomplete, setIncomplete] = createSignal(false);
  const [cov] = createResource(
    () => ({ path: { owner: r().owner, name: r().name }, query: { against: against() } }),
    (c) => api.json("/v1/repos/{owner}/{name}/coverage", c),
  );
  const open = (path: string | null | undefined, chapter?: number, verse?: string) => {
    if (!path) return;
    const q = new URLSearchParams({ file: path });
    if (chapter) q.set("ch", String(chapter));
    if (verse) q.set("v", verse);
    navigate(`/repos/${r().owner}/${r().name}?${q}`);
  };

  return (
    <div class="progress-tab">
      <Async of={cov} what="coverage">
        {(c) => {
          const totals = createMemo(() => {
            let present = 0;
            let total = 0;
            let done = 0;
            for (const b of c().books) {
              const p = b.chapters.reduce((n, x) => n + x.present, 0);
              const t = b.chapters.reduce((n, x) => n + x.total, 0);
              present += p;
              total += t;
              if (t && p === t) done++;
            }
            return { present, total, done, pct: total ? Math.round((100 * present) / total) : 0 };
          });
          const shown = () =>
            incomplete() ? c().books.filter((b) => b.chapters.some((x) => x.present < x.total)) : c().books;
          const cells = () =>
            shown().flatMap((b) =>
              b.chapters.map((x) => ({
                book: b.book,
                path: b.path,
                chapter: x.chapter,
                share: x.total ? x.present / x.total : 0,
                label: `${b.book} ${x.chapter}: ${x.present} of ${x.total} verses`,
              })),
            );
          return (
            <Show when={c().source} fallback={<p class="small">No source to measure against: neither this repo nor its language's copies declare one.</p>}>
              <section class="coverage-head">
                <p class="progress-number">
                  <b>{totals().pct}%</b>
                  <span>
                    of{" "}
                    <Show when={c().candidates.length > 1} fallback={<span class="mono">{c().source}</span>}>
                      <select aria-label="Source" onChange={(e) => setAgainst(e.currentTarget.value)}>
                        <For each={c().candidates}>
                          {(s) => (
                            <option value={s} selected={s === c().source}>
                              {s}
                            </option>
                          )}
                        </For>
                      </select>
                    </Show>
                    's verses · {totals().present.toLocaleString()} of {totals().total.toLocaleString()} · {totals().done} of {plural(c().books.length, "book")} complete
                  </span>
                </p>
              </section>
              <p class="strip-legend small">
                <For each={SHADES}>
                  {(s, i) => (
                    <span>
                      <i style={{ background: s }} /> {SHADE_LABELS[i()]}
                    </span>
                  )}
                </For>
                <span class="muted">· a chapter opens in the viewer</span>
                <label class="coverage-filter">
                  <input type="checkbox" checked={incomplete()} onChange={(e) => setIncomplete(e.currentTarget.checked)} /> Only books not yet complete
                </label>
              </p>
              <Chart
                onPick={(cell: { path?: string | null; chapter: number }) => open(cell.path, cell.chapter)}
                make={(width) =>
                  Plot.plot({
                    width,
                    height: 50 + 22 * Math.max(1, shown().length),
                    marginLeft: 48,
                    x: { label: "chapter", axis: "top", ticks: [1, ...Array.from({ length: 15 }, (_, i) => (i + 1) * 10)] },
                    y: { domain: shown().map((b) => b.book).sort(byCanon), label: null },
                    color: { type: "threshold", domain: [0.0001, 0.5, 0.9, 1], range: SHADES },
                    marks: [Plot.cell(cells(), { x: "chapter", y: "book", fill: "share", inset: 1, rx: 2, tip: true, title: "label" })],
                  })
                }
              />
            </Show>
          );
        }}
      </Async>

      <Show when={r().language && r().resource}>
        <BurnUp ietf={r().language!} resource={r().resource!} />
      </Show>

      <Show when={props.compared.length}>
        <Lengths repo={r()} books={props.compared} onVerse={(path, ref) => open(path, undefined, ref)} />
      </Show>
    </div>
  );
}

/** The language's Writer books over time: all together by default, by book on request. */
function BurnUp(props: { ietf: string; resource: string }) {
  const [data] = createResource(
    () => ({ path: { ietf: props.ietf }, query: { resource: props.resource } }),
    (c) => api.json("/v1/languages/{ietf}/progress", c).catch(() => null),
  );
  const [byBook, setByBook] = createSignal(false);
  return (
    <Show when={(data()?.books.length ?? 0) > 1 ? data() : null}>
      {(d) => {
        const rows = createMemo(() => {
          const times = [...new Set(d().books.flatMap((b) => b.points.map((p) => p[0])))].sort((a, b) => a - b);
          const step = Math.max(1, Math.ceil(times.length / 400));
          const at = times.filter((_, i) => i % step === 0 || i === times.length - 1);
          const out: { t: Date; book: string; verses: number }[] = [];
          for (const b of d().books) {
            let i = 0;
            let v = 0;
            for (const t of at) {
              while (i < b.points.length && b.points[i]![0] <= t) v = b.points[i++]![1];
              out.push({ t: time(t), book: b.book, verses: v });
            }
          }
          return out;
        });
        const target = () => d().books.reduce((n, b) => n + (b.source_verses ?? b.verses), 0);
        return (
          <section class="chart-section">
            <div class="chart-head">
              <h3>Burn-up <span class="muted">· the language's Writer projects, one per book</span></h3>
              <label class="small">
                <input type="checkbox" checked={byBook()} onChange={(e) => setByBook(e.currentTarget.checked)} /> By book
              </label>
            </div>
            <Chart
              make={(width) =>
                Plot.plot({
                  width,
                  height: 280,
                  color: byBook() ? { legend: true } : undefined,
                  y: { grid: true, label: "verses present" },
                  x: { label: null },
                  marks: [
                    byBook()
                      ? Plot.areaY(rows(), { x: "t", y: "verses", fill: "book", order: (a: { book: string }, b: { book: string }) => byCanon(a.book, b.book), curve: "step-after", tip: true })
                      : Plot.areaY(rows(), Plot.groupX({ y: "sum" }, { x: "t", y: "verses", fill: "#2f7a57", fillOpacity: 0.8, curve: "step-after", tip: true })),
                    Plot.ruleY([target()], { stroke: "currentColor", strokeDasharray: "4,3" }),
                    Plot.text([target()], { y: (v: number) => v, text: () => `the sources: ${target().toLocaleString()} verses`, frameAnchor: "right", dy: -6 }),
                  ],
                })
              }
            />
          </section>
        );
      }}
    </Show>
  );
}

/** Each verse's length against the source's, for one compared book; a dot opens its verse. */
function Lengths(props: { repo: RepoDetail; books: readonly { book: string; path: string }[]; onVerse: (path: string, ref: string) => void }) {
  const [pick, setPick] = createSignal(0);
  const book = () => props.books[pick()] ?? props.books[0]!;
  const [data] = createResource(
    () => ({ path: { owner: props.repo.owner, name: props.repo.name, book: book().book }, query: { text: false } }),
    async (c) => {
      const cmp = await api.json("/v1/repos/{owner}/{name}/books/{book}/compare", c);
      const rows = cmp.verses
        .filter((v) => v.target_len > 0 && v.source_len > 0)
        .map((v) => ({ ref: `${v.chapter}:${v.verse}`, t: v.target_len, s: v.source_len, ratio: v.target_len / v.source_len, flag: v.flag ?? "" }));
      return { source: cmp.source.split("@")[0], rows, median: cmp.usual_ratio ?? 1 };
    },
  );
  return (
    <section class="chart-section">
      <div class="chart-head">
        <h3>Verse length against the source</h3>
        <label class="small">
          Book{" "}
          <select onChange={(e) => setPick(Number(e.currentTarget.value))}>
            <For each={props.books}>
              {(b, i) => (
                <option value={i()} selected={i() === pick()}>
                  {b.book}
                </option>
              )}
            </For>
          </select>
        </label>
      </div>
      <Async of={data} what="the comparison">
        {(d) => (
          <>
            <p class="small muted">
              Against {d().source}: usually {d().median.toFixed(2)}× the source's length; {d().rows.filter((r) => r.flag).length} verses flagged. A dot opens its verse.
            </p>
            <Chart
              onPick={(row: { ref: string }) => props.onVerse(book().path, row.ref)}
              make={(width) =>
                Plot.plot({
                  width,
                  height: 360,
                  x: { label: "source verse (graphemes)", grid: true },
                  y: { label: "this verse (graphemes)", grid: true },
                  color: { domain: ["", "Long", "Short"], range: ["#9aa6af", "#c0392b", "#2471a3"], legend: true, tickFormat: (v: string) => v || "not flagged" },
                  marks: [
                    Plot.line([[0, 0], [Math.max(...d().rows.map((r) => r.s)), Math.max(...d().rows.map((r) => r.s)) * d().median]], { strokeDasharray: "4,3" }),
                    Plot.dot(d().rows.filter((r) => !r.flag), { x: "s", y: "t", r: 2, fill: "flag", fillOpacity: 0.5, tip: true, title: (r: { ref: string; ratio: number }) => `${r.ref}: ${r.ratio.toFixed(2)}×` }),
                    Plot.dot(d().rows.filter((r) => r.flag), { x: "s", y: "t", r: 4.5, fill: "flag", stroke: "white", tip: true, title: (r: { ref: string; t: number; s: number; ratio: number }) => `${r.ref}: ${r.t} vs ${r.s} (${r.ratio.toFixed(2)}×)` }),
                  ],
                })
              }
            />
          </>
        )}
      </Async>
    </section>
  );
}
