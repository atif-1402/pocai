//! ANSI escape-sequence output.
//!
//! Two output modes, matching what a terminal can be piped into:
//!
//! * **styled** — SGR sequences for color and modifiers, used when stdout
//!   is a terminal and `NO_COLOR` is unset.
//! * **plain** — no escape bytes at all, used when redirected or when
//!   `NO_COLOR` is set, so `pocai ... > file` yields clean text.

use std::io::{self, Write};

use super::style::{Color, Line, Style};
use super::width::iter_cluster_widths;

const RESET: &[u8] = b"\x1b[0m";

/// ANSI SGR parameters for the 16 basic colors, `[fg, bg]` per index.
#[rustfmt::skip]
const BASIC_CODES: [[&[u8]; 2]; 16] = [
    [b"30", b"40"], // 0  black
    [b"31", b"41"], // 1  red
    [b"32", b"42"], // 2  green
    [b"33", b"43"], // 3  yellow
    [b"34", b"44"], // 4  blue
    [b"35", b"45"], // 5  magenta
    [b"36", b"46"], // 6  cyan
    [b"37", b"47"], // 7  white
    [b"90", b"100"], // 8  bright black
    [b"91", b"101"], // 9  bright red
    [b"92", b"102"], // 10 bright green
    [b"93", b"103"], // 11 bright yellow
    [b"94", b"104"], // 12 bright blue
    [b"95", b"105"], // 13 bright magenta
    [b"96", b"106"], // 14 bright cyan
    [b"97", b"107"], // 15 bright white
];

/// Writes rendered lines to an output stream.
pub struct AnsiWriter<W: Write> {
    out: W,
    styled: bool,
    /// Style currently in effect on the terminal.
    ///
    /// Long unbreakable tokens -- a bare URL, say -- are split into one
    /// run per grapheme to stay inside the width. Re-emitting the SGR
    /// sequence for each of those would turn one URL into hundreds of
    /// bytes of escapes, so it is only written when the style changes.
    current: Style,
}

impl<W: Write> AnsiWriter<W> {
    /// A writer that emits escape sequences, for interactive terminals.
    pub fn styled(out: W) -> Self {
        AnsiWriter {
            out,
            styled: true,
            current: Style::default(),
        }
    }

    /// A writer that emits plain text only.
    pub fn plain(out: W) -> Self {
        AnsiWriter {
            out,
            styled: false,
            current: Style::default(),
        }
    }

    fn put(&mut self, bytes: &[u8]) {
        let _ = self.out.write_all(bytes);
    }

    fn put_str(&mut self, s: &str) {
        let _ = self.out.write_all(s.as_bytes());
    }

    /// Emit one line, breaking it at grapheme boundaries if it would
    /// exceed `max_width`.
    ///
    /// Layout already wraps to the target width; this is a final safety
    /// net so a bug upstream degrades into an extra line rather than a
    /// corrupted or overrunning line.
    pub fn line(&mut self, line: &Line, max_width: usize) {
        let mut col = 0usize;
        for run in &line.runs {
            if run.text.is_empty() {
                continue;
            }
            let styled_run = self.styled && !run.style.is_empty();
            if styled_run {
                if run.style != self.current {
                    self.set_style(run.style);
                    self.current = run.style;
                }
            } else if self.current != Style::default() {
                // Falling back to unstyled text: reset first, or the run
                // inherits the previous color. Padding inside a colored
                // table cell is the common case, and on a light
                // background that tint is plainly visible.
                self.put(RESET);
                self.current = Style::default();
            }
            for (cluster, cluster_w) in iter_cluster_widths(&run.text) {
                if col + cluster_w > max_width && col > 0 {
                    if styled_run {
                        self.put(RESET);
                        self.current = Style::default();
                    }
                    self.put(b"\n");
                    col = 0;
                    if styled_run {
                        self.set_style(run.style);
                        self.current = run.style;
                    }
                }
                self.put(cluster.as_bytes());
                col += cluster_w;
            }
        }
        if self.styled {
            // Close the style once, at the end of the line, rather than
            // after every run: fewer bytes, same terminal state. Plain
            // output must stay free of escape sequences.
            self.put(RESET);
            self.current = Style::default();
        }
        self.put(b"\n");
    }

