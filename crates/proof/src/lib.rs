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
//! With a declared source text ([`proofread_paired`]), the source-compared
//! lanes run too: length proportionality, presence, and (when asked) source
//! copy. Not run here: `--typos`, an on-demand report rather than findings.
//!
//! Results are a pure function of (every book's bytes in the corpus, engine
//! version), so a cache key must cover the whole repo, not one book.

pub mod compare;
mod message;

use serde::Serialize;
use sous_core::unicode::atoms::count_atoms;
use sous_core::{
    BookKey, Brigade, ChapterPass, Corpus, FindingKind, PackedFinding, Pattern, PresenceRow,
    ProjectedBook, Reasons, SourceLengths, SourceVerse, SourceWords, TextRange, VerseKey,
    analyze_paired, source_lengths,
};
use usfm_galley::onion::{Filter, Mask, Sid, Utf16Index, cst, lex, mask, toc, utf16_index};
use usfm_galley::sous::OnionBook;

/// The scripture-kitchen tag this crate's engine was built from. It must match
/// the `tag` of `usfm_galley` and `sous-core` in this crate's Cargo.toml.
pub const KITCHEN_TAG: &str = "v0.1.10";

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
    /// The source book this one was compared with ([`proofread_paired`]),
    /// by its host id; `None` when there was no source or it lacks this book.
    pub source: Option<String>,
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
    /// Source-compared lanes: what the comparison with the source found.
    pub paired: Option<PairedEvidence>,
}

/// The facts behind one source-compared finding (`rule: "source"`). Source
/// copy's run of shared words is [`Finding::run`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, utoipa::ToSchema)]
pub struct PairedEvidence {
    /// `LengthProportionality`: `Long` or `Short`. `Presence`: `Missing` (the
    /// source has verses the book lacks; the span is where they would go),
    /// `Extra` (the book has verses the source lacks) or `Empty` (the verse
    /// is there but holds nothing). `SourceCopy`: `Copy`.
    pub kind: &'static str,
    /// Presence: consecutive verse keys the row covers, a bridge counting once.
    /// `chapter`/`verse` are the first of them.
    pub keys: Option<u32>,
    /// Length: how many spreads this verse's target/source ratio sits from the
    /// usual ratio in this book, and in the whole repo. `None` when that scope
    /// had too few paired verses to judge (sous's `min_verses`, 50).
    pub z_book: Option<f64>,
    pub z_project: Option<f64>,
    /// Length: graphemes in the target verse and in its source verse.
    pub target_len: Option<u32>,
    pub source_len: Option<u32>,
    /// Source copy: target words in the verse that could be compared.
    pub eligible: Option<u32>,
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

/// A declared source text (e.g. `wa-catalog/en_ulb`), prepared once: per book,
/// each verse's length in graphemes and its word set. That is everything the
/// source-compared lanes read from a source, so one `Source` serves every
/// target that declares it, across threads.
#[derive(Debug, Default)]
pub struct Source {
    books: Vec<SourceBook>,
}

#[derive(Debug)]
struct SourceBook {
    id: String,
    key: BookKey,
    verses: Vec<SourceVerse>,
    words: SourceWords,
}

impl Source {
    /// Parses the source's books the way targets are parsed. A book that
    /// cannot be parsed or has no `\id` is left out; of two books with one
    /// `\id`, the first is kept (sous's own rule, `judge_lengths` @ v0.1.9
    /// `core/src/proportionality.rs:480`).
    pub fn prepare(books: &[Book]) -> Self {
        use rayon::prelude::*;
        // Parsed and measured per book in parallel; kept in input order after.
        let prepared: Vec<Option<SourceBook>> = books
            .par_iter()
            .map(|book| {
                let onion = OnionBook::parse(book.text).ok()?;
                let key = onion.key();
                (key.as_bytes() != [0; 3]).then(|| SourceBook {
                    id: book.id.to_owned(),
                    key,
                    verses: source_lengths(&onion),
                    words: SourceWords::of(&onion),
                })
            })
            .collect();
        let mut out = Self::default();
        for book in prepared.into_iter().flatten() {
            if !out.books.iter().any(|b| b.key == book.key) {
                out.books.push(book);
            }
        }
        out
    }

    /// Books the source holds.
    pub fn len(&self) -> usize {
        self.books.len()
    }

    pub fn is_empty(&self) -> bool {
        self.books.is_empty()
    }

