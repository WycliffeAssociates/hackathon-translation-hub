//! Proofreads one bare repo and prints a summary, or the whole proof as JSON.
//!
//! ```text
//! cargo run --release -p hub-proof --example proofread_repo -- dev-data/repositories/wa-catalog/en_ulb.git
//!   en_ulb: 66 books, 0 skipped, 1,234 findings in 210 ms (read 40 ms)
//!     Rarity 400 · Casing 300 · …
//! cargo run --release -p hub-proof --example proofread_repo -- --json <repo.git> > proof.json
//! ```
//!
//! A consolidated repo is proofread as its root `*.usfm` / `*.sfm` files; a
//! Writer repo as the one book `hub-synthetic` stitches from it.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use hub_proof::{Book, proofread};
use hub_repo_read::{EntryKind, RepoReader, head};
use hub_synthetic::{Upstream, synthesize};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut json = false;
    let mut repos = Vec::new();
    for arg in std::env::args().skip(1) {
        if arg == "--json" {
            json = true;
        } else {
            repos.push(arg);
        }
    }
    if repos.is_empty() {
        return Err("usage: proofread_repo [--json] <repo.git>...".into());
    }
    for repo in repos {
        run(Path::new(&repo), json)?;
    }
    Ok(())
}

fn run(path: &Path, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
    let Some(head) = head(path)? else {
        eprintln!("{}: empty repo", path.display());
        return Ok(());
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
    let read = started.elapsed();

    let books: Vec<Book> = files.iter().map(|(id, text)| Book { id, text }).collect();
    let started = Instant::now();
    let proof = proofread(&books)?;
    let took = started.elapsed();

    if json {
        println!("{}", serde_json::to_string_pretty(&proof)?);
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
