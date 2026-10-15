// Progress of a language and brand against its gateway sources (a proposal):
// one tab per source, each a strip of the source's books shaded by how many
// of the source's verses the language's text has (the official copy, else the
// pick; counted by verse number from the server's fingerprints), one number
// for the whole, and saves per week for momentum. A source can be pinned
// beside the ones the copies declare; pins are this browser's, per language.
import { For, Show, createMemo, createResource, createSignal } from "solid-js";
import { api } from "../api/client";
import type { SourceProgress } from "../api/types";
import { Sparkline } from "../ui/bits";

/** Five steps of one hue, light to dark: none, some, half, nearly, all. */
const STEPS = [
  { label: "none", test: (r: number) => r === 0, mix: 0 },
  { label: "under half", test: (r: number) => r < 0.5, mix: 30 },
  { label: "half or more", test: (r: number) => r < 0.9, mix: 55 },
  { label: "nearly all", test: (r: number) => r < 1, mix: 78 },
  { label: "all", test: () => true, mix: 100 },
] as const;
const step = (r: number) => STEPS.find((s) => s.test(r))!;
const fill = (mix: number) => (mix === 0 ? "var(--empty)" : `color-mix(in srgb, var(--official) ${mix}%, var(--wf-bg))`);

const pinKey = (ietf: string, resource: string) => `hub.pinned.${ietf}.${resource}`;
export const readPins = (ietf: string, resource: string): string[] => {
  try {
    return JSON.parse(localStorage.getItem(pinKey(ietf, resource)) ?? "[]") as string[];
  } catch {
    return [];
  }
};
const writePins = (ietf: string, resource: string, pins: string[]) => {
  try {
    localStorage.setItem(pinKey(ietf, resource), JSON.stringify(pins));
  } catch {
    // Storage blocked: the pin lasts for this page only.
  }
};

export default function SourcesProgress(props: {
  ietf: string;
  resource: string;
  progress: readonly SourceProgress[];
  /** The pinned sources changed: ask again with these. */
  onAgainst: (pins: string[]) => void;
}) {
  const [tab, setTab] = createSignal(0);
  const [tip, setTip] = createSignal<{ text: string; x: number; y: number }>();
  const current = () => props.progress[Math.min(tab(), props.progress.length - 1)];
  const totals = createMemo(() => {
    const p = current();
    if (!p) return null;
    const present = p.books.reduce((n, b) => n + b.present, 0);
    const total = p.books.reduce((n, b) => n + b.total, 0);
    const done = p.books.filter((b) => b.total > 0 && b.present === b.total).length;
    return { present, total, done, pct: total ? Math.round((present / total) * 100) : 0 };
  });

  // Saves per week over the last year, every author and book together.
  const [weekly] = createResource(
    () => ({ path: { ietf: props.ietf }, query: { resource: props.resource } }),
    (c) => api.json("/v1/languages/{ietf}/progress", c).catch(() => null),
  );
  const weeks = createMemo(() => {
    const w = weekly()?.weekly ?? [];
    if (!w.length) return [];
    const by = new Map<number, number>();
    for (const r of w) by.set(r.week, (by.get(r.week) ?? 0) + r.saves);
    const week = 7 * 86400;
    // Weeks count from the epoch (a Thursday), as the server buckets them.
    const last = Math.floor(Date.now() / 1000 / week) * week;
    return Array.from({ length: 52 }, (_, i) => by.get(last - (51 - i) * week) ?? 0);
  });

  const pin = (e: SubmitEvent) => {
    e.preventDefault();
    const input = (e.currentTarget as HTMLFormElement).elements.namedItem("source") as HTMLInputElement;
    const src = input.value.trim().toLowerCase();
    if (!/^[^/\s]+\/[^/\s]+$/.test(src)) return;
    const pins = [...new Set([...readPins(props.ietf, props.resource), src])];
    writePins(props.ietf, props.resource, pins);
    input.value = "";
    props.onAgainst(pins);
    setTab(props.progress.length);
  };
  const unpin = (src: string) => {
    const pins = readPins(props.ietf, props.resource).filter((p) => p !== src);
    writePins(props.ietf, props.resource, pins);
    setTab(0);
    props.onAgainst(pins);
  };

  return (
    <section class="progress" aria-label="Progress against sources">
      <div class="progress-tabs" role="tablist">
        <For each={props.progress}>
          {(p, i) => (
            <button type="button" role="tab" aria-selected={i() === tab()} onClick={() => setTab(i())}>
              {p.source}
            </button>
          )}
        </For>
        <form class="progress-pin" onSubmit={pin}>
          <input name="source" placeholder="Pin a source: owner/name" aria-label="Pin a source" size={22} />
        </form>
      </div>
      <Show when={current()} fallback={<p class="small">No source declared. Pin one to measure against it.</p>}>
        {(p) => (
          <div class="progress-body">
            <p class="progress-number">
              <b>{totals()!.pct}%</b>
              <span>
                of {p().source}'s verses · {totals()!.present.toLocaleString()} of {totals()!.total.toLocaleString()} ·{" "}
                {totals()!.done} of {p().books.length} books complete
                <Show when={readPins(props.ietf, props.resource).includes(p().source)}>
                  {" "}
                  <button type="button" class="linkish small" onClick={() => unpin(p().source)}>
                    Unpin
                  </button>
                </Show>
              </span>
            </p>
            <div class="strip" role="list" onMouseLeave={() => setTip(undefined)}>
              <For each={p().books}>
                {(b) => {
                  const r = () => (b.total ? b.present / b.total : 0);
                  const text = () =>
                    `${b.book}: ${b.present.toLocaleString()} of ${b.total.toLocaleString()} verses (${Math.round(r() * 100)}%)`;
                  return (
                    <span
                      role="listitem"
                      class="strip-cell"
                      classList={{ nt: b.book === "MAT" }}
                      style={{ background: fill(step(r()).mix) }}
                      aria-label={text()}
                      onMouseEnter={(e) => {
                        const box = (e.currentTarget as HTMLElement).getBoundingClientRect();
                        setTip({ text: text(), x: box.left + box.width / 2, y: box.top });
                      }}
                    />
                  );
                }}
              </For>
            </div>
            <Show when={tip()}>
              {(t) => (
                <div class="strip-tip" style={{ left: `${t().x}px`, top: `${t().y}px` }} role="tooltip">
                  {t().text}
                </div>
              )}
            </Show>
            <div class="progress-foot">
              <span class="strip-legend small">
                <For each={STEPS}>
                  {(s) => (
                    <span>
                      <i style={{ background: fill(s.mix) }} /> {s.label}
                    </span>
                  )}
                </For>
              </span>
              <Show when={weeks().some((n) => n > 0)}>
                <span class="progress-saves small">
                  <Sparkline values={weeks()} title={`Writer saves per week, the last 52 weeks: ${weeks().reduce((a, b) => a + b, 0)} in all`} />
                  {weeks().reduce((a, b) => a + b, 0).toLocaleString()} saves in the last year
                </span>
              </Show>
            </div>
          </div>
        )}
      </Show>
    </section>
  );
}
