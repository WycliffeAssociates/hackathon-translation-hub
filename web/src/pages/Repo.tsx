// One repo: the file tree is the navigation; books and proofreading below
// both open places in the viewer. Which file is open, and where, lives in the
// URL (?file=…&at=from-to&ch=…&source=true), so every view is a link.
import { A, useParams, useSearchParams } from "@solidjs/router";
import { For, Show, createMemo, createResource } from "solid-js";
import { api, startPageView } from "../api/client";
import type { Finding } from "../api/types";
import { Async, Counts, FORGEJO, StatusChip, ago, dateOf } from "../ui/bits";
import Books from "./repo/Books";
import FileTree from "./repo/FileTree";
import FileView from "./repo/FileView";
import Proofreading from "./repo/Proofreading";

type Params = { file?: string; source?: string; at?: string; ch?: string };

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
  const [proof] = createResource(id, (c) => api.json("/v1/repos/{owner}/{name}/proof", c));

  const source = () => search.source === "true";
  // With no file picked, a one-book repo (every synthetic repo) opens its book.
  const file = () => search.file ?? (!source() && repo.latest?.books.length === 1 ? repo.latest.books[0]!.path : undefined);
  const focus = () => {
    const m = /^(\d+)-(\d+)$/.exec(search.at ?? "");
    return m ? { from: Number(m[1]), to: Number(m[2]) } : null;
  };

  const show = (p: Params) => {
    setSearch({ file: p.file, source: p.source, at: p.at, ch: p.ch });
    requestAnimationFrame(() => document.getElementById("files")?.scrollIntoView({ block: "start" }));
  };
  const openBook = (path: string, ch?: string) => show({ file: path, ch });
  const openFinding = (f: Finding) => show({ file: f.id, at: `${f.from}-${f.to}` });

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
                  onPick={(file) => setSearch({ file, at: undefined, ch: undefined })}
                />
              )}
            </Async>
            <Show when={file()} keyed fallback={<p class="small viewer">Pick a file in the tree.</p>}>
              {(file) => (
                <FileView
                  owner={r().owner}
                  name={r().name}
                  path={file}
                  source={source()}
                  book={source() ? undefined : r().books.find((b) => b.path === file)}
                  findings={source() ? [] : (proof.latest?.findings.filter((f) => f.id === file) ?? [])}
                  focus={focus()}
                  chapter={search.ch ?? null}
                />
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
            <h2>Proofreading</h2>
            <Show when={!proof.error} fallback={<p class="small">{String((proof.error as { message?: string })?.message ?? "No proofreading run yet.")}</p>}>
              <Async of={proof} what="findings">
                {(p) => <Proofreading proof={p()} summary={r().proof} onOpen={openFinding} />}
              </Async>
            </Show>
          </section>
        </>
      )}
    </Async>
  );
}
