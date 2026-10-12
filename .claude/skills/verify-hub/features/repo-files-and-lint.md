# A repo's files and checks

**What:** a consolidated repo (RC, `NN-BOOK.usfm`) shows its file tree, each
file's bytes, and per book: lint counts by severity and code, chapters and
verses, and coverage against its declared source (`wa-catalog/<lang>_<id>`).

**Reach:** `GET /v1/repos/wa-catalog/en_ulb`, `/tree`, `/file/41-MAT.usfm`; page `/repos/wa-catalog/en_ulb`.

**Drive:** open the page, pick `41-MAT.usfm` in the tree, switch the viewer to
USFM mode, jump to a chapter.

**End state:** 66 books, each with a `lint` object; `/tree` lists `41-MAT.usfm`;
the file starts with `\id MAT`; the viewer shows the text with inline
diagnostics and changes between regular and USFM modes. A repo with a source
(e.g. a Writer `*_ulb` book) shows `coverage.present / source_verses`.
