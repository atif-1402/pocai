//! Spinner animation and Markdown rendering.
//!
//! Rendering is done in-process by [`crate::md`], so the response never
//! leaves the program and there is no external `leaf` dependency.

use std::io::{self, IsTerminal, Write};
use std::sync::mpsc;
use std::time::Duration;

use crate::md;
use crate::ui::palette;

/// Run `f` on a worker thread while animating a spinner on stderr.
pub fn run_with_spinner<T, F>(f: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    let p = palette();
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mut i = 0usize;
    loop {
        match rx.try_recv() {
            Ok(v) => {
                eprint!("\r\x1b[K");
                let _ = io::stderr().flush();
                return v;
            }
            Err(mpsc::TryRecvError::Empty) => {
                eprint!("\r{}{}{} Thinking...", p.cyan, frames[i], p.reset);
                let _ = io::stderr().flush();
                i = (i + 1) % frames.len();
                std::thread::sleep(Duration::from_millis(80));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                eprint!("\r\x1b[K");
                let _ = io::stderr().flush();
                return rx.recv().expect("worker thread failed");
            }
        }
    }
}

/// Usable width of the terminal, with a sane floor for the cases where
/// it cannot be determined.
pub fn terminal_width() -> usize {
    // Prefer the real terminal size, fall back to `COLUMNS` (which CI and
    // some shells set), then to a conventional 80.
    if let Some((terminal_size::Width(w), _)) = terminal_size::terminal_size() {
        let w = w as usize;
        if w >= 20 {
            return w;
        }
    }
    if let Some(v) = std::env::var("COLUMNS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        if v >= 20 {
            return v;
        }
    }
    80
}

/// Render Markdown to stdout.
///
/// Color is emitted only for an interactive terminal: a redirected or
/// piped stdout gets plain text, and `NO_COLOR` is honored, so output
/// stays greppable and pipe-safe.
pub fn render_response(text: &str) {
    let opts = md::Options {
        width: terminal_width(),
        color: io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        line_numbers: true,
        smart_punctuation: true,
        linkify: true,
    };
    let rendered = md::render(text, &opts);
    let mut out = io::stdout().lock();
    let _ = out.write_all(rendered.as_bytes());
    let _ = out.flush();
}
