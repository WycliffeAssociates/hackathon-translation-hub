//! hub-checks: what the hub learns about one USFM book from scripture-kitchen.
//!
//! - [`lint`]: a severity/code/chapter summary of the onion engine's findings,
//!   with Sefer's legacy-marker policy (`\s5`) applied per text.
//! - [`key`]: the content key lint results are cached under.
//! - [`coverage`]: which of a source text's verses a target text has.
//!
//! Lint results are a pure function of (book bytes, engine version), so the
//! hub caches them by `(content_key, KITCHEN_TAG)` — see [`key::cache_key`].

pub mod coverage;
pub mod key;
pub mod lint;

pub use coverage::{ChapterCoverage, Coverage, CoverageTotal, coverage, verses_by_chapter};
pub use key::{cache_key, content_key_for_blob, content_key_for_chunks};
pub use lint::{LintSummary, lint_book, lint_book_with, uses_s5};

/// The scripture-kitchen tag this crate's engine was built from. It must match
/// the `tag` of `usfm_onion` in the workspace Cargo.toml; it is part of every
/// lint cache key, so bumping the engine invalidates cached summaries.
pub const KITCHEN_TAG: &str = "v0.1.9";
