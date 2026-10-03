//! Link rendering: inline destinations and bare-URL linkification.

use std::sync::LazyLock;

use regex::Regex;

use super::inline;
use super::style::{Run, Style};
use super::theme;

/// How a link's destination is shown after its text.
///
/// The URL is printed verbatim. There is nowhere to click in a terminal
/// transcript, so hiding the destination -- the usual choice for a
/// mouse-driven viewer -- would make it unreachable.
pub fn display_url(url: &str) -> &str {
    url
}

/// Matches a bare URL in running text.
///
/// Stops at whitespace and at the brackets and quotes that delimit URLs
/// in prose. Parentheses *are* matched, because URLs legitimately
/// contain them (`.../Foo_(bar)`); [`trim_url_tail`] then drops the ones
/// that belong to the surrounding sentence.
static BARE_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s<>\[\]{}"'`]+"#).expect("bare url regex"));

/// Trailing characters that are almost always sentence punctuation
/// rather than part of the address.
fn trim_url_tail(url: &str) -> (String, &str) {
    let mut end = url.len();
    while end > 0 {
        let rest = &url[..end];
        let last = rest.chars().next_back().unwrap();
        // Trailing sentence punctuation.
        if matches!(last, '.' | ',' | ';' | ':' | '!' | '?' | '*' | '_' | '~') {
            end -= last.len_utf8();
            continue;
        }
        // Unbalanced closers belong to the surrounding sentence, as in
        // "(see https://example.com/a)".
        if matches!(last, ')' | ']' | '}') {
            let open = match last {
                ')' => '(',
                ']' => '[',
                _ => '{',
            };
            let closes = rest.matches(last).count();
            let opens = rest.matches(open).count();
            if closes > opens {
                end -= last.len_utf8();
                continue;
            }
        }
        break;
    }
    (url[..end].to_string(), &url[end..])
}

/// Push a text fragment, turning bare URLs into links and leaving the
/// surrounding prose untouched by URL styling.
///
/// Typographic substitution is applied to the non-URL segments only, so
/// a query string like `?a=1&b=2--3` is not mangled.
pub fn push_with_links(runs: &mut Vec<Run>, text: &str, base: Style, smart: bool) {
    if !text.contains("http") {
        let t = if smart {
            super::smart::convert(text)
        } else {
            text.to_string()
        };
        inline::push_text(runs, &t, base);
        return;
    }
    let mut last = 0usize;
    for m in BARE_URL.find_iter(text) {
        let (url, _tail) = trim_url_tail(m.as_str());
        if url.is_empty() {
            continue;
        }
        let m_start = m.start();
        let m_end = m_start + url.len();
        if m_start > last {
            let seg = &text[last..m_start];
            let seg = if smart {
                super::smart::convert(seg)
            } else {
                seg.to_string()
            };
            inline::push_text(runs, &seg, base);
        }
        runs.push(Run::new(url, theme::link_text()));
        last = m_end;
    }
    if last < text.len() {
        let seg = &text[last..];
        let seg = if smart {
            super::smart::convert(seg)
        } else {
            seg.to_string()
        };
        inline::push_text(runs, &seg, base);
    }
}

/// Whether `runs` already spell out `url`.
///
/// A self-describing link -- `[https://x.io](https://x.io)`, or a bare
/// autolink -- would otherwise print its destination twice.
pub fn text_is_url(runs: &[Run], url: &str) -> bool {
    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
    let text = text.trim();
    !text.is_empty() && text == display_url(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(runs: &[Run]) -> String {
        runs.iter().map(|r| r.text.as_str()).collect()
    }

    #[test]
    fn bare_urls_become_links() {
        let mut runs = Vec::new();
        push_with_links(
            &mut runs,
            "see https://example.com now",
            Style::default(),
            false,
        );
        assert!(
            runs.iter().any(|r| r.style.underline),
            "url should be styled"
        );
        assert!(texts(&runs).contains("https://example.com"));
    }

    #[test]
    fn surrounding_text_is_preserved() {
        let mut runs = Vec::new();
        push_with_links(&mut runs, "a https://x.io b", Style::default(), false);
        assert_eq!(texts(&runs), "a https://x.io b");
    }

    #[test]
    fn trailing_period_is_not_part_of_the_url() {
        let mut runs = Vec::new();
        push_with_links(
            &mut runs,
            "go to https://example.com.",
            Style::default(),
            false,
        );
        assert_eq!(texts(&runs), "go to https://example.com.");
        let linked: Vec<&Run> = runs.iter().filter(|r| r.style.underline).collect();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].text, "https://example.com");
    }

    #[test]
    fn balanced_brackets_stay_in_the_url() {
        let mut runs = Vec::new();
        push_with_links(
            &mut runs,
            "https://en.wikipedia.org/wiki/Foo_(bar)",
            Style::default(),
            false,
        );
        let linked: Vec<&Run> = runs.iter().filter(|r| r.style.underline).collect();
        assert_eq!(linked[0].text, "https://en.wikipedia.org/wiki/Foo_(bar)");
    }

    #[test]
    fn unbalanced_closer_is_dropped() {
        let mut runs = Vec::new();
        push_with_links(
            &mut runs,
            "(see https://example.com/a)",
            Style::default(),
            false,
        );
        let linked: Vec<&Run> = runs.iter().filter(|r| r.style.underline).collect();
        assert_eq!(linked[0].text, "https://example.com/a");
        assert!(texts(&runs).ends_with(')'));
    }

    #[test]
    fn http_is_not_required() {
        let mut runs = Vec::new();
        push_with_links(
            &mut runs,
            "no links in this sentence",
            Style::default(),
            false,
        );
        assert!(!runs.iter().any(|r| r.style.underline));
    }

    #[test]
    fn smart_punctuation_skips_the_url() {
        let mut runs = Vec::new();
        push_with_links(
            &mut runs,
            "see \"docs\" at https://example.com/a--b now",
            Style::default(),
            true,
        );
        let linked: Vec<&Run> = runs.iter().filter(|r| r.style.underline).collect();
        assert_eq!(linked[0].text, "https://example.com/a--b");
        // Prose around it still gets curly quotes.
        assert!(texts(&runs).contains('\u{201c}'), "{:?}", texts(&runs));
    }

    #[test]
    fn smart_punctuation_can_be_disabled() {
        let mut runs = Vec::new();
        push_with_links(&mut runs, "\"quoted\"", Style::default(), false);
        assert!(texts(&runs).contains('"'));
    }

    #[test]
    fn self_describing_links_do_not_duplicate() {
        assert!(text_is_url(
            &[Run::plain("https://example.com")],
            "https://example.com"
        ));
        assert!(!text_is_url(&[Run::plain("docs")], "https://example.com"));
        assert!(!text_is_url(&[], "https://example.com"));
    }
}
