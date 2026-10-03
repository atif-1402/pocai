//! Styled text primitives: colors, style flags, and the run/line
//! containers that the renderer emits.

/// A terminal color.
///
/// Only [`Color::Ansi`] is produced by the built-in palette, so output
/// follows whatever 16-color scheme the user's terminal is configured
/// with. Syntax highlighting receives 24-bit colors from syntect; those
/// are quantized down to this palette at the highlight boundary (see
/// [`crate::md::highlight`]) so they follow the terminal's own scheme
/// instead of imposing one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    /// One of the 16 basic ANSI colors. 0-7 are the normal (dim) set,
    /// 8-15 the bright set.
    Ansi(u8),
}

impl Color {
    /// The bright counterpart of a normal ANSI color (`0-7` -> `8-15`),
    /// or the color itself if already bright or not an ANSI color.
    #[allow(dead_code)] // part of the Color value API
    pub fn bright(self) -> Color {
        match self {
            Color::Ansi(n) if n < 8 => Color::Ansi(n + 8),
            other => other,
        }
    }

    /// True when this is a normal (non-bright) ANSI color, which is
    /// illegible against most dark backgrounds.
    #[allow(dead_code)] // part of the Color value API
    pub fn is_dark_ansi(self) -> bool {
        matches!(self, Color::Ansi(n) if n < 8)
    }
}

/// A set of style attributes applied to a run of text.
///
/// Defaults to "inherit the terminal's defaults" (no fg, no bg, no
/// modifiers), which is why plain prose carries no style at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Swap foreground and background. Used to emulate a highlight
    /// background, which 16-color terminals cannot express otherwise.
    pub reverse: bool,
}

impl Style {
    pub fn new() -> Style {
        Style::default()
    }

    pub fn fg(mut self, c: Color) -> Style {
        self.fg = Some(c);
        self
    }

    #[allow(dead_code)] // reserved for background chips
    pub fn bg(mut self, c: Color) -> Style {
        self.bg = Some(c);
        self
    }

    pub fn bold(mut self) -> Style {
        self.bold = true;
        self
    }

    pub fn italic(mut self) -> Style {
        self.italic = true;
        self
    }

    pub fn underline(mut self) -> Style {
        self.underline = true;
        self
    }

    #[allow(dead_code)] // part of the Style value API
    pub fn dim(mut self) -> Style {
        self.fg = Some(Color::Ansi(8));
        self
    }

    pub fn reverse(mut self) -> Style {
        self.reverse = true;
        self
    }

    /// True when this style would emit any escape sequence at all.
    pub fn is_empty(&self) -> bool {
        self.fg.is_none()
            && self.bg.is_none()
            && !self.bold
            && !self.italic
            && !self.underline
            && !self.strike
            && !self.reverse
    }

    /// Apply this style's attributes on top of `base`.
    pub fn merged_over(self, base: Style) -> Style {
        Style {
            fg: self.fg.or(base.fg),
            bg: self.bg.or(base.bg),
            bold: self.bold || base.bold,
            italic: self.italic || base.italic,
            underline: self.underline || base.underline,
            strike: self.strike || base.strike,
            reverse: self.reverse || base.reverse,
        }
    }
}

/// A run of text sharing a single style. The unit of horizontal layout.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Run {
    pub text: String,
    pub style: Style,
}

impl Run {
    pub fn new(text: impl Into<String>, style: Style) -> Run {
        Run {
            text: text.into(),
            style,
        }
    }

    /// A run that inherits the terminal defaults.
    pub fn plain(text: impl Into<String>) -> Run {
        Run::new(text, Style::default())
    }

    /// Total display width of this run, ignoring ANSI.
    pub fn width(&self) -> usize {
        crate::md::width::display_width(&self.text)
    }

    /// True when the run has no visible content.
    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }
}

/// A single output line: an ordered list of styled runs.
#[derive(Clone, Default, Debug)]
pub struct Line {
    pub runs: Vec<Run>,
}

// `Line` is a small value type with a complete constructor set; the
// plain-text conveniences below are the seam the layout tests build
// fixtures through, so they are exercised from `#[cfg(test)]` only.
#[allow(dead_code)]
impl Line {
    pub fn new() -> Line {
        Line { runs: Vec::new() }
    }

    pub fn from_runs(runs: Vec<Run>) -> Line {
        Line { runs }
    }

    pub fn plain(text: impl Into<String>) -> Line {
        Line {
            runs: vec![Run::plain(text)],
        }
    }

    pub fn push(&mut self, run: Run) {
        self.runs.push(run);
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Total display width of the line, ignoring ANSI.
    pub fn width(&self) -> usize {
        self.runs.iter().map(|r| r.width()).sum()
    }

    /// The line's text with all styling discarded. Used by the plain
    /// output mode and by tests.
    pub fn to_plain_text(&self) -> String {
        let mut s = String::new();
        for run in &self.runs {
            s.push_str(&run.text);
        }
        s
    }

    /// True when the line contains no visible characters.
    pub fn is_blank(&self) -> bool {
        self.runs.is_empty() || self.runs.iter().all(|r| r.is_blank())
    }

    /// Blank for spacing purposes, ignoring a blockquote rail.
    ///
    /// A rail-only line reads as empty to the eye, so a second one adds
    /// height rather than separation and stacks up into visible gaps.
    pub fn is_blank_content(&self) -> bool {
        !self
            .runs
            .iter()
            .any(|r| !r.text.chars().all(|c| c == '\u{258f}' || c.is_whitespace()))
    }

    /// Remove trailing blank runs, so a line's width is exact.
    pub fn trim_trailing_blank_runs(&mut self) {
        while self.runs.last().map(|r| r.is_blank()).unwrap_or(false) {
            self.runs.pop();
        }
    }
}
