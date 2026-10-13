//! Sous Chef runs for the poll: one per repo whose HEAD moved since its last
//! run. Findings are corpus-relative, so a run covers the whole repo.
//!
//! Every finding is counted (per book and check, in `finding_count`), but
//! rows are kept only up to [`ROWS_PER_BOOK_CODE`] per book and check: some
//! texts produce findings by the hundred thousand (Khmer's and Lao's word
//! separators, Persian's ZWNJ), and the pages read counts first and open rows
//! one book and one check at a time.

use std::collections::BTreeMap;
use std::time::Instant;

use hub_ingest::{Scanner, book_texts};
use hub_store::{ProofRow, ProofRun, StaleRepo};
use serde::Serialize;

pub const ROWS_PER_BOOK_CODE: usize = 200;

/// The run's corpus: which books formed it and which were left out. Stored
/// as the proof's summary body.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ProofCorpus {
    pub books: Vec<hub_proof::ProofBook>,
    pub skipped: Vec<hub_proof::Skipped>,
}

pub fn run(scanner: &Scanner, repo: &StaleRepo) -> Option<ProofRun<ProofCorpus>> {
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
    let mut run = ProofRun {
        repo_id: repo.id,
        head_sha: sha,
        millis,
        findings: 0,
        by_book: BTreeMap::new(),
        by_rule: BTreeMap::new(),
        body: ProofCorpus {
            books: Vec::new(),
            skipped: Vec::new(),
        },
        rows: Vec::new(),
        rows_per_book_code: ROWS_PER_BOOK_CODE,
        error: None,
    };
    match result {
        Ok(proof) => {
            run.findings = proof.findings.len();
            for f in &proof.findings {
                *run.by_book.entry(f.book.clone()).or_default() += 1;
                *run.by_rule.entry(f.code.to_owned()).or_default() += 1;
                run.rows.push(ProofRow {
                    book: f.book.clone(),
                    code: f.code.to_owned(),
                    severity: f.rule.to_owned(),
                    body: serde_json::to_string(f).unwrap_or_default(),
                });
            }
            run.body = ProofCorpus {
                books: proof.books,
                skipped: proof.skipped,
            };
        }
        Err(e) => run.error = Some(e),
    }
    Some(run)
}
