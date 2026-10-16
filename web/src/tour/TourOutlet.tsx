import { useLocation, useNavigate } from "@solidjs/router";
import { For, Show, createEffect, createSignal, on, onCleanup } from "solid-js";

import {
  anchored,
  goTo,
  pauseTour,
  restoreTour,
  resumeTour,
  runningTour,
  showMore,
  shownMore,
  startTour,
  stepBy,
  stepIndex,
  stopTour,
  type Running,
  type TourContext,
} from "./machine";
import { tourById } from "./tours";

/**
 * The one place a tour is drawn, mounted once in the layout so a tour can
 * cross routes: a ring around the step's anchor, a card beside it, the
 * paused pill, and the one modal a step's "more" fills. Ported from Sefer's
 * `src/app/tour/TourOutlet.tsx` (branch `r-links`, 95fedfd).
 *
 * `?tour=<id>` on any URL starts that tour, and `&step=<id or n>` starts it
 * there; both are then taken off the address so a reload does not restart
 * it. A reload mid-tour comes back paused at the same step (`restoreTour`).
 * Keys while a step shows: → next, ← back, Escape pauses.
 */

interface Box {
  readonly top: number;
  readonly left: number;
  readonly width: number;
  readonly height: number;
}

const RING_PAD = 6;
const CARD_GAP = 12;
const CARD_WIDTH = 340;
const CENTRED = { top: "20vh", left: `calc(50vw - ${CARD_WIDTH / 2}px)` };

