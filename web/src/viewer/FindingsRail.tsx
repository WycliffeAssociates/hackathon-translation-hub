// The findings rail beside the text. Two scopes:
//
// - This book: every mark the viewer draws (the engine's lint and Sous Chef's
//   stored rows), grouped by check, each row a chapter:verse that selects its
//   exact span. It lists what the viewer draws, so a row and a squiggle are the
//   same thing.
// - All books: the repo's counts by check, each opening to its books; a book
//   opens there with the check open.
//
// Filters up top narrow both: words (check name, message, marked text), engine,
// and severity (Sous Chef's hygiene reads as a warning, convention as info).
import { For, Show, createEffect, createMemo, createSignal, on } from "solid-js";
import type { CodeCount } from "../api/types";
import { plural } from "../ui/bits";

export type Severity = "error" | "warning" | "info";

export interface RailItem {
  readonly from: number;
  readonly to: number;
  /** `lint` or `sous`. */
  readonly engine: string;
  readonly code: string;
  readonly severity: Severity;
  readonly message: string;
  /** `3:16`, `3`, or `intro`. */
  readonly ref: string;
  /** The marked text with a little around it, invisible characters spelled out. */
  readonly context: string;
}

interface Group {
  readonly key: string;
  readonly engine: string;
  readonly code: string;
  readonly severity: Severity;
  readonly items: readonly RailItem[];
  /** The one message every row has, when they all have the same; rows then show their text instead. */
  readonly same: string | null;
}

const RANK: Record<Severity, number> = { error: 0, warning: 1, info: 2 };
const ENGINES = ["lint", "sous"] as const;
const SEVERITIES: readonly Severity[] = ["error", "warning", "info"];

/** A check's severity as the filter reads it: lint's own, Sous Chef's lane mapped. */
export const severityOf = (engine: string, severity: string): Severity =>
  engine === "sous"
    ? severity === "hygiene"
      ? "warning"
      : "info"
    : severity === "error" || severity === "warning"
      ? severity
      : "info";

