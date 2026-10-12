//! hub-manifest: read a repo's manifest, whatever format it is in, and collapse it
//! into one normalized shape.
//!
//! WACS repos carry one of three manifest formats at the repo root:
//!
//! - **Resource Container** (`manifest.yaml`): Dublin Core header plus a `projects`
//!   list. Used by catalog resources (ULB, tN, tW, ...) and multi-book repos.
//! - **Scripture Burrito** (`metadata.json`): the newer standard; rare on WACS.
//! - **translationStudio** (`manifest.json`): one book per repo, written by tS
//!   desktop / Writer. By far the most common.
//!
//! Parsing is two-stage. Stage 1 deserializes the format-specific structure into a
//! [`ParsedManifest`] variant, staying lenient about the shapes real manifests use
//! (numbers where strings belong, single values where lists belong, stray YAML
//! document separators). Stage 2, [`ParsedManifest::normalize`], collapses any
//! variant into a [`NormalizedManifest`] with a canonical subject and [`Domain`].
//!
//! This crate does no I/O. The caller lists the repo's root files, asks
//! [`find_manifest`] which one to read, reads its bytes, and hands them to
//! [`parse_by_filename`] (or [`parse_as`] when the format is already known).
//!
//! Public API:
//! - [`find_manifest`]: pick the manifest file from a repo's root file names.
//! - [`parse_by_filename`] / [`parse_as`]: bytes → [`ParsedManifest`].
//! - [`ParsedManifest::normalize`]: → [`NormalizedManifest`].
//! - [`NormalizedManifest::book_ids`]: the Bible books the manifest declares.
//! - [`ManifestError`]: why a manifest could not be parsed.
//! - Format modules [`rc_yaml`], [`burrito`], [`ts_manifest`] expose the stage-1 types.

use serde::{Deserialize, Serialize};

pub mod burrito;
pub mod rc_yaml;
pub mod ts_manifest;

/// Why a manifest could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error(
        "unknown manifest filename: {0} (expected manifest.yaml, metadata.json, or manifest.json)"
    )]
    UnknownFilename(String),
    #[error("invalid YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ManifestError>;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParsedManifest {
    ResourceContainer(rc_yaml::Document),
    ScriptureBurrito(burrito::Document),
    TsManifest(ts_manifest::Document),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ContainerFormat {
    RcYaml,
    ScriptureBurrito,
    TsManifest,
}

/// Broad classification of what a resource *is*, independent of which manifest format
/// declared it. Mirrors Scripture Burrito's `type.flavorType.name` namespace
/// (https://docs.burrito.bible/) — for RC YAML and TS manifest, we map their subjects
/// onto the same vocabulary so cross-format queries work.
///
/// - **scripture**: Bible text translations (USFM, etc.)
/// - **gloss**: narrative retellings — Open Bible Stories, etc.
/// - **parascriptural**: indexed by book/chapter/verse but not Scripture itself —
///   translation notes, translation words, translation questions, commentaries
/// - **peripheral**: everything else related to Scripture work (Translation Academy, ...)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Scripture,
    Gloss,
    Parascriptural,
    Peripheral,
}

