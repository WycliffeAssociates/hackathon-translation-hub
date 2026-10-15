import { For, Show, createResource, createSignal, lazy } from "solid-js";
import { api, startPageView } from "../api/client";
import { Async, ago, dateOf } from "../ui/bits";

// The WACS-wide charts (Observable Plot) load when asked for.
const WacsCharts = lazy(() => import("../charts/WacsCharts"));

export default function Status() {
  startPageView("status");
  const [totals] = createResource(() => api.json("/v1/status"));
  const [charts, setCharts] = createSignal(false);
  return (
    <>
      <h1>Status</h1>
      <Async of={totals} what="status">
        {(t) => (
          <>
            <div class="stats">
              <For
                each={[
                  ["Repos", t().repos],
                  ["Scanned", t().scanned],
                  ["Synthetic", t().synthetic],
                  ["Books", t().books],
                  ["Lint errors", t().errors],
                  ["Lint warnings", t().warnings],
                  ["Languages", t().languages],
                  ["Catalog rows", t().catalog_rows],
                  ["Proofread", t().proofread],
                ] as const}
              >
                {([label, n]) => (
                  <div class="stat">
                    <b>{n.toLocaleString()}</b>
                    <span>{label}</span>
                  </div>
                )}
              </For>
            </div>
            <h2>Watermarks</h2>
            <table>
              <tbody>
                <For each={Object.entries(t().state)}>
                  {([key, value]) => {
                    const unix = key.endsWith("_unix") ? Number(value) : null;
                    return (
                      <tr>
                        <th scope="row" class="mono">{key}</th>
                        <td>{unix ? `${dateOf(unix)} UTC (${ago(unix)})` : String(value)}</td>
                      </tr>
                    );
                  }}
                </For>
              </tbody>
            </table>
          </>
        )}
      </Async>
      <p>
        API: <a href="/v1/openapi.json">/v1/openapi.json</a>
      </p>
      <h2>Across WACS</h2>
      <Show when={charts()} fallback={<button type="button" onClick={() => setCharts(true)}>Show the charts</button>}>
        <WacsCharts />
      </Show>
    </>
  );
}
