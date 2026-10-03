//! LaTeX to Unicode rendering for inline `$...$` and block math.
//!
//! `unicodeit` does the bulk of the symbol table. The passes around it
//! handle the constructs it does not: the operators unicodeit leaves
//! alone, and the super/subscript scripts that have no single-character
//! Unicode equivalent.

use super::style::Run;
use super::theme;
use super::width::display_width;

/// Map a character to its superscript form, if one exists.
fn to_superscript(c: char) -> Option<char> {
    match c {
        '0' => Some('\u{2070}'),
        '1' => Some('\u{b9}'),
        '2' => Some('\u{b2}'),
        '3' => Some('\u{b3}'),
        '4' => Some('\u{2074}'),
        '5' => Some('\u{2075}'),
        '6' => Some('\u{2076}'),
        '7' => Some('\u{2077}'),
        '8' => Some('\u{2078}'),
        '9' => Some('\u{2079}'),
        'a' => Some('\u{1d43}'),
        'b' => Some('\u{1d47}'),
        'c' => Some('\u{1d9c}'),
        'd' => Some('\u{1d48}'),
        'e' => Some('\u{1d49}'),
        'f' => Some('\u{1da0}'),
        'g' => Some('\u{1d4d}'),
        'h' => Some('\u{02b0}'),
        'i' => Some('\u{2071}'),
        'j' => Some('\u{02b2}'),
        'k' => Some('\u{1d4f}'),
        'l' => Some('\u{02e1}'),
        'm' => Some('\u{1d50}'),
        'n' => Some('\u{207f}'),
        'o' => Some('\u{1d52}'),
        'p' => Some('\u{1d56}'),
        'r' => Some('\u{02b3}'),
        's' => Some('\u{02e2}'),
        't' => Some('\u{1d57}'),
        'u' => Some('\u{1d58}'),
        'v' => Some('\u{1d5b}'),
        'w' => Some('\u{02b7}'),
        'x' => Some('\u{02e3}'),
        'y' => Some('\u{02b8}'),
        'z' => Some('\u{1dbb}'),
        '+' => Some('\u{207a}'),
        '-' => Some('\u{207b}'),
        '=' => Some('\u{207c}'),
        '(' => Some('\u{207d}'),
        ')' => Some('\u{207e}'),
        _ => None,
    }
}

/// Map a character to its subscript form, if one exists.
fn to_subscript(c: char) -> Option<char> {
    match c {
        '0' => Some('\u{2080}'),
        '1' => Some('\u{2081}'),
        '2' => Some('\u{2082}'),
        '3' => Some('\u{2083}'),
        '4' => Some('\u{2084}'),
        '5' => Some('\u{2085}'),
        '6' => Some('\u{2086}'),
        '7' => Some('\u{2087}'),
        '8' => Some('\u{2088}'),
        '9' => Some('\u{2089}'),
        '+' => Some('\u{208a}'),
        '-' => Some('\u{208b}'),
        '=' => Some('\u{208c}'),
        '(' => Some('\u{208d}'),
        ')' => Some('\u{208e}'),
        'a' => Some('\u{2090}'),
        'e' => Some('\u{2091}'),
        'h' => Some('\u{2095}'),
        'i' => Some('\u{1d62}'),
        'j' => Some('\u{2c7c}'),
        'k' => Some('\u{2096}'),
        'l' => Some('\u{2097}'),
        'm' => Some('\u{2098}'),
        'n' => Some('\u{2099}'),
        'o' => Some('\u{2092}'),
        'p' => Some('\u{209a}'),
        'r' => Some('\u{1d63}'),
        's' => Some('\u{209b}'),
        't' => Some('\u{209c}'),
        'u' => Some('\u{1d64}'),
        'v' => Some('\u{1d65}'),
        'x' => Some('\u{2093}'),
        _ => None,
    }
}

/// A brace group starting at `open`, returning its body and the index
/// just past the closing brace.
fn brace_group(s: &str, open: usize) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    if bytes.get(open) != Some(&b'{') {
        return None;
    }
    let mut depth = 0usize;
    for (i, b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((s[open + 1..i].to_string(), i + 1));
                }
            }
            _ => {}
        }
    }
    None
}

