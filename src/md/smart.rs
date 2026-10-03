//! Typographic substitutions: curly quotes, dashes, ellipsis.
//!
//! Applied only to prose. Code spans, code blocks, and URLs bypass it,
//! so `--` inside a flag or a URL is left alone.

/// True when a quote in this position should open rather than close.
fn opens_quote(prev: Option<char>) -> bool {
    match prev {
        None => true,
        Some(c) => {
            c.is_whitespace()
                || matches!(
                    c,
                    '(' | '['
                        | '{'
                        | '<'
                        | '\u{2014}'
                        | '\u{2013}'
                        | '\u{201c}'
                        | '\u{2018}'
                        | ':'
                        | ';'
                )
        }
    }
}

/// Convert straight punctuation to typographic equivalents.
pub fn convert(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev: Option<char> = None;
    let mut i = 0usize;

    while i < text.len() {
        let rest = &text[i..];
        if rest.starts_with("---") {
            out.push('\u{2014}');
            i += 3;
            prev = Some('\u{2014}');
            continue;
        }
        if rest.starts_with("--") {
            out.push('\u{2013}');
            i += 2;
            prev = Some('\u{2013}');
            continue;
        }
        if rest.starts_with("...") {
            out.push('\u{2026}');
            i += 3;
            prev = Some('\u{2026}');
            continue;
        }
        let ch = rest.chars().next().expect("non-empty slice");
        i += ch.len_utf8();
        match ch {
            '"' => {
                out.push(if opens_quote(prev) {
                    '\u{201c}'
                } else {
                    '\u{201d}'
                });
                prev = Some('"');
            }
            '\'' => {
                out.push(if opens_quote(prev) {
                    '\u{2018}'
                } else {
                    '\u{2019}'
                });
                prev = Some('\'');
            }
            _ => {
                out.push(ch);
                prev = Some(ch);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn em_dash_for_triple_hyphen() {
        assert_eq!(convert("a---b"), "a\u{2014}b");
    }

    #[test]
    fn en_dash_for_double_hyphen() {
        assert_eq!(convert("a--b"), "a\u{2013}b");
    }

    #[test]
    fn ellipsis() {
        assert_eq!(convert("wait..."), "wait\u{2026}");
    }

    #[test]
    fn double_quotes_curl_both_ways() {
        assert_eq!(convert("\"hi\""), "\u{201c}hi\u{201d}");
    }

    #[test]
    fn apostrophe_stays_a_closing_quote() {
        assert_eq!(convert("don't"), "don\u{2019}t");
    }

    #[test]
    fn single_quotes_curl_both_ways() {
        assert_eq!(convert("'hi'"), "\u{2018}hi\u{2019}");
    }

    #[test]
    fn quote_after_opening_paren_opens() {
        assert_eq!(convert("(\"hi\")"), "(\u{201c}hi\u{201d})");
    }

    #[test]
    fn plain_text_is_untouched() {
        let s = "nothing to change here";
        assert_eq!(convert(s), s);
    }

    #[test]
    fn empty_input_is_empty() {
        assert_eq!(convert(""), "");
    }

    #[test]
    fn multi_byte_text_survives() {
        assert_eq!(
            convert("caf\u{e9} -- th\u{e9}"),
            "caf\u{e9} \u{2013} th\u{e9}"
        );
    }

    #[test]
    fn conversion_is_idempotent_for_unicode_output() {
        let once = convert("\"a\" -- b...");
        assert_eq!(convert(&once), once);
    }
}
