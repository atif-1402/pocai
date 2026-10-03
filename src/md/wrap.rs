//! Line wrapping.
//!
//! Prose wraps on whitespace; a single token longer than the available
//! width is hard-broken at grapheme cluster boundaries rather than being
//! truncated, so a long URL or hash stays readable and an emoji is never
//! torn apart.
//!
//! Wrapping is prefix-aware: blockquotes and list items supply a prefix
//! for the first line and a (usually different) one for continuations, so
//! wrapped text stays aligned under its own block instead of floating to
//! column zero.

use super::style::{Line, Run};
use super::width::{display_width, iter_cluster_widths};

/// Total display width of a run sequence.
pub fn runs_width(runs: &[Run]) -> usize {
    runs.iter().map(|r| r.width()).sum()
}

/// A fragment of text plus the style it inherited from its source run.
struct Token {
    text: String,
    style: super::style::Style,
    is_space: bool,
    width: usize,
}

/// Split runs into alternating runs of whitespace and non-whitespace.
///
/// Runs are split on *characters* but a token never crosses a style
/// boundary, so a styled span keeps its styling after being wrapped.
fn tokenize(runs: &[Run]) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    for run in runs {
        let mut buf = String::new();
        let mut is_space: Option<bool> = None;
        for ch in run.text.chars() {
            let this_is_space = ch.is_whitespace();
            match is_space {
                Some(prev) if prev == this_is_space => buf.push(ch),
                Some(_) => {
                    tokens.push(Token {
                        width: display_width(&buf),
                        text: std::mem::take(&mut buf),
                        style: run.style,
                        is_space: is_space.unwrap_or(false),
                    });
                    buf.push(ch);
                    is_space = Some(this_is_space);
                }
                None => {
                    buf.push(ch);
                    is_space = Some(this_is_space);
                }
            }
        }
        if !buf.is_empty() {
            tokens.push(Token {
                width: display_width(&buf),
                text: buf,
                style: run.style,
                is_space: is_space.unwrap_or(false),
            });
        }
    }
    tokens
}

fn emit(out: &mut Vec<Line>, cur: &mut Vec<Run>, first: &[Run], cont: &[Run]) {
    // Defensive: no line should ever end in unstyled padding.
    while cur
        .last()
        .map(|r| {
            r.text.chars().all(char::is_whitespace) && r.style.bg.is_none() && !r.style.reverse
        })
        .unwrap_or(false)
    {
        cur.pop();
    }
    let prefix = if out.is_empty() { first } else { cont };
    let mut line = Line::from_runs(prefix.to_vec());
    line.runs.append(cur);
    out.push(line);
}

/// Wrap styled runs to `render_width`, prefixing the first line with
/// `first_prefix` and every later line with `cont_prefix`.
pub fn wrap(
    runs: &[Run],
    first_prefix: &[Run],
    cont_prefix: &[Run],
    render_width: usize,
) -> Vec<Line> {
    let first_w = runs_width(first_prefix);
    let cont_w = runs_width(cont_prefix);
    // Never let the floor push the content past the line it must fit on:
    // a deep prefix can leave only a column or two, and that is still the
    // correct width to wrap to.
    let max_width = render_width
        .saturating_sub(first_w.max(cont_w))
        .clamp(1, render_width.max(1));

    let mut out: Vec<Line> = Vec::new();
    let mut cur: Vec<Run> = Vec::new();
    let mut cur_w = 0usize;
    let mut started = false;
    // Whitespace is held back until the following word is known to fit,
    // so a line never ends in a trailing space.
    let mut pending: Option<Run> = None;

    for tok in tokenize(runs) {
        if tok.is_space {
            if tok.style.bg.is_some() || tok.style.reverse {
                // A styled space is padding inside something like an
                // inline-code chip; dropping it would tear the chip.
                if cur_w + tok.width <= max_width {
                    cur.push(Run::new(tok.text, tok.style));
                    cur_w += tok.width;
                }
            } else if started {
                pending = Some(Run::new(tok.text, tok.style));
            }
            // A space before any content is a ragged indent, not text.
            continue;
        }

        let space_w = pending.as_ref().map_or(0, |s| s.width());
        if started && cur_w + space_w + tok.width > max_width {
            // Wrap here, discarding the space that would have joined the
            // word to the line being closed.
            pending = None;
            emit(&mut out, &mut cur, first_prefix, cont_prefix);
            cur_w = 0;
            started = false;
        }
        if let Some(space) = pending.take() {
            // Safe: the check above proved the space and the word both fit.
            cur_w += space.width();
            cur.push(space);
        }

        if cur_w + tok.width > max_width {
            // No line can hold this token, so break it at cluster
            // boundaries rather than letting it overflow or vanish.
            for (cluster, cw) in iter_cluster_widths(&tok.text) {
                if !cur.is_empty() && cur_w + cw > max_width {
                    emit(&mut out, &mut cur, first_prefix, cont_prefix);
                    cur_w = 0;
                }
                cur.push(Run::new(cluster, tok.style));
                cur_w += cw;
                started = true;
            }
            continue;
        }

        cur.push(Run::new(tok.text, tok.style));
        cur_w += tok.width;
        started = true;
    }

    // Any space still pending trails the last word and is dropped.
    if started || out.is_empty() {
        emit(&mut out, &mut cur, first_prefix, cont_prefix);
    }
    out
}

