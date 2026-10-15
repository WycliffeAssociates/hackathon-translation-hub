// Sources by book (a proposal, 2026-10-15): for one language and brand, every
// copy of every book, grouped into versions by their words (the server
// fingerprints each verse), so a publisher decides book by book which copy
// becomes the official text. tStudio and Writer leave the same book in many
// repos; this is where that gets sorted out instead of repo by repo.
//
// Per book: the versions, how each differs from the pick in verses, and the
// official copy's state against the repo it follows (current, behind,
// edited). A publisher can set a copy aside (here, or for every book), take a
// repo for all its books, pull a pick into the official repo, or, before
// there is one, create it from the picks.
import { A, useParams, useSearchParams } from "@solidjs/router";
import { For, Show, createMemo, createResource, createSignal } from "solid-js";
import { api, startPageView } from "../api/client";
import type { Done, SourceBook, SourceCopy, SourceVersion, VerseDiff } from "../api/types";
import { Async, ago, dateOf, plural } from "../ui/bits";
import { me, signInHref } from "../ui/session";

const key = (c: { owner: string; name: string }) => `${c.owner}/${c.name}`;

/** "40 changed · 3 more · 35 fewer", or "same words". */
const diffText = (d: VerseDiff | null | undefined) => {
  if (!d) return "";
  const parts = [
    d.changed && `${d.changed.toLocaleString()} changed`,
    d.added && `${d.added.toLocaleString()} more`,
    d.missing && `${d.missing.toLocaleString()} fewer`,
  ].filter(Boolean);
  return parts.length ? `${parts.join(" · ")} ${parts.length === 1 && !d.changed ? "verses" : ""}`.trim() : "same words";
};

/** The copy a version is taken from: its best member not set aside. */
const lead = (v: SourceVersion) => v.members.find((m) => !m.ignored && !m.official) ?? v.members[0]!;

const STATE: Record<string, string> = {
  current: "current",
  behind: "behind its source",
  edited: "edited in the official repo",
  unlinked: "source not recorded",
};

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
        <A href="/">Repos</A> › <A href={`/languages/${params.ietf}`}>{title()}</A> › sources
      </nav>
      <header class="page-head">
        <h1>
          {title()} · {resource()}
        </h1>
        <p class="lede-quiet">Every copy of every book on WACS, grouped by their words. Decide per book which one is the official text.</p>
      </header>
      <SourcesView ietf={params.ietf} resource={resource()} />
    </>
  );
}

