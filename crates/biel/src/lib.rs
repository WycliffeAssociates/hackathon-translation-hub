//! The BIEL public data API (a Hasura GraphQL endpoint), read-only.
//!
//! Two refreshes, each a few sequential paged POSTs:
//!
//! - [`Client::languages`]: every language's names, direction, region,
//!   alternate names and gateway flag, from `vw_langnames` (the view Sefer
//!   reads; it carries the same facts as the `language` table, one row each).
//! - [`Client::contents`]: every catalog content row, with what the hub needs
//!   to ask "is this repo Primary", "is it shown on BIEL", and BIEL's
//!   shown-twice rule (`hub_domain::shown_twice_on_biel`).
//!
//! Decoding is lenient, the way Sefer's is: unknown keys are dropped, nulls
//! are `None`, and a row that still cannot be read is skipped with a warning
//! rather than failing the refresh.

use std::collections::{BTreeSet, HashSet};
use std::time::Duration;

use hub_domain::{Candidate, Status};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::{field::Empty, instrument, warn};

pub const ENDPOINT: &str = "https://api.bibleineverylanguage.org/v1/graphql";

/// WACS's host. Catalog repo URLs on it map to `<owner>/<name>.git` on disk.
pub const WACS_HOST: &str = "content.bibletranslationtools.org";

/// Rows per request. A contents page is ~5.2 MB of JSON (gzipped on the wire)
/// and takes 1-6 s.
const PAGE: usize = 10_000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The request never got an answer (DNS, TLS, timeout, reset).
    #[error("public data API request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("public data API answered HTTP {0}")]
    Http(reqwest::StatusCode),
    #[error("public data API returned GraphQL errors: {0}")]
    GraphQl(String),
    /// A 200 whose envelope is not `{data: {<root>: [...]}}`.
    #[error("public data API payload not understood: {0}")]
    Decode(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// One language, as BIEL knows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LanguageFacts {
    /// BCP-47 tag as the catalog spells it (mostly lowercase; private-use
    /// tags keep their case, e.g. `abr-CI-abron`).
    pub ietf: String,
    /// The language's name in the language itself (`national_name`).
    pub name: Option<String>,
    pub english_name: Option<String>,
    /// `ltr` or `rtl`.
    pub direction: Option<String>,
    pub region: Option<String>,
    pub alternate_names: Vec<String>,
    pub gateway: bool,
}

/// One catalog `content` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogContent {
    pub id: String,
    /// Usually `<owner>/<repo>` lowercased, but not always (some are titles).
    pub name: Option<String>,
    /// The catalog's `type`: `text` or `audio`.
    pub track: Option<String>,
    /// `reg`, `ulb`, …
    pub resource_type: Option<String>,
    /// `scripture`, `gloss`, `parascriptural`, `peripheral`.
    pub domain: Option<String>,
    pub language: Option<String>,
    /// ISO-8601 without a zone, as the catalog sends it.
    pub modified_on: Option<String>,
    /// `git_repo.repo_url`, verbatim (mixed case).
    pub repo_url: Option<String>,
    /// Where the repo sits on disk, when it is a WACS repo.
    pub repo: Option<RepoPath>,
    /// `wa_content_metadata.status`; an empty or unknown label is `None`.
    pub status: Option<Status>,
    /// `wa_content_metadata.show_on_biel`; missing metadata is `false`.
    pub show_on_biel: bool,
    /// Book slugs BIEL rendered whole from this content, deduplicated and
    /// sorted, as the catalog spells them (USFM ids, almost always).
    pub books: Vec<String>,
}

/// A WACS repo's place on disk: `<root>/<owner>/<name>.git`, both lowercase.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct RepoPath {
    pub owner: String,
    pub name: String,
}

impl Candidate for CatalogContent {
    fn status(&self) -> Option<Status> {
        self.status
    }
    fn modified(&self) -> Option<&str> {
        self.modified_on.as_deref()
    }
    fn shown_on_biel(&self) -> bool {
        self.show_on_biel
    }
}

/// `https://content.bibletranslationtools.org/Owner/Repo(.git)` → `owner/repo`.
/// Anything not on WACS, or not exactly two path segments, is `None`.
pub fn wacs_repo(url: &str) -> Option<RepoPath> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/')?;
    if !host.eq_ignore_ascii_case(WACS_HOST) {
        return None;
    }
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        return None;
    }
    Some(RepoPath {
        owner: owner.to_lowercase(),
        name: name.to_lowercase(),
    })
}

pub struct Client {
    endpoint: String,
    http: reqwest::Client,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// The production endpoint, with a per-request timeout.
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("translation-hub/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(120))
            .build()
            .expect("a client with only a user agent and a timeout builds");
        Self::with_endpoint(ENDPOINT, http)
    }

