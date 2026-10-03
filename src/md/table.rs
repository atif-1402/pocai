//! Tables: column sizing, cell wrapping, alignment, and box chrome.

use super::style::{Line, Run, Style};
use super::theme;
use super::wrap;

/// Box-drawing characters.
///
/// Stroke weight has to stay consistent within one rule. The header
/// rule is double throughout (`╞ ═ ╪ ╡`) and the body rules are light
/// throughout (`├ ─ ┼ ┤`); pairing a double cap or a heavy cross with a
/// light fill leaves the junctions standing proud of the line they sit
/// in, which is what made the edges look broken.
const TL: &str = "\u{250c}"; // ┌
const TR: &str = "\u{2510}"; // ┐
const BL: &str = "\u{2514}"; // └
const BR: &str = "\u{2518}"; // ┘
const H: &str = "\u{2500}"; // ─
const HD: &str = "\u{2550}"; // ═
const V: &str = "\u{2502}"; // │
const T_DOWN: &str = "\u{252c}"; // ┬
const T_UP: &str = "\u{2534}"; // ┴
const T_LEFT: &str = "\u{251c}"; // ├
const T_RIGHT: &str = "\u{2524}"; // ┤
const T_X: &str = "\u{253c}"; // ┼
const TD_X: &str = "\u{256a}"; // ╪
const TD_L: &str = "\u{255e}"; // ╞
const TD_R: &str = "\u{2561}"; // ╡

/// Narrowest a column may become before its text is hard-wrapped.
const MIN_COLUMN: usize = 3;
/// Columns of horizontal space a cell reserves for its own padding.
const CELL_PADDING: usize = 2;

/// A cell's text plus the alignment from the delimiter row.
#[derive(Clone, Debug)]
pub struct Cell {
    pub runs: Vec<Run>,
    pub align: Align,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Align {
    #[default]
    None,
    Left,
    Center,
    Right,
}

/// A parsed table: a header row plus body rows.
#[derive(Clone, Debug, Default)]
pub struct Table {
    pub header: Vec<Cell>,
    pub rows: Vec<Vec<Cell>>,
}

impl Table {
    fn column_count(&self) -> usize {
        let from_header = self.header.len();
        let from_body = self.rows.iter().map(|r| r.len()).max().unwrap_or(0);
        from_header.max(from_body)
    }
}

/// One horizontal rule of `widths` columns wide, joined by `mid`.
/// One horizontal rule of `widths` columns wide, joined by `mid`.
///
/// `fill` is separate from the junctions so a rule can keep one stroke
/// weight end to end.
fn rule(widths: &[usize], left: &str, mid: &str, right: &str, fill: &str, style: Style) -> Line {
    let mut runs = vec![Run::new(left.to_string(), style)];
    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            runs.push(Run::new(mid.to_string(), style));
        }
        runs.push(Run::new(fill.repeat(*w), style));
    }
    runs.push(Run::new(right.to_string(), style));
    Line::from_runs(runs)
}

/// Size every column so the table fits `available` columns.
///
/// Columns start at their natural content width; when the total
/// overflows, the widest columns give up space first, repeatedly, so
/// narrow columns keep their content and the table degrades evenly.
fn size_columns(table: &Table, available: usize) -> Vec<usize> {
    let n = table.column_count();
    if n == 0 {
        return Vec::new();
    }
    let mut widths: Vec<usize> = vec![0; n];
    let consider = |cells: &[Cell], widths: &mut Vec<usize>| {
        for (i, cell) in cells.iter().enumerate() {
            if i >= widths.len() {
                break;
            }
            let w = wrap::runs_width(&cell.runs) + CELL_PADDING;
            if w > widths[i] {
                widths[i] = w;
            }
        }
    };
    consider(&table.header, &mut widths);
    for row in &table.rows {
        consider(row, &mut widths);
    }

    let separators = n + 1; // left edge, n-1 internals, right edge
    let total = |w: &[usize]| w.iter().sum::<usize>() + separators;

    if total(&widths) <= available {
        return widths;
    }

    // A header is a word or two, and a column narrower than its own
    // header splits it mid-word ("Descriptio" / "n"), which reads as a
    // broken table rather than a tight one. Columns defend their header
    // width first and only give it up once nothing else can move.
    let floors: Vec<usize> = table
        .header
        .iter()
        .map(|c| (wrap::runs_width(&c.runs) + CELL_PADDING).max(MIN_COLUMN))
        .collect();

    // Shrink the widest column by one column until the table fits or
    // every column is at its minimum.
    loop {
        if total(&widths) <= available {
            break;
        }
        // First pass: only columns with slack above their header width.
        // Second pass: anything still above the absolute minimum, so a
        // table of long headers can still be forced to fit.
        let pick = |widths: &[usize], floor: Option<&[usize]>| {
            widths
                .iter()
                .enumerate()
                .filter(|(i, w)| {
                    let min = floor.map_or(MIN_COLUMN, |f| f[*i].max(MIN_COLUMN));
                    **w > min
                })
                .max_by_key(|(_, w)| **w)
                .map(|(i, _)| i)
        };
        let Some(idx) = pick(&widths, Some(&floors)).or_else(|| pick(&widths, None)) else {
            break;
        };
        widths[idx] -= 1;
    }
    widths
}