impl Domain {
    /// Map a *canonical* subject string (as produced by `canonical_subject`) to its domain.
    /// Burrito reads its domain directly from `type.flavorType.name` and bypasses this.
    pub fn for_canonical_subject(canonical: &str) -> Option<Domain> {
        match canonical {
            "Bible" => Some(Domain::Scripture),
            "Open Bible Stories" => Some(Domain::Gloss),
            "Translation Notes"
            | "Translation Words"
            | "Translation Questions"
            | "OBS Translation Notes"
            | "TSV OBS Study Notes" => Some(Domain::Parascriptural),
            "Translation Academy" => Some(Domain::Peripheral),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedManifest {
    pub container_format: ContainerFormat,
    /// MIME of the resource files. Never "scripture burrito" — that's the container.
    pub content_format: Option<String>,

    // Three-layer classification — all populated when known, so callers can search at
    // the level that matches their use case:
    //   - `identifier`: as written in the manifest ('ulb', 'tn', 'reg', 'Bible')
    //   - `subject`:    canonical normalized form ('Translation Notes' — collapsing 'Translator Notes', 'tn', etc.)
    //                   The vision is for these to converge upstream; we normalize here in the meantime.
    //   - `domain`:     broad bucket — scripture | gloss | parascriptural | peripheral
    pub identifier: Option<String>,
    pub subject: Option<String>,
    pub domain: Option<Domain>,

    /// Burrito's `type.flavorType.flavor.name` kept verbatim — e.g. "textTranslation",
    /// "audioTranslation", "textStories". A sub-classification distinct from `domain`.
    /// None for non-burrito formats.
    pub flavor: Option<String>,
    /// Burrito-only: USFM version declared by the flavor.
    pub usfm_version: Option<String>,

    pub title: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub issued: Option<String>,
    pub rights: Option<String>,
    pub publisher: Option<String>,
    /// The repo's own (resource) language(s). Source-translation languages live in `sources`.
    pub languages: Vec<Language>,
    /// What this resource was translated from. For tS and RC both `language_tag` and
    /// `identifier` are filled when the manifest gives them, which is enough to resolve
    /// a source to the catalog repo `WA-Catalog/<language_tag>_<identifier>`.
    pub sources: Vec<Source>,
    /// One per book/project. The book(s) a repo covers are the ingredients' `book_id`s;
    /// see [`NormalizedManifest::book_ids`]. A tS manifest yields exactly one ingredient,
    /// from `project.id`.
    pub ingredients: Vec<Ingredient>,
    /// tS only: the chunks the translator marked done, as written ("01-05", "01-title",
    /// "front-title"). None when the manifest has no `finished_chunks` (always for RC
    /// and Burrito); Some(empty) when tS wrote an empty list.
    pub finished_chunks: Option<Vec<String>>,
    /// Manifest's own modified field. Informational only — actual "last updated" comes from Gitea.
    pub raw_modified: Option<String>,
    /// Manifest-declared relations. Stored separately from `sources` for use by health checks.
    pub relations: Vec<String>,
}

impl NormalizedManifest {
    /// The standard Bible books this manifest declares, lowercased ("mat", "1co"), in
    /// ingredient order without duplicates. Non-book projects (tA articles, "obs",
    /// "front") are left out; read `ingredients` directly to see those.
    pub fn book_ids(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for id in self.ingredients.iter().filter_map(|i| i.book_id.as_deref()) {
            if is_bible_book(id) && !out.contains(&id) {
                out.push(id);
            }
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Language {
    pub tag: String,
    pub name: Option<String>,
    pub direction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub identifier: String,
    pub language_tag: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ingredient {
    /// Lowercased book code as written: "mat", "1co". For non-book content this holds
    /// whatever the project id was ("obs", "translate"); [`is_bible_book`] tells them apart.
    pub book_id: Option<String>,
    pub path: String,
    pub mime_type: Option<String>,
    pub sort_order: Option<i32>,
    /// Display title from the manifest (e.g. "Matthew", "Mathayo"). Used by PROJECT_TITLES_TRANSLATED.
    pub title: Option<String>,
}

impl ParsedManifest {
    pub fn normalize(&self) -> NormalizedManifest {
        match self {
            ParsedManifest::ResourceContainer(d) => d.normalize(),
            ParsedManifest::ScriptureBurrito(d) => d.normalize(),
            ParsedManifest::TsManifest(d) => d.normalize(),
        }
    }

    pub fn container_format(&self) -> ContainerFormat {
        match self {
            ParsedManifest::ResourceContainer(_) => ContainerFormat::RcYaml,
            ParsedManifest::ScriptureBurrito(_) => ContainerFormat::ScriptureBurrito,
            ParsedManifest::TsManifest(_) => ContainerFormat::TsManifest,
        }
    }
}

/// Root-level manifest file names, in precedence order. Matched case-insensitively
/// against the whole file name, so look-alikes such as `org-manifest.json` or
/// `manifest (2).json` (both seen in real repos) are ignored.
///
/// On the WACS data no repo has more than one of these at its root, so the order
/// only matters for repos we haven't seen. It runs most-specific first: a Burrito
/// `metadata.json` names its format explicitly; an RC `manifest.yaml` is next; a tS
/// `manifest.json` is last because tS-derived exports can leave one behind next to
/// the manifest that actually describes the repo.
pub const MANIFEST_FILENAMES: &[(&str, ContainerFormat)] = &[
    ("metadata.json", ContainerFormat::ScriptureBurrito),
    ("manifest.yaml", ContainerFormat::RcYaml),
    ("manifest.yml", ContainerFormat::RcYaml),
    ("manifest.json", ContainerFormat::TsManifest),
];

/// Pick the manifest to parse from a repo's root-level file names (as `git ls-tree
/// --name-only HEAD` prints them). Returns the name as given plus its format, or
/// None when the repo has no manifest. Precedence is [`MANIFEST_FILENAMES`] order.
pub fn find_manifest<S: AsRef<str>>(root_files: &[S]) -> Option<(&str, ContainerFormat)> {
    MANIFEST_FILENAMES.iter().find_map(|(want, format)| {
        root_files
            .iter()
            .map(AsRef::as_ref)
            .find(|name| name.eq_ignore_ascii_case(want))
            .map(|name| (name, *format))
    })
}

/// Parse bytes as the given format. A leading UTF-8 byte-order mark is skipped:
/// some catalog manifests (e.g. `WA-Catalog/fr_ulb`) start with one, and with it
/// YAML reads `---` as part of a plain string.
pub fn parse_as(format: ContainerFormat, bytes: &[u8]) -> Result<ParsedManifest> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    Ok(match format {
        ContainerFormat::RcYaml => ParsedManifest::ResourceContainer(parse_yaml_lenient(bytes)?),
        ContainerFormat::ScriptureBurrito => {
            ParsedManifest::ScriptureBurrito(serde_json::from_slice(bytes)?)
        }
        ContainerFormat::TsManifest => ParsedManifest::TsManifest(serde_json::from_slice(bytes)?),
    })
}

/// Detect format by filename and parse. Filenames are case-insensitive.
pub fn parse_by_filename(filename: &str, bytes: &[u8]) -> Result<ParsedManifest> {
    let lower = filename.to_ascii_lowercase();
    // Some RC repos use "<resource>-manifest.yaml" or "<lang>_<resource>.manifest.yaml" etc.
    // The reliable signal is the suffix.
    let format = if lower.ends_with("manifest.yaml") || lower.ends_with("manifest.yml") {
        ContainerFormat::RcYaml
    } else if lower.ends_with("metadata.json") {
        ContainerFormat::ScriptureBurrito
    } else if lower.ends_with("manifest.json") {
        ContainerFormat::TsManifest
    } else {
        return Err(ManifestError::UnknownFilename(filename.to_string()));
    };
    parse_as(format, bytes)
}

/// Tolerant YAML parsing:
/// - Direct deserialize first.
/// - On failure (e.g. "more than one document", or a file that starts with `---`),
///   take the first non-empty document that deserializes.
/// - If none does, report the direct error.
fn parse_yaml_lenient<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
    let direct_err = match serde_yaml::from_slice::<T>(bytes) {
        Ok(v) => return Ok(v),
        Err(e) => e,
    };
    let text = std::str::from_utf8(bytes).unwrap_or("");
    // serde_yaml's document iterator does not end after a syntax error; it hands
    // back the same failing document forever. Stop at the first error, and cap the
    // count as a second guard.
    for de in serde_yaml::Deserializer::from_str(text).take(16) {
        let Ok(value) = serde_yaml::Value::deserialize(de) else {
            break;
        };
        if matches!(value, serde_yaml::Value::Null) {
            continue;
        }
        if let Ok(typed) = serde_yaml::from_value::<T>(value) {
            return Ok(typed);
        }
    }
    Err(direct_err.into())
}

/// Collapse a free-text subject onto a canonical form. Considers, in order:
/// the raw subject, the resource identifier, and an optional "book id" hint
/// (TS manifests sometimes have neither subject nor identifier — only a
/// `project.id` like "2th" or "obs" — which is enough to classify).
///
/// Returns the original (trimmed) raw subject when no canonical mapping is known
/// so callers see something rather than nothing for novel resource types.
pub(crate) fn canonical_subject(
    raw_subject: Option<&str>,
    identifier: Option<&str>,
    book_hint: Option<&str>,
) -> Option<String> {
    fn lookup(s: &str) -> Option<&'static str> {
        let lower = s.trim().to_ascii_lowercase();
        match lower.as_str() {
            "bible" | "aligned bible" | "open bible" | "texttranslation" | "audiotranslation"
            | "typesetscripture" | "ulb" | "udb" | "reg" => Some("Bible"),
            "translator notes" | "translation notes" | "notes" | "tn" => Some("Translation Notes"),
            "translation words" | "tw" => Some("Translation Words"),
            "translation questions" | "tq" => Some("Translation Questions"),
            "translation academy" | "translation manual" | "ta" => Some("Translation Academy"),
            "open bible stories" | "textstories" | "obs" => Some("Open Bible Stories"),
            other if is_bible_book(other) => Some("Bible"),
            _ => None,
        }
    }

    for candidate in [raw_subject, identifier, book_hint].into_iter().flatten() {
        if let Some(canonical) = lookup(candidate) {
            return Some(canonical.to_string());
        }
    }
    raw_subject
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// True for one of the 66 standard book codes, lowercase ("mat", "1co").
pub fn is_bible_book(code: &str) -> bool {
    BIBLE_BOOK_CODES.binary_search(&code).is_ok()
}

/// 66 standard Bible book identifiers (lowercase, sorted). Used so a TS manifest
/// whose only signal is `project.id="2th"` still classifies as `subject=Bible`.
const BIBLE_BOOK_CODES: &[&str] = &[
    "1ch", "1co", "1jn", "1ki", "1pe", "1sa", "1th", "1ti", "2ch", "2co", "2jn", "2ki", "2pe",
    "2sa", "2th", "2ti", "3jn", "act", "amo", "col", "dan", "deu", "ecc", "eph", "est", "exo",
    "ezk", "ezr", "gal", "gen", "hab", "hag", "heb", "hos", "isa", "jas", "jdg", "jer", "jhn",
    "job", "jol", "jon", "jos", "jud", "lam", "lev", "luk", "mal", "mat", "mic", "mrk", "nam",
    "neh", "num", "oba", "phm", "php", "pro", "psa", "rev", "rom", "rut", "sng", "tit", "zec",
    "zep",
];

/// Lowercase a book identifier. Book identifiers in our domain are always 2-3 chars
/// (e.g. "1co", "mat", "rev") or numeric variants. Kept permissive — we lowercase
/// everything, including non-books like "obs"; [`is_bible_book`] decides if it's a real book.
pub(crate) fn norm_book_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_manifest_ignores_lookalikes_and_follows_precedence() {
        let files = ["README.md", "manifest (2).json", "org-manifest.json", "manifest.json"];
        assert_eq!(
            find_manifest(&files),
            Some(("manifest.json", ContainerFormat::TsManifest))
        );
        let both = ["manifest.json", "Manifest.YAML"];
        assert_eq!(
            find_manifest(&both),
            Some(("Manifest.YAML", ContainerFormat::RcYaml))
        );
        assert_eq!(find_manifest(&["LICENSE.md"]), None);
    }

    #[test]
    fn rc_lenient_shapes() {
        // Leading `---`, a second document, a bare-string source, numeric sort and
        // version, and a null categories list.
        let yaml = b"\xEF\xBB\xBF---
dublin_core:
  identifier: ulb
  format: text/usfm
  language: { identifier: en, title: English, direction: ltr }
  source:
    - { identifier: ulb, language: en, version: 12 }
    - plain-string-source
  version: 3
  relation: en/tn
projects:
  - { identifier: GEN, path: ./01-GEN.usfm, sort: \"01\", categories: }
  - { identifier: frt, path: ./A0-FRT.usfm, sort: 0 }
---
junk: true
";
        let n = parse_by_filename("manifest.yaml", yaml).unwrap().normalize();
        assert_eq!(n.subject.as_deref(), Some("Bible"));
        assert_eq!(n.domain, Some(Domain::Scripture));
        assert_eq!(n.version.as_deref(), Some("3"));
        assert_eq!(n.sources[0].language_tag.as_deref(), Some("en"));
        assert_eq!(n.sources[0].identifier, "ulb");
        assert_eq!(n.sources[0].version.as_deref(), Some("12"));
        assert_eq!(n.sources[1].identifier, "plain-string-source");
        assert_eq!(n.relations, vec!["en/tn"]);
        assert_eq!(n.ingredients[0].sort_order, Some(1));
        assert_eq!(n.ingredients[1].sort_order, Some(0));
        assert_eq!(n.book_ids(), vec!["gen"]);
        assert_eq!(n.finished_chunks, None);
    }

    #[test]
    fn ts_book_sources_and_chunks() {
        let json = br#"{
            "format": "usfm",
            "target_language": {"id": "bjn", "name": "Banjar", "direction": "ltr"},
            "project": {"id": "heb", "name": "Hebrews"},
            "type": "book",
            "resource": {"id": "reg", "name": "Regular"},
            "source_translations": {"x": {"language_id": "id", "resource_id": "ayt", "version": 13}},
            "finished_chunks": ["front-title", "01-01", 7]
        }"#;
        let n = parse_by_filename("manifest.json", json).unwrap().normalize();
        assert_eq!(n.book_ids(), vec!["heb"]);
        assert_eq!(n.subject.as_deref(), Some("Bible"));
        assert_eq!(n.content_format.as_deref(), Some("text/usfm"));
        assert_eq!(n.sources[0].language_tag.as_deref(), Some("id"));
        assert_eq!(n.sources[0].identifier, "ayt");
        assert_eq!(
            n.finished_chunks,
            Some(vec!["front-title".to_string(), "01-01".to_string()])
        );

        // translationCore writes manifest.json too, without finished_chunks.
        let tc = br#"{"project": {"id": "tit"}, "finished_chunks": "nope"}"#;
        let n = parse_by_filename("manifest.json", tc).unwrap().normalize();
        assert_eq!(n.finished_chunks, None);
        assert_eq!(n.domain, Some(Domain::Scripture));
    }

    #[test]
    fn burrito_corrupt_type_and_source_relationship() {
        let json = br#"{
            "format": "scripture burrito",
            "identification": {"name": {"en": "Bible"}},
            "type": "Buffer",
            "languages": [{"tag": "fr", "name": {"fr": "Francais"}}],
            "ingredients": {"MAT.usfm": {"mimeType": "text/x-usfm", "scope": {"MAT": []}}},
            "localizedNames": {"MAT": {"short": {"fr": "Matthieu"}}},
            "relationships": [{"type": "source", "target": "some-opaque-id"}]
        }"#;
        let n = parse_by_filename("metadata.json", json).unwrap().normalize();
        assert_eq!(n.flavor, None);
        assert_eq!(n.subject.as_deref(), Some("Bible"));
        assert_eq!(n.content_format.as_deref(), Some("text/usfm"));
        assert_eq!(n.book_ids(), vec!["mat"]);
        assert_eq!(n.ingredients[0].title.as_deref(), Some("Matthieu"));
        assert_eq!(n.sources[0].identifier, "some-opaque-id");
        assert_eq!(n.sources[0].language_tag, None);
    }

    #[test]
    fn subject_falls_back_through_identifier_and_book_hint() {
        assert_eq!(
            canonical_subject(Some("Text"), Some("reg"), None).as_deref(),
            Some("Bible")
        );
        assert_eq!(
            canonical_subject(None, None, Some("2th")).as_deref(),
            Some("Bible")
        );
        assert_eq!(
            canonical_subject(Some(" Something New "), None, None).as_deref(),
            Some("Something New")
        );
    }
}
