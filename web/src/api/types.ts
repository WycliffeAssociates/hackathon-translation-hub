// Names for the generated OpenAPI types (src/api/schema.d.ts, from `pnpm gen`).
import type { components } from "./schema";

type S = components["schemas"];
export type RepoSummary = S["RepoSummary"];
export type RepoPage = S["RepoPage"];
export type BookRow = S["BookRow"];
export type LintRow = S["LintRow"];
export type CoverageRow = S["CoverageRow"];
export type CatalogRef = S["CatalogRef"];
export type ProofSummary = S["ProofSummary"];
export type RepoTree = S["RepoTree"];
export type TreeEntry = S["TreeEntry"];
export type LanguageSummary = S["LanguageSummary"];
export type LanguageDetail = S["LanguageDetail"];
export type BookPick = S["BookPick"];
export type BielConflict = S["BielConflict"];
export type HistoryPoint = S["HistoryPoint"];
export type Totals = S["Totals"];

export type RepoDetail = S["RepoDetail"];
export type ManifestInfo = S["ManifestInfo"];
export type SyntheticInfo = S["SyntheticInfo"];
export type ProofPage = S["ProofPage"];
export type Finding = S["Finding"];
export type PairedEvidence = S["PairedEvidence"];
export type CodeCount = S["CodeCount"];
export type HubCode = S["HubCode"];
export type RepoCodeCount = S["RepoCodeCount"];