/// Convert `^{...}` / `^x` and `_{...}` / `_x` to Unicode scripts.
fn convert_scripts(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < s.len() {
        let c = bytes[i];
        if c == b'^' || c == b'_' {
            let is_super = c == b'^';
            let next = i + 1;
            if next < s.len() {
                if bytes[next] == b'{' {
                    if let Some((body, end)) = brace_group(s, next) {
                        out.push_str(&script_body(&body, is_super));
                        i = end;
                        continue;
                    }
                } else {
                    let ch = s[next..].chars().next().unwrap();
                    if let Some(sc) = single_script(ch, is_super) {
                        out.push(sc);
                        i = next + ch.len_utf8();
                        continue;
                    }
                }
            }
        }
        // Not a script: copy the whole character.
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn single_script(c: char, is_super: bool) -> Option<char> {
    if is_super {
        to_superscript(c)
    } else {
        to_subscript(c)
    }
}

fn script_body(body: &str, is_super: bool) -> String {
    let mut out = String::with_capacity(body.len());
    for ch in body.chars() {
        match single_script(ch, is_super) {
            Some(sc) => out.push(sc),
            None => out.push(ch),
        }
    }
    out
}

/// Pre-pass: rewrite the operators unicodeit leaves in place.
fn pre_rewrite(src: &str) -> String {
    let mut s = src.to_string();
    // \frac{A}{B} -> A⁄B, before the symbol table can mangle the braces.
    s = replace_command_two_args(&s, "\\frac", '\u{2044}');
    s = replace_command_two_args(&s, "\\binom", '\u{1d3d}');
    // \sqrt{X} -> √X
    s = replace_command_braced(&s, "\\sqrt", '\u{221a}');
    // Size and spacing commands carry no meaning in a text terminal.
    for cmd in [
        "\\left",
        "\\right",
        "\\displaystyle",
        "\\textstyle",
        "\\quad",
        "\\qquad",
        "\\limits",
        "\\mathrm",
        "\\mathbf",
        "\\mathit",
        "\\mathcal",
        "\\mathbb",
        "\\operatorname",
        "\\textbf",
        "\\textit",
        "\\rm",
        "\\bf",
        "\\it",
    ] {
        s = s.replace(cmd, "");
    }
    s = s
        .replace("\\,", " ")
        .replace("\\;", " ")
        .replace("\\:", " ")
        .replace("\\!", "");
    s = s.replace("~", " ");
    s
}

fn replace_command_two_args(src: &str, cmd: &str, joiner: char) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(pos) = rest.find(cmd) {
        let after = &rest[pos + cmd.len()..];
        let Some((a, end_a)) = first_arg(after) else {
            out.push_str(&rest[..pos + cmd.len()]);
            rest = after;
            continue;
        };
        let Some((b, end_b)) = first_arg(&after[end_a..]) else {
            out.push_str(&rest[..pos + cmd.len()]);
            rest = after;
            continue;
        };
        out.push_str(&rest[..pos]);
        out.push_str(&a);
        out.push(joiner);
        out.push_str(&b);
        rest = &after[end_a + end_b..];
    }
    out.push_str(rest);
    out
}

fn replace_command_braced(src: &str, cmd: &str, symbol: char) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(pos) = rest.find(cmd) {
        let after = &rest[pos + cmd.len()..];
        out.push_str(&rest[..pos]);
        out.push(symbol);
        if let Some((body, end)) = brace_group(after, 0) {
            out.push_str(&body);
            rest = &after[end..];
        } else {
            // \sqrt x - a single bare argument.
            if let Some(ch) = after.chars().next() {
                out.push(ch);
                rest = &after[ch.len_utf8()..];
            } else {
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The first LaTeX argument after a command: a brace group, or a single
/// character.
fn first_arg(s: &str) -> Option<(String, usize)> {
    if s.is_empty() {
        return None;
    }
    if s.as_bytes()[0] == b'{' {
        return brace_group(s, 0);
    }
    let ch = s.chars().next()?;
    Some((ch.to_string(), ch.len_utf8()))
}

/// Convert a LaTeX fragment to Unicode.
/// Array environments, with the bracket pair each one implies.
const ARRAYS: [(&str, char, char); 5] = [
    ("bmatrix", '[', ']'),
    ("pmatrix", '(', ')'),
    ("Bmatrix", '{', '}'),
    ("vmatrix", '|', '|'),
    ("matrix", ' ', ' '),
];

/// Split a source into `(is_matrix, chunk)` parts.
///
/// `unicodeit` maps tokens, not layouts, so an array environment has no
/// Unicode form and used to reach the terminal as raw `\begin{bmatrix}`.
fn split_arrays(src: &str) -> Vec<(bool, String, String)> {
    let mut out = Vec::new();
    let mut rest = src;
    loop {
        let Some(start) = rest.find("\\begin{") else {
            if !rest.trim().is_empty() {
                out.push((false, rest.to_string(), String::new()));
            }
            return out;
        };
        let open_at = start + r"\begin{".len();
        let Some(brace) = rest[open_at..].find('}').map(|i| i + open_at) else {
            out.push((false, rest.to_string(), String::new()));
            return out;
        };
        let name = &rest[open_at..brace];
        if !ARRAYS.iter().any(|(e, _, _)| *e == name) {
            let skip = brace + 1;
            if !rest[..skip].trim().is_empty() {
                out.push((false, rest[..skip].to_string(), String::new()));
            }
            rest = &rest[skip..];
            continue;
        }
        let body_at = brace + 1;
        let marker = format!("\\end{{{name}}}");
        let Some(end) = rest[body_at..].find(&marker).map(|i| i + body_at) else {
            out.push((false, rest.to_string(), String::new()));
            return out;
        };
        if !rest[..start].trim().is_empty() {
            out.push((false, rest[..start].to_string(), String::new()));
        }
        out.push((true, rest[body_at..end].to_string(), name.to_string()));
        rest = &rest[end + marker.len()..];
    }
}

/// Lay out one array environment as a bracketed grid.
fn render_array(body: &str, env: &str) -> Vec<String> {
    let rows: Vec<Vec<String>> = body
        .split("\\\\")
        .map(|r| {
            r.split('&')
                .map(|c| to_unicode(c).trim().to_string())
                .collect::<Vec<String>>()
        })
        .filter(|r| !r.iter().all(|c| c.is_empty()))
        .collect();
    if rows.is_empty() {
        return Vec::new();
    }
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let widths: Vec<usize> = (0..cols)
        .map(|c| {
            rows.iter()
                .filter_map(|r| r.get(c))
                .map(|cell| display_width(cell))
                .max()
                .unwrap_or(0)
        })
        .collect();

    let (open, close) = ARRAYS
        .iter()
        .find(|(e, _, _)| *e == env)
        .map(|(_, o, c)| (*o, *c))
        .unwrap_or(('[', ']'));

    let n = rows.len();
    (0..n)
        .map(|i| {
            let body: Vec<String> = (0..cols)
                .map(|c| {
                    let cell = rows[i].get(c).cloned().unwrap_or_default();
                    let pad = widths[c].saturating_sub(display_width(&cell));
                    format!("{cell}{}", " ".repeat(pad))
                })
                .collect();
            let inner = body.join(" ");
            // A single row keeps the environment's own brackets; several
            // rows get the stretched forms that read as one block.
            match (i, n) {
                (0, 1) => format!("{open}{inner}{close}"),
                (0, _) => format!("\u{23a1}{inner}\u{23a4}"),
                (last, _) if last == n - 1 => format!("\u{23a3}{inner}\u{23a6}"),
                _ => format!("\u{23a2}{inner}\u{23a5}"),
            }
        })
        .collect()
}

/// Drop array delimiters so an inline or unhandled environment never
/// shows its LaTeX scaffolding.
fn strip_array_markers(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(at) = rest.find("\\begin{") {
        out.push_str(&rest[..at]);
        let open_at = at + r"\begin{".len();
        match rest[open_at..].find('}') {
            Some(i) => {
                let brace = open_at + i;
                let name = rest[open_at..brace].to_string();
                let marker = format!("\\end{{{name}}}");
                let body_at = brace + 1;
                match rest[body_at..].find(&marker) {
                    Some(j) => {
                        out.push_str(&rest[body_at..body_at + j]);
                        rest = &rest[body_at + j + marker.len()..];
                    }
                    None => {
                        out.push_str(&rest[at..]);
                        return out;
                    }
                }
            }
            None => {
                out.push_str(&rest[at..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn to_unicode(src: &str) -> String {
    let pre = strip_array_markers(&pre_rewrite(src));
    let converted = unicodeit::replace(&pre);
    let scripted = convert_scripts(&converted);
    scripted
        .replace("{}", "")
        .replace('$', "")
        .trim()
        .to_string()
}

/// Styled runs for inline `$...$` math.
pub fn inline_runs(src: &str) -> Vec<Run> {
    let converted = to_unicode(src);
    if converted.is_empty() {
        return Vec::new();
    }
    vec![Run::new(converted, theme::latex())]
}

/// Rendered lines for a display-math block.
pub fn block_lines(src: &str, render_width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for (is_array, chunk, env) in split_arrays(src) {
        if is_array {
            out.extend(render_array(&chunk, &env));
            continue;
        }
        for line in to_unicode(&chunk).lines() {
            if line.trim().is_empty() {
                continue;
            }
            if display_width(line) <= render_width {
                out.push(line.to_string());
            } else {
                out.extend(
                    super::wrap::hard_wrap(&[Run::plain(line.to_string())], render_width)
                        .into_iter()
                        .map(|r| r.into_iter().map(|x| x.text).collect::<String>()),
                );
            }
        }
    }
    out
}

/// True when a fence tag names a LaTeX block.
pub fn is_latex_lang(lang: &str) -> bool {
    matches!(lang.trim().to_ascii_lowercase().as_str(), "latex" | "tex")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_symbols_convert() {
        let out = to_unicode(r"\alpha + \beta");
        assert!(out.contains('\u{3b1}'), "alpha missing: {out:?}");
        assert!(out.contains('\u{3b2}'), "beta missing: {out:?}");
    }

    #[test]
    fn fractions_become_a_fraction_slash() {
        let out = to_unicode(r"\frac{1}{2}");
        assert!(out.contains('\u{2044}'), "no fraction slash: {out:?}");
        assert!(!out.contains("frac"), "leftover command: {out:?}");
    }

    /// Array environments have no single-token Unicode form, so they are
    /// laid out as a grid. They used to reach the terminal as raw
    /// `\begin{bmatrix}`.
    #[test]
    fn a_matrix_becomes_a_bracketed_grid() {
        let src =
            "A =\n\\begin{bmatrix}\n1 & 2 & 3 \\\\\n4 & 5 & 6 \\\\\n7 & 8 & 9\n\\end{bmatrix}";
        let out = block_lines(src, 80);
        let joined = out.join("\n");
        assert!(!joined.contains("begin{"), "raw latex leaked: {joined:?}");
        assert!(
            !joined.contains("\\\\"),
            "raw row separator leaked: {joined:?}"
        );
        assert_eq!(out.len(), 4, "expected `A =` plus three rows: {out:?}");
        assert!(out[1].starts_with('\u{23a1}') && out[1].ends_with('\u{23a4}'));
        assert!(out[2].starts_with('\u{23a2}') && out[2].ends_with('\u{23a5}'));
        assert!(out[3].starts_with('\u{23a3}') && out[3].ends_with('\u{23a6}'));
    }

    /// Cells are padded to a common width, or the grid looks ragged.
    #[test]
    fn matrix_cells_are_padded_to_a_common_width() {
        let src = "\\begin{bmatrix}1 & 22 \\\\ 333 & 4\\end{bmatrix}";
        let out = block_lines(src, 80);
        assert_eq!(out.len(), 2, "{out:?}");
        assert_eq!(
            out[0].chars().count(),
            out[1].chars().count(),
            "rows differ: {out:?}"
        );
    }

    #[test]
    fn roots_are_prefixed() {
        let out = to_unicode(r"\sqrt{2}");
        assert!(out.starts_with('\u{221a}'), "no root sign: {out:?}");
    }

    #[test]
    fn superscripts_convert() {
        let out = to_unicode("x^{2}");
        assert!(out.contains('\u{b2}'), "no superscript: {out:?}");
    }

    #[test]
    fn subscripts_convert() {
        let out = to_unicode("x_{1}");
        assert!(out.contains('\u{2081}'), "no subscript: {out:?}");
    }

    #[test]
    fn script_groups_handle_multiple_characters() {
        let out = to_unicode("x^{12}");
        assert!(out.contains('\u{b9}') && out.contains('\u{b2}'), "{out:?}");
    }

    #[test]
    fn dollar_signs_are_stripped() {
        assert!(!to_unicode("$x$").contains('$'));
    }

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(to_unicode("just words"), "just words");
    }

    #[test]
    fn empty_input_is_empty() {
        assert_eq!(to_unicode(""), "");
    }

    #[test]
    fn block_lines_drop_blanks() {
        let out = block_lines("a\n\n\nb", 80);
        assert_eq!(out, vec!["a", "b"]);
    }

    #[test]
    fn latex_language_tags_are_recognized() {
        assert!(is_latex_lang("latex"));
        assert!(is_latex_lang("TEX"));
        assert!(!is_latex_lang("rust"));
    }
}
