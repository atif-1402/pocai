//! Mermaid diagrams drawn directly into a character grid.
//!
//! Two families are supported: flowcharts (`graph`/`flowchart` in `TD`/
//! `LR` orientation) and sequence diagrams. Anything else reports
//! `None` so the caller can fall back to a highlighted code block rather
//! than print something wrong.

use super::style::{Line, Run, Style};
use super::theme;
use super::width::display_width;

/// A character plus the style it was drawn with.
#[derive(Clone, Debug)]
struct Cell {
    ch: char,
    style: Style,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            style: Style::default(),
        }
    }
}

/// A growable character grid that diagram primitives draw into.
#[derive(Clone, Debug, Default)]
struct Canvas {
    cells: Vec<Vec<Cell>>,
    width: usize,
    height: usize,
}

impl Canvas {
    fn ensure(&mut self, x: usize, y: usize) {
        if y + 1 > self.height {
            self.resize(y + 1, self.width);
        }
        if x + 1 > self.width {
            self.resize(self.height, x + 1);
        }
    }

    fn resize(&mut self, height: usize, width: usize) {
        let width = width.max(self.width);
        while self.cells.len() < height {
            self.cells.push(vec![Cell::default(); width]);
        }
        for row in &mut self.cells {
            while row.len() < width {
                row.push(Cell::default());
            }
        }
        self.height = self.height.max(height);
        self.width = self.width.max(width);
    }

    fn put(&mut self, x: usize, y: usize, ch: char, style: Style) {
        self.ensure(x, y);
        self.cells[y][x] = Cell { ch, style };
    }

    fn hline(&mut self, x: usize, y: usize, len: usize, ch: char, style: Style) {
        for i in 0..len {
            self.put(x + i, y, ch, style);
        }
    }

    fn vline(&mut self, x: usize, y: usize, len: usize, ch: char, style: Style) {
        for i in 0..len {
            self.put(x, y + i, ch, style);
        }
    }

    /// Draw a string, clipped to the canvas.
    fn text(&mut self, x: usize, y: usize, s: &str, style: Style) {
        for (i, ch) in s.chars().enumerate() {
            self.put(x + i, y, ch, style);
        }
    }

    fn into_lines(self) -> Vec<Line> {
        let mut out = Vec::new();
        for row in &self.cells {
            // Trim trailing blanks so rows are as short as their content.
            let end = row
                .iter()
                .rposition(|c| c.ch != ' ')
                .map(|i| i + 1)
                .unwrap_or(0);
            let mut runs: Vec<Run> = Vec::new();
            for cell in &row[..end] {
                runs.push(Run::new(cell.ch.to_string(), cell.style));
            }
            out.push(Line::from_runs(runs));
        }
        // Drop fully blank lines from the top and bottom, but never
        // return an empty diagram.
        while out
            .first()
            .map(|l| l.runs.iter().all(|r| r.text.trim().is_empty()))
            .unwrap_or(false)
        {
            out.remove(0);
        }
        while out
            .last()
            .map(|l| l.runs.iter().all(|r| r.text.trim().is_empty()))
            .unwrap_or(false)
        {
            out.pop();
        }
        if out.is_empty() {
            out.push(Line::from_runs(vec![Run::plain("".to_string())]));
        }
        out
    }
}

/// Box-drawing pieces.
const TL: &str = "\u{250c}"; // ┌
const TR: &str = "\u{2510}"; // ┐
const BL: &str = "\u{2514}"; // └
const BR: &str = "\u{2518}"; // ┘
const H: &str = "\u{2500}"; // ─
const V: &str = "\u{2502}"; // │
const DOWN: &str = "\u{25bc}"; // ▼
const RIGHT: &str = "\u{25b6}"; // ▶
const LEFT: &str = "\u{25c0}"; // ◀
/// Columns of offset below which an elbow is drawn as a straight line.
const ELBOW_SNAP: usize = 2;
const H_DASH: &str = "\u{2504}"; // ┄
const V_DASH: &str = "\u{2506}"; // ┆
const TL_T: &str = "\u{251c}"; // ├
const TR_T: &str = "\u{2524}"; // ┤
const T_DOWN: &str = "\u{252c}"; // ┬
const T_UP: &str = "\u{2534}"; // ┴
const TD_X: &str = "\u{253c}"; // ┼

