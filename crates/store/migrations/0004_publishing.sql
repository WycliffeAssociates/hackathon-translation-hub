-- Publishing (proposal): who is signed in, and what the hub wrote to Forgejo
-- on whose behalf. Forgejo stays the record of the writes themselves (repos,
-- commits with Source:/Pulled-by: trailers, release tags); this is the hub's
-- log of them, so a page can say who adopted or published what, and when.

-- A signed-in person. The id is the cookie (random, 32 bytes hex). Whether
-- they may publish is read from Forgejo's team at sign-in and kept until the
-- session ends, so a removal takes effect at the next sign-in.
CREATE TABLE session (
  id          TEXT    PRIMARY KEY,
  username    TEXT    NOT NULL,
  full_name   TEXT,
  avatar_url  TEXT,
  publisher   INTEGER NOT NULL CHECK (publisher IN (0, 1)),
  created     INTEGER NOT NULL,
  expires     INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

-- One write the hub made to Forgejo.
CREATE TABLE action (
  id          INTEGER PRIMARY KEY,
  at          INTEGER NOT NULL,
  username    TEXT    NOT NULL,              -- who asked (the service account wrote)
  kind        TEXT    NOT NULL CHECK (kind IN ('adopt', 'pull', 'publish')),
  target      TEXT    NOT NULL,              -- official owner/name
  source      TEXT,                          -- owner/name@sha it came from (adopt, pull)
  book        TEXT,                          -- pull: the book
  ref         TEXT,                          -- commit sha (pull) or tag (publish)
  detail      TEXT                           -- JSON: anything else worth keeping
) STRICT;

CREATE INDEX action_target ON action (target, at);
CREATE INDEX action_source ON action (source);