    /// Another endpoint (BIEL dev: `https://api-biel-dev.walink.org/v1/graphql`).
    pub fn with_endpoint(endpoint: impl Into<String>, http: reqwest::Client) -> Self {
        Self {
            endpoint: endpoint.into(),
            http,
        }
    }

    /// Every language. One request today (8,810 rows, 1.25 MB, ~0.6 s).
    #[instrument(name = "biel.languages", skip(self), fields(rows = Empty))]
    pub async fn languages(&self) -> Result<Vec<LanguageFacts>> {
        const QUERY: &str = "query ($limit: Int!, $offset: Int!) {
          vw_langnames(order_by: {lc: asc}, limit: $limit, offset: $offset) { lc ln ang ld lr gw alt }
        }";
        let rows: Vec<RawLanguage> = self.paged("vw_langnames", QUERY).await?;
        let mut seen = HashSet::new();
        let out: Vec<_> = rows
            .into_iter()
            .filter(|r| seen.insert(r.lc.clone()))
            .map(RawLanguage::into_facts)
            .collect();
        tracing::Span::current().record("rows", out.len());
        Ok(out)
    }

    /// Every content row. Six requests today (55,443 rows, ~29 MB, ~20-30 s).
    ///
    /// Books come from whole-book renderings only. Asking for every rendering
    /// with a book slug (workTracker's filter) is ~1.4 M rows; whole-book
    /// renderings are ~146 k and give the same books for every Primary +
    /// shown row (checked 2026-10-12). Only 18 audio rows, none with
    /// metadata, have chapter renderings alone; their `books` is empty.
    #[instrument(name = "biel.contents", skip(self), fields(rows = Empty))]
    pub async fn contents(&self) -> Result<Vec<CatalogContent>> {
        const QUERY: &str = "query ($limit: Int!, $offset: Int!) {
          content(order_by: {id: asc}, limit: $limit, offset: $offset) {
            id name type resource_type domain modified_on
            language { ietf_code }
            git_repo { repo_url }
            wa_content_metadata { status show_on_biel }
            rendered_contents(where: {scriptural_rendering_metadata: {is_whole_book: {_eq: true}}}) {
              scriptural_rendering_metadata { book_slug }
            }
          }
        }";
        let rows: Vec<RawContent> = self.paged("content", QUERY).await?;
        // Offset paging over a table that can change between pages may repeat
        // a row; keep the first.
        let mut seen = HashSet::new();
        let out: Vec<_> = rows
            .into_iter()
            .filter(|r| seen.insert(r.id.clone()))
            .map(RawContent::into_content)
            .collect();
        tracing::Span::current().record("rows", out.len());
        Ok(out)
    }

    /// Sequential pages of `root` until one comes back short.
    async fn paged<R: DeserializeOwned>(&self, root: &'static str, query: &str) -> Result<Vec<R>> {
        let mut out = Vec::new();
        let mut offset = 0;
        loop {
            let (rows, returned) = self.page(root, query, offset).await?;
            out.extend(rows);
            if returned < PAGE {
                return Ok(out);
            }
            offset += PAGE;
        }
    }

    #[instrument(
        name = "biel.page",
        skip(self, query),
        fields(rows = Empty, skipped = Empty, bytes = Empty)
    )]
    async fn page<R: DeserializeOwned>(
        &self,
        root: &'static str,
        query: &str,
        offset: usize,
    ) -> Result<(Vec<R>, usize)> {
        let body = json!({ "query": query, "variables": { "limit": PAGE, "offset": offset } });
        let res = self.http.post(&self.endpoint).json(&body).send().await?;
        if !res.status().is_success() {
            return Err(Error::Http(res.status()));
        }
        let bytes = res.bytes().await?;
        let span = tracing::Span::current();
        span.record("bytes", bytes.len());
        let (rows, returned) = decode_rows(root, &bytes)?;
        span.record("rows", rows.len());
        span.record("skipped", returned - rows.len());
        Ok((rows, returned))
    }
}

/// `{data: {<root>: [...]}}` → the rows that decode, and how many came back.
fn decode_rows<R: DeserializeOwned>(root: &str, body: &[u8]) -> Result<(Vec<R>, usize)> {
    #[derive(Deserialize)]
    struct Envelope {
        data: Option<serde_json::Map<String, Value>>,
        errors: Option<Vec<GqlError>>,
    }
    #[derive(Deserialize)]
    struct GqlError {
        message: String,
    }

    let envelope: Envelope =
        serde_json::from_slice(body).map_err(|e| Error::Decode(e.to_string()))?;
    if let Some(errors) = envelope.errors.filter(|e| !e.is_empty()) {
        let messages: Vec<_> = errors.into_iter().map(|e| e.message).collect();
        return Err(Error::GraphQl(messages.join("; ")));
    }
    let Some(Value::Array(raw)) = envelope.data.and_then(|mut d| d.remove(root)) else {
        return Err(Error::Decode(format!("no `{root}` list in `data`")));
    };
    let returned = raw.len();
    let rows = raw
        .into_iter()
        .filter_map(|row| match serde_json::from_value(row) {
            Ok(r) => Some(r),
            Err(error) => {
                warn!(root, %error, "skipped a row the decoder could not read");
                None
            }
        })
        .collect();
    Ok((rows, returned))
}

