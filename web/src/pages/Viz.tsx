// Candidate charts, on real data, to keep or cut (Will, 2026-10-14). Each card
// says the question it answers and where its data comes from; the server
// returns series, the page only draws (Observable Plot, 0.6.17). Cut freely:
// nothing else depends on this page.
import * as Plot from "@observablehq/plot";
import { For, createEffect, createMemo, createResource, createSignal, onCleanup, type JSX } from "solid-js";
import { api, startPageView } from "../api/client";
import type { RepoSummary } from "../api/types";
import { Async } from "../ui/bits";
import { graphemes, versesOf } from "../viewer/SourceCompare";

// --- the frame -----------------------------------------------------------------

/** Draws `make(width)` into the card, again when the card is resized. */
function Chart(props: { make: (width: number) => (SVGSVGElement | HTMLElement) | null }) {
  let host!: HTMLDivElement;
  const [width, setWidth] = createSignal(0);
  const ro = new ResizeObserver(([e]) => setWidth(Math.floor(e!.contentRect.width)));
  createEffect(() => {
    ro.observe(host);
    onCleanup(() => ro.disconnect());
  });
  createEffect(() => {
    const w = width();
    if (!w) return;
    const el = props.make(w);
    host.replaceChildren(...(el ? [el] : []));
  });
  return <div class="viz-chart" ref={host} />;
}

function Card(props: { n: number; title: string; asks: string; source: string; children: JSX.Element }) {
  return (
    <section class="viz-card" id={`viz-${props.n}`}>
      <header>
        <h3>
          <span class="viz-n">{props.n}</span> {props.title}
        </h3>
        <p class="viz-asks">{props.asks}</p>
      </header>
      {props.children}
      <p class="viz-source small">Data: {props.source}</p>
    </section>
  );
}

const time = (unix: number) => new Date(unix * 1000);
const CANON = "GEN EXO LEV NUM DEU JOS JDG RUT 1SA 2SA 1KI 2KI 1CH 2CH EZR NEH EST JOB PSA PRO ECC SNG ISA JER LAM EZK DAN HOS JOL AMO OBA JON MIC NAM HAB ZEP HAG ZEC MAL MAT MRK LUK JHN ACT ROM 1CO 2CO GAL EPH PHP COL 1TH 2TH 1TI 2TI TIT PHM HEB JAS 1PE 2PE 1JN 2JN 3JN JUD REV".split(" ");
const canonIndex = (b: string) => {
  const i = CANON.indexOf(b);
  return i < 0 ? 99 : i;
};
const byCanon = (a: string, b: string) => canonIndex(a) - canonIndex(b);

// --- A. a language's Writer books over time --------------------------------------

const PROGRESS_CHOICES = [
  ["twx", "reg"],
  ["hav", "reg"],
  ["sw", "ulb"],
  ["xsq", "reg"],
  ["tdx-x-mahafaly", "reg"],
  ["en-x-demo1", "reg"],
] as const;

