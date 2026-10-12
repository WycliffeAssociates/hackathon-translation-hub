//! A Writer (translationStudio) repo, served as if it were a consolidated one.
//!
//! The hub treats consolidated repos (an RC with `NN-BOOK.usfm` files) as first
//! class. A Writer repo is one book split into chunk files, so the hub gives it
//! a *synthetic* repo: the files Sefer's assemble would write for that one book,
//! built on the fly from the repo at a commit, never stored in Forgejo.
//!
//! - `NN-BOOK.usfm`: the chunks stitched with Sefer's template ([`hub_stitch`]).
//! - `manifest.yaml`: Sefer's `resourceContainerText` shape (bwc_reg's), with
//!   Sefer's `prefillForm` defaults: title "Bible", rights CC-BY-SA-4.0,
//!   checking level "1". The values match; YAML quoting may differ from Sefer's.
//! - `.upstreams.json`: Sefer's format, pointing back at the Writer repo and
//!   the commit the synthetic repo was built from.
//!
//! Pages and APIs redirect a Writer repo here; later, Sefer's assemble turns a
//! synthetic repo into a real one. Sources: Sefer `src/core/migrate/assemble.ts`,
//! `src/core/resources/writeMetadata.ts`, `src/core/migrate/upstreams.ts`
//! (commit 50aa9cd); keep in step with them.

