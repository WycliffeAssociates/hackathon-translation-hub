/**
 * The step machine a guided tour runs on. Ported from Sefer's
 * `src/app/tour/machine.ts` (branch `r-links`, 95fedfd), cut to what a demo
 * needs: no workflow kind, no list of tours, no command palette. Added here:
 * a table of contents to jump to any step, pause and resume, and a step's
 * "more" modal.
 *
 * A step is, in order:
 *
 *   go      take the app to the step: usually navigate, then any DOM effect
 *           (click a toggle, open a group). May be async; `waitFor` helps.
 *   ready   wait until the step can be shown. Defaults to "the anchor is on
 *           screen". Polled every 100 ms, so it reads a signal or the DOM
 *           equally well.
 *   card    a ring around the anchor and a card beside it (centred with no
 *           anchor), with an optional "more" modal for the longer why.
 *
 * Because the table of contents, a resume and `?step=` jump straight to a
 * step, a step's `go` must take the app there FROM ANYWHERE: only that
 * step's `go` runs, never the ones before it. A URL that carries the whole
 * state (`?file=&v=&check=`) is the easy way; a DOM effect after it should
 * check before it toggles (open only what is closed).
 *
 * Steps reach the app through `TourContext` and the DOM, never the other way
 * round: a screen knows a tour exists only by carrying a `data-tour` handle.
 */

import type { JSX } from "solid-js";
import { createSignal } from "solid-js";

/** What a step may reach. */
export interface TourContext {
  readonly navigate: (href: string) => void;
}

/** The longer explanation a card can open: one modal, filled per step. */
export interface More {
  readonly title: string;
  readonly body: () => JSX.Element;
}

export interface Step {
  /** Stable, for `?step=<id>`; the table of contents shows `title`. */
  readonly id: string;
  readonly title: string;
  /** The card's text: what the person is looking at. */
  readonly body: string;
  /** A CSS selector; the first match is ringed. Prefer `[data-tour=…]`. None: a centred card. */
  readonly anchor?: string;
  /** Where the card sits against the anchor. Default: below, or above when there is no room. */
  readonly side?: "below" | "right";
  readonly more?: More;
  readonly go?: (context: TourContext) => void | Promise<void>;
  readonly ready?: (context: TourContext) => boolean;
  /** How long `go` plus `ready` may take before the card says the step drifted. */
  readonly timeoutMs?: number;
}

export interface Tour {
  readonly id: string;
  readonly title: string;
  readonly steps: readonly Step[];
}

export type Phase = "waiting" | "showing" | "timedOut" | "paused";

export interface Running {
  readonly tour: Tour;
  readonly index: number;
  readonly phase: Phase;
}

const [running, setRunningSignal] = createSignal<Running | undefined>(undefined, { equals: false });
export const runningTour = running;

/*
 * The place is kept in sessionStorage, so a reload in the middle of a demo
 * (or a link opened in the same tab) comes back paused at the same step
 * instead of losing the tour. Per tab; a convenience, so it fails silently.
 */
const KEPT = "hub.tour";
const setRunning = (next: Running | undefined): void => {
  setRunningSignal(next);
  try {
    if (next === undefined) sessionStorage.removeItem(KEPT);
    else sessionStorage.setItem(KEPT, JSON.stringify({ tour: next.tour.id, index: next.index }));
  } catch {
    // Storage blocked: the tour still runs, it just does not survive a reload.
  }
};

/** A tour kept from before a reload, put back paused at its step. */
export const restoreTour = (byId: (id: string) => Tour | undefined): void => {
  try {
    const kept = JSON.parse(sessionStorage.getItem(KEPT) ?? "null") as { tour: string; index: number } | null;
    const tour = kept === null ? undefined : byId(kept.tour);
    if (tour !== undefined && kept !== null && tour.steps[kept.index] !== undefined)
      setRunningSignal({ tour, index: kept.index, phase: "paused" });
  } catch {
    // Nothing kept, or storage blocked.
  }
};

/** The modal: one, mounted by the outlet; anything may fill and open it. */
const [more, setMore] = createSignal<More | undefined>(undefined);
export const shownMore = more;
export const showMore = (content: More | undefined): void => {
  setMore(content);
};

const READY_TIMEOUT_MS = 10_000;
const POLL_MS = 100;