export default function FindingsRail(props: {
  /** What the viewer draws in the open book. */
  items: readonly RailItem[];
  /** The repo's counts, every book (`/findings`). */
  counts: readonly CodeCount[];
  /** The open book's id (`MAT`). */
  book: string | undefined;
  /** `engine:code` to open to start with (the check picked on the page). */
  initial: string | null;
  onPick: (item: RailItem) => void;
  /** Open another book with this check. */
  onBook: (engine: string, code: string, book: string) => void;
  /** Lint the scan counted in this book that the viewer is not drawing, by check. */
  undrawn: readonly { code: string; n: number }[];
  /** Regular hides markup, and with it the marks on markup; USFM shows them. */
  mode: "regular" | "usfm";
  onShowMarkup: () => void;
}) {
  const [scope, setScope] = createSignal<"book" | "all">("book");
  const [q, setQ] = createSignal("");
  const [engines, setEngines] = createSignal(new Set<string>(ENGINES));
  const [severities, setSeverities] = createSignal(new Set<Severity>(SEVERITIES));
  const flip = <T,>(set: (f: (s: Set<T>) => Set<T>) => void, v: T) =>
    set((s) => {
      const next = new Set(s);
      if (!next.delete(v)) next.add(v);
      return next;
    });

  const words = () => q().trim().toLowerCase();
  const passes = (engine: string, severity: Severity) => engines().has(engine) && severities().has(severity);

  /** A check's true count in the open book (the server keeps at most 200 Sous rows of one). */
  const total = (engine: string, code: string) =>
    props.counts.find((c) => c.engine === engine && c.code === code)?.books.find((b) => b.book === props.book)?.n;

  const groups = createMemo<Group[]>(() => {
    const w = words();
    const by = new Map<string, RailItem[]>();
    for (const item of props.items) {
      if (!passes(item.engine, item.severity)) continue;
      if (w && !`${item.code} ${item.message} ${item.context} ${item.ref}`.toLowerCase().includes(w)) continue;
      const key = `${item.engine}:${item.code}`;
      let list = by.get(key);
      if (!list) by.set(key, (list = []));
      list.push(item);
    }
    return [...by.entries()]
      .map(([key, items]) => {
        const sorted = items.toSorted((a, b) => a.from - b.from);
        const severity = sorted.reduce<Severity>((s, i) => (RANK[i.severity] < RANK[s] ? i.severity : s), "info");
        const same = sorted.length > 1 && sorted.every((i) => i.message === sorted[0]!.message) ? sorted[0]!.message : null;
        return { key, engine: sorted[0]!.engine, code: sorted[0]!.code, severity, items: sorted, same };
      })
      .sort((a, b) => RANK[a.severity] - RANK[b.severity] || b.items.length - a.items.length || a.code.localeCompare(b.code));
  });

  const repoChecks = createMemo(() => {
    const w = words();
    return props.counts
      .filter((c) => passes(c.engine, severityOf(c.engine, c.severity)))
      .filter((c) => !w || c.code.toLowerCase().includes(w) || c.books.some((b) => b.book.toLowerCase() === w))
      .toSorted(
        (a, b) =>
          RANK[severityOf(a.engine, a.severity)] - RANK[severityOf(b.engine, b.severity)] || b.total - a.total || a.code.localeCompare(b.code),
      );
  });

  const [open, setOpen] = createSignal(new Set(props.initial ? [props.initial] : []));
  // A check picked on the page while this book is open opens here too.
  createEffect(on(() => props.initial, (key) => key && setOpen((s) => new Set(s).add(key)), { defer: true }));
  const toggle = (key: string) => flip(setOpen, key);

  // By place and check, not identity: the list is rebuilt when the linter reruns.
  const [picked, setPicked] = createSignal("");
  const keyOf = (i: RailItem) => `${i.engine}:${i.code}@${i.from}-${i.to}`;

  const shownCount = () => groups().reduce((n, g) => n + g.items.length, 0);

  return (
    <aside class="rail" aria-label="Findings">
      <div class="rail-head">
        <div class="seg rail-scope" role="group" aria-label="Scope">
          <button type="button" aria-pressed={scope() === "book"} onClick={() => setScope("book")}>
            {props.book ?? "This book"} <span class="n">{shownCount().toLocaleString()}</span>
          </button>
          <button type="button" aria-pressed={scope() === "all"} onClick={() => setScope("all")}>
            All books <span class="n">{repoChecks().reduce((n, c) => n + c.total, 0).toLocaleString()}</span>
          </button>
        </div>
        <input
          type="search"
          class="rail-filter"
          placeholder={scope() === "book" ? "Filter: check, words, 3:16…" : "Filter: check or book…"}
          aria-label="Filter findings"
          value={q()}
          onInput={(e) => setQ(e.currentTarget.value)}
        />
        <div class="rail-chips" role="group" aria-label="Show">
          <For each={ENGINES}>
            {(e) => (
              <button type="button" class={`fchip eng-${e}`} aria-pressed={engines().has(e)} onClick={() => flip(setEngines, e)}>
                {e}
              </button>
            )}
          </For>
          <span class="rail-sep" aria-hidden="true" />
          <For each={SEVERITIES}>
            {(s) => (
              <button type="button" class="fchip" aria-pressed={severities().has(s)} onClick={() => flip(setSeverities, s)}>
                <span class={`dot ${s}`} aria-hidden="true" /> {s}
              </button>
            )}
          </For>
        </div>
      </div>

      <Show when={scope() === "book"}>
        <Show when={groups().length} fallback={<p class="small rail-empty">{props.items.length ? "Nothing matches the filters." : "Nothing fires in this book."}</p>}>
          <ul class="rail-groups">
            <For each={groups()}>
              {(g) => {
                const isOpen = () => open().has(g.key) || !!words();
                const t = () => total(g.engine, g.code);
                return (
                  <li class="rail-group">
                    <button type="button" class="rail-check" aria-expanded={isOpen()} onClick={() => toggle(g.key)}>
                      <span class="caret" aria-hidden="true">{isOpen() ? "▾" : "▸"}</span>
                      <span class={`dot ${g.severity}`} aria-hidden="true" />
                      <span class="mono code">{g.code}</span>
                      <span class={`tag small eng-${g.engine}`}>{g.engine}</span>
                      <span class="n">{g.items.length.toLocaleString()}</span>
                    </button>
                    <Show when={isOpen()}>
                      <Show when={g.same}>{(m) => <p class="small rail-same">{m()}</p>}</Show>
                      <Show when={(t() ?? 0) > g.items.length && g.engine === "sous"}>
                        <p class="small rail-capped">{t()!.toLocaleString()} in this book; the first {g.items.length} are kept</p>
                      </Show>
                      <ol class="rail-items">
                        <For each={g.items}>
                          {(item) => (
                            <li>
                              <button
                                type="button"
                                classList={{ on: picked() === keyOf(item) }}
                                title={`${item.message}\n${item.context}`}
                                onClick={() => {
                                  setPicked(keyOf(item));
                                  props.onPick(item);
                                }}
                              >
                                <span class="mono ref">{item.ref}</span>
                                <span class="msg" classList={{ mono: !!g.same }}>{g.same ? item.context : item.message}</span>
                              </button>
                            </li>
                          )}
                        </For>
                      </ol>
                    </Show>
                  </li>
                );
              }}
            </For>
          </ul>
        </Show>
        <Show when={props.undrawn.length}>
          <p class="small rail-hidden">
            <Show
              when={props.mode === "regular"}
              fallback={<>Counted by the scan, not drawn here: {props.undrawn.map((u) => `${u.code} ${u.n.toLocaleString()}`).join(", ")}</>}
            >
              On markup Regular hides: {props.undrawn.map((u) => `${u.code} ${u.n.toLocaleString()}`).join(", ")}.{" "}
              <button type="button" class="linkish" onClick={() => props.onShowMarkup()}>Show in USFM</button>
            </Show>
          </p>
        </Show>
      </Show>

      <Show when={scope() === "all"}>
        <Show when={repoChecks().length} fallback={<p class="small rail-empty">Nothing matches the filters.</p>}>
          <ul class="rail-groups">
            <For each={repoChecks()}>
              {(c) => {
                const key = `${c.engine}:${c.code}`;
                const isOpen = () => open().has(key);
                return (
                  <li class="rail-group">
                    <button type="button" class="rail-check" aria-expanded={isOpen()} onClick={() => toggle(key)}>
                      <span class="caret" aria-hidden="true">{isOpen() ? "▾" : "▸"}</span>
                      <span class={`dot ${severityOf(c.engine, c.severity)}`} aria-hidden="true" />
                      <span class="mono code">{c.code}</span>
                      <span class={`tag small eng-${c.engine}`}>{c.engine}</span>
                      <span class="n">{c.total.toLocaleString()}</span>
                    </button>
                    <Show when={isOpen()}>
                      <p class="small rail-same">{plural(c.books.length, "book")}</p>
                      <div class="rail-books">
                        <For each={c.books}>
                          {(b) => (
                            <button
                              type="button"
                              class="book-chip"
                              classList={{ here: b.book === props.book }}
                              title={b.book === props.book ? `${b.book} is open: list its ${c.code} marks` : `Open ${b.book} with ${c.code}`}
                              onClick={() => {
                                if (b.book === props.book) {
                                  setOpen((s) => new Set(s).add(key));
                                  setScope("book");
                                } else props.onBook(c.engine, c.code, b.book);
                              }}
                            >
                              {b.book} <span class="count">{b.n.toLocaleString()}</span>
                            </button>
                          )}
                        </For>
                      </div>
                    </Show>
                  </li>
                );
              }}
            </For>
          </ul>
        </Show>
      </Show>
    </aside>
  );
}