export function TourOutlet() {
  const navigate = useNavigate();
  const location = useLocation();
  const context: TourContext = { navigate: (href) => navigate(href) };
  restoreTour(tourById);

  createEffect(
    on(
      () => location.search,
      (search) => {
        const query = new URLSearchParams(search);
        const id = query.get("tour");
        if (id === null) return;
        const at = query.get("step");
        query.delete("tour");
        query.delete("step");
        navigate(`${location.pathname}${query.size ? `?${query}` : ""}`, { replace: true });
        const tour = tourById(id);
        if (tour === undefined) console.warn(`no tour called ${id}`);
        else startTour(context, tour, stepIndex(tour, at));
      },
    ),
  );

  const step = () => {
    const now = runningTour();
    return now?.tour.steps[now.index];
  };
  const [toc, setToc] = createSignal(false);

  // Where the anchor is, measured every frame only while a ringed step
  // shows: the page under it scrolls, resizes and re-renders.
  const [box, setBox] = createSignal<Box | undefined>(undefined);
  createEffect(() => {
    const now = runningTour();
    const anchor = now?.phase === "showing" ? step()?.anchor : undefined;
    if (anchor === undefined) return setBox(undefined);
    let scrolled = false;
    let frame = 0;
    const measure = (): void => {
      const element = anchored(anchor);
      if (element === null) setBox(undefined);
      else {
        if (!scrolled) {
          scrolled = true;
          // A tall anchor (a whole list) shows from its top, not its middle.
          const tall = element.getBoundingClientRect().height > window.innerHeight * 0.5;
          element.scrollIntoView({ block: tall ? "start" : "center", inline: "nearest" });
        }
        const r = element.getBoundingClientRect();
        const held = box();
        if (!held || held.top !== r.top || held.left !== r.left || held.width !== r.width || held.height !== r.height)
          setBox({ top: r.top, left: r.left, width: r.width, height: r.height });
      }
      frame = requestAnimationFrame(measure);
    };
    frame = requestAnimationFrame(measure);
    onCleanup(() => cancelAnimationFrame(frame));
  });

  // The page's own keys win: a key something else handled, or one typed
  // into a field or the viewer, is not the tour's.
  const onKeyDown = (event: KeyboardEvent): void => {
    const now = runningTour();
    if (now === undefined || now.phase === "paused" || event.defaultPrevented || shownMore()) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest("input, textarea, select, [contenteditable=true], .cm-editor")) return;
    if (event.key === "ArrowRight") stepBy(context, 1);
    else if (event.key === "ArrowLeft") stepBy(context, -1);
    else if (event.key === "Escape") pauseTour();
    else return;
    event.preventDefault();
  };
  document.addEventListener("keydown", onKeyDown);
  onCleanup(() => document.removeEventListener("keydown", onKeyDown));

  /** Below the anchor when there is room, else above; or to its right. */
  const cardStyle = (held: Box): Record<string, string> => {
    // An anchor taller than half the screen leaves no room above or below:
    // the card sits inside the screen, at its right edge.
    if (held.height > window.innerHeight * 0.5)
      return {
        top: `${Math.min(Math.max(CARD_GAP + 48, held.top + CARD_GAP), window.innerHeight * 0.5)}px`,
        right: `${CARD_GAP * 2}px`,
      };
    if (step()?.side === "right")
      return {
        top: `${Math.max(CARD_GAP, held.top)}px`,
        left: `${Math.min(held.left + held.width + RING_PAD + CARD_GAP, window.innerWidth - CARD_WIDTH - CARD_GAP)}px`,
      };
    const left = Math.min(
      Math.max(CARD_GAP, held.left + held.width / 2 - CARD_WIDTH / 2),
      window.innerWidth - CARD_WIDTH - CARD_GAP,
    );
    return held.top + held.height < window.innerHeight * 0.6
      ? { top: `${held.top + held.height + RING_PAD + CARD_GAP}px`, left: `${left}px` }
      : { bottom: `${window.innerHeight - held.top + RING_PAD + CARD_GAP}px`, left: `${left}px` };
  };

  const card = (now: Running, title: string, body: string, style: Record<string, string>) => {
    const last = now.index === now.tour.steps.length - 1;
    const s = now.tour.steps[now.index];
    return (
      <div role="dialog" aria-label={title} class="tour-card" style={{ width: `${CARD_WIDTH}px`, ...style }}>
        <div class="tour-card-head">
          <h2>{title}</h2>
          <button type="button" class="tour-at" aria-expanded={toc()} title="Steps" onClick={() => setToc(!toc())}>
            {now.index + 1} of {now.tour.steps.length} {toc() ? "▴" : "▾"}
          </button>
        </div>
        <Show when={toc()}>
          <ol class="tour-toc">
            <For each={now.tour.steps}>
              {(x, i) => (
                <li>
                  <button
                    type="button"
                    aria-current={i() === now.index ? "step" : undefined}
                    onClick={() => {
                      setToc(false);
                      goTo(context, now.tour, i());
                    }}
                  >
                    {x.title}
                  </button>
                </li>
              )}
            </For>
          </ol>
        </Show>
        <p>{body}</p>
        <Show when={s?.more}>
          {(m) => (
            <button type="button" class="tour-more" onClick={() => showMore(m())}>
              {m().title} ›
            </button>
          )}
        </Show>
        <div class="tour-buttons">
          <button type="button" onClick={pauseTour} title="Escape">
            Pause
          </button>
          <span>
            <Show when={now.index > 0}>
              <button type="button" onClick={() => stepBy(context, -1)} title="←">
                Back
              </button>
            </Show>
            <button type="button" class="primary" onClick={() => (last ? stopTour() : stepBy(context, 1))} title="→">
              {last ? "Done" : "Next"}
            </button>
          </span>
        </div>
      </div>
    );
  };

  let dialog: HTMLDialogElement | undefined;
  createEffect(() => {
    if (!dialog) return;
    if (shownMore() && !dialog.open) dialog.showModal();
    else if (!shownMore() && dialog.open) dialog.close();
  });

  return (
    <>
      {/* Keyed: rebuilt on every move of the machine, so a card never shows
          a step it has left. Only the ring's box changes in place. */}
      <Show when={runningTour()} keyed>
        {(now) => (
          <div class="tour-outlet" data-tour-outlet={now.tour.id}>
            <Show when={now.phase === "showing" && now.tour.steps[now.index]} keyed>
              {(s) => (
                // Fail open: an anchor that vanishes after the step showed
                // leaves the card centred, still readable and dismissable.
                <Show when={s.anchor !== undefined && box()} fallback={card(now, s.title, s.body, CENTRED)}>
                  {(held) => (
                    <>
                      <div
                        aria-hidden="true"
                        class="tour-ring"
                        style={{
                          top: `${held().top - RING_PAD}px`,
                          left: `${held().left - RING_PAD}px`,
                          width: `${held().width + RING_PAD * 2}px`,
                          height: `${held().height + RING_PAD * 2}px`,
                        }}
                      />
                      {card(now, s.title, s.body, cardStyle(held()))}
                    </>
                  )}
                </Show>
              )}
            </Show>
            <Show when={now.phase === "waiting"}>
              <div class="tour-pill small loading">{now.tour.steps[now.index]?.title}</div>
            </Show>
            <Show when={now.phase === "timedOut"}>
              {card(
                now,
                "This step could not be shown",
                `“${now.tour.steps[now.index]?.title}” points at something not on screen; the app has probably changed since the tour was written. Next skips it.`,
                CENTRED,
              )}
            </Show>
            <Show when={now.phase === "paused"}>
              <div class="tour-pill">
                <span class="small">
                  {now.tour.title} · paused at {now.index + 1} of {now.tour.steps.length}
                </span>
                <button type="button" class="primary" onClick={() => resumeTour(context)}>
                  Resume
                </button>
                <button type="button" onClick={stopTour} aria-label="End tour">
                  ×
                </button>
              </div>
            </Show>
          </div>
        )}
      </Show>
      <dialog ref={dialog} class="tour-modal" onClose={() => showMore(undefined)}>
        <Show when={shownMore()}>
          {(m) => (
            <>
              <h2>{m().title}</h2>
              <div class="tour-modal-body">{m().body()}</div>
              <form method="dialog">
                <button type="submit">Close</button>
              </form>
            </>
          )}
        </Show>
      </dialog>
    </>
  );
}
