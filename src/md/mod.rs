//! In-process Markdown rendering for the terminal.
//!
//! The layout targets `leaf`'s inline appearance, but the code is
//! Pocai's own: `pulldown-cmark` produces an event stream, a state
//! machine turns that stream into styled [`Run`]s grouped into [`Line`]s,
//! and the ANSI writer emits them with a final hard-wrap guard so no
//! line can ever exceed the terminal width.

mod ansi;
mod blocks;
mod code;
mod highlight;
mod inline;
mod latex;
mod links;
mod lists;
mod mermaid;
mod smart;
mod style;
mod table;
mod theme;
mod width;
mod wrap;

pub use style::{Line, Run, Style};

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, Options as CmarkOptions, Parser,
    Tag, TagEnd,
};

use lists::{ItemState, ListKind};
use theme::Alert;

/// Rendering options, resolved once from the environment and CLI.
#[derive(Clone, Debug)]
pub struct Options {
    /// Total columns available, including any list/quote indentation.
    pub width: usize,
    /// Emit ANSI escapes. Off for pipes, `NO_COLOR`, and dumb terminals.
    pub color: bool,
    /// Number the lines of fenced code blocks.
    pub line_numbers: bool,
    /// Convert straight quotes, dashes, and ellipses to typographic
    /// forms in prose.
    pub smart_punctuation: bool,
    /// Turn bare URLs in prose into styled links.
    pub linkify: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            width: 80,
            color: true,
            line_numbers: true,
            smart_punctuation: true,
            linkify: true,
        }
    }
}

impl Options {
    fn cmark(&self) -> CmarkOptions {
        let mut o = CmarkOptions::empty();
        o.insert(CmarkOptions::ENABLE_TABLES);
        o.insert(CmarkOptions::ENABLE_FOOTNOTES);
        o.insert(CmarkOptions::ENABLE_STRIKETHROUGH);
        o.insert(CmarkOptions::ENABLE_TASKLISTS);
        o.insert(CmarkOptions::ENABLE_MATH);
        o.insert(CmarkOptions::ENABLE_HEADING_ATTRIBUTES);
        o.insert(CmarkOptions::ENABLE_YAML_STYLE_METADATA_BLOCKS);
        o.insert(CmarkOptions::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS);
        o
    }
}

/// One level of block nesting.
#[derive(Clone, Debug)]
enum Frame {
    Quote { alert: Option<Alert> },
    List { kind: ListKind },
    Item(ItemState),
}

/// A link or image being collected, so the destination can be appended
/// once the visible text is known.
#[derive(Clone, Debug)]
struct PendingLink {
    start: usize,
    url: String,
}

/// Accumulated state for the table currently being parsed.
struct TableBuild {
    table: table::Table,
    aligns: Vec<Alignment>,
    in_head: bool,
    row: Vec<table::Cell>,
}

/// The renderer state machine.
struct Renderer<'a> {
    opts: &'a Options,
    out: Vec<Line>,
    stack: Vec<Frame>,
    pending: Vec<Run>,
    state: inline::InlineState,
    ctx: inline::InlineCtx,
    links: Vec<PendingLink>,
    code: Option<(String, String)>,
    table: Option<TableBuild>,
    quote_count: usize,
    in_heading: bool,
    heading_level: Option<u8>,
    in_metadata: bool,
    /// Suppress the paragraph gap before the next block, for tight lists.
    skip_next_blank: bool,
}

impl<'a> Renderer<'a> {
    fn new(opts: &'a Options) -> Self {
        Renderer {
            opts,
            out: Vec::new(),
            stack: Vec::new(),
            pending: Vec::new(),
            state: inline::InlineState::default(),
            ctx: inline::InlineCtx::root(),
            links: Vec::new(),
            code: None,
            table: None,
            quote_count: 0,
            in_heading: false,
            heading_level: None,
            in_metadata: false,
            skip_next_blank: false,
        }
    }

    // ── prefixes ────────────────────────────────────────────────────

    /// Prefix for the first line of a block: quote bars, then list
    /// markers for any enclosing item whose marker is not yet shown.
    fn first_prefix(&mut self) -> Vec<Run> {
        let mut runs = Vec::new();
        // `quote_prefix` takes a 1-based depth, matching how many bars
        // are actually drawn.
        let mut q = 0usize;
        for frame in &mut self.stack {
            match frame {
                Frame::Quote { alert } => {
                    q += 1;
                    runs.extend(blocks::quote_prefix(q, *alert));
                }
                Frame::List { .. } => {}
                Frame::Item(item) => {
                    let before = item.marker_emitted;
                    runs.extend(lists::first_line_prefix(item));
                    // Do not consume the marker for prefixes we discard.
                    if before && !item.marker_emitted {
                        item.marker_emitted = true;
                    }
                }
            }
        }
        runs
    }

