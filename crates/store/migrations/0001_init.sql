-- The hub's own SQLite. Forgejo keeps the code shape; this keeps the Bible
-- shape and the numbers derived from it. A proposal, not a settled schema:
-- every table is rebuilt from the repos, Forgejo's DB and the public data API,
-- except book_history, which only grows from the day the hub starts.

-- What Forgejo knows about each repo, copied in by the soft poll, plus what the
-- last scan found at HEAD. Only public repos are ever written here; a repo that
-- turns private is deleted (and its rows cascade).
CREATE TABLE repo (
  id              INTEGER PRIMARY KEY,          -- Forgejo repository.id
  owner           TEXT    NOT NULL,             -- lowercase, as on disk
  name            TEXT    NOT NULL,
  default_branch  TEXT,
  topics          TEXT    NOT NULL DEFAULT '[]',-- JSON array, e.g. ["consolidated"]
  is_mirror       INTEGER NOT NULL DEFAULT 0,
  is_archived     INTEGER NOT NULL DEFAULT 0,
  updated_unix    INTEGER NOT NULL,             -- Forgejo's, bumped on push
  -- The last scan. NULL scanned_unix: never scanned.
  scanned_unix    INTEGER,
  scanned_updated INTEGER,                      -- updated_unix the scan saw; differs → rescan
  scanned_tag     TEXT,                         -- scripture-kitchen tag; differs → rescan
  head_branch     TEXT,
  head_sha        TEXT,                         -- NULL for an empty repo
  head_unix       INTEGER,                      -- committer time of HEAD
  shape           TEXT CHECK (shape IN ('ts', 'rc', 'burrito', 'usfm', 'other', 'empty')),
  language        TEXT,                         -- from the manifest, lowercase
  resource        TEXT,                         -- manifest identifier: ulb, reg, tn
  subject         TEXT,                         -- Bible, Translation Notes, …
  title           TEXT,
  manifest        TEXT,                         -- JSON: the scan's manifest report
  synthetic       TEXT,                         -- JSON: set when served as a synthetic repo
  scan_error      TEXT,
  UNIQUE (owner, name)
) STRICT;

CREATE INDEX repo_language ON repo (language, resource);
CREATE INDEX repo_recent ON repo (head_unix DESC);

-- What is in the repo at HEAD, per book. Latest only.
CREATE TABLE repo_book (
  repo_id         INTEGER NOT NULL REFERENCES repo (id) ON DELETE CASCADE,
  book            TEXT    NOT NULL,             -- USFM id: GEN, 3JN ('' when unreadable)
  path            TEXT    NOT NULL,             -- file in the repo (or the synthetic repo)
  content_key     TEXT    NOT NULL,             -- blob sha (USFM) or hash of chunk shas (Writer)
  chapters        INTEGER NOT NULL,
  verses          INTEGER NOT NULL,
  -- Coverage against the declared source; NULL when none resolves.
  source_repo     TEXT,                         -- owner/name, e.g. wa-catalog/en_ulb
  source_verses   INTEGER,
  present         INTEGER,
  missing         INTEGER,
  extra           INTEGER,
  gaps            TEXT,                         -- JSON: {"3": [4, 5]}
  PRIMARY KEY (repo_id, path)
) STRICT, WITHOUT ROWID;

CREATE INDEX repo_book_content ON repo_book (content_key);

-- Lint is keyed by content, not by repo: the same text in two repos is checked
-- once, and a new scripture-kitchen tag re-checks lazily.
CREATE TABLE lint_summary (
  content_key     TEXT    NOT NULL,
  sk_tag          TEXT    NOT NULL,
  errors          INTEGER NOT NULL,
  warnings        INTEGER NOT NULL,
  infos           INTEGER NOT NULL,
  by_code         TEXT    NOT NULL,             -- JSON: {"verse-gap": 3}
  by_chapter      TEXT    NOT NULL,             -- JSON: {"1": [0, 2, 0]} errors/warnings/infos
  PRIMARY KEY (content_key, sk_tag)
) STRICT, WITHOUT ROWID;

-- Sous Chef findings are corpus-relative (a form is odd against the rest of
-- the repo), so they are keyed by repo and HEAD rather than by book content.
CREATE TABLE proof (
  repo_id         INTEGER NOT NULL REFERENCES repo (id) ON DELETE CASCADE,
  head_sha        TEXT    NOT NULL,
  sk_tag          TEXT    NOT NULL,
  ran_unix        INTEGER NOT NULL,
  millis          INTEGER NOT NULL,
  findings        INTEGER NOT NULL,
  by_book         TEXT    NOT NULL,             -- JSON: {"MAT": 12}
  by_rule         TEXT    NOT NULL,             -- JSON: {"rare-form": 40}
  body            TEXT    NOT NULL,             -- JSON: the findings, as the API serves them
  error           TEXT,
  PRIMARY KEY (repo_id)
) STRICT;

-- Language facts from the public data API (vw_langnames + language).
CREATE TABLE language (
  ietf            TEXT PRIMARY KEY,             -- lowercase
  name            TEXT,                         -- in the language
  english_name    TEXT,
  direction       TEXT,
  region          TEXT,
  gateway         INTEGER,                      -- 1 gateway, 0 heart, NULL unknown
  alternate_names TEXT NOT NULL DEFAULT '[]',   -- JSON array; searched, not drawn
  fetched_unix    INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

-- Catalog rows from the public data API that point at a WACS repo. The
-- catalog, not git, says Primary / show_on_biel.
CREATE TABLE catalog_content (
  id              TEXT PRIMARY KEY,             -- content.id
  owner           TEXT,                         -- lowercase, joins repo
  name            TEXT,
  language        TEXT,
  resource_type   TEXT,                         -- reg, ulb, …
  track           TEXT,                         -- content.type: text, audio
  domain          TEXT,                         -- scripture, gloss, …
  status          TEXT,                         -- Primary, Active, Inactive, or NULL
  show_on_biel    INTEGER NOT NULL DEFAULT 0,
  modified_on     TEXT,
  books           TEXT NOT NULL DEFAULT '[]',   -- JSON: book slugs it renders, upper-case
  fetched_unix    INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

CREATE INDEX catalog_repo ON catalog_content (owner, name);

-- Append-only: one row each time a book's content or the kitchen tag changes.
-- The burn-down and velocity charts read this; nothing else depends on it.
CREATE TABLE book_history (
  repo_id         INTEGER NOT NULL REFERENCES repo (id) ON DELETE CASCADE,
  book            TEXT    NOT NULL,
  at_unix         INTEGER NOT NULL,             -- when the hub saw it
  head_sha        TEXT    NOT NULL,
  head_unix       INTEGER,
  content_key     TEXT    NOT NULL,
  sk_tag          TEXT    NOT NULL,
  chapters        INTEGER NOT NULL,
  verses          INTEGER NOT NULL,
  errors          INTEGER NOT NULL,
  warnings        INTEGER NOT NULL,
  missing         INTEGER,
  PRIMARY KEY (repo_id, book, at_unix)
) STRICT, WITHOUT ROWID;

-- Watermarks: when the catalog was last fetched, the last poll, and so on.
CREATE TABLE sync_state (
  key             TEXT PRIMARY KEY,
  value           TEXT NOT NULL
) STRICT, WITHOUT ROWID;
