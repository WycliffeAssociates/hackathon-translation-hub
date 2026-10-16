//! A book's verses, content-addressed: which copies of a book are the same
//! text, and how two copies differ, verse by verse.
//!
//! tStudio and Writer leave the same book in many repos (three Tituses, a
//! merged repo that repeats them all). Byte equality is too strict for that
//! question (a re-stitched file, a changed heading or a trailing space is not
//! a different translation), so each verse is reduced to its words: markers
//! dropped, whitespace collapsed. Two copies are the same version when every
//! verse matches; otherwise the diff counts verses changed, only in one, only
//! in the other.
//!
//! Headings, titles and anything before a chapter's first verse are left out
//! on purpose: they are not what a publisher picks a copy for. A verse range
//! (`\v 1-2`) is keyed by its first verse.

use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};

/// `(chapter, verse)` → a hash of the verse's words.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fingerprint {
    pub verses: BTreeMap<(u16, u16), u64>,
}

/// How copy `a` differs from copy `b`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct VerseDiff {
    /// Verses in both with different words.
    pub changed: usize,
    /// Verses `a` has that `b` lacks.
    pub added: usize,
    /// Verses `b` has that `a` lacks.
    pub missing: usize,
}

impl VerseDiff {
    pub fn is_same(&self) -> bool {
        self.changed == 0 && self.added == 0 && self.missing == 0
    }
}

impl Fingerprint {
    /// One hash for the whole book: equal exactly when every verse is.
    pub fn id(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.verses.hash(&mut h);
        h.finish()
    }

    pub fn diff(&self, other: &Fingerprint) -> VerseDiff {
        let mut d = VerseDiff::default();
        for (key, hash) in &self.verses {
            match other.verses.get(key) {
                Some(h) if h == hash => {}
                Some(_) => d.changed += 1,
                None => d.added += 1,
            }
        }
        d.missing = other
            .verses
            .keys()
            .filter(|k| !self.verses.contains_key(k))
            .count();
        d
    }
}

/// The leading number of `\c 3` or `\v 1-2`, and the rest after it.
fn number(rest: &str) -> Option<(u16, &str)> {
    let t = rest.trim_start();
    let end = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    let n = t[..end].parse().ok()?;
    // Skip a range's tail (`-2`, `a`) up to the verse text, but never past
    // the next marker: WACS has `\v 5\v 6 …` (an empty verse 5).
    let tail = t[end..]
        .find(|c: char| c.is_whitespace() || c == '\\')
        .unwrap_or(t.len() - end);
    Some((n, &t[end + tail..]))
}

/// Text with markers dropped (`\w`, `\w*`, `\add`, `\f …\f*` markers but not
/// their words) and attributes (`|lemma="…"`) cut, whitespace collapsed.
fn words(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_attrs = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                in_attrs = false;
                while chars
                    .peek()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '*' || *c == '+')
                {
                    chars.next();
                }
                out.push(' ');
            }
            '|' => in_attrs = true,
            _ if in_attrs => {}
            _ => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_heading(marker: &str) -> bool {
    let base = marker.trim_end_matches(|c: char| c.is_ascii_digit());
    matches!(base, "s" | "ms" | "mr" | "sr" | "d" | "r")
}

/// A USFM book's fingerprint.
pub fn fingerprint(usfm: &str) -> Fingerprint {
    let verses = verse_words(usfm)
        .into_iter()
        .map(|(key, words)| {
            let mut h = DefaultHasher::new();
            words.hash(&mut h);
            (key, h.finish())
        })
        .collect();
    Fingerprint { verses }
}

/// Each verse's words, as the fingerprint compares them: what a diff shows.
pub fn verse_words(usfm: &str) -> BTreeMap<(u16, u16), String> {
    let mut verses = BTreeMap::new();
    let mut chapter = 0u16;
    let mut current: Option<((u16, u16), usize)> = None;
    let mut flush = |current: &mut Option<((u16, u16), usize)>, end: usize| {
        if let Some((key, start)) = current.take() {
            // A verse that ends where it starts (or before) is empty, not a panic.
            verses.insert(key, words(usfm.get(start..end).unwrap_or("")));
        }
    };
    let mut at = 0;
    while let Some(i) = usfm[at..].find('\\') {
        let start = at + i;
        let marker_end = usfm[start + 1..]
            .find(|c: char| !c.is_ascii_alphanumeric())
            .map_or(usfm.len(), |j| start + 1 + j);
        let marker = &usfm[start + 1..marker_end];
        match marker {
            "c" => {
                flush(&mut current, start);
                if let Some((n, _)) = number(&usfm[marker_end..]) {
                    chapter = n;
                }
            }
            "v" => {
                flush(&mut current, start);
                if let Some((n, text)) = number(&usfm[marker_end..]) {
                    current = Some(((chapter, n), usfm.len() - text.len()));
                }
            }
            // A heading or a new section ends the verse before it; the
            // heading itself belongs to no verse.
            // (`\s`, `\s1`…`\s5`, `\ms`, `\mr`, `\d`, `\r`, `\sr`; not `\sc` or `\sig`).
            m if is_heading(m) => {
                flush(&mut current, start);
            }
            _ => {}
        }
        at = marker_end.max(start + 1);
    }
    flush(&mut current, usfm.len());
    verses
}

#[cfg(test)]
mod tests {
    use super::*;

    // Why: a verse marker glued to the next one (`\v 5\v 6`, found on WACS in
    // gux-x-gourmantche) made the splitter skip past the next marker and slice
    // backwards, a panic that took down the whole Sources page.
    #[test]
    fn a_verse_glued_to_the_next_is_empty() {
        let v = verse_words("\\c 1\n\\v 5\\v 6 Ni yaa\n\\v 7-8a\\v 9 bi");
        assert_eq!(v[&(1, 5)], "");
        assert_eq!(v[&(1, 6)], "Ni yaa");
        assert_eq!(v[&(1, 7)], "");
        assert_eq!(v[&(1, 9)], "bi");
    }

    // Why: "the same text" must survive what re-stitching and tools change
    // (markers, spacing, headings) and still catch one changed word, or the
    // three-Tituses view either splits identical copies or merges different ones.
    #[test]
    fn same_words_are_the_same_verse() {
        let a = fingerprint(
            "\\id TIT\n\\c 1\n\\p\n\\v 1 Paul, a servant\n\\v 2 in hope\n\\c 2\n\\v 1 But you",
        );
        let b = fingerprint(
            "\\id TIT\n\\h Titus\n\\c 1\n\\s5\n\\p \\v 1 Paul,  a \\w servant|lemma=\"x\"\\w*\n\n\\s Greeting\n\\v 2 in hope\n\\c 2\n\\v 1 But you\n",
        );
        assert_eq!(a.id(), b.id(), "{a:?}\n{b:?}");
        let c = fingerprint(
            "\\c 1\n\\v 1 Paul, a slave\n\\v 2 in hope\n\\c 2\n\\v 1 But you\n\\v 2 older men",
        );
        assert_eq!(
            c.diff(&a),
            VerseDiff {
                changed: 1,
                added: 1,
                missing: 0
            }
        );
    }
}
