//! Proofreads one bare repo and prints a summary, or the whole proof as JSON.
//!
//! ```text
//! cargo run --release -p hub-proof --example proofread_repo -- dev-data/repositories/wa-catalog/en_ulb.git
//!   en_ulb: 66 books, 0 skipped, 1,234 findings in 210 ms (read 40 ms)
//!     Rarity 400 · Casing 300 · …
//! cargo run --release -p hub-proof --example proofread_repo -- --json <repo.git> > proof.json
//! cargo run --release -p hub-proof --example proofread_repo -- --source wa-catalog/en_ulb.git [--copy] <repo.git>
//! ```
//!
//! A consolidated repo is proofread as its root `*.usfm` / `*.sfm` files; a
//! Writer repo as the one book `hub-synthetic` stitches from it.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use hub_proof::{Book, Source, proofread, proofread_paired};
use hub_repo_read::{EntryKind, RepoReader, head};
use hub_synthetic::{Upstream, synthesize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut json = false;
    let mut copy = false;
    let mut source = None;
    let mut repos = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--copy" => copy = true,
            "--source" => source = args.next(),
            _ => repos.push(arg),
        }
    }
    if repos.is_empty() {
        return Err(
            "usage: proofread_repo [--json] [--source <repo.git> [--copy]] <repo.git>...".into(),
        );
    }
    let source = match source {
        Some(path) => {
            let files = read_books(Path::new(&path))?;
            let books: Vec<Book> = files.iter().map(|(id, text)| Book { id, text }).collect();
            let started = Instant::now();
            let source = Source::prepare(&books);
            eprintln!(
                "source {path}: {} books prepared in {:.0?}",
                source.len(),
                started.elapsed()
            );
            Some(source)
        }
        None => None,
    };
    for repo in repos {
        run(Path::new(&repo), json, source.as_ref().map(|s| (s, copy)))?;
    }
    Ok(())
}

fn run(
    path: &Path,
    json: bool,
    source: Option<(&Source, bool)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    let files = read_books(path)?;
    let read = started.elapsed();

    let books: Vec<Book> = files.iter().map(|(id, text)| Book { id, text }).collect();
    let started = Instant::now();
    let proof = match source {
        Some((source, copy)) => proofread_paired(&books, source, copy)?,
        None => proofread(&books)?,
    };
    let took = started.elapsed();
    report(path, json, &files, &proof, took, read)
}

/// The repo's books at HEAD: the stitched book of a Writer repo, or every root
/// USFM file.
fn read_books(path: &Path) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let Some(head) = head(path)? else {
        return Ok(Vec::new());
    };
    let reader = RepoReader::open(path)?;
    let upstream = Upstream {
        url: "",
        commit: &head.sha,
    };
    let files: Vec<(String, String)> = match synthesize(&reader, &head.sha, &upstream)? {
        Some(synthetic) => {
            let usfm = synthetic.usfm();
            vec![(usfm.path.clone(), usfm.text.clone())]
        }
        None => {
            let mut files = Vec::new();
            for entry in reader.root_files(&head.sha)? {
                let lower = entry.path.to_ascii_lowercase();
                if entry.kind != EntryKind::Blob
                    || !(lower.ends_with(".usfm") || lower.ends_with(".sfm"))
                {
                    continue;
                }
                // A file that is not UTF-8 is left out, as the checks crate does.
                if let Ok(text) = String::from_utf8(reader.blob(entry.oid)?) {
                    files.push((entry.path, text));
                }
            }
            files
        }
    };
    Ok(files)
}

fn report(
    path: &Path,
    json: bool,
    files: &[(String, String)],
    proof: &hub_proof::Proof,
    took: std::time::Duration,
    read: std::time::Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        println!("{}", serde_json::to_string_pretty(proof)?);
        return Ok(());
    }
    let bytes: usize = files.iter().map(|(_, text)| text.len()).sum();
    println!(
        "{}: {} books ({:.1} MB), {} skipped, {} findings in {:.0?} (read {:.0?})",
        path.display(),
        proof.books.len(),
        bytes as f64 / 1e6,
        proof.skipped.len(),
        proof.findings.len(),
        took,
        read,
    );
    let mut by_code: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for finding in &proof.findings {
        *by_code.entry((finding.rule, finding.code)).or_default() += 1;
    }
    let mut by_code: Vec<_> = by_code.into_iter().collect();
    by_code.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let line: Vec<String> = by_code
        .iter()
        .map(|((rule, code), count)| format!("{rule}/{code} {count}"))
        .collect();
    println!("  {}", line.join(" · "));
    for skipped in &proof.skipped {
        println!("  skipped {}: {}", skipped.id, skipped.reason);
    }
    Ok(())
}
