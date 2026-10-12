// The hub's shape over flat catalog rows:
//   Language → Version (resource type: reg, ulb, …) → Track (text, audio) → Book → candidate repos
// One candidate per book becomes Official, by the same rule Sefer uses
// (src/core/migrate/candidate.ts `defaultCandidate`).
import { BOOKS, bookByUsfm, bookFromRepoName } from "./books.js";

export const RESOURCE_NAMES = {
  reg: "Regular",
  ulb: "Unlocked Literal Bible",
  udb: "Unlocked Dynamic Bible",
  ult: "Unlocked Literal Text",
  bible: "Bible",
};

export const statusOf = (row) => row.wa_content_metadata?.status ?? null;
export const repoPath = (row) => (row.git_repo ? `${row.git_repo.username}/${row.git_repo.repo_name}` : null);

const RANK = { Primary: 0, Active: 1, null: 2, Inactive: 3 };
/** Primary first, Inactive last, newest first within each. */
export const byStatus = (a, b) => RANK[statusOf(a)] - RANK[statusOf(b)] || String(b.modified_on).localeCompare(String(a.modified_on));

/**
 * Sefer's rule: the only candidate; else the Primary; else the newest Active;
 * else the newest unlabelled. Inactive never wins against another candidate.
 */
export function officialOf(candidates) {
  if (candidates.length === 1) return candidates[0];
  const newest = (rows) => rows.toSorted((a, b) => String(b.modified_on).localeCompare(String(a.modified_on)))[0];
  const primaries = candidates.filter((c) => statusOf(c) === "Primary");
  if (primaries.length) return newest(primaries);
  const active = candidates.filter((c) => statusOf(c) === "Active");
  if (active.length) return newest(active);
  const unlabelled = candidates.filter((c) => !statusOf(c));
  return unlabelled.length ? newest(unlabelled) : null;
}

/**
 * One cell's state on a coverage shelf.
 *   official — an Official repo exists and is labelled Primary
 *   draft    — work exists but nothing is Primary yet
 *   conflict — more than one Primary claims the book
 *   inactive — only Inactive repos
 *   empty    — nothing
 */
export function bookState(candidates) {
  if (!candidates?.length) return "empty";
  const primaries = candidates.filter((c) => statusOf(c) === "Primary").length;
  if (primaries > 1) return "conflict";
  if (primaries === 1) return "official";
  if (candidates.every((c) => statusOf(c) === "Inactive")) return "inactive";
  return "draft";
}

/** Group a language's rows into versions → tracks → books. */
export function versionsOf(rows) {
  const versions = new Map();
  for (const row of rows) {
    const rtype = row.resource_type ?? "unknown";
    const track = row.type ?? "text";
    if (!versions.has(rtype)) versions.set(rtype, { rtype, name: RESOURCE_NAMES[rtype] ?? rtype, tracks: new Map() });
    const tracks = versions.get(rtype).tracks;
    if (!tracks.has(track)) tracks.set(track, { track, books: new Map(), whole: [] });
    const t = tracks.get(track);
    const add = (book) => (t.books.get(book.slug) ?? t.books.set(book.slug, []).get(book.slug)).push(row);
    const book = bookFromRepoName(row.git_repo?.repo_name ?? row.name.split("/").pop());
    if (book) add(book);
    else {
      // Whole-Bible repo: a candidate for each book the catalog rendered from it.
      t.whole.push(row);
      for (const id of row.books ?? []) {
        const b = bookByUsfm(id);
        if (b) add(b);
      }
    }
  }
  return [...versions.values()].sort((a, b) => sizeOf(b) - sizeOf(a));
}

const sizeOf = (v) => [...v.tracks.values()].reduce((n, t) => n + t.books.size, 0);

export function trackSummary(track) {
  const counts = { official: 0, draft: 0, conflict: 0, inactive: 0, empty: 0 };
  for (const b of BOOKS) counts[bookState(track.books.get(b.slug))]++;
  return counts;
}