    fn set_style(&mut self, style: Style) {
        let mut seq: Vec<String> = Vec::with_capacity(6);
        if let Some(c) = style.fg {
            seq.push(color_params(c, false));
        }
        if let Some(c) = style.bg {
            seq.push(color_params(c, true));
        }
        if style.bold {
            seq.push("1".to_string());
        }
        if style.italic {
            seq.push("3".to_string());
        }
        if style.underline {
            seq.push("4".to_string());
        }
        if style.strike {
            seq.push("9".to_string());
        }
        if style.reverse {
            seq.push("7".to_string());
        }
        if seq.is_empty() {
            return;
        }
        let joined = seq.join(";");
        let mut buf = String::with_capacity(joined.len() + 3);
        buf.push_str("\x1b[");
        buf.push_str(&joined);
        buf.push('m');
        self.put_str(&buf);
    }
}

impl<W: Write> Write for AnsiWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.out.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// SGR parameters selecting `color` for the foreground or background.
fn color_params(color: Color, bg: bool) -> String {
    match color {
        Color::Ansi(n) if n < 16 => {
            let code = BASIC_CODES[n as usize][bg as usize];
            String::from_utf8_lossy(code).into_owned()
        }
        // Out-of-range ANSI index: fall back to default rather than
        // emitting a malformed sequence.
        Color::Ansi(_) => {
            if bg {
                "49".to_string()
            } else {
                "39".to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::md::style::Run;

    fn styled_run(style: Style) -> Line {
        Line::from_runs(vec![Run::new("hi", style)])
    }

    /// Padding after colored text must not inherit the color.
    #[test]
    fn plain_run_after_styled_run_resets() {
        let mut w = AnsiWriter::styled(Vec::new());
        let line = Line::from_runs(vec![
            Run::new("Name", Style::new().bold().fg(Color::Ansi(12))),
            Run::plain("     "),
        ]);
        w.line(&line, 80);
        let out = String::from_utf8(w.out).expect("utf-8");
        // Style, text, reset, padding, trailing reset.
        assert_eq!(out.matches("\x1b[0m").count(), 2, "{out:?}");
        let after = &out[out.find("Name").expect("text") + 4..];
        assert!(
            after.starts_with("\x1b[0m"),
            "padding kept the color: {out:?}"
        );
    }

    /// Adjacent runs sharing a style must not each restate it. This is
    /// what keeps a split-up URL from costing one escape per character.
    #[test]
    fn adjacent_runs_sharing_a_style_emit_it_once() {
        let style = Style::new().fg(Color::Ansi(12)).underline();
        // 20 columns either way, so neither case wraps.
        for (label, runs) in [
            ("one run", vec![Run::new("x".repeat(20), style)]),
            ("ten runs", (0..10).map(|_| Run::new("xx", style)).collect()),
        ] {
            let mut w = AnsiWriter::styled(Vec::new());
            w.line(&Line::from_runs(runs), 80);
            let out = String::from_utf8(w.out).expect("utf-8");
            assert_eq!(out.matches("\x1b[").count(), 2, "{label}: {out:?}");
        }
    }

    #[test]
    fn plain_mode_emits_no_escape_bytes() {
        let mut buf: Vec<u8> = Vec::new();
        let mut w = AnsiWriter::plain(&mut buf);
        w.line(&styled_run(Style::new().fg(Color::Ansi(12)).bold()), 80);
        assert_eq!(String::from_utf8(buf).unwrap(), "hi\n");
    }

    #[test]
    fn styled_mode_emits_sgr() {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut w = AnsiWriter::styled(&mut buf);
            w.line(&styled_run(Style::new().fg(Color::Ansi(12))), 80);
        }
        assert_eq!(String::from_utf8(buf).unwrap(), "\x1b[94mhi\x1b[0m\n");
    }

    #[test]
    fn every_ansi_color_has_fg_and_bg_codes() {
        for n in 0..16u8 {
            assert!(!color_params(Color::Ansi(n), false).is_empty(), "fg {n}");
            assert!(!color_params(Color::Ansi(n), true).is_empty(), "bg {n}");
        }
        assert_eq!(color_params(Color::Ansi(1), false), "31");
        assert_eq!(color_params(Color::Ansi(9), false), "91");
        assert_eq!(color_params(Color::Ansi(1), true), "41");
    }

    #[test]
    fn wide_chars_never_exceed_width() {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut w = AnsiWriter::plain(&mut buf);
            w.line(&Line::plain("\u{6771}\u{4eac}\u{4e2d}\u{6587}"), 4);
        }
        let text = String::from_utf8(buf).unwrap();
        for l in text.lines() {
            assert!(display_width_of(l) <= 4, "line {l:?} too wide");
        }
    }

    fn display_width_of(s: &str) -> usize {
        super::super::width::display_width(s)
    }
}
