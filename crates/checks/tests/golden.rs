//! Golden sweep over real data, gated on `REPO_ROOT` (the dev-data
//! repositories directory). Skips silently without it. Run in release for the
//! throughput number:
//!
//! ```text
//! REPO_ROOT=$PWD/dev-data/repositories \
//!   cargo test -p hub-checks --release --test golden -- --nocapture
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use hub_checks::{LintSummary, coverage, lint_book, verses_by_chapter};
use rayon::prelude::*;

fn git(repo: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("--git-dir")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every bare repo (`*.git` directory) up to two levels under `root`.
fn repos(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if path.extension().is_some_and(|e| e == "git") {
                out.push(path);
            } else if depth < 1 {
                stack.push((path, depth + 1));
            }
        }
    }
    out.sort();
    out
}

/// Every root `*.usfm` at HEAD, as (repo, file, text).
fn books(repo: &Path) -> Vec<(PathBuf, String, String)> {
    let Some(names) = git(repo, &["ls-tree", "--name-only", "HEAD"]) else {
        return Vec::new();
    };
    names
        .lines()
        .filter(|name| name.to_ascii_lowercase().ends_with(".usfm"))
        .filter_map(|name| {
            let text = git(repo, &["show", &format!("HEAD:{name}")])?;
            Some((repo.to_path_buf(), name.to_owned(), text))
        })
        .collect()
}

/// A crude tS stitch for the sanity print only (hub-stitch does this properly):
/// every `NN/NN.txt` chunk in path order, behind an `\id`, with a `\c` added
/// at each new chapter directory whose chunks do not carry one.
fn stitch_ts(repo: &Path, book: &str) -> Option<String> {
    let names = git(repo, &["ls-tree", "-r", "--name-only", "HEAD"])?;
    let mut chunks: Vec<&str> = names
        .lines()
        .filter(|n| {
            let mut parts = n.split('/');
            let (Some(ch), Some(file), None) = (parts.next(), parts.next(), parts.next()) else {
                return false;
            };
            ch.bytes().all(|b| b.is_ascii_digit())
                && file.ends_with(".txt")
                && file[..file.len() - 4].bytes().all(|b| b.is_ascii_digit())
        })
        .collect();
    chunks.sort();
    let mut out = format!("\\id {book}\n");
    let mut chapter = "";
    for chunk in chunks {
        let text = git(repo, &["show", &format!("HEAD:{chunk}")])?;
        let dir = chunk.split('/').next().unwrap_or_default();
        if dir != chapter {
            chapter = dir;
            if !text.contains("\\c ") {
                out.push_str(&format!("\\c {}\n\\p\n", dir.trim_start_matches('0')));
            }
        }
        out.push_str(&text);
        out.push('\n');
    }
    Some(out)
}

#[test]
fn golden_sweep() {
    let Ok(root) = std::env::var("REPO_ROOT") else {
        eprintln!("REPO_ROOT unset; skipping golden sweep");
        return;
    };
    let root = PathBuf::from(root);

    let all: Vec<_> = repos(&root).par_iter().flat_map_iter(|r| books(r)).collect();
    let bytes: usize = all.iter().map(|(_, _, t)| t.len()).sum();
    eprintln!("{} books, {:.1} MB from {}", all.len(), bytes as f64 / 1e6, root.display());

    let start = Instant::now();
    let summaries: Vec<LintSummary> = all.par_iter().map(|(_, _, text)| lint_book(text)).collect();
    let elapsed = start.elapsed();
    eprintln!(
        "linted in {:.2?}: {:.0} books/s ({} threads)",
        elapsed,
        all.len() as f64 / elapsed.as_secs_f64(),
        rayon::current_num_threads()
    );

    let (mut errors, mut warnings, mut infos) = (0u64, 0u64, 0u64);
    let mut codes: BTreeMap<&str, u64> = BTreeMap::new();
    for s in &summaries {
        errors += u64::from(s.errors);
        warnings += u64::from(s.warnings);
        infos += u64::from(s.infos);
        for (code, n) in &s.by_code {
            *codes.entry(code).or_default() += u64::from(*n);
        }
    }
    eprintln!("totals: {errors} errors, {warnings} warnings, {infos} infos");
    let mut top: Vec<_> = codes.into_iter().collect();
    top.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    for (code, n) in top.iter().take(10) {
        eprintln!("  {n:>8}  {code}");
    }

    // en_ulb Ruth: its \s5 chunk markers are registered, so none is unknown.
    let ruth = all.iter().zip(&summaries).find(|((repo, name, _), _)| {
        let repo = repo.to_string_lossy().to_ascii_lowercase();
        (repo.ends_with("wa-catalog/en_ulb.git") || repo.ends_with("wycliffeassociates/en_ulb.git"))
            && name.ends_with("RUT.usfm")
    });
    let ((_, _, text), summary) = ruth.expect("en_ulb Ruth in REPO_ROOT");
    assert!(hub_checks::uses_s5(text));
    eprintln!("en_ulb RUT: {summary:?}");
    assert_eq!(summary.by_code.get("unknown-marker"), None);

    // Sanity: a tS ULB target against en_ulb's same book.
    let target = root.join("dcs-mirror/alexandre_brazil_pt-br_ezk_text_ulb.git");
    let source = all.iter().find(|(repo, name, _)| {
        repo.to_string_lossy().to_ascii_lowercase().ends_with("wa-catalog/en_ulb.git")
            && name.ends_with("EZK.usfm")
    });
    if let (Some(stitched), Some((_, _, source))) = (stitch_ts(&target, "EZK"), source) {
        let cov = coverage(&verses_by_chapter(&stitched), &verses_by_chapter(source));
        eprintln!("pt-br EZK (tS) vs en_ulb EZK: {:?}", cov.total);
        let short: Vec<_> = cov.chapters.iter().filter(|c| !c.missing.is_empty()).take(5).collect();
        eprintln!("  first chapters with gaps: {short:?}");
    }
}
