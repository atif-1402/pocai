//! Display-width helpers.
//!
//! Terminal columns are not byte offsets and not `char` counts: a CJK
//! ideograph occupies two columns, a combining accent occupies none, and
//! an emoji ZWJ sequence occupies one. Everything here is built on
//! extended grapheme clusters for that reason.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Column interval that tab stops land on.
pub const TAB_STOP: usize = 4;

/// Iterate `(cluster, display_width)` over a string.
///
/// Splitting on grapheme clusters rather than chars is what keeps a
/// wrapped line from tearing `👨‍👩‍👧` in half or orphaning a base
/// character from its variation selector.
pub fn iter_cluster_widths(text: &str) -> impl Iterator<Item = (&str, usize)> {
    text.graphemes(true).map(|g| (g, UnicodeWidthStr::width(g)))
}

/// Total display width of a string in terminal columns.
pub fn display_width(text: &str) -> usize {
    text.graphemes(true)
        .map(|g| UnicodeWidthStr::width(g))
        .sum()
}

/// Replace tabs with spaces advancing to the next [`TAB_STOP`].
///
/// `start_width` is the column the text starts at, so code inside an
/// indented context (a list item, a box) still lands on the right stops.
pub fn expand_tabs(text: &str, start_width: usize) -> String {
    let mut out = String::with_capacity(text.len());
    let mut width = start_width;
    for ch in text.chars() {
        if ch == '\t' {
            let advance = TAB_STOP - (width % TAB_STOP);
            for _ in 0..advance {
                out.push(' ');
            }
            width += advance;
        } else {
            out.push(ch);
            width += UnicodeWidthStr::width(ch.to_string().as_str());
        }
    }
    out
}

/// Clip `s` to `max` columns, marking the cut with an ellipsis.
///
/// Grapheme-aware, so a clipped run never leaves half a combining
/// sequence or a wrapped emoji in the output.
#[cfg_attr(not(test), allow(dead_code))]
pub fn truncate_to_width(s: &str, max: usize) -> String {
    if display_width(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0usize;
    for g in s.graphemes(true) {
        let gw = display_width(g);
        // Reserve one column for the ellipsis itself.
        if w + gw > max.saturating_sub(1) {
            break;
        }
        out.push_str(g);
        w += gw;
    }
    out.push('\u{2026}');
    out
}

/// Display width of `s` ignoring any ANSI escape sequences.
///
/// `display_width` is the right tool for measuring layout input, which
/// carries its styling as structured runs rather than escape bytes. This
/// is for measuring *output*, where the styling is already inlined.
#[cfg_attr(not(test), allow(dead_code))]
pub fn visible_width(s: &str) -> usize {
    let mut w = 0usize;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip a CSI sequence, up to and including its final byte.
            if chars.peek() == Some(&'[') {
                chars.next();
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        w += unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_is_double_width() {
        assert_eq!(display_width("\u{6771}\u{4eac}"), 4);
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width(""), 0);
    }

    #[test]
    fn cluster_widths_sum_matches_display_width() {
        for text in [
            "hello",
            "\u{6771}\u{4eac}",
            "a\u{1f1ee}\u{1f1f3}b",
            "e\u{301}",
            "  x  ",
        ] {
            let sum: usize = iter_cluster_widths(text).map(|(_, w)| w).sum();
            assert_eq!(sum, display_width(text), "mismatch for {text:?}");
        }
    }

    #[test]
    fn truncation_reserves_ellipsis_column() {
        assert_eq!(truncate_to_width("abcdef", 4), "abc\u{2026}");
        assert_eq!(display_width(&truncate_to_width("abcdef", 4)), 4);
        assert_eq!(truncate_to_width("abc", 10), "abc");
    }

    #[test]
    fn tabs_expand_to_stops() {
        assert_eq!(expand_tabs("a\tb", 0), "a   b");
        assert_eq!(expand_tabs("a\tb", 1), "a  b");
    }
}
