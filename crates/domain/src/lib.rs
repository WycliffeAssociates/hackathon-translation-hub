//! The Bible shape's rules, with no IO.
//!
//! A language's work is bucketed by Version (resource type: reg, ulb, …) and
//! Track (text, audio); within that, each book has candidate repos. This crate
//! answers the per-book questions the hub asks of those candidates:
//!
//! - [`official_of`]: which one is Official. Sefer's `defaultCandidate`
//!   (`src/core/migrate/candidate.ts`), ported as is so the two agree.
//! - [`book_state`]: what the coverage shelf shows for the book.
//! - [`shown_twice_on_biel`] and [`biel_collates`]: whether BIEL stitches two
//!   sources for the book. workTracker's `shared/biel-duplicate-chapters-primary.ts`
//!   is the source of truth for that rule (kept in step with biel-frontend).
//!
//! Candidates are anything that implements [`Candidate`], so the store, the
//! catalog sync and tests can each use their own row type.

use serde::{Deserialize, Serialize};

/// The catalog's label for a repo (Port, via `wa_content_metadata.status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    Primary,
    Active,
    Inactive,
}

impl Status {
    /// The catalog's spelling; anything else is unlabelled.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "Primary" => Some(Self::Primary),
            "Active" => Some(Self::Active),
            "Inactive" => Some(Self::Inactive),
            _ => None,
        }
    }
}

/// What the rules need to know about one candidate repo for one book.
pub trait Candidate {
    fn status(&self) -> Option<Status>;
    /// When the catalog last saw it change, as an ISO-8601 string. Compared as
    /// text, newest wins, exactly as Sefer compares `hints.modified`.
    fn modified(&self) -> Option<&str>;
    /// The catalog's `show_on_biel`.
    fn shown_on_biel(&self) -> bool {
        false
    }
}

/// Sefer's rank: Primary, then Active, then unlabelled; Inactive last.
fn rank(status: Option<Status>) -> u8 {
    match status {
        Some(Status::Primary) => 0,
        Some(Status::Active) => 1,
        None => 2,
        Some(Status::Inactive) => 3,
    }
}

/// The Official repo for one book: the only candidate; else the Primary; else
/// the newest Active; else the newest unlabelled. A book whose several
/// candidates are all Inactive has none.
///
/// Ties keep the input order (the sort is stable), as in Sefer.
pub fn official_of<C: Candidate>(candidates: &[C]) -> Option<&C> {
    if candidates.len() == 1 {
        return candidates.first();
    }
    let mut ranked: Vec<&C> = candidates
        .iter()
        .filter(|c| c.status() != Some(Status::Inactive))
        .collect();
    ranked.sort_by(|a, b| {
        rank(a.status())
            .cmp(&rank(b.status()))
            .then_with(|| b.modified().unwrap_or("").cmp(a.modified().unwrap_or("")))
    });
    ranked.first().copied()
}

/// What one book's cell on a coverage shelf shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BookState {
    /// Exactly one Primary.
    Official,
    /// Work exists but nothing is Primary.
    Draft,
    /// More than one Primary claims the book.
    Conflict,
    /// Only Inactive repos.
    Inactive,
    /// No repo at all.
    Empty,
}

pub fn book_state<C: Candidate>(candidates: &[C]) -> BookState {
    if candidates.is_empty() {
        return BookState::Empty;
    }
    let primaries = candidates
        .iter()
        .filter(|c| c.status() == Some(Status::Primary))
        .count();
    match primaries {
        0 if candidates
            .iter()
            .all(|c| c.status() == Some(Status::Inactive)) =>
        {
            BookState::Inactive
        }
        0 => BookState::Draft,
        1 => BookState::Official,
        _ => BookState::Conflict,
    }
}

/// BIEL shows a book twice when more than one of its repos is both Primary
/// and shown. Group candidates by language, domain, resource type and track
/// before asking, as workTracker does.
pub fn shown_twice_on_biel<C: Candidate>(candidates: &[C]) -> bool {
    candidates
        .iter()
        .filter(|c| c.status() == Some(Status::Primary) && c.shown_on_biel())
        .count()
        > 1
}

/// The languages BIEL collates (and so can show twice): every non-gateway
/// language, plus these gateway exceptions.
const GATEWAY_COLLATE_EXCEPTIONS: &[&str] = &["ceb"];

