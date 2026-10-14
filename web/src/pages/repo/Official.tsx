// The official side of one repo (a proposal): which official repo its language
// and brand publish to, that repo's releases, and, for a publisher, the three
// writes the hub makes: adopt (fork this consolidated repo in as the official
// one), pull (one book, stitched for a Writer project, as one commit), publish
// (a release tag), and, for a Writer project whose language has no official
// repo yet, consolidate (every Writer book of the language and brand into a new
// official repo, every save kept as a commit). The server does the work and the
// checks; this only asks.
import { For, Show, createResource, createSignal } from "solid-js";
import { api } from "../../api/client";
import type { BookRow, Done } from "../../api/types";
import { ago } from "../../ui/bits";
import { me, signInHref } from "../../ui/session";
import Assemble from "./Assemble";

/** The release after the newest one: v1.3 → v1.4; none → v1.0. */
const nextTag = (tags: readonly string[]) => {
  const newest = tags
    .map((t) => /^v(\d+)\.(\d+)$/.exec(t))
    .filter((m): m is RegExpExecArray => m !== null)
    .map((m) => [Number(m[1]), Number(m[2])] as const)
    .sort((a, b) => b[0] - a[0] || b[1] - a[1])[0];
  return newest ? `v${newest[0]}.${newest[1] + 1}` : "v1.0";
};

