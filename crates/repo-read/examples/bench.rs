//! Times the read paths over every repo under `REPO_ROOT`:
//!   (a) `head()` for all repos (plain files), plus the same via libgit2 for comparison
//!   (b) the full recursive tree for every non-empty repo
//!   (c) reading every `.usfm` blob and every tS chunk (`NN/NN.txt`)
//!
//! `REPO_ROOT=dev-data/repositories cargo run -p hub-repo-read --release --example bench`

use std::path::Path;
use std::time::Instant;

use hub_repo_read::{EntryKind, Head, RepoId, RepoReader, head, list_repos};
use rayon::prelude::*;

/// tS chunk: last two path components are digits, the file is `<digits>.txt`.
fn is_chunk(path: &str) -> bool {
    let mut parts = path.rsplit('/');
    let (Some(file), Some(dir)) = (parts.next(), parts.next()) else {
        return false;
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    file.strip_suffix(".txt").is_some_and(digits) && digits(dir)
}

fn main() {
    let root = std::env::var("REPO_ROOT").expect("set REPO_ROOT");
    let t = Instant::now();
    let repos = list_repos(Path::new(&root)).unwrap();
    let list_time = t.elapsed();

    // (a) HEAD via plain files.
    let t = Instant::now();
    let heads: Vec<(&RepoId, Option<Head>)> = repos
        .par_iter()
        .map(|r| (r, head(&r.path).unwrap()))
        .collect();
    let head_time = t.elapsed();

    // Same thing via libgit2, as a baseline and a correctness check.
    let t = Instant::now();
    let git2_heads: Vec<Option<String>> = repos
        .par_iter()
        .map(|r| {
            let repo = git2::Repository::open_bare(&r.path).unwrap();
            repo.head()
                .ok()
                .and_then(|h| h.target())
                .map(|o| o.to_string())
        })
        .collect();
    let git2_head_time = t.elapsed();
    let mismatches = heads
        .iter()
        .zip(&git2_heads)
        .filter(|((_, h), g)| h.as_ref().map(|h| &h.sha) != g.as_ref())
        .count();

    let live: Vec<(&RepoId, &Head)> = heads
        .iter()
        .filter_map(|(r, h)| h.as_ref().map(|h| (*r, h)))
        .collect();

    // (b) Full recursive tree per repo.
    let t = Instant::now();
    let trees: Vec<_> = live
        .par_iter()
        .map(|(r, h)| {
            let reader = RepoReader::open(&r.path).unwrap();
            reader.tree(&h.sha).unwrap()
        })
        .collect();
    let tree_time = t.elapsed();
    let entries: usize = trees.iter().map(Vec::len).sum();

    // (c) Read every .usfm and tS chunk blob.
    let t = Instant::now();
    let (files, bytes) = live
        .par_iter()
        .zip(&trees)
        .map(|((r, _), tree)| {
            let reader = RepoReader::open(&r.path).unwrap();
            let mut n = 0u64;
            let mut b = 0u64;
            for e in tree.iter().filter(|e| e.kind == EntryKind::Blob) {
                if e.path.ends_with(".usfm") || is_chunk(&e.path) {
                    b += reader.blob(e.oid).unwrap().len() as u64;
                    n += 1;
                }
            }
            (n, b)
        })
        .reduce(|| (0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    let blob_time = t.elapsed();

    let mut branches = std::collections::BTreeMap::<String, usize>::new();
    for (_, h) in &live {
        *branches
            .entry(h.branch.clone().unwrap_or_else(|| "(detached)".into()))
            .or_default() += 1;
    }

    println!("threads: {}", rayon::current_num_threads());
    println!("{:<34} {:>9} {:>10}", "step", "secs", "count");
    println!(
        "{:<34} {:>9.3} {:>10}",
        "list_repos",
        list_time.as_secs_f64(),
        repos.len()
    );
    println!(
        "{:<34} {:>9.3} {:>10}",
        "head() plain files",
        head_time.as_secs_f64(),
        repos.len()
    );
    println!(
        "{:<34} {:>9.3} {:>10}",
        "head via git2 (baseline)",
        git2_head_time.as_secs_f64(),
        repos.len()
    );
    println!(
        "{:<34} {:>9.3} {:>10}",
        "tree() recursive, entries",
        tree_time.as_secs_f64(),
        entries
    );
    println!(
        "{:<34} {:>9.3} {:>10}",
        "blobs .usfm + NN/NN.txt, files",
        blob_time.as_secs_f64(),
        files
    );
    println!("blob bytes: {:.1} MiB", bytes as f64 / 1048576.0);
    println!("empty repos: {}", repos.len() - live.len());
    println!("head mismatches vs git2: {mismatches}");
    println!("HEAD branches: {branches:?}");
}