export function SourcesView(props: { ietf: string; resource: string }) {
  const call = () => ({ path: { ietf: props.ietf }, query: { resource: props.resource } });
  const [data, { refetch }] = createResource(call, (c) => api.json("/v1/languages/{ietf}/sources", c));
  /** Book → owner/name, over the server's pick. */
  const [picks, setPicks] = createSignal<Record<string, string>>({});
  const [open, setOpen] = createSignal<ReadonlySet<string>>(new Set());
  const [busy, setBusy] = createSignal(false);
  const [done, setDone] = createSignal<string>();
  const [failed, setFailed] = createSignal<string>();
  const publisher = () => me()?.me?.publisher === true;

  const toggle = (book: string) => {
    const next = new Set(open());
    if (next.has(book)) next.delete(book);
    else next.add(book);
    setOpen(next);
  };

  const defaultPick = (b: SourceBook) => (b.pick == null ? "" : key(lead(b.versions[b.pick]!)));
  const picked = (b: SourceBook) => picks()[b.book] ?? defaultPick(b);
  const pickedVersion = (b: SourceBook) => b.versions.findIndex((v) => v.members.some((m) => key(m) === picked(b)));

  const write = async (what: string | null, run: () => Promise<unknown>, after?: string) => {
    if (what && !confirm(what)) return;
    setBusy(true);
    setFailed(undefined);
    setDone(undefined);
    try {
      const r = await run();
      setDone(after ?? (r as Done | undefined)?.message);
      await refetch();
    } catch (e) {
      setFailed(e instanceof Error ? e.message : String((e as { message?: string }).message ?? e));
    } finally {
      setBusy(false);
    }
  };

  const setAside = (repo: string, book: string, ignore: boolean) =>
    write(null, () => api.post("/v1/languages/{ietf}/sources/ignore", { path: { ietf: props.ietf } }, { resource: props.resource, book, repo, ignore }));

  /** Take one repo for every book it holds. */
  const takeEverywhere = (repo: string) => {
    const books = data()?.books ?? [];
    const next = { ...picks() };
    for (const b of books) if (b.versions.some((v) => v.members.some((m) => key(m) === repo && !m.official))) next[b.book] = repo;
    setPicks(next);
  };

  const pull = (b: SourceBook) => {
    const [owner, name] = picked(b).split("/");
    return write(`Pull ${b.book} from ${picked(b)} into ${data()?.target}, as one commit?`, () =>
      api.post("/v1/repos/{owner}/{name}/pull", { path: { owner: owner!, name: name! } }, { book: b.book }),
    );
  };

  /** No official repo yet: consolidate the Writer picks (history kept), then pull the rest. */
  const create = async () => {
    const d = data();
    if (!d) return;
    const chosen = d.books.filter((b) => picked(b) !== "");
    const copyOf = (b: SourceBook) => b.versions.flatMap((v) => v.members).find((m) => key(m) === picked(b));
    const writers = chosen.filter((b) => copyOf(b)?.writer);
    const others = chosen.filter((b) => !copyOf(b)?.writer);
    const via = writers[0] && copyOf(writers[0]);
    const what = `Create ${d.target} from ${plural(chosen.length, "book")}: ${plural(writers.length, "Writer project")} with every save kept as a commit${
      others.length ? `, then ${plural(others.length, "book")} pulled from consolidated repos` : ""
    }?`;
    if (!confirm(what)) return;
    await write(null, async () => {
      if (via) {
        const all = Object.fromEntries(d.books.map((b) => [b.book, writers.includes(b) ? picked(b) : ""]));
        await api.post("/v1/repos/{owner}/{name}/consolidate", { path: { owner: via.owner, name: via.name } }, { picks: all });
      }
      for (const b of others) {
        const [owner, name] = picked(b).split("/");
        await api.post("/v1/repos/{owner}/{name}/pull", { path: { owner: owner!, name: name! } }, { book: b.book });
      }
    }, `${d.target} created with ${plural(chosen.length, "book")}.`);
  };

  return (
    <Async of={data} what="sources">
      {(d) => {
        const behind = () => d().books.filter((b) => b.official?.state === "behind").length;
        const several = () => d().books.filter((b) => b.versions.filter((v) => !v.ignored).length > 1).length;
        return (
          <div class="sources">
            <p class="sources-summary">
              {plural(d().books.length, "book")} in {d().copies.toLocaleString()} {d().copies === 1 ? "copy" : "copies"}. {several()} {several() === 1 ? "book has" : "books have"} more than one
              version.
              <Show when={d().target}>
                {" "}
                Official: <span class="mono">{d().target}</span>
                <Show when={!d().official_exists}> (not created yet)</Show>
                <Show when={behind()}>; {plural(behind(), "book")} behind the repo it follows</Show>.
              </Show>
            </p>
            <Show when={!publisher() && me()?.publishing}>
              <p class="small">
                <Show when={me()?.me} fallback={<a href={signInHref()} rel="external">Sign in</a>}>
                  {(m) => <>{m().username} is not a publisher.</>}
                </Show>{" "}
                Publishers can set copies aside, pull books and create the official repo.
              </p>
            </Show>
            <Show when={publisher() && !d().official_exists && d().target}>
              <p>
                <button type="button" class="primary" disabled={busy()} onClick={() => void create()}>
                  Create {d().target} from these picks
                </button>
              </p>
            </Show>
            <Show when={done()}>{(m) => <p class="ok" role="status">{m()}</p>}</Show>
            <Show when={failed()}>{(m) => <p class="failure" role="alert">{m()}</p>}</Show>

            <ol class="sources-books">
              <For each={d().books}>
                {(b) => {
                  const live = () => b.versions.filter((v) => !v.ignored);
                  const copies = () => b.versions.reduce((n, v) => n + v.members.length, 0);
                  const pv = () => b.versions[pickedVersion(b)];
                  return (
                    <li class="sources-book" classList={{ open: open().has(b.book) }}>
                      <button type="button" class="sources-row" aria-expanded={open().has(b.book)} onClick={() => toggle(b.book)}>
                        <span class="mono sources-id">{b.book}</span>
                        <span class="sources-pick">
                          <Show when={picked(b)} fallback={<span class="muted">left out</span>}>
                            <RepoName repo={picked(b)} />
                          </Show>
                        </span>
                        <span class="sources-verses num">
                          <Show when={pv()}>
                            {(v) => (
                              <>
                                {v().verses.toLocaleString()}
                                <Show when={b.source_verses}>
                                  <span class="muted"> / {b.source_verses!.toLocaleString()}</span>
                                </Show>
                              </>
                            )}
                          </Show>
                        </span>
                        <span class="sources-count">
                          <Show when={live().length > 1} fallback={<span class="muted">{copies() === 1 ? "1 copy" : `${copies()} copies, same words`}</span>}>
                            {live().length} versions <span class="muted">in {copies()} copies</span>
                          </Show>
                        </span>
                        <span class="sources-state">
                          <Show when={b.official}>
                            {(o) => (
                              <span class={`official-state ${o().state}`}>
                                {STATE[o().state] ?? o().state}
                                <Show when={o().vs_linked}>{(x) => <span class="muted"> · {diffText(x())}</span>}</Show>
                              </span>
                            )}
                          </Show>
                        </span>
                      </button>
                      <Show when={open().has(b.book)}>
                        <div class="sources-versions">
                          <For each={b.versions}>
                            {(v, vi) => (
                              <section class="sources-version" classList={{ set: v.ignored, picked: vi() === pickedVersion(b) }}>
                                <p class="sources-version-head">
                                  <b>{v.verses.toLocaleString()} verses</b>
                                  <span class="muted"> in {plural(v.chapters, "chapter")}</span>
                                  <Show when={v.vs_pick} fallback={<span> · the pick</span>}>
                                    {(x) => <span> · against the pick: {diffText(x())}</span>}
                                  </Show>
                                  <Show when={b.official?.version === vi()}> · <b>in the official repo</b></Show>
                                  <Show when={v.ignored}> · set aside</Show>
                                </p>
                                <ul class="sources-members">
                                  <For each={v.members}>
                                    {(m) => <Member m={m} book={b.book} checked={picked(b) === key(m)} publisher={publisher()} busy={busy()}
                                      onPick={() => setPicks({ ...picks(), [b.book]: key(m) })}
                                      onAside={(all, ignore) => void setAside(key(m), all ? "*" : b.book, ignore)}
                                      onEverywhere={() => takeEverywhere(key(m))} />}
                                  </For>
                                </ul>
                              </section>
                            )}
                          </For>
                          <p class="sources-actions">
                            <label class="small">
                              <input type="radio" name={`pick-${b.book}`} checked={picked(b) === ""} onChange={() => setPicks({ ...picks(), [b.book]: "" })} /> Leave{" "}
                              {b.book} out
                            </label>
                            <Show when={publisher() && d().official_exists && picked(b) && pickedVersion(b) !== b.official?.version}>
                              <button type="button" disabled={busy()} onClick={() => void pull(b)}>
                                Pull {b.book} from {picked(b)}
                              </button>
                            </Show>
                          </p>
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

/**
 * A copy's name the way people say it: the owner, then the repo name in grey.
 * Writer names (`tzh-x-oxchuctzeltal_luk_text_reg`) only repeat the language,
 * book and brand the page already shows, so they are cut to "Writer".
 */
function RepoName(props: { repo: string }) {
  const parts = () => props.repo.split("/");
  const writer = () => /_[a-z0-9]{3}_text_[a-z0-9]+$/i.test(parts()[1] ?? "");
  return (
    <span class="repo-name">
      <span class="repo-owner">{parts()[0]}</span>
      <span class="muted">/{writer() ? "Writer" : parts()[1]}</span>
    </span>
  );
}

function Member(props: {
  m: SourceCopy;
  book: string;
  checked: boolean;
  publisher: boolean;
  busy: boolean;
  onPick: () => void;
  onAside: (everyBook: boolean, ignore: boolean) => void;
  onEverywhere: () => void;
}) {
  const m = () => props.m;
  return (
    <li classList={{ set: m().ignored }}>
      <label class="sources-member">
        <input type="radio" name={`pick-${props.book}`} checked={props.checked} disabled={m().official} onChange={props.onPick} />
        <A href={`/repos/${m().owner}/${m().name}`} title={key(m())}>
          <RepoName repo={key(m())} />
        </A>
      </label>
      <Show when={m().official}><span>official</span></Show>
      <Show when={m().status === "Primary" || m().show_on_biel}>
        <span>{[m().status === "Primary" && "Primary", m().show_on_biel && "on BIEL"].filter(Boolean).join(", ")}</span>
      </Show>
      <span class="muted" title={dateOf(m().head_unix)}>{ago(m().head_unix)}</span>
      <Show when={m().findings}>
        {(n) => (
          <A class="muted" href={`/repos/${m().owner}/${m().name}?file=${encodeURIComponent(m().path)}`}>
            {plural(n(), "finding")}
          </A>
        )}
      </Show>
      <Show when={props.publisher && !m().official}>
        <span class="sources-member-actions">
          <Show
            when={m().ignored}
            fallback={
              <>
                <button type="button" class="linkish small" disabled={props.busy} onClick={() => props.onAside(false, true)}>
                  Set aside
                </button>
                <button type="button" class="linkish small" disabled={props.busy} onClick={() => props.onAside(true, true)}>
                  for every book
                </button>
              </>
            }
          >
            <button type="button" class="linkish small" disabled={props.busy} onClick={() => props.onAside(false, false)}>
              Bring back
            </button>
          </Show>
          <button type="button" class="linkish small" onClick={props.onEverywhere} title="Pick this repo for every book it has">
            Use for all its books
          </button>
        </span>
      </Show>
    </li>
  );
}