function ProgressCards() {
  const [pick, setPick] = createSignal(0);
  const call = () => {
    const [ietf, resource] = PROGRESS_CHOICES[pick()]!;
    return { path: { ietf }, query: { resource } };
  };
  const [data] = createResource(call, (c) => api.json("/v1/languages/{ietf}/progress", c));
  return (
    <>
      <div class="viz-controls">
        <label>
          Language and brand{" "}
          <select onChange={(e) => setPick(Number(e.currentTarget.value))}>
            <For each={PROGRESS_CHOICES}>{([l, r], i) => <option value={i()}>{`${l} ${r}`}</option>}</For>
          </select>
        </label>
        <span class="small">One Writer project per book (the default pick: Primary and on BIEL first), its whole git history.</span>
      </div>
      <Async of={data} what="progress">
        {(d) => {
          // Every book's verse count at shared times: steps, so a book holds its
          // count until its next save. Capped at 400 instants for drawing.
          const aligned = createMemo(() => {
            const times = [...new Set(d().books.flatMap((b) => b.points.map((p) => p[0])))].sort((a, b) => a - b);
            const step = Math.max(1, Math.ceil(times.length / 400));
            const at = times.filter((_, i) => i % step === 0 || i === times.length - 1);
            const rows: { t: Date; book: string; verses: number; pct: number }[] = [];
            for (const b of d().books) {
              let i = 0;
              let v = 0;
              for (const t of at) {
                while (i < b.points.length && b.points[i]![0] <= t) v = b.points[i++]![1];
                rows.push({ t: time(t), book: b.book, verses: v, pct: b.source_verses ? (100 * v) / b.source_verses : NaN });
              }
            }
            return rows;
          });
          const target = () => d().books.reduce((n, b) => n + (b.source_verses ?? b.verses), 0);
          const authors = createMemo(() => {
            const by = new Map<string, number>();
            for (const w of d().weekly) by.set(w.author, (by.get(w.author) ?? 0) + w.saves);
            const top = [...by.entries()].sort((a, b) => b[1] - a[1]).slice(0, 7).map(([a]) => a);
            return new Set(top);
          });
          const weekly = () =>
            d().weekly.map((w) => ({ ...w, t: time(w.week), who: authors().has(w.author) ? w.author : "others" }));
          return (
            <>
              <Card
                n={1}
                title="Burn-up"
                asks="How far along is this language, how fast is it moving, and when did it stall?"
                source="/v1/languages/{ietf}/progress: verses present after every Writer save, per book; the line is the sources' total."
              >
                <Chart
                  make={(width) =>
                    Plot.plot({
                      width,
                      height: 320,
                      color: { legend: true },
                      y: { grid: true, label: "verses present" },
                      x: { label: null },
                      marks: [
                        Plot.areaY(aligned(), { x: "t", y: "verses", fill: "book", order: (a: { book: string }, b: { book: string }) => byCanon(a.book, b.book), curve: "step-after", tip: true }),
                        Plot.ruleY([target()], { stroke: "currentColor", strokeDasharray: "4,3" }),
                        Plot.text([target()], { y: (v: number) => v, text: () => `target ${target().toLocaleString()} verses`, frameAnchor: "right", dy: -6 }),
                      ],
                    })
                  }
                />
              </Card>
              <Card
                n={2}
                title="Each book toward its source"
                asks="Which books are done, which are stuck partway, and which started late?"
                source="the same series, as each book's % of its source's verse count."
              >
                <Chart
                  make={(width) =>
                    Plot.plot({
                      width,
                      height: 300,
                      // Writer can hold more verses than the source (versification): let it show.
                      y: { domain: [0, Math.max(105, ...aligned().map((r) => (Number.isNaN(r.pct) ? 0 : r.pct)))], grid: true, label: "% of source verses" },
                      x: { label: null },
                      color: { legend: true },
                      marks: [
                        Plot.ruleY([100], { strokeDasharray: "4,3" }),
                        Plot.lineY(aligned().filter((r) => !Number.isNaN(r.pct)), { x: "t", y: "pct", stroke: "book", curve: "step-after", tip: true }),
                      ],
                    })
                  }
                />
              </Card>
              <Card
                n={3}
                title="Who worked when"
                asks="Is anyone still working? Who did the bulk, and in which weeks?"
                source="/v1/languages/{ietf}/progress: Writer saves per author per week (tsDesktop is Writer's device identity, not a person)."
              >
                <Chart
                  make={(width) =>
                    Plot.plot({
                      width,
                      height: 260,
                      color: { legend: true },
                      y: { grid: true, label: "saves per week" },
                      x: { label: null },
                      // Stacked by author within each week (rectY stacks; the interval makes the bars weeks).
                      marks: [Plot.rectY(weekly(), { x: "t", y: "saves", fill: "who", interval: "week", tip: true }), Plot.ruleY([0])],
                    })
                  }
                />
              </Card>
              <Card
                n={4}
                title="Contributor × book"
                asks="Who owns which book? Is a book one person's work, or shared?"
                source="the same weekly saves, summed per author and book."
              >
                <Chart
                  make={(width) =>
                    Plot.plot({
                      width,
                      height: 60 + 26 * new Set(d().weekly.map((w) => w.author)).size,
                      marginLeft: 120,
                      x: { domain: [...new Set(d().weekly.map((w) => w.book))].sort(byCanon), label: null },
                      y: { label: null },
                      color: { type: "log", scheme: "greens", legend: true, label: "saves" },
                      marks: [
                        Plot.cell(d().weekly, Plot.group({ fill: "sum" }, { x: "book", y: "author", fill: "saves", tip: true })),
                        Plot.text(d().weekly, Plot.group({ text: "sum" }, { x: "book", y: "author", text: "saves", fill: "currentColor", fontSize: 10 })),
                      ],
                    })
                  }
                />
              </Card>
            </>
          );
        }}
      </Async>
    </>
  );
}