export default function Official(props: {
  owner: string;
  name: string;
  books: readonly BookRow[];
  /** The book open in the viewer, to pull by default. */
  openBook: string | undefined;
}) {
  const id = () => ({ path: { owner: props.owner, name: props.name } });
  const [info, { refetch }] = createResource(
    () => me()?.publishing && id(),
    (c) => api.json("/v1/repos/{owner}/{name}/official", c),
  );
  const [busy, setBusy] = createSignal(false);
  const [done, setDone] = createSignal<Done>();
  const [failed, setFailed] = createSignal<string>();
  const [book, setBook] = createSignal<string>();
  const [assembling, setAssembling] = createSignal(false);
  const pickBook = () => book() ?? props.openBook ?? props.books[0]?.book;

  const act = async (what: string, run: () => Promise<Done>) => {
    if (!confirm(what)) return;
    setBusy(true);
    setFailed(undefined);
    setDone(undefined);
    try {
      setDone(await run());
      await refetch();
    } catch (e) {
      setFailed(e instanceof Error ? e.message : String((e as { message?: string }).message ?? e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Show when={info()}>
      {(o) => {
        const t = () => o().target;
        const label = () => (t() ? `${t()!.owner}/${t()!.name}` : "");
        return (
          <section class="official" classList={{ none: !t()?.exists }} aria-label="Official">
            <p class="official-line">
              <Show
                when={!o().is_official}
                fallback={
                  <span>
                    <b>This is an official repo.</b> Its releases are what is published.
                  </span>
                }
              >
                <Show when={t()} fallback={<span class="small">No official repo: {o().reason}</span>}>
                  {(target) => (
                    <span>
                      Official for this language and brand:{" "}
                      <Show when={target().exists} fallback={<b class="mono">{label()}</b>}>
                        <a class="mono" href={target().html_url} target="_blank" rel="noopener">
                          {label()} ↗
                        </a>
                      </Show>
                      <Show when={!target().exists}> <span class="small">(not created yet)</span></Show>
                    </span>
                  )}
                </Show>
              </Show>
              <Show when={t()?.releases.length}>
                <span class="releases">
                  <For each={t()!.releases.slice(0, 5)}>
                    {(r) => (
                      <a class="tag official-tag" href={r.html_url} target="_blank" rel="noopener" title={r.name}>
                        {r.tag_name}
                      </a>
                    )}
                  </For>
                </span>
              </Show>
            </p>

            <Show when={t()}>
              <Show
                when={o().can_publish}
                fallback={
                  <p class="small official-line">
                    <Show when={me()?.me} fallback={<a href={signInHref()} rel="external">Sign in</a>}>
                      {(m) => <span>{m().username} is not a publisher.</span>}
                    </Show>{" "}
                    Publishers can adopt a consolidated repo as the official one, pull books into it, and publish releases.
                  </p>
                }
              >
                <div class="official-actions">
                  <Show when={o().writer_books.length}>
                    <button type="button" class="primary" aria-expanded={assembling()} onClick={() => setAssembling(!assembling())}>
                      Assemble {label()} from {o().writer_books.length} Writer {o().writer_books.length === 1 ? "book" : "books"}…
                    </button>
                  </Show>
                  <Show when={o().can_adopt}>
                    <button
                      type="button"
                      class="primary"
                      disabled={busy()}
                      onClick={() =>
                        act(`Fork ${props.owner}/${props.name} into ${label()}, with its history, as the official repo?`, () =>
                          api.post("/v1/repos/{owner}/{name}/adopt", id(), {} as never),
                        )
                      }
                    >
                      Adopt as {label()}
                    </button>
                  </Show>
                  <Show when={!o().is_official && props.books.length}>
                    <form
                      onSubmit={(e) => {
                        e.preventDefault();
                        const b = pickBook();
                        if (b)
                          void act(
                            `Commit ${b} from ${props.owner}/${props.name} into ${label()}${t()!.exists ? "" : " (creating it)"}?`,
                            () => api.post("/v1/repos/{owner}/{name}/pull", id(), { book: b }),
                          );
                      }}
                    >
                      <select aria-label="Book to pull" value={pickBook()} onChange={(e) => setBook(e.currentTarget.value)}>
                        <For each={props.books}>{(b) => <option value={b.book}>{b.book}</option>}</For>
                      </select>
                      <button type="submit" disabled={busy()}>
                        Pull into {label()}
                      </button>
                    </form>
                  </Show>
                  <Show when={t()!.exists}>
                    <form
                      onSubmit={(e) => {
                        e.preventDefault();
                        const f = new FormData(e.currentTarget);
                        const tag = String(f.get("tag") ?? "").trim();
                        const notes = String(f.get("notes") ?? "");
                        void act(`Publish ${label()} ${tag} at its current head?`, () =>
                          api.post("/v1/repos/{owner}/{name}/releases", id(), { tag, notes }),
                        );
                      }}
                    >
                      <input name="tag" aria-label="Version" value={nextTag(t()!.releases.map((r) => r.tag_name))} />
                      <input name="notes" aria-label="Release notes" placeholder="What changed (optional)" />
                      <button type="submit" class="primary" disabled={busy()}>
                        Publish
                      </button>
                    </form>
                  </Show>
                </div>
              </Show>
            </Show>

            <Show when={assembling() && o().can_publish && o().writer_books.length}>
              <Assemble owner={props.owner} name={props.name} busy={busy()} onCreate={(what, run) => void act(what, run)} />
            </Show>
            <Show when={done()}>
              {(d) => (
                <p class="official-done small">
                  {d().message}{" "}
                  <a href={d().html_url} target="_blank" rel="noopener">
                    See it on Forgejo ↗
                  </a>{" "}
                  The hub lists a new repo after its next poll.
                </p>
              )}
            </Show>
            <Show when={failed()}>{(m) => <div class="failure">{m()}</div>}</Show>
            <Show when={o().actions.length}>
              <ol class="official-log">
                <For each={o().actions.slice(0, 12)}>
                  {(a) => (
                    <li>
                      {ago(a.at_unix)}: <b>{a.username}</b> {a.kind === "adopt" ? "adopted" : a.kind === "consolidate" ? "consolidated Writer projects into" : a.kind === "pull" ? `pulled ${a.book}` : `published ${a.ref}`}{" "}
                      <span class="mono">{a.kind === "publish" || a.kind === "consolidate" ? a.target : `${a.source?.split("@")[0]} → ${a.target}`}</span>
                    </li>
                  )}
                </For>
              </ol>
            </Show>
          </section>
        );
      }}
    </Show>
  );
}
