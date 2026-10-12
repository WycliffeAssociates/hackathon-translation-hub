//! hub-scan: run the hub's pipeline over a directory of bare repos and write
//! what it found as JSON, or into the hub's SQLite. Nothing in the repos is
//! written.
//!
//!     hub-scan <repo-root> [--out scan.json] [--synthetic-dir DIR]
//!              [--db hub.sqlite --forgejo-dump repository.sql.gz]
//!              [--base-url https://content.bibletranslationtools.org] [--only owner/name]
//!
//! With `--db`, the repo list comes from Forgejo's `repository` table (public
//! repos only), so `--forgejo-dump` is required; every public repo on disk is
//! scanned and stored, the same as the server's first poll. Without it, every
//! repo on disk is scanned and the JSON doubles as fixtures for web work;
//! `--synthetic-dir` writes each synthetic repo's files as they would be served.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use hub_ingest::{Config, RepoScan, Scanner, synthetic_at};
use hub_repo_read::list_repos;
use hub_store::Store;
use rayon::prelude::*;
use serde::Serialize;

struct Args {
    root: PathBuf,
    out: PathBuf,
    synthetic_dir: Option<PathBuf>,
    db: Option<PathBuf>,
    forgejo_dump: Option<PathBuf>,
    base_url: String,
    only: Option<String>,
}

fn args() -> Args {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        root: PathBuf::new(),
        out: "scan.json".into(),
        synthetic_dir: None,
        db: None,
        forgejo_dump: None,
        base_url: "https://content.bibletranslationtools.org".into(),
        only: None,
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => a.out = it.next().expect("--out FILE").into(),
            "--synthetic-dir" => {
                a.synthetic_dir = Some(it.next().expect("--synthetic-dir DIR").into())
            }
            "--db" => a.db = Some(it.next().expect("--db FILE").into()),
            "--forgejo-dump" => {
                a.forgejo_dump = Some(it.next().expect("--forgejo-dump FILE").into())
            }
            "--base-url" => a.base_url = it.next().expect("--base-url URL"),
            "--only" => a.only = Some(it.next().expect("--only owner/name").to_lowercase()),
            _ if a.root.as_os_str().is_empty() => a.root = arg.into(),
            _ => panic!("unknown argument {arg}"),
        }
    }
    assert!(
        !a.root.as_os_str().is_empty(),
        "usage: hub-scan <repo-root> [--out FILE] [--synthetic-dir DIR] [--db FILE --forgejo-dump FILE] [--base-url URL] [--only owner/name]"
    );
    assert!(
        a.db.is_none() || a.forgejo_dump.is_some(),
        "--db needs --forgejo-dump: only repos Forgejo lists as public are stored"
    );
    a
}

#[derive(Serialize)]
struct Scan {
    kitchen_tag: &'static str,
    root: String,
    seconds: f64,
    totals: Totals,
    repos: Vec<RepoScan>,
}

#[derive(Serialize, Default)]
struct Totals {
    repos: usize,
    empty: usize,
    failed: usize,
    by_shape: BTreeMap<String, usize>,
    synthetic: usize,
    books: usize,
    books_with_coverage: usize,
    errors: u64,
    warnings: u64,
    infos: u64,
}

fn main() {
    let args = args();
    let started = Instant::now();
    let scanner = Scanner::new(Config {
        root: args.root.clone(),
        base_url: args.base_url.clone(),
    });

    if let Some(db) = &args.db {
        to_store(&args, db, &scanner, started);
        return;
    }

    let mut repos = list_repos(&args.root).expect("list repos");
    if let Some(only) = &args.only {
        repos.retain(|r| format!("{}/{}", r.owner, r.name) == *only);
    }
    let scans: Vec<RepoScan> = repos
        .par_iter()
        .map(|r| scanner.scan(&r.owner, &r.name))
        .collect();
    if let Some(dir) = &args.synthetic_dir {
        for s in scans.iter().filter(|s| s.synthetic.is_some()) {
            write_synthetic(dir, &scanner, s).expect("write synthetic repo");
        }
    }
    let scan = Scan {
        kitchen_tag: hub_checks::KITCHEN_TAG,
        root: args.root.display().to_string(),
        seconds: started.elapsed().as_secs_f64(),
        totals: totals(&scans),
        repos: scans,
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

/// The server's first poll, from the command line: Forgejo's public repos
/// that are on disk, scanned in parallel, written one by one.
fn to_store(args: &Args, db: &Path, scanner: &Scanner, started: Instant) {
    let store = Store::open(db).expect("open store");
    let dump = args.forgejo_dump.as_ref().expect("checked in args");
    let forgejo = hub_ingest::forgejo::from_dump(dump).expect("read Forgejo dump");
    let counts = store
        .sync_forgejo(&forgejo, |r| scanner.repo_path(&r.owner, &r.name).is_dir())
        .expect("sync repos");
    let mut targets: Vec<_> = store
        .scan_marks()
        .expect("read repos")
        .into_iter()
        .map(|m| m.repo)
        .collect();
    if let Some(only) = &args.only {
        targets.retain(|r| format!("{}/{}", r.owner, r.name) == *only);
    }
    let scans: Vec<(i64, RepoScan)> = targets
        .par_iter()
        .map(|r| (r.id, scanner.scan(&r.owner, &r.name)))
        .collect();
    for (id, scan) in &scans {
        store
            .write_scan(*id, scan, hub_checks::KITCHEN_TAG)
            .expect("write scan");
    }
    let scans: Vec<RepoScan> = scans.into_iter().map(|(_, s)| s).collect();
    eprintln!(
        "{}",
        serde_json::to_string_pretty(&totals(&scans)).expect("serialize")
    );
    eprintln!(
        "{:.1}s · {} public repos on disk ({} rows removed) · wrote {}",
        started.elapsed().as_secs_f64(),
        counts.public,
        counts.removed,
        db.display()
    );
}

fn totals(scans: &[RepoScan]) -> Totals {
    let mut t = Totals {
        repos: scans.len(),
        ..Totals::default()
    };
    for r in scans {
        t.empty += usize::from(r.sha.is_none() && r.error.is_none());
        t.failed += usize::from(r.error.is_some());
        *t.by_shape.entry(r.shape.as_str().to_owned()).or_default() += 1;
        t.synthetic += usize::from(r.synthetic.is_some());
        for b in &r.books {
            t.books += 1;
            t.books_with_coverage += usize::from(b.coverage.is_some());
            t.errors += u64::from(b.lint.errors);
            t.warnings += u64::from(b.lint.warnings);
            t.infos += u64::from(b.lint.infos);
        }
    }
    t
}

fn write_synthetic(dir: &Path, scanner: &Scanner, scan: &RepoScan) -> std::io::Result<()> {
    let Some(sha) = &scan.sha else { return Ok(()) };
    let Ok(Some(syn)) = synthetic_at(scanner, &scan.owner, &scan.name, sha) else {
        return Ok(());
    };
    let out = dir.join(&scan.owner).join(&scan.name);
    std::fs::create_dir_all(&out)?;
    for f in &syn.files {
        std::fs::write(out.join(&f.path), &f.text)?;
    }
    Ok(())
}
