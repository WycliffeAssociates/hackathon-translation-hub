// The History tab: the repo's newest commits, read from git by the server.
// The hub's own writes carry trailers (who asked, where a book came from), so
// a pull shows its source and the person, and a replayed Writer save its
// project; release tags sit on the commit they mark.
import { For, Show, createResource } from "solid-js";
import { api } from "../../api/client";
import { Async, ago, dateOf } from "../../ui/bits";

export default function History(props: { owner: string; name: string }) {
  const [log] = createResource(
    () => ({ path: { owner: props.owner, name: props.name }, query: { limit: 200 } }),
    (c) => api.json("/v1/repos/{owner}/{name}/commits", c),
  );
  return (
    <Async of={log} what="history">
      {(rows) => (
        <Show when={rows().length} fallback={<p class="small">No commits.</p>}>
          <ol class="history">
            <For each={rows()}>
              {(c) => (
                <li>
                  <span class="history-when" title={dateOf(c.unix)}>{ago(c.unix)}</span>
                  <span class="history-what">
                    <For each={c.tags}>{(t) => <b class="history-tag">{t}</b>}</For>
                    {c.summary}
                    <Show when={c.source && !c.summary.includes(c.source.split("@")[0]!) ? c.source : null}>
                      {(src) => <span class="muted"> · from {src().split("@")[0]}</span>}
                    </Show>
                  </span>
                  <span class="history-who">
                    {c.by ?? c.author}
                    <Show when={c.by && c.by !== c.author}><span class="muted"> via the hub</span></Show>
                  </span>
                  <code class="history-sha" title={c.sha}>{c.sha.slice(0, 7)}</code>
                </li>
              )}
            </For>
          </ol>
          <Show when={rows().length >= 200}><p class="small">The newest 200 commits.</p></Show>
        </Show>
      )}
    </Async>
  );
}