#[derive(Deserialize)]
struct RawLanguage {
    lc: String,
    ln: Option<String>,
    ang: Option<String>,
    ld: Option<String>,
    lr: Option<String>,
    gw: Option<bool>,
    alt: Option<Vec<Option<String>>>,
}

impl RawLanguage {
    fn into_facts(self) -> LanguageFacts {
        LanguageFacts {
            ietf: self.lc,
            name: self.ln,
            english_name: self.ang,
            direction: self.ld,
            region: self.lr,
            alternate_names: self.alt.into_iter().flatten().flatten().collect(),
            gateway: self.gw.unwrap_or(false),
        }
    }
}

#[derive(Deserialize)]
struct RawContent {
    id: String,
    name: Option<String>,
    #[serde(rename = "type")]
    track: Option<String>,
    resource_type: Option<String>,
    domain: Option<String>,
    modified_on: Option<String>,
    language: Option<RawLanguageRef>,
    git_repo: Option<RawGitRepo>,
    wa_content_metadata: Option<RawMetadata>,
    rendered_contents: Option<Vec<RawRendering>>,
}

#[derive(Deserialize)]
struct RawLanguageRef {
    ietf_code: Option<String>,
}

#[derive(Deserialize)]
struct RawGitRepo {
    repo_url: Option<String>,
}

#[derive(Deserialize)]
struct RawMetadata {
    status: Option<String>,
    show_on_biel: Option<bool>,
}

#[derive(Deserialize)]
struct RawRendering {
    scriptural_rendering_metadata: Option<RawScriptural>,
}

#[derive(Deserialize)]
struct RawScriptural {
    book_slug: Option<String>,
}

impl RawContent {
    fn into_content(self) -> CatalogContent {
        let repo_url = self.git_repo.and_then(|g| g.repo_url);
        let books: BTreeSet<String> = self
            .rendered_contents
            .into_iter()
            .flatten()
            .filter_map(|r| r.scriptural_rendering_metadata?.book_slug)
            .collect();
        let (status, show_on_biel) = match self.wa_content_metadata {
            Some(m) => (
                m.status.as_deref().and_then(Status::parse),
                m.show_on_biel.unwrap_or(false),
            ),
            None => (None, false),
        };
        CatalogContent {
            id: self.id,
            name: self.name,
            track: self.track,
            resource_type: self.resource_type,
            domain: self.domain,
            language: self.language.and_then(|l| l.ietf_code),
            modified_on: self.modified_on,
            repo: repo_url.as_deref().and_then(wacs_repo),
            repo_url,
            status,
            show_on_biel,
            books: books.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Why: the live catalog is full of nulls (no metadata, no git repo, no
    // language, no resource type, an empty status, mixed-case owners, a
    // "FRONT MATTER" slug), and one unreadable row must not cost the rest.
    // This page is real rows from 2026-10-12, trimmed, plus one row with a
    // null id at the end.
    #[test]
    fn a_real_page_with_its_nulls_decodes_and_skips_only_the_broken_row() {
        let body = include_bytes!("../testdata/contents.json");
        let (rows, returned) = decode_rows::<RawContent>("content", body).unwrap();
        assert_eq!((rows.len(), returned), (9, 10));
        let rows: Vec<_> = rows.into_iter().map(RawContent::into_content).collect();
        let by_name = |n: &str| rows.iter().find(|r| r.name.as_deref() == Some(n)).unwrap();

        // Three whole-book renderings (usfm, json, …) of one book are one book.
        let primary = by_name("bahasatech.indobarat/pse-x-riauasli_3jn_text_reg");
        assert_eq!(primary.status, Some(Status::Primary));
        assert!(primary.show_on_biel);
        assert_eq!(primary.books, ["3JN"]);

        // No metadata reads as unlabelled and not shown.
        let audio = by_name("pa/ulb");
        assert_eq!(
            (audio.status, audio.show_on_biel, audio.repo.as_ref()),
            (None, false, None)
        );

        // "" is not a status.
        assert_eq!(by_name("moffat_chola/bwc_2pe_text_reg").status, None);

        // The URL says GL_work, `username` says GL_Work; disk says gl_work.
        let gl = by_name("gl_work/ceb_ulb_rev");
        assert_eq!(
            gl.repo,
            Some(RepoPath {
                owner: "gl_work".into(),
                name: "ceb_ulb_rev".into()
            })
        );

        assert!(
            by_name("dcs-mirror/burritotruck_en_bsb")
                .books
                .contains(&"FRONT MATTER".into())
        );
    }
}