    /// The same prefix with every marker replaced by blanks, so
    /// continuation lines align under the text rather than the bullet.
    fn cont_prefix(&self) -> Vec<Run> {
        let mut runs = Vec::new();
        let mut q = 0usize;
        for frame in &self.stack {
            match frame {
                Frame::Quote { alert } => {
                    // Depth is 1-based, matching `first_prefix`. The rail
                    // is redrawn on every line, not reserved as blank
                    // space: a blockquote that loses its bar halfway down
                    // reads as a broken layout.
                    q += 1;
                    runs.extend(blocks::quote_prefix(q, *alert));
                }
                Frame::List { .. } => {}
                Frame::Item(item) => runs.extend(lists::continuation_prefix(item)),
            }
        }
        runs
    }

    /// Quote bars kept visible on otherwise empty lines.
    ///
    /// Deliberately *not* the list continuation indent: a blank line
    /// inside a nested list would otherwise be nothing but trailing
    /// whitespace, which is invisible noise and breaks copy-paste.
    fn blank_prefix(&self) -> Vec<Run> {
        let mut runs = Vec::new();
        let mut q = 0usize;
        for frame in &self.stack {
            if let Frame::Quote { alert } = frame {
                q += 1;
                runs.extend(blocks::quote_prefix(q, *alert));
            }
        }
        runs
    }

    // ── output helpers ──────────────────────────────────────────────

    fn push_lines(&mut self, lines: Vec<Line>) {
        for l in lines {
            self.out.push(l);
        }
    }

    /// Insert a blank separator line, unless one is already there or
    /// there is nothing to separate from.
    fn blank(&mut self) {
        if self.skip_next_blank {
            self.skip_next_blank = false;
            return;
        }
        if self.out.is_empty() {
            return;
        }
        if self
            .out
            .last()
            .map(|l| l.is_blank_content())
            .unwrap_or(true)
        {
            return;
        }
        let prefix = self.blank_prefix();
        self.out.push(Line::from_runs(prefix));
    }

