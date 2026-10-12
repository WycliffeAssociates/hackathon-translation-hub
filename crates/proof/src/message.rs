//! One plain English sentence per finding.
//!
//! A shortened version of the headlines in scripture-kitchen's English catalog
//! (`galley/sous-messages.en.json` @ v0.1.9), which is ICU MessageFormat for
//! a JS host. Marks are written in brackets (`[’]`), as that catalog's plain
//! rendering does, because a quoted quote mark cannot be read. Words keep
//! typographic quotes.

use sous_core::{
    BookKey, Form, HygieneClass, HygieneDigest, OuterClass, Pattern, PatternKey, ScalarKey, Side,
    Usual as SousUsual, unicode::Pool,
};

use crate::Usual;

pub(crate) fn hygiene(digest: HygieneDigest) -> String {
    let what = match digest.class() {
        HygieneClass::C0Control | HygieneClass::C1Control => {
            "An invisible control character is here"
        }
        HygieneClass::Delete => "An invisible delete character is here",
        HygieneClass::ReplacementChar => {
            "[\u{FFFD}] is here: a character was lost when the text was converted"
        }
        HygieneClass::StrayCarriageReturn => "A stray line-ending character is here",
        HygieneClass::StrandedBackslash => "A backslash is here, inside the text itself",
        HygieneClass::ConflictMarker => "This line is left over from merging two versions",
        HygieneClass::FreeCombiningMark => "An accent or other mark is here with no letter",
        HygieneClass::MisplacedFormat => "An invisible formatting character is here",
        HygieneClass::NoBreakSpace => "A no-break space is here, beside another space",
        HygieneClass::Noncharacter => "A character that never belongs in text is here",
    };
    match digest.run() {
        1 => format!("{what}."),
        run => format!("{what} ({run} in a row)."),
    }
}

pub(crate) fn convention(pattern: &Pattern, site: &str, book: BookKey) -> String {
    let glyph = mark(pattern.glyph);
    let (n, d) = (pattern.numerator, pattern.denominator);
    let only = if n == 1 {
        "only here".to_owned()
    } else {
        format!("only {n} of {d} times")
    };
    match pattern.key {
        PatternKey::Rarity => {
            let times = if n == 1 {
                "only here".to_owned()
            } else {
                format!("only {n} times")
            };
            match pattern.usual {
                SousUsual::Rarity {
                    glyph: Some(other),
                    count,
                    lookalike: true,
                } => format!(
                    "{glyph} appears {times} in this project; it writes the lookalike {} elsewhere ({count} times).",
                    mark(other)
                ),
                _ => format!("{glyph} appears {times} in this project."),
            }
        }
        PatternKey::Placement { side, class } => {
            format!("{glyph} {} appears {only}.", placement(side, class))
        }
        PatternKey::BookRate { side, class, .. } => format!(
            "{glyph} {} is far more common in {book} ({n} of {d}) than in the other books.",
            placement(side, class)
        ),
        PatternKey::ExactNeighbor(neighbor) => {
            let swapped = match pattern.usual {
                SousUsual::ExactNeighbor { reversed, .. } if reversed > 0 => format!(
                    "; this project writes [{}{}] {reversed} times",
                    bare(neighbor),
                    bare(pattern.glyph)
                ),
                _ => String::new(),
            };
            format!(
                "[{}{}] appears {only}{swapped}.",
                bare(pattern.glyph),
                bare(neighbor)
            )
        }
        PatternKey::PooledNeighbor(pool) => {
            format!(
                "{glyph} followed directly by {} appears {only}.",
                pool_name(pool)
            )
        }
        PatternKey::RunShape { .. } => {
            format!(
                "[{site}] is an unusual group of marks; {glyph} sits in a group like this {only}."
            )
        }
        PatternKey::Casing { form, .. } => match pattern.usual {
            SousUsual::Casing { form: usual, count } => format!(
                "“{site}” is {} here; this project writes it {} {count} times.",
                form_name(form),
                form_name(usual)
            ),
            _ => format!(
                "“{site}” is {} here, which this project rarely does.",
                form_name(form)
            ),
        },
        PatternKey::WordLength { .. } => {
            format!("“{site}” is much longer than most words in this project.")
        }
        PatternKey::Doubled { separated, .. } => {
            let between = if separated {
                " with only punctuation between"
            } else {
                ""
            };
            format!("“{site}” writes a word twice in a row{between}; the project does this {only}.")
        }
        PatternKey::LetterRun { length } => {
            let plus = if length == sous_core::words::LETTER_RUN_MAX {
                " or more"
            } else {
                ""
            };
            format!("{glyph} is written {length}{plus} times in a row in “{site}”.")
        }
        PatternKey::SentenceStart => format!(
            "“{site}” is lowercase after {glyph}; this project capitalizes the next word {} of {d} times.",
            d - n
        ),
    }
}

/// What the corpus usually does instead, when the row says.
pub(crate) fn usual(pattern: &Pattern) -> Option<Usual> {
    let (text, count) = match pattern.usual {
        SousUsual::None => return None,
        SousUsual::Placement { class, count } => (class_name(class).to_owned(), count),
        SousUsual::ExactNeighbor {
            neighbor, count, ..
        } => (mark(neighbor), count),
        SousUsual::RunShape { bucket, count, .. } => (format!("a group of {bucket}"), count),
        SousUsual::Rarity {
            glyph: Some(glyph),
            count,
            ..
        } => (mark(glyph), count),
        SousUsual::Rarity { glyph: None, .. } => return None,
        SousUsual::Casing { form, count } => (form_name(form).to_owned(), count),
        SousUsual::BookRate { baseline_bp, books } => (
            format!(
                "{:.1}% in the other books (median)",
                f64::from(baseline_bp) / 100.0
            ),
            books,
        ),
    };
    Some(Usual { text, count })
}

/// `[’]`, or `[U+00A0]` for a mark that does not show, or `a digit`.
fn mark(key: ScalarKey) -> String {
    if key.is_digits() {
        return "a digit".to_owned();
    }
    format!("[{}]", bare(key))
}

fn bare(key: ScalarKey) -> String {
    match key.scalar() {
        Some(c) if c.is_whitespace() || c.is_control() => format!("U+{:04X}", c as u32),
        Some(c) => c.to_string(),
        None => "0-9".to_owned(),
    }
}

fn placement(side: Side, class: OuterClass) -> String {
    match side {
        Side::Prev => format!("right after {}", class_name(class)),
        Side::Next => format!("right before {}", class_name(class)),
        Side::Both => "inside a word".to_owned(),
    }
}

fn class_name(class: OuterClass) -> &'static str {
    match class {
        OuterClass::Letter => "a letter",
        OuterClass::Space => "a space",
        OuterClass::Digit => "a digit",
        OuterClass::Nonletter => "a punctuation mark",
        OuterClass::Edge => "the edge of the text",
    }
}

fn pool_name(pool: Pool) -> &'static str {
    match pool {
        Pool::Quote => "a quotation mark",
        Pool::Bracket => "a bracket",
        Pool::Dash => "a dash",
        Pool::Terminal => "a mark that ends a sentence",
        Pool::Separator => "a comma or similar mark",
        Pool::Digit => "a digit",
        Pool::Symbol => "a symbol",
        Pool::Other => "another mark",
    }
}

fn form_name(form: Form) -> &'static str {
    match form {
        Form::Lower => "lowercase",
        Form::Title => "capitalized",
        Form::Upper => "in all capitals",
        Form::Mixed | Form::Uncased => "in mixed case",
    }
}
