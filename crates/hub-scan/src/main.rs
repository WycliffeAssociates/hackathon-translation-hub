//! hub-scan: run the hub's pipeline over a directory of bare repos and write
//! what it found as JSON. Nothing is stored and nothing in the repos is written.
//!
//!     hub-scan <repo-root> [--out scan.json] [--synthetic-dir DIR]
//!              [--base-url https://content.bibletranslationtools.org] [--only owner/name]
//!
//! Per repo: HEAD, the manifest, then per book a lint summary keyed by content.
//! A Writer repo becomes its synthetic repo first (one stitched book). A book
//! whose manifest declares a source that resolves to `wa-catalog/<lang>_<id>`
//! in the same root also gets verse coverage against it.
//!
//! The JSON doubles as fixtures for web work; `--synthetic-dir` writes each
//! synthetic repo's files out as they would be served.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use hub_checks::{
    Coverage, LintSummary, content_key_for_blob, content_key_for_chunks, coverage, lint_book,
    verses_by_chapter,
};
use hub_manifest::{ContainerFormat, NormalizedManifest, find_manifest, parse_as};
use hub_repo_read::{EntryKind, RepoId, RepoReader, head, list_repos};
use hub_synthetic::{SyntheticRepo, Upstream, synthesize};
use rayon::prelude::*;
use serde::Serialize;

struct Args {
    root: PathBuf,
    out: PathBuf,
    synthetic_dir: Option<PathBuf>,
    base_url: String,
    only: Option<String>,
}

fn args() -> Args {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        root: PathBuf::new(),
        out: "scan.json".into(),
        synthetic_dir: None,
        base_url: "https://content.bibletranslationtools.org".into(),
        only: None,
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => a.out = it.next().expect("--out FILE").into(),
            "--synthetic-dir" => {
                a.synthetic_dir = Some(it.next().expect("--synthetic-dir DIR").into())
            }
            "--base-url" => a.base_url = it.next().expect("--base-url URL"),
            "--only" => a.only = Some(it.next().expect("--only owner/name").to_lowercase()),
            _ if a.root.as_os_str().is_empty() => a.root = arg.into(),
            _ => panic!("unknown argument {arg}"),
        }
    }
    assert!(
        !a.root.as_os_str().is_empty(),
        "usage: hub-scan <repo-root> [--out FILE] [--synthetic-dir DIR] [--base-url URL] [--only owner/name]"
    );
    a
}

#[derive(Serialize)]
struct Scan {
    kitchen_tag: &'static str,
    root: String,
    seconds: f64,
    totals: Totals,
    repos: Vec<RepoReport>,
}

#[derive(Serialize, Default)]
struct Totals {
    repos: usize,
    empty: usize,
    failed: usize,
    by_format: BTreeMap<String, usize>,
    synthetic: usize,
    books: usize,
    books_with_coverage: usize,
    errors: u64,
    warnings: u64,
    infos: u64,
}

#[derive(Serialize)]
struct RepoReport {
    owner: String,
    name: String,
    branch: Option<String>,
    sha: Option<String>,
    manifest: Option<ManifestReport>,
    /// Set for a Writer repo: what the hub serves instead of its chunk files.
    synthetic: Option<SyntheticReport>,
    books: Vec<BookReport>,
    error: Option<String>,
}

#[derive(Serialize)]
struct ManifestReport {
    file: String,
    format: Option<String>,
    language: Option<String>,
    resource: Option<String>,
    subject: Option<String>,
    books: Vec<String>,
    sources: Vec<String>,
    finished_chunks: Option<usize>,
    error: Option<String>,
}

#[derive(Serialize)]
struct SyntheticReport {
    file: String,
    title: String,
    chapters: Vec<u32>,
    chunk_files: usize,
}

#[derive(Serialize)]
struct BookReport {
    book: String,
    path: String,
    content_key: String,
    lint: LintSummary,
    coverage: Option<CoverageReport>,
}

