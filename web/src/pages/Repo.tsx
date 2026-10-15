// One repo, in tabs like a repo on GitHub: Files (the tree and the viewer),
// Checks (every check that fired, the book's text with the findings beside it,
// and the source text in a second pane), Sources (every copy of each book in
// this language and brand), Releases (the official side: what is published,
// what changed since, a new release with its bulletin) and History (commits).
// Which tab, file, place and check are open lives in the URL
// (?tab=checks&file=…&at=from-to&ch=…&check=sous:Rarity), so every view is a link.
import { A, useParams, useSearchParams } from "@solidjs/router";
import { For, Match, Show, Switch, createMemo, createResource, createSignal } from "solid-js";
import { api, startPageView } from "../api/client";
import type { CodeCount } from "../api/types";
import { Async, Counts, FORGEJO, StatusChip, seferLink, ago, checkKey, dateOf } from "../ui/bits";
import Books from "./repo/Books";
import FileTree from "./repo/FileTree";
import FileView from "./repo/FileView";
import Findings from "./repo/Findings";
import History from "./repo/History";
import Official from "./repo/Official";
import Releases from "./repo/Releases";
import SourcePane from "./repo/SourcePane";
import { SourcesView } from "./Sources";
import type { RepoDetail } from "../api/types";
import { me } from "../ui/session";

type Params = { file?: string; source?: string; at?: string; ch?: string; v?: string; check?: string; tab?: string };

type Tab = "files" | "checks" | "sources" | "releases" | "history";