    fn book(&self, key: BookKey) -> Option<&SourceBook> {
        self.books.iter().find(|b| b.key == key)
    }
}

/// Proofreads `books` as one corpus.
///
/// A book that cannot join the corpus — no `\id`, a second book with the same
/// `\id`, chapters out of order — is listed in [`Proof::skipped`] rather than
/// failing the rest. A later duplicate is the one skipped.
pub fn proofread(books: &[Book]) -> Result<Proof, Error> {
    run(books, None)
}

/// [`proofread`], plus the source-compared lanes against `source`: length
/// proportionality and presence always, source copy when `source_copy` is set
/// (sous ships it off: against a source in the same language family it fires
/// about twice a verse; a host that knows its source is unrelated turns it
/// on). This is the sous CLI's `brigade_findings` (`cli/src/lengths.rs:14`
/// @ v0.1.9) with the source entering as lengths and word sets.
///
/// A target book pairs with the source book of the same `\id`; its verses pair
/// by onion's verse anchors (chapter, verse, bridge) plus occurrence, never by
/// position (`charter.md`, the aligned-unit contract). A book with no source
/// book still gets every unpaired check.
pub fn proofread_paired(
    books: &[Book],
    source: &Source,
    source_copy: bool,
) -> Result<Proof, Error> {
    run(books, Some((source, source_copy)))
}

/// `OnionBook::parse`, keeping a copy of the projection's offset map.
///
/// galley's `OnionBook::locate` walks the mask from its first range for every
/// call and does not stop past the range, so mapping a finding back to raw
/// bytes cost a pass over the whole book: 163k findings in hbo_uhb made that
/// 90% of a 4 s run. The same mask, kept here, answers by binary search
/// (`Mask::to_source`). The book is built from the same parts galley's
/// `parse` builds (`from_parts`), so nothing sous sees changes.
fn parse_book(text: &str) -> Result<(OnionBook, Mask), sous_core::InputError> {
    let tokens = lex(text);
    let tree = cst::build(&tokens);
    let toc = toc(text.as_bytes(), &tokens);
    let kept = mask(text.as_bytes(), &tokens, &tree, &Filter::verse_text());
    OnionBook::from_parts(text, kept.clone(), toc).map(|book| (book, kept))
}

fn run(books: &[Book], source: Option<(&Source, bool)>) -> Result<Proof, Error> {
    use rayon::prelude::*;
    let mut proof = Proof::default();
    // Each book parses on its own (in parallel); which books join the corpus
    // is decided after, in input order, so the first of two `\id`s still wins.
    let onions: Vec<_> = books.par_iter().map(|b| parse_book(b.text)).collect();
    // Index-aligned: `kept[i]` is the input book `parsed[i]` was parsed from,
    // and `masks[i]` its offset map.
    let mut kept: Vec<&Book> = Vec::with_capacity(books.len());
    let mut parsed: Vec<OnionBook> = Vec::with_capacity(books.len());
    let mut masks: Vec<Mask> = Vec::with_capacity(books.len());
    for (book, onion) in books.iter().zip(onions) {
        let skip = |reason: String| Skipped {
            id: book.id.to_owned(),
            reason,
        };
        let (onion, book_mask) = match onion {
            Ok(parts) => parts,
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
        masks.push(book_mask);
    }

    let corpus = Corpus::try_new(&parsed).map_err(|error| Error::Corpus(error.to_string()))?;
    // The CLI sets the switch in both word-judging slots (`cli/src/lengths.rs:26`).
    let mut config = <Brigade as ChapterPass>::Config::default();
    let source_copy = source.is_some_and(|(_, on)| on);
    config.1.lengths.source_copy = source_copy;
    config.2.lengths.source_copy = source_copy;
    let lengths: Vec<SourceLengths<'_>> = source
        .iter()
        .flat_map(|(source, _)| &source.books)
        .map(|b| SourceLengths {
            book: b.key,
            verses: &b.verses,
            words: Some(&b.words),
        })
        .collect();
    let (findings, paired) = analyze_paired(&corpus, &Brigade::default(), &config, &lengths);
    let (rows, patterns, _, _) = findings.into_parts();

    proof.books = kept
        .iter()
        .zip(&parsed)
        .map(|(book, onion)| ProofBook {
            id: book.id.to_owned(),
            book: onion.key().to_string(),
            findings: 0,
            source: source
                .and_then(|(source, _)| source.book(onion.key()))
                .map(|b| b.id.clone()),
        })
        .collect();
    // Rows arrive ordered by book, so each book's UTF-16 index is built once.
    for rows in rows.chunk_by(|a, b| a.book_idx() == b.book_idx()) {
        let at = usize::from(rows[0].book_idx().get());
        let target = Target {
            book: kept[at],
            onion: &parsed[at],
            mask: &masks[at],
            utf16: utf16_index(kept[at].text.as_bytes()),
            patterns: &patterns,
            presence: paired.presence.get(at).map_or(&[], Vec::as_slice),
            source: source.and_then(|(source, _)| source.book(parsed[at].key())),
        };
        let before = proof.findings.len();
        proof
            .findings
            .extend(rows.iter().filter_map(|row| target.finding(*row)));
        proof.books[at].findings = u32::try_from(proof.findings.len() - before).unwrap_or(u32::MAX);
    }
    Ok(proof)
}

/// One target book and what its rows are read against.
struct Target<'a> {
    book: &'a Book<'a>,
    onion: &'a OnionBook,
    /// The projection's offset map, for raw spans by binary search (`parse_book`).
    mask: &'a Mask,
    utf16: Utf16Index<'a>,
    patterns: &'a [Pattern],
    /// Sous's own presence rows for this book: they carry the verse key the
    /// packed row does not.
    presence: &'a [PresenceRow],
    source: Option<&'a SourceBook>,
}

impl Target<'_> {
    /// One packed row as a [`Finding`]: address, raw UTF-16 span, message.
    fn finding(&self, row: PackedFinding) -> Option<Finding> {
        let span = TextRange::new(row.from(), row.to()).ok()?;
        let (first, from, to) = self.raw(span)?;
        let text = self.onion.text()[row.from() as usize..row.to() as usize].to_owned();
        let mut finding = Finding {
            id: self.book.id.to_owned(),
            book: self.onion.key().to_string(),
            chapter: first.chapter,
            verse: (first.first != 0).then_some(first.first),
            from: self.utf16.to_utf16(from),
            to: self.utf16.to_utf16(to),
            rule: "",
            code: "",
            reasons: Vec::new(),
            message: String::new(),
            text,
            run: None,
            evidence: None,
            paired: None,
        };
        match row.kind() {
            FindingKind::Hygiene(digest) => {
                finding.rule = "hygiene";
                finding.code = digest.class().name();
                finding.run = Some(digest.run());
                finding.message = message::hygiene(digest);
            }
            FindingKind::Convention(digest) => {
                let index = digest.pattern().get();
                let pattern = self.patterns.get(usize::from(index))?;
                finding.rule = "convention";
                finding.code = pattern.channel.name();
                finding.reasons = reason_names(digest.reasons());
                finding.evidence = Some(Evidence {
                    pattern: index,
                    numerator: pattern.numerator,
                    denominator: pattern.denominator,
                    books: pattern.books,
                    usual: message::usual(pattern),
                });
                finding.message = message::convention(pattern, &finding.text, self.onion.key());
            }
            FindingKind::LengthProportionality(digest) => {
                let z_book = digest.book_scope().map(|z| z.as_f64());
                let z_project = digest.project_scope().map(|z| z.as_f64());
                let long = z_book.or(z_project).is_some_and(|z| z > 0.0);
                let (target_len, source_len) = self.lengths(span);
                finding.rule = "source";
                finding.code = "LengthProportionality";
                finding.message = message::length(long, z_book, z_project);
                finding.paired = Some(PairedEvidence {
                    kind: if long { "Long" } else { "Short" },
                    z_book,
                    z_project,
                    target_len: Some(target_len),
                    source_len,
                    ..PairedEvidence::default()
                });
            }
            FindingKind::Presence(digest) => {
                let kind = digest.kind();
                // The row this record was pushed from names its first key.
                if let Some(row) = self
                    .presence
                    .iter()
                    .find(|r| r.span() == span && r.kind() == kind && r.keys() == digest.keys())
                {
                    finding.chapter = row.key().chapter();
                    finding.verse = Some(row.key().first());
                }
                finding.rule = "source";
                finding.code = "Presence";
                finding.message = message::presence(kind, digest.keys());
                finding.paired = Some(PairedEvidence {
                    kind: kind.name(),
                    keys: Some(digest.keys()),
                    ..PairedEvidence::default()
                });
            }
            FindingKind::SourceCopy(digest) => {
                finding.rule = "source";
                finding.code = "SourceCopy";
                finding.run = Some(digest.run());
                finding.message = message::source_copy(digest.run(), digest.eligible());
                finding.paired = Some(PairedEvidence {
                    kind: "Copy",
                    eligible: Some(digest.eligible()),
                    ..PairedEvidence::default()
                });
            }
        }
        Some(finding)
    }

