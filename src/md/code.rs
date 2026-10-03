//! Fenced code blocks: the box chrome, in-box wrapping, and gutter.

use super::style::{Line, Run};
use super::theme;
use super::width::{display_width, expand_tabs};
use super::wrap;
use unicode_width::UnicodeWidthChar;

/// Box-drawing characters.
const TOP_LEFT: char = '\u{250c}'; // ┌
const TOP_RIGHT: char = '\u{2510}'; // ┐
const BOTTOM_LEFT: char = '\u{2514}'; // └
const BOTTOM_RIGHT: char = '\u{2518}'; // ┘
const TEE_DOWN: char = '\u{2534}'; // ┴
const HORIZONTAL: char = '\u{2500}'; // ─
const VERTICAL: char = '\u{2502}'; // │
const VERTICAL_WRAP: char = '\u{250a}'; // ┊

/// Box chrome for a code block.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// Total width of the box, including both outer borders.
    pub total_width: usize,
    /// Columns consumed by the gutter, which holds line numbers.
    pub gutter_width: usize,
    /// Columns available for code, excluding gutter and borders.
    pub content_width: usize,
    /// Render line numbers in the gutter.
    pub line_numbers: bool,
}

/// Columns of horizontal space a code cell reserves for its own padding.
const CELL_PADDING: usize = 2;

/// Label shown in the top border. An untagged fence reads `text`.
pub fn label_for(lang: &str) -> String {
    let lang = lang.trim();
    if lang.is_empty() {
        "text".to_string()
    } else {
        lang.to_string()
    }
}

/// Compute the box geometry for a code block.
///
/// The box shrinks to fit its content but never below 44 columns of
/// interior, because a narrower box wraps ordinary code into unreadable
/// stubs. It is always capped to the available width.
pub fn frame_for(
    label: &str,
    max_text_width: usize,
    line_count: usize,
    line_numbers: bool,
    render_width: usize,
) -> Frame {
    let max_inner = render_width
        .saturating_sub(2)
        .max(display_width(label) + 3)
        .max(2);
    // The gutter must be wide enough for the largest line number.
    let digits = line_count.max(1).to_string().len();
    let gutter_width = if line_numbers { digits + 2 } else { 1 };

    // `content_width` is `inner - gutter_width`, so the interior has to
    // cover the content, its padding, *and* the gutter -- otherwise the
    // widest line wraps even though the box was sized for it.
    let min_inner = (display_width(label) + 3).max(44).min(max_inner);
    let needed = max_text_width + CELL_PADDING + gutter_width;
    let inner = needed.max(min_inner).min(max_inner);

    Frame {
        total_width: inner + 2,
        gutter_width,
        content_width: inner.saturating_sub(gutter_width).max(1),
        line_numbers,
    }
}

/// The `┌─ label ─────┐` top border.
pub fn top_border(label: &str, frame: &Frame) -> Line {
    let inner = frame.total_width.saturating_sub(2);
    let label_with_space = format!("{label} ");
    let head = display_width(&label_with_space) + 2; // "┌─ " plus label
    let bar = inner.saturating_sub(head).max(0);
    let mut runs = vec![
        Run::new(format!("{TOP_LEFT}{HORIZONTAL} "), theme::code_frame()),
        Run::new(label_with_space, theme::code_label()),
        Run::new(
            format!("{}{TOP_RIGHT}", HORIZONTAL.to_string().repeat(bar)),
            theme::code_frame(),
        ),
    ];
    // When the box is too narrow for the label the header would overflow;
    // drop the frame run that cannot fit rather than misalign the box.
    runs.retain(|r| r.style != theme::code_label() || display_width(&r.text) + 2 <= inner);
    Line::from_runs(runs)
}

/// The `└────┴────┘` bottom border.
pub fn bottom_border(frame: &Frame) -> Line {
    let inner = frame.total_width.saturating_sub(2);
    if frame.line_numbers && frame.gutter_width > 2 {
        let left = frame.gutter_width - 2;
        let right = inner.saturating_sub(frame.gutter_width - 1);
        Line::from_runs(vec![Run::new(
            format!(
                "{BOTTOM_LEFT}{}{TEE_DOWN}{}{BOTTOM_RIGHT}",
                HORIZONTAL.to_string().repeat(left),
                HORIZONTAL.to_string().repeat(right)
            ),
            theme::code_frame(),
        )])
    } else {
        Line::from_runs(vec![Run::new(
            format!(
                "{BOTTOM_LEFT}{}{BOTTOM_RIGHT}",
                HORIZONTAL.to_string().repeat(inner)
            ),
            theme::code_frame(),
        )])
    }
}

