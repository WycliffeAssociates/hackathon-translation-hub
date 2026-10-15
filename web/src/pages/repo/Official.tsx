// The publishing side of one repo (a proposal), at the top of its Releases
// tab: what is published for this repo's language and resource, and for a
// publisher, the two ways this repo can become it:
//
// - **Make this the published one**: a pointer kept by the hub. Nothing is
//   copied; readers are sent here. The hub then reads it but does not write
//   to it (it writes only in the official org).
// - **Copy it into the official org**: a fork (Forgejo's "Adopt" here), so
//   the hub can pull books into it and release from it.
//
// Pulling books and creating a published repo from Writer projects happen on
// the sources page, book by book. The server does the work and the checks.
import { For, Show, createResource, createSignal } from "solid-js";
import { A } from "@solidjs/router";
import { api } from "../../api/client";
import type { Done } from "../../api/types";
import { ago } from "../../ui/bits";
import { resourceTitle } from "../../ui/resources";
import { me, signInHref } from "../../ui/session";

export default function Official(props: { owner: string; name: string; language: string | null | undefined; resource: string | null | undefined }) {
  const id = () => ({ path: { owner: props.owner, name: props.name } });
  const [info, { refetch }] = createResource(
    () => me()?.publishing && id(),
    (c) => api.json("/v1/repos/{owner}/{name}/official", c),
  );
  const [published, { refetch: refetchPublished }] = createResource(
    () => (props.language ? { path: { ietf: props.language } } : false),
    (c) => api.json("/v1/languages/{ietf}/published", c).catch(() => []),
  );
  const mine = () => published()?.find((p) => p.resource === props.resource);
  const self = () => `${props.owner}/${props.name}`.toLowerCase();
  const isPublished = () => mine()?.published?.repo === self();
  const [busy, setBusy] = createSignal(false);
  const [done, setDone] = createSignal<string>();
  const [failed, setFailed] = createSignal<string>();
  const publisher = () => info()?.can_publish === true;
  const what = () => `${props.language ?? ""} ${resourceTitle(props.resource ?? "")}`;

  const act = async (question: string, run: () => Promise<Done | boolean>, after?: string) => {
    if (!confirm(question)) return;
    setBusy(true);
    setFailed(undefined);
    setDone(undefined);
    try {
      const r = await run();
      await Promise.all([refetch(), refetchPublished()]);
      setDone(after ?? (typeof r === "object" ? r.message : ""));
    } catch (e) {
      setFailed(e instanceof Error ? e.message : String((e as { message?: string }).message ?? e));
    } finally {
      setBusy(false);
    }
  };
  const bless = (repo: string | null) =>
    act(
      repo ? `Make ${repo} the published ${what()}? Nothing is copied; the hub points at it.` : `Clear the choice of published ${what()}?`,
      () => api.post("/v1/languages/{ietf}/published", { path: { ietf: props.language! } }, { resource: props.resource!, repo }),
      repo ? `${repo} is now the published ${what()}.` : "Cleared.",
    );

  return (
    <Show when={info()}>
      {(o) => (
        <section class="publishing" aria-label="Publishing">
          <Show when={props.language && props.resource} fallback={<p class="small">This repo declares no language or resource, so it cannot be published.</p>}>
            <Show
              when={isPublished()}
              fallback={
                <p>
                  Published {what()}:{" "}
                  <Show when={mine()?.published} fallback={<span class="muted">nothing yet</span>}>
                    {(p) => (
                      <A class="mono" href={`/repos/${p().repo}?tab=releases`}>
                        {p().repo}
                      </A>
                    )}
                  </Show>{" "}
                  <A class="small" href={`/languages/${props.language}/sources?resource=${encodeURIComponent(props.resource!)}`}>
                    Sources by book ›
                  </A>
                </p>
              }
            >
              <p>
                <b>This is the published {what()}.</b>{" "}
                <Show when={mine()?.published?.chosen} fallback={<span class="muted">(the official org's repo)</span>}>
                  <span class="muted">(chosen by a publisher)</span>
                </Show>
              </p>
            </Show>

            <Show
              when={publisher()}
              fallback={
                <p class="small muted">
                  <Show when={me()?.me} fallback={<a href={signInHref()} rel="external">Sign in</a>}>
                    {(m) => <>{m().username} is not a publisher.</>}
                  </Show>{" "}
                  Publishers choose what is published, and release it.
                </p>
              }
            >
              <div class="publishing-actions">
                <Show when={!isPublished() && !o().is_official}>
                  <button type="button" class="primary" disabled={busy()} onClick={() => void bless(self())}>
                    Make this the published one
                  </button>
                  <Show when={o().can_adopt}>
                    <button
                      type="button"
                      disabled={busy()}
                      title="A fork in the official org, so the hub can pull books into it and release from it"
                      onClick={() =>
                        void act(`Copy ${self()} into ${o().target?.owner}/${o().target?.name} (a fork, with its history), so the hub can pull into it and release from it?`, () =>
                          api.post("/v1/repos/{owner}/{name}/adopt", id(), {} as never),
                        )
                      }
                    >
                      Copy it into {o().target?.owner}
                    </button>
                  </Show>
                </Show>
                <Show when={isPublished() && mine()?.published?.chosen}>
                  <button type="button" class="linkish small" disabled={busy()} onClick={() => void bless(null)}>
                    Clear this choice
                  </button>
                </Show>
              </div>
              <Show when={!o().is_official && !isPublished()}>
                <p class="small muted">
                  Making it the published one only points at it; the hub reads it and does not write to it. Copying it into {o().target?.owner} makes a
                  fork the hub can pull books into and release from.
                </p>
              </Show>
            </Show>
          </Show>
          <Show when={done()}>{(m) => <p class="ok" role="status">{m()}</p>}</Show>
          <Show when={failed()}>{(m) => <p class="failure" role="alert">{m()}</p>}</Show>
          <Show when={o().actions.length}>
            <ol class="official-log">
              <For each={o().actions.slice(0, 12)}>
                {(a) => (
                  <li>
                    {ago(a.at_unix)}: <b>{a.username}</b>{" "}
                    {a.kind === "adopt" ? "copied" : a.kind === "consolidate" ? "consolidated Writer projects into" : a.kind === "pull" ? `pulled ${a.book}` : `published ${a.ref}`}{" "}
                    <span class="mono">{a.kind === "publish" || a.kind === "consolidate" ? a.target : `${a.source?.split("@")[0]} → ${a.target}`}</span>
                  </li>
                )}
              </For>
            </ol>
          </Show>
        </section>
      )}
    </Show>
  );
}
