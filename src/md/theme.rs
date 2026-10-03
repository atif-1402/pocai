//! The palette: a map from semantic Markdown roles to the terminal's
//! own 16 colors.
//!
//! There is deliberately no theme file, no preset, and no RGB table.
//! Output uses only ANSI-16 indices, so it recolors with whatever
//! 16-color scheme the terminal is configured with instead of imposing
//! one. A 16-color terminal is also the worst case every terminal can
//! render, so nothing here needs a capability probe.
//!
//! Index convention: 0-7 are the "normal" colors and 8-15 the "bright"
//! ones. Text meant to be read leans bright, since the normal set is
//! illegible on a dark background.

use super::style::{Color, Style};

pub const GREEN: Color = Color::Ansi(2);
pub const YELLOW: Color = Color::Ansi(3);
pub const BLUE: Color = Color::Ansi(4);
pub const BRIGHT_BLACK: Color = Color::Ansi(8);
pub const BRIGHT_RED: Color = Color::Ansi(9);
pub const BRIGHT_GREEN: Color = Color::Ansi(10);
pub const BRIGHT_YELLOW: Color = Color::Ansi(11);
pub const BRIGHT_BLUE: Color = Color::Ansi(12);
pub const BRIGHT_MAGENTA: Color = Color::Ansi(13);
pub const BRIGHT_CYAN: Color = Color::Ansi(14);

/// GitHub-style callout kinds, from a `> [!NOTE]` blockquote.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Alert {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl Alert {
    /// The marker and label a callout renders with.
    pub fn icon_label(self) -> (&'static str, &'static str) {
        match self {
            Alert::Note => ("[i]", "Note"),
            Alert::Tip => ("[*]", "Tip"),
            Alert::Important => ("[!]", "Important"),
            Alert::Warning => ("[!]", "Warning"),
            Alert::Caution => ("[x]", "Caution"),
        }
    }

    pub fn color(self) -> Color {
        match self {
            Alert::Note => BRIGHT_BLUE,
            Alert::Tip => BRIGHT_GREEN,
            Alert::Important => BRIGHT_MAGENTA,
            Alert::Warning => BRIGHT_YELLOW,
            Alert::Caution => BRIGHT_RED,
        }
    }
}

/// Body text: no styling, so the terminal's own foreground is used and
/// the text follows the user's color scheme exactly.
pub fn text() -> Style {
    Style::default()
}

/// `**bold**`.
pub fn strong() -> Style {
    Style::new().bold()
}

/// Headings. Levels 1-4 get distinct hues so a document's outline is
/// readable at a glance; 5 and 6 fall back to the same color as 4.
pub fn heading(level: u8) -> Style {
    let color = match level {
        1 => BRIGHT_BLUE,
        2 => BRIGHT_CYAN,
        3 => BRIGHT_YELLOW,
        4 => BRIGHT_GREEN,
        _ => BRIGHT_MAGENTA,
    };
    Style::new().fg(color).bold()
}

/// The `═` under an h1 and the `─` under an h2.
pub fn heading_rule() -> Style {
    Style::new().fg(BRIGHT_BLACK)
}

/// A `---` thematic break.
pub fn rule() -> Style {
    Style::new().fg(BRIGHT_BLACK)
}

/// The `▏` bar in front of blockquoted text.
pub fn quote_bar(alert: Option<Alert>) -> Style {
    match alert {
        Some(a) => Style::new().fg(a.color()),
        None => Style::new().fg(BLUE),
    }
}

/// Text inside a blockquote. Italic plus a dimmer hue keeps it clearly
/// subordinate to the surrounding prose.
pub fn quote_text(alert: Option<Alert>) -> Style {
    let color = match alert {
        Some(a) => a.color(),
        None => BRIGHT_BLACK,
    };
    Style::new().fg(color).italic()
}

/// The bold `NOTE` header of a callout.
pub fn alert_label(alert: Alert) -> Style {
    Style::new().fg(alert.color()).bold()
}

/// `•` / `◦` / `▸` bullets, colored by nesting depth.
pub fn bullet(depth: usize) -> Style {
    let color = match depth {
        1 => GREEN,
        2 => BLUE,
        _ => YELLOW,
    };
    Style::new().fg(color)
}

