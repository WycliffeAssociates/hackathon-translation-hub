// One file from the tree: USFM in the viewer, text in a <pre>, audio and
// images by URL, anything else as a download link.
import { Match, Show, Switch, createResource, lazy } from "solid-js";
import { api, urlOf } from "../../api/client";
import type { BookRow, CodeCount } from "../../api/types";
import { Async, Counts, Sparkline } from "../../ui/bits";
import type { SourceRef } from "../../viewer/SourceCompare";
import type { Range, ViewerFinding } from "../../viewer/UsfmViewer";

const UsfmViewer = lazy(() => import("../../viewer/UsfmViewer"));

const ext = (path: string) => path.slice(path.lastIndexOf(".") + 1).toLowerCase();
const USFM = new Set(["usfm", "sfm"]);
const TEXT = new Set(["md", "yaml", "yml", "json", "txt", "tsv", "csv", "toml", "xml", "html"]);
const AUDIO = new Set(["mp3", "wav", "ogg", "opus", "m4a"]);
const IMAGE = new Set(["png", "jpg", "jpeg", "svg", "gif", "webp"]);

export default function FileView(props: {
  owner: string;
  name: string;
  path: string;
  source: boolean;
  book: BookRow | undefined;
  findings: readonly ViewerFinding[];
  focus: Range | null;
  chapter: string | null;
  counts?: readonly CodeCount[];
  check?: string | null;
  onBook?: (engine: string, code: string, book: string) => void;
  /** The source text this book was compared with (not `source`, the Writer-files switch). */
  compareWith?: SourceRef | null;
}) {
  const call = () => ({ path: { owner: props.owner, name: props.name, path: props.path }, query: { source: props.source || undefined } });
  const url = () => urlOf("/v1/repos/{owner}/{name}/file/{path}", call());
  const kind = () => ext(props.path);
  const isText = () => USFM.has(kind()) || TEXT.has(kind());
  const [text] = createResource(
    () => isText() && call(),
    (c) => api.text("/v1/repos/{owner}/{name}/file/{path}", c),
  );

  return (
    <section class="viewer" aria-label="File">
      <div class="viewer-head">
        <h3 class="mono">{props.path}</h3>
        <Show when={props.book}>{(b) => <BookFacts owner={props.owner} name={props.name} book={b()} />}</Show>
        <a class="small" href={url()} target="_blank" rel="noopener">raw ↗</a>
      </div>
      <Switch fallback={<p class="small">Not shown here (binary). <a href={url()} download="">Download</a></p>}>
        <Match when={isText()}>
          <Async of={text} what={props.path}>
            {(t) => (
              <Show when={USFM.has(kind())} fallback={<pre class="code">{t()}</pre>}>
                {/* Keyed on the text: a new file is a new editor. */}
                <Show when={t()} keyed>
                  {(body) => (
                    <UsfmViewer
                      text={body}
                      findings={props.findings}
                      focus={props.focus}
                      chapter={props.chapter}
                      book={props.book}
                      counts={props.counts}
                      check={props.check}
                      onBook={props.onBook}
                      source={props.compareWith}
                    />
                  )}
                </Show>
              </Show>
            )}
          </Async>
        </Match>
        <Match when={AUDIO.has(kind())}>
          <audio controls preload="metadata" src={url()} />
        </Match>
        <Match when={IMAGE.has(kind())}>
          <img class="file-img" src={url()} alt={props.path} />
        </Match>
      </Switch>
    </section>
  );
}

/** Lint for the open book, and its error count over time when there is more than one point. */
function BookFacts(props: { owner: string; name: string; book: BookRow }) {
  const [history] = createResource(
    () => ({ path: { owner: props.owner, name: props.name, book: props.book.book } }),
    (c) => api.json("/v1/repos/{owner}/{name}/books/{book}/history", c),
  );
  return (
    <span class="book-facts small">
      <Show when={props.book.lint}>{(l) => <Counts errors={l().errors} warnings={l().warnings} infos={l().infos} />}</Show>
      <Show when={history()?.toSorted((a, b) => a.at_unix - b.at_unix)}>
        {(h) => (
          <Sparkline
            values={h().map((p) => p.errors)}
            title={`Errors over ${h().length} scans: ${h().map((p) => p.errors).join(", ")}`}
          />
        )}
      </Show>
    </span>
  );
}