// --- B. one book against its source -------------------------------------------------

const BOOK_CHOICES = [
  ["wa-catalog", "ru_ulb", "01-GEN.usfm", "GEN"],
  ["wa-catalog", "ru_ulb", "19-PSA.usfm", "PSA"],
  ["wa-catalog", "ru_ulb", "42-MRK.usfm", "MRK"],
  ["wa-catalog", "km_ulb", "41-MAT.usfm", "MAT"],
] as const;

function SourceCards() {
  const [pick, setPick] = createSignal(0);
  const [data] = createResource(pick, async (i) => {
    const [owner, name, path, book] = BOOK_CHOICES[i]!;
    const proof = await api.json("/v1/repos/{owner}/{name}/proof", { path: { owner, name }, query: { book } });
    const src = proof.corpus.source;
    const srcPath = proof.corpus.books.find((b) => b.id === path)?.source;
    if (!src || !srcPath) throw new Error(`no source recorded for ${owner}/${name} ${book}`);
    const [so, sn] = src.split("@")[0]!.split("/");
    const [target, source] = await Promise.all([
      api.text("/v1/repos/{owner}/{name}/file/{path}", { path: { owner, name, path } }),
      api.text("/v1/repos/{owner}/{name}/file/{path}", { path: { owner: so!, name: sn!, path: srcPath } }),
    ]);
    const flagged = new Map(
      proof.findings
        .filter((f) => f.code === "LengthProportionality" && f.verse != null)
        .map((f) => [`${f.chapter}:${f.verse}`, f.paired?.kind ?? "?"]),
    );
    const t = versesOf(target);
    const s = versesOf(source);
    const rows: { ref: string; i: number; chapter: number; t: number; s: number; ratio: number; flag: string }[] = [];
    let i2 = 0;
    for (const [ref, text] of t) {
      const other = s.get(ref);
      if (!other || !text) continue;
      const tl = graphemes(text);
      const sl = graphemes(other);
      rows.push({ ref, i: i2++, chapter: Number(ref.split(":")[0]), t: tl, s: sl, ratio: tl / sl, flag: flagged.get(ref) ?? "" });
    }
    const sorted = rows.map((r) => r.ratio).sort((a, b) => a - b);
    return { label: `${owner}/${name} ${book} vs ${src.split("@")[0]}`, rows, median: sorted[sorted.length >> 1] ?? 1 };
  });
  return (
    <>
      <div class="viz-controls">
        <label>
          Book{" "}
          <select onChange={(e) => setPick(Number(e.currentTarget.value))}>
            <For each={BOOK_CHOICES}>{([o, n, , b], i) => <option value={i()}>{`${o}/${n} ${b}`}</option>}</For>
          </select>
        </label>
      </div>
      <Async of={data} what="the book and its source">
        {(d) => (
          <>
            <Card
              n={5}
              title="Verse length against the source"
              asks="Why is a verse flagged much longer or shorter? Is it alone, or is the whole book drifting?"
              source="both books read in the browser (graphemes per verse, markers stripped); flags from /proof LengthProportionality; the line is the book's usual ratio."
            >
              <p class="small">{d().label}: usual ratio {d().median.toFixed(2)}×, {d().rows.filter((r) => r.flag).length} flagged.</p>
              <Chart
                make={(width) =>
                  Plot.plot({
                    width,
                    height: 380,
                    x: { label: "source verse (graphemes)", grid: true },
                    y: { label: "this verse (graphemes)", grid: true },
                    color: { domain: ["", "Long", "Short"], range: ["#9aa6af", "#c0392b", "#2471a3"], legend: true, tickFormat: (v: string) => v || "not flagged" },
                    marks: [
                      Plot.line([[0, 0], [Math.max(...d().rows.map((r) => r.s)), Math.max(...d().rows.map((r) => r.s)) * d().median]], { strokeDasharray: "4,3" }),
                      Plot.dot(d().rows.filter((r) => !r.flag), { x: "s", y: "t", r: 2, fill: "flag", fillOpacity: 0.5 }),
                      Plot.dot(d().rows.filter((r) => r.flag), { x: "s", y: "t", r: 4.5, fill: "flag", stroke: "white", tip: true, title: (r: { ref: string; t: number; s: number; ratio: number }) => `${r.ref}: ${r.t} vs ${r.s} (${r.ratio.toFixed(2)}×)` }),
                    ],
                  })
                }
              />
            </Card>
            <Card
              n={6}
              title="Length ratio along the book"
              asks="Where in the book does the translation drift from the source: a chapter, a genre, a translator's stretch?"
              source="the same verses in book order; each dot is this verse ÷ the source's; the line is a moving median of 25 verses."
            >
              <Chart
                make={(width) =>
                  Plot.plot({
                    width,
                    height: 260,
                    x: { label: "verse, in book order" },
                    y: { type: "log", label: "length ratio (log)", grid: true },
                    color: { domain: ["", "Long", "Short"], range: ["#9aa6af", "#c0392b", "#2471a3"] },
                    marks: [
                      Plot.ruleY([d().median], { strokeDasharray: "4,3" }),
                      Plot.dot(d().rows, { x: "i", y: "ratio", r: (r: { flag: string }) => (r.flag ? 3.5 : 1.5), fill: "flag", tip: true, title: (r: { ref: string; ratio: number }) => `${r.ref}: ${r.ratio.toFixed(2)}×` }),
                      Plot.lineY(d().rows, Plot.windowY({ k: 25, reduce: "median" }, { x: "i", y: "ratio", stroke: "currentColor" })),
                    ],
                  })
                }
              />
            </Card>
          </>
        )}
      </Async>
    </>
  );
}

