import { A } from "@solidjs/router";
import { For, Show, createResource } from "solid-js";
import { api, startPageView } from "../api/client";
import type { BielConflict } from "../api/types";
import { Async, RepoLink, StatusChip } from "../ui/bits";

// Books BIEL shows twice: two repos both Primary and shown on BIEL, per language.
export default function Biel() {
  startPageView("biel");
  const [conflicts] = createResource(() => api.json("/v1/biel/conflicts"));
  const byLanguage = (list: readonly BielConflict[]) => {
    const m = new Map<string, BielConflict[]>();
    for (const c of list) m.set(c.language, [...(m.get(c.language) ?? []), c]);
    return [...m.values()].sort((a, b) => b.length - a.length);
  };
  return (
    <>
      <h1>BIEL conflicts</h1>
      <p class="lede">Books that BIEL shows twice: in each, two or more repos are both Primary and shown on BIEL.</p>
      <Async of={conflicts} what="conflicts">
        {(list) => (
          <Show when={list().length} fallback={<p class="clean-note">No book is shown twice.</p>}>
            <p class="small">{list().length} books in {byLanguage(list()).length} languages.</p>
            <For each={byLanguage(list())}>
              {(group) => (
                <section class="conflict">
                  <h3>
                    <A href={`/languages/${group[0]!.language}`}>{group[0]!.language_name ?? group[0]!.language}</A>{" "}
                    <code>{group[0]!.language}</code> <span class="count">{group.length} book{group.length === 1 ? "" : "s"}</span>
                  </h3>
                  <table>
                    <tbody>
                      <For each={group}>
                        {(c) => (
                          <tr>
                            <th scope="row" class="mono">{c.book}</th>
                            <td class="small">{c.resource_type} · {c.track}</td>
                            <td>
                              <For each={c.repos}>
                                {(r) => (
                                  <div>
                                    <Show when={r.owner && r.name} fallback={<span class="mono">{r.name ?? r.content_id}</span>}>
                                      <RepoLink owner={r.owner!} name={r.name!} />
                                    </Show>{" "}
                                    <StatusChip status={r.status} /> <span class="small">{r.modified_on?.slice(0, 10)}</span>
                                  </div>
                                )}
                              </For>
                            </td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </section>
              )}
            </For>
          </Show>
        )}
      </Async>
    </>
  );
}