    /// The address and bounding raw byte span of a projected span. An empty
    /// span (a presence row for verses the target lacks sits at the point they
    /// would go) is placed at the raw position of the character there.
    fn raw(&self, span: TextRange) -> Option<(Sid, u32, u32)> {
        if span.is_empty() {
            let text = self.onion.text();
            let at = span.from() as usize;
            if let Some(c) = text[at..].chars().next() {
                let next = TextRange::new(span.from(), (at + c.len_utf8()) as u32).ok()?;
                let (sid, from, _) = self.raw(next)?;
                return Some((sid, from, from));
            }
            let c = text[..at].chars().next_back()?;
            let previous = TextRange::new((at - c.len_utf8()) as u32, span.from()).ok()?;
            let (sid, _, to) = self.raw(previous)?;
            return Some((sid, to, to));
        }
        // The address from the TOC (cheap); the bounding raw span from the
        // mask: the first projected byte's raw byte, and one past the last's.
        // The same span galley's run walk gives (first run's start, last
        // run's end), without walking.
        let located = self.onion.locate(span)?;
        let start = self.mask.to_source(span.from());
        let end = self.mask.to_source(span.to() - 1) + 1;
        Some((located.first, start, end))
    }

    /// Graphemes in a length row's target unit, and in the source verses its
    /// keys cover. The row's span bounds the unit, so the unit is the target
    /// verses inside it; a bridge on either side sums its constituents.
    fn lengths(&self, span: TextRange) -> (u32, Option<u32>) {
        let text = &self.onion.text()[span.from() as usize..span.to() as usize];
        let target = count_atoms(text);
        let keys: Vec<VerseKey> = self
            .onion
            .verses()
            .filter(|v| span.from() <= v.text().from() && v.text().to() <= span.to())
            .map(|v| v.key())
            .collect();
        let source = self.source.zip(keys.first()).map(|(source, first)| {
            let last = keys.iter().map(|k| k.last()).max().unwrap_or(first.last());
            source
                .verses
                .iter()
                .filter(|v| {
                    let key = v.key();
                    key.chapter() == first.chapter()
                        && key.first() >= first.first()
                        && key.last() <= last
                })
                .map(|v| v.graphemes())
                .sum()
        });
        (target, source)
    }
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

