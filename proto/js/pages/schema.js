import { html, raw } from "../ui.js";

// The hub's own SQLite. Forgejo keeps the code shape; this keeps the Bible
// shape and the numbers we derive from it. Sketch, not a migration.
const SECTIONS = [
  {
    title: "What Forgejo already knows, copied in",
    note: "Refreshed by the soft poll. Only public repos (is_private = 0) ever land here.",
    sql: `CREATE TABLE repo (
  id             INTEGER PRIMARY KEY,      -- Forgejo repository.id
  owner          TEXT    NOT NULL,         -- lowercase, as on disk
  name           TEXT    NOT NULL,
  default_branch TEXT    NOT NULL,
  head_sha       TEXT,                     -- NULL for an empty repo
  head_at        INTEGER,                  -- unix seconds of the HEAD commit
  shape          TEXT    NOT NULL CHECK (shape IN ('ts', 'rc', 'burrito', 'usfm', 'other')),
  manifest       TEXT,                     -- the parsed manifest, as JSON
  indexed_at     INTEGER NOT NULL,
  UNIQUE (owner, name)
) STRICT;

-- Watermarks for the soft poll: last repository.updated_unix seen, last nightly pass.
CREATE TABLE sync_state (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT, WITHOUT ROWID;`,
  },
  {
    title: "The Bible shape",
    note: "Language → Version (resource type) → Track → Book. Catalog fields (names, status) come from Port via the public data API.",
    sql: `CREATE TABLE language (
  ietf      TEXT PRIMARY KEY,               -- gl, en, byv-x-hohsieuh
  name      TEXT NOT NULL,
  native    TEXT,
  direction TEXT NOT NULL DEFAULT 'ltr' CHECK (direction IN ('ltr', 'rtl')),
  gateway   TEXT REFERENCES language (ietf)  -- the gateway it translates from
) STRICT, WITHOUT ROWID;

CREATE TABLE version (
  id            INTEGER PRIMARY KEY,
  language      TEXT NOT NULL REFERENCES language (ietf),
  resource_type TEXT NOT NULL,              -- reg, ulb, udb, ult, bible
  track         TEXT NOT NULL CHECK (track IN ('text', 'audio')),
  UNIQUE (language, resource_type, track)
) STRICT;

-- Which repo belongs to which version. A book repo has one row; a whole-Bible
-- repo also has one, and repo_book says which books it holds.
CREATE TABLE repo_version (
  repo_id    INTEGER PRIMARY KEY REFERENCES repo (id) ON DELETE CASCADE,
  version_id INTEGER NOT NULL REFERENCES version (id),
  status       TEXT CHECK (status IN ('Primary', 'Active', 'Inactive')),  -- from the catalog
  show_on_biel INTEGER NOT NULL DEFAULT 0                              -- from the catalog
) STRICT;

-- What is in the repo at HEAD, per book. Latest only.
CREATE TABLE repo_book (
  repo_id     INTEGER NOT NULL REFERENCES repo (id) ON DELETE CASCADE,
  book        TEXT    NOT NULL,             -- USFM id: GEN, 3JN
  chapters    INTEGER NOT NULL,             -- chapters present
  verses      INTEGER NOT NULL,             -- verses present
  content_key TEXT    NOT NULL,             -- blob SHA (USFM) or xxh3 of chunk SHAs (tS)
  PRIMARY KEY (repo_id, book)
) STRICT, WITHOUT ROWID;

-- The one repo per version and book. Picked by Sefer's rule unless someone overrode it.
CREATE TABLE official (
  version_id INTEGER NOT NULL REFERENCES version (id),
  book       TEXT    NOT NULL,
  repo_id    INTEGER NOT NULL REFERENCES repo (id),
  picked_by  TEXT    NOT NULL CHECK (picked_by IN ('rule', 'person')),
  picked_at  INTEGER NOT NULL,
  PRIMARY KEY (version_id, book)
) STRICT, WITHOUT ROWID;`,
  },
  {
    title: "Checks",
    note: "Keyed by content, not by repo: the same text in two repos is checked once. A new scripture-kitchen tag re-checks lazily.",
    sql: `CREATE TABLE lint_summary (
  content_key TEXT    NOT NULL,
  sk_tag      TEXT    NOT NULL,             -- scripture-kitchen tag, e.g. v0.1.10
  errors      INTEGER NOT NULL,
  warnings    INTEGER NOT NULL,
  infos       INTEGER NOT NULL,
  by_code     TEXT    NOT NULL,             -- JSON: {"verse-gap": 3, "unclosed-note": 1}
  by_chapter  TEXT    NOT NULL,             -- JSON: {"1": [0, 2, 0], …} errors/warnings/infos
  PRIMARY KEY (content_key, sk_tag)
) STRICT, WITHOUT ROWID;
-- Full findings are not stored: re-run the lint on demand (milliseconds per book).`,
  },
  {
    title: "Shown twice on BIEL",
    note: "workTracker's rule as SQL: two repos both Primary and shown for one book. (The collate rule — non-gateway languages plus ceb — is a WHERE on language.)",
    sql: `SELECT v.language, v.resource_type, v.track, b.book, count(*) AS sources
FROM repo_version rv
JOIN version v ON v.id = rv.version_id
JOIN repo_book b ON b.repo_id = rv.repo_id
WHERE rv.status = 'Primary' AND rv.show_on_biel = 1
GROUP BY v.language, v.resource_type, v.track, b.book
HAVING count(*) > 1;`,
  },
  {
    title: "Later idea: over time",
    later: true,
    flag: "Not for the first slice. Would change “the database keeps master as latest only”: repo contents stay latest-only, plus an append-only log of numbers derived from master.",
    note: "One row per repo, book and day the book changed. Backfilled once by walking master's history (last commit of each day).",
    sql: `CREATE TABLE book_day (
  repo_id     INTEGER NOT NULL REFERENCES repo (id) ON DELETE CASCADE,
  book        TEXT    NOT NULL,
  day         INTEGER NOT NULL,             -- days since 1970-01-01, UTC
  sha         TEXT    NOT NULL,             -- last master commit that day
  chapters    INTEGER NOT NULL,
  verses      INTEGER NOT NULL,
  content_key TEXT    NOT NULL,             -- joins lint_summary: burn-down for free
  commits     INTEGER NOT NULL,             -- activity signal only; Writer commits per save
  PRIMARY KEY (repo_id, book, day)
) STRICT, WITHOUT ROWID;

-- Status changes, from the day we start recording. Git does not know them.
CREATE TABLE status_change (
  repo_id INTEGER NOT NULL REFERENCES repo (id) ON DELETE CASCADE,
  at      INTEGER NOT NULL,
  status  TEXT,
  PRIMARY KEY (repo_id, at)
) STRICT, WITHOUT ROWID;

-- Per version per day, rebuilt nightly from book_day and official. What charts read.
CREATE TABLE version_day (
  version_id     INTEGER NOT NULL REFERENCES version (id),
  day            INTEGER NOT NULL,
  books_official INTEGER NOT NULL,
  chapters       INTEGER NOT NULL,
  verses         INTEGER NOT NULL,
  errors         INTEGER NOT NULL,
  warnings       INTEGER NOT NULL,
  PRIMARY KEY (version_id, day)
) STRICT, WITHOUT ROWID;`,
  },
  {
    title: "Later idea: questions it answers",
    later: true,
    note: "Plain SQL against the over-time tables.",
    sql: `-- Velocity: verses added per week for one version's official repos.
SELECT day / 7 AS week, max(verses) - min(verses) AS verses_added
FROM version_day WHERE version_id = ? GROUP BY week ORDER BY week;

-- Burn-down: lint errors in one book over time.
SELECT d.day, l.errors, l.warnings
FROM book_day d JOIN lint_summary l ON l.content_key = d.content_key AND l.sk_tag = ?
WHERE d.repo_id = ? AND d.book = ? ORDER BY d.day;

-- Stalled: official books with no change in 180 days.
SELECT o.version_id, o.book, max(d.day) AS last_change
FROM official o JOIN book_day d ON d.repo_id = o.repo_id AND d.book = o.book
GROUP BY o.version_id, o.book
HAVING last_change < unixepoch() / 86400 - 180;`,
  },
];

