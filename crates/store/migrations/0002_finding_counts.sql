-- Findings as counts first, rows second. Nobody reads 300k rows; what the
-- pages need is "which checks fire, how often, in which books", sorted. Rows
-- are fetched only for one book and one check, when someone opens them.

-- One row per repo, engine, book and check: the count at HEAD. Lint rows are
-- written by each scan, sous rows by each proofreading run. Summing over
-- repos gives the hub-wide view (which rule is noisy where).
CREATE TABLE finding_count (
  repo_id   INTEGER NOT NULL REFERENCES repo (id) ON DELETE CASCADE,
  engine    TEXT    NOT NULL CHECK (engine IN ('lint', 'sous')),
  book      TEXT    NOT NULL,            -- USFM id; the file path when it has none
  code      TEXT    NOT NULL,            -- lint: verse-gap … · sous: Rarity …
  severity  TEXT    NOT NULL,            -- lint: error/warning/info · sous: hygiene/convention
  n         INTEGER NOT NULL,
  PRIMARY KEY (repo_id, engine, book, code)
) STRICT, WITHOUT ROWID;

CREATE INDEX finding_count_code ON finding_count (engine, code);

-- Sous Chef's rows, kept per book and check up to a cap (the server's
-- PROOF_ROWS_PER_BOOK_CODE), so every book can show its own findings even when
-- one check fires 300k times. Lint rows are not stored: the viewer's engine
-- computes them for the open book.
CREATE TABLE proof_finding (
  repo_id   INTEGER NOT NULL REFERENCES repo (id) ON DELETE CASCADE,
  book      TEXT    NOT NULL,
  code      TEXT    NOT NULL,
  seq       INTEGER NOT NULL,            -- order within (book, code): position in the book
  body      TEXT    NOT NULL,            -- JSON: one hub-proof Finding
  PRIMARY KEY (repo_id, book, code, seq)
) STRICT, WITHOUT ROWID;

-- proof.body now holds only the run's books and skipped files; the rows moved
-- to proof_finding. Old bodies are dropped so every repo is proofread again.
DELETE FROM proof;

-- Lint counts are written by scans, so mark every repo for one rescan.
UPDATE repo SET scanned_tag = NULL;
