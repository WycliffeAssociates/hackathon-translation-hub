import { waitFor, type Tour } from "../machine";

/**
 * PLACEHOLDER: proves the machine on three real screens (a centred card with
 * a "more" modal, a chart that renders late, a DOM effect after navigating).
 * The real stops and their why come from Will; replace these then.
 *
 * Start: `/?tour=demo`, or `/?tour=demo&step=verse` to jump in.
 */
export const demoTour: Tour = {
  id: "demo",
  title: "Translation Hub",
  steps: [
    {
      id: "start",
      title: "What the hub is",
      body: "Placeholder. One sentence on what the person is looking at.",
      go: ({ navigate }) => navigate("/"),
      more: {
        title: "Why",
        body: () => (
          <p>
            Placeholder. The longer reason, for an audience that asks. Any JSX: a list, a link, a
            figure.
          </p>
        ),
      },
    },
    {
      id: "charts",
      title: "Verse length against the source",
      body: "Placeholder. Each dot is a verse; the far ones are worth a look. A dot opens its verse.",
      go: ({ navigate }) => navigate("/viz"),
      anchor: "[data-tour=viz-5]",
      // The card is there before Plot has drawn into it.
      ready: () => document.querySelector("[data-tour=viz-5] svg") !== null,
    },
    {
      id: "verse",
      title: "A verse much longer than its source",
      body: "Placeholder. The length check, opened beside the verse it found.",
      go: async ({ navigate }) => {
        navigate("/repos/wa-catalog/ru_ulb?file=01-GEN.usfm&v=31:50");
        // A DOM effect: open the check's group, unless it is already open.
        const group = await waitFor("[data-tour=check-LengthProportionality]");
        if (group.getAttribute("aria-expanded") !== "true") group.click();
      },
      anchor: "[data-tour=check-LengthProportionality]",
    },
  ],
};
