//! Markdown parsing into a flat list of styled blocks.
//!
//! This module is pure data transformation (no UI): it walks pulldown-cmark
//! events and produces `Block`s whose spans borrow from the input `&str`
//! wherever possible. The UI layer (`crate::ui::markdown`) paints these
//! blocks each frame — nothing here retains plaintext beyond the frame.

use pulldown_cmark::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};

/// Inline style flags accumulated from nested markdown tags.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SpanStyle {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: bool,
}

/// A run of text with a single style.
pub struct Span<'a> {
    pub text: CowStr<'a>,
    pub style: SpanStyle,
}

/// List item marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Bullet,
    Number(u64),
    Task(bool),
}

/// A block-level element, flattened for simple sequential rendering.
#[allow(clippy::enum_variant_names)] // `CodeBlock` is the standard markdown term
pub enum Block<'a> {
    Heading {
        level: u8,
        spans: Vec<Span<'a>>,
    },
    Paragraph {
        spans: Vec<Span<'a>>,
        /// List nesting depth (0 = not in a list).
        indent: u8,
        /// Marker for the first paragraph of a list item.
        marker: Option<Marker>,
        /// Block-quote nesting depth (0 = not quoted).
        quote: u8,
    },
    CodeBlock {
        lang: Option<CowStr<'a>>,
        text: String,
        indent: u8,
    },
    Rule,
    Table {
        header: Vec<Vec<Span<'a>>>,
        rows: Vec<Vec<Vec<Span<'a>>>>,
    },
}

struct Ctx<'a> {
    blocks: Vec<Block<'a>>,
    spans: Vec<Span<'a>>,
    style: SpanStyle,
    heading: Option<u8>,
    quote: u8,
    /// One entry per open list; `Some(next_index)` for ordered lists.
    lists: Vec<Option<u64>>,
    /// Marker waiting to be attached to the next flushed paragraph.
    pending_marker: Option<Marker>,
    /// Open fenced/indented code block: (language, accumulated text).
    code: Option<(Option<CowStr<'a>>, String)>,
    table: Option<TableCtx<'a>>,
}

struct TableCtx<'a> {
    in_head: bool,
    header: Vec<Vec<Span<'a>>>,
    rows: Vec<Vec<Vec<Span<'a>>>>,
    current_row: Vec<Vec<Span<'a>>>,
}

impl<'a> Ctx<'a> {
    fn push_text(&mut self, text: CowStr<'a>, style: SpanStyle) {
        if text.is_empty() {
            return;
        }
        self.spans.push(Span { text, style });
    }

    /// Flush accumulated spans as a paragraph (or heading) block.
    fn flush(&mut self) {
        if self.spans.is_empty() {
            return;
        }
        let spans = std::mem::take(&mut self.spans);

        if let Some(table) = &mut self.table {
            // Inside a table, span runs are flushed per-cell elsewhere;
            // stray flushes (e.g. inline HTML) fold into the current cell.
            table.current_row.push(spans);
            return;
        }

        if let Some(level) = self.heading {
            self.blocks.push(Block::Heading { level, spans });
        } else {
            let indent = self.lists.len() as u8;
            self.blocks.push(Block::Paragraph {
                spans,
                indent,
                marker: self.pending_marker.take(),
                quote: self.quote,
            });
        }
    }
}

/// Parse markdown into a flat block list (the renderer uses
/// [`parse_blocks_with_lines`]; this thin wrapper serves the tests).
#[cfg_attr(not(test), allow(dead_code))]
pub fn parse_blocks(content: &str) -> Vec<Block<'_>> {
    parse_inner(content).0
}

