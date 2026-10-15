-- Sources by book (proposal): copies of a book a publisher has set aside, so
-- the book's options show only the ones worth weighing (a merged repo that
-- repeats the Writer projects, an abandoned try). Shared by every publisher
-- of the language; the hub's own state, nothing on Forgejo changes.
-- `book` = '*' sets the repo aside for every book it holds.
CREATE TABLE source_ignore (
  language    TEXT    NOT NULL,
  resource    TEXT    NOT NULL,
  book        TEXT    NOT NULL,              -- USFM id, or '*'
  repo        TEXT    NOT NULL,              -- owner/name, lower case
  username    TEXT    NOT NULL,
  at          INTEGER NOT NULL,
  PRIMARY KEY (language, resource, book, repo)
) STRICT, WITHOUT ROWID;