/// Hard-break a single run into rows of at most `max_width` columns.
///
/// This is code-block layout, not prose: no word boundaries, no space
/// handling, every row packed to exactly the given width. Adjacent
/// fragments with equal styles are merged back together so a syntax
/// highlighter's fragmented spans do not produce a redundant escape
/// sequence per character.
pub fn hard_wrap(runs: &[Run], max_width: usize) -> Vec<Vec<Run>> {
    let mut rows: Vec<Vec<Run>> = vec![Vec::new()];
    let mut row_width = 0usize;

    for run in runs {
        let mut pending = String::new();
        let mut pending_w = 0usize;
        let push_fragment =
            |rows: &mut Vec<Vec<Run>>, pending: &mut String, style: super::style::Style| {
                if pending.is_empty() {
                    return;
                }
                let text = std::mem::take(pending);
                if let Some(last) = rows.last_mut().and_then(|r| r.last_mut()) {
                    if last.style == style {
                        last.text.push_str(&text);
                        return;
                    }
                }
                rows.last_mut().unwrap().push(Run::new(text, style));
            };

        for (cluster, cw) in iter_cluster_widths(&run.text) {
            if row_width + cw > max_width && row_width > 0 {
                push_fragment(&mut rows, &mut pending, run.style);
                rows.push(Vec::new());
                row_width = 0;
            }
            pending.push_str(cluster);
            pending_w += cw;
            row_width += cw;
        }
        let _ = pending_w;
        push_fragment(&mut rows, &mut pending, run.style);
    }

    rows
}

/// Right-pad a row of runs out to exactly `width` columns with spaces.
pub fn pad_row_to_width(row: &mut Vec<Run>, width: usize) {
    let used = runs_width(row);
    if used < width {
        row.push(Run::plain(" ".repeat(width - used)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::md::style::{Color, Style};

    fn plain(s: &str) -> Vec<Run> {
        vec![Run::plain(s)]
    }

    fn texts(lines: &[Line]) -> Vec<String> {
        lines.iter().map(|l| l.to_plain_text()).collect()
    }

    #[test]
    fn short_text_stays_on_one_line() {
        let out = wrap(&plain("hello world"), &[], &[], 40);
        assert_eq!(texts(&out), vec!["hello world"]);
    }

    #[test]
    fn wraps_on_word_boundaries() {
        let out = wrap(&plain("aaa bbb ccc ddd"), &[], &[], 8);
        assert_eq!(texts(&out), vec!["aaa bbb", "ccc ddd"]);
    }

    #[test]
    fn no_line_exceeds_the_width() {
        let out = wrap(&plain("alpha beta gamma delta epsilon zeta"), &[], &[], 12);
        for l in &out {
            assert!(l.width() <= 12, "over-wide line: {:?}", l.to_plain_text());
        }
    }

    #[test]
    fn long_token_is_broken_not_dropped() {
        let out = wrap(&plain("aaaaaaaaaa"), &[], &[], 4);
        let joined: String = texts(&out).join("");
        assert_eq!(joined, "aaaaaaaaaa", "content must be preserved");
        for l in &out {
            assert!(l.width() <= 4);
        }
    }

    #[test]
    fn wide_chars_are_never_split_across_lines() {
        // Each ideograph is two columns; a width-5 line fits two per row.
        let out = wrap(
            &plain("\u{6771}\u{4eac}\u{4e2d}\u{6587}\u{6e2c}\u{8a66}"),
            &[],
            &[],
            5,
        );
        let joined: String = texts(&out).join("");
        assert_eq!(joined, "\u{6771}\u{4eac}\u{4e2d}\u{6587}\u{6e2c}\u{8a66}");
        for l in &out {
            assert!(l.width() <= 5, "{:?} exceeds 5", l.to_plain_text());
        }
    }

    #[test]
    fn prefixes_apply_to_first_and_continuation_lines() {
        let first = vec![Run::plain("> ")];
        let cont = vec![Run::plain("  ")];
        // Two columns of prefix leave seven for text, which is exactly
        // "aaa bbb"; the rendered lines are therefore nine wide.
        let out = wrap(&plain("aaa bbb ccc ddd"), &first, &cont, 9);
        assert_eq!(texts(&out), vec!["> aaa bbb", "  ccc ddd"]);
        for l in &out {
            assert_eq!(l.width(), 9, "{:?}", l.to_plain_text());
        }
    }

    #[test]
    fn wide_prefix_reduces_available_width() {
        let first = vec![Run::plain("> ")];
        let cont = vec![Run::plain("  ")];
        let out = wrap(&plain("aaa bbb ccc"), &first, &cont, 8);
        for l in &out {
            assert!(l.width() <= 8);
        }
    }

    #[test]
    fn empty_input_yields_one_prefixed_blank_line() {
        let first = vec![Run::plain("\u{258f} ")];
        let out = wrap(&[], &first, &[], 40);
        assert_eq!(texts(&out), vec!["\u{258f} "]);
    }

    #[test]
    fn styles_survive_wrapping() {
        let runs = vec![
            Run::plain("normal "),
            Run::new("bold", Style::new().fg(Color::Ansi(12)).bold()),
            Run::plain(" tail"),
        ];
        let out = wrap(&runs, &[], &[], 8);
        let flat: Vec<Run> = out.into_iter().flat_map(|l| l.runs).collect();
        assert!(
            flat.iter().any(|r| r.style.bold),
            "bold styling must be preserved across the wrap"
        );
    }

    #[test]
    fn hard_wrap_packs_rows_to_the_limit() {
        let rows = hard_wrap(&plain("abcdefghij"), 4);
        let got: Vec<String> = rows
            .iter()
            .map(|r| r.iter().map(|x| x.text.as_str()).collect::<String>())
            .collect();
        assert_eq!(got, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn hard_wrap_merges_adjacent_equal_styles() {
        let s = Style::new();
        let rows = hard_wrap(
            &[
                Run::new("ab", s),
                Run::new("cd", s),
                Run::new("ef", Style::new().bold()),
            ],
            10,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].len(), 2, "same-style runs should coalesce");
    }
}