/// The leading `│n│ ` of a code row, plus the `│` that continues on
/// wrapped rows (`┊`).
fn row_gutter(frame: &Frame, line_no: Option<usize>, wrapped: bool) -> Vec<Run> {
    let mut runs = Vec::new();
    if wrapped {
        // A wrapped row is not a new line, so its number column is blank
        // and the inner rule is the lighter dashed variant.
        runs.push(Run::new(VERTICAL_WRAP, theme::code_gutter()));
        if frame.line_numbers {
            let digits = frame.gutter_width - 2;
            runs.push(Run::plain(" ".repeat(digits)));
            runs.push(Run::new(VERTICAL_WRAP, theme::code_gutter()));
        }
        runs.push(Run::new(" ", theme::code_gutter()));
        return runs;
    }
    runs.push(Run::new(VERTICAL, theme::code_frame()));
    if frame.line_numbers {
        let digits = frame.gutter_width - 2;
        let label = line_no.map(|n| n.to_string()).unwrap_or_default();
        let pad = digits.saturating_sub(display_width(&label));
        runs.push(Run::plain(" ".repeat(pad)));
        runs.push(Run::new(label, theme::code_line_number()));
        runs.push(Run::new(VERTICAL, theme::code_gutter()));
    }
    runs.push(Run::new(" ", theme::code_gutter()));
    runs
}

/// Render the full box for one code block.
///
/// `lines` are the styled code lines, already tab-expanded. `prefix` is
/// inherited from the enclosing context (a list item, a blockquote).
pub fn render(
    code: &str,
    lang: &str,
    render_width: usize,
    line_numbers: bool,
    prefix: &[Run],
) -> Vec<Line> {
    let label = label_for(lang);
    // Expand tabs here rather than trusting the caller: tab stops are
    // relative to the code column, and a literal tab would misalign
    // every row against the line-number gutter.
    let code = &expand_tabs(code, 0);
    let highlighted = super::highlight::highlight(code, &label);

    // Widest *line*, not widest run: highlighting splits a line into
    // short tokens, so the widest token is much narrower than the line
    // containing it and would size the box far too small.
    let max_text_width = highlighted
        .iter()
        .map(|l| l.iter().map(|r| r.width()).sum::<usize>())
        .max()
        .unwrap_or(0);
    let frame = frame_for(
        &label,
        max_text_width,
        highlighted.len(),
        line_numbers,
        render_width,
    );

    let mut out: Vec<Line> = Vec::new();

    let mut header = top_border(&label, &frame);
    prepend_prefix(&mut header, prefix);
    out.push(header);

    for (idx, line_runs) in highlighted.iter().enumerate() {
        let rows = wrap::hard_wrap(line_runs, frame.content_width);
        for (row_idx, row) in rows.iter().enumerate() {
            let mut runs = prefix.to_vec();
            runs.extend(row_gutter(
                &frame,
                if row_idx == 0 { Some(idx + 1) } else { None },
                row_idx > 0,
            ));
            let mut row = row.clone();
            wrap::pad_row_to_width(&mut row, frame.content_width);
            runs.extend(row);
            runs.push(Run::new(VERTICAL, theme::code_frame()));
            out.push(Line::from_runs(runs));
        }
    }

    let mut footer = bottom_border(&frame);
    prepend_prefix(&mut footer, prefix);
    out.push(footer);
    out
}

/// Box already-rendered lines under a label, with no line-number
/// gutter.
///
/// Used for diagrams: they are drawn rather than highlighted, but still
/// want the same frame as a code block so a fenced diagram is visually
/// part of the same family.
pub fn truncate_row(runs: &[Run], max_width: usize) -> Vec<Run> {
    if wrap::runs_width(runs) <= max_width {
        return runs.to_vec();
    }
    // Reserve the last column for the marker. Budgeting by width rather
    // than by run boundary matters because diagram rows are emitted one
    // run per character, so a boundary-wise cut would never mark itself.
    let limit = max_width.saturating_sub(1);
    let mut out: Vec<Run> = Vec::new();
    let mut used = 0usize;
    'fill: for r in runs {
        for ch in r.text.chars() {
            let w = ch.width().unwrap_or(0);
            if used + w > limit {
                break 'fill;
            }
            match out.last_mut() {
                Some(last) if last.style == r.style => last.text.push(ch),
                _ => out.push(Run::new(ch.to_string(), r.style)),
            }
            used += w;
        }
    }
    let style = runs.first().map(|r| r.style).unwrap_or_default();
    out.push(Run::new("\u{2026}".to_string(), style));
    out
}

