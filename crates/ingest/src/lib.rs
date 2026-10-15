//! The hub's pipeline for one repo: HEAD, the manifest, then per book a lint
//! summary keyed by content and verse coverage against its source.
//!
//! A Writer repo becomes its synthetic repo first (one stitched book). A book
//! whose manifest declares a source that resolves to `wa-catalog/<lang>_<id>`
//! in the same root also gets coverage against it (Sefer's convention,
//! `src/core/resources/relationships.ts`).
//!
//! Pure reading: nothing here writes to the repos. `hub-scan` runs it over a
//! root and prints JSON; the server runs it from the soft poll and stores it.

pub mod forgejo;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use hub_checks::{
    Coverage, LintSummary, content_key_for_blob, content_key_for_chunks, coverage, lint_book,
    verses_by_chapter,
};
use hub_manifest::{ContainerFormat, NormalizedManifest, find_manifest, parse_as};
use hub_repo_read::{EntryKind, RepoReader, head};
use hub_synthetic::{SyntheticRepo, Upstream, synthesize};
use serde::Serialize;

/// Where the repos are and how the outside world names them.
#[derive(Debug, Clone)]
pub struct Config {
    /// Forgejo's repository root: `<root>/<owner>/<name>.git`.
    pub root: PathBuf,
    /// The public URL repos are cloned from, for `.upstreams.json`.
    pub base_url: String,
}

/// What one repo looks like at HEAD.
#[derive(Debug, Clone, Serialize)]
pub struct RepoScan {
    pub owner: String,
    pub name: String,
    pub branch: Option<String>,
    /// `None` for an empty repo.
    pub sha: Option<String>,
    /// Committer time of HEAD, Unix seconds.
    pub head_at: Option<i64>,
    pub shape: Shape,
    pub manifest: Option<ManifestReport>,
    /// Set for a Writer repo: what the hub serves instead of its chunk files.
    pub synthetic: Option<SyntheticReport>,
    pub books: Vec<BookScan>,
    pub error: Option<String>,
}

/// What kind of repo it is, by its manifest (or lack of one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// translationStudio / Writer `manifest.json`.
    Ts,
    /// Resource Container `manifest.yaml`.
    Rc,
    /// Scripture Burrito `metadata.json`.
    Burrito,
    /// No manifest, but USFM at the root.
    Usfm,
    Other,
    Empty,
}

