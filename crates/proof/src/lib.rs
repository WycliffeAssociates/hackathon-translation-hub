//! hub-proof: Sous Chef proofreading over one repo's books.
//!
//! Sous Chef (scripture-kitchen's `sous-core`) finds reviewable
//! inconsistencies in scripture text without a dictionary or a language
//! model: it learns what the translation normally does and reports the rare
//! exceptions with their counts ("this mark appears 2 of 1,053 times"). Its
//! findings are corpus-relative, so the unit of work is a set of books — for
//! the hub, every USFM book of ONE repo, or the one stitched book of a Writer
//! repo. Adding or editing one book can change another book's findings.
//!
//! ```text
//! proofread(&[Book { id: "41-MAT.usfm", text }, Book { id: "42-MRK.usfm", text }])
//!   → Proof { books: [MAT, MRK], skipped: [], findings: [
//!       Finding { book: "MRK", chapter: 3, verse: Some(5), from: 4511, to: 4512,
//!                 rule: "convention", code: "Rarity", message: "[;] appears only 2 times …",
//!                 evidence: Some(Evidence { numerator: 2, denominator: 1053, … }) }, … ] }
//! ```
//!
//! The pipeline is the `sous` CLI's (`sous-chef/cli/src/main.rs` and
//! `lengths.rs` @ v0.1.9) without a source text: each book is parsed by
//! galley's `OnionBook` (onion's verse-text mask + TOC, as sous-core's neutral
//! `ProjectedBook`), the books form one `Corpus`, and `analyze` runs the
//! product pass (`Brigade`: hygiene, punctuation substrate, words) under its
//! default config. Each finding is then located back in the raw USFM the way
//! galley's publication does it (`galley/src/sous/mod.rs`): the projected span
//! maps to its BOUNDING raw span, then to UTF-16.
//!
//! Not run here: the source-compared lanes (length proportionality, presence,
//! source-copy), which need a declared source text, and `--typos`, an
//! on-demand report rather than findings.
//!
//! Results are a pure function of (every book's bytes in the corpus, engine
//! version), so a cache key must cover the whole repo, not one book.

mod message;

use serde::Serialize;
use sous_core::{
    Brigade, Corpus, FindingKind, PackedFinding, Pattern, ProjectedBook, Reasons, TextRange,
    analyze,
};
use usfm_galley::onion::utf16_index;
use usfm_galley::sous::OnionBook;

/// The scripture-kitchen tag this crate's engine was built from. It must match
/// the `tag` of `usfm_galley` and `sous-core` in this crate's Cargo.toml.
pub const KITCHEN_TAG: &str = "v0.1.9";

/// One book handed in: the host's id for it (a path in practice) and its raw
/// USFM.
#[derive(Debug, Clone, Copy)]
pub struct Book<'a> {
    pub id: &'a str,
    pub text: &'a str,
}

