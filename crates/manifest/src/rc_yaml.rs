//! Resource Container manifest (manifest.yaml, dublin_core schema).
//!
//! Spec lineage: door43/unfoldingWord Resource Container 0.2.
//! We deliberately drop fields we don't query (contributor, creator, checking_*, comment).

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    Domain, Ingredient, Language, NormalizedManifest, Source, canonical_subject, norm_book_id,
};

/// Coerce string / integer / float into Option<String>. Real-world manifests sometimes
/// quote-wrap numeric `sort` values and sometimes leave them bare numbers.
fn flex_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    use serde::de::Error;
    let v = serde_yaml::Value::deserialize(d)?;
    Ok(match v {
        serde_yaml::Value::Null => None,
        serde_yaml::Value::String(s) => Some(s),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        other => return Err(D::Error::custom(format!("expected scalar, got {other:?}"))),
    })
}

/// Parse `sort: "01"` (string) or `sort: 1` (number) or null/missing → Option<i32>.
fn flex_i32<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i32>, D::Error> {
    let s = flex_string(d)?;
    Ok(s.and_then(|s| {
        s.trim_start_matches('0').parse::<i32>().ok().or_else(|| {
            // "00" all-zeros: parse to 0
            if s.chars().all(|c| c == '0') {
                Some(0)
            } else {
                None
            }
        })
    }))
}

/// `categories:` is sometimes a list of strings, sometimes null/empty (parses as unit),
/// sometimes a single string, sometimes missing entirely. Normalize to Vec<String>.
fn flex_string_list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let v = serde_yaml::Value::deserialize(d).unwrap_or(serde_yaml::Value::Null);
    Ok(match v {
        serde_yaml::Value::Null => Vec::new(),
        serde_yaml::Value::String(s) => vec![s],
        serde_yaml::Value::Sequence(items) => items
            .into_iter()
            .filter_map(|x| match x {
                serde_yaml::Value::String(s) => Some(s),
                serde_yaml::Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    })
}

/// `dublin_core.source` in the wild: structured array, single string, single struct, map.
/// Try all sensible shapes, drop entries we can't make sense of.
fn flex_dc_sources<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<DcSource>, D::Error> {
    let v = serde_yaml::Value::deserialize(d).unwrap_or(serde_yaml::Value::Null);
    fn one(v: serde_yaml::Value) -> Option<DcSource> {
        match v {
            serde_yaml::Value::String(s) => Some(DcSource {
                identifier: s,
                language: None,
                version: None,
            }),
            serde_yaml::Value::Mapping(_) => serde_yaml::from_value::<DcSource>(v).ok(),
            _ => None,
        }
    }
    Ok(match v {
        serde_yaml::Value::Null => Vec::new(),
        serde_yaml::Value::Sequence(items) => items.into_iter().filter_map(one).collect(),
        // Map shape: a single entry treated as one source.
        serde_yaml::Value::Mapping(_) => one(v).into_iter().collect(),
        // String shape: one source with that identifier.
        serde_yaml::Value::String(s) => vec![DcSource {
            identifier: s,
            language: None,
            version: None,
        }],
        _ => Vec::new(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub dublin_core: DublinCore,
    #[serde(default)]
    pub projects: Vec<Project>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DublinCore {
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub language: Option<DcLanguage>,
    #[serde(default, deserialize_with = "flex_dc_sources")]
    pub source: Vec<DcSource>,
    #[serde(default)]
    pub rights: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default, deserialize_with = "flex_string")]
    pub issued: Option<String>,
    #[serde(default, deserialize_with = "flex_string")]
    pub modified: Option<String>,
    #[serde(default, deserialize_with = "flex_string")]
    pub version: Option<String>,
    #[serde(default, deserialize_with = "flex_string_list")]
    pub relation: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcLanguage {
    /// Required by the spec; some real manifests omit it. Treat as Optional so the rest
    /// of the manifest still parses; downstream health check flags missing language.
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub direction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcSource {
    pub identifier: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default, deserialize_with = "flex_string")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    #[serde(default)]
    pub title: Option<String>,
    /// Spec says required, real-world manifests omit it. Optional so we don't drop the
    /// whole repo over one bad project entry.
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default, deserialize_with = "flex_i32")]
    pub sort: Option<i32>,
    #[serde(default, deserialize_with = "flex_string_list")]
    pub categories: Vec<String>,
}

impl Document {
    pub fn normalize(&self) -> NormalizedManifest {
        let dc = &self.dublin_core;

        let languages = dc
            .language
            .as_ref()
            .and_then(|l| {
                l.identifier.clone().map(|tag| {
                    vec![Language {
                        tag,
                        name: l.title.clone(),
                        direction: l.direction.clone(),
                    }]
                })
            })
            .unwrap_or_default();

        let sources = dc
            .source
            .iter()
            .map(|s| Source {
                identifier: s.identifier.clone(),
                language_tag: s.language.clone(),
                version: s.version.clone(),
            })
            .collect();

        let mime_for_ingredients = dc.format.clone();
        let ingredients = self
            .projects
            .iter()
            .filter_map(|p| {
                let path = p.path.clone()?;
                Some(Ingredient {
                    book_id: p.identifier.as_deref().and_then(norm_book_id),
                    path,
                    mime_type: mime_for_ingredients.clone(),
                    sort_order: p.sort,
                    title: p.title.clone(),
                })
            })
            .collect();

        // RC manifests are multi-project; no single book hint applies.
        let subject = canonical_subject(dc.subject.as_deref(), dc.identifier.as_deref(), None);
        let domain = subject.as_deref().and_then(Domain::for_canonical_subject);

        NormalizedManifest {
            container_format: crate::ContainerFormat::RcYaml,
            content_format: dc.format.clone(),
            subject,
            identifier: dc.identifier.clone(),
            domain,
            flavor: None,
            usfm_version: None,
            title: dc.title.clone(),
            description: dc.description.clone(),
            version: dc.version.clone(),
            issued: dc.issued.clone(),
            rights: dc.rights.clone(),
            publisher: dc.publisher.clone(),
            languages,
            sources,
            ingredients,
            raw_modified: dc.modified.clone(),
            finished_chunks: None,
            relations: dc.relation.clone(),
        }
    }
}