pub fn biel_collates(is_gateway: bool, ietf: &str) -> bool {
    !is_gateway || GATEWAY_COLLATE_EXCEPTIONS.contains(&ietf)
}

/// Only these domains are stitched by book and chapter, so only they can show
/// a book twice.
pub const STITCHED_DOMAINS: &[&str] = &["scripture", "gloss", "parascriptural"];

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Row {
        name: &'static str,
        status: Option<Status>,
        modified: Option<&'static str>,
        shown: bool,
    }

    impl Candidate for Row {
        fn status(&self) -> Option<Status> {
            self.status
        }
        fn modified(&self) -> Option<&str> {
            self.modified
        }
        fn shown_on_biel(&self) -> bool {
            self.shown
        }
    }

    fn row(name: &'static str, status: Option<Status>, modified: Option<&'static str>) -> Row {
        Row {
            name,
            status,
            modified,
            shown: false,
        }
    }

    fn pick(rows: &[Row]) -> Option<&'static str> {
        official_of(rows).map(|r| r.name)
    }

    #[test]
    fn the_only_candidate_wins_even_when_inactive() {
        assert_eq!(pick(&[row("a", Some(Status::Inactive), None)]), Some("a"));
    }

    #[test]
    fn primary_beats_a_newer_active() {
        let rows = [
            row("active", Some(Status::Active), Some("2026-09-01")),
            row("primary", Some(Status::Primary), Some("2020-01-01")),
        ];
        assert_eq!(pick(&rows), Some("primary"));
    }

    #[test]
    fn newest_active_beats_older_active_and_unlabelled() {
        let rows = [
            row("old", Some(Status::Active), Some("2024-01-01")),
            row("none", None, Some("2026-10-01")),
            row("new", Some(Status::Active), Some("2025-06-01")),
        ];
        assert_eq!(pick(&rows), Some("new"));
    }

    #[test]
    fn unlabelled_beats_inactive_and_several_inactive_pick_none() {
        let rows = [
            row("in", Some(Status::Inactive), Some("2026-01-01")),
            row("none", None, None),
        ];
        assert_eq!(pick(&rows), Some("none"));
        let all_inactive = [
            row("a", Some(Status::Inactive), None),
            row("b", Some(Status::Inactive), None),
        ];
        assert_eq!(pick(&all_inactive), None);
    }

    #[test]
    fn ties_keep_input_order() {
        let rows = [
            row("first", Some(Status::Active), None),
            row("second", Some(Status::Active), None),
        ];
        assert_eq!(pick(&rows), Some("first"));
    }

    #[test]
    fn book_states() {
        assert_eq!(book_state::<Row>(&[]), BookState::Empty);
        assert_eq!(
            book_state(&[row("a", Some(Status::Inactive), None)]),
            BookState::Inactive
        );
        assert_eq!(
            book_state(&[row("a", None, None), row("b", Some(Status::Inactive), None)]),
            BookState::Draft
        );
        assert_eq!(
            book_state(&[
                row("a", Some(Status::Primary), None),
                row("b", Some(Status::Active), None)
            ]),
            BookState::Official
        );
        assert_eq!(
            book_state(&[
                row("a", Some(Status::Primary), None),
                row("b", Some(Status::Primary), None)
            ]),
            BookState::Conflict
        );
    }

    #[test]
    fn shown_twice_needs_two_rows_both_primary_and_shown() {
        let shown = |name, status| Row {
            name,
            status,
            modified: None,
            shown: true,
        };
        assert!(shown_twice_on_biel(&[
            shown("a", Some(Status::Primary)),
            shown("b", Some(Status::Primary))
        ]));
        assert!(!shown_twice_on_biel(&[
            shown("a", Some(Status::Primary)),
            shown("b", Some(Status::Active))
        ]));
        let hidden = Row {
            name: "b",
            status: Some(Status::Primary),
            modified: None,
            shown: false,
        };
        assert!(!shown_twice_on_biel(&[
            shown("a", Some(Status::Primary)),
            hidden
        ]));
    }

    #[test]
    fn collation_skips_gateways_except_ceb() {
        assert!(biel_collates(false, "gl"));
        assert!(!biel_collates(true, "en"));
        assert!(biel_collates(true, "ceb"));
    }
}
