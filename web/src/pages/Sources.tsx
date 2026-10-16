// Sources by book (a proposal, 2026-10-15): for one language and resource,
// what is published and what each published book tracks, then every other
// copy of the book on WACS, grouped into variants by their words (the server
// fingerprints each verse) and compared with the published copy: how many
// verses differ, and a diff to read them. tStudio and Writer leave the same
// book in many repos; this is where a publisher decides, book by book.
//
// Before anything is published, the comparison is with the pick (the copy
// BIEL shows as Primary, else the best), and the picks can become a new
// published repo, or an existing repo can be made the published one as is.
// "Version" is kept for releases (v1.0); a distinct text is a "variant".
import { A, useParams, useSearchParams } from "@solidjs/router";
import { For, Show, createMemo, createResource, createSignal } from "solid-js";
import { api, startPageView } from "../api/client";
import type { Done, SourceBook, SourceCopy, SourceVariant, VerseDiff } from "../api/types";
import { Async, ago, dateOf, plural } from "../ui/bits";
import { resourceTitle } from "../ui/resources";
import { me, signInHref } from "../ui/session";
import SourcesProgress, { readPins } from "./SourcesProgress";
import VerseDiffView, { sourcesDiff } from "./VerseDiffView";

const key = (c: { owner: string; name: string }) => `${c.owner}/${c.name}`;

/** "40 changed · 3 more · 35 fewer", or "same words". */
export const diffText = (d: VerseDiff | null | undefined) => {
  if (!d) return "";
  const parts = [
    d.changed && `${d.changed.toLocaleString()} changed`,
    d.added && `${d.added.toLocaleString()} more`,
    d.missing && `${d.missing.toLocaleString()} fewer`,
  ].filter(Boolean);
  return parts.length ? `${parts.join(" · ")} ${parts.length === 1 && !d.changed ? "verses" : ""}`.trim() : "same words";
};

/** The copy a variant is taken from: its best member not set aside. */
const lead = (v: SourceVariant) => v.members.find((m) => !m.ignored) ?? v.members[0];

const STATE: Record<string, string> = {
  current: "current",
  behind: "behind what it tracks",
  edited: "edited in the published repo",
  unlinked: "source not recorded",
};

/**
 * A copy's name the way people say it: the owner, then the repo name in grey.
 * Writer names (`tzh-x-oxchuctzeltal_luk_text_reg`) only repeat the language,
 * book and resource the page already shows, so they are cut to "Writer".
 */
export function RepoName(props: { repo: string }) {
  const parts = () => props.repo.split("/");
  const writer = () => /_[a-z0-9]{3}_text_[a-z0-9]+$/i.test(parts()[1] ?? "");
  return (
    <span class="repo-name">
      <span class="repo-owner">{parts()[0]}</span>
      <span class="muted">/{writer() ? "Writer" : parts()[1]}</span>
    </span>
  );
}

export default function Sources() {
  const params = useParams<{ ietf: string }>();
  const [search] = useSearchParams<{ resource?: string }>();
  const resource = () => search.resource ?? "reg";
  const [lang] = createResource(
    () => params.ietf,
    (ietf) => api.json("/v1/languages/{ietf}", { path: { ietf } }).catch(() => null),
  );
  createMemo(() => startPageView("sources"));
  const title = () => lang()?.facts?.english_name ?? params.ietf;
  return (
    <>
      <nav class="crumbs">
        <A href="/">Repos</A> › <A href={`/languages/${params.ietf}`}>{title()}</A> › {resourceTitle(resource())}
      </nav>
      <header class="page-head">
        <h1>
          {title()} · {resourceTitle(resource())} <span class="muted">{resource()}</span>
        </h1>
      </header>
      <SourcesView ietf={params.ietf} resource={resource()} />
    </>
  );
}

