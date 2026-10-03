//! Code block syntax highlighting.
//!
//! syntect does the token classification; the output colors are then
//! quantized to the terminal's 16 colors so highlighting follows the
//! terminal's scheme like the rest of the renderer, instead of imposing
//! 24-bit colors the terminal may not even be in.
//!
//! The syntax set is expensive to build, so it is loaded lazily and
//! only when a fence actually carries a language tag.

use std::sync::LazyLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use super::style::{Color, Run, Style};

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEMES: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

/// Preferred syntect theme, in order. The first available wins.
const THEME_PREFERENCES: [&str; 4] = [
    "base16-ocean.dark",
    "base16-ocean",
    "InspiredGitHub",
    "Solarized (dark)",
];

/// Fence language tags that do not match a syntax name directly.
/// Map a fence tag to a syntax name present in the bundled set.
///
/// The default `SyntaxSet` carries 75 grammars. Tags outside it -- `ts`,
/// `dockerfile`, `toml`, `kotlin` among them -- have no entry here, and
/// correctly fall through to unstyled text rather than being highlighted
/// as the wrong language.
fn alias(lang: &str) -> &str {
    match lang {
        "sh" | "shell" | "zsh" | "bashrc" | "ksh" => "Bourne Again Shell (bash)",
        "py" | "python3" | "py3" => "Python",
        "rs" | "rust" => "Rust",
        "js" | "mjs" | "cjs" => "JavaScript",
        "golang" => "Go",
        "c++" | "cc" | "hpp" | "hxx" => "C++",
        "cs" | "csharp" => "C#",
        "yml" | "yaml" => "YAML",
        "md" | "markdown" => "Markdown",
        "make" | "makefile" => "Makefile",
        "objc" => "Objective-C",
        "rb" => "Ruby",
        "tex" | "latex" => "LaTeX",
        "html" => "HTML",
        "css" => "CSS",
        "txt" | "text" | "plain" => "Plain Text",
        _ => return lang,
    }
}

fn theme() -> &'static Theme {
    static FALLBACK: LazyLock<Theme> = LazyLock::new(|| {
        THEMES
            .themes
            .values()
            .next()
            .cloned()
            .unwrap_or_else(Theme::default)
    });
    for name in THEME_PREFERENCES {
        if let Some(t) = THEMES.themes.get(name) {
            // SAFETY-free approach: leak-free by returning a reference into
            // the `ThemeSet`, which lives for the rest of the process.
            return t;
        }
    }
    &FALLBACK
}

/// Find a syntax for a fence language tag, or `None` for plain text.
pub fn resolve_syntax(lang: &str) -> Option<&'static SyntaxReference> {
    let lang = lang.trim();
    if lang.is_empty() {
        return None;
    }
    if lang.eq_ignore_ascii_case("text") || lang.eq_ignore_ascii_case("plaintext") {
        return None;
    }
    // Cheap guard: don't pay the ~30-60ms syntax-set build for tags that
    // cannot possibly resolve.
    if !lang
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '#' | '-' | '_' | '.'))
    {
        return None;
    }
    let set = &*SYNTAXES;
    let lower = lang.to_ascii_lowercase();
    set.find_syntax_by_token(lang)
        .or_else(|| set.find_syntax_by_name(lang))
        .or_else(|| set.find_syntax_by_extension(lang))
        .or_else(|| set.find_syntax_by_token(&lower))
        .or_else(|| set.find_syntax_by_extension(&lower))
        .or_else(|| set.find_syntax_by_name(alias(&lower)))
        .or_else(|| set.find_syntax_by_extension(alias(&lower)))
        .filter(|s| s.name != "Plain Text")
}

