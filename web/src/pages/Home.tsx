import { A, useSearchParams } from "@solidjs/router";
import { For, Show, createResource } from "solid-js";
import { api, startPageView } from "../api/client";
import type { RepoSummary } from "../api/types";
import { Async, Counts, Pills, RepoLink, StatusChip, ago } from "../ui/bits";

const LIMIT = 50;
type Params = { q?: string; subject?: string; shape?: string; synthetic?: string; topic?: string; offset?: string };

// The home feed: every repo, newest HEAD first, with a few filters.
export default function Home() {
  startPageView("home");
  const [params, setParams] = useSearchParams<Params>();
  const offset = () => Number(params.offset ?? 0) || 0;
  const query = () => ({
    q: params.q,
    subject: params.subject,
    shape: params.shape,
    synthetic: params.synthetic === "true" ? true : undefined,
    topic: params.topic,
    limit: LIMIT,
    offset: offset(),
  });
  const [page] = createResource(query, (q) => api.json("/v1/repos", { query: q }));
  // Any filter change goes back to the first page.
  const set = (p: Params) => setParams({ ...p, offset: undefined });

  return (
    <>
      <div class="toolbar">
        <h1>Recently updated</h1>
        <form
          class="search-inline"
          onSubmit={(e) => {
            e.preventDefault();
            set({ q: String(new FormData(e.currentTarget).get("q") ?? "") || undefined });
          }}
        >
          <input name="q" type="search" placeholder="Filter repos: owner/name, title, language" value={params.q ?? ""} aria-label="Filter repos" />
        </form>
      </div>
      <div class="facets">
        <Pills label="Subject" value={params.subject ?? ""} onPick={(v) => set({ subject: v || undefined })} options={[["", "any"], ["Bible", "Bible"], ["Open Bible Stories", "OBS"], ["Translation Notes", "Notes"]]} />
        <Pills label="Shape" value={params.shape ?? ""} onPick={(v) => set({ shape: v || undefined })} options={[["", "any"], ["rc", "RC"], ["ts", "tS"], ["burrito", "Burrito"], ["usfm", "USFM"], ["other", "other"]]} />
        <Pills label="Kind" value={params.synthetic === "true" ? "syn" : params.topic === "consolidated" ? "con" : ""}
          onPick={(v) => set({ synthetic: v === "syn" ? "true" : undefined, topic: v === "con" ? "consolidated" : undefined })}
          options={[["", "all"], ["syn", "synthetic only"], ["con", "topic: consolidated"]]} />
      </div>
      <Async of={page} what="repos">
        {(p) => (
          <>
            <p class="small" aria-live="polite">
              {p().total ? `${offset() + 1}–${offset() + p().repos.length} of ${p().total.toLocaleString()}` : "Nothing matches."}
              <Show when={page.loading}> · loading…</Show>
            </p>
            <ul class="feed">
              <For each={p().repos}>{(r) => <RepoRow repo={r} />}</For>
            </ul>
            <p class="pager">
              <button disabled={offset() === 0} onClick={() => setParams({ offset: String(Math.max(0, offset() - LIMIT)) || undefined })}>← Newer</button>
              <button disabled={offset() + LIMIT >= p().total} onClick={() => setParams({ offset: String(offset() + LIMIT) })}>Older →</button>
            </p>
          </>
        )}
      </Async>
    </>
  );
}

export function RepoRow(props: { repo: RepoSummary }) {
  const r = () => props.repo;
  return (
    <li class="feed-row">
      <div class="feed-body">
        <div class="feed-line">
          <RepoLink owner={r().owner} name={r().name} />
          <Show when={r().title}>
            <span class="native">{r().title}</span>
          </Show>
        </div>
        <div class="feed-line small">
          <Show when={r().language}>
            <A href={`/languages/${r().language}`}>{r().language}</A>
          </Show>
          <Show when={r().resource}><span class="tag">{r().resource}</span></Show>
          <Show when={r().shape}><span class="tag">{r().shape}</span></Show>
          <Show when={r().is_synthetic}><span class="tag syn">synthetic</span></Show>
          <Show when={r().status}><StatusChip status={r().status} /></Show>
          <Show when={r().show_on_biel}><span class="tag biel">on BIEL</span></Show>
          <Show when={r().book_count}>
            <span>{r().book_count} book{r().book_count === 1 ? "" : "s"}</span>
            <Counts errors={r().errors} warnings={r().warnings} />
          </Show>
        </div>
      </div>
      <span class="small when">{ago(r().head_unix)}</span>
    </li>
  );
}