    /// End the current inline run and lay it out as a paragraph.
    fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let runs = std::mem::take(&mut self.pending);
        if runs.iter().all(|r| r.text.trim().is_empty()) {
            // A whitespace-only paragraph inside a quote still shows the
            // bar, matching how leaf keeps the rail continuous.
            if self.ctx.blockquote_depth > 0 {
                let prefix = self.blank_prefix();
                self.out.push(Line::from_runs(prefix));
            }
            return;
        }
        let first = self.first_prefix();
        let cont = self.cont_prefix();
        let lines = wrap::wrap(&runs, &first, &cont, self.opts.width);
        self.push_lines(lines);
    }

    /// Flush the paragraph and separate it from the next block.
    fn break_block(&mut self) {
        self.flush();
        self.blank();
    }

    /// Emit a block that has already been laid out, indenting it under
    /// the current item and quote.
    ///
    /// The first line takes the *first* prefix, so a block that opens a
    /// list item still gets its bullet; later lines align under the
    /// text. [`Renderer::first_prefix`] emits the marker only once, so
    /// subsequent blocks in the same item fall back to blanks.
    fn emit_block(&mut self, lines: Vec<Line>) {
        let first = self.first_prefix();
        let cont = self.cont_prefix();
        for (i, mut l) in lines.into_iter().enumerate() {
            let mut runs = if i == 0 { first.clone() } else { cont.clone() };
            runs.append(&mut l.runs);
            l.runs = runs;
            self.out.push(l);
        }
    }

    // ── event handling ──────────────────────────────────────────────

    fn base_style(&self) -> Style {
        let mut ctx = self.ctx;
        ctx.in_heading = self.in_heading;
        inline::text_style(&ctx, &self.state)
    }

    fn push_text(&mut self, text: &str) {
        let base = self.base_style();
        let mut runs = Vec::new();
        if self.opts.linkify {
            links::push_with_links(&mut runs, text, base, self.opts.smart_punctuation);
        } else if self.opts.smart_punctuation {
            let t = smart::convert(text);
            inline::push_text(&mut runs, &t, base);
        } else {
            inline::push_text(&mut runs, text, base);
        }
        self.pending.extend(runs);
    }

    fn run(&mut self, events: Vec<Event>) {
        let mut i = 0usize;
        while i < events.len() {
            let ev = events[i].clone();
            i += 1;
            if self.in_metadata {
                if matches!(ev, Event::End(TagEnd::MetadataBlock(_))) {
                    self.in_metadata = false;
                }
                continue;
            }
            if self.code.is_some() {
                self.on_code_event(&ev);
                continue;
            }
            if self.table.is_some() {
                self.on_table_event(&ev);
                continue;
            }
            self.on_event(ev);
        }
        self.flush();
    }

    fn on_code_event(&mut self, ev: &Event) {
        match ev {
            Event::Text(t) => {
                if let Some((_, buf)) = &mut self.code {
                    buf.push_str(t);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                let (lang, buf) = self.code.take().expect("code buffer");
                self.render_code_block(&lang, &buf);
            }
            _ => {}
        }
    }

    /// Close the row being accumulated, filing it as the header or as a
    /// body row depending on where we are in the table.
    fn flush_table_row(&mut self) {
        let Some(t) = &mut self.table else { return };
        let row = std::mem::take(&mut t.row);
        if row.is_empty() {
            return;
        }
        if t.in_head {
            t.table.header = row;
        } else {
            t.table.rows.push(row);
        }
    }

    fn on_table_event(&mut self, ev: &Event) {
        let align = self
            .table
            .as_ref()
            .and_then(|t| t.aligns.get(t.row.len()).copied())
            .unwrap_or(Alignment::None);
        match ev {
            // pulldown-cmark emits no `TableRow` around the header: its
            // cells sit directly inside `TableHead`. So `TableHead` *is*
            // the first row boundary, and treating only `TableRow` as one
            // would splice the header onto the first body row.
            Event::Start(Tag::TableHead) => {
                if let Some(t) = &mut self.table {
                    t.in_head = true;
                    t.row.clear();
                }
            }
            Event::Start(Tag::TableRow) => {
                if let Some(t) = &mut self.table {
                    t.row.clear();
                }
            }
            Event::Start(Tag::TableCell) => self.pending.clear(),
            Event::End(TagEnd::TableCell) => {
                let runs = std::mem::take(&mut self.pending);
                let a = match align {
                    Alignment::Left => table::Align::Left,
                    Alignment::Center => table::Align::Center,
                    Alignment::Right => table::Align::Right,
                    Alignment::None => table::Align::None,
                };
                if let Some(t) = &mut self.table {
                    t.row.push(table::Cell { runs, align: a });
                }
            }
            Event::End(TagEnd::TableRow) => self.flush_table_row(),
            Event::End(TagEnd::TableHead) => {
                self.flush_table_row();
                if let Some(t) = &mut self.table {
                    t.in_head = false;
                }
            }
            Event::End(TagEnd::Table) => {
                let build = self.table.take().expect("table build");
                let lines = table::render(&build.table, self.opts.width);
                if !lines.is_empty() {
                    self.blank();
                    self.emit_block(lines);
                }
            }
            _ => self.on_event(ev.clone()),
        }
    }

    fn on_event(&mut self, ev: Event) {
        match ev {
            // ── inline ──
            Event::Text(t) => self.push_text(&t),
            Event::Code(t) => {
                let base = self.base_style();
                inline::push_inline_code(&mut self.pending, &t);
                let _ = base;
            }
            Event::InlineMath(t) => {
                let runs = latex::inline_runs(&t);
                self.pending.extend(runs);
            }
            Event::SoftBreak => self.push_text(" "),
            Event::HardBreak => {
                // A hard break cannot be honoured inside a wrapped run,
                // so split the paragraph at this point.
                self.flush();
            }
            Event::InlineHtml(t) | Event::Html(t) => self.push_text(&t),
            Event::FootnoteReference(name) => {
                let base = self.base_style();
                self.pending.push(Run::new(format!("[^{name}]"), base));
            }
            Event::TaskListMarker(checked) => {
                if let Some(Frame::Item(item)) = self
                    .stack
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f, Frame::Item(_)))
                {
                    item.checkbox = Some(checked);
                }
            }
            Event::Rule => {
                self.break_block();
                let indent = self.cont_prefix();
                let w = self.opts.width.saturating_sub(wrap::runs_width(&indent));
                let lines = blocks::rule_lines(w, 0);
                self.emit_block(lines);
            }

            // ── emphasis ──
            Event::Start(Tag::Emphasis) => self.state.emphasis += 1,
            Event::End(TagEnd::Emphasis) => {
                self.state.emphasis = self.state.emphasis.saturating_sub(1)
            }
            Event::Start(Tag::Strong) => self.state.strong += 1,
            Event::End(TagEnd::Strong) => self.state.strong = self.state.strong.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => self.state.strike += 1,
            Event::End(TagEnd::Strikethrough) => {
                self.state.strike = self.state.strike.saturating_sub(1)
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                self.links.push(PendingLink {
                    start: self.pending.len(),
                    url: dest_url.to_string(),
                });
                self.state.link = true;
            }
            Event::End(TagEnd::Link) => self.close_link(false),
            Event::Start(Tag::Image { dest_url, .. }) => {
                self.links.push(PendingLink {
                    start: self.pending.len(),
                    url: dest_url.to_string(),
                });
                self.pending.push(Run::plain("["));
                self.state.link = true;
            }
            Event::End(TagEnd::Image) => self.close_link(true),

            // ── blocks ──
            Event::Start(Tag::Paragraph) => {
                self.break_block();
            }
            Event::End(TagEnd::Paragraph) => {
                self.flush();
                // A tight list has no paragraph tags, so its items run
                // together; the gap returns once a real block appears.
                self.skip_next_blank = false;
            }
            Event::Start(Tag::Heading { level, .. }) => {
                self.break_block();
                self.in_heading = true;
                self.heading_level = Some(heading_number(level));
            }
            Event::End(TagEnd::Heading(_)) => {
                let runs = std::mem::take(&mut self.pending);
                self.in_heading = false;
                let level = self.heading_level.take();
                if !runs.is_empty() {
                    if let Some(level) = level {
                        let avail = self
                            .opts
                            .width
                            .saturating_sub(wrap::runs_width(&self.cont_prefix()))
                            .max(8);
                        let lines = blocks::heading_lines(level, &runs, avail);
                        self.emit_block(lines);
                    } else {
                        self.pending = runs;
                        self.flush();
                    }
                }
                self.blank();
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                self.break_block();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().unwrap_or("").to_string()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((lang, String::new()));
            }
            Event::Start(Tag::BlockQuote(kind)) => {
                self.break_block();
                let alert = kind.and_then(Alert::from_kind);
                if let Some(a) = alert {
                    let first = self.first_prefix();
                    let header = blocks::alert_header(a, &first);
                    self.out.push(header);
                }
                self.ctx.blockquote_depth += 1;
                self.ctx.alert = alert;
                self.stack.push(Frame::Quote { alert });
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                self.flush();
                self.stack.pop();
                self.quote_count = self.quote_count.saturating_sub(1);
                self.ctx.blockquote_depth = self.ctx.blockquote_depth.saturating_sub(1);
                self.ctx.alert = self.stack.iter().rev().find_map(|f| match f {
                    Frame::Quote { alert } => *alert,
                    _ => None,
                });
            }
            Event::Start(Tag::List(start)) => {
                self.break_block();
                let kind = match start {
                    Some(n) => ListKind::Ordered(n),
                    None => ListKind::Unordered,
                };
                self.stack.push(Frame::List { kind });
            }
            Event::End(TagEnd::List(_)) => {
                self.flush();
                self.stack.pop();
            }
            Event::Start(Tag::Item) => {
                self.flush();
                let depth = self
                    .stack
                    .iter()
                    .filter(|f| matches!(f, Frame::Item(_)))
                    .count()
                    + 1;
                // Draw this item's number off the list frame, which owns
                // the counter. Copying the kind instead would hand every
                // item the same number.
                let kind = match self
                    .stack
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f, Frame::List { .. }))
                {
                    Some(Frame::List { kind }) => match kind {
                        ListKind::Ordered(_) => ListKind::Ordered(kind.next_number().unwrap_or(1)),
                        ListKind::Unordered => ListKind::Unordered,
                    },
                    _ => ListKind::Unordered,
                };
                self.stack
                    .push(Frame::Item(ItemState::new(depth, kind, None)));
            }
            Event::End(TagEnd::Item) => {
                self.flush();
                self.stack.pop();
            }
            Event::Start(Tag::Table(aligns)) => {
                self.break_block();
                self.table = Some(TableBuild {
                    table: table::Table::default(),
                    aligns: aligns.clone(),
                    in_head: false,
                    row: Vec::new(),
                });
            }
            Event::DisplayMath(t) => {
                self.break_block();
                let indent = self.cont_prefix();
                let w = self
                    .opts
                    .width
                    .saturating_sub(wrap::runs_width(&indent))
                    .max(8);
                // One output line per rendered line: accumulating them into
                // `pending` folded a whole display block onto one line, which
                // is only invisible until the math needs more than one.
                let first = self.first_prefix();
                let mut out_lines: Vec<Line> = Vec::new();
                for (i, text) in latex::block_lines(&t, w).into_iter().enumerate() {
                    let mut runs = if i == 0 {
                        first.clone()
                    } else {
                        indent.clone()
                    };
                    runs.push(Run::new(text, theme::latex()));
                    out_lines.push(Line::from_runs(runs));
                }
                self.emit_block(out_lines);
            }
            Event::Start(Tag::FootnoteDefinition(name)) => {
                self.break_block();
                let first = self.first_prefix();
                self.out.push(Line::from_runs(first_with_text(
                    &first,
                    format!("[^{name}] "),
                )));
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                self.flush();
            }
            Event::Start(Tag::DefinitionList) => self.break_block(),
            Event::End(TagEnd::DefinitionList) => self.flush(),
            Event::Start(Tag::DefinitionListTitle) => {
                self.flush();
                self.blank();
            }
            Event::End(TagEnd::DefinitionListTitle) => {
                let runs = std::mem::take(&mut self.pending);
                let bold: Vec<Run> = runs
                    .iter()
                    .map(|r| Run::new(r.text.clone(), theme::strong().merged_over(r.style)))
                    .collect();
                self.pending = bold;
                self.flush();
            }
            Event::Start(Tag::DefinitionListDefinition) => {
                self.flush();
                self.blank();
            }
            Event::End(TagEnd::DefinitionListDefinition) => self.flush(),
            Event::Start(Tag::MetadataBlock(_)) => {
                self.in_metadata = true;
            }
            Event::End(TagEnd::MetadataBlock(_)) => {
                self.in_metadata = false;
            }
            _ => {}
        }
    }

    /// Append the destination to a finished link, unless the visible
    /// text is already the address.
    fn close_link(&mut self, image: bool) {
        self.state.link = false;
        let Some(link) = self.links.pop() else { return };
        let text_runs: Vec<Run> = self.pending[link.start..].to_vec();
        let plain: String = text_runs.iter().map(|r| r.text.as_str()).collect();
        if image {
            self.pending.push(Run::plain("]("));
            self.pending
                .push(Run::new(link.url.clone(), theme::link_url()));
            self.pending.push(Run::plain(")"));
            return;
        }
        // Drop trailing whitespace so "text " does not push the URL away.
        let mut trailing = 0usize;
        for r in text_runs.iter().rev() {
            if r.text.chars().all(char::is_whitespace) {
                trailing += r.text.len();
            } else {
                break;
            }
        }
        if trailing > 0 {
            self.pending.truncate(self.pending.len() - trailing);
        }
        let trimmed = plain.trim();
        if trimmed.is_empty() {
            // A bare autolink: the text is the address already.
            return;
        }
        if links::text_is_url(&text_runs, &link.url) {
            return;
        }
        self.pending.push(Run::plain(" "));
        inline::push_link_url(&mut self.pending, &link.url);
    }

    /// Render a fenced code block, dispatching on its language.
    fn render_code_block(&mut self, lang: &str, body: &str) {
        let body = width::expand_tabs(body, 0);
        let body = body.trim_end_matches('\n').to_string();
        if self.opts.linkify && lang.is_empty() {
            // A bare fence stays a code block; nothing to do.
        }

        if mermaid::is_mermaid_lang(lang) {
            if let Some(lines) = mermaid::render(&body) {
                let indent = self.cont_prefix();
                let avail = self
                    .opts
                    .width
                    .saturating_sub(wrap::runs_width(&indent))
                    .max(24);
                let boxed = code::box_lines("mermaid", lines, avail);
                self.emit_block(boxed);
                return;
            }
            // An unsupported diagram family falls through to the code
            // box, which still shows the source readably.
        }
        if latex::is_latex_lang(lang) {
            let indent = self.cont_prefix();
            let w = self
                .opts
                .width
                .saturating_sub(wrap::runs_width(&indent))
                .max(8);
            let mut out_lines: Vec<Line> = Vec::new();
            for text in latex::block_lines(&body, w) {
                out_lines.push(Line::from_runs(vec![Run::new(text, theme::latex())]));
            }
            self.emit_block(out_lines);
            return;
        }

        let indent = self.cont_prefix();
        let avail = self
            .opts
            .width
            .saturating_sub(wrap::runs_width(&indent))
            .max(24);
        let lines = code::render(&body, lang, avail, self.opts.line_numbers, &indent);
        self.push_lines(lines);
    }
}

