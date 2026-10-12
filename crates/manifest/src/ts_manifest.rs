//! translationStudio desktop manifest.json — single-book, simpler than RC.
//!
//! The same file name is also written by translationCore; those manifests share
//! `project`, `target_language`, `resource` and `source_translations`, and lack
//! `finished_chunks`.
//!
//! Drops: translators, parent_draft, generator, package_version, and tC's extras.

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    Domain, Ingredient, Language, NormalizedManifest, Source, canonical_subject, norm_book_id,
};

/// Coerce string / integer / float / null into Option<String>. TS manifests in the wild
/// sometimes write `version: 20211118` (a date as integer) instead of a string.
fn flex_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    use serde::de::Error;
    let v = serde_json::Value::deserialize(d)?;
    Ok(match v {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => Some(s),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        other => {
            return Err(D::Error::custom(format!(
                "expected string-ish, got {other}"
            )));
        }
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    #[serde(default)]
    pub format: Option<String>, // "usfm"
    #[serde(default)]
    pub target_language: Option<TargetLanguage>,
    #[serde(default)]
    pub project: Option<ProjectBlock>,
    #[serde(default, rename = "type")]
    pub type_: Option<NameBlock>,
    #[serde(default)]
    pub resource: Option<NameBlock>,
    /// Some real-world TS manifests write this as a map (`{}`) instead of an array.
    /// Accept either; collect the values when it's a map.
    #[serde(default, deserialize_with = "flex_source_translations")]
    pub source_translations: Vec<SourceTranslation>,
    /// Chunks marked done in tS, e.g. "front-title", "01-title", "01-05". Missing in
    /// translationCore manifests. Non-string entries are dropped; a non-list value
    /// reads as None.
    #[serde(default, deserialize_with = "flex_chunk_list")]
    pub finished_chunks: Option<Vec<String>>,
}

fn flex_chunk_list<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<String>>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(match v {
        serde_json::Value::Array(items) => Some(
            items
                .into_iter()
                .filter_map(|x| match x {
                    serde_json::Value::String(s) => Some(s),
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    })
}

fn flex_source_translations<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<SourceTranslation>, D::Error> {
    use serde::de::Error;
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::Array(_) => serde_json::from_value(v).map_err(D::Error::custom),
        serde_json::Value::Object(map) => {
            // Treat the values as the array contents.
            let arr = serde_json::Value::Array(map.into_iter().map(|(_, v)| v).collect());
            serde_json::from_value(arr).map_err(D::Error::custom)
        }
        other => Err(D::Error::custom(format!(
            "source_translations: expected array, map, or null; got {other}"
        ))),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetLanguage {
    /// Spec says required; some manifests omit it. Optional so the rest still parses.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub direction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectBlock {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

/// `type:` and `resource:` are normally `{id, name}` but real-world TS manifests sometimes
/// inline a bare string ("book"). Accept either shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NameBlock {
    Struct {
        #[serde(default)]
        id: String,
        #[serde(default)]
        name: Option<String>,
    },
    Bare(String),
}

impl NameBlock {
    fn id(&self) -> &str {
        match self {
            NameBlock::Struct { id, .. } => id.as_str(),
            NameBlock::Bare(s) => s.as_str(),
        }
    }
    fn name(&self) -> Option<String> {
        match self {
            NameBlock::Struct { name, .. } => name.clone(),
            NameBlock::Bare(_) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceTranslation {
    /// Both language_id and resource_id are required by the canonical TS schema, but
    /// real-world manifests sometimes omit them. Make Option so we can still ingest the repo.
    #[serde(default)]
    pub language_id: Option<String>,
    #[serde(default)]
    pub resource_id: Option<String>,
    #[serde(default, deserialize_with = "flex_string")]
    pub version: Option<String>,
    #[serde(default, deserialize_with = "flex_string")]
    pub date_modified: Option<String>,
    #[serde(default, deserialize_with = "flex_string")]
    pub checking_level: Option<String>,
}

impl Document {
    pub fn normalize(&self) -> NormalizedManifest {
        let languages = self
            .target_language
            .as_ref()
            .and_then(|tl| {
                tl.id.clone().map(|tag| {
                    vec![Language {
                        tag,
                        name: tl.name.clone(),
                        direction: tl.direction.clone(),
                    }]
                })
            })
            .unwrap_or_default();

        let sources = self
            .source_translations
            .iter()
            .filter_map(|s| {
                // We need at least an identifier to record a source; skip otherwise.
                let identifier = s.resource_id.clone()?;
                Some(Source {
                    identifier,
                    language_tag: s.language_id.clone(),
                    version: s.version.clone(),
                })
            })
            .collect();

        // TS manifests describe a single project: `project.id` is the book ("heb"), and
        // it becomes the one ingredient's `book_id`. They don't carry a file path; we
        // leave path empty here. The ingest layer (which has the repo tree) is responsible
        // for filling in the actual file path before insertion.
        let ingredients = self
            .project
            .as_ref()
            .map(|p| {
                vec![Ingredient {
                    book_id: norm_book_id(&p.id),
                    path: String::new(),
                    mime_type: self.format.as_ref().map(|f| match f.as_str() {
                        "usfm" => "text/usfm".to_string(),
                        other => other.to_string(),
                    }),
                    sort_order: None,
                    title: p.name.clone(),
                }]
            })
            .unwrap_or_default();

        let content_format = self.format.as_ref().map(|f| match f.as_str() {
            "usfm" => "text/usfm".to_string(),
            other => other.to_string(),
        });

        let raw_subject = self
            .type_
            .as_ref()
            .and_then(|t| t.name())
            .or_else(|| self.type_.as_ref().map(|t| t.id().to_string()));

        let identifier = self.resource.as_ref().map(|r| r.id().to_string());
        // TS is single-book — fall back to project.id (e.g. "2th", "obs") so manifests
        // missing both type and resource still get a canonical subject.
        let book_hint = self.project.as_ref().map(|p| p.id.as_str());
        let subject = canonical_subject(raw_subject.as_deref(), identifier.as_deref(), book_hint);

        // For a single-book TS repo, the project name ("2 Thessalonians") is the most
        // useful display title. Resource name ("Regular") is too generic to lead with.
        let title = self
            .project
            .as_ref()
            .and_then(|p| p.name.clone())
            .or_else(|| self.resource.as_ref().and_then(|r| r.name()));

        // TS desktop is dominantly used for Bible translations, but it does produce OBS
        // (gloss) and other resource types. Derive domain from the canonical subject when
        // we can; fall back to Scripture (the overwhelming default) when subject is unknown.
        let domain = subject
            .as_deref()
            .and_then(Domain::for_canonical_subject)
            .or(Some(Domain::Scripture));

        NormalizedManifest {
            container_format: crate::ContainerFormat::TsManifest,
            content_format,
            subject,
            identifier,
            domain,
            flavor: None,
            usfm_version: None,
            title,
            description: None,
            version: None,
            issued: None,
            rights: None,
            publisher: None,
            languages,
            sources,
            ingredients,
            raw_modified: None,
            finished_chunks: self.finished_chunks.clone(),
            relations: Vec::new(),
        }
    }
}