pub fn box_lines(label: &str, lines: Vec<Line>, available: usize) -> Vec<Line> {
    let widest = lines
        .iter()
        .map(|l| l.runs.iter().map(|r| r.width()).sum::<usize>())
        .max()
        .unwrap_or(0);
    let frame = frame_for(label, widest, lines.len().max(1), false, available);

    let mut out = vec![top_border(label, &frame)];
    for line in &lines {
        // A diagram wider than the terminal is clipped rather than
        // allowed to run past the frame; the ellipsis says so instead of
        // letting the right border drift out of alignment.
        let mut row = line.clone();
        row.runs = truncate_row(&row.runs, frame.content_width);
        wrap::pad_row_to_width(&mut row.runs, frame.content_width);
        let mut runs = row_gutter(&frame, None, false);
        runs.extend(row.runs);
        runs.push(Run::new(VERTICAL.to_string(), theme::code_frame()));
        out.push(Line::from_runs(runs));
    }
    out.push(bottom_border(&frame));
    out
}

fn prepend_prefix(line: &mut Line, prefix: &[Run]) {
    if prefix.is_empty() {
        return;
    }
    let mut runs = prefix.to_vec();
    runs.append(&mut line.runs);
    line.runs = runs;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[Line]) -> Vec<String> {
        lines.iter().map(|l| l.to_plain_text()).collect()
    }

    fn all(texts: &[String]) -> String {
        texts.join("\n")
    }

    #[test]
    fn untagged_fence_is_labelled_text() {
        assert_eq!(label_for(""), "text");
        assert_eq!(label_for("  "), "text");
        assert_eq!(label_for("rust"), "rust");
    }

    #[test]
    fn box_is_a_closed_rectangle() {
        let out = render("x\n", "", 80, true, &[]);
        let t = texts(&out);
        assert!(t[0].starts_with('\u{250c}'), "top border: {:?}", t[0]);
        assert!(t[0].ends_with('\u{2510}'), "top border: {:?}", t[0]);
        assert!(t.last().unwrap().starts_with('\u{2514}'));
        assert!(t.last().unwrap().ends_with('\u{2518}'));
    }

    #[test]
    fn every_box_line_is_the_same_width() {
        let out = render("let x = 1;\nfn f() {}\n", "rust", 80, true, &[]);
        let widths: Vec<usize> = out.iter().map(|l| l.width()).collect();
        assert!(
            widths.windows(2).all(|w| w[0] == w[1]),
            "ragged box: {widths:?}"
        );
    }

    #[test]
    fn box_never_exceeds_the_render_width() {
        for w in [20usize, 40, 60, 80, 120] {
            let out = render("some code here\nand more\n", "rust", w, true, &[]);
            for l in &out {
                assert!(
                    l.width() <= w,
                    "width {w} exceeded by {:?}",
                    l.to_plain_text()
                );
            }
        }
    }

    #[test]
    fn line_numbers_appear_when_enabled() {
        let out = render("a\nb\nc\n", "", 80, true, &[]);
        let t = all(&texts(&out));
        assert!(t.contains("\u{2502}1\u{2502}"), "got:\n{t}");
        assert!(t.contains("\u{2502}2\u{2502}"));
        assert!(t.contains("\u{2502}3\u{2502}"));
    }

    #[test]
    fn line_numbers_are_absent_when_disabled() {
        let out = render("a\nb\n", "", 80, false, &[]);
        let t = all(&texts(&out));
        assert!(!t.contains("\u{2502}1\u{2502}"), "got:\n{t}");
    }

    #[test]
    fn long_line_wraps_with_a_dashed_gutter() {
        let long = "x".repeat(200);
        let out = render(&long, "", 60, true, &[]);
        let t = all(&texts(&out));
        assert!(t.contains('\u{250a}'), "wrap marker missing:\n{t}");
        for l in &out {
            assert!(l.width() <= 60);
        }
    }

    #[test]
    fn content_is_preserved_exactly() {
        let code = "fn main() {\n    println!(\"hi\");\n}\n";
        let out = render(code, "rust", 80, false, &[]);
        let t = all(&texts(&out));
        for line in code.trim_end().lines() {
            assert!(t.contains(line), "missing {line:?} in:\n{t}");
        }
    }

    #[test]
    fn tabs_are_expanded_to_stops() {
        let out = render("\tx", "", 80, false, &[]);
        let t = all(&texts(&out));
        assert!(t.contains("    x"), "tab not expanded:\n{t}");
    }

    #[test]
    fn empty_code_still_draws_a_box() {
        let out = render("", "", 80, true, &[]);
        assert_eq!(out.len(), 2, "top and bottom only");
    }

    #[test]
    fn prefix_is_applied_to_every_row() {
        let out = render("a\n", "", 80, false, &[Run::plain("  ")]);
        for l in &out {
            assert!(
                l.to_plain_text().starts_with("  "),
                "{:?}",
                l.to_plain_text()
            );
        }
    }
}