export function SourcesView(props: { ietf: string; resource: string }) {
  const [pins, setPins] = createSignal(readPins(props.ietf, props.resource));
  const call = () => ({
    path: { ietf: props.ietf },
    query: { resource: props.resource, against: pins().length ? pins().join(",") : undefined },
  });
  const [data, { refetch }] = createResource(call, (c) => api.json("/v1/languages/{ietf}/sources", c));
  const [published, { refetch: refetchPublished }] = createResource(
    () => ({ path: { ietf: props.ietf } }),
    (c) => api.json("/v1/languages/{ietf}/published", c).catch(() => []),
  );
  const mine = () => published()?.find((p) => p.resource === props.resource);
  /** Book → owner/name, over the server's pick (only before anything is published). */
  const [picks, setPicks] = createSignal<Record<string, string>>({});
  const [open, setOpen] = createSignal<ReadonlySet<string>>(new Set());
  /** `BOOK owner/name` of the copy whose diff is open. */
  const [diffOpen, setDiffOpen] = createSignal<string>();
  const [busy, setBusy] = createSignal(false);
  /** What a long write is doing while it runs (Create replays every Writer save: a minute or more). */
  const [working, setWorking] = createSignal<string>();
  const [done, setDone] = createSignal<string>();
  const [failed, setFailed] = createSignal<string>();
  const publisher = () => me()?.me?.publisher === true;

  const toggle = (book: string) => {
    const next = new Set(open());
    if (next.has(book)) next.delete(book);
    else next.add(book);
    setOpen(next);
  };

  const defaultPick = (b: SourceBook) => {
    const v = b.pick == null ? undefined : b.variants[b.pick];
    const l = v && lead(v);
    return l ? key(l) : "";
  };
  const picked = (b: SourceBook) => picks()[b.book] ?? defaultPick(b);

  const write = async (what: string | null, run: () => Promise<unknown>, after?: string) => {
    if (what && !confirm(what)) return;
    setBusy(true);
    setFailed(undefined);
    setDone(undefined);
    try {
      const r = await run();
      await Promise.all([refetch(), refetchPublished()]);
      setDone(after ?? (r as Done | undefined)?.message);
    } catch (e) {
      setFailed(e instanceof Error ? e.message : String((e as { message?: string }).message ?? e));
    } finally {
      setBusy(false);
      setWorking(undefined);
    }
  };

  const setAside = (repo: string, book: string, ignore: boolean) =>
    write(null, () => api.post("/v1/languages/{ietf}/sources/ignore", { path: { ietf: props.ietf } }, { resource: props.resource, book, repo, ignore }));

  /** Take one repo for every book it holds. */
  const takeEverywhere = (repo: string) => {
    const next = { ...picks() };
    for (const b of data()?.books ?? []) if (b.variants.some((v) => v.members.some((m) => key(m) === repo))) next[b.book] = repo;
    setPicks(next);
  };

  /** Pull a book from a copy into the published repo; the published book then tracks that copy. */
  const pull = (book: string, repo: string, why: string) => {
    const [owner, name] = repo.split("/");
    return write(`${why}: pull ${book} from ${repo} into ${data()?.published?.repo}, as one commit?`, () =>
      api.post("/v1/repos/{owner}/{name}/pull", { path: { owner: owner!, name: name! } }, { book }),
    );
  };

  const bless = (repo: string | null) =>
    write(
      repo ? `Make ${repo} the published ${resourceTitle(props.resource)} for ${props.ietf}? Nothing is copied; the hub points at it.` : "Clear the choice of published repo?",
      () => api.post("/v1/languages/{ietf}/published", { path: { ietf: props.ietf } }, { resource: props.resource, repo }),
      repo ? `${repo} is now the published text.` : "Cleared.",
    );

  /** Nothing published yet: consolidate the Writer picks (history kept), then pull the rest. */
  const create = async () => {
    const d = data();
    if (!d) return;
    const chosen = d.books.filter((b) => picked(b) !== "");
    const copyOf = (b: SourceBook) => b.variants.flatMap((v) => v.members).find((m) => key(m) === picked(b));
    const writers = chosen.filter((b) => copyOf(b)?.writer);
    const others = chosen.filter((b) => !copyOf(b)?.writer);
    const via = writers[0] && copyOf(writers[0]);
    const what = `Create ${d.target} from ${plural(chosen.length, "book")}: ${plural(writers.length, "Writer project")} with every save kept as a commit${
      others.length ? `, then ${plural(others.length, "book")} pulled from consolidated repos` : ""
    }?`;
    if (!confirm(what)) return;
    await write(null, async () => {
      setWorking(`Creating ${d.target}: replaying every Writer save of ${plural(writers.length, "book")} into one history, then pushing it. This can take a minute or more; keep this page open.`);
      if (via) {
        const all = Object.fromEntries(d.books.map((b) => [b.book, writers.includes(b) ? picked(b) : ""]));
        await api.post("/v1/repos/{owner}/{name}/consolidate", { path: { owner: via.owner, name: via.name } }, { picks: all });
      }
      for (const [i, b] of others.entries()) {
        setWorking(`Pulling ${b.book} from ${picked(b)} (${i + 1} of ${others.length})…`);
        const [owner, name] = picked(b).split("/");
        await api.post("/v1/repos/{owner}/{name}/pull", { path: { owner: owner!, name: name! } }, { book: b.book });
      }
    }, `${d.target} created with ${plural(chosen.length, "book")}.`);
  };

  return (
    <Async of={data} what="sources">
      {(d) => {
        const pub = () => d().published;
        const behind = () => d().books.filter((b) => b.published?.state === "behind").length;
        const tracked = () => d().books.filter((b) => b.published).length;
        return (
          <div class="sources">
            {/* What is published leads the page. */}
            <section class="published-bar" classList={{ none: !pub() }}>
              <Show
                when={pub()}
                fallback={
                  <>
                    <p>
                      <b>Nothing is published</b> for {props.ietf} {resourceTitle(props.resource)} yet.
                    </p>
                    <Show when={publisher()}>
                      <p class="published-actions">
                        <Show when={d().target}>
                          <button type="button" class="primary" disabled={busy()} onClick={() => void create()}>
                            Create {d().target} from the picks below
                          </button>
                        </Show>
                        <Show when={mine()?.suggest}>
                          {(s) => (
                            <button type="button" disabled={busy()} onClick={() => void bless(s())}>
                              Make {s()} the published one
                            </button>
                          )}
                        </Show>
                      </p>
                    </Show>
                  </>
                }
              >
                {(p) => (
                  <>
                    <p>
                      Published:{" "}
                      <A class="mono" href={`/repos/${p().repo}?tab=releases`}>
                        {p().repo}
                      </A>
                      <Show when={mine()?.last_tag} fallback={<span class="muted"> · no release yet</span>}>
                        {(t) => (
                          <span>
                            {" "}
                            · {t()} <span class="muted">{ago(mine()?.last_tag_unix)}</span>
                          </span>
                        )}
                      </Show>
                      <span class="muted">
                        {" "}
                        · {plural(tracked(), "book")}
                        <Show when={behind()}>, {behind()} behind what {behind() === 1 ? "it tracks" : "they track"}</Show>
                        <Show when={p().chosen}> · chosen by a publisher</Show>
                      </span>
                    </p>
                    <Show when={!p().writable}>
                      <p class="small muted">
                        It is not in the official org, so the hub reads it but does not pull into it or release from it.
                        <Show when={publisher() && p().chosen}>
                          {" "}
                          <button type="button" class="linkish small" disabled={busy()} onClick={() => void bless(null)}>
                            Clear this choice
                          </button>
                        </Show>
                      </p>
                    </Show>
                  </>
                )}
              </Show>
            </section>

            <SourcesProgress ietf={props.ietf} resource={props.resource} progress={d().progress} onAgainst={setPins} />
            <Show when={!publisher() && me()?.publishing}>
              <p class="small">
                <Show when={me()?.me} fallback={<a href={signInHref()} rel="external">Sign in</a>}>
                  {(m) => <>{m().username} is not a publisher.</>}
                </Show>{" "}
                Publishers can set copies aside, pull books and choose what is published.
              </p>
            </Show>
            <Show when={working()}>{(m) => <p class="loading working" role="status">{m()}</p>}</Show>
            <Show when={done()}>{(m) => <p class="ok" role="status">{m()}</p>}</Show>
            <Show when={failed()}>{(m) => <p class="failure" role="alert">{m()}</p>}</Show>

            <p class="sources-summary small muted">
              {plural(d().books.length, "book")} in {d().copies.toLocaleString()} {d().copies === 1 ? "copy" : "copies"} on WACS.
            </p>
            <ol class="sources-books">
              <For each={d().books}>
                {(b) => {
                  const others = () => b.variants.filter((v) => v.members.length > 0);
                  const differing = () => others().filter((v) => v.vs_reference && !v.ignored);
                  const tracks = () => b.published?.linked?.repo;
                  return (
                    <li class="sources-book" classList={{ open: open().has(b.book) }} data-tour={`book-${b.book}`}>
                      <button type="button" class="sources-row" aria-expanded={open().has(b.book)} onClick={() => toggle(b.book)}>
                        <span class="mono sources-id">{b.book}</span>
                        <span class="sources-pick">
                          <Show
                            when={b.published}
                            fallback={
                              <Show when={picked(b)} fallback={<span class="muted">left out</span>}>
                                <span class="muted">pick: </span>
                                <RepoName repo={picked(b)} />
                              </Show>
                            }
                          >
                            <Show when={tracks()} fallback={<span class="muted">{d().published?.writable ? "published, source not recorded" : "published as is"}</span>}>
                              {(r) => (
                                <>
                                  <span class="muted">tracks </span>
                                  <RepoName repo={r()} />
                                </>
                              )}
                            </Show>
                          </Show>
                        </span>
                        <span class="sources-verses num">
                          {(b.published?.verses ?? b.variants[b.reference ?? 0]?.verses ?? 0).toLocaleString()}
                          <Show when={b.source_verses}>
                            <span class="muted"> / {b.source_verses!.toLocaleString()}</span>
                          </Show>
                        </span>
                        <span class="sources-state">
                          <Show when={b.published} fallback={<span class="muted">not published</span>}>
                            {(p) => (
                              // A repo published as is follows nothing; only the hub's own say "not recorded".
                              <Show when={p().state !== "unlinked" || d().published?.writable}>
                              <span class={`official-state ${p().state}`}>
                                {STATE[p().state] ?? p().state}
                                <Show when={p().vs_linked}>{(x) => <span class="muted"> · {diffText(x())}</span>}</Show>
                              </span>
                              </Show>
                            )}
                          </Show>
                        </span>
                        <span class="sources-count muted">{differing().length ? plural(differing().length, "other variant") : ""}</span>
                      </button>
                      <Show when={open().has(b.book)}>
                        <div class="sources-detail">
                          <Show when={b.published}>
                            {(p) => (
                              <div class="published-book">
                                <p>
                                  <b>Published</b> · {plural(p().chapters, "chapter")} · {plural(p().verses, "verse")}
                                  <span class="muted" title={dateOf(p().head_unix)}>
                                    {" "}
                                    · repo updated {ago(p().head_unix)}
                                  </span>
                                </p>
                                <Show when={p().linked}>
                                  {(l) => (
                                    <p class="small">
                                      Tracks <RepoName repo={l().repo} /> <span class="muted">· taken {l().taken_at}</span> ·{" "}
                                      <span class={`official-state ${p().state}`}>{STATE[p().state]}</span>
                                      <Show when={p().vs_linked}>
                                        {(x) => (
                                          <>
                                            <span class="muted"> · {diffText(x())}</span>{" "}
                                            <button
                                              type="button"
                                              class="linkish small"
                                              onClick={() => setDiffOpen(diffOpen() === `${b.book} ${l().repo}` ? undefined : `${b.book} ${l().repo}`)}
                                            >
                                              See the changes
                                            </button>
                                            {/* Behind: the tracked copy moved on. Edited: the published copy
                                                changed (or was built short); pulling replaces it, as one commit. */}
                                            <Show when={publisher() && d().published?.writable && (p().state === "behind" || p().state === "edited")}>
                                              {" "}
                                              <button
                                                type="button"
                                                class="small"
                                                disabled={busy()}
                                                onClick={() => void pull(b.book, l().repo, p().state === "edited" ? "Replace the published copy with the tracked one" : "Update")}
                                              >
                                                {p().state === "edited" ? "Pull the tracked copy" : "Pull the update"}
                                              </button>
                                            </Show>
                                          </>
                                        )}
                                      </Show>
                                    </p>
                                  )}
                                </Show>
                                <Show when={p().linked && diffOpen() === `${b.book} ${p().linked!.repo}`}>
                                  <VerseDiffView load={sourcesDiff(props.ietf, props.resource, b.book, d().published!.repo, p().linked!.repo)} />
                                </Show>
                              </div>
                            )}
                          </Show>

                          <Show when={others().length} fallback={<p class="small muted">No other copy of {b.book} on WACS.</p>}>
                            <h4 class="sources-sub">{b.published ? "Other copies on WACS" : "Copies on WACS"}</h4>
                            <For each={others()}>
                              {(v) => {
                                const first = () => lead(v);
                                const id = () => `${b.book} ${first() ? key(first()!) : ""}`;
                                const reference = () => {
                                  if (d().published) return d().published!.repo;
                                  const r = b.reference != null ? b.variants[b.reference] : undefined;
                                  const l = r && lead(r);
                                  return l ? key(l) : "";
                                };
                                return (
                                  <section class="sources-variant" classList={{ set: v.ignored }}>
                                    <p class="sources-variant-head">
                                      <span>
                                        {plural(v.verses, "verse")} <span class="muted">· {plural(v.chapters, "chapter")}</span>
                                      </span>
                                      <span>
                                        <Show when={v.vs_reference} fallback={<span class="muted">{v.published ? "same words as published" : "the pick"}</span>}>
                                          {(x) => (
                                            <>
                                              <span>
                                                {b.published ? "against published" : "against the pick"}: {diffText(x())}
                                              </span>{" "}
                                              <button type="button" class="linkish small" onClick={() => setDiffOpen(diffOpen() === id() ? undefined : id())}>
                                                {diffOpen() === id() ? "Hide the diff" : "See the diff"}
                                              </button>
                                            </>
                                          )}
                                        </Show>
                                      </span>
                                    </p>
                                    <Show when={diffOpen() === id() && first() && reference()}>
                                      <VerseDiffView load={sourcesDiff(props.ietf, props.resource, b.book, reference(), key(first()!))} />
                                    </Show>
                                    <ul class="sources-members">
                                      <For each={v.members}>
                                        {(m) => (
                                          <Member
                                            m={m}
                                            book={b.book}
                                            publisher={publisher()}
                                            busy={busy()}
                                            choosing={!b.published}
                                            checked={!b.published && picked(b) === key(m)}
                                            tracked={tracks() === key(m).toLowerCase()}
                                            canTrack={publisher() && !!d().published?.writable && !!b.published && tracks() !== key(m).toLowerCase()}
                                            onPick={() => setPicks({ ...picks(), [b.book]: key(m) })}
                                            onTrack={() => void pull(b.book, key(m), "Track this copy")}
                                            onAside={(all, ignore) => void setAside(key(m), all ? "*" : b.book, ignore)}
                                            onEverywhere={() => takeEverywhere(key(m))}
                                          />
                                        )}
                                      </For>
                                    </ul>
                                  </section>
                                );
                              }}
                            </For>
                          </Show>
                          <Show when={!b.published}>
                            <p class="sources-actions">
                              <label class="small">
                                <input type="radio" name={`pick-${b.book}`} checked={picked(b) === ""} onChange={() => setPicks({ ...picks(), [b.book]: "" })} /> Leave{" "}
                                {b.book} out
                              </label>
                            </p>
                          </Show>
                        </div>
                      </Show>
                    </li>
                  );
                }}
              </For>
            </ol>
          </div>
        );
      }}
    </Async>
  );
}

