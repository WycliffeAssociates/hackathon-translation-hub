-- The languages a signed-in person works in (proposal): what their home page
-- (/welcome) is about. The hub's own state, by Forgejo username.
CREATE TABLE my_language (
  username    TEXT    NOT NULL,
  ietf        TEXT    NOT NULL,
  at          INTEGER NOT NULL,
  PRIMARY KEY (username, ietf)
) STRICT, WITHOUT ROWID;
