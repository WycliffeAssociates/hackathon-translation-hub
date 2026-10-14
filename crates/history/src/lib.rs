//! A Writer project's history, kept in the project's shape, not Writer's.
//!
//! Ported from Sefer `src/core/migrate/writerHistory.ts` at `50aa9cd` (Will,
//! 2026-10-09: replay every save rather than one assembly commit). Writer kept
//! one repository per book, chunk files (`05/03.txt`) and a commit per
//! autosave. A consolidated repo keeps every save that changed a book's text as
//! a commit of its own: the whole book file (`46-ROM.usfm`) as the migration
//! template renders it at that save (`hub-stitch`, the same template), with the
//! save's own author, committer, date and zone. Several books' saves are
//! interleaved by author time, so the log reads as the team worked.
//!
//! Writer's own messages are a timestamp, so a replayed commit says what it
//! changed, the verses of the chunks it touched (`TIT 1:6–7`), and keeps the
//! original and its source as trailers (`Writer-Message`, `Writer-Source`).
//!
//! What differs from Sefer, and why: Sefer reads through isomorphic-git over
//! the browser's storage and writes its own pack; here libgit2 reads the bare
//! repos on disk, books are read and rendered in parallel (rayon), and the
//! objects go into an ordinary repository that git packs on push.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use git2::{Delta, DiffOptions, Oid, Repository, Signature, Sort, Time};
use hub_stitch::{
    decode_text, is_writer_text_path, tree_from_files, usfm_file_name, writer_book_to_usfm,
};
use rayon::prelude::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("git: {0}")]
    Git(#[from] git2::Error),
    #[error("{0} is not a book of the canon")]
    Book(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Who and when, as git stores it: seconds, and the zone in minutes.
#[derive(Debug, Clone)]
pub struct Sig {
    pub name: String,
    pub email: String,
    pub at: i64,
    pub offset_minutes: i32,
}

impl Sig {
    fn of(s: &Signature<'_>) -> Self {
        Self {
            name: s.name().unwrap_or("").to_owned(),
            email: s.email().unwrap_or("").to_owned(),
            at: s.when().seconds(),
            offset_minutes: s.when().offset_minutes(),
        }
    }

    fn git(&self) -> std::result::Result<Signature<'static>, git2::Error> {
        // Writer's device identity (`tsDesktop`) stays the author where it was;
        // an empty name or email is not a signature libgit2 accepts.
        let name = if self.name.trim().is_empty() {
            "unknown"
        } else {
            &self.name
        };
        let email = if self.email.trim().is_empty() {
            "unknown@writer.invalid"
        } else {
            &self.email
        };
        Signature::new(name, email, &Time::new(self.at, self.offset_minutes))
    }
}

/// One Writer save that changed a text file the template reads.
#[derive(Debug, Clone)]
pub struct Save {
    pub id: String,
    pub author: Sig,
    pub committer: Sig,
    /// Writer's own message, first line: a timestamp, in practice.
    pub message: String,
    /// The files it changed and their new text; `None`: deleted (or not text).
    pub changes: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone)]
pub struct WriterHistory {
    /// Upper-case USFM id.
    pub book: String,
    /// The template's fallback title.
    pub title: String,
    /// `owner/repo`: where `Writer-Source` says a save came from.
    pub source: String,
    /// Oldest first.
    pub saves: Vec<Save>,
}

