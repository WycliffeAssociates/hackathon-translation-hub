//! Repo markdown (and TSV notes) as sanitised HTML, with links that stay in
//! the hub.
//!
//! Translation resources are markdown: notes (`tn`), questions (`tq`), words
//! (`tw`), the manual (`ta`/`tm`). They link three ways, and each becomes a
//! hub link:
//!
//! - relative paths (`../kt/falsegod.md`): resolved against the file's folder,
//!   to the same repo's page for that file (an image, to its raw bytes);
//! - resource container links (`rc://en/tn/help/1jn/01/07`, and the bare
//!   `[[rc://en/ta/man/jit/translate-names]]` form, whose text is the
//!   target's title): resolved by the caller, which knows the repos;
//! - `http(s):` and `mailto:` links: kept.
//!
//! The HTML is CommonMark with GitHub's tables, task lists, strikethrough and
//! footnotes (pulldown-cmark), then sanitised (ammonia): repo content is
//! user content, and raw HTML or a `javascript:` link in a `.md` must never
//! reach the page.

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};

/// A resource container link, `rc://{lang}/{resource}/{type}/{project}/…`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rc {
    pub lang: String,
    pub resource: String,
    /// `man` (tA), `dict` (tW), `help` (tN, tQ), `book` (a Bible).
    pub kind: String,
    /// What follows: `["jit", "translate-names"]`, `["bible", "kt", "god"]`,
    /// `["1jn", "01", "07"]`.
    pub rest: Vec<String>,
}

impl Rc {
    pub fn parse(url: &str) -> Option<Self> {
        let path = url.strip_prefix("rc://")?;
        let mut parts = path
            .split('/')
            .filter(|p| !p.is_empty() && *p != "*")
            .map(str::to_owned);
        Some(Self {
            lang: parts.next()?,
            resource: parts.next()?,
            kind: parts.next().unwrap_or_default(),
            rest: parts.collect(),
        })
    }

    /// The text a bare `[[rc://…]]` shows when its target has no title: the
    /// last segment, or `1JN 1:7` for a note or question.
    pub fn fallback_title(&self) -> String {
        if self.kind == "help" && self.rest.len() >= 3 {
            let n = |s: &str| s.trim_start_matches('0').to_owned();
            return format!(
                "{} {}:{}",
                self.rest[0].to_uppercase(),
                n(&self.rest[1]),
                n(&self.rest[2])
            );
        }
        self.rest
            .last()
            .cloned()
            .unwrap_or_else(|| self.resource.clone())
            .replace('-', " ")
    }
}

/// Where links go; the caller knows the repo and the hub's routes.
pub trait Links {
    /// The folder of the file being rendered, repo-relative (`bible/kt`).
    fn here(&self) -> &str;
    /// The hub page for a file of this repo (`bible/kt/god.md`).
    fn page(&self, path: &str) -> String;
    /// The raw bytes of a file of this repo (an image).
    fn raw(&self, path: &str) -> String;
    /// A resource container link's hub page and its target's title, or `None`
    /// when no repo here has it.
    fn rc(&self, rc: &Rc) -> Option<(String, Option<String>)>;
}

/// `..` and `.` resolved: `bible/kt` + `../other/creation.md` →
/// `bible/other/creation.md`. `None` when it climbs out of the repo.
fn join(here: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = here.split('/').filter(|p| !p.is_empty()).collect();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            seg => parts.push(seg),
        }
    }
    Some(parts.join("/"))
}

fn has_scheme(url: &str) -> bool {
    url.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
    })
}

/// `[[rc://…]]` → `[title](rc://…)`, so the parser sees an ordinary link.
fn bare_rc_links(text: &str, links: &dyn Links) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[[rc://") {
        let Some(len) = rest[start + 2..].find("]]") else {
            break;
        };
        let url = &rest[start + 2..start + 2 + len];
        out.push_str(&rest[..start]);
        let title = Rc::parse(url).map_or_else(
            || url.to_owned(),
            |rc| {
                links
                    .rc(&rc)
                    .and_then(|(_, t)| t)
                    .unwrap_or_else(|| rc.fallback_title())
            },
        );
        // Square brackets in a title would end the link text early.
        out.push_str(&format!("[{}]({url})", title.replace(['[', ']'], "")));
        rest = &rest[start + 2 + len + 2..];
    }
    out.push_str(rest);
    out
}

