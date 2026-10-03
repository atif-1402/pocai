//! Inline content: emphasis, inline code, `==highlight==`, and links.

use super::style::{Line, Run, Style};
use super::theme::{self, Alert};
use super::wrap;

/// Delimiter for the `==highlight==` extension.
const MARK: &str = "==";

/// Where inline content is being rendered, which changes its base color.
#[derive(Clone, Copy, Default, Debug)]
pub struct InlineCtx {
    pub blockquote_depth: usize,
    pub alert: Option<Alert>,
    /// Inside a heading, where link styling is still honored but the
    /// surrounding paragraph color does not apply.
    pub in_heading: bool,
}

impl InlineCtx {
    pub fn root() -> Self {
        InlineCtx::default()
    }
}

/// Nesting state of inline emphasis. Counters rather than booleans
/// because emphasis can nest (`**bold with *italic* inside**`).
#[derive(Clone, Copy, Default, Debug)]
pub struct InlineState {
    pub strong: usize,
    pub emphasis: usize,
    pub strike: usize,
    pub link: bool,
}

/// The style plain text takes on in the current context.
pub fn text_style(ctx: &InlineCtx, st: &InlineState) -> Style {
    let mut s = if st.link {
        theme::link_text()
    } else if ctx.blockquote_depth > 0 {
        theme::quote_text(ctx.alert)
    } else {
        theme::text()
    };
    if st.link && ctx.blockquote_depth > 0 {
        s = s.italic();
    }
    if st.strong > 0 {
        s = s.bold();
    }
    if st.emphasis > 0 {
        s = s.italic();
    }
    if st.strike > 0 {
        s.strike = true;
    }
    s
}

/// True when `content` is a well-formed `==highlight==` body.
///
/// Rejects empty, multi-line, and space-padded bodies so that
/// `==unclosed`, `====`, and `== spaced ==` stay literal.
fn is_valid_mark(content: &str) -> bool {
    !content.is_empty()
        && !content.contains('\n')
        && !content.starts_with(' ')
        && !content.ends_with(' ')
}

fn push_segment(runs: &mut Vec<Run>, text: &str, style: Style) {
    if text.is_empty() {
        return;
    }
    // Coalesce with the previous run when the style matches, so
    // highlighted text does not fragment the surrounding run.
    if let Some(last) = runs.last_mut() {
        if last.style == style {
            last.text.push_str(text);
            return;
        }
    }
    runs.push(Run::new(text, style));
}

/// Push plain text, interpreting `==highlight==` spans.
///
/// This is a scan of each text fragment rather than a parser extension,
/// so a marker straddling an inline boundary (`**bo==ld**`) is left
/// literal instead of being half-styled.
pub fn push_text(runs: &mut Vec<Run>, text: &str, base: Style) {
    let mut rest = text;
    loop {
        let Some(open) = rest.find(MARK) else {
            push_segment(runs, rest, base);
            return;
        };
        let after = &rest[open + MARK.len()..];
        let Some(close) = after.find(MARK) else {
            push_segment(runs, rest, base);
            return;
        };
        let content = &after[..close];
        if is_valid_mark(content) {
            push_segment(runs, &rest[..open], base);
            let marked = format!(" {content} ");
            push_segment(runs, &marked, theme::mark());
            rest = &after[close + MARK.len()..];
        } else {
            // Not a highlight: keep this opener literal and resume
            // scanning just after it, so a later valid pair still works.
            push_segment(runs, &rest[..open + MARK.len()], base);
            rest = after;
        }
    }
}

/// Push inline code, space-padded so it reads as a distinct chip.
pub fn push_inline_code(runs: &mut Vec<Run>, text: &str) {
    push_segment(runs, &format!(" {text} "), theme::inline_code());
}

/// Push a link's URL after its text, dimmed so the text stays primary.
///
/// Unlike a mouse-driven viewer there is nowhere to click here, so
/// showing the destination is the only way it is reachable.
pub fn push_link_url(runs: &mut Vec<Run>, url: &str) {
    if url.is_empty() {
        return;
    }
    let shown = super::links::display_url(url);
    push_segment(runs, &format!(" ({shown})"), theme::link_url());
}

/// Wrap inline runs into finished lines, prefixing blockquote bars.
pub fn wrap_inline(runs: &[Run], prefix: &[Run], render_width: usize) -> Vec<Line> {
    wrap::wrap(runs, prefix, prefix, render_width)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(runs: &[Run]) -> String {
        runs.iter().map(|r| r.text.as_str()).collect()
    }

    #[test]
    fn plain_text_gets_no_style() {
        let mut runs = Vec::new();
        push_text(&mut runs, "hello", Style::default());
        assert_eq!(texts(&runs), "hello");
        assert!(runs[0].style.is_empty());
    }

    #[test]
    fn bold_adds_bold_without_color() {
        let ctx = InlineCtx::root();
        let st = InlineState {
            strong: 1,
            ..InlineState::default()
        };
        let s = text_style(&ctx, &st);
        assert!(s.bold);
        assert!(s.fg.is_none());
    }

    #[test]
    fn inline_code_is_padded() {
        let mut runs = Vec::new();
        push_inline_code(&mut runs, "ls");
        assert_eq!(texts(&runs), " ls ");
    }

    #[test]
    fn highlight_is_padded_and_styled() {
        let mut runs = Vec::new();
        push_text(&mut runs, "before ==marked== after", Style::default());
        assert_eq!(texts(&runs), "before  marked  after");
        assert!(runs[1].style.reverse, "mark uses reverse video");
    }

    #[test]
    fn unclosed_marker_stays_literal() {
        let mut runs = Vec::new();
        push_text(&mut runs, "==unclosed", Style::default());
        assert_eq!(texts(&runs), "==unclosed");
        assert!(!runs.iter().any(|r| r.style.reverse));
    }

    #[test]
    fn empty_and_spaced_markers_stay_literal() {
        for input in ["====", "== text =="] {
            let mut runs = Vec::new();
            push_text(&mut runs, input, Style::default());
            assert!(!runs.iter().any(|r| r.style.reverse), "{input:?}");
        }
    }

    #[test]
    fn invalid_first_pair_falls_through_to_the_second() {
        let mut runs = Vec::new();
        push_text(&mut runs, "x == y and ==marked== end", Style::default());
        let joined = texts(&runs);
        assert!(joined.contains("x =="), "got {joined:?}");
        assert!(
            runs.iter().any(|r| r.style.reverse),
            "second pair should mark"
        );
    }

    #[test]
    fn link_url_is_shown_inline() {
        let mut runs = Vec::new();
        runs.push(Run::new("docs", theme::link_text()));
        push_link_url(&mut runs, "https://example.com");
        assert_eq!(texts(&runs), "docs (https://example.com)");
    }

    #[test]
    fn emphasis_nests_without_clobbering_link_color() {
        let ctx = InlineCtx::root();
        let st = InlineState {
            strong: 1,
            emphasis: 1,
            link: true,
            ..InlineState::default()
        };
        let s = text_style(&ctx, &st);
        assert_eq!(s, theme::link_text().bold().italic());
    }

    #[test]
    fn blockquote_text_is_italic() {
        let ctx = InlineCtx {
            blockquote_depth: 1,
            ..InlineCtx::root()
        };
        assert!(text_style(&ctx, &InlineState::default()).italic);
    }
}
