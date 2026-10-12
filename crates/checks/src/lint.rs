//! A lint summary of one USFM book: counts by severity, by code and by chapter.
//!
//! The pipeline is the onion engine's own: `lex_with → cst::build → lint`, then
//! the engine's toc to say which chapter each finding falls in.
//!
//! **Severity** is the engine catalog's, walked up the escalation ladder with
//! the `\usfm` version the document declares (`LintRow::severity_at`, the same
//! walk as `DiagnosticView.severity` in onion-wasm's reader). Rows whose
//! severity is `form` (formatter-only) or gated off at this version are not
//! counted: the hub ranks by structure, not by formatting. `hint` has no bucket
//! of its own in [`LintSummary`] and is counted with `info`.
//!
//! **Legacy markers** follow Sefer's policy (`Sefer/src/app/legacyMarkers.ts`):
//! `\s5` (en_ulb's chunk marker) is registered as a `standalone` extension, with
//! the z-prefix relaxed, only for a text that already has `\s5` alone on a
//! line. A text that never used it but contains a stray one still reports it
//! as an unknown marker.
//!
//! **Why no global registry.** The engine's `set_extensions` installs a
//! PROCESS-WIDE registry that plain `lex` reads, which would race when the hub
//! lints thousands of books in parallel with rayon (one book's policy leaking
//! into another's parse). But `usfm_onion::lex_with(text, &Extensions)` takes a
//! registry per call, and nothing after the lexer consults the registry — cst,
//! lint and toc all work from the `marker_idx` the lexer already resolved. So
//! this module builds two immutable registries once (empty, and `\s5`) and
//! picks one per book. No lock, no global state, and `set_extensions` is never
//! called — so this crate cannot disturb any other engine user in the process.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Serialize;
use usfm_onion::extensions::{CustomMarker, ExtensionCategory, ExtensionOptions, Extensions};
use usfm_onion::lint::Severity;

/// What lint found in one book.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LintSummary {
    pub errors: u32,
    pub warnings: u32,
    /// `info` and `hint` findings together.
    pub infos: u32,
    /// Counted findings per code name (`unknown-marker`, `verse-gap`, …).
    pub by_code: BTreeMap<String, u32>,
    /// Per chapter: `[errors, warnings, infos]`. Findings before the first `\c`
    /// (or under a `\c` with no readable number) are chapter 0.
    pub by_chapter: BTreeMap<u32, [u32; 3]>,
}

/// The registry for a text that uses no legacy markers.
static PLAIN: LazyLock<Extensions> = LazyLock::new(Extensions::default);

/// The registry for a text that already uses `\s5`: Sefer's one legacy entry.
static WITH_S5: LazyLock<Extensions> = LazyLock::new(|| {
    let list = [CustomMarker {
        name: "s5".to_owned(),
        category: ExtensionCategory::Standalone,
        description: String::new(),
        attributes: Vec::new(),
    }];
    let (ext, refused) = Extensions::new_with(
        &list,
        &ExtensionOptions {
            relax_z_prefix: true,
        },
    );
    assert!(refused.is_empty(), "engine refused \\s5: {refused:?}");
    ext
});

/// Does `text` have `\s5` alone on a line? Sefer's regex, `^\\s5[ \t]*\r?$`
/// in multiline mode, written out by hand.
pub fn uses_s5(text: &str) -> bool {
    text.split('\n').any(|line| {
        let line = line.strip_suffix('\r').unwrap_or(line);
        line.strip_prefix("\\s5")
            .is_some_and(|rest| rest.bytes().all(|b| b == b' ' || b == b'\t'))
    })
}

/// Lints one book, registering `\s5` only if this text already uses it.
///
/// Sefer decides per PROJECT (any book using `\s5` registers it for all); a
/// caller that wants that should test [`uses_s5`] over the project's books and
/// call [`lint_book_with`].
pub fn lint_book(text: &str) -> LintSummary {
    lint_book_with(text, uses_s5(text))
}

