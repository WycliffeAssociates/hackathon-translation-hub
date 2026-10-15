-- What is published for a language and resource (proposal): a pointer to one
-- repo, chosen by a publisher. It may be a repo the hub made in the official
-- org, or an existing repo blessed as it is (wa-catalog/pt-br_ulb), with no
-- copy made. With no row, the official org's {ietf}_{resource} counts when it
-- exists. The hub's own state; nothing on Forgejo changes.
CREATE TABLE published (
  language    TEXT    NOT NULL,
  resource    TEXT    NOT NULL,
  repo        TEXT    NOT NULL,              -- owner/name, lower case
  username    TEXT    NOT NULL,
  at          INTEGER NOT NULL,
  PRIMARY KEY (language, resource)
) STRICT, WITHOUT ROWID;
