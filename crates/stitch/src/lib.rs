//! hub-stitch: a BTT Writer (translationStudio) book project, turned into one
//! USFM book.
//!
//! PORTED FROM Sefer's `src/core/migrate/writerBook.ts` (and the book order in
//! `src/core/location/canon.ts`) at Sefer commit `50aa9cd`. The hub and Sefer
//! must stitch a Writer book to the same bytes, so the two are kept in step:
//! when Sefer's template changes, this file changes with it, and the other way
//! round.
//!
//! Writer kept one project per book, in its own shape:
//!
//! ```text
//!   manifest.json        the book, the language, the resource
//!   front/title.txt      the book's title, one line per \mt
//!   NN/title.txt         a chapter's label, when it has one
//!   NN/NN.txt            the chunks, as inline USFM fragments; the first
//!                        chunk of a chapter carries its \c
//! ```
//!
//! ONE TEMPLATE, and it copies structure only: the header usfm-tools'
//! `txt2USFM.py` writes (`writeHeader`), then the chapters and chunks in
//! numeric order. It does not repair the translator's text — no verse-marker
//! fixes, no `\p` added or removed, no title-casing, no guessed headings.
//!
//! Only the pure part of Sefer's file is here: which paths are the book, the
//! tree read from those files, the template, and the RC file name. Reading the
//! manifest and the files themselves is the caller's job. Sefer decodes every
//! file before `treeFromFiles` sees it, and that step changes bytes, so it is
//! ported too: pass each file's bytes through [`decode_text`] first, and leave
//! out the ones it refuses.
//!
//! Where JavaScript and Rust disagree on a primitive, the JavaScript meaning is
//! the one kept: `trim` and `\s` here are ECMAScript's whitespace (which counts
//! U+FEFF and not U+0085), and `\d` is the ASCII digits only.

use std::collections::{BTreeMap, HashMap};

/// The book's parts, in numeric order. Pure data for the template.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WriterTree {
    /// `front/title.txt`, one entry per non-empty line, each trimmed.
    pub title: Vec<String>,
    pub chapters: Vec<WriterChapter>,
}

/// One chapter of a [`WriterTree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterChapter {
    pub number: u32,
    /// `NN/title.txt`, when it says something other than the number.
    pub label: Option<String>,
    /// The chunk files' text, as written, in chunk-number order.
    pub chunks: Vec<String>,
}

/// The protestant canon's book ids, in order. Only the order is needed here:
/// it is what numbers the RC file name. (Sefer: `CANON` in `canon.ts`.)
pub const CANON: [&str; 66] = [
    "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA", "1KI", "2KI", "1CH",
    "2CH", "EZR", "NEH", "EST", "JOB", "PSA", "PRO", "ECC", "SNG", "ISA", "JER", "LAM", "EZK",
    "DAN", "HOS", "JOL", "AMO", "OBA", "JON", "MIC", "NAM", "HAB", "ZEP", "HAG", "ZEC", "MAL",
    "MAT", "MRK", "LUK", "JHN", "ACT", "ROM", "1CO", "2CO", "GAL", "EPH", "PHP", "COL", "1TH",
    "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAS", "1PE", "2PE", "1JN", "2JN", "3JN", "JUD",
    "REV",
];

/// ECMAScript's whitespace: what `String.prototype.trim` and a regex `\s`
/// mean. Rust's `char::is_whitespace` differs on two code points: it counts
/// U+0085 (NEL), which JavaScript does not, and not U+FEFF (BOM), which
/// JavaScript does.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// `text.trim()`, as JavaScript trims.
fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

/// `/^\d{2,3}$/`: two or three ASCII digits and nothing else.
fn is_two_or_three_digits(name: &str) -> bool {
    (2..=3).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_digit())
}