/// Lay a cell's runs into `width` columns, one styled run list per row.
fn layout_cell(cell: &Cell, width: usize) -> Vec<Vec<Run>> {
    let lines = wrap::wrap(&cell.runs, &[], &[], width.max(1));
    lines
        .into_iter()
        .map(|l| {
            let mut runs = l.runs;
            // Alignment must consume the slack *before* the row is padded
            // out, or there would be nothing left to distribute.
            align_runs(&mut runs, width, cell.align);
            wrap::pad_row_to_width(&mut runs, width);
            runs
        })
        .collect()
}

/// Pad a padded-width row to the column width per its alignment.
fn align_runs(runs: &mut Vec<Run>, width: usize, align: Align) {
    let used = wrap::runs_width(runs);
    if used >= width {
        return;
    }
    let slack = width - used;
    let (left, right) = match align {
        Align::Left | Align::None => (0, slack),
        Align::Right => (slack, 0),
        Align::Center => (slack / 2, slack - slack / 2),
    };
    if left > 0 {
        runs.insert(0, Run::plain(" ".repeat(left)));
    }
    if right > 0 {
        runs.push(Run::plain(" ".repeat(right)));
    }
}

fn header_style_for(runs: &[Run]) -> Style {
    runs.iter()
        .fold(theme::table_header(), |acc, r| acc.merged_over(r.style))
}

/// Render the whole table, or nothing when it is empty.
pub fn render(table: &Table, available: usize) -> Vec<Line> {
    if table.column_count() == 0 {
        return Vec::new();
    }
    let widths = size_columns(table, available);
    if widths.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<Line> = Vec::new();

    // Normalize every row to the same cell count so the rules line up.
    let pad_row = |cells: &[Cell], align_default: Align| -> Vec<Cell> {
        (0..widths.len())
            .map(|i| {
                cells.get(i).cloned().unwrap_or(Cell {
                    runs: Vec::new(),
                    align: align_default,
                })
            })
            .collect()
    };

    let header = pad_row(&table.header, Align::Left);
    let rows: Vec<Vec<Cell>> = table.rows.iter().map(|r| pad_row(r, Align::None)).collect();

    let top: Vec<usize> = widths.clone();
    out.push(rule(&top, TL, T_DOWN, TR, H, theme::table_border()));

    if !header.iter().all(|c| c.runs.is_empty()) {
        out.extend(render_row(&header, &widths, true));
        let sep: Vec<usize> = widths.clone();
        out.push(rule(&sep, TD_L, TD_X, TD_R, HD, theme::table_separator()));
    }

    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            let mid: Vec<usize> = widths.clone();
            out.push(rule(&mid, T_LEFT, T_X, T_RIGHT, H, theme::table_border()));
        }
        out.extend(render_row(row, &widths, false));
    }

    let bottom: Vec<usize> = widths.clone();
    out.push(rule(&bottom, BL, T_UP, BR, H, theme::table_border()));
    out
}

