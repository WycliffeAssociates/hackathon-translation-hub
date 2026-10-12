//! Scripture Burrito metadata (metadata.json).
//!
//! Spec: https://docs.burrito.bible/ — BTT's compliant variant at
//! https://github.com/Bible-Translation-Tools/scripture-burrito adds documentation
//! around our intended handling but is fully spec-compliant.
//!
//! We deliberately drop: idAuthorities, localizedNames, confidential,
//! meta.generator, meta.category, copyright details beyond a license reference.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    Domain, Ingredient, Language, NormalizedManifest, Source, canonical_subject, norm_book_id,
};

fn flex_type_block<'de, D: Deserializer<'de>>(d: D) -> Result<Option<TypeBlock>, D::Error> {
    // Accept the structured form by deserializing directly. Fall back to None for any
    // shape (string, number, null) the canonical schema doesn't expect — keeps the
    // overall manifest parseable even when a single field is corrupt.
    let v = serde_json::Value::deserialize(d)?;
    if !v.is_object() {
        return Ok(None);
    }
    // Use Deserialize's untyped Value path to round-trip into TypeBlock.
    Ok(serde_json::from_value::<TypeBlock>(v).ok())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    #[serde(default)]
    pub format: Option<String>, // "scripture burrito"
    #[serde(default)]
    pub meta: Option<Meta>,
    #[serde(default)]
    pub identification: Option<Identification>,
    /// Real-world manifests occasionally write a corrupted `type` (e.g. the literal
    /// string "Buffer" — looks like a serialization bug at the source). Accept any JSON
    /// shape and recover the structured form when possible; otherwise leave it None.
    #[serde(default, rename = "type", deserialize_with = "flex_type_block")]
    pub type_: Option<TypeBlock>,
    #[serde(default)]
    pub copyright: Option<Copyright>,
    #[serde(default)]
    pub languages: Vec<BurritoLanguage>,
    #[serde(default)]
    pub ingredients: BTreeMap<String, IngredientBlock>,
    #[serde(default)]
    pub relationships: Vec<Relationship>,
    /// Per-book localized names. Not stored in NormalizedManifest, but used to
    /// populate ingredient titles for health checks. Keys are uppercase book codes.
    #[serde(default, rename = "localizedNames")]
    pub localized_names: BTreeMap<String, LocalizedName>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalizedName {
    #[serde(default)]
    pub short: BTreeMap<String, String>,
    #[serde(default)]
    pub long: BTreeMap<String, String>,
    #[serde(default)]
    pub abbr: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default, rename = "dateCreated")]
    pub date_created: Option<String>,
    #[serde(default, rename = "defaultLocale")]
    pub default_locale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identification {
    /// Localized resource name keyed by locale code: { "en": "Bible" }.
    #[serde(default)]
    pub name: BTreeMap<String, String>,
    /// Source/edition identifiers — opaque per-authority blobs we don't try to model.
    #[serde(default)]
    pub primary: serde_json::Value,
    #[serde(default)]
    pub abbreviation: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeBlock {
    #[serde(rename = "flavorType")]
    pub flavor_type: FlavorType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlavorType {
    pub name: String, // "scripture", "gloss", ...
    pub flavor: Flavor,
    #[serde(default, rename = "currentScope")]
    pub current_scope: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flavor {
    pub name: String, // "textTranslation", "audioTranslation", ...
    #[serde(default, rename = "usfmVersion")]
    pub usfm_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Copyright {
    #[serde(default, rename = "fullStatementPlain")]
    pub full_statement_plain: BTreeMap<String, String>,
    #[serde(default)]
    pub licenses: Vec<License>,
    #[serde(default, rename = "shortStatementPlain")]
    pub short_statement_plain: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct License {
    /// Reference to a LICENSE.md ingredient inside the repo, or an SPDX id.
    #[serde(default)]
    pub ingredient: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurritoLanguage {
    pub tag: String,
    #[serde(default)]
    pub name: BTreeMap<String, String>,
    #[serde(default, rename = "scriptDirection")]
    pub script_direction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngredientBlock {
    #[serde(default, rename = "mimeType")]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub scope: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub checksum: Option<Checksum>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checksum {
    #[serde(default)]
    pub md5: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
}

impl Document {
    pub fn normalize(&self) -> NormalizedManifest {
        let default_locale = self
            .meta
            .as_ref()
            .and_then(|m| m.default_locale.clone())
            .unwrap_or_else(|| "en".to_string());

        // identifier: prefer English (always present per BTT convention), then defaultLocale, then any.
        // title: use the same lookup — for burrito the identifying name and the display name coincide.
        let pick_localized = |map: &BTreeMap<String, String>| -> Option<String> {
            map.get("en")
                .or_else(|| map.get(&default_locale))
                .or_else(|| map.values().next())
                .cloned()
        };
        let identifier = self
            .identification
            .as_ref()
            .and_then(|i| pick_localized(&i.name));
        let title = identifier.clone();

        let flavor_type_name = self.type_.as_ref().map(|t| t.flavor_type.name.clone());
        let flavor_name = self
            .type_
            .as_ref()
            .map(|t| t.flavor_type.flavor.name.clone());
        let usfm_version = self
            .type_
            .as_ref()
            .and_then(|t| t.flavor_type.flavor.usfm_version.clone());

        // Subject is the canonical form. Feed it the flavor name (verbatim) and the
        // identifier — canonicalization happens in one place for all formats.
        let subject = canonical_subject(flavor_name.as_deref(), identifier.as_deref(), None);

        // Burrito's flavor_type.name is the same vocabulary as our Domain.
        let domain = flavor_type_name.as_deref().and_then(|name| match name {
            "scripture" => Some(Domain::Scripture),
            "gloss" => Some(Domain::Gloss),
            "parascriptural" => Some(Domain::Parascriptural),
            "peripheral" => Some(Domain::Peripheral),
            _ => None,
        });

        let content_format = burrito_content_format(self);

        let languages = self
            .languages
            .iter()
            .map(|l| Language {
                tag: l.tag.clone(),
                name: l
                    .name
                    .get(&default_locale)
                    .or_else(|| l.name.values().next())
                    .cloned(),
                direction: l.script_direction.clone(),
            })
            .collect();

        // Burrito has no first-class "source[]" array. We pull source-like info from
        // relationships of type 'source' if present; otherwise leave empty. The target
        // is opaque (a URL, a DBL id, a free-form name) and there is no language tag,
        // so these rarely resolve to a `WA-Catalog/<lang>_<identifier>` repo.
        let sources = self
            .relationships
            .iter()
            .filter(|r| r.kind.as_deref() == Some("source"))
            .filter_map(|r| {
                let target = r.target.clone()?;
                Some(Source {
                    identifier: target,
                    language_tag: None,
                    version: None,
                })
            })
            .collect();

        // Pick a locale for ingredient titles. Burrito ingredients carry no name,
        // so we look them up in localizedNames by the uppercase book code, preferring
        // the resource's own language tag, then defaultLocale, then any value.
        let primary_locale = self.languages.first().map(|l| l.tag.clone());
        let ingredients = self
            .ingredients
            .iter()
            .map(|(path, block)| {
                let scope_key = block.scope.keys().next().cloned();
                let book_id = scope_key.as_deref().and_then(norm_book_id);
                let title = scope_key
                    .as_ref()
                    .and_then(|k| self.localized_names.get(k))
                    .and_then(|ln| {
                        let pick = |map: &BTreeMap<String, String>| -> Option<String> {
                            primary_locale
                                .as_ref()
                                .and_then(|loc| map.get(loc).cloned())
                                .or_else(|| map.get(&default_locale).cloned())
                                .or_else(|| map.values().next().cloned())
                        };
                        pick(&ln.short)
                            .or_else(|| pick(&ln.long))
                            .or_else(|| pick(&ln.abbr))
                    });
                Ingredient {
                    book_id,
                    path: path.clone(),
                    mime_type: block.mime_type.clone(),
                    sort_order: None,
                    title,
                }
            })
            .collect();

        // Rights: prefer the first license id (SPDX-ish), else the ingredient ref (e.g. "LICENSE.md").
        let rights = self.copyright.as_ref().and_then(|c| {
            c.licenses
                .iter()
                .find_map(|l| l.id.clone().or_else(|| l.ingredient.clone()))
        });

        let relations = self
            .relationships
            .iter()
            .filter_map(|r| r.target.clone())
            .collect();

        NormalizedManifest {
            container_format: crate::ContainerFormat::ScriptureBurrito,
            content_format,
            subject,
            identifier,
            domain,
            flavor: flavor_name,
            usfm_version,
            title,
            description: None,
            version: self.meta.as_ref().and_then(|m| m.version.clone()),
            issued: self.meta.as_ref().and_then(|m| m.date_created.clone()),
            rights,
            publisher: None,
            languages,
            sources,
            ingredients,
            raw_modified: None,
            finished_chunks: None,
            relations,
        }
    }
}

/// Derive the content MIME for the burrito as a whole. We pick the most common
/// ingredient mime, or fall back to scripture defaults based on flavor.
fn burrito_content_format(doc: &Document) -> Option<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for ing in doc.ingredients.values() {
        if let Some(m) = &ing.mime_type {
            *counts.entry(m.clone()).or_default() += 1;
        }
    }
    if let Some((mime, _)) = counts.into_iter().max_by_key(|(_, c)| *c) {
        // Burrito uses `text/x-usfm`; normalize to the more common `text/usfm` for cross-format parity.
        let normalized = if mime == "text/x-usfm" {
            "text/usfm".to_string()
        } else {
            mime
        };
        return Some(normalized);
    }
    None
}
