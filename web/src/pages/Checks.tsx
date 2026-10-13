// Every check across the hub, by total. The point is the outliers: a check
// that fires mostly in one or two repos is often a writing system's convention
// (MisplacedFormat in Khmer, Lao, Persian, Hebrew) rather than a mistake.
import { A } from "@solidjs/router";
import { For, Show, createResource, createSignal } from "solid-js";
import { api, startPageView } from "../api/client";
import type { HubCode } from "../api/types";
import { Async, CheckBadges, checkKey, repoHref } from "../ui/bits";

/** At or above this share of its total in one repo, a check is flagged. */
const DOMINATED = 0.3;

export default function Checks() {
  startPageView("checks");
  const [codes] = createResource(() => api.json("/v1/findings"));
  const [open, setOpen] = createSignal(new Set<string>());
  const toggle = (key: string) =>
    setOpen((s) => {
      const next = new Set(s);
      if (!next.delete(key)) next.add(key);
      return next;
    });
  return (
    <>
      <h1>Checks</h1>
      <p class="lede">
        Every check across all repos, by count, for spotting checks that fire because of a writing system's convention rather than a mistake
        (MisplacedFormat in Khmer, Lao, Persian, Hebrew): a large share in one repo is the tell.
      </p>
      <Async of={codes} what="checks">
        {(list) => (
          <Show when={list().length} fallback={<p class="clean-note">No check fired.</p>}>
            <div class="checks-table">
              <table>
                <thead>
                  <tr>
                    <th scope="col">Check</th>
                                        <th scope="col" class="num">Total</th>
                    <th scope="col" class="num opt">Repos</th>
                    <th scope="col" class="num opt">Books</th>
                    <th scope="col">Top repo</th>
                  </tr>
                </thead>
                <tbody>
                  <For each={list()}>
                    {(c) => {
                      const key = checkKey(c);
                      const isOpen = () => open().has(key);
                      return (
                        <>
                          <tr class="check-row" onClick={() => toggle(key)}>
                            <th scope="row">
                              <button type="button" class="linkish mono" aria-expanded={isOpen()}>
                                <span class="caret" aria-hidden="true">{isOpen() ? "▾" : "▸"}</span>{c.code}
                              </button> <CheckBadges engine={c.engine} severity={c.severity} />
                            </th>
                            <td class="num total">{c.total.toLocaleString()}</td>
                            <td class="num opt">{c.repos.toLocaleString()}</td>
                            <td class="num opt">{c.books.toLocaleString()}</td>
                            <td><Share c={c} /></td>
                          </tr>
                          <Show when={isOpen()}>
                            <tr class="check-books">
                              <td colSpan={5}>
                                <TopRepos engine={c.engine} code={c.code} />
                              </td>
                            </tr>
                          </Show>
                        </>
                      );
                    }}
                  </For>
                </tbody>
              </table>
            </div>
          </Show>
        )}
      </Async>
    </>
  );
}

/** "31% in one repo", with a bar; loud when one repo explains much of the count. */
function Share(props: { c: HubCode }) {
  const pct = () => Math.round(props.c.top_repo_share * 100);
  const loud = () => props.c.repos > 1 && props.c.top_repo_share >= DOMINATED;
  return (
    <span class="share" classList={{ loud: loud() }} title={`${pct()}% of ${props.c.total.toLocaleString()} in the noisiest repo`}>
      <span class="share-bar" aria-hidden="true"><i style={{ width: `${pct()}%` }} /></span>{" "}
      {props.c.repos === 1 ? "all in one repo" : `${pct()}% in one repo`}
    </span>
  );
}

function TopRepos(props: { engine: string; code: string }) {
  const [repos] = createResource(
    () => ({ path: { engine: props.engine, code: props.code }, query: { limit: 10 } }),
    (c) => api.json("/v1/findings/{engine}/{code}", c),
  );
  return (
    <Async of={repos} what={`repos for ${props.code}`}>
      {(list) => (
        <ol class="top-repos">
          <For each={list()}>
            {(r) => (
              <li>
                <A class="mono" href={`${repoHref(r.owner, r.name)}?check=${encodeURIComponent(checkKey(props))}`}>
                  {r.owner}/{r.name}
                </A>{" "}
                <span class="count">{r.n.toLocaleString()}</span>{" "}
                <span class="small">
                  {r.title}
                  <Show when={r.language}> · {r.language}</Show> · {r.books} book{r.books === 1 ? "" : "s"}
                </span>
              </li>
            )}
          </For>
        </ol>
      )}
    </Async>
  );
}