/// One physical table row, which may span several lines if a cell wraps.
fn render_row(cells: &[Cell], widths: &[usize], is_header: bool) -> Vec<Line> {
    let inner_widths: Vec<usize> = widths
        .iter()
        .map(|w| w.saturating_sub(CELL_PADDING).max(1))
        .collect();
    let laid: Vec<Vec<Vec<Run>>> = cells
        .iter()
        .zip(inner_widths.iter())
        .map(|(c, w)| layout_cell(c, *w))
        .collect();
    let height = laid.iter().map(|c| c.len().max(1)).max().unwrap_or(1);

    let mut out = Vec::with_capacity(height);
    for row_idx in 0..height {
        let mut runs = vec![Run::new(V.to_string(), theme::table_border())];
        for (col_idx, w) in inner_widths.iter().enumerate() {
            let mut cell_runs = laid[col_idx]
                .get(row_idx)
                .cloned()
                .unwrap_or_else(|| vec![Run::plain(" ".repeat(*w))]);
            if cell_runs.is_empty() {
                cell_runs.push(Run::plain(" ".repeat(*w)));
            }
            wrap::pad_row_to_width(&mut cell_runs, *w);
            let mut styled: Vec<Run> = Vec::with_capacity(cell_runs.len() + 2);
            styled.push(Run::plain(" "));
            for r in cell_runs {
                // Alignment padding is whitespace-only; styling it would
                // tint the cell's right edge, which reads as a stray
                // block of colour on a light background.
                if r.text.chars().all(char::is_whitespace) {
                    styled.push(Run::plain(r.text));
                    continue;
                }
                let base = if is_header {
                    header_style_for(&[r.clone()])
                } else {
                    theme::table_cell()
                };
                styled.push(Run::new(r.text, base.merged_over(r.style)));
            }
            styled.push(Run::plain(" "));
            runs.extend(styled);
            runs.push(Run::new(V.to_string(), theme::table_border()));
        }
        out.push(Line::from_runs(runs));
    }
    out
}

#[cfg(test)]
mod tests {
    /// A cell holding unstyled text.
    fn plain_cell(text: &str, align: Align) -> Cell {
        Cell {
            runs: vec![Run::plain(text)],
            align,
        }
    }

    use super::*;

    fn texts(lines: &[Line]) -> Vec<String> {
        lines.iter().map(|l| l.to_plain_text()).collect()
    }

    fn simple() -> Table {
        Table {
            header: vec![plain_cell("A", Align::None), plain_cell("B", Align::None)],
            rows: vec![vec![
                plain_cell("1", Align::None),
                plain_cell("2", Align::None),
            ]],
        }
    }

    #[test]
    fn draws_a_closed_box() {
        let out = render(&simple(), 80);
        let t = texts(&out);
        assert!(t[0].starts_with('\u{250c}') && t[0].ends_with('\u{2510}'));
        assert!(t.last().unwrap().starts_with('\u{2514}'));
        assert!(t.last().unwrap().ends_with('\u{2518}'));
    }

    #[test]
    fn header_gets_a_double_rule() {
        let out = render(&simple(), 80);
        let t = texts(&out);
        let sep = &t[2];
        assert!(sep.contains('\u{255e}'), "expected ╞ in {sep:?}");
        assert!(sep.contains('\u{256a}'), "expected ╪ in {sep:?}");
        assert!(sep.contains('\u{2561}'), "expected ╡ in {sep:?}");
    }

    /// The two ends of the header rule are mirrors of each other.
    ///
    /// `╞` (U+255E) and `╡` (U+2561) cap a horizontal `═` run against a
    /// light `│`. `╢` (U+2562) is the other axis entirely: it caps a
    /// *vertical* double rule turning *horizontal* single, so using it
    /// here drew a heavy vertical stub on the right that clashed with
    /// the light dividers above and below it while the left end stayed
    /// clean. The glyph had a comment reading `╡`, so it survived review
    /// and a test whose assertion *message* said `╡` while checking
    /// U+2562 -- pin the codepoints so neither can drift again.
    #[test]
    fn header_rule_ends_match() {
        let t = texts(&render(&simple(), 80));
        let sep = &t[2];
        assert!(
            sep.starts_with('\u{255e}'),
            "left cap should be ╞, got {sep:?}"
        );
        assert!(
            sep.ends_with('\u{2561}'),
            "right cap should be ╡, got {sep:?}"
        );
        assert!(
            !sep.contains('\u{2562}'),
            "U+2562 (╢) caps a vertical double rule and must not appear here"
        );
    }
    /// The outer corners and rules are the light family, the header rule
    /// the double one -- and nothing borrows from the wrong family.
    #[test]
    fn no_glyph_is_taken_from_the_wrong_family() {
        const DOUBLE: &[char] = &['\u{255e}', '\u{256a}', '\u{2561}', '\u{2550}'];
        const LIGHT: &[char] = &[
            '\u{250c}', '\u{252c}', '\u{2510}', '\u{2502}', '\u{251c}', '\u{253c}', '\u{2524}',
            '\u{2514}', '\u{2534}', '\u{2518}', '\u{2500}',
        ];
        // U+2562/╢ is the trap: plausible-looking, wrong axis.
        const WRONG: &[char] = &['\u{2562}', '\u{256c}', '\u{255f}', '\u{2560}'];

        for (i, line) in texts(&render(&simple(), 80)).iter().enumerate() {
            // Cell text is not a border glyph, so it falls out of every
            // list below and is simply not considered.
            let border: Vec<char> = line
                .chars()
                .filter(|c| DOUBLE.contains(c) || LIGHT.contains(c) || WRONG.contains(c))
                .collect();
            assert!(!border.is_empty(), "row {i} has no border glyphs");

            for ch in &border {
                assert!(
                    !WRONG.contains(ch),
                    "row {i} uses {ch:?} (U+{:04X}), wrong family",
                    *ch as u32
                );
                if i == 2 {
                    assert!(DOUBLE.contains(ch), "header rule has {ch:?} in {line:?}");
                } else {
                    assert!(LIGHT.contains(ch), "row {i} has {ch:?} in {line:?}");
                }
            }
        }
    }