impl Shape {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ts => "ts",
            Self::Rc => "rc",
            Self::Burrito => "burrito",
            Self::Usfm => "usfm",
            Self::Other => "other",
            Self::Empty => "empty",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestReport {
    pub file: String,
    pub language: Option<String>,
    pub resource: Option<String>,
    pub subject: Option<String>,
    pub title: Option<String>,
    pub books: Vec<String>,
    /// As `lang/resource`, in the order declared.
    pub sources: Vec<String>,
    pub finished_chunks: Option<usize>,
    /// The whole normalized manifest, for pages that show it.
    pub normalized: Option<NormalizedManifest>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyntheticReport {
    pub file: String,
    pub title: String,
    pub chapters: Vec<u32>,
    pub chunk_files: usize,
}

/// One book at HEAD.
#[derive(Debug, Clone, Serialize)]
pub struct BookScan {
    /// Upper-case USFM id; empty when the file has no readable `\id`.
    pub book: String,
    pub path: String,
    /// Blob sha (USFM) or a hash of chunk shas (Writer): the same text in two
    /// repos has the same key, so checks keyed on it run once.
    pub content_key: String,
    pub chapters: u32,
    pub verses: u32,
    pub lint: LintSummary,
    pub coverage: Option<CoverageReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverageReport {
    /// `owner/name` of the source repo.
    pub source: String,
    pub source_verses: u32,
    pub present: u32,
    pub missing: u32,
    pub extra: u32,
    /// Only chapters with missing verses, as `chapter → missing verses`.
    pub gaps: BTreeMap<u32, Vec<u32>>,
}

type Verses = Arc<BTreeMap<u32, BTreeSet<u32>>>;

/// Runs the pipeline. Shareable across threads: the only state is a cache of
/// source verse sets, read once per (source repo, book).
pub struct Scanner {
    config: Config,
    sources: Mutex<HashMap<(String, String), Option<Verses>>>,
}

impl Scanner {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            sources: Mutex::new(HashMap::new()),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Forget cached source verses, e.g. after a source repo moved.
    pub fn clear_sources(&self) {
        self.sources.lock().expect("source cache").clear();
    }

    pub fn repo_path(&self, owner: &str, name: &str) -> PathBuf {
        self.config.root.join(owner).join(format!("{name}.git"))
    }

    /// Scans one repo. Never panics on bad data; failures land in `error`.
    #[tracing::instrument(skip(self), fields(books))]
    pub fn scan(&self, owner: &str, name: &str) -> RepoScan {
        let mut scan = RepoScan {
            owner: owner.to_owned(),
            name: name.to_owned(),
            branch: None,
            sha: None,
            head_at: None,
            shape: Shape::Empty,
            manifest: None,
            synthetic: None,
            books: Vec::new(),
            error: None,
        };
        if let Err(e) = self.fill(&mut scan) {
            tracing::warn!(error = %e, "scan failed");
            scan.error = Some(e);
        }
        tracing::Span::current().record("books", scan.books.len());
        scan
    }

    fn fill(&self, scan: &mut RepoScan) -> Result<(), String> {
        let path = self.repo_path(&scan.owner, &scan.name);
        let Some(h) = head(&path).map_err(|e| e.to_string())? else {
            return Ok(());
        };
        scan.branch = h.branch;
        scan.sha = Some(h.sha.clone());
        let reader = RepoReader::open(&path).map_err(|e| e.to_string())?;
        scan.head_at = reader.commit_time(&h.sha).ok();
        let root_entries = reader.root_files(&h.sha).map_err(|e| e.to_string())?;
        let root: Vec<&str> = root_entries.iter().map(|e| e.path.as_str()).collect();

        let found = find_manifest(&root);
        scan.shape = match found.map(|(_, f)| f) {
            Some(ContainerFormat::TsManifest) => Shape::Ts,
            Some(ContainerFormat::RcYaml) => Shape::Rc,
            Some(ContainerFormat::ScriptureBurrito) => Shape::Burrito,
            None if root.iter().any(|p| is_usfm_path(p)) => Shape::Usfm,
            None => Shape::Other,
        };
        let manifest = found.map(|(file, format)| read_manifest(&reader, &h.sha, file, format));
        let source = manifest
            .as_ref()
            .and_then(|m| m.normalized.as_ref())
            .and_then(|m| resolve_source(m, &self.config.root));
        scan.manifest = manifest;

        // A Writer repo is served as its synthetic repo.
        let url = format!("{}/{}/{}.git", self.config.base_url, scan.owner, scan.name);
        let synthetic = synthesize(
            &reader,
            &h.sha,
            &Upstream {
                url: &url,
                commit: &h.sha,
            },
        )
        .map_err(|e| e.to_string())?;
        if let Some(syn) = synthetic {
            let usfm = syn.usfm();
            scan.books.push(self.book(
                &syn.book,
                &usfm.path,
                content_key_for_chunks(&syn.chunks),
                &usfm.text,
                source.as_deref(),
            ));
            scan.synthetic = Some(SyntheticReport {
                file: usfm.path.clone(),
                title: syn.title.clone(),
                chapters: syn.chapters.clone(),
                chunk_files: syn.chunks.len(),
            });
            return Ok(());
        }

        // Otherwise every root USFM file is a book. The blobs are read in
        // order (one git handle), then each book is counted and linted on its
        // own, in parallel: a 66-book repo is 66 independent lint runs.
        use rayon::prelude::*;
        let mut texts = Vec::new();
        for entry in &root_entries {
            if entry.kind != EntryKind::Blob || !is_usfm_path(&entry.path) {
                continue;
            }
            let bytes = reader.blob(entry.oid).map_err(|e| e.to_string())?;
            texts.push((entry, String::from_utf8_lossy(&bytes).into_owned()));
        }
        scan.books = texts
            .par_iter()
            .map(|(entry, text)| {
                self.book(
                    &book_id(text).unwrap_or_default(),
                    &entry.path,
                    content_key_for_blob(&entry.oid.to_string()),
                    text,
                    source.as_deref(),
                )
            })
            .collect();
        Ok(())
    }

    fn book(
        &self,
        book: &str,
        path: &str,
        content_key: String,
        text: &str,
        source: Option<&str>,
    ) -> BookScan {
        let verses = verses_by_chapter(text);
        let coverage = source.and_then(|repo| {
            let want = self.source_verses(repo, book)?;
            Some(coverage_report(repo, coverage(&verses, &want)))
        });
        BookScan {
            book: book.to_owned(),
            path: path.to_owned(),
            content_key,
            chapters: verses.keys().filter(|&&c| c > 0).count() as u32,
            verses: verses.values().map(|v| v.len() as u32).sum(),
            lint: lint_book(text),
            coverage,
        }
    }

    /// The source repo's verses for `book`, from its `NN-BOOK.usfm`; read once.
    fn source_verses(&self, repo: &str, book: &str) -> Option<Verses> {
        let key = (repo.to_owned(), book.to_owned());
        if let Some(hit) = self.sources.lock().expect("source cache").get(&key) {
            return hit.clone();
        }
        let read = || -> Option<Verses> {
            let path = self.config.root.join(format!("{repo}.git"));
            let sha = head(&path).ok()??.sha;
            let reader = RepoReader::open(&path).ok()?;
            let suffix = format!("-{book}.usfm").to_ascii_lowercase();
            let entry = reader
                .root_files(&sha)
                .ok()?
                .into_iter()
                .find(|e| e.path.to_ascii_lowercase().ends_with(&suffix))?;
            let text = String::from_utf8_lossy(&reader.blob(entry.oid).ok()?).into_owned();
            Some(Arc::new(verses_by_chapter(&text)))
        };
        let value = read();
        self.sources
            .lock()
            .expect("source cache")
            .insert(key, value.clone());
        value
    }
}

pub fn is_usfm_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".usfm") || lower.ends_with(".sfm")
}

fn read_manifest(
    reader: &RepoReader,
    sha: &str,
    file: &str,
    format: ContainerFormat,
) -> ManifestReport {
    let mut report = ManifestReport {
        file: file.to_owned(),
        language: None,
        resource: None,
        subject: None,
        title: None,
        books: Vec::new(),
        sources: Vec::new(),
        finished_chunks: None,
        normalized: None,
        error: None,
    };
    let parsed = reader
        .file(sha, file)
        .map_err(|e| e.to_string())
        .and_then(|raw| {
            let raw = raw.ok_or("manifest vanished")?;
            parse_as(format, &raw)
                .map(|p| p.normalize())
                .map_err(|e| e.to_string())
        });
    match parsed {
        Ok(m) => {
            report.language = m.languages.first().map(|l| l.tag.clone());
            report.resource = m.identifier.clone();
            report.subject = m.subject.clone();
            report.title = m.title.clone();
            report.books = m.book_ids().iter().map(|b| b.to_string()).collect();
            report.sources = m
                .sources
                .iter()
                .map(|s| {
                    format!(
                        "{}/{}",
                        s.language_tag.as_deref().unwrap_or("?"),
                        s.identifier
                    )
                })
                .collect();
            report.finished_chunks = m.finished_chunks.as_ref().map(Vec::len);
            report.normalized = Some(m);
        }
        Err(e) => report.error = Some(e),
    }
    report
}

/// `wa-catalog/<lang>_<identifier>` in the same root, for the first declared
/// source that has one. The convention the catalog follows; lowercase on disk.
pub fn resolve_source(m: &NormalizedManifest, root: &Path) -> Option<String> {
    m.sources.iter().find_map(|s| {
        let repo =
            format!("wa-catalog/{}_{}", s.language_tag.as_deref()?, s.identifier).to_lowercase();
        root.join(format!("{repo}.git")).is_dir().then_some(repo)
    })
}

fn coverage_report(source: &str, c: Coverage) -> CoverageReport {
    CoverageReport {
        source: source.to_owned(),
        source_verses: c.total.source_verses,
        present: c.total.present,
        missing: c.total.missing,
        extra: c.total.extra,
        gaps: c
            .chapters
            .into_iter()
            .filter(|ch| !ch.missing.is_empty())
            .map(|ch| (ch.chapter, ch.missing))
            .collect(),
    }
}

/// The book a USFM file declares in its `\id` line.
pub fn book_id(text: &str) -> Option<String> {
    let rest = text
        .trim_start_matches('\u{feff}')
        .trim_start()
        .strip_prefix("\\id")?;
    let id: String = rest.trim_start().chars().take(3).collect();
    (id.len() == 3).then(|| id.to_ascii_uppercase())
}

/// The synthetic repo for a Writer repo at `sha`, rebuilt on request (the
/// server serves its files without storing them).
pub fn synthetic_at(
    scanner: &Scanner,
    owner: &str,
    name: &str,
    sha: &str,
) -> Result<Option<SyntheticRepo>, String> {
    let reader = RepoReader::open(&scanner.repo_path(owner, name)).map_err(|e| e.to_string())?;
    let url = format!("{}/{owner}/{name}.git", scanner.config.base_url);
    synthesize(
        &reader,
        sha,
        &Upstream {
            url: &url,
            commit: sha,
        },
    )
    .map_err(|e| e.to_string())
}

/// The repo's books as (path, text) at `sha`: the stitched book of a Writer
/// repo, or every root USFM file. What a corpus-wide check (sous-chef) reads.
pub fn book_texts(
    scanner: &Scanner,
    owner: &str,
    name: &str,
    sha: &str,
) -> Result<Vec<(String, String)>, String> {
    if let Some(syn) = synthetic_at(scanner, owner, name, sha)? {
        let usfm = syn.usfm();
        return Ok(vec![(usfm.path.clone(), usfm.text.clone())]);
    }
    let reader = RepoReader::open(&scanner.repo_path(owner, name)).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for entry in reader.root_files(sha).map_err(|e| e.to_string())? {
        if entry.kind != EntryKind::Blob || !is_usfm_path(&entry.path) {
            continue;
        }
        let bytes = reader.blob(entry.oid).map_err(|e| e.to_string())?;
        out.push((entry.path, String::from_utf8_lossy(&bytes).into_owned()));
    }
    Ok(out)
}