/// Lints one book with `\s5` registered (`s5 = true`) or not.
pub fn lint_book_with(text: &str, s5: bool) -> LintSummary {
    let ext: &Extensions = if s5 { &WITH_S5 } else { &PLAIN };
    let source = text.as_bytes();
    let tokens = usfm_onion::lex_with(text, ext);
    let cst = usfm_onion::cst::build(&tokens);
    let report = usfm_onion::lint::lint(source, &tokens, &cst);
    let toc = usfm_onion::toc::toc(source, &tokens);

    let mut out = LintSummary::default();
    for observation in &report.observations {
        let row = observation.code.row();
        let bucket = match row.severity_at(report.declared_version) {
            Some(Severity::Error) => 0,
            Some(Severity::Warning) => 1,
            Some(Severity::Info | Severity::Hint) => 2,
            Some(Severity::Form) | None => continue,
        };
        match bucket {
            0 => out.errors += 1,
            1 => out.warnings += 1,
            _ => out.infos += 1,
        }
        *out.by_code.entry(row.name.to_owned()).or_default() += 1;
        // The anchor is a token index; the reader's `Toc.at(span.from)` is
        // the anchor token's first byte.
        let byte = tokens
            .get(observation.anchor as usize)
            .map_or(0, |token| token.start);
        let chapter = u32::from(toc.locate(byte).chapter);
        out.by_chapter.entry(chapter).or_default()[bucket] += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "\\id RUT\n\\usfm 3.0\n\\h Ruth\n\\mt Ruth\n";

    fn book(body: &str) -> String {
        format!("{HEAD}{body}")
    }

    #[test]
    fn clean_book_has_no_findings() {
        let text = book("\\c 1\n\\p\n\\v 1 One.\n\\v 2 Two.\n\\c 2\n\\p\n\\v 1 One.\n");
        let summary = lint_book(&text);
        assert_eq!(summary, LintSummary::default(), "{summary:#?}");
    }

    #[test]
    fn verse_gap_and_duplicate_land_in_their_chapter() {
        let text = book(
            "\\c 1\n\\p\n\\v 1 One.\n\\v 2 Two.\n\\c 2\n\\p\n\\v 1 One.\n\\v 3 Three.\n\\v 3 Again.\n",
        );
        let summary = lint_book(&text);
        assert_eq!(summary.by_code.get("verse-gap"), Some(&1), "{summary:#?}");
        assert_eq!(
            summary.by_code.get("verse-duplicate"),
            Some(&1),
            "{summary:#?}"
        );
        assert_eq!(summary.warnings, 2);
        assert_eq!(summary.by_chapter.get(&2), Some(&[0, 2, 0]));
        assert!(!summary.by_chapter.contains_key(&1));
    }

    #[test]
    fn unclosed_note_is_an_error() {
        let text = book("\\c 1\n\\p\n\\v 1 One \\f + \\ft a note.\n\\v 2 Two.\n");
        let summary = lint_book(&text);
        assert_eq!(
            summary.by_code.get("unclosed-note"),
            Some(&1),
            "{summary:#?}"
        );
        assert!(summary.errors >= 1);
        assert!(summary.by_chapter[&1][0] >= 1);
    }

    #[test]
    fn front_matter_findings_are_chapter_zero() {
        let text = "\\id RUT\n\\h Ruth\n\\zzz odd\n\\c 1\n\\p\n\\v 1 One.\n";
        let summary = lint_book(text);
        assert!(
            summary.by_chapter.get(&0).is_some_and(|c| c[1] >= 1),
            "{summary:#?}"
        );
    }

    #[test]
    fn s5_policy_matches_sefer() {
        // Uses \s5 alone on its lines: registered, so no unknown-marker.
        let uses = book("\\c 1\n\\s5\n\\p\n\\v 1 One.\n\\s5   \r\n\\v 2 Two.\n");
        assert!(uses_s5(&uses));
        let summary = lint_book(&uses);
        assert_eq!(summary.by_code.get("unknown-marker"), None, "{summary:#?}");

        // Never used \s5 on its own line; one stray inline: still reported.
        let stray = book("\\c 1\n\\p\n\\v 1 One. \\s5 \\v 2 Two.\n");
        assert!(!uses_s5(&stray));
        let summary = lint_book(&stray);
        assert_eq!(
            summary.by_code.get("unknown-marker"),
            Some(&1),
            "{summary:#?}"
        );

        // And the same text forced through the \s5 registry is quiet, which
        // is the project-wide switch a caller can make.
        let summary = lint_book_with(&stray, true);
        assert_eq!(summary.by_code.get("unknown-marker"), None, "{summary:#?}");
    }

    #[test]
    fn s5_registries_do_not_leak_across_threads() {
        let uses = book("\\c 1\n\\s5\n\\p\n\\v 1 One.\n");
        let stray = book("\\c 1\n\\p\n\\v 1 One. \\s5 \\v 2 Two.\n");
        std::thread::scope(|scope| {
            for i in 0..8 {
                let (uses, stray) = (&uses, &stray);
                scope.spawn(move || {
                    for _ in 0..200 {
                        let (text, want) = if i % 2 == 0 {
                            (uses, None)
                        } else {
                            (stray, Some(&1))
                        };
                        assert_eq!(lint_book(text).by_code.get("unknown-marker"), want);
                    }
                });
            }
        });
    }

    #[test]
    fn serializes() {
        let json = serde_json::to_string(&lint_book(&book("\\c 1\n\\p\n\\v 2 x\n"))).unwrap();
        assert!(json.contains("\"by_chapter\""), "{json}");
    }
}