export default function Repo() {
  const params = useParams<{ owner: string; name: string }>();
  const [search, setSearch] = useSearchParams<Params>();
  // A new repo is a new page view (the router reuses this component).
  const id = createMemo(() => {
    startPageView("repo");
    return { path: { owner: params.owner, name: params.name } };
  });

  const [repo] = createResource(id, (c) => api.json("/v1/repos/{owner}/{name}", c));
  const [tree] = createResource(id, (c) => api.json("/v1/repos/{owner}/{name}/tree", c));
  const [counts] = createResource(id, (c) => api.json("/v1/repos/{owner}/{name}/findings", c));

  // A failed resource rethrows from `.latest`; read these only through `ok`, so a
  // 404 shows as a failure instead of breaking the page in "Loading…".
  const ok = <T,>(r: { error: unknown; latest: T | undefined }) => (r.error ? undefined : r.latest);
  const source = () => search.source === "true";
  // With no file picked, a one-book repo (every synthetic repo) opens its book.
  const file = () => search.file ?? (!source() && ok(repo)?.books.length === 1 ? ok(repo)!.books[0]!.path : undefined);
  const focus = () => {
    const m = /^(\d+)-(\d+)$/.exec(search.at ?? "");
    return m ? { from: Number(m[1]), to: Number(m[2]) } : null;
  };

  // The open book's id (`MAT`).
  const openBookId = () => (source() ? undefined : ok(repo)?.books.find((b) => b.path === file())?.book);
  // Sous Chef's stored rows for the open book, every check. Lint rows are not
  // served: the viewer's engine draws lint for the open book itself.
  const [rows] = createResource(
    () => {
      const book = openBookId();
      return book ? { ...id(), query: { book } } : false;
    },
    (c) => api.json("/v1/repos/{owner}/{name}/proof", c),
  );

  const show = (p: Params) => {
    setSearch({ file: p.file, source: p.source, at: p.at, ch: p.ch, check: p.check });
    requestAnimationFrame(() => document.querySelector(".checks-view, #files")?.scrollIntoView({ block: "start" }));
  };
  const openBook = (path: string, ch?: string) => show({ file: path, ch });
  const openCheck = (c: Pick<CodeCount, "engine" | "code">, book: string) => {
    const path = ok(repo)?.books.find((b) => b.book === book)?.path;
    if (path) show({ file: path, check: checkKey(c) });
  };

  // Tabs, like a repo on GitHub: the tab is in the URL (?tab=checks), Files by default.
  const tab = (): Tab => (["checks", "sources", "releases", "history"].includes(search.tab ?? "") ? (search.tab as Tab) : "files");
  const tabHref = (t: Tab) => {
    const q = new URLSearchParams(location.search);
    if (t === "files") q.delete("tab");
    else q.set("tab", t);
    return `${location.pathname}${q.size ? `?${q}` : ""}`;
  };
  const tabs = (r: RepoDetail) =>
    [
      { id: "files" as Tab, label: "Files", count: 0 },
      { id: "checks" as Tab, label: "Checks", count: r.proof?.findings ?? 0 },
      ...(r.language && r.resource ? [{ id: "sources" as Tab, label: "Sources", count: 0 }] : []),
      { id: "releases" as Tab, label: "Releases", count: official()?.target?.releases.length ?? 0 },
      { id: "history" as Tab, label: "History", count: 0 },
    ];
  const [official, { refetch: refetchOfficial }] = createResource(
    () => me()?.publishing && id(),
    (c) => api.json("/v1/repos/{owner}/{name}/official", c).catch(() => undefined),
  );

  // Checks: the open book, else the first book with findings, else the first.
  const checkFile = (r: RepoDetail) =>
    file() ?? r.books.find((b) => (ok(counts) ?? []).some((c) => c.books.some((x) => x.book === b.book)))?.path ?? r.books[0]?.path;
  const [sideBySide, setSideBySide] = createSignal(false);
  const [following, setFollowing] = createSignal<string | null>(null);
  /** The verse of the finding picked last (`3:15`), lit in the source pane too. */
  const [pickedVerse, setPickedVerse] = createSignal<string | null>(null);
  /** The source this book was compared with, when the last run recorded one. */
  const compareWith = (r: RepoDetail, path: string) => {
    const corpus = ok(rows)?.corpus;
    const book = corpus?.books.find((b) => b.id === path);
    return corpus?.source && book?.source ? { owner: r.owner, name: r.name, book: book.book } : null;
  };
  const view = (r: RepoDetail, path: string) => (
    <FileView
      owner={r.owner}
      name={r.name}
      path={path}
      source={source()}
      book={source() ? undefined : r.books.find((b) => b.path === path)}
      findings={(ok(rows)?.findings ?? [])
        .filter((f) => f.id === path)
        // Hygiene is "the text should never hold this"; convention is "rare here".
        .map((f) => ({ ...f, severity: f.rule === "hygiene" ? ("warning" as const) : ("info" as const) }))}
      focus={focus()}
      chapter={search.ch ?? null}
      verse={search.v ?? null}
      counts={ok(counts) ?? []}
      check={search.check ?? null}
      onBook={(engine, code, book) => openCheck({ engine, code }, book)}
      compareWith={compareWith(r, path)}
      onChapter={setFollowing}
      onPicked={(ref) => setPickedVerse(/^\d+:\d+/.exec(ref)?.[0] ?? null)}
    />
  );

  return (
    <Async of={repo} what={`${params.owner}/${params.name}`}>
      {(r) => (
        <>
          <nav class="crumbs">
            <A href="/">Repos</A> ›{" "}
            <Show when={r().language}>
              <A href={`/languages/${r().language}`}>{r().language}</A> ›{" "}
            </Show>
            {r().name}
          </nav>
          <header class="page-head">
            <h1>{r().title ?? r().name}</h1>
            <p class="facts">
              <span class="mono">{r().owner}/{r().name}</span>
              <Show when={r().language}>
                <span>language <A href={`/languages/${r().language}`}>{r().language}</A></span>
              </Show>
              <Show when={r().resource}><span>resource <b>{r().resource}</b></span></Show>
              <Show when={r().shape}><span class="tag">{r().shape}</span></Show>
              <Show when={r().head_branch ?? r().default_branch}>{(b) => <span>branch <code>{b()}</code></span>}</Show>
              <Show when={r().head_unix}>
                <span title={dateOf(r().head_unix)}>HEAD {ago(r().head_unix)}</span>
              </Show>
              <Show when={r().catalog.length} fallback={<StatusChip status={null} />}>
                <For each={r().catalog}>{(c) => <StatusChip status={c.status} />}</For>
              </Show>
              <Show when={r().show_on_biel}><span class="tag biel">shown on BIEL</span></Show>
              <a href={`${FORGEJO}/${r().owner}/${r().name}`} target="_blank" rel="noopener">Open in WACS ↗</a>
              <a href={seferLink(r().owner, r().name)} target="_blank" rel="noopener">Open in Sefer ↗</a>
            </p>
            <Show when={r().scan_error}><div class="failure">Scan failed: {r().scan_error}</div></Show>
          </header>

          <nav class="tabs" aria-label="Repo">
            <For each={tabs(r())}>
              {(t) => (
                <a
                  href={tabHref(t.id)}
                  aria-current={tab() === t.id ? "page" : undefined}
                  onClick={(e) => {
                    e.preventDefault();
                    setSearch({ tab: t.id === "files" ? undefined : t.id });
                  }}
                >
                  {t.label}
                  <Show when={t.count}>{(n) => <span class="count"> {n().toLocaleString()}</span>}</Show>
                </a>
              )}
            </For>
          </nav>

          <Switch>
            <Match when={tab() === "files"}>
              <Show when={r().synthetic}>
                <div class="synthetic-note" role="note">
                  <p>
                    <b>Not a real repo.</b> This Writer project is shown as if it were consolidated (stitched with Sefer's template); checks and
                    verse counts run against the stitched book.
                  </p>
                  <label>
                    <input type="checkbox" checked={source()} onChange={(e) => setSearch({ source: e.currentTarget.checked ? "true" : undefined, file: undefined, at: undefined, ch: undefined })} />{" "}
                    Show the original Writer files
                  </label>
                </div>
              </Show>
              <section id="files" class="browser">
                <Async of={tree} what="files">
                  {(t) => (
                    <FileTree
                      entries={source() ? (t().source_files ?? []) : t().files}
                      selected={file() ?? null}
                      onPick={(file) => setSearch({ file, at: undefined, ch: undefined, check: undefined })}
                    />
                  )}
                </Async>
                <Show when={file()} keyed fallback={<p class="small viewer">Pick a file in the tree.</p>}>
                  {(file) => <div class="viewer-col">{view(r(), file)}</div>}
                </Show>
              </section>
              <Show when={r().books.length}>
                <section>
                  <div class="section-head">
                    <h2>Books</h2>
                    <Counts errors={r().errors} warnings={r().warnings} />
                  </div>
                  <Books books={r().books} onOpen={openBook} />
                </section>
              </Show>
            </Match>

            <Match when={tab() === "checks"}>
              <section class="checks-tab">
                <div class="section-head">
                  <Show when={r().proof}>
                    {(s) => (
                      <span class="small">
                        Sous Chef ran {ago(s().ran_unix)} with scripture-kitchen {s().sk_tag}
                        <Show when={!s().current}> · <b>HEAD moved since; a new run is queued</b></Show>
                        <Show when={s().error}> · <span class="bad">{s().error}</span></Show>
                      </span>
                    )}
                  </Show>
                </div>
                <Async of={counts} what="findings">
                  {(list) => <Findings counts={list()} current={search.check ?? null} onBook={openCheck} />}
                </Async>
                <Show when={checkFile(r())} keyed>
                  {(path) => (
                    <div class="checks-view" classList={{ paired: sideBySide() && !!compareWith(r(), path) }}>
                      <div class="checks-bar">
                        <label class="small">
                          Book{" "}
                          <select value={path} onChange={(e) => setSearch({ file: e.currentTarget.value, at: undefined, ch: undefined })}>
                            <For each={r().books}>{(b) => <option value={b.path}>{b.book}</option>}</For>
                          </select>
                        </label>
                        <Show when={compareWith(r(), path)}>
                          <label class="small">
                            <input type="checkbox" checked={sideBySide()} onChange={(e) => setSideBySide(e.currentTarget.checked)} /> Source text beside it
                          </label>
                        </Show>
                      </div>
                      <div class="viewer-col">{view(r(), path)}</div>
                      <Show when={sideBySide() && compareWith(r(), path)}>
                        {(src) => <SourcePane of={src()} chapter={following()} verse={pickedVerse()} />}
                      </Show>
                    </div>
                  )}
                </Show>
              </section>
            </Match>

            <Match when={tab() === "sources" && r().language && r().resource}>
              <SourcesView ietf={r().language!} resource={r().resource!} />
            </Match>

            <Match when={tab() === "releases"}>
              <Official owner={r().owner} name={r().name} language={r().language} resource={r().resource} />
              <Show when={official()?.is_official && official()?.target}>
                {(t) => <Releases owner={r().owner} name={r().name} releases={t().releases} onPublished={() => void refetchOfficial()} />}
              </Show>
            </Match>

            <Match when={tab() === "history"}>
              <History owner={r().owner} name={r().name} />
            </Match>
          </Switch>
        </>
      )}
    </Async>
  );
}