    /// Every border in a table is one color.
    ///
    /// The frame, the cell dividers, the body rules, and the `╞══╪╡`
    /// header rule all share a single color; the rules are told apart by
    /// their glyphs, not by their palette. The header rule was once blue
    /// and bold, and then merely a lighter grey, and both times it was
    /// the one line in the grid that did not match the rest.
    #[test]
    fn every_border_shares_one_color() {
        let out = render(&simple(), 80);
        let sep = theme::table_separator();
        let border = theme::table_border();

        assert_eq!(sep, border, "the header rule must match the other borders");
        assert!(!sep.bold, "the header rule must not be bold");
        assert!(
            !matches!(sep.fg, Some(theme::BLUE) | Some(theme::BRIGHT_BLUE)),
            "the header rule must not be tinted; got {:?}",
            sep.fg
        );

        for run in &out[2].runs {
            assert_eq!(run.style, sep, "unexpected style on {run:?}");
        }
    }

    /// No rule anywhere in the table may carry a color the frame lacks.
    #[test]
    fn no_rule_is_tinted_or_bold() {
        let mut table = simple();
        table.rows.push(vec![
            plain_cell("second", Align::None),
            plain_cell("row", Align::None),
        ]);
        let border = theme::table_border();
        for (i, line) in render(&table, 80).iter().enumerate() {
            let is_text_row = i % 2 == 1;
            for run in &line.runs {
                if is_text_row {
                    continue; // header/body text legitimately carries its own style
                }
                assert_eq!(run.style, border, "rule row {i} has a styled run: {run:?}");
            }
        }
    }

    /// The accent belongs to the header text, not the chrome.
    #[test]
    fn header_text_carries_the_accent() {
        let out = render(&simple(), 80);
        let header = theme::table_header();
        assert!(header.bold, "header text should be bold");
        let styled: Vec<&Run> = out[1]
            .runs
            .iter()
            .filter(|r| r.style != theme::table_border() && !r.text.trim().is_empty())
            .collect();
        assert!(!styled.is_empty(), "header text should be styled");
        for run in styled {
            assert_eq!(run.style, header, "unexpected style on {run:?}");
        }
    }

    /// A rule must keep one stroke weight end to end.
    ///
    /// Pairing the double caps `╞`/`╡` with a light `─` fill, or hanging
    /// the double-vertical `╬` off a light line, leaves every junction
    /// standing proud of the run it sits in -- that mismatch is what made
    /// table edges look broken.
    #[test]
    fn a_rule_never_mixes_stroke_weights() {
        let mut table = simple();
        table.rows.push(vec![
            plain_cell("second", Align::None),
            plain_cell("row", Align::None),
        ]);
        let t = texts(&render(&table, 80));
        let header_rule = &t[2];

        // Double throughout.
        assert!(
            header_rule.contains('\u{2550}'),
            "no ═ fill in {header_rule:?}"
        );
        assert!(!header_rule.contains('\u{2500}'), "light ─ in double rule");
        assert!(!header_rule.contains('\u{253c}'), "light ┼ in double rule");
        assert!(!header_rule.contains('\u{256c}'), "╬ in double rule");

        // Light throughout: every interior rule below the header.
        for line in t.iter().skip(3) {
            if !line.contains('\u{251c}') {
                continue;
            }
            assert!(line.contains('\u{253c}'), "no ┼ cross in {line:?}");
            assert!(!line.contains('\u{2550}'), "═ in light rule");
            assert!(!line.contains('\u{256a}'), "╪ in light rule");
            assert!(!line.contains('\u{256c}'), "╬ in light rule");
        }
    }