function Member(props: {
  m: SourceCopy;
  book: string;
  publisher: boolean;
  busy: boolean;
  /** Nothing published: the radio picks the copy to create from. */
  choosing: boolean;
  checked: boolean;
  /** The published book tracks this copy. */
  tracked: boolean;
  canTrack: boolean;
  onPick: () => void;
  onTrack: () => void;
  onAside: (everyBook: boolean, ignore: boolean) => void;
  onEverywhere: () => void;
}) {
  const m = () => props.m;
  return (
    <li class="sources-member" classList={{ set: m().ignored }}>
      <span class="sources-member-name">
        <Show when={props.choosing}>
          <input type="radio" name={`pick-${props.book}`} checked={props.checked} onChange={props.onPick} aria-label={`Pick ${key(m())}`} />
        </Show>
        <A href={`/repos/${m().owner}/${m().name}`} title={key(m())}>
          <RepoName repo={key(m())} />
        </A>
        <Show when={props.tracked}>
          <span class="tracked">tracked</span>
        </Show>
      </span>
      <span class="muted">{[m().status === "Primary" && "Primary", m().show_on_biel && "on BIEL"].filter(Boolean).join(", ")}</span>
      <span class="muted" title={dateOf(m().head_unix)}>
        {ago(m().head_unix)}
      </span>
      <span>
        <Show when={m().findings}>
          {(n) => (
            <A class="muted" href={`/repos/${m().owner}/${m().name}?tab=checks&file=${encodeURIComponent(m().path)}`}>
              {plural(n(), "finding")}
            </A>
          )}
        </Show>
      </span>
      <span class="sources-member-actions">
        <Show when={props.canTrack}>
          <button type="button" class="linkish small" disabled={props.busy} onClick={props.onTrack}>
            Track this
          </button>
        </Show>
        <Show when={props.publisher}>
          <details class="more">
            <summary aria-label="More">⋯</summary>
            <div class="more-menu">
              <Show
                when={m().ignored}
                fallback={
                  <>
                    <button type="button" disabled={props.busy} onClick={() => props.onAside(false, true)}>
                      Set aside for {props.book}
                    </button>
                    <button type="button" disabled={props.busy} onClick={() => props.onAside(true, true)}>
                      Set aside for every book
                    </button>
                  </>
                }
              >
                <button type="button" disabled={props.busy} onClick={() => props.onAside(false, false)}>
                  Bring back
                </button>
              </Show>
              <Show when={props.choosing}>
                <button type="button" onClick={props.onEverywhere}>
                  Pick this repo for every book it has
                </button>
              </Show>
            </div>
          </details>
        </Show>
      </span>
    </li>
  );
}
