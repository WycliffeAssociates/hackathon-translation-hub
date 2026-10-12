//! Real Writer repos, stitched. Runs only when `REPO_ROOT` names a folder of
//! bare repos (`owner/name.git`); without it the test passes and says so.
//!
//! `STITCH_REPOS` (comma-separated `owner/name.git`) replaces the default list,
//! and `STITCH_DUMP_DIR`, when set, receives each book as
//! `owner__name.usfm` — for comparing byte for byte with Sefer's own output.
//!
//! The repos are read with the `git` command, not a library: this crate has no
//! dependencies, and the test needs only a tree listing and blobs.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use hub_stitch::{decode_text, is_writer_text_path, tree_from_files, writer_book_to_usfm};

const REPOS: &[&str] = &[
    "kpass/tmf_3jn_text_reg.git",
    "bahasatech.indotimur/kps-x-kaigar_3jn_text_reg.git",
    "chunga_major/bem-x-ngumbo_2ch_text_reg.git",
    "andriatiana_wa/msh_job_text_udb.git",
    "bahasatech.indotimur/saw_isa_text_reg.git",
];

/// `kps-x-kaigar_3jn_text_reg` → `3JN`: tS names a project
/// `{language}_{book}_{type}_{resource}`.
fn book_of(repo: &str) -> String {
    let name = repo.rsplit('/').next().unwrap().trim_end_matches(".git");
    let parts: Vec<&str> = name.split('_').collect();
    let at = parts
        .iter()
        .position(|p| *p == "text")
        .expect("a tS text repo name");
    parts[at - 1].to_uppercase()
}

/// Every Writer text file at HEAD, as raw bytes.
fn read_files(git_dir: &Path) -> Vec<(String, Vec<u8>)> {
    let listing = Command::new("git")
        .arg("--git-dir")
        .arg(git_dir)
        .args(["ls-tree", "-r", "--name-only", "HEAD"])
        .output()
        .expect("git runs");
    assert!(listing.status.success(), "ls-tree {}", git_dir.display());
    let paths: Vec<String> = String::from_utf8(listing.stdout)
        .unwrap()
        .lines()
        .filter(|p| is_writer_text_path(p))
        .map(str::to_string)
        .collect();

    // One `cat-file --batch` for the lot, not a `git show` per chunk.
    let mut child = Command::new("git")
        .arg("--git-dir")
        .arg(git_dir)
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("git runs");
    let mut stdin = child.stdin.take().unwrap();
    let requests: String = paths.iter().map(|p| format!("HEAD:{p}\n")).collect();
    let writer = std::thread::spawn(move || stdin.write_all(requests.as_bytes()));
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut files = Vec::new();
    for path in paths {
        let mut header = String::new();
        out.read_line(&mut header).unwrap();
        let size: usize = header
            .split_whitespace()
            .nth(2)
            .expect("a blob")
            .parse()
            .unwrap();
        let mut body = vec![0; size + 1];
        out.read_exact(&mut body).unwrap();
        body.pop();
        files.push((path, body));
    }
    writer.join().unwrap().unwrap();
    child.wait().unwrap();
    files
}

#[test]
fn real_writer_repos_stitch() {
    let Ok(root) = std::env::var("REPO_ROOT") else {
        eprintln!("REPO_ROOT is not set; skipping the golden Writer repos");
        return;
    };
    let chosen: Vec<String> = match std::env::var("STITCH_REPOS") {
        Ok(list) => list
            .split(',')
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty())
            .collect(),
        Err(_) => REPOS.iter().map(|r| r.to_string()).collect(),
    };
    let dump = std::env::var("STITCH_DUMP_DIR").ok();
    let mut stitched = 0;
    for repo in &chosen {
        let git_dir = Path::new(&root).join(repo);
        if !git_dir.exists() {
            eprintln!("{repo}: not under REPO_ROOT; skipped");
            continue;
        }
        let book = book_of(repo);
        let raw = read_files(&git_dir);
        let decoded: Vec<(&str, String)> = raw
            .iter()
            .filter_map(|(path, bytes)| decode_text(bytes).map(|text| (path.as_str(), text)))
            .collect();
        let tree = tree_from_files(decoded.iter().map(|(p, t)| (*p, t.as_str())));
        let usfm = writer_book_to_usfm(&tree, &book, &book);

        assert!(
            usfm.starts_with(&format!("\\id {book}\n\\ide UTF-8\n")),
            "{repo}"
        );
        assert!(usfm.ends_with('\n'), "{repo}");
        if tree.chapters.is_empty() {
            // A project created and never written in (only `manifest.json`
            // and `LICENSE.md`) is a header and nothing else, as in Sefer.
            eprintln!("{repo}: no chapter files");
        }

        // Exactly one `\c N` line per chapter, in order, and none doubled.
        let markers: Vec<u32> = usfm
            .lines()
            .filter_map(|line| line.strip_prefix("\\c "))
            .filter_map(|rest| rest.parse().ok())
            .collect();
        let numbers: Vec<u32> = tree.chapters.iter().map(|c| c.number).collect();
        assert_eq!(markers, numbers, "{repo}: chapter markers");
        let distinct: BTreeSet<u32> = numbers.iter().copied().collect();
        assert_eq!(distinct.len(), numbers.len(), "{repo}");
        for n in &numbers {
            assert!(
                !usfm.contains(&format!("\\c {n} \\c {n}")),
                "{repo}: doubled \\c {n}"
            );
            assert!(
                !usfm.contains(&format!("\\c {n}\n\\c {n}\n")),
                "{repo}: doubled \\c {n}"
            );
        }
        assert!(!usfm.contains('\r'), "{repo}: a carriage return survived");

        eprintln!(
            "{repo}: {book}, {} chapters, {} labelled, {} bytes",
            tree.chapters.len(),
            tree.chapters.iter().filter(|c| c.label.is_some()).count(),
            usfm.len()
        );
        if let Some(dir) = &dump {
            let name = repo.trim_end_matches(".git").replace('/', "__");
            std::fs::write(Path::new(dir).join(format!("{name}.usfm")), &usfm).unwrap();
        }
        stitched += 1;
    }
    eprintln!("stitched {stitched} of {} repos", chosen.len());
}
