-- A fourth write: consolidating a language's Writer projects into a new
-- official repo, every save replayed (crates/history). SQLite cannot change a
-- CHECK in place, so the log is rebuilt with the new kind allowed.
CREATE TABLE action_new (
  id          INTEGER PRIMARY KEY,
  at          INTEGER NOT NULL,
  username    TEXT    NOT NULL,
  kind        TEXT    NOT NULL CHECK (kind IN ('adopt', 'pull', 'publish', 'consolidate')),
  target      TEXT    NOT NULL,
  source      TEXT,
  book        TEXT,
  ref         TEXT,
  detail      TEXT
) STRICT;
INSERT INTO action_new SELECT id, at, username, kind, target, source, book, ref, detail FROM action;
DROP TABLE action;
ALTER TABLE action_new RENAME TO action;
CREATE INDEX action_target ON action (target, at);
CREATE INDEX action_source ON action (source);