#[derive(Serialize)]
struct CoverageReport {
    source: String,
    source_verses: u32,
    present: u32,
    missing: u32,
    extra: u32,
    /// Only chapters with missing verses, as `chapter → missing verses`.
    gaps: BTreeMap<u32, Vec<u32>>,
}

/// Source verse sets, read once per (source repo, book) across all threads.
type SourceCache = Mutex<HashMap<(String, String), Option<Arc<BTreeMap<u32, BTreeSet<u32>>>>>>;

fn main() {
    let args = args();
    let started = Instant::now();
    let mut repos = list_repos(&args.root).expect("list repos");
    if let Some(only) = &args.only {
        repos.retain(|r| format!("{}/{}", r.owner, r.name) == *only);
    }
    let sources: SourceCache = Mutex::new(HashMap::new());
    let reports: Vec<RepoReport> = repos
        .par_iter()
        .map(|r| scan_repo(r, &args, &sources))
        .collect();

    let mut totals = Totals {
        repos: reports.len(),
        ..Totals::default()
    };
    for r in &reports {
        totals.empty += usize::from(r.sha.is_none() && r.error.is_none());
        totals.failed += usize::from(r.error.is_some());
        let format = r
            .manifest
            .as_ref()
            .and_then(|m| m.format.clone())
            .unwrap_or_else(|| "none".into());
        *totals.by_format.entry(format).or_default() += 1;
        totals.synthetic += usize::from(r.synthetic.is_some());
        for b in &r.books {
            totals.books += 1;
            totals.books_with_coverage += usize::from(b.coverage.is_some());
            totals.errors += u64::from(b.lint.errors);
            totals.warnings += u64::from(b.lint.warnings);
            totals.infos += u64::from(b.lint.infos);
        }
    }
    let scan = Scan {
        kitchen_tag: hub_checks::KITCHEN_TAG,
        root: args.root.display().to_string(),
        seconds: started.elapsed().as_secs_f64(),
        totals,
        repos: reports,
    };
    std::fs::write(
        &args.out,
        serde_json::to_string_pretty(&scan).expect("serialize"),
    )
    .expect("write output");
    eprintln!(
        "{}",
        serde_json::to_string_pretty(&scan.totals).expect("serialize")
    );
    eprintln!("{:.1}s · wrote {}", scan.seconds, args.out.display());
}

fn scan_repo(id: &RepoId, args: &Args, sources: &SourceCache) -> RepoReport {
    let mut report = RepoReport {
        owner: id.owner.clone(),
        name: id.name.clone(),
        branch: None,
        sha: None,
        manifest: None,
        synthetic: None,
        books: Vec::new(),
        error: None,
    };
    if let Err(e) = fill(&mut report, id, args, sources) {
        report.error = Some(e);
    }
    report
}

fn fill(
    report: &mut RepoReport,
    id: &RepoId,
    args: &Args,
    sources: &SourceCache,
) -> Result<(), String> {
    let Some(h) = head(&id.path).map_err(|e| e.to_string())? else {
        return Ok(());
    };
    report.branch = h.branch;
    report.sha = Some(h.sha.clone());
    let reader = RepoReader::open(&id.path).map_err(|e| e.to_string())?;
    let root: Vec<String> = reader
        .root_files(&h.sha)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|e| e.path)
        .collect();

    let manifest =
        find_manifest(&root).map(|(file, format)| read_manifest(&reader, &h.sha, file, format));
    let normalized = manifest.as_ref().and_then(|(_, m)| m.clone());
    report.manifest = manifest.map(|(r, _)| r);
    let source = normalized
        .as_ref()
        .and_then(|m| resolve_source(m, &args.root));

    // A Writer repo is served as its synthetic repo.
    let url = format!("{}/{}/{}.git", args.base_url, id.owner, id.name);
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
        report.books.push(book_report(
            &syn.book,
            &usfm.path,
            content_key_for_chunks(&syn.chunks),
            &usfm.text,
            source.as_ref(),
            &args.root,
            sources,
        ));
        report.synthetic = Some(SyntheticReport {
            file: usfm.path.clone(),
            title: syn.title.clone(),
            chapters: syn.chapters.clone(),
            chunk_files: syn.chunks.len(),
        });
        if let Some(dir) = &args.synthetic_dir {
            write_synthetic(dir, id, &syn).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    // Otherwise every root USFM file is a book.
    for entry in reader.root_files(&h.sha).map_err(|e| e.to_string())? {
        let lower = entry.path.to_ascii_lowercase();
        if entry.kind != EntryKind::Blob || !(lower.ends_with(".usfm") || lower.ends_with(".sfm")) {
            continue;
        }
        let bytes = reader.blob(entry.oid).map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&bytes);
        let book = book_id(&text).unwrap_or_default();
        report.books.push(book_report(
            &book,
            &entry.path,
            content_key_for_blob(&entry.oid.to_string()),
            &text,
            source.as_ref(),
            &args.root,
            sources,
        ));
    }
    Ok(())
}

