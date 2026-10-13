import { A, useSearchParams } from "@solidjs/router";
import { For, Show, createResource } from "solid-js";
import { api, startPageView } from "../api/client";
import { Async } from "../ui/bits";
import { RepoRow } from "./Home";

export default function Search() {
  startPageView("search");
  const [params, setParams] = useSearchParams<{ q?: string }>();
  const q = () => params.q?.trim() ?? "";
  const [languages] = createResource(q, (q) => api.json("/v1/languages", { query: { q, limit: 30 } }));
  const [repos] = createResource(q, (q) => api.json("/v1/repos", { query: { q, limit: 30 } }));

  return (
    <>
      <h1>Search</h1>
      <form
        class="search-box"
        role="search"
        onSubmit={(e) => {
          e.preventDefault();
          setParams({ q: String(new FormData(e.currentTarget).get("q") ?? "") || undefined });
        }}
      >
        <input name="q" type="search" value={params.q ?? ""} placeholder="A language, a code, owner/name, a title" aria-label="Search" />
        <button type="submit">Search</button>
      </form>
      <Show when={q()} fallback={<p class="small">Type a language name or code (e.g. <A href="/search?q=cebuano">cebuano</A>), or part of a repo name.</p>}>
        <div class="side-by-side">
          <section>
            <h2>Languages</h2>
            <Async of={languages} what="languages">
              {(ls) => (
                <Show when={ls().length} fallback={<p class="small">No languages match.</p>}>
                  <ul class="results">
                    <For each={ls()}>
                      {(l) => (
                        <li>
                          <A class="result-title" href={`/languages/${l.ietf}`}>{l.english_name ?? l.ietf}</A>
                          <Show when={l.name && l.name !== l.english_name}><span class="native">{l.name}</span></Show>
                          <code>{l.ietf}</code>
                          <div class="small">
                            {l.repos} repo{l.repos === 1 ? "" : "s"}
                            <Show when={l.region}> · {l.region}</Show>
                            <Show when={l.gateway}> · gateway</Show>
                          </div>
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
              )}
            </Async>
          </section>
          <section>
            <h2>Repos</h2>
            <Async of={repos} what="repos">
              {(p) => (
                <Show when={p().repos.length} fallback={<p class="small">No repos match.</p>}>
                  <p class="small">{p().total.toLocaleString()} match{p().total === 1 ? "" : "es"}{p().total > p().repos.length ? `; the first ${p().repos.length}` : ""}</p>
                  <ul class="feed"><For each={p().repos}>{(r) => <RepoRow repo={r} />}</For></ul>
                </Show>
              )}
            </Async>
          </section>
        </div>
      </Show>
    </>
  );
}