/// The saves of the bare repo at `repo` that changed a text file the template
/// reads, oldest first (topological, then time). A file that will not decode
/// reads as deleted, as the stitch leaves out a file it cannot read.
pub fn read_writer_history(
    repo: &Path,
    book: &str,
    title: &str,
    source: &str,
) -> Result<WriterHistory> {
    let git = Repository::open_bare(repo)?;
    let mut walk = git.revwalk()?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME | Sort::REVERSE)?;
    walk.push_head()?;
    let mut saves = Vec::new();
    for oid in walk {
        let commit = git.find_commit(oid?)?;
        let tree = commit.tree()?;
        // Against the first parent: Writer's history is a line, and a merge's
        // own change is what it brought to that line.
        let parent = if commit.parent_count() > 0 {
            Some(commit.parent(0)?.tree()?)
        } else {
            None
        };
        let mut opts = DiffOptions::new();
        opts.skip_binary_check(true);
        let diff = git.diff_tree_to_tree(parent.as_ref(), Some(&tree), Some(&mut opts))?;
        let mut changes = Vec::new();
        for delta in diff.deltas() {
            let file = match delta.status() {
                Delta::Deleted => delta.old_file(),
                _ => delta.new_file(),
            };
            let Some(path) = file.path().and_then(|p| p.to_str()) else {
                continue;
            };
            if !is_writer_text_path(path) {
                continue;
            }
            let body = match delta.status() {
                Delta::Deleted => None,
                _ => git
                    .find_blob(delta.new_file().id())
                    .ok()
                    .and_then(|b| decode_text(b.content())),
            };
            changes.push((path.to_owned(), body));
        }
        if changes.is_empty() {
            continue;
        }
        let message = commit
            .message()
            .unwrap_or("")
            .trim()
            .lines()
            .next()
            .unwrap_or("")
            .to_owned();
        saves.push(Save {
            id: commit.id().to_string(),
            author: Sig::of(&commit.author()),
            committer: Sig::of(&commit.committer()),
            message,
            changes,
        });
    }
    Ok(WriterHistory {
        book: book.to_owned(),
        title: title.to_owned(),
        source: source.to_owned(),
        saves,
    })
}

/// Verse numbers a chunk holds (a bridge `1-3` is three).
fn verse_count(chunk: &str) -> i64 {
    let mut n = 0;
    let mut rest = chunk;
    while let Some(i) = rest.find("\\v") {
        rest = &rest[i + 2..];
        let t = rest.trim_start();
        if t.len() == rest.len() {
            continue;
        }
        let num: String = t
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '-')
            .collect();
        let mut parts = num.split('-').filter_map(|p| p.parse::<i64>().ok());
        if let Some(from) = parts.next() {
            n += parts.next().map_or(1, |to| (to - from + 1).max(1));
        }
    }
    n
}

/// Verses present after each save, as `(author time, verses)`, oldest first:
/// the book's progress toward its full count, save by save.
pub fn progress(history: &WriterHistory) -> Vec<(i64, i64)> {
    let mut per_chunk: HashMap<&str, i64> = HashMap::new();
    let mut total = 0;
    history
        .saves
        .iter()
        .map(|save| {
            for (path, body) in &save.changes {
                let now = body.as_deref().map_or(0, verse_count);
                let was = per_chunk.insert(path.as_str(), now).unwrap_or(0);
                total += now - was;
            }
            (save.author.at, total)
        })
        .collect()
}

/// `5:1–11`: the chapter and the verses a chunk holds; a chunk with no verse
/// marker says so rather than a bare `5` that reads as a verse.
fn verses_of(chapter: u32, chunk: &str) -> String {
    let mut low = u32::MAX;
    let mut high = 0;
    let mut rest = chunk;
    while let Some(i) = rest.find("\\v") {
        rest = &rest[i + 2..];
        let t = rest.trim_start();
        if t.len() == rest.len() {
            continue; // `\va`, `\vp`: not a verse marker
        }
        let num: String = t
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '-')
            .collect();
        let mut parts = num.split('-').filter_map(|n| n.parse::<u32>().ok());
        if let Some(from) = parts.next() {
            let to = parts.next().unwrap_or(from);
            low = low.min(from);
            high = high.max(to);
        }
    }
    if low == u32::MAX {
        format!("{chapter} (no verse number)")
    } else if low == high {
        format!("{chapter}:{low}")
    } else {
        format!("{chapter}:{low}–{high}")
    }
}