/// What Sous found over one corpus.
#[derive(Debug, Clone, Default, PartialEq, Serialize, utoipa::ToSchema)]
pub struct Proof {
    /// The books that formed the corpus, in input order.
    pub books: Vec<ProofBook>,
    /// Books left out of the corpus, and why.
    pub skipped: Vec<Skipped>,
    /// Ordered by input book, then by position in the book.
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct ProofBook {
    pub id: String,
    /// The `\id` code as written, first three bytes: `MAT`.
    pub book: String,
    pub findings: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct Skipped {
    pub id: String,
    pub reason: String,
}

/// One reviewable site.
///
/// **Offsets are UTF-16 code units into the raw book text** (`from..to`,
/// half-open), which is what CodeMirror positions are. A site whose content
/// crosses removed markup (a footnote, a marker) gets the BOUNDING raw range:
/// first content character through last.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct Finding {
    /// The host id of the book ([`Book::id`]).
    pub id: String,
    /// The `\id` code: `MAT`.
    pub book: String,
    /// 0 for front matter, before the first `\c`.
    pub chapter: u16,
    /// `None` before the chapter's first `\v`.
    pub verse: Option<u16>,
    pub from: u32,
    pub to: u32,
    /// The lane: `hygiene` (deterministic: the text should never hold this)
    /// or `convention` (corpus-relative: this corpus rarely does this).
    pub rule: &'static str,
    /// Sous's own name for what fired: a hygiene class (`StrayCarriageReturn`)
    /// or the pattern's channel (`Rarity`, `Casing`, …).
    pub code: &'static str,
    /// Every way a convention site is unusual, Sous's `Reasons` names; the
    /// `code` is the finest of them. Empty for hygiene.
    pub reasons: Vec<&'static str>,
    /// One plain English sentence. The claim is narrow on purpose: rare, not
    /// wrong.
    pub message: String,
    /// The site's content, markup removed.
    pub text: String,
    /// Hygiene: how many offending characters are in the run.
    pub run: Option<u32>,
    /// Convention: the counts behind the claim.
    pub evidence: Option<Evidence>,
}

/// The fraction behind one convention finding. Every site of one pattern
/// carries the same evidence; `pattern` says which sites belong together.
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct Evidence {
    /// The pattern's row in this corpus's table; stable only within one
    /// [`Proof`].
    pub pattern: u16,
    /// How often this form occurs…
    pub numerator: u32,
    /// …out of how many chances (the comparison population).
    pub denominator: u32,
    /// Books holding part of the numerator (saturates at 255), out of
    /// `Proof::books.len()`.
    pub books: u8,
    /// What the corpus usually does instead, when the pattern names it.
    pub usual: Option<Usual>,
}

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct Usual {
    /// A mark in brackets (`[']`), a word form, or a class (`a letter`).
    pub text: String,
    pub count: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sous refused the corpus: {0}")]
    Corpus(String),
}

/// Proofreads `books` as one corpus.
///
/// A book that cannot join the corpus — no `\id`, a second book with the same
/// `\id`, chapters out of order — is listed in [`Proof::skipped`] rather than
/// failing the rest. A later duplicate is the one skipped.
pub fn proofread(books: &[Book]) -> Result<Proof, Error> {
    let mut proof = Proof::default();
    // Index-aligned: `kept[i]` is the input book `parsed[i]` was parsed from.
    let mut kept: Vec<&Book> = Vec::with_capacity(books.len());
    let mut parsed: Vec<OnionBook> = Vec::with_capacity(books.len());
    for book in books {
        let skip = |reason: String| Skipped {
            id: book.id.to_owned(),
            reason,
        };
        let onion = match OnionBook::parse(book.text) {
            Ok(onion) => onion,
            Err(error) => {
                proof.skipped.push(skip(error.to_string()));
                continue;
            }
        };
        let key = onion.key();
        if key.as_bytes() == [0; 3] {
            proof.skipped.push(skip("no \\id line".to_owned()));
            continue;
        }
        if let Some(first) = parsed.iter().position(|other| other.key() == key) {
            proof
                .skipped
                .push(skip(format!("\\id {key} is also {}", kept[first].id)));
            continue;
        }
        kept.push(book);
        parsed.push(onion);
    }

    let corpus = Corpus::try_new(&parsed).map_err(|error| Error::Corpus(error.to_string()))?;
    let (rows, patterns, _, _) = analyze(&corpus, &Brigade::default()).into_parts();

    proof.books = kept
        .iter()
        .zip(&parsed)
        .map(|(book, onion)| ProofBook {
            id: book.id.to_owned(),
            book: onion.key().to_string(),
            findings: 0,
        })
        .collect();
    // Rows arrive ordered by book, so each book's UTF-16 index is built once.
    for rows in rows.chunk_by(|a, b| a.book_idx() == b.book_idx()) {
        let at = usize::from(rows[0].book_idx().get());
        let (book, onion) = (kept[at], &parsed[at]);
        let utf16 = utf16_index(book.text.as_bytes());
        let before = proof.findings.len();
        proof.findings.extend(
            rows.iter()
                .filter_map(|row| locate(book, onion, &utf16, &patterns, *row)),
        );
        proof.books[at].findings = u32::try_from(proof.findings.len() - before).unwrap_or(u32::MAX);
    }
    Ok(proof)
}

/// One packed row as a [`Finding`]: address, raw UTF-16 span, message.
fn locate(
    book: &Book,
    parsed: &OnionBook,
    utf16: &usfm_galley::onion::Utf16Index<'_>,
    patterns: &[Pattern],
    row: PackedFinding,
) -> Option<Finding> {
    let span = TextRange::new(row.from(), row.to()).ok()?;
    let located = parsed.locate(span)?;
    let (first, mut runs) = (located.first, located.spans);
    let start = runs.next()?;
    let end = runs.last().map_or(start.end, |run| run.end);
    let text = parsed.text()[row.from() as usize..row.to() as usize].to_owned();

    let (rule, code, reasons, run, evidence, message) = match row.kind() {
        FindingKind::Hygiene(digest) => (
            "hygiene",
            digest.class().name(),
            Vec::new(),
            Some(digest.run()),
            None,
            message::hygiene(digest),
        ),
        FindingKind::Convention(digest) => {
            let index = digest.pattern().get();
            let pattern = patterns.get(usize::from(index))?;
            (
                "convention",
                pattern.channel.name(),
                reason_names(digest.reasons()),
                None,
                Some(Evidence {
                    pattern: index,
                    numerator: pattern.numerator,
                    denominator: pattern.denominator,
                    books: pattern.books,
                    usual: message::usual(pattern),
                }),
                message::convention(pattern, &text, parsed.key()),
            )
        }
        // Source-compared kinds; `analyze` without a source emits none.
        _ => return None,
    };
    Some(Finding {
        id: book.id.to_owned(),
        book: parsed.key().to_string(),
        chapter: first.chapter,
        verse: (first.first != 0).then_some(first.first),
        from: utf16.to_utf16(start.start),
        to: utf16.to_utf16(end),
        rule,
        code,
        reasons,
        message,
        text,
        run,
        evidence,
    })
}

fn reason_names(reasons: Reasons) -> Vec<&'static str> {
    Reasons::NAMES
        .iter()
        .enumerate()
        .filter(|(bit, _)| reasons.bits() & (1 << bit) != 0)
        .map(|(_, name)| *name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pins the offset convention the web UI depends on: `from..to` are UTF-16
    // units into the RAW book, so CodeMirror can mark them directly. The
    // astral 🧅 (4 UTF-8 bytes, 2 UTF-16 units) and the footnote before the
    // site make bytes, UTF-16 and projected offsets all disagree, so any of
    // the three wrong conventions fails here.
    #[test]
    fn offsets_are_raw_utf16() {
        let text = "\\id MRK\n\\c 1\n\\p\n\\v 1 An 🧅 \\f + \\ft note\\f* rang\u{7} here.\n";
        let proof = proofread(&[Book {
            id: "41-MRK.usfm",
            text,
        }])
        .unwrap();
        let bell = proof
            .findings
            .iter()
            .find(|finding| finding.code == "C0Control")
            .expect("a control character in verse text is always reported");

        let utf16: Vec<u16> = text.encode_utf16().collect();
        let site = String::from_utf16(&utf16[bell.from as usize..bell.to as usize]).unwrap();
        assert_eq!(site, "\u{7}");
        assert_ne!(bell.from as usize, text.find('\u{7}').unwrap(), "not bytes");
        assert_eq!(
            (bell.book.as_str(), bell.chapter, bell.verse),
            ("MRK", 1, Some(1))
        );
    }

    // A repo with two files claiming one `\id` (a backup copy, a rename) or a
    // file with no `\id` must not cost the rest of the repo its proofreading:
    // sous-core refuses such a corpus outright, so the crate skips the book.
    #[test]
    fn a_book_that_cannot_join_is_skipped_not_fatal() {
        let mark = "\\id MRK\n\\c 1\n\\p\n\\v 1 One.\n";
        let books = [
            Book {
                id: "41-MRK.usfm",
                text: mark,
            },
            Book {
                id: "41-MRK-copy.usfm",
                text: mark,
            },
            Book {
                id: "notes.usfm",
                text: "\\c 1\n\\p\n\\v 1 One.\n",
            },
        ];
        let proof = proofread(&books).unwrap();
        assert_eq!(proof.books.len(), 1);
        let skipped: Vec<&str> = proof.skipped.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(skipped, ["41-MRK-copy.usfm", "notes.usfm"]);
    }
}
