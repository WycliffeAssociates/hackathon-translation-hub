// The source text beside the translation, in the Checks tab: the same book in
// a gateway source, in a second viewer with no rail, kept on the translation's
// chapter and lighting the verse of the picked finding. The candidates are the
// source the last proofreading run compared with, then the sources this
// language's copies declare (a published repo often names none itself, but
// the Writer projects it tracks do); a picker switches between them.
import { For, Show, createResource, createSignal } from "solid-js";
import { api } from "../../api/client";
import { Async } from "../../ui/bits";
import UsfmViewer from "../../viewer/UsfmViewer";

export default function SourcePane(props: { candidates: readonly string[]; book: string; chapter: string | null; verse: string | null }) {
  const [chosen, setChosen] = createSignal<string>();
  const source = () => chosen() ?? props.candidates[0] ?? "";
  const parts = () => source().split("/");
  const [detail] = createResource(
    () => (parts().length === 2 ? { path: { owner: parts()[0]!, name: parts()[1]! } } : false),
    (c) => api.json("/v1/repos/{owner}/{name}", c),
  );
  const path = () => detail()?.books.find((b) => b.book === props.book)?.path;
  const [text] = createResource(
    () => (path() ? { path: { owner: parts()[0]!, name: parts()[1]!, path: path()! } } : false),
    (c) => api.text("/v1/repos/{owner}/{name}/file/{path}", c),
  );
  // Picking a finding lights its verse here and also scrolls the translation,
  // which then reports the verse's chapter: that report must not pull this
  // pane back to the top of the chapter it is already showing.
  const chapter = () => (props.verse && props.verse.split(":")[0] === props.chapter ? null : props.chapter);
  return (
    <section class="source-pane" aria-label="Source text">
      <div class="source-pane-head small">
        Source{" "}
        <Show when={props.candidates.length > 1} fallback={<span class="mono">{source()}</span>}>
          <select aria-label="Source text" onChange={(e) => setChosen(e.currentTarget.value)}>
            <For each={props.candidates}>
              {(c) => (
                <option value={c} selected={c === source()}>
                  {c}
                </option>
              )}
            </For>
          </select>
        </Show>
      </div>
      <Show when={detail() && !path()}>
        <p class="small muted">{source()} has no {props.book}.</p>
      </Show>
      <Show when={path()}>
        <Async of={text} what="the source text">
          {(t) => (
            <Show when={t()} keyed>
              {(body) => <UsfmViewer text={body} findings={[]} focus={null} chapter={chapter()} verse={props.verse} rail={false} />}
            </Show>
          )}
        </Async>
      </Show>
    </section>
  );
}
