//! A book beside its source, verse by verse: the text and length of each
//! verse on both sides, in Sous Chef's own terms. Verses come from galley's
//! `OnionBook` (onion's verse-text projection and verse anchors) and lengths
//! from sous's `count_atoms` (graphemes), the same reading the length lane
//! (`LengthProportionality`) uses, so a chart or a card shows the numbers the
//! findings were computed from.
//!
//! Pairing is the lane's: by verse key (chapter, verse, bridge). A target
//! verse takes every source verse its key covers, so a bridge `1-2` meets the
//! source's 1 and 2 summed. Target verses with nothing on the source side
//! (and the reverse) are left out: Presence reports those.

use serde::Serialize;
use sous_core::unicode::atoms::count_atoms;
use sous_core::{ProjectedBook, VerseKey};
use usfm_galley::sous::OnionBook;

/// One target verse beside its source.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct VersePair {
    pub chapter: u16,
    pub verse: u16,
    /// The last verse of a bridge (`1-2` → 2); equal to `verse` otherwise.
    pub last: u16,
    /// Graphemes, as sous counts them.
    pub target_len: u32,
    pub source_len: u32,
    /// The verse text (onion's projection: markers and notes removed, spaces
    /// collapsed); empty when the caller asked for lengths only.
    pub target: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct Comparison {
    pub verses: Vec<VersePair>,
    /// The middle target/source length ratio over every pair: this book's
    /// usual ratio, what "much longer" is measured against.
    pub usual_ratio: Option<f64>,
}

fn raw(book: &OnionBook, from: u32, to: u32) -> &str {
    book.text().get(from as usize..to as usize).unwrap_or("")
}

/// For reading: spaces and line breaks collapsed. Lengths are counted on the
/// raw projection, as the length lane counts them.
fn shown(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Pairs `target`'s verses with `source`'s. `None` when either will not parse.
/// `keep`: which target verses to return with text (all lengths are always
/// used for the usual ratio); `texts`: include the texts at all.
pub fn compare(
    target: &str,
    source: &str,
    texts: bool,
    keep: impl Fn(u16, u16) -> bool,
) -> Option<Comparison> {
    let t = OnionBook::parse(target).ok()?;
    let s = OnionBook::parse(source).ok()?;
    let source_verses: Vec<(VerseKey, u32, u32)> = s
        .verses()
        .map(|v| (v.key(), v.text().from(), v.text().to()))
        .collect();
    let mut ratios = Vec::new();
    let mut verses = Vec::new();
    for v in t.verses() {
        let key = v.key();
        let covered: Vec<&(VerseKey, u32, u32)> = source_verses
            .iter()
            .filter(|(k, _, _)| {
                k.chapter() == key.chapter() && k.first() >= key.first() && k.last() <= key.last()
            })
            .collect();
        if covered.is_empty() {
            continue;
        }
        let target_raw = raw(&t, v.text().from(), v.text().to());
        let target_len = count_atoms(target_raw);
        // A bridge on either side sums its constituents (the lane's rule).
        let source_len: u32 = covered
            .iter()
            .map(|(_, from, to)| count_atoms(raw(&s, *from, *to)))
            .sum();
        if target_len > 0 && source_len > 0 {
            ratios.push(f64::from(target_len) / f64::from(source_len));
        }
        if keep(key.chapter(), key.first()) {
            verses.push(VersePair {
                chapter: key.chapter(),
                verse: key.first(),
                last: key.last(),
                target_len,
                source_len,
                target: if texts {
                    shown(target_raw)
                } else {
                    String::new()
                },
                source: if texts {
                    covered
                        .iter()
                        .map(|(_, from, to)| shown(raw(&s, *from, *to)))
                        .collect::<Vec<_>>()
                        .join(" ")
                } else {
                    String::new()
                },
            });
        }
    }
    ratios.sort_by(f64::total_cmp);
    Some(Comparison {
        usual_ratio: ratios.get(ratios.len() / 2).copied(),
        verses,
    })
}