/// A node in a flowchart.
#[derive(Clone, Debug)]
struct FlowNode {
    id: String,
    label: String,
    shape: Shape,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shape {
    Rect,
    Round,
    Diamond,
    Stadium,
}

/// An edge in a flowchart.
#[derive(Clone, Debug)]
struct FlowEdge {
    from: String,
    to: String,
    label: Option<String>,
    style: EdgeStyle,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EdgeStyle {
    Solid,
    Dashed,
    Thick,
}

/// A parsed flowchart.
#[derive(Clone, Debug, Default)]
struct Flowchart {
    nodes: Vec<FlowNode>,
    edges: Vec<FlowEdge>,
}

/// Parse a flowchart body, returning every node and edge.
fn parse_flow(body: &str) -> Flowchart {
    let mut flow = Flowchart::default();
    for raw in body.lines() {
        let line = raw.split("%%").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        for piece in split_statements(line) {
            parse_statement(&piece, &mut flow);
        }
    }
    flow
}

/// Split a line into statements on `;` or on whitespace, never inside a
/// quoted label or a bracketed node shape -- `A[x y z]` is one node.
fn split_top_level(line: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    let mut in_quote = false;
    for ch in line.chars() {
        match ch {
            '"' | '\'' if depth == 0 => {
                in_quote = !in_quote;
                cur.push(ch);
            }
            '[' | '(' | '{' if !in_quote => {
                depth += 1;
                cur.push(ch);
            }
            ']' | ')' | '}' if !in_quote => {
                depth -= 1;
                cur.push(ch);
            }
            c if c == sep && depth == 0 && !in_quote => {
                out.push(cur.trim().to_string());
                cur = String::new();
            }
            _ => cur.push(ch),
        }
    }
    out.push(cur.trim().to_string());
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

/// Split a line on `;` into statements.
fn split_statements(line: &str) -> Vec<String> {
    split_top_level(line, ';')
}

fn parse_statement(stmt: &str, flow: &mut Flowchart) {
    if let Some(chain) = parse_edge(stmt) {
        // `A[Start] --> B[End]` declares both nodes on the edge line, so
        // the endpoints are parsed as nodes first and their labels kept.
        // A chain like `A --> B --> C` is several edges sharing an
        // endpoint, so every hop is registered, not just the first.
        for parts in &chain {
            for raw in [&parts.from_raw, &parts.to_raw] {
                if let Some(node) = parse_node(raw) {
                    upsert_node(flow, node);
                }
            }
            ensure_node(flow, &parts.edge.from);
            ensure_node(flow, &parts.edge.to);
            flow.edges.push(parts.edge.clone());
        }
        return;
    }
    // Not an edge, so this is one or more bare node declarations. Mermaid
    // allows them to sit next to each other: `A[one] B[two]`.
    for piece in split_top_level(stmt, ' ') {
        if let Some(node) = parse_node(&piece) {
            upsert_node(flow, node);
        }
    }
}

/// An edge plus the raw text of each endpoint, so that a label written
/// inline (`A[Start] --> B`) is not lost.
#[derive(Clone)]
struct EdgeParts {
    edge: FlowEdge,
    from_raw: String,
    to_raw: String,
}

fn ensure_node(flow: &mut Flowchart, id: &str) {
    if !flow.nodes.iter().any(|n| n.id == id) {
        flow.nodes.push(FlowNode {
            id: id.to_string(),
            label: id.to_string(),
            shape: Shape::Rect,
        });
    }
}

fn upsert_node(flow: &mut Flowchart, node: FlowNode) {
    if let Some(existing) = flow.nodes.iter_mut().find(|n| n.id == node.id) {
        // A bare id repeated as an edge endpoint must not erase a label.
        if existing.label == existing.id {
            existing.label = node.label;
            existing.shape = node.shape;
        }
        return;
    }
    flow.nodes.push(node);
}

/// Parse `A[Label]` and its shape variants.
fn parse_node(s: &str) -> Option<FlowNode> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    if i == 0 || i >= s.len() {
        return None;
    }
    let id = s[..i].to_string();
    let rest = &s[i..];
    let (label, shape) = match rest.as_bytes().first() {
        Some(b'[') => {
            if rest.starts_with("[[") {
                (trim_wrapper(&rest[2..], "]]")?, Shape::Stadium)
            } else {
                (trim_wrapper(&rest[1..], "]")?, Shape::Rect)
            }
        }
        Some(b'(') => {
            if rest.starts_with("((") {
                (trim_wrapper(&rest[2..], "))")?, Shape::Diamond)
            } else {
                (trim_wrapper(&rest[1..], ")")?, Shape::Round)
            }
        }
        Some(b'{') => (trim_wrapper(&rest[1..], "}")?, Shape::Diamond),
        Some(b'>') => (trim_wrapper(&rest[1..], "]")?, Shape::Stadium),
        _ => return None,
    };
    Some(FlowNode {
        id,
        label: unquote(&label),
        shape,
    })
}

/// Strip a closing suffix, returning the body.
fn trim_wrapper(s: &str, close: &str) -> Option<String> {
    let s = s.trim_start();
    let end = s.len().checked_sub(close.len())?;
    if !s[end..].starts_with(close) {
        return None;
    }
    Some(s[..end].trim().to_string())
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// Parse an edge: `A --> B`, `A -- text --> B`, `A -->|text| B`.
/// Byte spans of every edge arrow in `s`, with its style.
fn arrow_spans(s: &str) -> Vec<(usize, usize, EdgeStyle)> {
    // Longest first so `-->` is not read as `--` + `>`.
    const KINDS: [(&str, EdgeStyle); 9] = [
        ("<-->", EdgeStyle::Dashed),
        ("-.->", EdgeStyle::Dashed),
        ("--->", EdgeStyle::Solid),
        ("==>", EdgeStyle::Thick),
        ("-->", EdgeStyle::Solid),
        ("---", EdgeStyle::Solid),
        ("===", EdgeStyle::Thick),
        ("->>", EdgeStyle::Solid),
        ("-->>", EdgeStyle::Dashed),
    ];
    let mut out = Vec::new();
    for (arrow, style) in KINDS {
        let mut from = 0usize;
        while from + arrow.len() <= s.len() {
            let Some(at) = s[from..].find(arrow) else {
                break;
            };
            let pos = from + at;
            // Skip an arrow that is really the tail of a longer one, so
            // `--->` is not also reported as `--` + `->`.
            if out
                .iter()
                .any(|(p, l, _)| *p <= pos && pos + arrow.len() <= *p + *l)
            {
                from = pos + arrow.len();
                continue;
            }
            out.push((pos, arrow.len(), style));
            from = pos + arrow.len();
        }
    }
    out.sort_by_key(|(p, _, _)| *p);
    out
}

/// Every edge expressed by one statement.
///
/// `A --> B --> C` is two edges, not one edge from `A` to `B` with `C`
/// glued onto the end. Reading only the first arrow dropped every node
/// past the second, so a five-stage pipeline rendered as two boxes.
fn parse_edge(s: &str) -> Option<Vec<EdgeParts>> {
    let arrows = arrow_spans(s);
    let mut parts: Vec<EdgeParts> = Vec::new();
    let mut prev_raw = s[..arrows.first()?.0].trim().to_string();

    for (i, (pos, len, style)) in arrows.iter().enumerate() {
        let after_start = pos + len;
        let end = arrows.get(i + 1).map_or(s.len(), |(p, _, _)| *p);
        let seg = &s[after_start..end];

        // `-->|label|` writes the label immediately after the arrow.
        let mut label = match seg.trim_start().strip_prefix('|') {
            Some(rest) => match rest.find('|') {
                Some(e) => Some(unquote(&rest[..e])),
                None => None,
            },
            None => None,
        };
        let target = match seg.trim_start().strip_prefix('|') {
            Some(rest) => match rest.find('|') {
                Some(e) => rest[e + 1..].trim(),
                None => seg.trim(),
            },
            None => seg.trim(),
        };

        // `A -- label --> B` writes it just before the arrow instead,
        // which on a chain means trailing the previous node.
        let mut from_raw = std::mem::take(&mut prev_raw);
        if label.is_none() {
            if let Some(sp) = from_raw.rfind("-- ") {
                let candidate = from_raw[..sp].trim();
                if !candidate.is_empty() && !looks_like_node_id(candidate) {
                    label = Some(candidate.to_string());
                    from_raw = from_raw[sp + 3..].trim().to_string();
                }
            }
        }
        // An incoming arrow on the left.
        from_raw = from_raw
            .strip_suffix('<')
            .unwrap_or(&from_raw)
            .trim()
            .to_string();

        let to_raw = target.to_string();
        let from = strip_node(&from_raw)?;
        let to = strip_node(&to_raw)?;
        if from.is_empty() || to.is_empty() {
            return None;
        }
        parts.push(EdgeParts {
            edge: FlowEdge {
                from,
                to,
                label,
                style: *style,
            },
            from_raw,
            to_raw: to_raw.clone(),
        });
        prev_raw = to_raw;
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

fn looks_like_node_id(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// Reduce `A[Label]` to `A`.
fn strip_node(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(id) = parse_node(s) {
        return Some(id.id);
    }
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    if i == 0 {
        None
    } else {
        Some(s[..i].to_string())
    }
}

/// Longest-path depth for every node, tolerating cycles.
fn depths(flow: &Flowchart) -> std::collections::HashMap<String, usize> {
    let mut has_in = std::collections::HashSet::new();
    for e in &flow.edges {
        has_in.insert(e.to.as_str());
    }

    let mut depth: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut queue: std::collections::VecDeque<(usize, &str)> = std::collections::VecDeque::new();
    for n in &flow.nodes {
        if !has_in.contains(n.id.as_str()) {
            depth.insert(n.id.clone(), 0);
            queue.push_back((0, &n.id));
        }
    }

    // Longest-path relaxation is undefined once the graph has a cycle, and
    // a back edge makes it diverge: `F --> B` kept pushing B one row
    // further down on every pass until the cap bound it, which scattered
    // the whole chart over dozens of rows and pushed most nodes out of
    // the canvas. Walking a spanning tree instead fixes each node's row
    // on first arrival, so a loop cannot move anything.
    while let Some((d, id)) = queue.pop_front() {
        for e in &flow.edges {
            if e.from == id && !depth.contains_key(&e.to) {
                depth.insert(e.to.clone(), d + 1);
                queue.push_back((d + 1, &e.to));
            }
        }
    }

    // A component with no root at all -- a closed loop -- starts at the top.
    for n in &flow.nodes {
        depth.entry(n.id.clone()).or_insert(0);
    }
    depth
}

/// Node box width including borders.
fn node_box_width(node: &FlowNode) -> usize {
    // Diamonds draw as boxes here, so every shape gets the same padding;
    // a diamond reserving extra room would only look like a typo.
    let w = display_width(&node.label).max(1);
    w + 4
}

/// Render a flowchart with nodes stacked in levels.
fn render_flowchart(flow: &Flowchart, horizontal: bool) -> Vec<Line> {
    if flow.nodes.is_empty() {
        return Vec::new();
    }
    let depth = depths(flow);

    // Group node indices by level, preserving declaration order.
    let max_level = flow.nodes.iter().map(|n| depth[&n.id]).max().unwrap_or(0);
    let mut levels: Vec<Vec<usize>> = vec![Vec::new(); max_level + 1];
    for (i, n) in flow.nodes.iter().enumerate() {
        levels[depth[&n.id]].push(i);
    }

    let gap = 4usize; // space between sibling boxes

    if !horizontal {
        // Top-to-bottom: each level is a row of boxes.
        let widths: Vec<Vec<usize>> = levels
            .iter()
            .map(|lvl| {
                lvl.iter()
                    .map(|&i| node_box_width(&flow.nodes[i]))
                    .collect()
            })
            .collect();
        let row_widths: Vec<usize> = widths
            .iter()
            .map(|ws| ws.iter().sum::<usize>() + gap * ws.len().saturating_sub(1))
            .collect();
        // Back edges need a clear column to run up, one each so two loops
        // stay distinguishable. Acyclic graphs reserve nothing, so their
        // width is untouched.
        let back_edges: Vec<&FlowEdge> = flow
            .edges
            .iter()
            .filter(|e| depth[&e.to] < depth[&e.from])
            .collect();
        let margin = if back_edges.is_empty() {
            0
        } else {
            back_edges.len() + 1
        };
        let content_width = row_widths.iter().copied().max().unwrap_or(1).max(3);
        let canvas_width = content_width + 2 + margin;
        // Spines live in the reserved strip, working inwards.
        let first_spine = canvas_width - 1 - back_edges.len();

        let mut c = Canvas::default();
        // Box top-left positions per node.
        let mut pos: std::collections::HashMap<usize, (usize, usize)> =
            std::collections::HashMap::new();
        let mut y = 0usize;
        let mut level_y: Vec<usize> = Vec::new();
        for (li, lvl) in levels.iter().enumerate() {
            level_y.push(y);
            let inner = row_widths[li];
            let offset = (content_width.saturating_sub(inner)) / 2;
            let mut x = 1 + offset;
            for (k, &ni) in lvl.iter().enumerate() {
                pos.insert(ni, (x, y));
                x += widths[li][k];
                if k + 1 < lvl.len() {
                    x += gap;
                }
            }
            y += 3; // box height
            if li + 1 < levels.len() {
                y += 3; // connector band
            }
        }

        // Back edges go down first so boxes are drawn over them: a line
        // that clips a sibling then reads as passing behind it, which is
        // how mermaid routes loops.
        for (n, e) in back_edges.iter().enumerate() {
            let (Some(fi), Some(ti)) = (
                flow.nodes.iter().position(|x| x.id == e.from),
                flow.nodes.iter().position(|x| x.id == e.to),
            ) else {
                continue;
            };
            let (fx, fy) = pos[&fi];
            let (tx, ty) = pos[&ti];
            draw_edge_back(
                &mut c,
                fx,
                fy,
                node_box_width(&flow.nodes[fi]),
                tx,
                ty,
                node_box_width(&flow.nodes[ti]),
                first_spine + n,
                e,
            );
        }

        for (li, lvl) in levels.iter().enumerate() {
            for &ni in lvl {
                let (x, y) = pos[&ni];
                draw_node(&mut c, x, y, &flow.nodes[ni]);
            }
            let _ = li;
        }

        // Connect each edge that spans exactly one level.
        for e in &flow.edges {
            let (Some(fi), Some(ti)) = (
                flow.nodes.iter().position(|n| n.id == e.from),
                flow.nodes.iter().position(|n| n.id == e.to),
            ) else {
                continue;
            };
            if depth[&e.to] < depth[&e.from] {
                continue; // already routed around the side
            }
            let (fl, tl) = (depth[&e.from], depth[&e.to]);
            if fl + 1 != tl {
                continue;
            }
            let (fx, fy) = pos[&fi];
            let (tx, ty) = pos[&ti];
            draw_edge_down(
                &mut c,
                fx,
                fy,
                node_box_width(&flow.nodes[fi]),
                tx,
                ty,
                node_box_width(&flow.nodes[ti]),
                e,
            );
        }
        c.into_lines()
    } else {
        // Left-to-right: each level is a column of boxes.
        let col_widths: Vec<usize> = levels
            .iter()
            .map(|lvl| {
                lvl.iter()
                    .map(|&i| node_box_width(&flow.nodes[i]))
                    .max()
                    .unwrap_or(3)
            })
            .collect();
        let canvas_width: usize = col_widths.iter().sum::<usize>() + 4 * col_widths.len();
        let mut c = Canvas::default();
        let mut pos: std::collections::HashMap<usize, (usize, usize)> =
            std::collections::HashMap::new();
        let mut x = 1usize;
        let mut col_x: Vec<usize> = Vec::new();
        for (li, lvl) in levels.iter().enumerate() {
            col_x.push(x);
            let widest = col_widths[li];
            let mut y = 1usize;
            for &ni in lvl.iter() {
                let w = node_box_width(&flow.nodes[ni]);
                let ox = x + (widest - w) / 2;
                pos.insert(ni, (ox, y));
                draw_node(&mut c, ox, y, &flow.nodes[ni]);
                y += 3;
            }
            x += widest + 4;
        }
        let _ = canvas_width;
        for e in &flow.edges {
            let (Some(fi), Some(ti)) = (
                flow.nodes.iter().position(|n| n.id == e.from),
                flow.nodes.iter().position(|n| n.id == e.to),
            ) else {
                continue;
            };
            let (fl, tl) = (depth[&e.from], depth[&e.to]);
            if fl + 1 != tl {
                continue;
            }
            let (fx, fy) = pos[&fi];
            let (tx, ty) = pos[&ti];
            draw_edge_right(&mut c, fx, fy, node_box_width(&flow.nodes[fi]), tx, ty, e);
        }
        c.into_lines()
    }
}

fn draw_node(c: &mut Canvas, x: usize, y: usize, node: &FlowNode) {
    let style = theme::diagram_chrome();
    let label_style = theme::diagram_label();
    let w = node_box_width(node);
    let (tl, tr, bl, br, ml, mr) = match node.shape {
        Shape::Rect => (TL, TR, BL, BR, V, V),
        Shape::Round => ("\u{256d}", "\u{256e}", "\u{2570}", "\u{256f}", V, V),
        Shape::Diamond => (TL, TR, BL, BR, "\u{2572}", "\u{256e}"),
        Shape::Stadium => ("\u{256d}", "\u{256e}", "\u{256f}", "\u{2570}", V, V),
    };
    let _ = (ml, mr);
    c.put(x, y, tl.chars().next().unwrap(), style);
    c.put(x + w - 1, y, tr.chars().next().unwrap(), style);
    c.put(x, y + 2, bl.chars().next().unwrap(), style);
    c.put(x + w - 1, y + 2, br.chars().next().unwrap(), style);
    c.hline(x + 1, y, w - 2, H.chars().next().unwrap(), style);
    c.hline(x + 1, y + 2, w - 2, H.chars().next().unwrap(), style);
    c.put(x, y + 1, V.chars().next().unwrap(), style);
    c.put(x + w - 1, y + 1, V.chars().next().unwrap(), style);

    let label = node.label.replace(['\n', '\r'], " ");
    let inner = w.saturating_sub(2);
    let lw = display_width(&label);
    let pad = inner.saturating_sub(lw) / 2;
    let tx = x + 1 + pad;
    c.text(tx, y + 1, &label, label_style);
    // Pad the row so the right border stays visible.
    let end = tx + lw;
    if end < x + w - 1 {
        c.hline(end, y + 1, x + w - 1 - end, ' ', Style::default());
    }
}

/// The stroke to draw for an edge in the given style.
fn edge_ch(style: EdgeStyle, ch: char) -> char {
    if style != EdgeStyle::Dashed {
        return ch;
    }
    match ch {
        c if c == H.chars().next().unwrap() => H_DASH.chars().next().unwrap(),
        c if c == V.chars().next().unwrap() => V_DASH.chars().next().unwrap(),
        other => other,
    }
}

fn draw_edge_down(
    c: &mut Canvas,
    fx: usize,
    fy: usize,
    fw: usize,
    tx: usize,
    ty: usize,
    tw: usize,
    e: &FlowEdge,
) {
    let style = theme::diagram_text();
    let from_x = fx + fw / 2;
    // Box centres come from `x + width / 2`, so a parent and child whose
    // widths differ in parity end up one column apart. Elbowing across
    // that gap paints a single dash and a tee, which reads as a glitch
    // rather than a line, so a near miss is treated as aligned. Snapped
    // before the label maths below, or a label on a snapped edge would be
    // placed against the pre-snap column.
    let mut to_x = tx + tw / 2;
    if to_x.abs_diff(from_x) <= ELBOW_SNAP {
        to_x = from_x;
    }

    let start_y = fy + 3;
    let end_y = ty;
    if start_y >= end_y {
        return;
    }
    let h = edge_ch(e.style, H.chars().next().unwrap());
    let v = edge_ch(e.style, V.chars().next().unwrap());

    // Where the label goes on the bus, if there is one.
    let mut label_at: Option<usize> = None;
    if from_x == to_x {
        c.vline(from_x, start_y, end_y - start_y - 1, v, style);
    } else {
        // Horizontal bus between the two columns. An edge label sits on
        // the bus, so the bus is drawn in two pieces around it.
        let (lo, hi) = (from_x.min(to_x), from_x.max(to_x));
        let (l0, l1) = match &e.label {
            Some(label) => {
                let lw = display_width(label);
                let start = (lo + hi) / 2 + 1 - lw / 2;
                let start = start.max(lo).min(hi + 1);
                label_at = Some(start);
                (start, start + lw - 1)
            }
            None => (hi + 1, hi),
        };
        if l0 > lo {
            c.hline(lo, start_y, l0 - lo, h, style);
        }
        if l1 < hi {
            c.hline(l1 + 1, start_y, hi - l1, h, style);
        }
        // Both tees go on after the bus. The parent tee is the line's
        // origin and the child tee is where it turns down, so the bus
        // must not paint over them. Only the parent stub is a tee: a
        // full-height one would hang below the bus on a branching edge,
        // where nothing continues.
        c.put(from_x, start_y, T_DOWN.chars().next().unwrap(), style);
        c.put(to_x, start_y, T_DOWN.chars().next().unwrap(), style);
        c.vline(to_x, start_y + 1, end_y - start_y - 2, v, style);
    }
    c.put(to_x, end_y - 1, DOWN.chars().next().unwrap(), style);

    if let Some(label) = &e.label {
        if let Some(lx) = label_at {
            c.text(lx, start_y, label, theme::diagram_label());
        } else {
            // A straight drop has no bus to interrupt, so the label
            // goes beside it.
            c.text(
                from_x + 2,
                (start_y + end_y) / 2,
                label,
                theme::diagram_label(),
            );
        }
    }
}

/// Route an edge that points back up the diagram.
///
/// The tree layout only has room for edges that step down one level, so a
/// loop has to travel out to a reserved column, climb, and come back in.
/// Drawn before the boxes, so a line that crosses a sibling reads as
/// passing behind it.
fn draw_edge_back(
    c: &mut Canvas,
    fx: usize,
    fy: usize,
    fw: usize,
    tx: usize,
    ty: usize,
    tw: usize,
    spine: usize,
    e: &FlowEdge,
) {
    let style = theme::diagram_text();
    let h = edge_ch(e.style, H.chars().next().unwrap());
    let v = edge_ch(e.style, V.chars().next().unwrap());

    // Leave the source on its right edge at mid-height, and arrive at the
    // target's right edge at mid-height, so both boxes stay readable.
    let exit_y = fy + 1;
    let enter_y = ty + 1;
    let exit_x = fx + fw;
    let enter_x = tx + tw;

    // East to the spine, then north up it. BL joins the westward run to
    // the climb.
    if exit_x < spine {
        c.hline(exit_x, exit_y, spine - exit_x, h, style);
    }
    c.put(spine, exit_y, BL.chars().next().unwrap(), style);

    if enter_y < exit_y {
        c.vline(spine, enter_y + 1, exit_y - enter_y - 1, v, style);
        c.put(spine, enter_y, TR.chars().next().unwrap(), style);
    }

    // West into the target, head last so it survives the box. The head
    // sits one column *outside* the box: drawn on the border itself the
    // box would paint over it.
    let head_x = enter_x + 1;
    if head_x < spine {
        c.hline(head_x, enter_y, spine - head_x, h, style);
    }
    c.put(head_x, enter_y, LEFT.chars().next().unwrap(), style);

    if let Some(label) = &e.label {
        let lx = spine.saturating_sub(display_width(label)).saturating_sub(1);
        c.text(
            lx,
            enter_y.saturating_sub(1).max(1),
            label,
            theme::diagram_label(),
        );
    }
}

fn draw_edge_right(
    c: &mut Canvas,
    fx: usize,
    fy: usize,
    fw: usize,
    tx: usize,
    ty: usize,
    e: &FlowEdge,
) {
    let style = theme::diagram_text();
    let start_x = fx + fw;
    let end_x = tx;
    let from_y = fy + 1;
    let to_y = ty + 1;
    if start_x >= end_x {
        return;
    }
    let stroke = edge_ch(e.style, H.chars().next().unwrap());
    let len = end_x - start_x - 1;
    c.hline(start_x, from_y, len, stroke, style);
    if from_y == to_y {
        c.put(end_x - 1, to_y, RIGHT.chars().next().unwrap(), style);
    } else {
        let (lo, hi) = (from_y.min(to_y), from_y.max(to_y));
        c.vline(
            start_x,
            lo,
            hi - lo + 1,
            edge_ch(e.style, V.chars().next().unwrap()),
            style,
        );
        c.hline(start_x, hi, len, stroke, style);
        c.put(start_x, hi, TL_T.chars().next().unwrap(), style);
        c.vline(
            end_x - 1,
            hi,
            (to_y as isize - hi as isize).unsigned_abs().max(1),
            V.chars().next().unwrap(),
            style,
        );
        c.put(end_x - 1, to_y, RIGHT.chars().next().unwrap(), style);
        let _ = TD_X;
        let _ = T_UP;
        let _ = TR_T;
    }
    if let Some(label) = &e.label {
        c.text(
            start_x + 1,
            from_y.saturating_sub(1),
            label,
            theme::diagram_label(),
        );
    }
}

/// A participant in a sequence diagram.
#[derive(Clone, Debug)]
struct Participant {
    id: String,
    label: String,
}

/// One message or note in a sequence diagram.
#[derive(Clone, Debug)]
enum SeqEvent {
    Message {
        from: String,
        to: String,
        text: String,
        dashed: bool,
    },
    Note {
        over: (String, String),
        text: String,
    },
}

struct Sequence {
    participants: Vec<Participant>,
    events: Vec<SeqEvent>,
}

fn parse_sequence(body: &str) -> Option<Sequence> {
    let mut seq = Sequence {
        participants: Vec::new(),
        events: Vec::new(),
    };
    for raw in body.lines() {
        let line = raw.split("%%").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("participant ") {
            let (id, label) = split_alias(rest, &mut seq);
            add_participant(&mut seq, id, label);
        } else if let Some(rest) = line.strip_prefix("actor ") {
            let (id, label) = split_alias(rest, &mut seq);
            add_participant(&mut seq, id, label);
        } else if let Some(rest) = line.strip_prefix("Note over ") {
            let (over, text) = rest.split_once(':').unwrap_or((rest, ""));
            let names: Vec<&str> = over.split(',').map(|s| s.trim()).collect();
            if names.len() == 2 {
                let a = names[0].to_string();
                let b = names[1].to_string();
                add_participant(&mut seq, a.clone(), None);
                add_participant(&mut seq, b.clone(), None);
                seq.events.push(SeqEvent::Note {
                    over: (a, b),
                    text: text.trim().to_string(),
                });
            }
        } else if let Some((from_raw, dashed, after)) = split_message(line) {
            let from = from_raw.trim().to_string();
            let (to, text) = match after.split_once(':') {
                Some((t, x)) => (t.trim().to_string(), x.trim().to_string()),
                None => (after.trim().to_string(), String::new()),
            };
            if from.is_empty() || to.is_empty() {
                return None;
            }
            add_participant(&mut seq, from.clone(), None);
            add_participant(&mut seq, to.clone(), None);
            seq.events.push(SeqEvent::Message {
                from,
                to,
                text,
                dashed,
            });
        } else {
            // Unknown statement: give up on the whole diagram rather
            // than render something misleading.
            return None;
        }
    }
    if seq.participants.is_empty() || seq.events.is_empty() {
        return None;
    }
    // Present participants in first-seen order.
    Some(seq)
}

fn add_participant(seq: &mut Sequence, id: String, label: Option<String>) {
    if let Some(p) = seq.participants.iter_mut().find(|p| p.id == id) {
        if let Some(l) = label {
            p.label = l;
        }
        return;
    }
    seq.participants.push(Participant {
        label: label.unwrap_or_else(|| id.clone()),
        id,
    });
}

fn split_alias(s: &str, seq: &mut Sequence) -> (String, Option<String>) {
    let s = s.trim();
    match s.split_once(" as ") {
        Some((id, label)) => {
            let id = id.trim().to_string();
            let label = label.trim().to_string();
            add_participant(seq, id.clone(), None);
            (id, Some(label))
        }
        None => {
            let id = s.to_string();
            add_participant(seq, id.clone(), None);
            (id, None)
        }
    }
}

/// Recognise a message arrow, returning the text before it, whether it
/// is dashed, and the text after it.
///
/// The two-character forms are tested first: `-->>` contains `->>`, so
/// the shorter pattern would otherwise match a dashed arrow mid-way and
/// leave a stray `-` on the sender.
fn split_message(line: &str) -> Option<(&str, bool, &str)> {
    for (arrow, dashed) in [
        ("-->>", true),
        ("->>", false),
        ("--x", true),
        ("-x", false),
        ("--)", true),
        ("->", false),
    ] {
        if let Some(i) = line.find(arrow) {
            return Some((&line[..i], dashed, &line[i + arrow.len()..]));
        }
    }
    None
}

const SEQ_COL_W: usize = 14;
const SEQ_GAP: usize = 8;
/// Rows used by a participant header box: top rule, label, bottom rule.
const SEQ_HEADER_H: usize = 3;
/// Rows a single event occupies: label, arrow, blank.
const SEQ_EVENT_H: usize = 3;

fn render_sequence(seq: &Sequence) -> Vec<Line> {
    let n = seq.participants.len();
    // Center column for each participant.
    let centers: Vec<usize> = (0..n)
        .map(|i| SEQ_COL_W / 2 + i * (SEQ_COL_W + SEQ_GAP))
        .collect();

    let mut c = Canvas::default();
    let box_style = theme::diagram_chrome();
    let label_style = theme::diagram_label();
    let lifeline = theme::diagram_text();

    for (i, p) in seq.participants.iter().enumerate() {
        let x = centers[i] - SEQ_COL_W / 2;
        let last = x + SEQ_COL_W - 1;
        // A three-row header, so the label has a line of its own instead
        // of overwriting the top rule.
        c.put(x, 0, TL.chars().next().unwrap(), box_style);
        c.put(last, 0, TR.chars().next().unwrap(), box_style);
        c.hline(
            x + 1,
            0,
            SEQ_COL_W - 2,
            H.chars().next().unwrap(),
            box_style,
        );
        c.put(x, 1, V.chars().next().unwrap(), box_style);
        c.put(last, 1, V.chars().next().unwrap(), box_style);
        c.put(x, 2, BL.chars().next().unwrap(), box_style);
        c.put(last, 2, BR.chars().next().unwrap(), box_style);
        c.hline(
            x + 1,
            2,
            SEQ_COL_W - 2,
            H.chars().next().unwrap(),
            box_style,
        );

        let label = p.label.replace(['\n', '\r'], " ");
        let lw = display_width(&label).min(SEQ_COL_W - 2);
        let lx = x + 1 + (SEQ_COL_W - 2 - lw) / 2;
        c.text(lx, 1, &label, label_style);
        c.hline(x + 1, 1, SEQ_COL_W - 2, ' ', Style::default());
        c.text(lx, 1, &label, label_style);
    }

    let top = SEQ_HEADER_H;
    // Rows each event occupies: label, arrow, blank. The lifeline span has
    // to use the same figure as the loop below, or the verticals stop
    // short of the last message.
    let height = seq.events.len() * SEQ_EVENT_H + 2;
    for cx in &centers {
        c.vline(*cx, top, height, V_DASH.chars().next().unwrap(), lifeline);
    }

    let index = |id: &str| seq.participants.iter().position(|p| p.id == id);

    let mut y = top;
    for ev in &seq.events {
        match ev {
            SeqEvent::Message {
                from,
                to,
                text,
                dashed,
            } => {
                let (Some(fi), Some(ti)) = (index(from), index(to)) else {
                    return Vec::new();
                };
                let (lo, hi) = (centers[fi].min(centers[ti]), centers[fi].max(centers[ti]));
                // A reply travels right-to-left, so the head has to point
                // back at the receiver. The label stays anchored at the
                // low end of the span either way -- it only has to sit
                // inside it.
                let rightward = centers[fi] <= centers[ti];
                let span = hi - lo - 1;
                // The label sits on its own row above the arrow. Clear a
                // background for it first, but stop one short of the far
                // lifeline: overrunning took the receiving vertical with
                // it and truncated the row.
                if !text.is_empty() {
                    let tw = display_width(text).min(span + 1);
                    c.hline(lo + 1, y, span, ' ', Style::default());
                    c.text(lo + 1, y, &text[..safe_len(text, tw)], label_style);
                }
                // A dashed message still needs a shaft. Blanking it left
                // the head floating with no line attached.
                let stroke = if *dashed {
                    H_DASH.chars().next().unwrap()
                } else {
                    H.chars().next().unwrap()
                };
                c.hline(lo + 1, y + 1, hi - lo - 1, stroke, lifeline);
                // Punch the sender and receiver through so the arrow
                // starts and ends at the lifelines cleanly, and point the
                // head back at whoever is receiving it.
                let head = if rightward { RIGHT } else { LEFT };
                let (tail, head_col) = if rightward { (lo, hi) } else { (hi, lo) };
                c.put(tail, y + 1, V_DASH.chars().next().unwrap(), lifeline);
                c.put(head_col, y + 1, head.chars().next().unwrap(), lifeline);
            }
            SeqEvent::Note { over, text } => {
                let (Some(a), Some(b)) = (index(&over.0), index(&over.1)) else {
                    return Vec::new();
                };
                let (lo, hi) = (centers[a].min(centers[b]), centers[a].max(centers[b]));
                let left = lo.saturating_sub(4);
                let right = hi + 4;
                // Three rows: top rule, text, bottom rule.
                c.put(left, y, TL.chars().next().unwrap(), box_style);
                c.hline(
                    left + 1,
                    y,
                    right - left - 1,
                    H.chars().next().unwrap(),
                    box_style,
                );
                c.put(right, y, TR.chars().next().unwrap(), box_style);
                c.put(left, y + 1, V.chars().next().unwrap(), box_style);
                c.put(right, y + 1, V.chars().next().unwrap(), box_style);
                c.hline(left + 1, y + 1, right - left - 1, ' ', Style::default());
                // The note body is opaque, so the lifelines it covers
                // are left blank rather than struck through the text.
                let inner = right - left - 1;
                let body = text.clone();
                let bw = display_width(&body).min(inner.saturating_sub(2));
                c.text(left + 2, y + 1, &body[..safe_len(&body, bw)], label_style);
                c.hline(
                    left + 1,
                    y + 2,
                    right - left - 1,
                    H.chars().next().unwrap(),
                    box_style,
                );
                c.put(left, y + 2, BL.chars().next().unwrap(), box_style);
                c.put(right, y + 2, BR.chars().next().unwrap(), box_style);
            }
        }
        y += SEQ_EVENT_H;
    }
    c.into_lines()
}

/// Byte length of the longest prefix of `s` whose display width is at
/// most `max`, so text is never cut through a character.
fn safe_len(s: &str, max: usize) -> usize {
    let mut w = 0usize;
    for (i, ch) in s.char_indices() {
        let cw = super::width::display_width(&ch.to_string());
        if w + cw > max {
            return i;
        }
        w += cw;
    }
    s.len()
}

/// True when a fence tag names a Mermaid diagram.
pub fn is_mermaid_lang(lang: &str) -> bool {
    matches!(lang.trim().to_ascii_lowercase().as_str(), "mermaid" | "mmd")
}

/// Render a Mermaid source block, or `None` when the diagram family is
/// not supported.
pub fn render(src: &str) -> Option<Vec<Line>> {
    let first = src.lines().map(str::trim).find(|l| !l.is_empty())?;
    let mut it = first.split_whitespace();
    let keyword = it.next()?.to_ascii_lowercase();
    // Everything after the header line is the diagram body. The header
    // may carry trailing tokens (`graph TD`), but never statements.
    let body: String = src.lines().skip(1).collect::<Vec<&str>>().join("\n");

    match keyword.as_str() {
        "graph" | "flowchart" => {
            let dir = it.next().unwrap_or("TD").to_ascii_lowercase();
            let horizontal = matches!(dir.as_str(), "lr" | "rl");
            let flow = parse_flow(&body);
            if flow.nodes.is_empty() {
                return None;
            }
            Some(render_flowchart(&flow, horizontal))
        }
        "sequencediagram" => {
            let seq = parse_sequence(&body)?;
            Some(render_sequence(&seq))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Char predicates, so the assertions below read as intent rather
    /// than as a column of integer comparisons.
    trait Glyph {
        fn is_vline(&self) -> bool;
        fn is_right_head(&self) -> bool;
        fn is_left_head(&self) -> bool;
        fn is_hline(&self) -> bool;
    }
    impl Glyph for char {
        fn is_vline(&self) -> bool {
            matches!(self, '\u{2502}' | '\u{2506}')
        }
        fn is_right_head(&self) -> bool {
            matches!(self, '\u{25b6}' | '\u{25b7}' | '\u{27a1}')
        }
        fn is_left_head(&self) -> bool {
            matches!(self, '\u{25c0}' | '\u{25c1}' | '\u{2b95}')
        }
        fn is_hline(&self) -> bool {
            matches!(self, '\u{2500}' | '\u{2501}' | '\u{2504}' | '\u{2508}')
        }
    }

    /// `A --> B --> C` is two edges. Reading only the first arrow left
    /// every node past the second unrendered.
    #[test]
    fn an_arrow_chain_keeps_every_node() {
        let out = plain("flowchart LR\n    A --> B --> C --> D\n").expect("renders");
        for n in ["A", "B", "C", "D"] {
            assert!(
                out.iter().any(|l| l.contains(&format!("│ {n} │"))),
                "{n} missing from {out:?}"
            );
        }
    }

    /// A chained edge can still carry per-hop labels.
    #[test]
    fn an_arrow_chain_keeps_per_hop_labels() {
        let out = plain("flowchart TD\n    A -->|yes| B -->|no| C\n").expect("renders");
        assert!(out.iter().any(|l| l.contains("yes")), "{out:?}");
        assert!(out.iter().any(|l| l.contains("no")), "{out:?}");
    }

    /// A diagram must arrive framed, like any other fenced block.
    #[test]
    fn a_rendered_diagram_is_boxed() {
        let out = framed(
            "```mermaid\nflowchart TD\n    A[Start] --> B[End]\n```\n",
            80,
        );
        assert!(out.first().unwrap().contains("mermaid"), "{out:?}");
        assert!(out[0].contains('┌') && out[1].contains('│'), "{out:?}");
        assert!(out.last().unwrap().contains('┘'), "{out:?}");
    }

    /// Wider than the terminal, a diagram is clipped with a marker so the
    /// right border stays in place instead of drifting off the row.
    #[test]
    fn an_over_wide_diagram_is_clipped_inside_its_frame() {
        let src = "```mermaid\nflowchart LR\n    A --> B --> C --> D --> E --> F\n```\n";
        let out = framed(src, 44);
        for l in &out {
            assert!(l.chars().count() <= 44, "line overflows: {l:?}");
        }
        assert!(
            out.iter().any(|l| l.contains('…')),
            "clipped without a marker: {out:?}"
        );
    }

    /// Render through the Markdown pipeline, which is what adds the frame.
    fn framed(src: &str, width: usize) -> Vec<String> {
        let opts = crate::md::Options {
            width,
            color: false,
            ..Default::default()
        };
        crate::md::render(src, &opts)
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn plain(src: &str) -> Option<Vec<String>> {
        render(src).map(|ls| {
            ls.iter()
                .map(|l| l.runs.iter().map(|r| r.text.as_str()).collect::<String>())
                .collect()
        })
    }

    /// A graph with no loop must not pay for the margin a loop needs.
    ///
    /// The numbers are pinned regression guards captured before the
    /// back-edge routing existed. Reserving spine columns for a diagram
    /// that has no back edge would silently widen every flowchart, so
    /// these fail loudly if the margin leaks.
    #[test]
    fn an_acyclic_flowchart_reserves_no_margin() {
        for (src, want) in [
            ("flowchart TD\n A[Start] --> B[Middle] --> C[End]", 11),
            ("flowchart TD\n A --> B --> C", 6),
            ("flowchart TD\n A[One] --> B[Two]\n B --> C[Three]", 10),
            (
                "flowchart TD\n A[Start] --> B{Mid}\n B -->|yes| C[Yes]\n B -->|no| D[No]",
                18,
            ),
        ] {
            let out = plain(src).unwrap();
            let got = out.iter().map(|l| l.chars().count()).max().unwrap();
            assert_eq!(got, want, "acyclic width moved for {src:?}");
        }
    }

    /// The retry loop has to be visible.
    ///
    /// Edges were drawn only when they advanced exactly one level, so
    /// `F --> B` was dropped and the diagram read as a one-way pipeline
    /// even though its whole point is "fix the input and check again".
    #[test]
    fn a_back_edge_reaches_its_target() {
        let out = plain(
            "flowchart TD\n A[Start] --> B[Check]\n B --> C[Parse]\n C --> D[Render]\n D --> E[Fix]\n E --> B",
        )
        .unwrap();
        // The loop has to arrive at Check from the right. A head anywhere
        // on the canvas would pass a weaker check, so pin it to the box.
        let row: Vec<char> = out
            .iter()
            .find(|l| l.contains("Check"))
            .expect("the Check box")
            .chars()
            .collect();
        let borders: Vec<usize> = row
            .iter()
            .enumerate()
            .filter(|(_, c)| **c == V.chars().next().unwrap())
            .map(|(i, _)| i)
            .collect();
        let box_right = borders[1];
        let head = row.iter().position(|c| *c == LEFT.chars().next().unwrap());
        assert!(
            head.is_some_and(|h| h > box_right),
            "nothing arrives at Check from the right (box edge {box_right}): {row:?}"
        );
    }

    #[test]
    fn every_edge_in_the_source_is_drawn() {
        // Seven edges: A>B, B>C, B>D, C>E, D>F, E>G, F>B.
        let src = "flowchart TD\n A[Start] --> B[Check]\n B -->|Yes| C[Parse]\n B -->|No| D[Err]\n C --> E[Render]\n D --> F[Fix]\n F --> B\n E --> G[Done]";
        let out = plain(src).unwrap();
        let heads = out
            .iter()
            .map(|l| {
                l.chars()
                    .filter(|c| {
                        *c == DOWN.chars().next().unwrap() || *c == LEFT.chars().next().unwrap()
                    })
                    .count()
            })
            .sum::<usize>();
        // One head per forward edge, plus the back edge arriving sideways.
        assert!(
            heads >= 7,
            "expected 7 edges to be visible, found {heads} heads:\n{}",
            out.join("\n")
        );
    }

    /// A child one column off should get a clean vertical, not a stub.
    ///
    /// Box centres come from `x + width / 2`, so odd and even widths put
    /// a parent and child one column apart. The elbow then painted a
    /// single dash and a tee, which read as a glitch rather than a line.
    #[test]
    fn a_near_aligned_child_gets_a_plain_vertical() {
        // "One" is 3 wide and "Twox" is 4, so the boxes differ in parity
        // and their centres land one column apart.
        let out = plain("flowchart TD\n A[One] --> B[Twox]").unwrap();
        let tee = T_DOWN.chars().next().unwrap();
        for l in &out {
            let chars: Vec<char> = l.chars().collect();
            for (i, c) in chars.iter().enumerate() {
                if *c != tee {
                    continue;
                }
                let before = chars[..i].iter().rev().take_while(|c| **c != ' ').count();
                let after = chars[i + 1..].iter().take_while(|c| **c != ' ').count();
                assert!(
                    !(before <= 1 && after <= 1),
                    "degenerate elbow left a stub at column {i}: {l:?}"
                );
            }
        }
    }

    /// A bus has to tee where the line branches, not just where it ends.
    ///
    /// The bus is painted from the parent's column outwards, so drawing
    /// the parent's tee first let the bus paint straight over it. On an
    /// unlabelled bus that left a bare `------` with one tee at the far end.
    #[test]
    fn a_bus_tees_where_the_line_branches() {
        let out = plain("flowchart TD\n A[One] --> C[Three]\n B[Two]").unwrap();
        let tees = out
            .iter()
            .map(|l| {
                l.chars()
                    .filter(|c| *c == T_DOWN.chars().next().unwrap())
                    .count()
            })
            .max()
            .unwrap();
        assert_eq!(tees, 2, "the bus lost its parent tee:\n{}", out.join("\n"));
    }

    #[test]
    fn simple_flowchart_renders_both_nodes() {
        let out = plain("graph TD\nA[Start] --> B[End]").unwrap();
        let joined = out.join("\n");
        assert!(joined.contains("Start"), "{joined}");
        assert!(joined.contains("End"), "{joined}");
    }

    #[test]
    fn flowchart_has_an_arrow() {
        let out = plain("graph TD\nA --> B").unwrap();
        assert!(out.join("\n").contains('\u{25bc}'), "no arrow: {out:?}");
    }

    #[test]
    fn shapes_are_parsed() {
        let flow = parse_flow("A[rect] B(round) C{diamond} D((circle)) E[[sub]]");
        let shapes: Vec<Shape> = flow.nodes.iter().map(|n| n.shape).collect();
        assert_eq!(
            shapes,
            vec![
                Shape::Rect,
                Shape::Round,
                Shape::Diamond,
                Shape::Diamond,
                Shape::Stadium
            ]
        );
    }

    #[test]
    fn edge_labels_are_captured() {
        let flow = parse_flow("A -->|yes| B");
        assert_eq!(flow.edges[0].label.as_deref(), Some("yes"));
    }

    #[test]
    fn dashed_edges_are_recognized() {
        let flow = parse_flow("A -.-> B");
        assert_eq!(flow.edges[0].style, EdgeStyle::Dashed);
    }

    #[test]
    fn diamond_parentheses_map_to_diamond() {
        let flow = parse_flow("A((both))");
        assert_eq!(flow.nodes[0].shape, Shape::Diamond);
    }

    #[test]
    fn cycles_terminate() {
        let out = plain("graph TD\nA --> B\nB --> C\nC --> A");
        assert!(out.is_some());
    }

    #[test]
    fn three_step_chain_stacks_vertically() {
        let out = plain("graph TD\nA[One] --> B[Two]\nB --> C[Three]").unwrap();
        // Header row for One, then Two, then Three, each on its own line.
        let find = |needle: &str| out.iter().position(|l| l.contains(needle)).unwrap();
        assert!(find("One") < find("Two"), "{out:?}");
        assert!(find("Two") < find("Three"), "{out:?}");
    }

    #[test]
    fn left_to_right_differs_from_top_to_bottom() {
        let td = plain("graph TD\nA[Start] --> B[End]").unwrap();
        let lr = plain("graph LR\nA[Start] --> B[End]").unwrap();
        assert_ne!(td, lr, "orientation had no effect");
    }

    #[test]
    fn left_to_right_places_nodes_side_by_side() {
        let out = plain("graph LR\nA[Start] --> B[End]").unwrap();
        let start_line = out.iter().position(|l| l.contains("Start")).unwrap();
        let end_line = out.iter().position(|l| l.contains("End")).unwrap();
        assert_eq!(start_line, end_line, "LR should share a line: {out:?}");
    }

    #[test]
    fn sequence_renders_participants_and_messages() {
        let out =
            plain("sequenceDiagram\nparticipant A as Alice\nA->>B: Hello\nB-->>A: Hi").unwrap();
        let joined = out.join("\n");
        assert!(joined.contains("Alice"), "{joined}");
        assert!(joined.contains("Hello"), "{joined}");
        assert!(joined.contains("Hi"), "{joined}");
    }

    #[test]
    fn sequence_alias_labels_win() {
        let out = plain("sequenceDiagram\nparticipant A as Zed\nA->>B: x").unwrap();
        assert!(out.join("\n").contains("Zed"));
    }

    #[test]
    fn sequence_notes_are_drawn() {
        let out = plain("sequenceDiagram\nA->>B: x\nNote over A,B: careful").unwrap();
        assert!(out.join("\n").contains("careful"), "{out:?}");
    }

    /// The row below a message's label, where that message's arrow sits.
    fn arrow_row<'a>(out: &'a [String], label: &str) -> &'a String {
        let i = out
            .iter()
            .position(|l| l.contains(label))
            .expect("label row");
        out.get(i + 1).expect("arrow row below the label")
    }

    /// (column, glyph) of an arrow head in a row.
    ///
    /// Columns are *character* offsets, not byte offsets: these rows are
    /// mostly three-byte box glyphs, so `char_indices` would report the
    /// head at 73 when it is at 29.
    fn head_of(row: &str) -> (usize, char) {
        row.chars()
            .enumerate()
            .find(|(_, c)| c.is_right_head() || c.is_left_head())
            .expect("row carries an arrow head")
    }

    /// Column of the sender's lifeline.
    ///
    /// An arrow row has exactly one vertical -- the tail -- because the
    /// head is drawn *over* the receiving lifeline.
    fn tail_of(row: &str) -> usize {
        let cols: Vec<usize> = row
            .chars()
            .enumerate()
            .filter(|(_, c)| c.is_vline())
            .map(|(i, _)| i)
            .collect();
        assert_eq!(cols.len(), 1, "expected one tail in {row:?}");
        cols[0]
    }

    /// A dashed reply has to travel the same distance as a solid one.
    ///
    /// The head was drawn at the receiving lifeline with a blank shaft
    /// behind it, so `B-->>A` rendered as a bare triangle adrift in the
    /// middle of the diagram with no line attached to it.
    #[test]
    fn a_dashed_reply_has_a_visible_shaft() {
        let out = plain("sequenceDiagram\nA->>B: go\nB-->>A: back").unwrap();
        let arrow = arrow_row(&out, "back");
        let (hc, _) = head_of(arrow);
        let tc = tail_of(arrow);
        let between: String = arrow
            .chars()
            .skip(tc.min(hc) + 1)
            .take((tc.max(hc) - tc.min(hc) - 1) as usize)
            .collect();
        assert!(
            between.chars().any(|c| c.is_hline()),
            "reply has no shaft between lifelines {tc} and {hc}: {arrow:?}"
        );
    }

    /// A reply points at whoever receives it, whichever side that is.
    ///
    /// Endpoints came from `min`/`max` and the tail was always drawn on
    /// the left, so every `B-->>A` reply rendered as though A had sent
    /// it. In a four-party diagram all three replies came out reversed.
    #[test]
    fn a_reply_points_at_its_receiver() {
        let out = plain("sequenceDiagram\nA->>B: go\nB-->>A: back").unwrap();
        let arrow = arrow_row(&out, "back");
        let (hc, head) = head_of(arrow);
        let tc = tail_of(arrow);
        assert!(
            head.is_left_head(),
            "a reply travelling left needs ◀, got {head:?}: {arrow:?}"
        );
        assert!(
            hc < tc,
            "reply must arrive before it departs: head at {hc}, tail at {tc}"
        );
    }

    /// A request keeps travelling right, so replies are the only thing
    /// that changed direction.
    #[test]
    fn a_request_still_points_right() {
        let out = plain("sequenceDiagram\nA->>B: go").unwrap();
        let arrow = arrow_row(&out, "go");
        let (hc, head) = head_of(arrow);
        let tc = tail_of(arrow);
        assert!(head.is_right_head(), "a request needs ▶, got {head:?}");
        assert!(
            tc < hc,
            "request must depart before it arrives: {tc} then {hc}"
        );
    }

    /// Writing a message label must not erase the lifeline it points at.
    ///
    /// The clear behind the label ran one column long and took the
    /// receiving lifeline with it, truncating the row, so lifelines
    /// looked randomly snipped wherever a message was written.
    #[test]
    fn a_label_row_keeps_the_receiving_lifeline() {
        let out = plain("sequenceDiagram\nA->>B: a message label").unwrap();
        let arrow = arrow_row(&out, "a message label");
        let (hc, _) = head_of(arrow);
        let label_row = out.iter().find(|l| l.contains("a message label")).unwrap();
        assert!(
            label_row.chars().nth(hc).is_some_and(|c| c.is_vline()),
            "receiver lifeline at {hc} erased by the label: {label_row:?}"
        );
    }

    /// Lifelines run from the participant header to the last message.
    ///
    /// Their height was computed as two rows per event while the layout
    /// advances three (label, arrow, blank), so the tail of the diagram
    /// came out with no verticals at all.
    #[test]
    fn lifelines_span_the_whole_diagram() {
        let out = plain("sequenceDiagram\nA->>B: one\nB-->>A: two\nA->>B: three").unwrap();
        // The first arrow row pins the two lifeline columns: its tail and
        // its head each sit on one.
        let first = arrow_row(&out, "one");
        let (hc, _) = head_of(first);
        let tc = tail_of(first);
        let header_end = out
            .iter()
            .position(|l| l.contains("one"))
            .expect("first label row");

        for (i, line) in out.iter().enumerate().skip(header_end) {
            // The sender's column carries a vertical on every row. The
            // receiver's carries one too, except on an arrow row, where
            // the head is drawn over it.
            for col in [tc, hc] {
                let ok = line
                    .chars()
                    .nth(col)
                    .is_some_and(|c| c.is_vline() || c.is_right_head() || c.is_left_head());
                assert!(ok, "row {i} lost the lifeline at column {col}: {line:?}");
            }
        }
    }

    #[test]
    fn unknown_family_returns_none() {
        assert!(render("pie title Pets\n\"a\" : 10").is_none());
        assert!(render("classDiagram\nA <|-- B").is_none());
    }

    #[test]
    fn empty_source_returns_none() {
        assert!(render("").is_none());
        assert!(render("   \n\n").is_none());
    }

    #[test]
    fn comments_are_ignored() {
        let out = plain("graph TD\n%% a comment\nA[X] --> B[Y]").unwrap();
        assert!(!out.join("\n").contains("comment"));
    }

    #[test]
    fn semicolons_separate_statements() {
        let flow = parse_flow("A[One]; B[Two]; A --> B");
        assert_eq!(flow.nodes.len(), 2);
        assert_eq!(flow.edges.len(), 1);
    }

    #[test]
    fn mermaid_language_tags_are_recognized() {
        assert!(is_mermaid_lang("mermaid"));
        assert!(is_mermaid_lang("MMD"));
        assert!(!is_mermaid_lang("rust"));
    }

    #[test]
    fn canvas_grows_to_fit_writes() {
        let mut c = Canvas::default();
        c.put(10, 4, 'x', Style::default());
        assert!(c.width >= 11 && c.height >= 5);
    }
}