/// A Writer file's bytes as the text `tree_from_files` expects, or `None` when
/// they are not UTF-8 (Sefer reads such a file as absent, or as deleted when
/// replaying history). Ported from the text half of `decode` in Sefer's
/// `src/core/source/source.ts`:
///
/// - a leading UTF-8 byte order mark is dropped, and then a second one too:
///   Sefer strips one itself and hands the rest to a `TextDecoder`, which by
///   default drops a BOM of its own;
/// - the bytes must be valid UTF-8, with no repair (`fatal: true`);
/// - every `\r\n`, and every lone `\r`, becomes `\n`.
///
/// The line endings matter to the bytes: a chunk is trimmed at its ends only,
/// so a CRLF inside it would otherwise reach the book.
pub fn decode_text(bytes: &[u8]) -> Option<String> {
    const BOM: &[u8] = b"\xEF\xBB\xBF";
    let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let body = body.strip_prefix(BOM).unwrap_or(body);
    let text = std::str::from_utf8(body).ok()?;
    Some(text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// `01`, `150`; not `00`, which Writer uses for front matter.
fn numbered(name: &str) -> Option<u32> {
    if !is_two_or_three_digits(name) {
        return None;
    }
    let number: u32 = name.parse().ok()?;
    (number > 0).then_some(number)
}

/// `NN/title.txt` or `NN/NN.txt` (any case, as the `/i` flag allows), split
/// into the chapter folder and the file's stem. The regex
/// `^(\d{2,3})\/(title|\d{2,3})\.txt$`, by hand.
fn split_chapter_path(path: &str) -> Option<(&str, &str)> {
    let (folder, file) = path.split_once('/')?;
    if !is_two_or_three_digits(folder) {
        return None;
    }
    // `.txt` is ASCII, so the stem ends on a character boundary.
    if file.len() < 4 || !file.is_char_boundary(file.len() - 4) {
        return None;
    }
    let (stem, extension) = file.split_at(file.len() - 4);
    if !extension.eq_ignore_ascii_case(".txt") {
        return None;
    }
    (stem.eq_ignore_ascii_case("title") || is_two_or_three_digits(stem)).then_some((folder, stem))
}

/// The files the template reads: `front/title.txt`, and each chapter's
/// `NN/title.txt` and `NN/NN.txt` chunks. Everything else in a Writer
/// project (the manifest, `LICENSE.md`, `.git`) is not the book.
///
/// Like Sefer, `00/…` passes here (it is a well-formed name) and is dropped
/// later by [`tree_from_files`], which only takes chapters above zero.
pub fn is_writer_text_path(path: &str) -> bool {
    path == "front/title.txt" || split_chapter_path(path).is_some()
}

/// The book's parts, from its files as `(relative path, text)`. Pure: the same
/// tree from a folder on disk or from a commit's blobs.
///
/// Sefer takes a `Map`, so the input is read as one: a path given twice keeps
/// its first position and its last text. Order otherwise matters only where
/// two files land on the same number (`01/01.txt` and `01/001.txt`, `01/` and
/// `001/`): chunks keep the order they were given in (a stable sort), and of
/// two title files the later one wins.
pub fn tree_from_files<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> WriterTree {
    // As a JS Map: insertion order, last value per key.
    let mut order: Vec<(&str, &str)> = Vec::new();
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (path, text) in files {
        match seen.get(path) {
            Some(&at) => order[at].1 = text,
            None => {
                seen.insert(path, order.len());
                order.push((path, text));
            }
        }
    }

    let front = order
        .iter()
        .find(|(path, _)| *path == "front/title.txt")
        .map_or("", |(_, text)| *text);
    // `split(/\r?\n/)` then `trim`: the trim takes any `\r` with it.
    let title = front
        .split('\n')
        .map(js_trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();

    struct Held<'a> {
        label: Option<&'a str>,
        chunks: Vec<(u32, &'a str)>,
    }
    let mut by_chapter: BTreeMap<u32, Held> = BTreeMap::new();
    for (path, text) in order {
        let Some((folder, stem)) = split_chapter_path(path) else {
            continue;
        };
        let Some(chapter) = numbered(folder) else {
            continue;
        };
        let held = by_chapter.entry(chapter).or_insert(Held {
            label: None,
            chunks: Vec::new(),
        });
        if stem.eq_ignore_ascii_case("title") {
            held.label = Some(js_trim(text));
        } else if let Some(chunk) = numbered(stem) {
            held.chunks.push((chunk, text));
        }
    }

    let chapters = by_chapter
        .into_iter()
        .filter(|(_, held)| !held.chunks.is_empty())
        .map(|(number, mut held)| {
            // Stable, as `Array.prototype.sort` is.
            held.chunks.sort_by_key(|(chunk, _)| *chunk);
            let label = held
                .label
                .filter(|label| !label.is_empty() && *label != number.to_string())
                .map(str::to_string);
            WriterChapter {
                number,
                label,
                chunks: held
                    .chunks
                    .into_iter()
                    .map(|(_, text)| text.to_string())
                    .collect(),
            }
        })
        .collect();

    WriterTree { title, chapters }
}

/// A chunk's own leading `\c N`, every one of them: the template writes the
/// chapter marker once. Writer sometimes saved it twice (`\c 3 \c 3 \v 1`,
/// Moffat_chola's bwc REV), and the second made the book unreadable to the
/// engine. That marker is Writer's chunk framing, not the translator's text.
///
/// The regex `^(?:\s*\\c\s+\d+\s*)+`, by hand. Taking each repetition as
/// greedily as it goes is the same match the regex finds: backtracking only
/// ever gives a shorter one.
fn without_chapter_marker(chunk: &str) -> &str {
    let mut rest = chunk;
    loop {
        let candidate = rest.trim_start_matches(is_js_space);
        let Some(after_c) = candidate.strip_prefix("\\c") else {
            return rest;
        };
        let after_space = after_c.trim_start_matches(is_js_space);
        if after_space.len() == after_c.len() {
            return rest;
        }
        let after_digits = after_space.trim_start_matches(|c: char| c.is_ascii_digit());
        if after_digits.len() == after_space.len() {
            return rest;
        }
        rest = after_digits.trim_start_matches(is_js_space);
    }
}

/// The template. Header as txt2USFM's `writeHeader` writes it (`\toc3` is the
/// book id in lower case; one `\mt` per title line), then each chapter: `\c`,
/// `\cl` when the chapter has its own label, then its chunks as written, each
/// trimmed and the empty ones left out. `book` is the claimed id; Writer's
/// chunks carry no `\id`. `fallback_title` stands in for a book with no
/// `front/title.txt` (or an empty one).
pub fn writer_book_to_usfm(tree: &WriterTree, book: &str, fallback_title: &str) -> String {
    let fallback = [fallback_title.to_string()];
    let title: &[String] = if tree.title.is_empty() {
        &fallback
    } else {
        &tree.title
    };
    let head = title[0].as_str();
    let mut lines: Vec<String> = vec![
        format!("\\id {book}"),
        "\\ide UTF-8".to_string(),
        format!("\\h {head}"),
        format!("\\toc1 {head}"),
        format!("\\toc2 {head}"),
        // JavaScript's `toLowerCase` is full Unicode lower-casing, as Rust's.
        format!("\\toc3 {}", book.to_lowercase()),
    ];
    for (index, line) in title.iter().enumerate() {
        lines.push(format!("\\mt{} {line}", index + 1));
    }
    for chapter in &tree.chapters {
        lines.push(format!("\\c {}", chapter.number));
        if let Some(label) = &chapter.label {
            lines.push(format!("\\cl {label}"));
        }
        for (index, chunk) in chapter.chunks.iter().enumerate() {
            let text = js_trim(if index == 0 {
                without_chapter_marker(chunk)
            } else {
                chunk
            });
            if !text.is_empty() {
                lines.push(text.to_string());
            }
        }
    }
    let mut usfm = lines.join("\n");
    usfm.push('\n');
    usfm
}

/// `44-JHN.usfm`: the RC file name, numbered as the catalogue numbers books
/// (MAT is 41: the Old Testament is 01–39, the New starts at 41).
///
/// `None` for an id the canon does not know (matched exactly, upper case).
/// Sefer has no such case to answer — it only stitches books its manifest
/// check found in the canon — and would write `00-XYZ.usfm` if asked.
pub fn usfm_file_name(book: &str) -> Option<String> {
    let index = CANON.iter().position(|entry| *entry == book)?;
    let number = if index < 39 { index + 1 } else { index + 2 };
    Some(format!("{number:02}-{book}.usfm"))
}

#[cfg(test)]
mod tests;