impl Alert {
    fn from_kind(kind: BlockQuoteKind) -> Option<Alert> {
        match kind {
            BlockQuoteKind::Note => Some(Alert::Note),
            BlockQuoteKind::Tip => Some(Alert::Tip),
            BlockQuoteKind::Important => Some(Alert::Important),
            BlockQuoteKind::Warning => Some(Alert::Warning),
            BlockQuoteKind::Caution => Some(Alert::Caution),
        }
    }
}

/// Map a `pulldown-cmark` heading level to the 1-6 the theme uses.
fn heading_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Prepend text to a prefix, for block labels that need their own words.
fn first_with_text(prefix: &[Run], text: String) -> Vec<Run> {
    let mut runs = prefix.to_vec();
    runs.push(Run::new(text, theme::strong()));
    runs
}

/// Render Markdown to styled terminal lines.
pub fn render_lines(src: &str, opts: &Options) -> Vec<Line> {
    let events: Vec<Event> = Parser::new_ext(src, opts.cmark()).collect();
    let mut r = Renderer::new(opts);
    r.run(events);
    r.out
}

/// Render Markdown to a finished string, ANSI-styled unless disabled.
pub fn render(src: &str, opts: &Options) -> String {
    let lines = render_lines(src, opts);
    let mut out: Vec<u8> = Vec::new();
    {
        let mut w = if opts.color {
            ansi::AnsiWriter::styled(&mut out)
        } else {
            ansi::AnsiWriter::plain(&mut out)
        };
        for line in &lines {
            w.line(line, opts.width);
        }
    }
    // Every line is written with a trailing newline, so a non-empty
    // document always ends with one.
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_opts(width: usize) -> Options {
        Options {
            width,
            color: false,
            ..Options::default()
        }
    }

    fn render_plain(src: &str, width: usize) -> String {
        render(src, &plain_opts(width))
    }

    fn lines_of(src: &str, width: usize) -> Vec<String> {
        render_plain(src, width)
            .lines()
            .map(|l| l.to_string())
            .collect()
    }

    #[test]
    fn plain_text_renders() {
        let out = render_plain("hello world", 80);
        assert!(out.contains("hello world"));
    }

    #[test]
    fn color_output_contains_escapes() {
        let opts = Options {
            width: 80,
            color: true,
            ..Options::default()
        };
        let out = render("**bold**", &opts);
        assert!(out.contains('\u{1b}'), "no ANSI escapes emitted");
    }

    #[test]
    fn plain_output_has_no_escapes() {
        let out = render_plain("**bold**", 80);
        assert!(!out.contains('\u{1b}'));
    }

    #[test]
    fn no_line_exceeds_the_width() {
        let long = "word ".repeat(200);
        for l in lines_of(&long, 40) {
            assert!(width::display_width(&l) <= 40, "too wide: {l:?}");
        }
    }

    #[test]
    fn headings_are_styled() {
        let out = render_plain("# Title", 80);
        assert!(out.contains("Title"));
        assert!(out.contains('\u{2550}'), "h1 needs a rule: {out:?}");
    }

    #[test]
    fn paragraphs_are_separated_by_a_blank_line() {
        let out = lines_of("one\n\ntwo", 80);
        assert_eq!(out.len(), 3, "{out:?}");
        assert!(out[1].trim().is_empty());
    }

    #[test]
    fn inline_code_is_marked() {
        let out = render_plain("use `let x = 1` here", 80);
        assert!(out.contains("let x = 1"), "{out:?}");
    }

    #[test]
    fn fenced_code_gets_a_box() {
        let out = render_plain("```rust\nfn main() {}\n```", 80);
        assert!(out.contains('\u{250c}'), "no box: {out:?}");
        assert!(out.contains("fn main() {}"));
    }

    #[test]
    fn code_line_numbers_can_be_disabled() {
        let src = "```rust\nfn main() {}\n```";
        let with = render_plain(src, 80);
        let without = render(
            src,
            &Options {
                width: 80,
                color: false,
                line_numbers: false,
                ..Options::default()
            },
        );
        assert_ne!(with, without, "line numbers had no effect");
    }

    #[test]
    fn bullet_list_uses_bullets() {
        let out = render_plain("- one\n- two", 80);
        assert!(out.contains('\u{2022}'), "{out:?}");
        assert!(out.contains("one") && out.contains("two"));
    }

    #[test]
    fn ordered_list_numbers_items() {
        let out = render_plain("1. one\n2. two", 80);
        assert!(out.contains("1. one"), "{out:?}");
        assert!(out.contains("2. two"), "{out:?}");
    }

    #[test]
    fn ordered_list_preserves_the_start_number() {
        let out = render_plain("5. five\n6. six", 80);
        assert!(out.contains("5. five"), "{out:?}");
    }

    #[test]
    fn task_list_shows_checkboxes() {
        let out = render_plain("- [x] done\n- [ ] todo", 80);
        assert!(out.contains('\u{2611}'), "{out:?}");
        assert!(out.contains('\u{2610}'), "{out:?}");
    }

    #[test]
    fn nested_list_changes_the_marker() {
        let out = render_plain("- one\n  - inner", 80);
        assert!(out.contains('\u{25e6}'), "no nested marker: {out:?}");
    }

    #[test]
    fn blockquote_gets_a_rail() {
        let out = render_plain("> quoted", 80);
        assert!(out.contains('\u{258f}'), "no quote rail: {out:?}");
        assert!(out.contains("quoted"));
    }

    #[test]
    fn alert_quote_shows_its_label() {
        let out = render_plain("> [!WARNING]\n> careful", 80);
        assert!(out.to_lowercase().contains("warning"), "{out:?}");
    }

    #[test]
    fn thematic_break_is_a_rule() {
        let out = render_plain("---", 80);
        assert!(out.trim().chars().all(|c| c == '\u{2500}'), "{out:?}");
    }

    #[test]
    fn links_show_their_destination() {
        let out = render_plain("[docs](https://example.com)", 80);
        assert!(out.contains("docs"), "{out:?}");
        assert!(out.contains("https://example.com"), "{out:?}");
    }

    #[test]
    fn autolinks_are_not_duplicated() {
        let out = render_plain("<https://example.com>", 80);
        assert_eq!(out.matches("https://example.com").count(), 1, "{out:?}");
    }

    #[test]
    fn bare_urls_are_linkified() {
        let out = render_plain("visit https://example.com today", 80);
        assert!(out.contains("https://example.com"), "{out:?}");
    }

    #[test]
    fn linkify_can_be_disabled() {
        let out = render(
            "https://example.com",
            &Options {
                width: 80,
                color: false,
                linkify: false,
                ..Options::default()
            },
        );
        assert!(out.contains("https://example.com"));
    }

    #[test]
    fn strikethrough_survives() {
        let out = render_plain("~~gone~~", 80);
        assert!(out.contains("gone"), "{out:?}");
    }

    #[test]
    fn highlight_markup_renders() {
        let out = render_plain("==important==", 80);
        assert!(out.contains("important"), "{out:?}");
    }

    #[test]
    fn tables_render_a_grid() {
        let out = render_plain("| a | b |\n|---|---|\n| 1 | 2 |", 80);
        assert!(out.contains('\u{250c}'), "no table border: {out:?}");
        assert!(out.contains("a") && out.contains("2"));
    }

    #[test]
    fn table_rows_share_a_width() {
        let out = lines_of("| a | b |\n|---|---|\n| longer cell | 2 |", 60);
        let widths: Vec<usize> = out
            .iter()
            .filter(|l| l.contains('\u{2502}'))
            .map(|l| width::display_width(l))
            .collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]), "{widths:?}");
    }

    #[test]
    fn inline_math_converts() {
        let out = render_plain("cost is $\\alpha$ today", 80);
        assert!(out.contains('\u{3b1}'), "{out:?}");
    }

    #[test]
    fn display_math_converts() {
        let out = render_plain("$$\n\\frac{1}{2}\n$$", 80);
        assert!(out.contains('\u{2044}'), "{out:?}");
    }

    #[test]
    fn mermaid_flowchart_renders_nodes() {
        let out = render_plain("```mermaid\ngraph TD\nA[Start] --> B[End]\n```", 80);
        assert!(out.contains("Start"), "{out:?}");
        assert!(out.contains("End"), "{out:?}");
    }

    #[test]
    fn unsupported_mermaid_falls_back_to_source() {
        let out = render_plain("```mermaid\npie title X\n\"a\" : 1\n```", 80);
        assert!(out.contains("pie"), "fallback lost the source: {out:?}");
    }

    #[test]
    fn nested_blocks_indent_under_the_item() {
        let out = lines_of("- item\n\n  ```\n  code\n  ```", 80);
        let code_line = out.iter().find(|l| l.contains("code")).expect("code line");
        assert!(
            code_line.starts_with("  "),
            "code not indented under the bullet: {code_line:?}"
        );
    }

    #[test]
    fn yaml_frontmatter_is_hidden() {
        let out = render_plain("---\ntitle: x\n---\n\nbody", 80);
        assert!(!out.contains("title:"), "{out:?}");
        assert!(out.contains("body"));
    }

    #[test]
    fn smart_punctuation_can_be_disabled() {
        let out = render(
            "\"quoted\"",
            &Options {
                width: 80,
                color: false,
                smart_punctuation: false,
                ..Options::default()
            },
        );
        assert!(out.contains('"'), "{out:?}");
    }

    #[test]
    fn smart_punctuation_curls_quotes() {
        let out = render_plain("\"quoted\"", 80);
        assert!(out.contains('\u{201c}'), "{out:?}");
    }

    #[test]
    fn empty_input_produces_no_output() {
        assert_eq!(render_plain("", 80), "");
    }

    #[test]
    fn whitespace_only_input_produces_no_output() {
        assert_eq!(render_plain("   \n\n  ", 80), "");
    }

    #[test]
    fn very_narrow_widths_still_work() {
        for w in [20usize, 30, 40] {
            let out = render_plain("# Head\n\n- a\n- b\n\n```\ncode\n```", w);
            for l in out.lines() {
                assert!(width::display_width(l) <= w, "w={w}: {l:?}");
            }
        }
    }

    #[test]
    fn wide_characters_count_as_two_columns() {
        let out = lines_of("日本語のテキストはここにあります", 20);
        for l in out {
            assert!(width::display_width(&l) <= 20, "{l:?}");
        }
    }

    #[test]
    fn document_ends_with_a_newline() {
        let out = render_plain("text", 80);
        assert!(out.ends_with('\n'), "{out:?}");
    }

    #[test]
    fn a_full_readme_style_document_renders() {
        let src = "# Title\n\nIntro with **bold** and `code`.\n\n- one\n- two\n  - nested\n\n> quote\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n\n[link](https://example.com)\n\n---\n";
        let out = render_plain(src, 80);
        for l in out.lines() {
            assert!(width::display_width(l) <= 80, "overflow: {l:?}");
        }
        for needle in [
            "Title",
            "bold",
            "one",
            "nested",
            "quote",
            "fn main",
            "https://example.com",
        ] {
            assert!(out.contains(needle), "missing {needle} in {out:?}");
        }
    }
}