/// Parse markdown into blocks plus each block's 1-based source line
/// (for the viewer's line-number gutter).
pub fn parse_blocks_with_lines(content: &str) -> (Vec<Block<'_>>, Vec<usize>) {
    let (blocks, offsets) = parse_inner(content);
    // Convert byte offsets to line numbers in one pass (offsets are
    // non-decreasing in source order).
    let bytes = content.as_bytes();
    let mut lines = Vec::with_capacity(offsets.len());
    let mut line = 1usize;
    let mut pos = 0usize;
    for off in offsets {
        let off = off.min(bytes.len());
        while pos < off {
            if bytes[pos] == b'\n' {
                line += 1;
            }
            pos += 1;
        }
        lines.push(line);
    }
    (blocks, lines)
}

fn parse_inner(content: &str) -> (Vec<Block<'_>>, Vec<usize>) {
    let options =
        Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(content, options);

    let mut ctx = Ctx {
        blocks: Vec::new(),
        spans: Vec::new(),
        style: SpanStyle::default(),
        heading: None,
        quote: 0,
        lists: Vec::new(),
        pending_marker: None,
        code: None,
        table: None,
    };

    // Source byte offset where each pushed block starts. pulldown-cmark's
    // offset iterator gives every event its full source range; blocks are
    // pushed while handling an event whose range covers them, so after
    // each event any newly pushed blocks get that event's start offset.
    let mut offsets: Vec<usize> = Vec::new();

    for (event, range) in parser.into_offset_iter() {
        let event_start = range.start;
        match event {
            Event::Start(tag) => start_tag(&mut ctx, tag),
            Event::End(tag) => end_tag(&mut ctx, tag),

            Event::Text(text) => {
                if let Some((_, buf)) = &mut ctx.code {
                    buf.push_str(&text);
                } else {
                    let style = ctx.style;
                    ctx.push_text(text, style);
                }
            }
            Event::Code(text) => {
                let style = SpanStyle {
                    code: true,
                    ..ctx.style
                };
                ctx.push_text(text, style);
            }
            Event::SoftBreak => {
                let style = ctx.style;
                ctx.push_text(CowStr::Borrowed(" "), style);
            }
            Event::HardBreak => {
                // Flush the line as a paragraph continuation.
                ctx.flush();
            }
            Event::Rule => {
                ctx.flush();
                ctx.blocks.push(Block::Rule);
            }
            Event::TaskListMarker(checked) => {
                ctx.pending_marker = Some(Marker::Task(checked));
            }
            Event::Html(text) | Event::InlineHtml(text) => {
                // Render raw HTML literally, styled as code.
                let style = SpanStyle {
                    code: true,
                    ..ctx.style
                };
                ctx.push_text(text, style);
            }
            Event::FootnoteReference(name) => {
                let style = ctx.style;
                ctx.push_text(CowStr::from(format!("[^{name}]")), style);
            }
            Event::InlineMath(text) | Event::DisplayMath(text) => {
                let style = SpanStyle {
                    code: true,
                    ..ctx.style
                };
                ctx.push_text(text, style);
            }
        }
        while offsets.len() < ctx.blocks.len() {
            offsets.push(event_start);
        }
    }

    ctx.flush();
    while offsets.len() < ctx.blocks.len() {
        offsets.push(content.len());
    }
    (ctx.blocks, offsets)
}

fn start_tag<'a>(ctx: &mut Ctx<'a>, tag: Tag<'a>) {
    match tag {
        Tag::Paragraph => {}
        Tag::Heading { level, .. } => {
            ctx.flush();
            ctx.heading = Some(level as u8);
        }
        Tag::BlockQuote(_) => {
            ctx.flush();
            ctx.quote += 1;
        }
        Tag::CodeBlock(kind) => {
            ctx.flush();
            let lang = match kind {
                CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang),
                _ => None,
            };
            ctx.code = Some((lang, String::new()));
        }
        Tag::List(start) => {
            ctx.flush();
            ctx.lists.push(start);
        }
        Tag::Item => {
            ctx.flush();
            ctx.pending_marker = Some(match ctx.lists.last_mut() {
                Some(Some(n)) => {
                    let marker = Marker::Number(*n);
                    *n += 1;
                    marker
                }
                _ => Marker::Bullet,
            });
        }
        Tag::Emphasis => ctx.style.italic = true,
        Tag::Strong => ctx.style.bold = true,
        Tag::Strikethrough => ctx.style.strike = true,
        Tag::Link { .. } | Tag::Image { .. } => ctx.style.link = true,
        Tag::Table(_) => {
            ctx.flush();
            ctx.table = Some(TableCtx {
                in_head: false,
                header: Vec::new(),
                rows: Vec::new(),
                current_row: Vec::new(),
            });
        }
        Tag::TableHead => {
            if let Some(t) = &mut ctx.table {
                t.in_head = true;
            }
        }
        Tag::TableRow | Tag::TableCell => {}
        _ => {}
    }
}