    #[test]
    fn body_rows_use_the_light_cross() {
        let mut table = simple();
        table.rows.push(vec![
            plain_cell("second", Align::None),
            plain_cell("row", Align::None),
        ]);
        let t = texts(&render(&table, 80));
        let sep = t
            .iter()
            .find(|l| l.contains('\u{251c}'))
            .expect("a body rule");
        assert!(sep.contains('\u{253c}'), "expected ┼ in {sep:?}");
        assert!(!sep.contains('\u{256c}'), "heavy ╬ in {sep:?}");
    }

    #[test]
    fn every_line_is_the_same_width() {
        let mut table = simple();
        table.rows.push(vec![
            plain_cell("a much longer value", Align::None),
            plain_cell("x", Align::None),
        ]);
        let out = render(&table, 80);
        let widths: Vec<usize> = out.iter().map(|l| l.width()).collect();
        assert!(
            widths.windows(2).all(|w| w[0] == w[1]),
            "ragged: {widths:?}"
        );
    }

    #[test]
    fn table_never_exceeds_available_width() {
        let mut table = simple();
        for i in 0..20 {
            table.rows.push(vec![
                plain_cell(&format!("cell number {i} with a lot of text"), Align::None),
                plain_cell("another long cell value here", Align::None),
            ]);
        }
        for w in [20usize, 30, 50, 80] {
            for l in render(&table, w) {
                assert!(
                    l.width() <= w,
                    "width {w} exceeded by {:?}",
                    l.to_plain_text()
                );
            }
        }
    }

    #[test]
    fn right_alignment_pushes_content_right() {
        // The header is wider than the cell, which is what creates the
        // slack the alignment consumes.
        let table = Table {
            header: vec![plain_cell("Header", Align::Right)],
            rows: vec![vec![plain_cell("7", Align::Right)]],
        };
        let out = render(&table, 40);
        let row = texts(&out).into_iter().find(|l| l.contains('7')).unwrap();
        assert_eq!(row, "\u{2502}      7 \u{2502}", "not right aligned");
    }

    #[test]
    fn center_alignment_balances_slack() {
        let table = Table {
            header: vec![plain_cell("Header", Align::Center)],
            rows: vec![vec![plain_cell("x", Align::Center)]],
        };
        let out = render(&table, 40);
        let row = texts(&out).into_iter().find(|l| l.contains('x')).unwrap();
        assert_eq!(row, "\u{2502}   x    \u{2502}", "not centered");
    }

    #[test]
    fn left_alignment_is_the_default() {
        let table = Table {
            header: vec![plain_cell("Header", Align::None)],
            rows: vec![vec![plain_cell("x", Align::None)]],
        };
        let out = render(&table, 40);
        let row = texts(&out).into_iter().find(|l| l.contains('x')).unwrap();
        assert_eq!(row, "\u{2502} x      \u{2502}");
    }

    #[test]
    fn alignment_never_changes_the_table_width() {
        // All three alignments must produce identically wide grids, or
        // the border rules would not line up.
        let mk = |a: Align| {
            render(
                &Table {
                    header: vec![plain_cell("Header", a)],
                    rows: vec![vec![plain_cell("x", a)]],
                },
                40,
            )
        };
        let w = |ls: &[Line]| ls.iter().map(|l| l.width()).collect::<Vec<_>>();
        assert_eq!(w(&mk(Align::Left)), w(&mk(Align::Center)));
        assert_eq!(w(&mk(Align::Center)), w(&mk(Align::Right)));
    }

    #[test]
    fn long_cells_wrap_instead_of_breaking_the_table() {
        let table = Table {
            header: vec![plain_cell("K", Align::None)],
            rows: vec![vec![plain_cell(
                "a b c d e f g h i j k l m n o p q r s t u v w x y z",
                Align::None,
            )]],
        };
        let out = render(&table, 24);
        assert!(
            out.len() > 4,
            "should need multiple lines: {:?}",
            texts(&out)
        );
        for l in &out {
            assert!(l.width() <= 24);
        }
        let joined = texts(&out).join(" ");
        for letter in ['a', 'z'] {
            assert!(joined.contains(letter), "lost {letter} in wrap");
        }
    }

    #[test]
    fn empty_table_renders_nothing() {
        assert!(render(&Table::default(), 80).is_empty());
    }

    #[test]
    fn header_only_table_is_valid() {
        let table = Table {
            header: vec![plain_cell("A", Align::None), plain_cell("B", Align::None)],
            rows: vec![],
        };
        let out = render(&table, 80);
        assert!(out.len() >= 3, "needs top, header, bottom");
    }
}