use hub_manifest::{ContainerFormat, NormalizedManifest, find_manifest, parse_as};
use hub_repo_read::{EntryKind, RepoReader};
use hub_stitch::{
    CANON, decode_text, is_writer_text_path, tree_from_files, usfm_file_name, writer_book_to_usfm,
};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Repo(#[from] hub_repo_read::Error),
    #[error("manifest: {0}")]
    Manifest(#[from] hub_manifest::ManifestError),
    #[error("yaml: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Where the Writer repo lives, for `.upstreams.json`.
pub struct Upstream<'a> {
    /// Clone URL, e.g. `https://content.bibletranslationtools.org/owner/repo.git`.
    pub url: &'a str,
    /// The commit the synthetic repo is built from.
    pub commit: &'a str,
}

/// One book's synthetic repo.
#[derive(Debug, Clone, Serialize)]
pub struct SyntheticRepo {
    /// Upper-case USFM id: `3JN`.
    pub book: String,
    /// The book's name, from the `\h` the template wrote.
    pub title: String,
    /// `manifest.yaml`, `NN-BOOK.usfm`, `.upstreams.json`, in that order.
    pub files: Vec<SyntheticFile>,
    /// The Writer files the book was stitched from, as (path, blob sha), so a
    /// caller can key caches on them (`hub_checks::content_key_for_chunks`).
    pub chunks: Vec<(String, String)>,
    /// Chapters that have at least one chunk.
    pub chapters: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyntheticFile {
    pub path: String,
    pub text: String,
    pub kind: FileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    /// Stitched from the repo's chunks.
    Stitched,
    /// Written from the manifest and the commit.
    Generated,
}

impl SyntheticRepo {
    pub fn usfm(&self) -> &SyntheticFile {
        self.files
            .iter()
            .find(|f| f.kind == FileKind::Stitched)
            .expect("a synthetic repo always has its book")
    }
}

/// The synthetic repo for a Writer repo at `commit`, or `None` when the repo
/// is not a Writer *scripture* project for one of the 66 books (an RC, a
/// Burrito, Writer notes or questions, tA, OBS…).
pub fn synthesize(
    reader: &RepoReader,
    commit: &str,
    upstream: &Upstream,
) -> Result<Option<SyntheticRepo>> {
    let root: Vec<String> = reader
        .root_files(commit)?
        .into_iter()
        .map(|e| e.path)
        .collect();
    let Some((name, ContainerFormat::TsManifest)) = find_manifest(&root) else {
        return Ok(None);
    };
    let Some(raw) = reader.file(commit, name)? else {
        return Ok(None);
    };
    let manifest = parse_as(ContainerFormat::TsManifest, &raw)?.normalize();
    // Writer also writes notes and questions (`tn`, `tq`) book by book, in
    // markdown. Only scripture becomes a synthetic Bible repo. Sefer does not
    // need this check (its candidates come from a list of Bibles); the hub
    // scans everything, so it does: subject Bible and USFM chunks, not
    // markdown. Sefer's own rule: a target language.
    let has_language = manifest
        .languages
        .first()
        .is_some_and(|l| !l.tag.trim().is_empty());
    let is_usfm = manifest.content_format.as_deref() == Some("text/usfm");
    if manifest.subject.as_deref() != Some("Bible") || !is_usfm || !has_language {
        return Ok(None);
    }
    let Some(book) = manifest.book_ids().first().map(|b| b.to_ascii_uppercase()) else {
        return Ok(None);
    };
    let Some(book_file) = usfm_file_name(&book) else {
        return Ok(None);
    };

    // The book's files, read and decoded the way Sefer reads them; a file that
    // is not UTF-8 is left out, as Sefer leaves it out.
    let mut chunks = Vec::new();
    let mut texts = Vec::new();
    for entry in reader.tree(commit)? {
        if entry.kind != EntryKind::Blob || !is_writer_text_path(&entry.path) {
            continue;
        }
        if let Some(text) = decode_text(&reader.blob(entry.oid)?) {
            chunks.push((entry.path.clone(), entry.oid.to_string()));
            texts.push((entry.path, text));
        }
    }
    let tree = tree_from_files(texts.iter().map(|(p, t)| (p.as_str(), t.as_str())));
    let chapters = tree.chapters.iter().map(|c| c.number).collect();
    let fallback = manifest.title.clone().unwrap_or_else(|| book.clone());
    let usfm = writer_book_to_usfm(&tree, &book, &fallback);
    let title = usfm
        .lines()
        .find_map(|l| l.strip_prefix("\\h ").map(|t| t.trim().to_string()))
        .unwrap_or(fallback);

    let date = date_of(reader.commit_time(commit)?);
    let translators = translators(&raw);
    let manifest_yaml =
        resource_container(&manifest, &book, &title, &book_file, &translators, &date)?;
    let upstreams = upstreams_json(&book, upstream, &date);

    Ok(Some(SyntheticRepo {
        book,
        title,
        files: vec![
            SyntheticFile {
                path: "manifest.yaml".into(),
                text: manifest_yaml,
                kind: FileKind::Generated,
            },
            SyntheticFile {
                path: book_file,
                text: usfm,
                kind: FileKind::Stitched,
            },
            SyntheticFile {
                path: ".upstreams.json".into(),
                text: upstreams,
                kind: FileKind::Generated,
            },
        ],
        chunks,
        chapters,
    }))
}

/// Writer's `translators`: usually names, sometimes objects with a name.
/// The normalized manifest drops them, so read them from the raw JSON.
fn translators(raw: &[u8]) -> Vec<String> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(
        raw.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(raw),
    ) else {
        return Vec::new();
    };
    let names = value["translators"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| {
            t.as_str()
                .map(str::to_string)
                .or_else(|| t["name"].as_str().map(str::to_string))
        });
    dedupe_contributors(names)
}

/// Sefer's `dedupeContributors`: merged on a key that folds case, reads `_`
/// as a space and collapses whitespace; the first spelling seen is kept.
fn dedupe_contributors(names: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    names
        .filter(|name| {
            let key = name
                .replace('_', " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            !key.is_empty() && seen.insert(key)
        })
        .collect()
}

#[derive(Serialize)]
struct Rc<'a> {
    dublin_core: DublinCore<'a>,
    checking: Checking,
    projects: Vec<Project<'a>>,
}

#[derive(Serialize)]
struct DublinCore<'a> {
    conformsto: &'static str,
    contributor: &'a [String],
    creator: &'static str,
    format: &'static str,
    identifier: &'a str,
    issued: &'a str,
    language: RcLanguage<'a>,
    modified: &'a str,
    publisher: &'static str,
    relation: Vec<String>,
    rights: &'static str,
    source: Vec<RcSource<'a>>,
    subject: &'static str,
    title: &'static str,
    #[serde(rename = "type")]
    kind: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct RcLanguage<'a> {
    direction: &'a str,
    identifier: &'a str,
    title: &'a str,
}

