import { bookBySlug } from "./books.js";

/**
 * Which shape a repo's file list has. The real hub reads the manifest off disk;
 * here the file names are enough to pick a stitch.
 *   ts   — translationStudio: `01/01.txt` chunk files, `manifest.json`
 *   usfm — Resource Container or loose `.usfm`/`.sfm` files
 */
export function repoShape(paths) {
  if (paths.some((p) => /\.(usfm|sfm)$/i.test(p))) return "usfm";
  if (paths.some((p) => /^\d+\/\d+\.txt$/.test(p))) return "ts";
  return "other";
}

/** tS chunk files in reading order, chapter then chunk, numerically. */
export function tsChunkPaths(paths) {
  return paths
    .map((p) => p.match(/^(\d+)\/(\d+)\.txt$/))
    .filter(Boolean)
    .map((m) => ({ path: m[0], chapter: Number(m[1]), chunk: Number(m[2]) }))
    .sort((a, b) => a.chapter - b.chapter || a.chunk - b.chunk);
}

// --- Writer (tS) → one USFM book ---------------------------------------------
// Ported from Sefer, src/core/migrate/writerBook.ts (`isWriterTextPath`,
// `treeFromFiles`, `writerBookToUsfm`, `usfmFileName`). Keep in step with it;
// the hub should share one implementation (a scripture-kitchen recipe).

const numbered = (name) => (/^\d{2,3}$/.test(name) && Number(name) > 0 ? Number(name) : undefined);

/** `front/title.txt`, and each chapter's `NN/title.txt` and `NN/NN.txt` chunks. */
export const isWriterTextPath = (path) => path === "front/title.txt" || /^\d{2,3}\/(?:title|\d{2,3})\.txt$/i.test(path);

/** The book's parts from its files by path (Map<path, text>). */
export function treeFromFiles(files) {
  const title = (files.get("front/title.txt") ?? "").split(/\r?\n/).map((l) => l.trim()).filter((l) => l !== "");
  const byChapter = new Map();
  for (const [path, text] of files) {
    const match = /^(\d{2,3})\/(title|\d{2,3})\.txt$/i.exec(path);
    const chapter = match === null ? undefined : numbered(match[1] ?? "");
    if (match === null || chapter === undefined) continue;
    const held = byChapter.get(chapter) ?? { label: undefined, chunks: [] };
    if (match[2]?.toLowerCase() === "title") held.label = text.trim();
    else {
      const chunk = numbered(match[2] ?? "");
      if (chunk !== undefined) held.chunks.push([chunk, text]);
    }
    byChapter.set(chapter, held);
  }
  const chapters = [...byChapter.entries()]
    .filter(([, held]) => held.chunks.length > 0)
    .sort(([a], [b]) => a - b)
    .map(([number, held]) => ({
      number,
      label: held.label === undefined || held.label === "" || held.label === String(number) ? undefined : held.label,
      chunks: held.chunks.sort(([a], [b]) => a - b).map(([, text]) => text),
    }));
  return { title, chapters };
}

// Writer sometimes saved the chapter marker twice (`\c 3 \c 3 \v 1`); it is framing, not text.
const withoutChapterMarker = (chunk) => chunk.replace(/^(?:\s*\\c\s+\d+\s*)+/, "");

/** Sefer's template: txt2USFM's header, then per chapter `\c`, `\cl` when labelled, then the chunks. */
export function writerBookToUsfm(tree, book, fallbackTitle) {
  const title = tree.title.length > 0 ? tree.title : [fallbackTitle];
  const head = title[0] ?? fallbackTitle;
  const lines = [`\\id ${book}`, "\\ide UTF-8", `\\h ${head}`, `\\toc1 ${head}`, `\\toc2 ${head}`, `\\toc3 ${book.toLowerCase()}`, ...title.map((line, i) => `\\mt${i + 1} ${line}`)];
  for (const chapter of tree.chapters) {
    lines.push(`\\c ${chapter.number}`);
    if (chapter.label !== undefined) lines.push(`\\cl ${chapter.label}`);
    chapter.chunks.forEach((chunk, i) => {
      const text = (i === 0 ? withoutChapterMarker(chunk) : chunk).trim();
      if (text !== "") lines.push(text);
    });
  }
  return `${lines.join("\n")}\n`;
}

/** `44-JHN.usfm`: numbered as the catalogue numbers books (MAT is 41). */
export const usfmFileName = (index, usfm) => `${String(index < 39 ? index + 1 : index + 2).padStart(2, "0")}-${usfm}.usfm`;