#[cfg(test)]
mod width_audit {
    use super::width::visible_width;

    /// The `/test` stress document, so `/test` and the invariants below
    /// are checked against the very same input.
    const DOC: &str = include_str!("../../testdata/render_test.md");

    /// Every line of output must fit the terminal, or the shell will
    /// hard-wrap it and break the box drawing.
    #[test]
    fn no_line_exceeds_the_configured_width() {
        for width in [20usize, 40, 72, 100, 200] {
            let opts = super::Options {
                width,
                color: false,
                ..Default::default()
            };
            for (n, line) in super::render(DOC, &opts).lines().enumerate() {
                let w = visible_width(line);
                assert!(
                    w <= width,
                    "width {width}: line {} is {w} cols: {line:?}",
                    n + 1
                );
            }
        }
    }

    /// Styling must not change what the text *says*: the visible width
    /// of the colored output has to match the plain output exactly.
    #[test]
    fn color_does_not_change_visible_width() {
        for width in [20usize, 40, 72, 120] {
            let plain = super::render(
                DOC,
                &super::Options {
                    width,
                    color: false,
                    ..Default::default()
                },
            );
            let colored = super::render(
                DOC,
                &super::Options {
                    width,
                    color: true,
                    ..Default::default()
                },
            );
            let w = |t: &str| -> Vec<usize> { t.lines().map(visible_width).collect() };
            assert_eq!(
                w(&plain),
                w(&colored),
                "visible width differs at width {width}"
            );
        }
    }

