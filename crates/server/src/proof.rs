//! Sous Chef runs for the poll: one per repo whose HEAD moved since its last
//! run. Findings are corpus-relative, so a run covers the whole repo.
//!
//! Every finding is counted (per book and check, in `finding_count`), but
//! rows are kept only up to [`ROWS_PER_BOOK_CODE`] per book and check: some
//! texts produce findings by the hundred thousand (Khmer's and Lao's word
//! separators, Persian's ZWNJ), and the pages read counts first and open rows
//! one book and one check at a time.
//!
//! A repo whose scan resolved a source text (`wa-catalog/<lang>_<id>`, the
//! catalog convention in `hub_ingest::resolve_source`) is proofread against
//! it too: length proportionality, presence, and source copy when the two
//! languages differ. The source is read and prepared once per poll round
//! ([`SourceText`]) and shared by every repo that declares it.

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
    /// The source text the run compared against, `owner/name@sha`; absent
    /// when the repo resolves none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Whether the source-copy lane ran (only when the languages differ).
    pub source_copy: bool,
}

/// One source repo's books at one HEAD, prepared for Sous's source lanes.
pub struct SourceText {
    /// `owner/name`.
    pub repo: String,
    pub sha: String,
    pub source: hub_proof::Source,
}

/// Reads and prepares the source repo `owner/name` at its HEAD; `None` when
/// it is empty or unreadable (its targets then run without it).
pub fn prepare_source(scanner: &Scanner, repo: &str) -> Option<SourceText> {
    let (owner, name) = repo.split_once('/')?;
    let sha = hub_repo_read::head(&scanner.repo_path(owner, name))
        .ok()??
        .sha;
    let texts = book_texts(scanner, owner, name, &sha).ok()?;
    let books: Vec<hub_proof::Book> = texts
        .iter()
        .map(|(id, text)| hub_proof::Book { id, text })
        .collect();
    Some(SourceText {
        repo: repo.to_owned(),
        sha,
        source: hub_proof::Source::prepare(&books),
    })
}

/// Source copy flags words spelled exactly as in the source. Sous ships it
/// off because a source in the same language matches everywhere (en_udb
/// against en_ulb: 59,101 rows); against a source in another language it is
/// the "left untranslated" signal. So: on when the primary language subtags
/// differ (`wa-catalog/<lang>_<id>` names the source's language).
fn source_copy(language: Option<&str>, source_repo: &str) -> bool {
    let primary = |tag: &str| tag.split('-').next().unwrap_or("").to_ascii_lowercase();
    let source_lang = source_repo
        .split_once('/')
        .and_then(|(_, name)| name.split_once('_'))
        .map(|(lang, _)| primary(lang));
    match (language, source_lang) {
        (Some(language), Some(source)) => primary(language) != source,
        _ => false,
    }
}

pub fn run(
    scanner: &Scanner,
    repo: &StaleRepo,
    language: Option<&str>,
    source: Option<&SourceText>,
) -> Option<ProofRun<ProofCorpus>> {
    let sha = repo.head_sha.clone()?;
    let started = Instant::now();
    let copy = source.is_some_and(|s| source_copy(language, &s.repo));
    let result = book_texts(scanner, &repo.owner, &repo.name, &sha).and_then(|texts| {
        let books: Vec<hub_proof::Book> = texts
            .iter()
            .map(|(id, text)| hub_proof::Book { id, text })
            .collect();
        match source {
            Some(s) => hub_proof::proofread_paired(&books, &s.source, copy),
            None => hub_proof::proofread(&books),
        }
        .map_err(|e| e.to_string())
    });
    let millis = started.elapsed().as_millis() as i64;
    let mut run = ProofRun {
        repo_id: repo.id,
        head_sha: sha,
        source_repo: source.map(|s| s.repo.clone()),
        source_sha: source.map(|s| s.sha.clone()),
        millis,
        findings: 0,
        by_book: BTreeMap::new(),
        by_rule: BTreeMap::new(),
        body: ProofCorpus {
            books: Vec::new(),
            skipped: Vec::new(),
            source: source.map(|s| format!("{}@{}", s.repo, s.sha)),
            source_copy: copy,
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
            run.body.books = proof.books;
            run.body.skipped = proof.skipped;
        }
        Err(e) => run.error = Some(e),
    }
    Some(run)
}
