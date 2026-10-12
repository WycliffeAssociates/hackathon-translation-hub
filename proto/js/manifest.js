// Just enough manifest reading to find a repo's book, language and declared
// source. The hub does this in Rust (the old dashboard has full tS, RC and
// Burrito parsers); here it is a sketch over the three shapes.

export const MANIFEST_FILES = ["manifest.json", "manifest.yaml", "metadata.json"];

/**
 * { kind, language, book, resource, sources: [{language, resource}], finishedChunks, error }
 * `sources` are what the manifest declares, unresolved.
 */
export function parseManifest(path, text) {
  try {
    if (path === "manifest.json") return tsManifest(JSON.parse(text));
    if (path === "metadata.json") return burrito(JSON.parse(text));
    if (path === "manifest.yaml") return rcManifest(text);
  } catch (e) {
    return { kind: kindOf(path), error: e.message, sources: [] };
  }
  return null;
}

const kindOf = (path) => ({ "manifest.json": "tS", "manifest.yaml": "RC", "metadata.json": "Burrito" })[path];

function tsManifest(m) {
  // Some manifests write source_translations as a map instead of an array.
  const raw = m.source_translations ?? [];
  const list = Array.isArray(raw) ? raw : Object.values(raw);
  return {
    kind: "tS",
    language: m.target_language?.id ?? null,
    book: m.project?.id ?? null,
    resource: m.resource?.id ?? null,
    sources: list.filter((s) => s.language_id && s.resource_id).map((s) => ({ language: s.language_id, resource: s.resource_id })),
    finishedChunks: Array.isArray(m.finished_chunks) ? m.finished_chunks : null,
  };
}

function burrito(m) {
  // Burrito has no source list; relationships of type "source" carry an opaque target.
  const rels = (m.relationships ?? []).filter((r) => r.type === "source" && r.target);
  return {
    kind: "Burrito",
    language: m.languages?.[0]?.tag ?? null,
    book: null,
    resource: null,
    sources: rels.map((r) => ({ opaque: String(r.target) })),
    finishedChunks: null,
  };
}

/**
 * RC `dublin_core` without a YAML library: the fields we need sit at fixed
 * indents. `source:` is a list of `{identifier, language, version}`.
 */
function rcManifest(text) {
  const field = (re) => text.match(re)?.[1]?.trim().replace(/^['"]|['"]$/g, "") ?? null;
  const sources = [];
  const block = text.match(/\n\s+source:\s*\n((?:\s+-[^\n]*\n(?:\s{4,}[^\n]*\n)*)+)/);
  if (block) {
    for (const item of block[1].split(/\n\s+-\s/)) {
      const id = item.match(/identifier:\s*['"]?([\w-]+)/)?.[1];
      const lang = item.match(/language:\s*['"]?([\w-]+)/)?.[1];
      if (id && lang) sources.push({ language: lang, resource: id });
    }
  }
  return {
    kind: "RC",
    language: field(/\n\s+language:\s*\n\s+(?:direction:[^\n]*\n\s+)?identifier:\s*([^\n]+)/),
    book: null,
    resource: field(/\n\s{2}identifier:\s*([^\n]+)/),
    sources,
    finishedChunks: null,
  };
}

/**
 * The convention: a source (language, resource) lives at
 * `WA-Catalog/<language>_<resource>`, else `Door43-Catalog/…`.
 * `exists(owner, repo)` resolves to repo meta or throws.
 */
export async function resolveSource(sources, exists) {
  for (const s of sources) {
    if (!s.language) continue;
    for (const owner of ["WA-Catalog", "Door43-Catalog"]) {
      const repo = `${s.language}_${s.resource}`;
      try {
        const meta = await exists(owner, repo);
        if (!meta.empty) return { owner, repo, ref: meta.default_branch, declared: s };
      } catch {
        // not there; try the next owner
      }
    }
  }
  return null;
}
