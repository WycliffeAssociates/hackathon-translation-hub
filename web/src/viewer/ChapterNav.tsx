// The viewer's chapter navigator: ‹ [Chapter 5 of 28 ▾] ›, where the middle
// button opens a grid of every chapter in rows of ten (a hundred-chart, so
// Psalm 119 is row 12, column 9). Fixed width, so switching books does not move
// the bar. Cells are tinted with the book's lint and marked where verses are
// missing, from the BookRow the page already has.
import { For, Show, createEffect, createSignal, createUniqueId, onCleanup } from "solid-js";
import type { BookRow } from "../api/types";
import { plural, ranges } from "../ui/bits";

type Grade = "err" | "warn" | "clean";

interface Mark {
  readonly grade: Grade;
  readonly title: string;
}

/** Lint and coverage for one chapter label, as a tint and a tooltip. */
export function markOf(book: BookRow | undefined, label: string): Mark & { gap: boolean } {
  const [e = 0, w = 0, n = 0] = book?.lint?.by_chapter[label] ?? [];
  const gaps = book?.coverage?.gaps[label] ?? [];
  const parts = [`Chapter ${label}`];
  const lint = [e && plural(e, "error"), w && plural(w, "warning"), n && `${n} info`].filter(Boolean);
  if (lint.length) parts.push(lint.join(", "));
  if (gaps.length) parts.push(`missing v. ${ranges(gaps)}`);
  return { grade: e ? "err" : w ? "warn" : "clean", gap: gaps.length > 0, title: parts.join(" · ") };
}

export default function ChapterNav(props: {
  /** Chapter labels in book order. */
  labels: readonly string[];
  /** The label in view; `""` while the front matter is. */
  current: string;
  book: BookRow | undefined;
  onPick: (label: string) => void;
  onStep: (by: -1 | 1) => void;
}) {
  let root!: HTMLDivElement;
  let trigger!: HTMLButtonElement;
  let grid!: HTMLDivElement;
  const id = createUniqueId();
  const [open, setOpen] = createSignal(false);
  const at = () => props.labels.indexOf(props.current);
  const annotated = () => !!(props.book?.lint || props.book?.coverage);

  const close = (refocus: boolean) => {
    setOpen(false);
    if (refocus) trigger.focus();
  };

  // Outside click closes; opening focuses the current chapter.
  createEffect(() => {
    if (!open()) return;
    const away = (e: PointerEvent) => !root.contains(e.target as Node) && close(false);
    document.addEventListener("pointerdown", away);
    onCleanup(() => document.removeEventListener("pointerdown", away));
    requestAnimationFrame(() => {
      const cell = grid.querySelector<HTMLElement>('[aria-current="true"]') ?? grid.querySelector<HTMLElement>("button");
      cell?.focus();
    });
  });

  /** Arrows move through the grid: ten to a row. */
  const onGridKey = (e: KeyboardEvent) => {
    const cells = [...grid.querySelectorAll<HTMLButtonElement>("button")];
    const i = cells.indexOf(document.activeElement as HTMLButtonElement);
    const by: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -10, ArrowDown: 10 };
    let to: number | undefined;
    if (e.key in by) to = Math.max(0, Math.min(cells.length - 1, i + by[e.key]!));
    else if (e.key === "Home") to = 0;
    else if (e.key === "End") to = cells.length - 1;
    if (to === undefined) return;
    e.preventDefault();
    cells[to]?.focus();
  };

  return (
    <div class="chnav" ref={root} onKeyDown={(e) => e.key === "Escape" && open() && (e.preventDefault(), close(true))}>
      <div class="chnav-ctl">
        <button type="button" class="chnav-step" aria-label="Previous chapter" title="Previous chapter ( [ )" disabled={at() <= 0} onClick={() => props.onStep(-1)}>
          ‹
        </button>
        <button
          type="button"
          class="chnav-open"
          ref={trigger}
          aria-haspopup="dialog"
          aria-expanded={open()}
          aria-controls={id}
          aria-label={`${props.current ? `Chapter ${props.current} of ${props.labels.length}` : `${props.labels.length} chapters`}. Choose a chapter`}
          onClick={() => setOpen(!open())}
        >
          {/* In the front matter, the book's range: "Chapters 1–150". */}
          <Show
            when={props.current}
            fallback={
              <>
                <span class="chnav-word">Chapters </span>
                <span class="chnav-now">{props.labels[0]}{props.labels.length > 1 ? `–${props.labels.at(-1)}` : ""}</span>
              </>
            }
          >
            <span class="chnav-word">Chapter </span>
            <b class="chnav-now">{props.current}</b>
            <span class="chnav-of"> of {props.labels.length}</span>
          </Show>
          <span class="chnav-caret" aria-hidden="true">▾</span>
        </button>
        <button type="button" class="chnav-step" aria-label="Next chapter" title="Next chapter ( ] )" disabled={at() >= props.labels.length - 1} onClick={() => props.onStep(1)}>
          ›
        </button>
      </div>
      <Show when={open()}>
        <div class="chnav-pop" id={id} role="dialog" aria-label="Chapters">
          <div class="chnav-grid" ref={grid} onKeyDown={onGridKey}>
            <For each={props.labels}>
              {(label) => {
                const m = () => markOf(props.book, label);
                return (
                  <button
                    type="button"
                    class={`chn ${m().grade}`}
                    classList={{ gap: m().gap }}
                    aria-current={label === props.current ? "true" : undefined}
                    title={m().title}
                    aria-label={m().title}
                    onClick={() => {
                      setOpen(false);
                      props.onPick(label);
                    }}
                  >
                    {label}
                  </button>
                );
              }}
            </For>
          </div>
          <p class="chnav-legend small">
            <Show when={annotated()}>
              <span><i class="chn err" /> errors</span>
              <span><i class="chn warn" /> warnings</span>
              <span><i class="chn gap" /> missing verses</span>
            </Show>
            <span class="chnav-keys"><kbd>[</kbd> <kbd>]</kbd> previous / next</span>
          </p>
        </div>
      </Show>
    </div>
  );
}
