// Small shared view pieces. Nothing here fetches.
import { For, Match, Show, Switch, type Accessor, type JSX, type Resource } from "solid-js";
import { A } from "@solidjs/router";
import type { ApiError } from "../api/client";

/** Loading, failure, or the value of a resource. Keeps the old value while it refetches. */
export function Async<T>(props: { of: Resource<T>; what: string; children: (v: Accessor<T>) => JSX.Element }) {
  return (
    <Switch>
      <Match when={props.of.error}>
        <Failure error={props.of.error} />
      </Match>
      <Match when={props.of.latest === undefined}>
        <p class="loading">Loading {props.what}…</p>
      </Match>
      <Match when={props.of.latest !== undefined}>{props.children(() => props.of.latest as T)}</Match>
    </Switch>
  );
}

export function Failure(props: { error: unknown }) {
  const e = () => props.error as Partial<ApiError> & { message?: string };
  return (
    <div class="failure" role="alert">
      <b>{e().status ? `${e().status}` : "Error"}</b> {e().message ?? String(props.error)}
    </div>
  );
}

/** `[4, 5, 6, 9]` → `4–6, 9`. */
export const ranges = (ns: readonly number[]) => {
  const out: string[] = [];
  for (let i = 0; i < ns.length; i++) {
    let j = i;
    while (j + 1 < ns.length && ns[j + 1] === ns[j]! + 1) j++;
    out.push(i === j ? `${ns[i]}` : `${ns[i]}–${ns[j]}`);
    i = j;
  }
  return out.join(", ");
};

export const plural = (n: number, word: string) => `${n.toLocaleString()} ${word}${n === 1 ? "" : "s"}`;

/** The error / warning / info badges. Zero counts are drawn quietly. */
export function Counts(props: { errors: number; warnings: number; infos?: number }) {
  return (
    <span class="counts">
      <span class="sev error" classList={{ zero: !props.errors }}>{plural(props.errors, "error")}</span>{" "}
      <span class="sev warning" classList={{ zero: !props.warnings }}>{plural(props.warnings, "warning")}</span>
      <Show when={props.infos !== undefined}>
        {" "}
        <span class="sev info" classList={{ zero: !props.infos }}>{(props.infos ?? 0).toLocaleString()} info</span>
      </Show>
    </span>
  );
}

export const ago = (unix: number | null | undefined): string => {
  if (!unix) return "";
  const minutes = Math.round((Date.now() / 1000 - unix) / 60);
  if (minutes < 60) return `${Math.max(minutes, 1)} min ago`;
  if (minutes < 60 * 24) return `${Math.round(minutes / 60)} h ago`;
  const days = Math.round(minutes / 1440);
  if (days < 60) return `${days} days ago`;
  if (days < 730) return `${Math.round(days / 30)} months ago`;
  return `${Math.round(days / 365)} years ago`;
};

export const dateOf = (unix: number | null | undefined) => (unix ? new Date(unix * 1000).toISOString().slice(0, 16).replace("T", " ") : "");

export const FORGEJO = "https://content.bibletranslationtools.org";

/**
 * Sefer on the Web (the r-links dev preview for now). A share link
 * `/r/<owner>/<repo>?host=<cloud>` opens the repo there; the book path and
 * `at=` are optional (Sefer `src/app/ui/landing/OpenByLink.tsx`, branch
 * r-links). `host` is where Sefer reads the repo, the same WACS as FORGEJO.
 */
export const SEFER = "https://r-links-sefer-web-dev.wycliffe-associates-account.workers.dev";
export const seferLink = (owner: string, name: string): string =>
  `${SEFER}/r/${encodeURIComponent(owner)}/${encodeURIComponent(name)}?host=${new URL(FORGEJO).host}`;

export const repoHref = (owner: string, name: string) => `/repos/${owner}/${name}`;

export function RepoLink(props: { owner: string; name: string }) {
  return (
    <A class="mono" href={repoHref(props.owner, props.name)}>
      {props.owner}/{props.name}
    </A>
  );
}

/** Catalog label (Primary / Active / Inactive) as a chip. */
export function StatusChip(props: { status: string | null | undefined }) {
  return <span class={`chip chip-${(props.status ?? "none").toLowerCase()}`}>{props.status ?? "not in catalog"}</span>;
}

/** A tiny line of numbers, drawn only when there is more than one point. */
/** `max`: the top of the scale (a book's full verse count), when not the largest value. */
export function Sparkline(props: { values: readonly number[]; title: string; max?: number }) {
  const w = 120;
  const h = 24;
  const points = () => {
    const v = props.values;
    const max = Math.max(1, props.max ?? 0, ...v);
    return v.map((y, i) => `${((i / (v.length - 1)) * w).toFixed(1)},${(h - 2 - (y / max) * (h - 4)).toFixed(1)}`).join(" ");
  };
  return (
    <Show when={props.values.length > 1}>
      <svg class="spark" viewBox={`0 0 ${w} ${h}`} width={w} height={h} role="img" aria-label={props.title}>
        <title>{props.title}</title>
        <polyline points={points()} fill="none" stroke="currentColor" stroke-width="1.5" />
      </svg>
    </Show>
  );
}

/** Pills that each set one query parameter. */
export function Pills<T extends string>(props: {
  label: string;
  options: readonly (readonly [T, string])[];
  value: T;
  onPick: (v: T) => void;
}) {
  return (
    <div class="facet">
      <span class="facet-label">{props.label}</span>
      <For each={props.options}>
        {([v, text]) => (
          <button type="button" class="pill" classList={{ on: props.value === v }} aria-pressed={props.value === v} onClick={() => props.onPick(v)}>
            {text}
          </button>
        )}
      </For>
    </div>
  );
}

/** Which engine fired (`lint` = scripture-kitchen structure, `sous` = Sous Chef) and how severe. */
export function CheckBadges(props: { engine: string; severity: string }) {
  return (
    <span class="check-badges">
      <span class={`tag eng-${props.engine}`} title={props.engine === "lint" ? "scripture-kitchen structure check" : "Sous Chef proofreading"}>
        {props.engine}
      </span>{" "}
      <span class={`sev ${props.severity}`}>{props.severity}</span>
    </span>
  );
}

/** `sous:Rarity` in a URL ↔ `{engine, code}`. */
export const checkKey = (c: { engine: string; code: string }) => `${c.engine}:${c.code}`;
export const parseCheck = (s: string | undefined) => {
  const i = s?.indexOf(":") ?? -1;
  return s && i > 0 ? { engine: s.slice(0, i), code: s.slice(i + 1) } : null;
};
