// One language. What is published leads the page: per main resource (Bible,
// notes, questions, stories), the published repo and its newest release, or
// "nothing published" with the way to create or choose one. Then the repos,
// grouped by resource under its full name, the main resources first and the
// rest (odd or one-off resource codes) folded away.
import { A, useParams } from "@solidjs/router";
import { For, Show, createMemo, createResource, createSignal } from "solid-js";
import { api, startPageView } from "../api/client";
import type { Published, RepoSummary } from "../api/types";
import { Async, ago, plural } from "../ui/bits";
import { isMain, resourceTitle } from "../ui/resources";
import { me } from "../ui/session";
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
  const [published] = createResource(ietf, (ietf) => api.json("/v1/languages/{ietf}/published", { path: { ietf } }).catch(() => [] as Published[]));
  const [showOther, setShowOther] = createSignal(false);

  return (
    <Async of={lang} what={params.ietf}>
      {(l) => {
        const name = () => l().facts?.english_name ?? l().ietf;
        const groups = () =>
          groupBy(l().repos, (r: RepoSummary) => r.resource || "none").sort(
            ([a, x], [b, y]) => Number(isMain(b)) - Number(isMain(a)) || y.length - x.length,
          );
        const main = () => groups().filter(([r]) => isMain(r));
        const other = () => groups().filter(([r]) => !isMain(r));
        const shown = () => (main().length ? main() : other());
        const lead = () => (published() ?? []).filter((p) => isMain(p.resource) || p.published);
        return (
          <>
            <nav class="crumbs">
              <A href="/">Repos</A> › {l().ietf}
            </nav>
            <header class="page-head">
              <h1>
                {name()}{" "}
                <Show when={l().facts?.name && l().facts?.name !== l().facts?.english_name}>
                  <span class="native">{l().facts?.name}</span>
                </Show>
              </h1>
              <p class="facts">
                <code>{l().ietf}</code>
                <Show when={l().facts?.region}>
                  <span>{l().facts?.region}</span>
                </Show>
                <Show when={l().facts?.gateway != null}>
                  <span>{l().facts?.gateway ? "gateway language" : "heart language"}</span>
                </Show>
                <span>{plural(l().repos.length, "repo")}</span>
              </p>
            </header>

            <Show when={lead().length}>
              <section class="published-list" aria-label="Published">
                <h2>Published</h2>
                <ul>
                  <For each={lead()}>
                    {(p) => (
                      <li classList={{ none: !p.published }}>
                        <span class="published-resource">
                          {resourceTitle(p.resource)} <span class="muted">{p.resource}</span>
                        </span>
                        <Show
                          when={p.published}
                          fallback={
                            <span class="published-what">
                              <span class="muted">Nothing published</span>
                              <Show when={me()?.me?.publisher && p.suggest}>
                                {(s) => <span class="muted"> · {s()} could be</span>}
                              </Show>
                            </span>
                          }
                        >
                          {(x) => (
                            <span class="published-what">
                              <A class="mono" href={`/repos/${x().repo}`}>
                                {x().repo}
                              </A>
                              <Show when={p.last_tag} fallback={<span class="muted"> · no release yet</span>}>
                                <span> · {p.last_tag}</span>
                                <span class="muted"> {ago(p.last_tag_unix)}</span>
                              </Show>
                              <Show when={p.books}>
                                <span class="muted"> · {plural(p.books!, "book")}</span>
                              </Show>
                            </span>
                          )}
                        </Show>
                        <A class="published-go" href={`/languages/${l().ietf}/sources?resource=${encodeURIComponent(p.resource)}`}>
                          {p.published ? "Sources" : "Choose or create"} ›
                        </A>
                      </li>
                    )}
                  </For>
                </ul>
              </section>
            </Show>

            <Show when={l().facts?.alternate_names.length}>
              <p class="small muted">Also called {l().facts?.alternate_names.join(", ")}</p>
            </Show>

            <h2>Repos</h2>
            <Show when={l().repos.length} fallback={<p class="small">No repos here in this language.</p>}>
              <For each={showOther() ? [...shown(), ...(main().length ? other() : [])] : shown()}>
                {([resource, repos]) => (
                  <section class="group">
                    <h3 class="resource-head">
                      {resourceTitle(resource)} <span class="muted">{resource}</span> <span class="count">{repos.length}</span>
                    </h3>
                    <ul class="feed">
                      <For each={repos}>{(r) => <RepoRow repo={r} />}</For>
                    </ul>
                  </section>
                )}
              </For>
              <Show when={main().length && other().length}>
                <p>
                  <button type="button" class="linkish" onClick={() => setShowOther(!showOther())}>
                    {showOther()
                      ? "Hide the other resources"
                      : `Other resources: ${other()
                          .map(([r, xs]) => `${r} (${xs.length})`)
                          .join(", ")}`}
                  </button>
                </p>
              </Show>
            </Show>
          </>
        );
      }}
    </Async>
  );
}
