// One repo: the file tree is the navigation; findings and books below both
// open places in the viewer. Which file is open, where, and for which check
// lives in the URL (?file=…&at=from-to&ch=…&source=true&check=sous:Rarity), so
// every view is a link.
import { A, useParams, useSearchParams } from "@solidjs/router";
import { For, Show, createMemo, createResource } from "solid-js";
import { api, startPageView } from "../api/client";
import type { CodeCount, Finding } from "../api/types";
import { Async, Counts, FORGEJO, StatusChip, ago, checkKey, dateOf, parseCheck } from "../ui/bits";
import Books from "./repo/Books";
import FileTree from "./repo/FileTree";
import FileView from "./repo/FileView";
import Findings, { CheckRows } from "./repo/Findings";

type Params = { file?: string; source?: string; at?: string; ch?: string; check?: string };

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

  const source = () => search.source === "true";
  // With no file picked, a one-book repo (every synthetic repo) opens its book.
  const file = () => search.file ?? (!source() && repo.latest?.books.length === 1 ? repo.latest.books[0]!.path : undefined);
  const focus = () => {
    const m = /^(\d+)-(\d+)$/.exec(search.at ?? "");
    return m ? { from: Number(m[1]), to: Number(m[2]) } : null;
  };

  // The check picked in Findings, and the open book's id (`MAT`).
  const check = () => parseCheck(search.check);
  const openBookId = () => (source() ? undefined : repo.latest?.books.find((b) => b.path === file())?.book);
  // A Sous Chef check's stored rows for the open book. Lint rows are not
  // served: the viewer's engine draws lint for the open book itself.
  const [rows] = createResource(
    () => {
      const c = check();
      const book = openBookId();
      return c?.engine === "sous" && book ? { ...id(), query: { book, code: c.code } } : false;
    },
    (c) => api.json("/v1/repos/{owner}/{name}/proof", c),
  );
  const total = () => {
    const c = check();
    return counts.latest?.find((x) => x.engine === c?.engine && x.code === c?.code)?.books.find((b) => b.book === openBookId())?.n;
  };

  const show = (p: Params) => {
    setSearch({ file: p.file, source: p.source, at: p.at, ch: p.ch, check: p.check });
    requestAnimationFrame(() => document.getElementById("files")?.scrollIntoView({ block: "start" }));
  };
  const openBook = (path: string, ch?: string) => show({ file: path, ch });
  const openFinding = (f: Finding) => show({ file: f.id, at: `${f.from}-${f.to}`, check: search.check });
  const openCheck = (c: CodeCount, book: string) => {
    const path = repo.latest?.books.find((b) => b.book === book)?.path;
    if (path) show({ file: path, check: checkKey(c) });
  };

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
            </p>
            <Show when={r().scan_error}><div class="failure">Scan failed: {r().scan_error}</div></Show>
          </header>

          <Show when={r().synthetic}>
            <div class="synthetic-note" role="note">
              <p>
                <b>Not a real repo.</b> This Writer project is shown as if it were consolidated (stitched with Sefer's template); checks and
                verse counts below run against the stitched book.
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
              {(file) => (
                <div class="viewer-col">
                  <FileView
                    owner={r().owner}
                    name={r().name}
                    path={file}
                    source={source()}
                    book={source() ? undefined : r().books.find((b) => b.path === file)}
                    findings={rows.latest?.findings.filter((f) => f.id === file) ?? []}
                    focus={focus()}
                    chapter={search.ch ?? null}
                  />
                  <Show when={check()?.engine === "sous" && openBookId()}>
                    {(book) => (
                      <Async of={rows} what={`${check()!.code} in ${book()}`}>
                        {(p) => <CheckRows code={check()!.code} book={book()} total={total()} rows={p().findings} onOpen={openFinding} />}
                      </Async>
                    )}
                  </Show>
                  <Show when={check()?.engine === "lint" && openBookId()}>
                    <p class="small check-rows">
                      <b class="mono">{check()!.code}</b> is a lint check: the viewer underlines it where it fires.
                    </p>
                  </Show>
                </div>
              )}
            </Show>
          </section>

          <Show when={r().books.length}>
            <section>
              <div class="section-head">
                <h2>Books</h2>
                <Counts errors={r().errors} warnings={r().warnings} />
                <Show when={r().sk_tag}><span class="small">linted with scripture-kitchen {r().sk_tag}</span></Show>
              </div>
              <Books books={r().books} onOpen={openBook} />
            </section>
          </Show>

          <section>
            <div class="section-head">
              <h2>Findings</h2>
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
          </section>
        </>
      )}
    </Async>
  );
}