let poll: ReturnType<typeof setInterval> | undefined;
const stopPolling = (): void => {
  if (poll !== undefined) clearInterval(poll);
  poll = undefined;
};

/** Whether a selector matches; an invalid one simply does not. */
export const anchored = (selector: string): Element | null => {
  try {
    return document.querySelector(selector);
  } catch {
    return null;
  }
};

/**
 * The first element matching `selector` once it exists, for a `go` that
 * clicks after it navigates. Rejects after `ms`, which the step counts as a
 * miss, not an error.
 */
export const waitFor = (selector: string, ms = 8_000): Promise<HTMLElement> =>
  new Promise((resolve, reject) => {
    const started = Date.now();
    const look = (): void => {
      const found = anchored(selector);
      if (found instanceof HTMLElement) resolve(found);
      else if (Date.now() - started > ms) reject(new Error(`nothing matches ${selector}`));
      else setTimeout(look, POLL_MS);
    };
    look();
  });

/*
 * Tours FAIL OPEN (as in Sefer). They are written against screens that are
 * still changing, so nothing a step does may stop the tour or the app: a
 * `go` or `ready` that throws counts as "not ready", a step never ready
 * times out to a card that says so and still offers Back, Next and the
 * steps, and every miss is a warning naming the tour, step and anchor.
 */
const drifted = (tour: Tour, step: Step, why: string, error?: unknown): void => {
  console.warn("tour.step failed", { tour: tour.id, step: step.id, anchor: step.anchor, why, error });
};

/** Jumps to step `index` (clamped): runs its `go`, then waits for `ready`. */
export const goTo = (context: TourContext, tour: Tour, index: number): void => {
  stopPolling();
  showMore(undefined);
  const at = Math.min(Math.max(0, index), tour.steps.length - 1);
  const step = tour.steps[at];
  if (step === undefined) return stopTour();
  setRunning({ tour, index: at, phase: "waiting" });
  const current = (): boolean => {
    const now = running();
    return now !== undefined && now.tour === tour && now.index === at && now.phase === "waiting";
  };
  const started = Date.now();
  let reported = false;
  const ready = (): boolean => {
    try {
      return (step.ready ?? (() => step.anchor === undefined || anchored(step.anchor) !== null))(context);
    } catch (error) {
      if (!reported) drifted(tour, step, "ready threw", error);
      reported = true;
      return false;
    }
  };
  const watch = (): void => {
    if (!current()) return;
    poll = setInterval(() => {
      if (!current()) return stopPolling();
      if (ready()) {
        stopPolling();
        setRunning({ tour, index: at, phase: "showing" });
      } else if (Date.now() - started > (step.timeoutMs ?? READY_TIMEOUT_MS)) {
        stopPolling();
        drifted(tour, step, "never became ready");
        setRunning({ tour, index: at, phase: "timedOut" });
      }
    }, POLL_MS);
  };
  // `go` first, then watch: a step that clicks after navigating is ready
  // only once the click has happened. A `go` that fails still watches, so
  // the step shows if the app happens to be there anyway.
  Promise.resolve()
    .then(() => step.go?.(context))
    .catch((error: unknown) => drifted(tour, step, "go failed", error))
    .finally(watch);
};

export const startTour = (context: TourContext, tour: Tour, index = 0): void => goTo(context, tour, index);

export const stepBy = (context: TourContext, delta: 1 | -1): void => {
  const now = running();
  if (now !== undefined) goTo(context, now.tour, now.index + delta);
};

/** Hides the ring and card and keeps the place; resume re-runs the step's `go`. */
export const pauseTour = (): void => {
  const now = running();
  if (now === undefined) return;
  stopPolling();
  showMore(undefined);
  setRunning({ ...now, phase: "paused" });
};

export const resumeTour = (context: TourContext): void => {
  const now = running();
  if (now !== undefined) goTo(context, now.tour, now.index);
};

export const stopTour = (): void => {
  stopPolling();
  showMore(undefined);
  setRunning(undefined);
};

/** A step by its id or its 1-based number, as `?step=` gives it. */
export const stepIndex = (tour: Tour, step: string | null): number => {
  if (step === null) return 0;
  const byId = tour.steps.findIndex((s) => s.id === step);
  if (byId >= 0) return byId;
  const n = Number.parseInt(step, 10);
  return Number.isFinite(n) ? n - 1 : 0;
};
