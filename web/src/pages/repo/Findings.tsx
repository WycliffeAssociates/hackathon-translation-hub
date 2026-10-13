// Every check that fired in the repo, counts first: one row per check, sorted
// by total (the server's order). A row opens to its per-book counts; a book
// opens the viewer. Rows themselves are fetched only for the book picked
// (see CheckRows), since the server keeps at most 200 per book and check.
import { For, Show, createSignal } from "solid-js";
import type { CodeCount, Finding } from "../../api/types";
import { CheckBadges, checkKey, plural } from "../../ui/bits";

export default function Findings(props: {
  counts: readonly CodeCount[];
  /** The check in the URL, opened to start with. */
  current: string | null;
  onBook: (c: CodeCount, book: string) => void;
}) {
  const [open, setOpen] = createSignal(new Set(props.current ? [props.current] : []));
  const toggle = (key: string) =>
    setOpen((s) => {
      const next = new Set(s);
      if (!next.delete(key)) next.add(key);
      return next;
    });
  return (
    <Show when={props.counts.length} fallback={<p class="clean-note">No check fired.</p>}>
      <div class="checks-table">
        <table>
          <thead>
            <tr>
              <th scope="col">Check</th>
                            <th scope="col" class="num">Total</th>
              <th scope="col" class="num">Books</th>
            </tr>
          </thead>
          <tbody>
            <For each={props.counts}>
              {(c) => {
                const key = checkKey(c);
                const isOpen = () => open().has(key);
                return (
                  <>
                    <tr class="check-row" classList={{ focus: props.current === key }} onClick={() => toggle(key)}>
                      <th scope="row">
                        <button type="button" class="linkish mono" aria-expanded={isOpen()}>
                          <span class="caret" aria-hidden="true">{isOpen() ? "▾" : "▸"}</span>{c.code}
                        </button> <CheckBadges engine={c.engine} severity={c.severity} />
                      </th>
                      <td class="num total">{c.total.toLocaleString()}</td>
                      <td class="num">{c.books.length}</td>
                    </tr>
                    <Show when={isOpen()}>
                      <tr class="check-books">
                        <td colSpan={3}>
                          <div class="book-counts">
                            <For each={c.books}>
                              {(b) => (
                                <button type="button" class="book-chip" onClick={() => props.onBook(c, b.book)} title={`Open ${b.book} with ${c.code}`}>
                                  {b.book} <span class="count">{b.n.toLocaleString()}</span>
                                </button>
                              )}
                            </For>
                          </div>
                        </td>
                      </tr>
                    </Show>
                  </>
                );
              }}
            </For>
          </tbody>
        </table>
      </div>
    </Show>
  );
}

/** The stored rows of one Sous Chef check in the open book, each opening its range. */
export function CheckRows(props: {
  code: string;
  book: string;
  /** The true count, from /findings. */
  total: number | undefined;
  rows: readonly Finding[];
  onOpen: (f: Finding) => void;
}) {
  const more = () => (props.total ?? 0) > props.rows.length;
  return (
    <div class="check-rows">
      <p class="small">
        <b class="mono">{props.code}</b> in <b class="mono">{props.book}</b>:{" "}
        <Show when={more()} fallback={plural(props.rows.length, "finding")}>
          showing {props.rows.length.toLocaleString()} of {props.total!.toLocaleString()} · the rest are counted, not kept
        </Show>
      </p>
      <Show when={props.rows.length}>
        <ol class="finding-list">
          <For each={props.rows}>
            {(f) => (
              <li>
                <button type="button" class="linkish mono" onClick={() => props.onOpen(f)} title={`Select ${f.book} ${f.chapter}:${f.verse ?? "–"} in the viewer`}>
                  {f.chapter}:{f.verse ?? "–"}
                </button>{" "}
                <span>{f.message}</span> <span class="excerpt mono">{f.text}</span>
              </li>
            )}
          </For>
        </ol>
      </Show>
    </div>
  );
}