/// An ordered-list marker such as `1.`.
pub fn ordered_marker() -> Style {
    Style::new().fg(GREEN)
}

/// `☑` and `☐`.
pub fn task_marker(checked: bool) -> Style {
    if checked {
        Style::new().fg(BRIGHT_GREEN)
    } else {
        Style::new().fg(BRIGHT_BLACK)
    }
}

/// The `┌─ │ └` frame drawn around a code block.
pub fn code_frame() -> Style {
    Style::new().fg(BRIGHT_BLACK)
}

/// The language name in a code block's top border.
pub fn code_label() -> Style {
    Style::new().fg(BRIGHT_CYAN).bold()
}

/// The `│` / `┊` gutter of a code block row.
pub fn code_gutter() -> Style {
    Style::new().fg(BRIGHT_BLACK)
}

/// Line numbers in a code block gutter.
pub fn code_line_number() -> Style {
    Style::new().fg(BRIGHT_BLACK)
}

/// Inline code. Bright yellow reads as "code" without needing a
/// background color, which 16-color terminals cannot do.
pub fn inline_code() -> Style {
    Style::new().fg(BRIGHT_YELLOW)
}

/// `==highlighted==`. Reverse video is the closest a 16-color terminal
/// gets to a highlighter background, and it reads correctly as one.
pub fn mark() -> Style {
    Style::new().reverse()
}

/// The visible text of a link.
pub fn link_text() -> Style {
    Style::new().fg(BRIGHT_BLUE).underline()
}

/// A link's URL, shown after its text. Dimmed so it stays subordinate
/// to the link text itself.
pub fn link_url() -> Style {
    Style::new().fg(BLUE)
}

/// Table borders and the `│` between cells.
pub fn table_border() -> Style {
    Style::new().fg(BRIGHT_BLACK)
}

/// The `╞══╪╡` row under a table header.
///
/// Deliberately the same color as [`table_border`]. Every border in a
/// table -- outer frame, cell dividers, body rules, and the header rule
/// -- is one flat grid, so the rules differ by shape (`═` with `╞ ╪ ╡`
/// against `─` with `├ ┼ ┤`) and never by color. Tinting this one line
/// made the header rule the loudest element in the table.
///
/// Delegating instead of repeating the color keeps the two styles from
/// drifting apart when the palette is restyled.
pub fn table_separator() -> Style {
    table_border()
}

/// Table header cells.
pub fn table_header() -> Style {
    Style::new().fg(BRIGHT_BLUE).bold()
}

/// Table body cells.
pub fn table_cell() -> Style {
    Style::default()
}

/// Inline LaTeX.
pub fn latex() -> Style {
    Style::new().fg(BRIGHT_MAGENTA)
}

/// Mermaid diagram lines that are structural chrome (frames, arrows).
pub fn diagram_chrome() -> Style {
    Style::new().fg(BRIGHT_BLACK)
}

/// Mermaid node labels.
pub fn diagram_label() -> Style {
    Style::new().fg(BRIGHT_CYAN)
}

/// Mermaid edge and node text.
pub fn diagram_text() -> Style {
    Style::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_stays_inside_the_16_color_palette() {
        let styles = [
            text(),
            strong(),
            heading(1),
            heading(2),
            heading(3),
            heading(4),
            heading(5),
            heading(6),
            heading_rule(),
            rule(),
            quote_bar(None),
            quote_bar(Some(Alert::Note)),
            quote_text(None),
            alert_label(Alert::Caution),
            bullet(1),
            bullet(2),
            bullet(3),
            ordered_marker(),
            task_marker(true),
            task_marker(false),
            code_frame(),
            code_label(),
            code_gutter(),
            code_line_number(),
            inline_code(),
            mark(),
            link_text(),
            link_url(),
            table_border(),
            table_separator(),
            table_header(),
            table_cell(),
            latex(),
            diagram_chrome(),
            diagram_label(),
            diagram_text(),
        ];
        for s in styles {
            for c in [s.fg, s.bg].into_iter().flatten() {
                assert!(
                    matches!(c, Color::Ansi(n) if n < 16),
                    "role produced non-16-color output: {c:?}"
                );
            }
        }
    }

    #[test]
    fn body_text_emits_nothing() {
        assert!(text().is_empty());
    }
}