fn read_manifest(
    reader: &RepoReader,
    sha: &str,
    file: &str,
    format: ContainerFormat,
) -> (ManifestReport, Option<NormalizedManifest>) {
    let mut report = ManifestReport {
        file: file.to_string(),
        format: Some(format!("{format:?}")),
        language: None,
        resource: None,
        subject: None,
        books: Vec::new(),
        sources: Vec::new(),
        finished_chunks: None,
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
            (report, Some(m))
        }
        Err(e) => {
            report.error = Some(e);
            (report, None)
        }
    }
}

/// `wa-catalog/<lang>_<identifier>` in the same root, for the first declared
/// source that has one. The convention the catalog follows; lowercase on disk.
fn resolve_source(m: &NormalizedManifest, root: &Path) -> Option<String> {
    m.sources.iter().find_map(|s| {
        let repo =
            format!("wa-catalog/{}_{}", s.language_tag.as_deref()?, s.identifier).to_lowercase();
        root.join(format!("{repo}.git")).is_dir().then_some(repo)
    })
}

fn book_report(
    book: &str,
    path: &str,
    content_key: String,
    text: &str,
    source: Option<&String>,
    root: &Path,
    cache: &SourceCache,
) -> BookReport {
    let coverage = source.and_then(|repo| {
        let verses = source_verses(root, repo, book, cache)?;
        Some(coverage_report(
            repo,
            coverage(&verses_by_chapter(text), &verses),
        ))
    });
    BookReport {
        book: book.to_string(),
        path: path.to_string(),
        content_key,
        lint: lint_book(text),
        coverage,
    }
}

fn coverage_report(source: &str, c: Coverage) -> CoverageReport {
    CoverageReport {
        source: source.to_string(),
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

/// The source repo's verses for `book`, from its `NN-BOOK.usfm`; read once.
fn source_verses(
    root: &Path,
    repo: &str,
    book: &str,
    cache: &SourceCache,
) -> Option<Arc<BTreeMap<u32, BTreeSet<u32>>>> {
    let key = (repo.to_string(), book.to_string());
    if let Some(hit) = cache.lock().expect("cache lock").get(&key) {
        return hit.clone();
    }
    let read = || -> Option<Arc<BTreeMap<u32, BTreeSet<u32>>>> {
        let path = root.join(format!("{repo}.git"));
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
    cache.lock().expect("cache lock").insert(key, value.clone());
    value
}

/// The book a USFM file declares in its `\id` line.
fn book_id(text: &str) -> Option<String> {
    let rest = text
        .trim_start_matches('\u{feff}')
        .trim_start()
        .strip_prefix("\\id")?;
    let id: String = rest.trim_start().chars().take(3).collect();
    (id.len() == 3).then(|| id.to_ascii_uppercase())
}

fn write_synthetic(dir: &Path, id: &RepoId, syn: &SyntheticRepo) -> std::io::Result<()> {
    let out = dir.join(&id.owner).join(&id.name);
    std::fs::create_dir_all(&out)?;
    for f in &syn.files {
        std::fs::write(out.join(&f.path), &f.text)?;
    }
    Ok(())
}