/// A link or image target as the hub serves it.
fn rewrite(dest: &str, image: bool, links: &dyn Links) -> String {
    if let Some(rc) = Rc::parse(dest) {
        // Unresolved: the link keeps a scheme the sanitiser drops, so it
        // renders as its text, not a dead link.
        return links
            .rc(&rc)
            .map_or_else(|| dest.to_owned(), |(href, _)| href);
    }
    if dest.is_empty() || dest.starts_with('#') || dest.starts_with('/') || has_scheme(dest) {
        return dest.to_owned();
    }
    let (path, fragment) = dest.split_once('#').map_or((dest, ""), |(p, f)| (p, f));
    let path = path.split('?').next().unwrap_or(path);
    let Some(resolved) = join(links.here(), path) else {
        return dest.to_owned();
    };
    let href = if image {
        links.raw(&resolved)
    } else {
        links.page(&resolved)
    };
    if fragment.is_empty() {
        href
    } else {
        format!("{href}#{fragment}")
    }
}

fn sanitise(html: &str) -> String {
    ammonia::Builder::default()
        .link_rel(Some("noopener noreferrer"))
        .url_relative(ammonia::UrlRelative::PassThrough)
        .clean(html)
        .to_string()
}

/// Markdown as sanitised HTML with hub links.
pub fn render_markdown(text: &str, links: &dyn Links) -> String {
    let text = bare_rc_links(text, links);
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_FOOTNOTES);
    let events = Parser::new_ext(&text, options).map(|event| match event {
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: CowStr::from(rewrite(&dest_url, false, links)),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: CowStr::from(rewrite(&dest_url, true, links)),
            title,
            id,
        }),
        other => other,
    });
    let mut out = String::with_capacity(text.len() * 3 / 2);
    html::push_html(&mut out, events);
    sanitise(&out)
}

/// A TSV note file (`tn_GEN.tsv`) as a table, each cell's markdown rendered.
/// At most `max_rows` rows; the second value says whether more were left out.
/// TSV notes write a line break inside a cell as `<br>` or a literal `\n`.
pub fn render_tsv(text: &str, links: &dyn Links, max_rows: usize) -> (String, bool) {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let Some(header) = lines.next() else {
        return (String::new(), false);
    };
    let escape = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let mut out = String::from("<table class=\"tsv\"><thead><tr>");
    for h in header.split('\t') {
        out.push_str(&format!("<th>{}</th>", escape(h)));
    }
    out.push_str("</tr></thead><tbody>");
    let mut truncated = false;
    for (i, line) in lines.enumerate() {
        if i == max_rows {
            truncated = true;
            break;
        }
        out.push_str("<tr>");
        for cell in line.split('\t') {
            let cell = cell.replace("<br>", "\n").replace("\\n", "\n");
            out.push_str(&format!("<td>{}</td>", render_markdown(&cell, links)));
        }
        out.push_str("</tr>");
    }
    out.push_str("</tbody></table>");
    (sanitise(&out), truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Here;
    impl Links for Here {
        fn here(&self) -> &str {
            "bible/kt"
        }
        fn page(&self, path: &str) -> String {
            format!("/repos/o/n?file={path}")
        }
        fn raw(&self, path: &str) -> String {
            format!("/raw/{path}")
        }
        fn rc(&self, rc: &Rc) -> Option<(String, Option<String>)> {
            (rc.resource == "ta").then(|| {
                (
                    "/repos/o/en_tm?file=jit/translate-names/01.md".into(),
                    Some("Translate Names".into()),
                )
            })
        }
    }

    // Why: repo markdown is user content shown in the hub's own page. Raw HTML,
    // script and javascript: links must not survive, whatever the markdown says.
    #[test]
    fn user_html_is_sanitised() {
        let html = render_markdown(
            "<script>alert(1)</script>\n\n[x](javascript:alert(1)) <img src=x onerror=alert(1)>",
            &Here,
        );
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("javascript:"), "{html}");
        assert!(!html.contains("onerror"), "{html}");
    }

    // Why: the three link kinds in real tW/tN files (en_tw bible/kt/god.md) are
    // the point of rendering here, and relative `..` paths are easy to get wrong.
    #[test]
    fn links_stay_in_the_hub() {
        let html = render_markdown(
            "[create](../other/creation.md) ![map](../img/map.png) [[rc://en/ta/man/jit/translate-names]] [1 John 1:7](rc://en/tn/help/1jn/01/07) [web](https://example.org)",
            &Here,
        );
        assert!(
            html.contains("href=\"/repos/o/n?file=bible/other/creation.md\""),
            "{html}"
        );
        assert!(html.contains("src=\"/raw/bible/img/map.png\""), "{html}");
        assert!(html.contains(">Translate Names</a>"), "{html}");
        // No repo for tN here: the text stays, the dead link does not.
        assert!(
            html.contains("1 John 1:7") && !html.contains("rc://"),
            "{html}"
        );
        assert!(html.contains("href=\"https://example.org\""), "{html}");
    }
}