// --- C. one repo's state ----------------------------------------------------------------

const REPO_CHOICES = ["wa-catalog/ru_ulb", "wa-catalog/en_ulb", "wa-catalog/km_ulb", "texttranslation/twx_reg"] as const;

function RepoCards() {
  const [pick, setPick] = createSignal<string>(REPO_CHOICES[0]);
  const call = () => {
    const [owner, name] = pick().split("/");
    return { path: { owner: owner!, name: name! } };
  };
  const [repo] = createResource(call, (c) => api.json("/v1/repos/{owner}/{name}", c));
  const [findings] = createResource(call, (c) => api.json("/v1/repos/{owner}/{name}/findings", c));
  return (
    <>
      <div class="viz-controls">
        <label>
          Repo{" "}
          <select onChange={(e) => setPick(e.currentTarget.value)}>
            <For each={REPO_CHOICES}>{(r) => <option value={r}>{r}</option>}</For>
          </select>
        </label>
      </div>
      <Async of={repo} what="the repo">
        {(r) => {
          const cells = () =>
            r().books.flatMap((b) => {
              const gaps = b.coverage?.gaps ?? {};
              return Array.from({ length: b.chapters }, (_, i) => ({
                book: b.book,
                chapter: i + 1,
                missing: gaps[String(i + 1)]?.length ?? 0,
                covered: b.coverage != null,
              }));
            });
          return (
            <Card
              n={7}
              title="Coverage, book × chapter"
              asks="Which chapters are missing verses against the source, at a glance across the whole Bible?"
              source="/v1/repos/{owner}/{name}: each book's chapters and its coverage gaps (verse numbers the source has and this text lacks)."
            >
              <Chart
                make={(width) =>
                  Plot.plot({
                    width,
                    height: 40 + 13 * r().books.length,
                    marginLeft: 44,
                    x: { label: "chapter", axis: "top", ticks: [1, ...Array.from({ length: 15 }, (_, i) => (i + 1) * 10)] },
                    y: { domain: r().books.map((b) => b.book).sort(byCanon), label: null },
                    color: { type: "symlog", scheme: "oranges", legend: true, label: "verses missing" },
                    marks: [
                      Plot.cell(cells(), { x: "chapter", y: "book", fill: (c: { missing: number; covered: boolean }) => (c.covered ? c.missing : null), inset: 0.5, tip: true }),
                    ],
                  })
                }
              />
            </Card>
          );
        }}
      </Async>
      <Async of={findings} what="findings">
        {(f) => {
          const rows = () => f().filter((c) => c.total > 0).flatMap((c) => c.books.map((b) => ({ code: `${c.code} (${c.engine})`, book: b.book, n: b.n })));
          return (
            <Card
              n={8}
              title="Findings, book × check"
              asks="Where does each check fire? One noisy book, or everywhere (a convention, or a rule to tune)?"
              source="/v1/repos/{owner}/{name}/findings: counts per check per book."
            >
              <Chart
                make={(width) =>
                  Plot.plot({
                    width,
                    height: 60 + 20 * new Set(rows().map((r) => r.code)).size,
                    marginLeft: 200,
                    x: { domain: [...new Set(rows().map((r) => r.book))].sort(byCanon), tickRotate: -60, label: null },
                    y: { label: null },
                    color: { type: "log", scheme: "purples", legend: true, label: "findings" },
                    marks: [Plot.cell(rows(), { x: "book", y: "code", fill: "n", tip: true })],
                  })
                }
              />
            </Card>
          );
        }}
      </Async>
    </>
  );
}