fn end_tag(ctx: &mut Ctx<'_>, tag: TagEnd) {
    match tag {
        TagEnd::Paragraph => ctx.flush(),
        TagEnd::Heading(_) => {
            ctx.flush();
            ctx.heading = None;
        }
        TagEnd::BlockQuote(..) => {
            ctx.flush();
            ctx.quote = ctx.quote.saturating_sub(1);
        }
        TagEnd::CodeBlock => {
            if let Some((lang, mut text)) = ctx.code.take() {
                // Trim the trailing newline fences leave behind
                if text.ends_with('\n') {
                    text.pop();
                }
                let indent = ctx.lists.len() as u8;
                ctx.blocks.push(Block::CodeBlock { lang, text, indent });
            }
        }
        TagEnd::List(_) => {
            ctx.flush();
            ctx.lists.pop();
        }
        TagEnd::Item => {
            ctx.flush();
            // An empty item still consumes its marker.
            ctx.pending_marker = None;
        }
        TagEnd::Emphasis => ctx.style.italic = false,
        TagEnd::Strong => ctx.style.bold = false,
        TagEnd::Strikethrough => ctx.style.strike = false,
        TagEnd::Link | TagEnd::Image => ctx.style.link = false,
        TagEnd::TableCell => {
            let spans = std::mem::take(&mut ctx.spans);
            if let Some(t) = &mut ctx.table {
                t.current_row.push(spans);
            }
        }
        TagEnd::TableHead => {
            if let Some(t) = &mut ctx.table {
                t.header = std::mem::take(&mut t.current_row);
                t.in_head = false;
            }
        }
        TagEnd::TableRow => {
            if let Some(t) = &mut ctx.table {
                let row = std::mem::take(&mut t.current_row);
                t.rows.push(row);
            }
        }
        TagEnd::Table => {
            if let Some(t) = ctx.table.take() {
                ctx.blocks.push(Block::Table {
                    header: t.header,
                    rows: t.rows,
                });
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(spans: &[Span<'_>]) -> String {
        spans.iter().map(|s| s.text.as_ref()).collect()
    }

    #[test]
    fn headings_and_paragraphs() {
        let blocks = parse_blocks("# Title\n\nBody text here.\n\n## Sub");
        assert_eq!(blocks.len(), 3);
        match &blocks[0] {
            Block::Heading { level, spans } => {
                assert_eq!(*level, 1);
                assert_eq!(text_of(spans), "Title");
            }
            _ => panic!("expected heading"),
        }
        match &blocks[1] {
            Block::Paragraph { spans, indent, .. } => {
                assert_eq!(text_of(spans), "Body text here.");
                assert_eq!(*indent, 0);
            }
            _ => panic!("expected paragraph"),
        }
        assert!(matches!(&blocks[2], Block::Heading { level: 2, .. }));
    }

    #[test]
    fn inline_styles() {
        let blocks = parse_blocks("plain **bold** *italic* `code` ~~gone~~");
        let Block::Paragraph { spans, .. } = &blocks[0] else {
            panic!("expected paragraph");
        };
        let styled: Vec<(&str, SpanStyle)> =
            spans.iter().map(|s| (s.text.as_ref(), s.style)).collect();
        assert!(styled.contains(&(
            "bold",
            SpanStyle {
                bold: true,
                ..Default::default()
            }
        )));
        assert!(styled.contains(&(
            "italic",
            SpanStyle {
                italic: true,
                ..Default::default()
            }
        )));
        assert!(styled.contains(&(
            "code",
            SpanStyle {
                code: true,
                ..Default::default()
            }
        )));
        assert!(styled.contains(&(
            "gone",
            SpanStyle {
                strike: true,
                ..Default::default()
            }
        )));
    }

    #[test]
    fn lists_bullets_numbers_tasks() {
        let md = "- one\n- two\n\n1. first\n2. second\n\n- [x] done\n- [ ] todo";
        let blocks = parse_blocks(md);
        let markers: Vec<Option<Marker>> = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph { marker, .. } => Some(*marker),
                _ => None,
            })
            .collect();
        assert_eq!(
            markers,
            vec![
                Some(Marker::Bullet),
                Some(Marker::Bullet),
                Some(Marker::Number(1)),
                Some(Marker::Number(2)),
                Some(Marker::Task(true)),
                Some(Marker::Task(false)),
            ]
        );
    }

    #[test]
    fn nested_list_indent() {
        let blocks = parse_blocks("- outer\n  - inner");
        let indents: Vec<u8> = blocks
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph { indent, .. } => Some(*indent),
                _ => None,
            })
            .collect();
        assert_eq!(indents, vec![1, 2]);
    }

    #[test]
    fn block_quote_depth() {
        let blocks = parse_blocks("> quoted\n\nnot quoted");
        match &blocks[0] {
            Block::Paragraph { quote, spans, .. } => {
                assert_eq!(*quote, 1);
                assert_eq!(text_of(spans), "quoted");
            }
            _ => panic!("expected paragraph"),
        }
        match &blocks[1] {
            Block::Paragraph { quote, .. } => assert_eq!(*quote, 0),
            _ => panic!("expected paragraph"),
        }
    }

    #[test]
    fn fenced_code_block() {
        let blocks = parse_blocks("```rust\nfn main() {}\n```");
        match &blocks[0] {
            Block::CodeBlock { lang, text, .. } => {
                assert_eq!(lang.as_deref(), Some("rust"));
                assert_eq!(text, "fn main() {}");
            }
            _ => panic!("expected code block"),
        }
    }

    #[test]
    fn rule_and_link() {
        let blocks = parse_blocks("above\n\n---\n\n[click](https://example.com)");
        assert!(matches!(blocks[1], Block::Rule));
        let Block::Paragraph { spans, .. } = &blocks[2] else {
            panic!("expected paragraph");
        };
        assert!(spans[0].style.link);
        assert_eq!(spans[0].text.as_ref(), "click");
    }

    #[test]
    fn simple_table() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |\n";
        let blocks = parse_blocks(md);
        match &blocks[0] {
            Block::Table { header, rows } => {
                assert_eq!(header.len(), 2);
                assert_eq!(text_of(&header[0]), "a");
                assert_eq!(rows.len(), 1);
                assert_eq!(text_of(&rows[0][1]), "2");
            }
            _ => panic!("expected table"),
        }
    }

    #[test]
    fn hard_break_splits_paragraph() {
        let blocks = parse_blocks("line one  \nline two");
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn blocks_carry_source_lines() {
        let md = "# Title\n\npara one\n\n```\ncode\n```\n";
        let (blocks, lines) = parse_blocks_with_lines(md);
        assert_eq!(blocks.len(), lines.len());
        assert_eq!(lines[0], 1, "heading starts on line 1");
        assert_eq!(lines[1], 3, "paragraph starts on line 3");
        assert_eq!(lines[2], 5, "code fence starts on line 5");
    }
}
