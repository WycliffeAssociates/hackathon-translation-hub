// The two outside sources this prototype stands in for. The real hub reads
// Forgejo's repos off disk and its own SQLite; here the public data API (BIEL
// GraphQL) plays the catalog and the WACS REST API plays the disk.
import { bookFromRepoName } from "./books.js";

export const GQL = "https://api.bibleineverylanguage.org/v1/graphql";
export const WACS = "https://content.bibletranslationtools.org";

const memo = new Map();
const cached = (key, load) => {
  if (!memo.has(key)) memo.set(key, load().catch((e) => (memo.delete(key), Promise.reject(e))));
  return memo.get(key);
};

export class SourceError extends Error {
  constructor(source, status, detail) {
    super(`${source} answered ${status}${detail ? `: ${detail}` : ""}`);
    this.source = source;
    this.status = status;
  }
}

async function gql(query, variables) {
  const res = await fetch(GQL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ query, variables }),
  });
  if (!res.ok) throw new SourceError("Public data API", res.status);
  const body = await res.json();
  if (body.errors) throw new SourceError("Public data API", "an error", body.errors[0].message);
  return body.data;
}

const CONTENT_FIELDS = `name type resource_type title modified_on
  git_repo { username repo_name }
  wa_content_metadata { status show_on_biel }`;

export const search = (q) =>
  cached(`search:${q}`, () =>
    gql(
      `query($q: String!) {
        language(where: {_or: [{ietf_code: {_ilike: $q}}, {english_name: {_ilike: $q}}, {national_name: {_ilike: $q}}],
                         contents: {domain: {_eq: "scripture"}}}, limit: 25, order_by: {english_name: asc}) {
          ietf_code english_name national_name
          contents(where: {domain: {_eq: "scripture"}}) { type resource_type wa_content_metadata { status } }
        }
        content(where: {name: {_ilike: $q}, domain: {_eq: "scripture"}}, limit: 20, order_by: {modified_on: desc}) {
          ${CONTENT_FIELDS} language { ietf_code english_name }
        }
      }`,
      { q: `%${q}%` },
    ),
  );

/** Most recently changed scripture repos, newest first, with optional facets. */
export const recent = ({ rt, track, status, offset = 0, limit = 60 }) =>
  cached(`recent:${rt}:${track}:${status}:${offset}`, async () => {
    const where = { domain: { _eq: "scripture" } };
    if (rt) where.resource_type = { _eq: rt };
    if (track) where.type = { _eq: track };
    if (status) where.wa_content_metadata = { status: { _eq: status } };
    const data = await gql(
      `query($where: content_bool_exp!, $offset: Int!, $limit: Int!) {
        content(where: $where, order_by: {modified_on: desc_nulls_last}, offset: $offset, limit: $limit) {
          ${CONTENT_FIELDS} language { ietf_code english_name }
        }
      }`,
      { where, offset, limit },
    );
    return data.content;
  });

export const language = (ietf) =>
  cached(`lang:${ietf}`, async () => {
    const data = await gql(
      `query($ietf: String!) {
        language(where: {ietf_code: {_eq: $ietf}}) {
          ietf_code english_name national_name direction
          gateway_languages { gateway_language_ietf }
          dependent_languages_aggregate { aggregate { count } }
          wa_language_metadata { is_gateway }
        }
        content(where: {language: {ietf_code: {_eq: $ietf}}, domain: {_eq: "scripture"}}, order_by: {modified_on: desc}) {
          ${CONTENT_FIELDS}
        }
      }`,
      { ietf },
    );
    // A whole-Bible repo's name says nothing about its books. The catalog
    // knows what it rendered from each repo, so ask it, for those rows only.
    const whole = data.content.filter((r) => !bookFromRepoName(r.git_repo?.repo_name ?? r.name)).map((r) => r.name);
    const books = whole.length ? await booksRendered(whole) : new Map();
    for (const row of data.content) if (books.has(row.name)) row.books = books.get(row.name);
    return { language: data.language[0] ?? null, content: data.content };
  });

async function booksRendered(names) {
  const data = await gql(
    `query($n: [String!]) {
      content(where: {name: {_in: $n}}) {
        name
        rendered_contents(where: {file_type: {_eq: "json"}, scriptural_rendering_metadata: {is_whole_book: {_eq: true}}}) {
          scriptural_rendering_metadata { book_slug }
        }
      }
    }`,
    { n: names },
  );
  return new Map(data.content.map((c) => [c.name, c.rendered_contents.map((r) => r.scriptural_rendering_metadata.book_slug)]));
}

export const contentForRepo = (owner, repo) =>
  cached(`repo-row:${owner}/${repo}`, async () => {
    const data = await gql(
      `query($o: String!, $r: String!) {
        content(where: {git_repo: {username: {_ilike: $o}, repo_name: {_ilike: $r}}}, limit: 1) {
          ${CONTENT_FIELDS} language { ietf_code english_name national_name direction }
        }
      }`,
      { o: owner, r: repo },
    );
    return data.content[0] ?? null;
  });

// --- WACS (Gitea) -----------------------------------------------------------
// WACS sits behind Cloudflare: `/raw` and archives are challenged, and bursts
// over ~100 requests get a 429. Blobs by SHA pass, so files come that way, a
// few at a time.

let inFlight = 0;
const queue = [];
const LIMIT = 4;
async function throttled(fn) {
  if (inFlight >= LIMIT) await new Promise((go) => queue.push(go));
  inFlight++;
  try {
    return await fn();
  } finally {
    inFlight--;
    queue.shift()?.();
  }
}

async function wacs(path) {
  return throttled(async () => {
    const res = await fetch(`${WACS}/api/v1${path}`);
    if (res.status === 429) throw new SourceError("WACS", 429, "rate limited — wait a minute and retry");
    if (!res.ok) throw new SourceError("WACS", res.status);
    const type = res.headers.get("content-type") ?? "";
    if (!type.includes("json")) throw new SourceError("WACS", res.status, "Cloudflare challenge page");
    return res.json();
  });
}

export const repoMeta = (owner, repo) => cached(`meta:${owner}/${repo}`, () => wacs(`/repos/${owner}/${repo}`));

export const repoTree = (owner, repo, ref) =>
  cached(`tree:${owner}/${repo}@${ref}`, async () => {
    const out = [];
    // Gitea pages a recursive tree; 1000 entries a page covers every book repo.
    for (let page = 1; ; page++) {
      const t = await wacs(`/repos/${owner}/${repo}/git/trees/${ref}?recursive=1&per_page=1000&page=${page}`);
      out.push(...(t.tree ?? []));
      if (!t.truncated || !t.tree?.length) return out;
    }
  });

const utf8 = new TextDecoder();
export const blobText = (owner, repo, sha) =>
  cached(`blob:${sha}`, async () => {
    const b = await wacs(`/repos/${owner}/${repo}/git/blobs/${sha}`);
    const bin = atob(b.content.replace(/\n/g, ""));
    return utf8.decode(Uint8Array.from(bin, (c) => c.charCodeAt(0)));
  });
