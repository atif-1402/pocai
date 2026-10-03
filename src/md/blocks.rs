//! Block-level layout: headings, thematic breaks, and blockquote bars.

use super::style::{Line, Run};
use super::theme::{self, Alert};

/// Column interval in which a `▏` blockquote bar is drawn.
const QUOTE_BAR: &str = "\u{258f} ";

/// Width of the quote prefix added per nesting level.
#[cfg(test)]
const QUOTE_BAR_WIDTH: usize = 2;

/// Horizontal rule characters: heavy for an h1's underline, light
/// everywhere else.
pub const HEAVY_RULE: char = '\u{2550}';
pub const LIGHT_RULE: char = '\u{2500}';

/// Shortest horizontal rule we will draw, so a deeply indented rule
/// still reads as a rule.
const MIN_RULE_WIDTH: usize = 8;

/// The `▏ ` prefix repeated once per blockquote nesting level.
///
/// `QUOTE_BAR` already carries its own padding column, so the result is
/// self-aligning: level 1 is two columns wide, level 2 is four.
///
/// The outermost bar takes the callout's color when the blockquote is a
/// GitHub alert, so `> [!NOTE]` is visually distinct from `>`.
pub fn quote_prefix(depth: usize, alert: Option<Alert>) -> Vec<Run> {
    if depth == 0 {
        return Vec::new();
    }
    let mut runs = Vec::with_capacity(depth);
    let outer = theme::quote_bar(alert);
    for level in 0..depth {
        let style = if level == 0 {
            outer
        } else {
            theme::quote_bar(None)
        };
        runs.push(Run::new(QUOTE_BAR, style));
    }
    runs
}

/// Width consumed by [`quote_prefix`].
/// Only the regression tests need this now that the rail itself is drawn
/// on every line rather than measured as reserved space.
#[cfg(test)]
pub fn quote_prefix_width(depth: usize) -> usize {
    depth * QUOTE_BAR_WIDTH
}

/// Render a heading, plus the underline an h1 or h2 gets beneath it.
pub fn heading_lines(level: u8, runs: &[Run], render_width: usize) -> Vec<Line> {
    let heading_style = theme::heading(level);
    let styled: Vec<Run> = runs
        .iter()
        .map(|r| Run::new(r.text.clone(), heading_style.merged_over(r.style)))
        .collect();

    let mut lines = super::inline::wrap_inline(&styled, &[], render_width);

    let rule_char = match level {
        1 => HEAVY_RULE,
        2 => LIGHT_RULE,
        _ => return lines,
    };

    let title_width = lines.first().map(|l| l.width()).unwrap_or(0);
    let rule_width = title_width
        .min(render_width)
        .max(1)
        .min(super::blocks::rule_width(render_width, 0));
    lines.push(Line::from_runs(vec![Run::new(
        rule_char.to_string().repeat(rule_width),
        theme::heading_rule(),
    )]));
    lines
}

/// Width of a thematic break, given the current indent.
pub fn rule_width(render_width: usize, indent: usize) -> usize {
    render_width.saturating_sub(indent).max(MIN_RULE_WIDTH)
}

/// A `---` thematic break, followed by a blank line.
pub fn rule_lines(render_width: usize, indent: usize) -> Vec<Line> {
    let w = rule_width(render_width.saturating_sub(indent), 0);
    vec![
        Line::from_runs(vec![Run::new(
            LIGHT_RULE.to_string().repeat(w),
            theme::rule(),
        )]),
        Line::new(),
    ]
}

/// The bold `[i] Note` header of a GitHub callout.
pub fn alert_header(alert: Alert, prefix: &[Run]) -> Line {
    let (icon, label) = alert.icon_label();
    let mut runs = prefix.to_vec();
    let style = theme::alert_label(alert);
    runs.push(Run::new(icon, style));
    runs.push(Run::new(" ", style));
    runs.push(Run::new(label, style));
    Line::from_runs(runs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::md::style::Style;
    use crate::md::width::display_width;

    fn texts(lines: &[Line]) -> Vec<String> {
        lines.iter().map(|l| l.to_plain_text()).collect()
    }

    #[test]
    fn h1_gets_a_heavy_underline_the_width_of_its_title() {
        let out = heading_lines(1, &[Run::new("Tokyo", Style::default())], 80);
        assert_eq!(texts(&out), vec!["Tokyo".to_string(), "\u{2550}".repeat(5)]);
    }

    #[test]
    fn h2_gets_a_light_underline() {
        let out = heading_lines(2, &[Run::new("Tokyo", Style::default())], 80);
        assert_eq!(texts(&out), vec!["Tokyo".to_string(), "\u{2500}".repeat(5)]);
    }

    #[test]
    fn h3_and_below_get_no_underline() {
        for level in 3..=6 {
            let out = heading_lines(level, &[Run::plain("Tokyo")], 80);
            assert_eq!(out.len(), 1, "level {level} should not add a rule");
        }
    }

    #[test]
    fn wide_title_underline_measures_in_columns_not_chars() {
        // Four ideographs = eight columns, so the rule is eight long.
        let out = heading_lines(1, &[Run::plain("\u{6771}\u{4eac}\u{4e2d}\u{6587}")], 80);
        assert_eq!(out[1].to_plain_text().chars().count(), 8);
    }

    #[test]
    fn heading_levels_use_distinct_colors() {
        let a = theme::heading(1);
        let b = theme::heading(2);
        assert_ne!(a.fg, b.fg);
    }

    #[test]
    fn quote_prefix_is_two_columns_per_level() {
        assert_eq!(quote_prefix_width(0), 0);
        assert_eq!(quote_prefix_width(1), 2);
        assert_eq!(quote_prefix_width(3), 6);
    }

    #[test]
    fn nested_quote_bars_nest() {
        // Each bar carries its own padding column, so two levels of
        // quoting cost four columns and stay self-aligning.
        let text = |p: Vec<Run>| p.iter().map(|r| r.text.as_str()).collect::<String>();
        assert_eq!(text(quote_prefix(1, None)), "\u{258f} ");
        assert_eq!(text(quote_prefix(2, None)), "\u{258f} \u{258f} ");
        assert_eq!(text(quote_prefix(3, None)), "\u{258f} \u{258f} \u{258f} ");
        assert!(quote_prefix(0, None).is_empty());
    }

    #[test]
    fn quote_prefix_width_matches_its_text() {
        for depth in 0..5 {
            let p = quote_prefix(depth, None);
            let actual: usize = p.iter().map(|r| r.width()).sum();
            assert_eq!(
                actual,
                quote_prefix_width(depth),
                "declared width disagrees with the prefix at depth {depth}"
            );
        }
    }

    #[test]
    fn alert_colors_only_the_outermost_bar() {
        let p = quote_prefix(2, Some(Alert::Note));
        assert_ne!(p[0].style.fg, p[1].style.fg);
    }

    #[test]
    fn rule_fills_the_width_and_is_followed_by_a_blank() {
        let out = rule_lines(24, 0);
        assert_eq!(display_width(&out[0].to_plain_text()), 24);
        assert!(out[1].is_blank());
    }

    #[test]
    fn alert_header_uses_the_documented_marker() {
        let line = alert_header(Alert::Tip, &[]);
        assert_eq!(line.to_plain_text(), "[*] Tip");
    }
}