/// Canonical sRGB values of the xterm 16 colors, used only to compute
/// which ANSI index is visually closest to a given RGB triple. The
/// terminal is free to use different values for the same index; this
/// only has to be good enough to pick a reasonable slot.
/// Perceived brightness of an RGB triple, 0-255.
fn luminance(r: u8, g: u8, b: u8) -> u32 {
    // Integer Rec. 601 luma, good enough for a slot choice.
    (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000
}

/// Map a 24-bit color to the nearest of the terminal's 16 colors.
///
/// If the nearest slot is one of the normal (non-bright) colors and the
/// source is bright, the bright variant is used instead, so a light
/// comment or a pale string does not end up as near-black on a dark
/// background.
pub fn quantize(r: u8, g: u8, b: u8) -> Color {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let chroma = max - min;
    let luma = luminance(r, g, b);

    // Near-gray text stays gray. Giving it a hue would paint ordinary
    // punctuation and brackets a colour they never had.
    if chroma < 32 {
        return Color::Ansi(if luma > 150 {
            15
        } else if luma > 60 {
            8
        } else {
            0
        });
    }

    // Nearest-colour matching in RGB is the obvious approach and the
    // wrong one: a muted theme's accents sit closer to grey than to any
    // saturated ANSI colour, so every token collapses to the same slot
    // and the block reads as unhighlighted. Hue is what carries the
    // distinction, so match on that and use lightness only to choose
    // between the dim and bright halves of the palette.
    let (rf, gf, bf) = (r as f32, g as f32, b as f32);
    let chroma = chroma as f32;
    let hue = if max == r {
        (gf - bf) / chroma
    } else if max == g {
        (bf - rf) / chroma + 2.0
    } else {
        (rf - gf) / chroma + 4.0
    }
    .rem_euclid(6.0)
        * 60.0;

    /// Hue of each of the six chromatic ANSI slots.
    const ANCHORS: [(f32, u8); 6] = [
        (0.0, 1),   // red
        (60.0, 3),  // yellow
        (120.0, 2), // green
        (180.0, 6), // cyan
        (240.0, 4), // blue
        (300.0, 5), // magenta
    ];
    let slot = ANCHORS
        .iter()
        .map(|(h, slot)| {
            let d = (hue - h).abs();
            (d.min(360.0 - d), *slot)
        })
        .fold(
            (f32::MAX, 1u8),
            |acc, cur| if cur.0 < acc.0 { cur } else { acc },
        )
        .1;

    if luma > 120 {
        Color::Ansi(slot + 8)
    } else {
        Color::Ansi(slot)
    }
}

fn syntect_style_to_ours(style: syntect::highlighting::Style) -> Style {
    let mut s = Style::default();
    // syntect 5 models both colors as plain fields, not options.
    let fg = style.foreground;
    s.fg = Some(quantize(fg.r, fg.g, fg.b));
    // A highlighted background would fight the terminal's own, so the
    // background color is deliberately dropped.
    let _bg = style.background;
    if style.font_style.contains(FontStyle::BOLD) {
        s.bold = true;
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        s.italic = true;
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        s.underline = true;
    }
    s
}

/// Highlight `code`, returning one styled run list per source line.
///
/// Falls back to unstyled runs when the language is unknown or
/// highlighting fails, so a fence always renders something legible.
pub fn highlight(code: &str, lang: &str) -> Vec<Vec<Run>> {
    let plain: Vec<Vec<Run>> = code
        .lines()
        .map(|l| vec![Run::plain(l.to_string())])
        .collect();

    let Some(syntax) = resolve_syntax(lang) else {
        return plain;
    };
    // `HighlightLines::new` is infallible in syntect 5; it only borrows
    // the syntax and theme.
    let mut highlighter = HighlightLines::new(syntax, theme());

    let mut out: Vec<Vec<Run>> = Vec::with_capacity(plain.len());
    for line in code.lines() {
        // Expand tabs first so columns line up with the gutter math.
        let line = super::width::expand_tabs(line, 0);
        // The set is loaded with `load_defaults_newlines`, so the parser
        // only closes an open construct when it sees a line terminator.
        // Without it, an unterminated `# comment` on line 1 leaves the
        // scope stack parked in `comment` and every later line in the
        // block comes back uniformly grey.
        let mut terminated = line.clone();
        terminated.push('\n');
        match highlighter.highlight_line(&terminated, &SYNTAXES) {
            Ok(regions) => {
                let runs: Vec<Run> = regions
                    .into_iter()
                    .map(|(st, text)| {
                        // Drop the terminator we added; it is not content.
                        let text = text.strip_suffix('\n').unwrap_or(text);
                        Run::new(text.to_string(), syntect_style_to_ours(st))
                    })
                    .filter(|r| !r.text.is_empty())
                    .collect();
                out.push(if runs.is_empty() {
                    vec![Run::plain("")]
                } else {
                    runs
                });
            }
            Err(_) => out.push(vec![Run::plain(line.clone())]),
        }
    }
    if out.is_empty() && !code.is_empty() {
        out.push(vec![Run::plain("")]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_aliases_resolve() {
        for lang in [
            "rust", "rs", "python", "py", "js", "json", "bash", "sh", "yaml", "md", "go", "c",
            "cpp", "java", "html", "css", "sql", "php", "ruby", "rb", "make",
        ] {
            assert!(resolve_syntax(lang).is_some(), "{lang} should resolve");
        }
    }

    #[test]
    fn every_alias_target_exists_in_the_bundled_set() {
        // Guards against the alias table drifting away from the bundled
        // SyntaxSet, which would silently disable highlighting.
        let set = &*SYNTAXES;
        for tag in [
            "sh", "shell", "py", "python3", "rs", "rust", "js", "golang", "c++", "cs", "yml",
            "markdown", "makefile", "objc", "rb", "tex", "html", "css", "txt",
        ] {
            let target = alias(tag);
            assert!(
                set.find_syntax_by_name(target).is_some(),
                "alias {tag} -> {target} is not in the bundled set"
            );
        }
    }

    #[test]
    fn unsupported_languages_degrade_to_plain_text() {
        // No TypeScript in the default set, so it must not resolve and
        // must not panic; the fence still renders as a code box.
        assert!(resolve_syntax("ts").is_none());
        assert!(resolve_syntax("dockerfile").is_none());
        assert!(highlight("const x: number = 1;", "ts").len() == 1);
    }

    #[test]
    fn text_tags_are_explicitly_plain() {
        assert!(resolve_syntax("text").is_none());
        assert!(resolve_syntax("plaintext").is_none());
        assert!(resolve_syntax("").is_none());
    }

    #[test]
    fn unknown_and_empty_languages_are_plain() {
        assert!(resolve_syntax("").is_none());
        assert!(resolve_syntax("text").is_none());
        assert!(resolve_syntax("plain").is_none());
        assert!(resolve_syntax("not-a-real-language").is_none());
    }

    #[test]
    fn absurd_tags_do_not_pay_the_syntax_set_cost() {
        // Must return without building the syntax set.
        assert!(resolve_syntax("a b; rm -rf /").is_none());
    }

    /// Every line must be highlighted in its own right. A `#` comment
    /// on the first line used to leave the parser parked in `comment`
    /// scope, flattening every later line to a single grey run.
    #[test]
    fn later_lines_are_not_swallowed_by_an_open_comment() {
        let out = highlight("# comment\ndef greet(name):\n    return 1\n", "python");
        assert_eq!(out.len(), 3);
        // The keyword line must be split into multiple scopes.
        assert!(
            out[1].len() > 1,
            "line 2 came back as one flat run: {:?}",
            out[1]
        );
        // And the comment must be grey, not keyword-coloured.
        let comment_fg = out[0][0].style.fg;
        let keyword_fg = out[1]
            .iter()
            .find(|r| r.text == "def")
            .and_then(|r| r.style.fg);
        assert_ne!(comment_fg, keyword_fg, "keyword lost its colour: {out:?}");
    }

    /// Chromatic tokens must survive the trip to 16 colours. Matching in
    /// RGB instead of on hue collapsed a whole block to grey.
    #[test]
    fn highlighting_uses_more_than_grey_and_white() {
        let src = "def f(x):\n    return \"str\" + str(1) + 2.5\n";
        let out = highlight(src, "python");
        let mut seen = std::collections::HashSet::new();
        for line in &out {
            for r in line {
                if let Some(Color::Ansi(n)) = r.style.fg {
                    seen.insert(n);
                }
            }
        }
        assert!(seen.len() >= 4, "only {seen:?} used -- colours collapsed");
    }

    #[test]
    fn quantize_lands_in_the_16_color_palette() {
        for rgb in [
            (0, 0, 0),
            (255, 255, 255),
            (205, 0, 0),
            (92, 92, 255),
            (127, 127, 127),
            (1, 2, 3),
        ] {
            assert!(matches!(quantize(rgb.0, rgb.1, rgb.2), Color::Ansi(n) if n < 16));
        }
    }

    #[test]
    fn light_colors_do_not_map_to_normal_slots() {
        // A pale gray must land on bright black or white, never the
        // near-black normal slots.
        match quantize(229, 229, 229) {
            Color::Ansi(n) => assert!(n >= 8, "pale gray mapped to dim slot {n}"),
        }
    }

    #[test]
    fn dark_colors_stay_in_normal_slots() {
        match quantize(0, 0, 0) {
            Color::Ansi(n) => assert_eq!(n, 0),
        }
    }

    #[test]
    fn highlighting_produces_one_run_list_per_line() {
        let out = highlight("fn main() {\n    let x = 1;\n}", "rust");
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn highlighting_never_exceeds_16_colors() {
        for line in highlight("fn main() { /* c */ let s = \"x\"; }", "rust") {
            for run in line {
                if let Some(fg) = run.style.fg {
                    assert!(matches!(fg, Color::Ansi(n) if n < 16), "{fg:?}");
                }
            }
        }
    }

    #[test]
    fn unknown_language_falls_back_to_plain_runs() {
        let out = highlight("hello", "definitely-not-a-language");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0][0].text, "hello");
        assert!(out[0][0].style.is_empty());
    }
}
