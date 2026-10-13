import { A, useParams } from "@solidjs/router";
import { For, Show, createMemo, createResource } from "solid-js";
import { api, startPageView } from "../api/client";
import type { BookPick, RepoSummary } from "../api/types";
import { Async, repoHref } from "../ui/bits";
import { RepoRow } from "./Home";

const groupBy = <T,>(xs: readonly T[], key: (x: T) => string) => {
  const m = new Map<string, T[]>();
  for (const x of xs) m.set(key(x), [...(m.get(key(x)) ?? []), x]);
  return [...m.entries()];
};

export default function Language() {
  const params = useParams<{ ietf: string }>();
  const ietf = createMemo(() => {
    startPageView("language");
    return params.ietf;
  });
  const [lang] = createResource(ietf, (ietf) => api.json("/v1/languages/{ietf}", { path: { ietf } }));

  return (
    <Async of={lang} what={params.ietf}>
      {(l) => (
        <>
          <nav class="crumbs"><A href="/">Repos</A> › {l().ietf}</nav>
          <header class="page-head">
            <h1>
              {l().facts?.english_name ?? l().ietf}{" "}
              <Show when={l().facts?.name && l().facts?.name !== l().facts?.english_name}>
                <span class="native">{l().facts?.name}</span>
              </Show>
            </h1>
            <p class="facts">
              <code>{l().ietf}</code>
              <Show when={l().facts?.region}><span>{l().facts?.region}</span></Show>
              <Show when={l().facts?.gateway != null}><span class="tag">{l().facts?.gateway ? "gateway language" : "heart language"}</span></Show>
              <Show when={l().facts?.direction}><span>{l().facts?.direction}</span></Show>
              <span>{l().repos.length} repos</span>
            </p>
            <Show when={l().facts?.alternate_names.length}>
              <p class="small">Also called {l().facts?.alternate_names.join(", ")}</p>
            </Show>
          </header>

          <h2>Repos</h2>
          <Show when={l().repos.length} fallback={<p class="small">No repos here in this language.</p>}>
            <For each={groupBy(l().repos, (r: RepoSummary) => r.resource ?? "unknown")}>
              {([resource, repos]) => (
                <section class="group">
                  <h3>{resource} <span class="count">{repos.length}</span></h3>
                  <ul class="feed"><For each={repos}>{(r) => <RepoRow repo={r} />}</For></ul>
                </section>
              )}
            </For>
          </Show>

          <h2>Books</h2>
          <p class="small">Per book, which repo is Official, and whether BIEL shows the book twice (two repos both Primary and shown on BIEL).</p>
          <Show when={l().books.length} fallback={<p class="small">No catalog entries.</p>}>
            <Picks books={l().books} />
          </Show>
        </>
      )}
    </Async>
  );
}

function Picks(props: { books: readonly BookPick[] }) {
  return (
    <div class="books">
      <table>
        <thead>
          <tr><th scope="col">Version</th><th scope="col">Book</th><th scope="col">State</th><th scope="col">Official</th><th scope="col">Repos</th><th scope="col">BIEL</th></tr>
        </thead>
        <tbody>
          <For each={props.books}>
            {(b) => (
              <tr classList={{ focus: b.shown_twice_on_biel }}>
                <td class="small">{b.resource_type} · {b.track}</td>
                <th scope="row" class="mono">{b.book}</th>
                <td><span class={`state-chip ${b.state}`}>{b.state}</span></td>
                <td>
                  <Show when={b.official} fallback={<span class="small">none</span>}>
                    {(o) => <A class="mono" href={repoHref(o().split("/")[0]!, o().split("/")[1]!)}>{o()}</A>}
                  </Show>
                </td>
                <td class="num">{b.candidates.length}</td>
                <td>
                  <Show when={b.shown_twice_on_biel}><span class="sev error">shown twice</span></Show>
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
    </div>
  );
}
