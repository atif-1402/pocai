//! List markers, indentation, and continuation alignment.

use super::style::{Run, Style};
use super::theme;
use super::width::display_width;

/// Columns of indent added per nesting level.
const INDENT_PER_LEVEL: usize = 2;

/// Checkbox glyphs for task list items. All exactly two columns wide so
/// they align with the `• ` and `1. ` markers.
const TASK_CHECKED: &str = "\u{2611} ";
const TASK_UNCHECKED: &str = "\u{2610} ";

/// What kind of list we're inside, which selects the marker glyph and
/// the numbering source.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ListKind {
    Unordered,
    /// An ordered list, carrying the number the source started at.
    Ordered(u64),
}

impl ListKind {
    /// The next item's number, for an ordered list.
    pub fn next_number(&mut self) -> Option<u64> {
        match self {
            ListKind::Ordered(n) => {
                let current = *n;
                *n += 1;
                Some(current)
            }
            ListKind::Unordered => None,
        }
    }
}

/// Per-item layout state, needed because the continuation indent depends
/// on the marker width, which is only known once the marker is emitted.
#[derive(Clone, Debug)]
pub struct ItemState {
    pub depth: usize,
    pub checkbox: Option<bool>,
    pub marker_emitted: bool,
    pub continuation_indent: usize,
    pub list_kind: ListKind,
}

impl ItemState {
    pub fn new(depth: usize, list_kind: ListKind, checkbox: Option<bool>) -> Self {
        ItemState {
            depth,
            checkbox,
            marker_emitted: false,
            continuation_indent: 0,
            list_kind,
        }
    }

    /// The marker for this item: `(text, style, width)`.
    pub fn marker(&mut self) -> (String, Style, usize) {
        let (text, style) = match self.checkbox {
            Some(true) => (TASK_CHECKED.to_string(), theme::task_marker(true)),
            Some(false) => (TASK_UNCHECKED.to_string(), theme::task_marker(false)),
            None => match self.list_kind {
                ListKind::Unordered => {
                    let glyph = match self.depth {
                        1 => "\u{2022} ",
                        2 => "\u{25e6} ",
                        _ => "\u{25b8} ",
                    };
                    (glyph.to_string(), theme::bullet(self.depth))
                }
                ListKind::Ordered(_) => {
                    let n = self.list_kind.next_number().unwrap_or(1);
                    (format!("{n}. "), theme::ordered_marker())
                }
            },
        };
        let width = display_width(&text);
        self.continuation_indent = INDENT_PER_LEVEL * self.depth.saturating_sub(1) + width;
        self.marker_emitted = true;
        (text, style, width)
    }
}

/// Indentation before an item's marker.
pub fn indent_prefix(depth: usize) -> Vec<Run> {
    if depth <= 1 {
        return Vec::new();
    }
    vec![Run::plain(" ".repeat(INDENT_PER_LEVEL * (depth - 1)))]
}

/// Blank columns aligning a wrapped or subsequent line under an item's
/// text rather than its marker.
///
/// Equal to the marker column count, so `- one` wraps to
/// `  continued` and `10. ten` wraps to `    continued`.
pub fn continuation_prefix(item: &ItemState) -> Vec<Run> {
    if item.continuation_indent == 0 {
        return Vec::new();
    }
    vec![Run::plain(" ".repeat(item.continuation_indent))]
}

/// The first-line prefix for an item: indent plus, on the first line
/// only, the marker itself.
pub fn first_line_prefix(item: &mut ItemState) -> Vec<Run> {
    let mut runs = indent_prefix(item.depth);
    if !item.marker_emitted {
        let (text, style, _) = item.marker();
        runs.push(Run::new(text, style));
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unordered_markers_change_with_depth() {
        let mut a = ItemState::new(1, ListKind::Unordered, None);
        let mut b = ItemState::new(2, ListKind::Unordered, None);
        let mut c = ItemState::new(3, ListKind::Unordered, None);
        assert_eq!(a.marker().0, "\u{2022} ");
        assert_eq!(b.marker().0, "\u{25e6} ");
        assert_eq!(c.marker().0, "\u{25b8} ");
    }

    #[test]
    fn ordered_markers_start_from_the_source_number() {
        let mut item = ItemState::new(1, ListKind::Ordered(7), None);
        assert_eq!(item.marker().0, "7. ");
        assert_eq!(item.marker().0, "8. ");
    }

    #[test]
    fn task_markers_use_checkboxes() {
        let mut yes = ItemState::new(1, ListKind::Unordered, Some(true));
        let mut no = ItemState::new(1, ListKind::Unordered, Some(false));
        assert_eq!(yes.marker().0, "\u{2611} ");
        assert_eq!(no.marker().0, "\u{2610} ");
    }

    #[test]
    fn unordered_markers_are_two_columns_wide() {
        // Bullets and checkboxes are deliberately the same width so
        // nested items line up whichever glyph is used.
        let cases = [
            ItemState::new(1, ListKind::Unordered, None),
            ItemState::new(2, ListKind::Unordered, None),
            ItemState::new(3, ListKind::Unordered, None),
            ItemState::new(1, ListKind::Unordered, Some(true)),
            ItemState::new(1, ListKind::Unordered, Some(false)),
        ];
        for mut c in cases {
            assert_eq!(c.marker().2, 2, "marker must be two columns");
        }
    }

    #[test]
    fn ordered_markers_are_as_wide_as_their_number() {
        // "1. " is three columns; the continuation indent has to grow
        // with it or wrapped text would sit under the digit.
        assert_eq!(ItemState::new(1, ListKind::Ordered(1), None).marker().2, 3);
        assert_eq!(ItemState::new(1, ListKind::Ordered(9), None).marker().2, 3);
        assert_eq!(ItemState::new(1, ListKind::Ordered(10), None).marker().2, 4);
    }

    #[test]
    fn continuation_indent_accounts_for_marker_width() {
        let mut bullet = ItemState::new(1, ListKind::Unordered, None);
        bullet.marker();
        assert_eq!(bullet.continuation_indent, 2);

        let mut ordered = ItemState::new(1, ListKind::Ordered(1), None);
        ordered.marker();
        assert_eq!(ordered.continuation_indent, 3, "'1. ' is three columns");

        let mut deep = ItemState::new(2, ListKind::Ordered(1), None);
        deep.marker();
        assert_eq!(deep.continuation_indent, 5, "indent 2 plus marker 3");
    }

    #[test]
    fn marker_is_emitted_only_once() {
        let mut item = ItemState::new(1, ListKind::Unordered, None);
        let first = first_line_prefix(&mut item);
        let second = first_line_prefix(&mut item);
        assert_eq!(first[0].text, "\u{2022} ");
        assert!(second.is_empty(), "marker must not repeat");
    }

    #[test]
    fn nested_indent_is_two_columns_per_level() {
        assert_eq!(indent_prefix(1).len(), 0);
        assert_eq!(indent_prefix(2)[0].text, "  ");
        assert_eq!(indent_prefix(4)[0].text, "      ");
    }
}
