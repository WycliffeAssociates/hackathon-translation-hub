// The chart frame every chart in the hub uses (Observable Plot 0.6.17): it
// draws `make(width)` again when its box is resized, and with `onPick` a click
// opens the datum under the pointer (Plot sets the figure's `value` to it for
// any mark with a tooltip).
import { createEffect, createSignal, onCleanup, type JSX } from "solid-js";

/**
 * Draws `make(width)` into the card, again when the card is resized. With
 * `onPick`, a click opens the datum under the pointer: Plot sets the figure's
 * `value` to it for any mark with a tooltip (`tip: true`).
 */
export function Chart<T>(props: { make: (width: number) => (SVGSVGElement | HTMLElement) | null; onPick?: (datum: T) => void }) {
  let host!: HTMLDivElement;
  const [width, setWidth] = createSignal(0);
  const ro = new ResizeObserver(([e]) => setWidth(Math.floor(e!.contentRect.width)));
  createEffect(() => {
    ro.observe(host);
    onCleanup(() => ro.disconnect());
  });
  createEffect(() => {
    const w = width();
    if (!w) return;
    const el = props.make(w);
    const pick = props.onPick;
    if (el && pick) {
      el.classList.add("pickable");
      el.addEventListener("click", () => {
        const v = (el as { value?: T | null }).value;
        if (v != null) pick(v);
      });
    }
    host.replaceChildren(...(el ? [el] : []));
  });
  return <div class="viz-chart" ref={host} />;
}

export function Card(props: { n: number; title: string; asks: string; source: string; children: JSX.Element }) {
  return (
    <section class="viz-card" id={`viz-${props.n}`} data-tour={`viz-${props.n}`}>
      <header>
        <h3>
          <span class="viz-n">{props.n}</span> {props.title}
        </h3>
        <p class="viz-asks">{props.asks}</p>
      </header>
      {props.children}
      <p class="viz-source small">Data: {props.source}</p>
    </section>
  );
}

export const time = (unix: number) => new Date(unix * 1000);
export const CANON = "GEN EXO LEV NUM DEU JOS JDG RUT 1SA 2SA 1KI 2KI 1CH 2CH EZR NEH EST JOB PSA PRO ECC SNG ISA JER LAM EZK DAN HOS JOL AMO OBA JON MIC NAM HAB ZEP HAG ZEC MAL MAT MRK LUK JHN ACT ROM 1CO 2CO GAL EPH PHP COL 1TH 2TH 1TI 2TI TIT PHM HEB JAS 1PE 2PE 1JN 2JN 3JN JUD REV".split(" ");
const canonIndex = (b: string) => {
  const i = CANON.indexOf(b);
  return i < 0 ? 99 : i;
};
export const byCanon = (a: string, b: string) => canonIndex(a) - canonIndex(b);

