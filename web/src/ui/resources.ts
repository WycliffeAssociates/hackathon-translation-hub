// Resource (brand) codes as people say them. The codes are WA's and
// unfoldingWord's; a code not listed shows as itself.
const TITLES: Record<string, string> = {
  ulb: "Unlocked Literal Bible",
  udb: "Unlocked Dynamic Bible",
  reg: "Bible",
  tn: "Translation Notes",
  tq: "Translation Questions",
  tw: "Translation Words",
  ta: "Translation Academy",
  tm: "Translation Manual",
  obs: "Open Bible Stories",
  "obs-tn": "Open Bible Stories Notes",
  "obs-tq": "Open Bible Stories Questions",
  "obs-sn": "Open Bible Stories Study Notes",
  "obs-sq": "Open Bible Stories Study Questions",
  bc: "Bible Commentary",
  ult: "unfoldingWord Literal Text",
  ust: "unfoldingWord Simplified Text",
};

/** "Translation Notes", or the code itself. */
export const resourceTitle = (code: string) => TITLES[code.toLowerCase()] ?? code;

/** The resources a language page leads with (Will, 2026-10-15); the rest fold under "Other". */
export const MAIN_RESOURCES = ["ulb", "reg", "tn", "tq", "obs"] as const;
export const isMain = (code: string) => (MAIN_RESOURCES as readonly string[]).includes(code.toLowerCase());
