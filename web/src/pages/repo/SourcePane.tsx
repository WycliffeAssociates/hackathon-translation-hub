// The source text beside the translation, in the Checks tab: the book the last
// proofreading run compared this one with (the server says which, through the
// compare route), in a second viewer with no rail, kept on the same chapter
// as the translation. Two panes rather than a card per verse, so a reviewer
// reads both texts the way a checker does.
import { Show, createResource } from "solid-js";
import { api } from "../../api/client";
import { Async } from "../../ui/bits";
import UsfmViewer from "../../viewer/UsfmViewer";
import type { SourceRef } from "../../viewer/SourceCompare";

export default function SourcePane(props: { of: SourceRef; chapter: string | null; verse: string | null }) {
  const [where] = createResource(
    () => ({ path: { owner: props.of.owner, name: props.of.name, book: props.of.book }, query: { chapter: 1, verse: 1, count: 1, text: false } }),
    (c) => api.json("/v1/repos/{owner}/{name}/books/{book}/compare", c),
  );
  const repo = () => where()?.source.split("@")[0]?.split("/") ?? [];
  const [text] = createResource(
    () => where() && repo().length === 2 && { path: { owner: repo()[0]!, name: repo()[1]!, path: where()!.source_path } },
    (c) => api.text("/v1/repos/{owner}/{name}/file/{path}", c),
  );
  // Picking a finding lights its verse here and also scrolls the translation,
  // which then reports the verse's chapter: that report must not pull this
  // pane back to the top of the chapter it is already showing.
  const chapter = () => (props.verse && props.verse.split(":")[0] === props.chapter ? null : props.chapter);
  return (
    <section class="source-pane" aria-label="Source text">
      <h3 class="small">
        Source: <span class="mono">{repo().join("/")}</span>
      </h3>
      <Async of={text} what="the source text">
        {(t) => (
          <Show when={t()} keyed>
            {(body) => <UsfmViewer text={body} findings={[]} focus={null} chapter={chapter()} verse={props.verse} rail={false} />}
          </Show>
        )}
      </Async>
    </section>
  );
}
