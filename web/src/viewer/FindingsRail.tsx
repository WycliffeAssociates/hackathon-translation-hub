// The open book's findings beside the text: everything the viewer underlines
// (the engine's lint and Sous Chef's stored rows), grouped by check with the
// noisiest first, each row a chapter:verse that selects its exact span. It
// lists what the viewer draws, so a row and a squiggle are the same thing.
import { For, Show, createEffect, createMemo, createSignal, on } from "solid-js";
import { plural } from "../ui/bits";

export interface RailItem {
  readonly from: number;
  readonly to: number;
  /** `lint` or `sous`. */
  readonly engine: string;
  readonly code: string;
  readonly severity: "error" | "warning" | "info" | "hint";
  readonly message: string;
  /** `3:16`, `3`, or `intro`. */
  readonly ref: string;
  /** The marked text with a little around it, invisible characters spelled out. */
  readonly context: string;
}

interface Group {
  readonly key: string;
  /** The one message every row has, when they all have the same; rows then show their text instead. */
  readonly same: string | null;
  readonly engine: string;
  readonly code: string;
  readonly severity: RailItem["severity"];
  readonly items: readonly RailItem[];
}

const RANK = { error: 0, warning: 1, info: 2, hint: 2 } as const;

export default function FindingsRail(props: {
  items: readonly RailItem[];
  /** Stored-row caps: a check whose true count in this book is higher says so. */
  totals: (engine: string, code: string) => number | undefined;
  /** `engine:code` to open to start with (the check picked on the page). */
  initial: string | null;
  onPick: (item: RailItem) => void;
  /** Lint the server's scan counted in this book that the viewer's engine does not draw, by check. */
  undrawn: readonly { code: string; n: number }[];
}) {
  const groups = createMemo<Group[]>(() => {
    const by = new Map<string, RailItem[]>();
    for (const item of props.items) {
      const key = `${item.engine}:${item.code}`;
      let list = by.get(key);
      if (!list) by.set(key, (list = []));
      list.push(item);
    }
    return [...by.entries()]
      .map(([key, items]) => {
        const sorted = items.toSorted((a, b) => a.from - b.from);
        const severity = sorted.reduce<RailItem["severity"]>((s, i) => (RANK[i.severity] < RANK[s] ? i.severity : s), "hint");
        const same = sorted.length > 1 && sorted.every((i) => i.message === sorted[0]!.message) ? sorted[0]!.message : null;
        return { key, same, engine: sorted[0]!.engine, code: sorted[0]!.code, severity, items: sorted };
      })
      .sort((a, b) => RANK[a.severity] - RANK[b.severity] || b.items.length - a.items.length || a.code.localeCompare(b.code));
  });

  const [open, setOpen] = createSignal(new Set(props.initial ? [props.initial] : []));
  // A check picked on the page while this book is open opens here too.
  createEffect(on(() => props.initial, (key) => key && setOpen((s) => new Set(s).add(key)), { defer: true }));
  const toggle = (key: string) =>
    setOpen((s) => {
      const next = new Set(s);
      if (!next.delete(key)) next.add(key);
      return next;
    });
  // By place and check, not identity: the list is rebuilt when the linter reruns.
  const [picked, setPicked] = createSignal("");
  const keyOf = (i: RailItem) => `${i.engine}:${i.code}@${i.from}-${i.to}`;

  return (
    <aside class="rail" aria-label="Findings in this book">
      <div class="rail-head">
        <b>Findings</b> <span class="small">{plural(props.items.length, "mark")}</span>
      </div>
      <Show when={groups().length} fallback={<p class="small rail-empty">Nothing fires in this book.</p>}>
        <ul class="rail-groups">
          <For each={groups()}>
            {(g) => {
              const isOpen = () => open().has(g.key);
              const total = () => props.totals(g.engine, g.code);
              return (
                <li class="rail-group">
                  <button type="button" class="rail-check" aria-expanded={isOpen()} onClick={() => toggle(g.key)}>
                    <span class="caret" aria-hidden="true">{isOpen() ? "▾" : "▸"}</span>
                    <span class={`dot ${g.severity}`} aria-hidden="true" />
                    <span class="mono code">{g.code}</span>
                    <span class="tag small">{g.engine}</span>
                    <span class="n">{g.items.length.toLocaleString()}</span>
                  </button>
                  <Show when={isOpen()}>
                    <Show when={g.same}>{(m) => <p class="small rail-same">{m()}</p>}</Show>
                    <Show when={(total() ?? 0) > g.items.length}>
                      <p class="small rail-capped">{total()!.toLocaleString()} in this book; the first {g.items.length} are kept</p>
                    </Show>
                    <ol class="rail-items">
                      <For each={g.items}>
                        {(item) => (
                          <li>
                            <button
                              type="button"
                              classList={{ on: picked() === keyOf(item) }}
                              title={item.message}
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
        <p class="small rail-hidden" title="The scan and the viewer's engine disagree on these; see TODO.md">
          Counted by the scan, not drawn here:{" "}
          {props.undrawn.map((u) => `${u.code} ${u.n.toLocaleString()}`).join(", ")}
        </p>
      </Show>
    </aside>
  );
}
