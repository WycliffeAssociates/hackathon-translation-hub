import { api } from "../../api/client";
import { waitFor, type Tour } from "../machine";

/**
 * The demo (first draft, 2026-10-16): one language, Bemba, from all of WACS
 * down to one verse, then what a publisher does with it. Bemba because the
 * same repos are on WACS-local (dev-data) and on wacsdev, so every stop works
 * on both; only "published" differs (WACS-local has TextTranslation/bem_reg,
 * wacsdev has none yet), and that stop falls back to the language page.
 *
 * Start: the header's "Tour", `/?tour=demo`, or `/?tour=demo&step=checks`.
 */
const LANGUAGE = "bem";
const WRITER = "/repos/felix_sichies/bem_jhn_text_reg";

/** The published repo for Bemba REG, if there is one on this hub. */
const published = async (): Promise<string | undefined> => {
  const rows = await api.json("/v1/languages/{ietf}/published", { path: { ietf: LANGUAGE } }).catch(() => []);
  return rows.find((p) => p.resource === "reg")?.published?.repo;
};

/** Opens a disclosure unless it is open already (a step may be re-entered). */
const expand = async (selector: string): Promise<void> => {
  const el = await waitFor(selector);
  if (el.getAttribute("aria-expanded") !== "true") el.click();
};

export const demoTour: Tour = {
  id: "demo",
  title: "Translation Hub",
  steps: [
    {
      id: "start",
      title: "Every translation on WACS, in one place",
      body: "The hub reads every public repo on WACS and shows what is in it: books, chapters, verses, and what the checks found. It changes nothing on WACS.",
      go: ({ navigate }) => navigate("/"),
      more: {
        title: "Why",
        body: () => (
          <ul>
            <li>WACS holds tens of thousands of repos. Forgejo shows them one at a time, as files.</li>
            <li>The hub reads Forgejo's repos off disk and its repo list read-only, and keeps its own database.</li>
            <li>Translators keep working where they do now. The hub is for the people who look across their work.</li>
          </ul>
        ),
      },
    },
    {
      id: "status",
      title: "All of it, scanned",
      body: "Every public repo scanned, every book linted and proofread. After the first pass the hub only rereads a repo when its HEAD moves.",
      go: ({ navigate }) => navigate("/status"),
      anchor: ".stats",
    },
    {
      id: "language",
      title: "One language: Bemba",
      body: "A language page leads with what is published: one official repo for each resource. Below it, every repo that holds Bemba.",
      go: ({ navigate }) => navigate(`/languages/${LANGUAGE}`),
      anchor: ".published-list",
    },
    {
      id: "sources",
      title: "Sources, book by book",
      body: "Bemba REG is spread over dozens of Writer projects. Here each book shows where it comes from and whether the published copy still matches.",
      go: ({ navigate }) => navigate(`/languages/${LANGUAGE}/sources?resource=reg`),
      anchor: ".sources-books",
    },
    {
      id: "variants",
      title: "One book, every copy",
      body: "A book opens to every copy of it on WACS. Identical copies are grouped; where copies differ, the hub shows the verses that differ. A publisher picks one, and nothing is merged by guesswork.",
      go: async ({ navigate }) => {
        navigate(`/languages/${LANGUAGE}/sources?resource=reg`);
        await expand("[data-tour=book-LEV] .sources-row");
      },
      anchor: "[data-tour=book-LEV]",
    },
    {
      id: "progress",
      title: "Progress against the source",
      body: "Verses translated against a gateway source, book by book, so the gaps are visible before anyone opens a file.",
      go: ({ navigate }) => navigate(`/languages/${LANGUAGE}/sources?resource=reg`),
      anchor: ".progress",
    },
    {
      id: "writer",
      title: "A Writer project, read as a book",
      body: "Writer saves chunks, not books. The hub stitches them into one USFM book, the way Sefer does, and says that this is not a real repo.",
      go: ({ navigate }) => navigate(WRITER),
      anchor: ".synthetic-note",
    },
    {
      id: "checks",
      title: "Checks, beside the text",
      body: "Lint and proofreading findings in one list. A finding opens its verse, with the source text next to it on the same verse.",
      go: async ({ navigate }) => {
        navigate(`${WRITER}?tab=checks&check=sous:Doubled`);
        await expand("[data-tour=check-Doubled]");
        // Open the first finding's verse, unless one is open already.
        const group = ".rail-group:has([data-tour=check-Doubled])";
        if (!document.querySelector(`${group} .rail-items button.on`)) (await waitFor(`${group} .rail-items button`)).click();
      },
      anchor: ".checks-view",
      timeoutMs: 15_000,
    },
    {
      id: "published",
      title: "The published repo",
      body: "The official Bemba REG, made from the Writer projects in one step. Up front: what changed since the last release, and which books differ from what they follow.",
      go: async ({ navigate }) => {
        const repo = await published();
        navigate(repo ? `/repos/${repo}` : `/languages/${LANGUAGE}`);
      },
      anchor: ".repo-state, .published-list",
    },
    {
      id: "repo-progress",
      title: "Its progress",
      body: "Which chapters are there against the source, how the work grew over time, and which verses run long or short.",
      go: async ({ navigate }) => navigate(`/repos/${(await published()) ?? WRITER.slice("/repos/".length)}?tab=progress`),
      anchor: ".coverage-head",
      timeoutMs: 15_000,
    },
    {
      id: "releases",
      title: "A release is the publish",
      body: "A commit is never a publish. A release is: a date tag, a checksum file and a manifest, so apps can fetch exactly that version.",
      go: async ({ navigate }) => {
        const repo = await published();
        navigate(repo ? `/repos/${repo}?tab=releases` : `/languages/${LANGUAGE}`);
      },
      anchor: ".releases-tab, .published-list",
    },
    {
      id: "sefer",
      title: "Edit in Sefer",
      body: "The hub does not edit. Any repo opens in Sefer, the editor, on the same text.",
      go: ({ navigate }) => navigate(WRITER),
      anchor: ".page-head .facts",
    },
    {
      id: "charts",
      title: "Across WACS",
      body: "The same data across every repo: when work happened, in which languages, and how much of it.",
      go: async ({ navigate }) => {
        navigate("/status");
        await waitFor(".stats");
        // The charts load on demand; press "Show the charts" if it is still there.
        [...document.querySelectorAll("main button")].find((b) => b.textContent?.includes("Show the charts"))?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      },
      anchor: "[data-tour=viz-9]",
      ready: () => document.querySelector("[data-tour=viz-9] svg") !== null,
      timeoutMs: 30_000,
    },
    {
      id: "next",
      title: "What is next",
      body: "Placeholder for Will: what this replaces, who uses it first, and what we need to decide.",
      go: ({ navigate }) => navigate("/"),
    },
  ],
};