    /// pulldown-cmark emits no `TableRow` around the header row, so a
    /// renderer that only closes rows on `TableRow` splices the header
    /// onto the first body row and invents columns.
    #[test]
    fn table_header_is_its_own_row() {
        let src = "| A | B |\n| :- | -: |\n| 1 | 2 |\n| 3 | 4 |\n";
        let out = super::render(
            src,
            &super::Options {
                width: 80,
                color: false,
                ..Default::default()
            },
        );
        let rows: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
        let widths: Vec<usize> = rows.iter().map(|l| visible_width(l)).collect();
        for w in &widths {
            assert_eq!(*w, widths[0], "ragged table:\n{out}");
        }
        // Top rule, header, head separator, two body rows separated by a
        // rule, bottom rule.
        assert_eq!(rows.len(), 7, "{out}");
        assert!(out.contains("│ A "), "header missing: {out}");
        assert!(out.contains("│ 1 "), "first body row missing: {out}");
        assert!(out.contains("│ 3 "), "second body row missing: {out}");
    }

    /// An alignment row must set alignment, not become data.
    #[test]
    fn table_alignment_row_is_not_rendered_as_data() {
        let src = "| L | R |\n| :- | -: |\n| a | b |\n";
        let out = super::render(
            src,
            &super::Options {
                width: 80,
                color: false,
                ..Default::default()
            },
        );
        assert!(!out.contains(":-"), "delimiter row leaked as data: {out}");
        assert!(!out.contains("-:"), "delimiter row leaked as data: {out}");
    }

