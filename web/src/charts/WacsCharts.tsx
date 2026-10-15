// Charts across all of WACS, on the Status page: which checks are noise, how
// much of WACS is alive, and which languages BIEL shows twice. Kept from the
// /viz candidates (Will, 2026-10-15).
import * as Plot from "@observablehq/plot";
import { createResource } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { api } from "../api/client";
import type { RepoSummary } from "../api/types";
import { Async } from "../ui/bits";
import { Card, Chart, time } from "./kit";

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

export default function WacsCharts() {
  const navigate = useNavigate();
  const [rules] = createResource(() => api.json("/v1/findings"));
  const [repos] = createResource(allRepos);
  const [conflicts] = createResource(() => api.json("/v1/biel/conflicts"));
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
            const by = new Map<string, { language: string; ietf: string; n: number }>();
            for (const x of c()) {
              const k = x.language_name ? `${x.language_name} (${x.language})` : x.language;
              by.set(k, { language: k, ietf: x.language, n: (by.get(k)?.n ?? 0) + 1 });
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
                onPick={(row: { ietf: string }) => navigate(`/languages/${row.ietf}`)}
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
    </>
  );
}

