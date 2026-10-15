// Two copies of a book, verse by verse: only the verses whose words differ,
// the left copy (usually the published one) beside the other, with the words
// that differ marked. The server pairs the verses (`/sources/diff`); the word
// marks are drawn here, a longest-common-subsequence over each verse's words.
import { For, Show, createResource, createSignal } from "solid-js";
import { api } from "../api/client";
import { Async, plural } from "../ui/bits";

type Mark = { word: string; changed: boolean };

/** Each side's words, marked where they are not in the other side's common run. */
export function markWords(a: string, b: string): [Mark[], Mark[]] {
  const x = a.split(/\s+/).filter(Boolean);
  const y = b.split(/\s+/).filter(Boolean);
  // Long verses are rare; past this size, mark the whole verse rather than spend the time.
  if (x.length * y.length > 250_000) return [x.map((word) => ({ word, changed: true })), y.map((word) => ({ word, changed: true }))];
  const n = x.length;
  const m = y.length;
  const lcs: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) lcs[i]![j] = x[i] === y[j] ? lcs[i + 1]![j + 1]! + 1 : Math.max(lcs[i + 1]![j]!, lcs[i]![j + 1]!);
  const left: Mark[] = [];
  const right: Mark[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (x[i] === y[j]) {
      left.push({ word: x[i]!, changed: false });
      right.push({ word: y[j]!, changed: false });
      i++;
      j++;
    } else if (lcs[i + 1]![j]! >= lcs[i]![j + 1]!) left.push({ word: x[i++]!, changed: true });
    else right.push({ word: y[j++]!, changed: true });
  }
  while (i < n) left.push({ word: x[i++]!, changed: true });
  while (j < m) right.push({ word: y[j++]!, changed: true });
  return [left, right];
}

function Words(props: { marks: Mark[] }) {
  return (
    <For each={props.marks}>
      {(w) => (
        <>
          <Show when={w.changed} fallback={w.word}>
            <mark>{w.word}</mark>
          </Show>{" "}
        </>
      )}
    </For>
  );
}

const SHOWN = 40;

export default function VerseDiffView(props: { ietf: string; resource: string; book: string; a: string; b: string }) {
  const [diff] = createResource(
    () => ({ path: { ietf: props.ietf }, query: { resource: props.resource, book: props.book, a: props.a, b: props.b } }),
    (c) => api.json("/v1/languages/{ietf}/sources/diff", c),
  );
  const [all, setAll] = createSignal(false);
  return (
    <div class="verse-diff">
      <Async of={diff} what="the diff">
        {(d) => (
          <>
            <p class="small muted">
              {plural(d().verses.length, "verse")} differ, {d().same.toLocaleString()} the same.
            </p>
            <table>
              <thead>
                <tr>
                  <th scope="col" class="num" />
                  <th scope="col" class="mono">{d().a}</th>
                  <th scope="col" class="mono">{d().b}</th>
                </tr>
              </thead>
              <tbody>
                <For each={all() ? d().verses : d().verses.slice(0, SHOWN)}>
                  {(v) => {
                    const [l, r] = markWords(v.a ?? "", v.b ?? "");
                    return (
                      <tr>
                        <th scope="row" class="mono num">
                          {v.chapter}:{v.verse}
                        </th>
                        <td>{v.a === null || v.a === undefined ? <span class="muted">absent</span> : <Words marks={l} />}</td>
                        <td>{v.b === null || v.b === undefined ? <span class="muted">absent</span> : <Words marks={r} />}</td>
                      </tr>
                    );
                  }}
                </For>
              </tbody>
            </table>
            <Show when={!all() && d().verses.length > SHOWN}>
              <button type="button" class="linkish small" onClick={() => setAll(true)}>
                Show all {d().verses.length.toLocaleString()}
              </button>
            </Show>
          </>
        )}
      </Async>
    </div>
  );
}