    /// A hostile width must not panic. The CLI floors this at 20, but
    /// the renderer is reachable from tests and should not care.
    #[test]
    fn degenerate_widths_do_not_panic() {
        for width in [0usize, 1, 2, 3, 5, 10] {
            let opts = super::Options {
                width,
                color: false,
                ..Default::default()
            };
            for src in [
                "",
                "word",
                "# heading",
                "| a | b |\n| - | - |\n| 1 | 2 |\n",
                "```rust\nfn main() {}\n```",
                "- item\n- item",
                "> quote",
                "$$x^2$$",
                "```mermaid\nflowchart TD\n  A --> B\n```",
                "https://example.com/some/very/long/path/that/exceeds/width",
            ] {
                let _ = super::render(src, &opts);
            }
        }
    }

    /// A code box must be sized for its widest *line*. Highlighting
    /// splits a line into short tokens, so sizing to the widest token
    /// left every block wrapping far earlier than it needed to.
    #[test]
    fn code_box_fits_its_widest_line() {
        let src =
            "```python\ndef fibonacci(n):\n    return fibonacci(n - 1) + fibonacci(n - 2)\n```\n";
        let out = super::render(
            src,
            &super::Options {
                width: 100,
                color: false,
                ..Default::default()
            },
        );
        let longest = "    return fibonacci(n - 1) + fibonacci(n - 2)";
        assert!(out.contains(longest), "widest line was wrapped:\n{out}");
        // And nothing may exceed the terminal.
        for (n, line) in out.lines().enumerate() {
            assert!(
                visible_width(line) <= 100,
                "line {} too wide: {line:?}",
                n + 1
            );
        }
    }

    /// Every line of a multi-line block keeps the blockquote rail. The
    /// continuation width was measured at depth 0, so a top-level quote
    /// indented by nothing and the bar vanished below the first line.
    #[test]
    fn a_blockquote_rail_runs_the_full_height_of_a_block() {
        let src = "> | a | b |\n> |---|---|\n> | 1 | 2 |\n";
        let out = super::render(
            src,
            &super::Options {
                width: 40,
                color: false,
                ..Default::default()
            },
        );
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.len() >= 5, "{lines:?}");
        for l in &lines {
            assert!(l.starts_with('\u{258f}'), "rail missing on {l:?}");
        }
    }

    #[test]
    fn plain_mode_emits_no_escapes() {
        let opts = super::Options {
            color: false,
            ..Default::default()
        };
        let out = super::render(DOC, &opts);
        assert!(!out.contains('\x1b'), "plain output leaked an escape byte");
    }
}