export function schemaPage() {
  return html`
    <h1>Suggested schema</h1>
    <p class="lede">The hub's own SQLite. Forgejo keeps the code shape; this keeps the Bible shape and the numbers derived from it. A sketch to argue over, not a migration.</p>
    <nav class="toc small">${SECTIONS.map((s, i) => html`<a href="#/schema" data-jump="s${i}">${s.title}</a>`)}</nav>
    ${SECTIONS.map((s, i) => {
      const body = html`<h2>${s.title}</h2>
        ${s.flag ? html`<p class="flag">${s.flag}</p>` : ""}
        <p class="small">${s.note}</p>
        <pre class="code sql">${raw(highlight(s.sql))}</pre>`;
      return s.later
        ? html`<section id="s${i}" class="wire"><span class="wire-tag">later idea · mocked</span>${body}</section>`
        : html`<section id="s${i}">${body}</section>`;
    })}
    <h2>Later idea: over time, drawn</h2>
    <div class="grid2">
      ${spark("Burn-down · errors in a book", [42, 40, 40, 31, 28, 28, 19, 12, 12, 7, 3, 1], "errors")}
      ${spark("Velocity · verses per week", [0, 120, 80, 210, 190, 40, 0, 0, 260, 300, 180, 220], "verses")}
    </div>`;
}

// Mocked: made-up numbers, drawn as SVG on the server or here. Shows the shape only.
function spark(title, values, unit) {
  const w = 300, h = 80, max = Math.max(...values);
  const pts = values.map((v, i) => `${(i / (values.length - 1)) * w},${h - (v / max) * (h - 8) - 4}`).join(" ");
  return html`<section class="wire"><span class="wire-tag">mocked · made-up numbers</span>
    <h3>${title}</h3>
    <svg viewBox="0 0 ${w} ${h}" class="spark" role="img" aria-label="${title}"><polyline points="${pts}" fill="none" stroke="currentColor" stroke-width="2"/></svg>
    <p class="small">${values[0]} → ${values.at(-1)} ${unit} over 12 weeks</p></section>`;
}

// Just enough colour to read SQL: comments, keywords, strings.
function highlight(sql) {
  const esc = sql.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);
  return esc
    .split("\n")
    .map((line) => {
      const at = line.indexOf("--");
      const code = at === -1 ? line : line.slice(0, at);
      const comment = at === -1 ? "" : `<span class="c">${line.slice(at)}</span>`;
      return (
        code
          .replace(/'[^']*'/g, (s) => `<span class="s">${s}</span>`)
          .replace(/\b(CREATE TABLE|PRIMARY KEY|REFERENCES|NOT NULL|UNIQUE|CHECK|DEFAULT|ON DELETE CASCADE|WITHOUT ROWID|STRICT|INTEGER|TEXT|SELECT|FROM|JOIN|WHERE|GROUP BY|ORDER BY|HAVING|AND|AS|ON|IN|NULL)\b/g, '<span class="k">$1</span>') + comment
      );
    })
    .join("\n");
}
