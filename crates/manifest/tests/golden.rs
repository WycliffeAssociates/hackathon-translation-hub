//! Golden run over real repos. Set `REPO_ROOT` to a directory of
//! `<owner>/<repo>.git` bare repos; without it the test is skipped.
//!
//! For each repo: list root files with `git ls-tree`, pick the manifest with
//! `find_manifest`, read it with `git show`, parse and normalize. Prints counts by
//! container format and every failure. Fails if any manifest fails to parse.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use hub_manifest::{find_manifest, parse_as};

fn git(repo: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .arg("--git-dir")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

#[test]
fn golden_real_repos() {
    let Ok(root) = std::env::var("REPO_ROOT") else {
        eprintln!("REPO_ROOT unset; skipping golden run");
        return;
    };

    let mut repos = Vec::new();
    for owner in std::fs::read_dir(&root).expect("read REPO_ROOT") {
        let owner = owner.unwrap().path();
        if !owner.is_dir() {
            continue;
        }
        for repo in std::fs::read_dir(&owner).unwrap() {
            let repo = repo.unwrap().path();
            if repo.extension().is_some_and(|e| e == "git") {
                repos.push(repo);
            }
        }
    }
    repos.sort();

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    for repo in &repos {
        let name = repo.strip_prefix(&root).unwrap().display().to_string();
        // An empty repo has no HEAD commit, so ls-tree fails.
        let Some(listing) = git(repo, &["ls-tree", "--name-only", "HEAD"]) else {
            *counts.entry("empty repo".into()).or_default() += 1;
            println!("empty: {name}");
            continue;
        };
        let listing = String::from_utf8_lossy(&listing);
        let files: Vec<&str> = listing.lines().collect();
        let Some((file, format)) = find_manifest(&files) else {
            *counts.entry("no manifest".into()).or_default() += 1;
            continue;
        };
        let bytes = git(repo, &["show", &format!("HEAD:{file}")]).expect("git show");
        match parse_as(format, &bytes) {
            Ok(parsed) => {
                let n = parsed.normalize();
                *counts
                    .entry(format!("{:?}", n.container_format))
                    .or_default() += 1;
            }
            Err(e) => failures.push(format!("{name}/{file}: {e}")),
        }
    }

    println!("repos: {}", repos.len());
    for (k, v) in &counts {
        println!("{k:>16}: {v}");
    }
    println!("parse failures: {}", failures.len());
    for f in &failures {
        println!("  {f}");
    }
    assert!(
        failures.is_empty(),
        "{} manifests failed to parse",
        failures.len()
    );
}