/// What a save did to its book, in the project's terms: `ROM 5:1–11, 6:4–6; titles`.
fn summary_of(
    book: &str,
    changes: &[(String, Option<String>)],
    before: &BTreeMap<String, String>,
) -> String {
    let mut chunks: Vec<&str> = changes
        .iter()
        .map(|(p, _)| p.as_str())
        .filter(|p| {
            let mut parts = p.split('/');
            matches!((parts.next(), parts.next(), parts.next()), (Some(c), Some(f), None)
                if (2..=3).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_digit())
                && f.strip_suffix(".txt").is_some_and(|n| (2..=3).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit())))
        })
        .collect();
    chunks.sort_unstable();
    let now: HashMap<&str, Option<&str>> = changes
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_deref()))
        .collect();
    let where_: Vec<String> = chunks
        .iter()
        .map(|p| {
            let chapter = p
                .split('/')
                .next()
                .and_then(|c| c.parse().ok())
                .unwrap_or(0);
            let text = now
                .get(p)
                .copied()
                .flatten()
                .or(before.get(*p).map(String::as_str))
                .unwrap_or("");
            verses_of(chapter, text)
        })
        .collect();
    let shown = if where_.len() > 6 {
        format!("{}, +{} more", where_[..6].join(", "), where_.len() - 6)
    } else {
        where_.join(", ")
    };
    let titles = changes.iter().any(|(p, _)| p.ends_with("title.txt"));
    let said: Vec<&str> = [shown.as_str(), if titles { "titles" } else { "" }]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    if said.is_empty() {
        book.to_owned()
    } else {
        format!("{book} {}", said.join("; "))
    }
}

/// One rendered save, ready to commit.
struct Step<'a> {
    key: i64,
    book_index: usize,
    save_index: usize,
    save: &'a Save,
    file: String,
    usfm: String,
    message: String,
}

#[derive(Debug, Clone)]
pub struct Replayed {
    /// The last replayed commit: what anything after commits on top of.
    pub head: Option<Oid>,
    pub commits: usize,
    /// Saves that changed no rendered text (a manifest save, an unchanged chunk).
    pub unchanged: usize,
}

