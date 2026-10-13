// Per book: lint counts, the codes that fire most, size, coverage against the
// declared source, and one cell per chapter.
import { For, Show } from "solid-js";
import type { BookRow, CoverageRow, LintRow } from "../../api/types";
import { Counts } from "../../ui/bits";

/** `[4, 5, 6, 9]` → `4–6, 9`. */
const ranges = (ns: readonly number[]) => {
  const out: string[] = [];
  for (let i = 0; i < ns.length; i++) {
    let j = i;
    while (j + 1 < ns.length && ns[j + 1] === ns[j]! + 1) j++;
    out.push(i === j ? `${ns[i]}` : `${ns[i]}–${ns[j]}`);
    i = j;
  }
  return out.join(", ");
};

const topCodes = (lint: LintRow) =>
  Object.entries(lint.by_code)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 3);

export default function Books(props: {
  books: readonly BookRow[];
  /** Open a book in the viewer, optionally at a chapter. */
  onOpen: (path: string, chapter?: string) => void;
}) {
  return (
    <div class="books">
      <table>
        <thead>
          <tr>
            <th scope="col">Book</th>
            <th scope="col">Lint</th>
            <th scope="col">Top codes</th>
            <th scope="col">Ch / vv</th>
            <th scope="col">Coverage</th>
            <th scope="col">Chapters</th>
          </tr>
        </thead>
        <tbody>
          <For each={props.books}>
            {(b) => (
              <tr>
                <th scope="row">
                  <button type="button" class="linkish mono" onClick={() => props.onOpen(b.path)} title={`Open ${b.path}`}>
                    {b.book || b.path}
                  </button>
                </th>
                <td>
                  <Show when={b.lint} fallback={<span class="small">not linted</span>}>
                    {(l) => <Counts errors={l().errors} warnings={l().warnings} infos={l().infos} />}
                  </Show>
                </td>
                <td class="small mono">
                  <Show when={b.lint}>
                    {(l) => <For each={topCodes(l())}>{([code, n]) => <div>{code} <span class="count">{n}</span></div>}</For>}
                  </Show>
                </td>
                <td class="num">
                  {b.chapters} / {b.verses.toLocaleString()}
                </td>
                <td>
                  <Show when={b.coverage} fallback={<span class="small">no source</span>}>
                    {(c) => <Coverage c={c()} onChapter={(ch) => props.onOpen(b.path, ch)} />}
                  </Show>
                </td>
                <td>
                  <ChapterBar book={b} onChapter={(ch) => props.onOpen(b.path, ch)} />
                </td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
    </div>
  );
}

function Coverage(props: { c: CoverageRow; onChapter: (ch: string) => void }) {
  const pct = () => (props.c.source_verses ? Math.floor((props.c.present / props.c.source_verses) * 100) : 0);
  const gaps = () => Object.entries(props.c.gaps).sort((a, b) => Number(a[0]) - Number(b[0]));
  return (
    <div class="cov">
      <span class="cov-num" classList={{ ok: !props.c.missing }}>{pct()}%</span>{" "}
      <span class="small">
        {props.c.present.toLocaleString()} of {props.c.source_verses.toLocaleString()} vs <span class="mono">{props.c.source}</span>
      </span>
      <Show when={props.c.missing}>
        <details class="small">
          <summary>{props.c.missing.toLocaleString()} missing</summary>
          <ul class="gaps">
            <For each={gaps()}>
              {([ch, vv]) => (
                <li>
                  <button type="button" class="linkish" onClick={() => props.onChapter(ch)}>ch {ch}</button>: {ranges(vv)}
                </li>
              )}
            </For>
          </ul>
        </details>
      </Show>
      <Show when={props.c.extra}><span class="small"> · {props.c.extra} extra</span></Show>
    </div>
  );
}

/** One cell per chapter: worst lint severity, and a mark where verses are missing. */
function ChapterBar(props: { book: BookRow; onChapter: (ch: string) => void }) {
  const cells = () =>
    Array.from({ length: props.book.chapters }, (_, i) => {
      const ch = String(i + 1);
      const [e = 0, w = 0, n = 0] = props.book.lint?.by_chapter[ch] ?? [];
      const gap = props.book.coverage?.gaps[ch]?.length ?? 0;
      const grade = e ? "err" : w ? "warn" : n ? "info" : "clean";
      return { ch, grade, gap, title: `Chapter ${ch}: ${e} errors, ${w} warnings, ${n} info${gap ? `, ${gap} verses missing` : ""}` };
    });
  return (
    <div class="chbar" role="list">
      <For each={cells()}>
        {(c) => <button type="button" role="listitem" class={`chc ${c.grade}`} classList={{ gap: c.gap > 0 }} title={c.title} aria-label={c.title} onClick={() => props.onChapter(c.ch)} />}
      </For>
    </div>
  );
}
