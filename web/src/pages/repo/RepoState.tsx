// What a published repo's page says up front (Will, 2026-10-15), so its
// state is seen without opening Sources or Releases: what changed since the
// newest release (books, chapters, verses), and which books differ from the
// copy they track. Only for the repo that is published for its language and
// resource; every other repo shows nothing here.
import { For, Show, createResource } from "solid-js";
import { api } from "../../api/client";
import type { RepoDetail } from "../../api/types";
import { plural } from "../../ui/bits";

export default function RepoState(props: { repo: RepoDetail; onTab: (tab: "releases" | "sources") => void }) {
  const r = () => props.repo;
  const self = () => `${r().owner}/${r().name}`.toLowerCase();
  const [published] = createResource(
    () => (r().language ? { path: { ietf: r().language! } } : false),
    (c) => api.json("/v1/languages/{ietf}/published", c).catch(() => []),
  );
  const isPublished = () => published()?.some((p) => p.resource === r().resource && p.published?.repo === self()) ?? false;
  const [unreleased] = createResource(
    () => (isPublished() ? { path: { owner: r().owner, name: r().name } } : false),
    (c) => api.json("/v1/repos/{owner}/{name}/unreleased", c).catch(() => null),
  );
  const [sources] = createResource(
    () => (isPublished() ? { path: { ietf: r().language! }, query: { resource: r().resource! } } : false),
    (c) => api.json("/v1/languages/{ietf}/sources", c).catch(() => null),
  );
  const changed = () => {
    const u = unreleased();
    if (!u) return null;
    const verses = u.books.reduce((n, b) => n + b.diff.changed + b.diff.added + b.diff.missing, 0);
    const chapters = u.books.reduce((n, b) => n + Math.abs((b.after?.chapters ?? 0) - (b.before?.chapters ?? 0)), 0);
    return { books: u.books.length, verses, chapters, tag: u.last_tag };
  };
  const differing = () =>
    (sources()?.books ?? [])
      .filter((b) => b.published && (b.published.state === "behind" || b.published.state === "edited"))
      .map((b) => ({ book: b.book, state: b.published!.state, diff: b.published!.vs_linked }));
  const verseText = (d: { changed: number; added: number; missing: number } | null | undefined) =>
    d ? [d.changed && `${d.changed} changed`, d.added && `${d.added} more`, d.missing && `${d.missing} fewer`].filter(Boolean).join(", ") + " verses" : "";

  return (
    <Show when={isPublished()}>
      <div class="repo-state">
        <Show when={changed()}>
          {(c) => (
            <p>
              <Show when={c().books} fallback={<span class="muted">Nothing changed since {c().tag ?? "the start"}.</span>}>
                <Show
                  when={c().tag}
                  fallback={
                    <>
                      <b>Not released yet:</b> {plural(c().books, "book")} · {plural(c().chapters, "chapter")} · {c().verses.toLocaleString()} verses.{" "}
                    </>
                  }
                >
                  <b>Since {c().tag}:</b> {plural(c().books, "book")} changed, {c().verses.toLocaleString()} verses
                  <Show when={c().chapters}>, {plural(c().chapters, "chapter")} more or fewer</Show>.{" "}
                </Show>
                <button type="button" class="linkish" onClick={() => props.onTab("releases")}>
                  See and release
                </button>
              </Show>
            </p>
          )}
        </Show>
        <Show when={differing().length}>
          <p>
            <b>{plural(differing().length, "book")} differ from what they track:</b>{" "}
            <For each={differing()}>
              {(d, i) => (
                <span>
                  {i() ? "; " : ""}
                  <span class="mono">{d.book}</span> <span class="muted">({d.state === "behind" ? "behind" : "edited"}, {verseText(d.diff)})</span>
                </span>
              )}
            </For>
            .{" "}
            <button type="button" class="linkish" onClick={() => props.onTab("sources")}>
              Review in Sources
            </button>
          </p>
        </Show>
      </div>
    </Show>
  );
}