#[derive(Serialize)]
struct RcSource<'a> {
    identifier: &'a str,
    language: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<&'a str>,
}

#[derive(Serialize)]
struct Checking {
    checking_entity: Vec<String>,
    checking_level: &'static str,
}

#[derive(Serialize)]
struct Project<'a> {
    title: &'a str,
    identifier: String,
    path: String,
    sort: usize,
    categories: [&'static str; 1],
    versification: &'static str,
}

fn resource_container(
    m: &NormalizedManifest,
    book: &str,
    title: &str,
    file: &str,
    translators: &[String],
    date: &str,
) -> Result<String> {
    let language = m.languages.first();
    let lang_id = language.map(|l| l.tag.as_str()).unwrap_or("");
    let resource = m.identifier.as_deref().unwrap_or("reg");
    let sort = CANON.iter().position(|b| *b == book).unwrap_or(0);
    // Sefer's dedupeSources: one entry per language/resource/version.
    let mut seen = std::collections::HashSet::new();
    let source = m
        .sources
        .iter()
        .filter_map(|s| Some((s.language_tag.as_deref()?, s)))
        .filter(|(lang, s)| {
            seen.insert(format!(
                "{lang}/{}/{}",
                s.identifier,
                s.version.as_deref().unwrap_or("")
            ))
        })
        .map(|(language, s)| RcSource {
            identifier: &s.identifier,
            language,
            version: s.version.as_deref(),
        })
        .collect();
    let rc = Rc {
        dublin_core: DublinCore {
            conformsto: "rc0.2",
            contributor: translators,
            creator: "Bible translation community",
            format: "text/usfm",
            identifier: resource,
            issued: date,
            language: RcLanguage {
                direction: language
                    .and_then(|l| l.direction.as_deref())
                    .unwrap_or("ltr"),
                identifier: lang_id,
                title: language.and_then(|l| l.name.as_deref()).unwrap_or(lang_id),
            },
            modified: date,
            publisher: "Wycliffe Associates",
            relation: vec![format!("{lang_id}/{resource}")],
            rights: "CC-BY-SA-4.0",
            source,
            subject: "Bible",
            title: "Bible",
            kind: "bundle",
            version: "1",
        },
        checking: Checking {
            checking_entity: Vec::new(),
            checking_level: "1",
        },
        projects: vec![Project {
            title,
            identifier: book.to_lowercase(),
            path: format!("./{file}"),
            sort,
            categories: [if sort < 39 { "bible-ot" } else { "bible-nt" }],
            versification: "ufw",
        }],
    };
    Ok(serde_yaml::to_string(&rc)?)
}

/// Sefer's `.upstreams.json`: `{ v: 1, upstreams: [{ books, via, url, commit, takenAt }] }`,
/// keys in Sefer's order.
#[derive(Serialize)]
struct UpstreamsFile<'a> {
    v: u8,
    upstreams: [UpstreamEntry<'a>; 1],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpstreamEntry<'a> {
    books: [&'a str; 1],
    via: &'static str,
    url: &'a str,
    commit: &'a str,
    taken_at: &'a str,
}

fn upstreams_json(book: &str, upstream: &Upstream, date: &str) -> String {
    let file = UpstreamsFile {
        v: 1,
        upstreams: [UpstreamEntry {
            books: [book],
            via: "git",
            url: upstream.url,
            commit: upstream.commit,
            taken_at: date,
        }],
    };
    format!(
        "{}\n",
        serde_json::to_string_pretty(&file).expect("plain structs serialize")
    )
}

/// `YYYY-MM-DD` (UTC) for unix seconds, without a date crate.
fn date_of(unix: i64) -> String {
    // Howard Hinnant's days-to-civil.
    let z = unix.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(1_593_111_823), "2020-06-25");
        assert_eq!(date_of(951_782_400), "2000-02-29");
    }

    #[test]
    fn contributors_merge_like_sefer() {
        let names = ["Mary_Chishimba", "mary  chishimba", "John"]
            .map(String::from)
            .into_iter();
        assert_eq!(dedupe_contributors(names), ["Mary_Chishimba", "John"]);
    }
}