/// Every history's saves as commits in `out`, oldest first by author time, on
/// top of `parent` (or as a new root). Books render in parallel; the commits
/// are one line. `out`'s refs are not touched: the caller points a branch at
/// `head`.
pub fn replay(
    histories: &[WriterHistory],
    out: &Repository,
    parent: Option<Oid>,
) -> Result<Replayed> {
    // Render each book's saves in order, in parallel across books: the book
    // file after every save, and what the save changed. A save that leaves the
    // rendering as it was is dropped here.
    let rendered: Vec<Result<(Vec<Step<'_>>, usize)>> = histories
        .par_iter()
        .enumerate()
        .map(|(book_index, h)| {
            let file = usfm_file_name(&h.book).ok_or_else(|| Error::Book(h.book.clone()))?;
            let mut held: BTreeMap<String, String> = BTreeMap::new();
            let mut last = String::new();
            let mut steps = Vec::new();
            let mut unchanged = 0;
            // A book's own saves never reorder: a device whose clock ran wrong
            // (a reset battery's 2000-01-01) would put a save before an earlier
            // one, so a save's key is never earlier than the one before it.
            let mut key = i64::MIN;
            for (save_index, save) in h.saves.iter().enumerate() {
                key = key.max(save.author.at);
                let before = held.clone();
                for (path, body) in &save.changes {
                    match body {
                        Some(text) => held.insert(path.clone(), text.clone()),
                        None => held.remove(path),
                    };
                }
                let tree = tree_from_files(held.iter().map(|(p, t)| (p.as_str(), t.as_str())));
                let usfm = writer_book_to_usfm(&tree, &h.book, &h.title);
                if usfm == last {
                    unchanged += 1;
                    continue;
                }
                last.clone_from(&usfm);
                let message = format!(
                    "{}\n\nWriter-Message: {}\nWriter-Source: {}@{}\n",
                    summary_of(&h.book, &save.changes, &before),
                    save.message,
                    h.source,
                    save.id
                );
                steps.push(Step {
                    key,
                    book_index,
                    save_index,
                    save,
                    file: file.clone(),
                    usfm,
                    message,
                });
            }
            Ok((steps, unchanged))
        })
        .collect();

    let mut steps = Vec::new();
    let mut unchanged = 0;
    for r in rendered {
        let (s, u) = r?;
        steps.extend(s);
        unchanged += u;
    }
    steps.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then(a.book_index.cmp(&b.book_index))
            .then(a.save_index.cmp(&b.save_index))
    });

    // Objects go to an in-memory store and leave as one pack at the end:
    // written loose, one file per object, 3,965 commits of ten books took
    // 175 s. (Sefer wrote its own pack for the same reason in the browser.)
    // On a handle of its own: the in-memory store stays attached to the
    // handle it was added to, and the caller's later writes (a manifest
    // commit) must go to disk, not into memory nobody dumps.
    let work = Repository::open(out.path())?;
    let odb = work.odb()?;
    let mempack = odb.add_new_mempack_backend(1000)?;

    // One line of commits; the root tree holds every book file written so far.
    let mut files: BTreeMap<String, Oid> = BTreeMap::new();
    if let Some(p) = parent {
        for entry in work.find_commit(p)?.tree()?.iter() {
            if let Some(name) = entry.name() {
                files.insert(name.to_owned(), entry.id());
            }
        }
    }
    let mut head = parent;
    for step in &steps {
        files.insert(step.file.clone(), work.blob(step.usfm.as_bytes())?);
        let mut tb = work.treebuilder(None)?;
        for (name, oid) in &files {
            tb.insert(name, *oid, 0o100644)?;
        }
        let tree = work.find_tree(tb.write()?)?;
        let parents: Vec<git2::Commit<'_>> = head
            .map(|h| work.find_commit(h))
            .transpose()?
            .into_iter()
            .collect();
        let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();
        head = Some(work.commit(
            None,
            &step.save.author.git()?,
            &step.save.committer.git()?,
            &step.message,
            &tree,
            &parent_refs,
        )?);
    }
    let mut pack = git2::Buf::new();
    mempack.dump(&work, &mut pack)?;
    let mut writer = odb.packwriter()?;
    std::io::Write::write_all(&mut writer, &pack)
        .map_err(|e| git2::Error::from_str(&e.to_string()))?;
    writer.commit()?;
    mempack.reset()?;
    // The new pack is on disk; the caller's handle looks again so it can
    // read the replayed commits (a ref to them, a commit on top, a push).
    out.odb()?.refresh()?;
    Ok(Replayed {
        head,
        commits: steps.len(),
        unchanged,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Why: the subject line is the only "intent" a replayed commit carries,
    // and it is a port of Sefer's `versesOf`/`summaryOf` (bridges, chunks with
    // no verse marker, titles, the "+N more" cut). Easy to get subtly wrong.
    #[test]
    fn summaries_say_what_changed() {
        let before = BTreeMap::new();
        let ch = |p: &str, t: &str| (p.to_owned(), Some(t.to_owned()));
        assert_eq!(
            summary_of(
                "ROM",
                &[ch("05/01.txt", "\\v 1 a \\v 2-3 b \\v 11 c")],
                &before
            ),
            "ROM 5:1–11"
        );
        assert_eq!(
            summary_of("ROM", &[ch("05/01.txt", "no marker")], &before),
            "ROM 5 (no verse number)"
        );
        assert_eq!(
            summary_of("ROM", &[ch("05/title.txt", "Chapter 5")], &before),
            "ROM titles"
        );
        assert_eq!(
            summary_of("ROM", &[ch("manifest.json", "{}")], &before),
            "ROM"
        );
        let many: Vec<_> = (1..=8)
            .map(|i| ch(&format!("01/{i:02}.txt"), &format!("\\v {i} x")))
            .collect();
        assert_eq!(
            summary_of("TIT", &many, &before),
            "TIT 1:1, 1:2, 1:3, 1:4, 1:5, 1:6, +2 more"
        );
    }
}