// --- D. across WACS ---------------------------------------------------------------------

/** Repos per quarter of their last commit, by kind (Writer = served as synthetic). */
function quarters(all: readonly RepoSummary[]) {
  const by = new Map<string, { q: Date; kind: string; n: number }>();
  for (const r of all) {
    if (!r.head_unix) continue;
    const d = time(r.head_unix);
    const q = new Date(Date.UTC(d.getUTCFullYear(), Math.floor(d.getUTCMonth() / 3) * 3, 1));
    const kind = r.is_synthetic ? "Writer project" : (r.shape ?? "other");
    const key = `${q.getTime()} ${kind}`;
    const row = by.get(key) ?? { q, kind, n: 0 };
    row.n += 1;
    by.set(key, row);
  }
  return [...by.values()];
}

async function allRepos(): Promise<RepoSummary[]> {
  const out: RepoSummary[] = [];
  for (let offset = 0; offset < 50_000; offset += 500) {
    const page = await api.json("/v1/repos", { query: { limit: 500, offset } });
    out.push(...page.repos);
    if (page.repos.length < 500) break;
  }
  return out;
}

function HubCards() {
  const [rules] = createResource(() => api.json("/v1/findings"));
  const [repos] = createResource(allRepos);
  const [conflicts] = createResource(() => api.json("/v1/biel/conflicts"));
  const [lang, setLang] = createSignal("ceb");
  const [language] = createResource(lang, (ietf) => api.json("/v1/languages/{ietf}", { path: { ietf } }));
  return (
    <>
      <Async of={rules} what="checks">
        {(r) => (
          <Card
            n={9}
            title="Which checks are noise"
            asks="Is a check finding real problems everywhere, or one repo's habit? (High and left: few repos, many findings.)"
            source="/v1/findings: per check, findings, repos it fires in, and the top repo's share (dot size)."
          >
            <Chart
              make={(width) =>
                Plot.plot({
                  width,
                  height: 360,
                  x: { type: "log", label: "repos it fires in", grid: true },
                  y: { type: "log", label: "findings", grid: true },
                  r: { range: [2, 14] },
                  color: { legend: true },
                  marks: [
                    Plot.dot(r(), { x: "repos", y: "total", r: "top_repo_share", fill: "engine", fillOpacity: 0.7, tip: true, title: (c: { code: string; total: number; repos: number; top_repo_share: number }) => `${c.code}: ${c.total.toLocaleString()} in ${c.repos} repos, top repo ${(c.top_repo_share * 100).toFixed(0)}%` }),
                    Plot.text(r().filter((c) => c.total > 5000), { x: "repos", y: "total", text: "code", dy: -10, fontSize: 10 }),
                  ],
                })
              }
            />
          </Card>
        )}
      </Async>
      <Async of={repos} what="repos">
        {(all) => (
          <Card
            n={10}
            title="How much of WACS is alive"
            asks="When was each repo last touched? How many Writer projects went quiet years ago?"
            source="/v1/repos (every public repo): last commit time, by kind (Writer = served as synthetic)."
          >
            <Chart
              make={(width) =>
                Plot.plot({
                  width,
                  height: 260,
                  color: { legend: true },
                  y: { grid: true, label: "repos" },
                  x: { label: "last commit" },
                  marks: [
                    // Counted here per quarter and kind, then stacked by rectY.
                    Plot.rectY(quarters(all()), { x: "q", y: "n", fill: "kind", interval: "quarter", tip: true }),
                    Plot.ruleY([0]),
                  ],
                })
              }
            />
          </Card>
        )}
      </Async>
      <Async of={conflicts} what="BIEL conflicts">
        {(c) => {
          const rows = () => {
            const by = new Map<string, { language: string; n: number }>();
            for (const x of c()) {
              const k = x.language_name ? `${x.language_name} (${x.language})` : x.language;
              by.set(k, { language: k, n: (by.get(k)?.n ?? 0) + 1 });
            }
            return [...by.values()].sort((a, b) => b.n - a.n).slice(0, 25);
          };
          return (
            <Card
              n={11}
              title="BIEL shows these books twice"
              asks="Which languages need someone to pick one repo per book (Primary and shown on BIEL, twice)?"
              source="/v1/biel/conflicts: books with more than one Primary, shown repo."
            >
              <Chart
                make={(width) =>
                  Plot.plot({
                    width,
                    height: 40 + 18 * rows().length,
                    marginLeft: 200,
                    x: { grid: true, label: "books shown twice" },
                    y: { label: null },
                    marks: [Plot.barX(rows(), { x: "n", y: "language", sort: { y: "-x" }, fill: "#a8445a", tip: true }), Plot.ruleX([0])],
                  })
                }
              />
            </Card>
          );
        }}
      </Async>
      <div class="viz-controls">
        <label>
          Language{" "}
          <input value={lang()} size={10} onChange={(e) => setLang(e.currentTarget.value.trim().toLowerCase() || "ceb")} />
        </label>
      </div>
      <Async of={language} what="the language">
        {(l) => (
          <Card
            n={12}
            title="A language: book × version"
            asks="For one language, which books exist in which version, and is each settled (Official), a draft, or in conflict?"
            source="/v1/languages/{ietf}: per book and version (resource type), the hub's Official pick and state."
          >
            <Chart
              make={(width) =>
                Plot.plot({
                  width,
                  height: 60 + 26 * new Set(l().books.map((b) => b.resource_type)).size,
                  marginLeft: 60,
                  x: { domain: [...new Set(l().books.map((b) => b.book))].sort(byCanon), tickRotate: -60, label: null },
                  y: { label: null },
                  color: { domain: ["official", "draft", "conflict", "inactive", "empty"], range: ["#2f7a57", "#c08a1e", "#a8445a", "#9aa6af", "#dee3df"], legend: true },
                  marks: [Plot.cell(l().books, { x: "book", y: "resource_type", fill: "state", inset: 0.5, tip: true, title: (b: { book: string; official?: string | null; state: string }) => `${b.book}: ${b.state}${b.official ? `, ${b.official}` : ""}` })],
                })
              }
            />
          </Card>
        )}
      </Async>
    </>
  );
}

export default function Viz() {
  startPageView("viz");
  return (
    <>
      <header class="page-head">
        <h1>Chart candidates</h1>
        <p class="lede">Twelve charts on real data, to keep or cut. Each says the question it answers; the numbers are for referring to them.</p>
        <nav class="viz-toc small">
          <a href="#viz-1">A. A language over time (1–4)</a> · <a href="#viz-5">B. A book against its source (5–6)</a> ·{" "}
          <a href="#viz-7">C. A repo's state (7–8)</a> · <a href="#viz-9">D. Across WACS (9–12)</a>
        </nav>
      </header>
      <h2>A. A language over time</h2>
      <ProgressCards />
      <h2>B. A book against its source</h2>
      <SourceCards />
      <h2>C. A repo's state</h2>
      <RepoCards />
      <h2>D. Across WACS</h2>
      <HubCards />
    </>
  );
}

