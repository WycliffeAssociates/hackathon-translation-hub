-- Where each book of an official repo came from (proposal): the pointer the
-- sources page compares with to say "current", "behind" or "edited". The
-- hub's own state, not a file in the repo: the published repo carries only
-- the text and its manifest, and every pull commit still names its source in
-- a `Source:` trailer. Written by pull and consolidate.
CREATE TABLE official_link (
  target      TEXT    NOT NULL,              -- the official repo, owner/name, lower case
  book        TEXT    NOT NULL,              -- USFM id
  repo        TEXT    NOT NULL,              -- the copy it follows, owner/name, lower case
  commit_sha  TEXT    NOT NULL,              -- that copy's commit when it was taken
  username    TEXT    NOT NULL,
  at          INTEGER NOT NULL,
  PRIMARY KEY (target, book)
) STRICT, WITHOUT ROWID;
