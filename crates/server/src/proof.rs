//! Sous Chef runs for the poll: one per repo whose HEAD moved since its last
//! run. Findings are corpus-relative, so a run covers the whole repo.
//!
//! Some texts produce findings by the hundred thousand (fa_opv's word-final
//! ZWNJ: 235k `MisplacedFormat`), which no page can show. The stored body
//! keeps the first [`KEEP_PER_CODE`] findings of each code and says how many
//! it dropped; the counts in `by_rule` stay exact.

use std::collections::BTreeMap;
use std::time::Instant;

use hub_ingest::{Scanner, book_texts};
use hub_store::{ProofRun, StaleRepo};
use serde::Serialize;

pub const KEEP_PER_CODE: usize = 500;

/// What `/v1/repos/{owner}/{name}/proof` serves.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ProofBody {
    pub books: Vec<hub_proof::ProofBook>,
    pub skipped: Vec<hub_proof::Skipped>,
    pub findings: Vec<hub_proof::Finding>,
    /// Code → findings left out of `findings` past the per-code cap.
    pub dropped: BTreeMap<String, usize>,
}

pub fn run(scanner: &Scanner, repo: &StaleRepo) -> Option<ProofRun<ProofBody>> {
    let sha = repo.head_sha.clone()?;
    let started = Instant::now();
    let result = book_texts(scanner, &repo.owner, &repo.name, &sha).and_then(|texts| {
        let books: Vec<hub_proof::Book> = texts
            .iter()
            .map(|(id, text)| hub_proof::Book { id, text })
            .collect();
        hub_proof::proofread(&books).map_err(|e| e.to_string())
    });
    let millis = started.elapsed().as_millis() as i64;
    Some(match result {
        Ok(proof) => {
            let total = proof.findings.len();
            let mut by_book = BTreeMap::new();
            let mut by_rule: BTreeMap<String, usize> = BTreeMap::new();
            let mut kept = Vec::new();
            let mut dropped: BTreeMap<String, usize> = BTreeMap::new();
            for f in proof.findings {
                *by_book.entry(f.book.clone()).or_default() += 1;
                let n = by_rule.entry(f.code.to_owned()).or_default();
                *n += 1;
                if *n <= KEEP_PER_CODE {
                    kept.push(f);
                } else {
                    *dropped.entry(f.code.to_owned()).or_default() += 1;
                }
            }
            ProofRun {
                repo_id: repo.id,
                head_sha: sha,
                millis,
                findings: total,
                by_book,
                by_rule,
                body: ProofBody {
                    books: proof.books,
                    skipped: proof.skipped,
                    findings: kept,
                    dropped,
                },
                error: None,
            }
        }
        Err(e) => ProofRun {
            repo_id: repo.id,
            head_sha: sha,
            millis,
            findings: 0,
            by_book: BTreeMap::new(),
            by_rule: BTreeMap::new(),
            body: ProofBody {
                books: Vec::new(),
                skipped: Vec::new(),
                findings: Vec::new(),
                dropped: BTreeMap::new(),
            },
            error: Some(e),
        },
    })
}
