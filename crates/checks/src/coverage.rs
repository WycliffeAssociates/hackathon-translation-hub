//! Verse coverage: which of a source text's verses a target text has.
//!
//! Verses come from the engine's toc, counting every verse a designator
//! COVERS (its members, as the reader's `Toc.verses()` exposes them): `\v 5-7`
//! is 5, 6 and 7, and the list `\v 5,7` is 5 and 7 but not 6. Segments
//! (`12a`, `12b`) count as their verse.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

/// The widest single bridge expanded. Anything wider is a typo (`\v 1-1000`),
/// and only its endpoints are kept rather than inventing a thousand verses.
const MAX_BRIDGE: u16 = 200;

/// Every verse number present, per chapter. Verses before the first `\c` sit
/// under chapter 0; malformed designators (number 0) are skipped.
pub fn verses_by_chapter(text: &str) -> BTreeMap<u32, BTreeSet<u32>> {
    // The \s5 registry only changes how `\s5` itself lexes, never `\c`/`\v`,
    // so plain `lex` (no extensions) is enough here — via `lex_with` with an
    // empty registry, so a process-wide `set_extensions` elsewhere can't matter.
    let tokens = usfm_onion::lex_with(text, &usfm_onion::Extensions::default());
    let toc = usfm_onion::toc::toc(text.as_bytes(), &tokens);
    let mut out: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for anchor in &toc.verses {
        let verses = out.entry(u32::from(anchor.chapter)).or_default();
        for member in toc.members_of(anchor) {
            let (lo, hi) = (member.from.min(member.to), member.from.max(member.to));
            if hi - lo <= MAX_BRIDGE {
                verses.extend((lo..=hi).map(u32::from));
            } else {
                verses.extend([u32::from(lo), u32::from(hi)]);
            }
        }
        verses.remove(&0);
    }
    out.retain(|_, verses| !verses.is_empty());
    out
}

/// One source chapter, measured against the target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChapterCoverage {
    pub chapter: u32,
    /// Verses the source has in this chapter.
    pub source_verses: u32,
    /// Of those, how many the target has.
    pub present: u32,
    /// The source's verses the target lacks, ascending.
    pub missing: Vec<u32>,
    /// Verses the target has here that the source does not — usually a
    /// versification difference, not an error.
    pub extra: u32,
}

/// The sums over every chapter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CoverageTotal {
    pub source_verses: u32,
    pub present: u32,
    pub missing: u32,
    pub extra: u32,
}

/// Per-chapter coverage plus the total.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Coverage {
    pub chapters: Vec<ChapterCoverage>,
    pub total: CoverageTotal,
}

/// Measures `target` against `source`, over the SOURCE's chapters. Chapters
/// only the target has are not reported (they have no source to cover).
pub fn coverage(
    target: &BTreeMap<u32, BTreeSet<u32>>,
    source: &BTreeMap<u32, BTreeSet<u32>>,
) -> Coverage {
    let empty = BTreeSet::new();
    let mut out = Coverage::default();
    for (&chapter, want) in source {
        let have = target.get(&chapter).unwrap_or(&empty);
        let missing: Vec<u32> = want.difference(have).copied().collect();
        let row = ChapterCoverage {
            chapter,
            source_verses: want.len() as u32,
            present: want.intersection(have).count() as u32,
            extra: have.difference(want).count() as u32,
            missing,
        };
        out.total.source_verses += row.source_verses;
        out.total.present += row.present;
        out.total.missing += row.missing.len() as u32;
        out.total.extra += row.extra;
        out.chapters.push(row);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_and_lists_count_what_they_cover() {
        let text = "\\id RUT\n\\c 1\n\\p\n\\v 1 a\n\\v 2-4 b\n\\v 6,8 c\n\\v 9a d\n\\v 9b e\n\\c 2\n\\p\n\\v 1 f\n";
        let verses = verses_by_chapter(text);
        assert_eq!(
            verses[&1].iter().copied().collect::<Vec<_>>(),
            [1, 2, 3, 4, 6, 8, 9]
        );
        assert_eq!(verses[&2].iter().copied().collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn coverage_reports_missing_and_extra() {
        let source = verses_by_chapter(
            "\\id RUT\n\\c 1\n\\p\n\\v 1 a\n\\v 2 b\n\\v 3 c\n\\v 4 d\n\\c 2\n\\p\n\\v 1 e\n\\v 2 f\n",
        );
        // Target bridges 1-3, skips 4, adds a verse 5, and has no chapter 2.
        let target = verses_by_chapter("\\id RUT\n\\c 1\n\\p\n\\v 1-3 abc\n\\v 5 x\n");
        let cov = coverage(&target, &source);
        assert_eq!(
            cov.chapters[0],
            ChapterCoverage {
                chapter: 1,
                source_verses: 4,
                present: 3,
                missing: vec![4],
                extra: 1
            }
        );
        assert_eq!(cov.chapters[1].missing, [1, 2]);
        assert_eq!(
            cov.total,
            CoverageTotal {
                source_verses: 6,
                present: 3,
                missing: 3,
                extra: 1
            }
        );
    }
}