    // Pins pairing by verse key, the source lanes' one structural promise
    // (charter.md, the aligned-unit contract): a target that skips verse 3
    // reports 3 missing at the point it would go, with 4 still paired to 4.
    // Pairing by position would report nothing missing and compare 4 with 3.
    #[test]
    fn source_lanes_pair_by_verse_key() {
        let source = "\\id MRK\n\\c 1\n\\p\n\\v 1 One.\n\\v 2 Two.\n\\v 3 Three.\n\\v 4 Four.\n";
        let target = "\\id MRK\n\\c 1\n\\p\n\\v 1 Uno.\n\\v 2 Dos.\n\\v 4 Cuatro.\n";
        let source = Source::prepare(&[Book {
            id: "src/41-MRK.usfm",
            text: source,
        }]);
        let books = [Book {
            id: "41-MRK.usfm",
            text: target,
        }];
        let proof = proofread_paired(&books, &source, false).unwrap();
        assert_eq!(proof.books[0].source.as_deref(), Some("src/41-MRK.usfm"));
        let missing: Vec<_> = proof
            .findings
            .iter()
            .filter(|f| f.paired.as_ref().is_some_and(|p| p.kind == "Missing"))
            .collect();
        assert_eq!(missing.len(), 1, "{:#?}", proof.findings);
        let at = target.find("Cuatro").unwrap() as u32; // ASCII: bytes = UTF-16
        assert_eq!(
            (
                missing[0].chapter,
                missing[0].verse,
                missing[0].from,
                missing[0].to
            ),
            (1, Some(3), at, at)
        );
    }
}
