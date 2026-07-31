//! Paints parsed markdown blocks with egui.
//!
//! Text is borrowed from the secure buffer and re-parsed each frame
//! (immediate mode); pulldown-cmark easily keeps up with document sizes
//! this app targets. Links are styled but deliberately not clickable —
//! opening a URL would leak document content into browser history.

use egui::text::LayoutJob;
use egui::{Color32, FontId, Stroke, TextFormat, Ui};

use super::highlight::{self, Counter, Highlight};
use super::theme;
use crate::document::markdown::{parse_blocks_with_lines, Block, Marker, Span, SpanStyle};

/// Per-render state threaded through the block renderers: wrap mode plus
/// the live search and its running match index.
struct Render<'a> {
    wrap: bool,
    find: Option<Highlight<'a>>,
    counter: Counter,
}

/// Render markdown content into the given Ui. `wrap` controls long-line
/// wrapping for headings/paragraphs/code (off = horizontal scrolling,
/// when hosted in a horizontal-capable scroll area); `line_numbers` adds
/// a source-line gutter per rendered block.
pub fn render(
    ui: &mut Ui,
    content: &str,
    wrap: bool,
    line_numbers: bool,
    find: Option<Highlight<'_>>,
) {
    let ctx = Render {
        wrap,
        find,
        counter: Counter::new(),
    };
    let (blocks, lines) = parse_blocks_with_lines(content);

    ui.spacing_mut().item_spacing.y = 6.0;

    let number_width = lines.iter().max().copied().unwrap_or(1).to_string().len();

    for (i, block) in blocks.iter().enumerate() {
        if line_numbers {
            let line = lines.get(i).copied().unwrap_or(1);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.label(
                    egui::RichText::new(format!("{line:>number_width$}"))
                        .font(egui::FontId::monospace(theme::FONT_SIZE - 2.0))
                        .color(theme::text_dim()),
                );
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    render_block(ui, i, block, &ctx);
                });
            });
        } else {
            render_block(ui, i, block, &ctx);
        }
    }
}

fn render_block(ui: &mut Ui, i: usize, block: &Block<'_>, ctx: &Render<'_>) {
    match block {
        Block::Heading { level, spans } => render_heading(ui, *level, spans, ctx),
        Block::Paragraph {
            spans,
            indent,
            marker,
            quote,
        } => {
            if *quote > 0 {
                render_quote(ui, spans, *quote, ctx);
            } else {
                render_paragraph(ui, spans, *indent, *marker, ctx);
            }
        }
        Block::CodeBlock { lang, text, indent } => {
            render_code_block(ui, lang.as_deref(), text, *indent, ctx)
        }
        Block::Rule => {
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);
        }
        Block::Table { header, rows } => render_table(ui, i, header, rows, ctx),
    }
}

/// A block-level text label: wrapped, or extended for horizontal scroll.
/// The wrap mode is set EXPLICITLY: egui's default is wrap-in-vertical
/// but extend-in-horizontal layouts, so list items (marker + text rows)
/// would otherwise silently run off screen even with word wrap on.
fn block_label(ui: &mut Ui, job: LayoutJob, wrap: bool) {
    if wrap {
        ui.add(egui::Label::new(job).wrap());
    } else {
        ui.add(egui::Label::new(job).extend());
    }
}

/// Build a LayoutJob from styled spans at the given base font size,
/// painting search matches within each span.
///
/// Matches are found per span, so one that straddles an inline-style
/// boundary (e.g. half inside `**bold**`) isn't highlighted — the find
/// bar's own count is authoritative.
fn layout_spans(
    spans: &[Span<'_>],
    base_size: f32,
    base_color: Color32,
    ctx: &Render<'_>,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    for span in spans {
        let fmt = format_for(span.style, base_size, base_color);
        highlight::append(&mut job, &span.text, &fmt, ctx.find, &ctx.counter);
    }
    job
}

fn format_for(style: SpanStyle, base_size: f32, base_color: Color32) -> TextFormat {
    let mut fmt = TextFormat {
        font_id: FontId::proportional(base_size),
        color: base_color,
        ..Default::default()
    };
    if style.code {
        fmt.font_id = FontId::monospace(base_size * 0.92);
        fmt.color = theme::text_code();
        fmt.background = theme::bg_code();
    }
    if style.bold {
        // egui's default fonts have no bold weight; brighten instead.
        fmt.color = theme::text_strong();
    }
    if style.italic {
        fmt.italics = true;
    }
    if style.strike {
        fmt.strikethrough = Stroke::new(1.0, theme::text_dim());
    }
    if style.link {
        fmt.color = theme::accent();
        fmt.underline = Stroke::new(1.0, theme::accent());
    }
    fmt
}

fn render_heading(ui: &mut Ui, level: u8, spans: &[Span<'_>], ctx: &Render<'_>) {
    let idx = (level.clamp(1, 6) - 1) as usize;
    let size = theme::HEADING_SIZES[idx];

    ui.add_space(if idx == 0 { 10.0 } else { 8.0 });
    let job = layout_spans(spans, size, theme::text_strong(), ctx);
    block_label(ui, job, ctx.wrap);
    if idx == 0 {
        ui.separator();
    }
    ui.add_space(2.0);
}

fn render_paragraph(
    ui: &mut Ui,
    spans: &[Span<'_>],
    indent: u8,
    marker: Option<Marker>,
    ctx: &Render<'_>,
) {
    let job = layout_spans(spans, theme::FONT_SIZE, theme::text_primary(), ctx);

    if indent == 0 && marker.is_none() {
        block_label(ui, job, ctx.wrap);
        return;
    }

    ui.horizontal_top(|ui| {
        ui.add_space(theme::MD_INDENT * indent.saturating_sub(1) as f32);
        if indent > 0 || marker.is_some() {
            ui.add_space(theme::MD_INDENT * 0.25);
        }
        match marker {
            Some(Marker::Bullet) => {
                ui.label(
                    egui::RichText::new("•")
                        .size(theme::FONT_SIZE)
                        .color(theme::text_dim()),
                );
            }
            Some(Marker::Number(n)) => {
                ui.label(
                    egui::RichText::new(format!("{n}."))
                        .size(theme::FONT_SIZE)
                        .color(theme::text_dim()),
                );
            }
            Some(Marker::Task(checked)) => {
                ui.label(
                    egui::RichText::new(if checked { "\u{2611}" } else { "\u{2610}" })
                        .size(theme::FONT_SIZE)
                        .color(if checked {
                            theme::accent()
                        } else {
                            theme::text_dim()
                        }),
                );
            }
            None => {
                // Continuation paragraph inside a list item — align with text.
                ui.add_space(theme::MD_INDENT * 0.75);
            }
        }
        block_label(ui, job, ctx.wrap);
    });
}

fn render_quote(ui: &mut Ui, spans: &[Span<'_>], depth: u8, ctx: &Render<'_>) {
    let job = layout_spans(spans, theme::FONT_SIZE, theme::text_dim(), ctx);
    let response = egui::Frame::NONE
        .fill(theme::bg_quote())
        .corner_radius(3.0)
        .inner_margin(egui::Margin {
            left: (10 + 12 * (depth as i8 - 1)),
            right: 10,
            top: 6,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(job);
        });

    // Accent bar along the quote's actual (post-layout) left edge.
    let rect = response.response.rect;
    ui.painter().vline(
        rect.left() + 1.5,
        rect.y_range(),
        Stroke::new(3.0, theme::quote_bar()),
    );
}

fn render_code_block(ui: &mut Ui, lang: Option<&str>, text: &str, indent: u8, ctx: &Render<'_>) {
    ui.horizontal_top(|ui| {
        ui.add_space(theme::MD_INDENT * indent as f32);
        egui::Frame::NONE
            .fill(theme::bg_code())
            .corner_radius(4.0)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.vertical(|ui| {
                    if let Some(lang) = lang {
                        ui.label(
                            egui::RichText::new(lang)
                                .size(theme::FONT_SIZE_STATUS)
                                .color(theme::text_dim())
                                .monospace(),
                        );
                    }
                    let mut job = LayoutJob::default();
                    let fmt = TextFormat {
                        font_id: FontId::monospace(theme::FONT_SIZE * 0.92),
                        color: theme::text_code(),
                        ..Default::default()
                    };
                    highlight::append(&mut job, text, &fmt, ctx.find, &ctx.counter);
                    block_label(ui, job, ctx.wrap);
                });
            });
    });
}

/// Horizontal gap between table columns.
const TABLE_H_SPACING: f32 = 18.0;
/// The table frame's `inner_margin(8)` on both sides.
const TABLE_FRAME_PAD: f32 = 16.0;
/// Minimum column width, so a many-column table in a narrow window stays
/// legible (and scrolls) rather than collapsing to slivers.
const TABLE_MIN_COL: f32 = 48.0;

/// Width for each of `ncols` columns sharing `avail` pixels.
///
/// egui's Grid otherwise sizes columns to their content's minimum
/// wrapped width, which squeezes cells into a narrow ribbon no matter how
/// wide the window is — the bug this exists to fix.
fn table_col_width(avail: f32, ncols: usize) -> f32 {
    let ncols = ncols.max(1);
    let gaps = TABLE_H_SPACING * (ncols - 1) as f32;
    ((avail - gaps) / ncols as f32).max(TABLE_MIN_COL)
}

fn render_table(
    ui: &mut Ui,
    table_idx: usize,
    header: &[Vec<Span<'_>>],
    rows: &[Vec<Vec<Span<'_>>>],
    ctx: &Render<'_>,
) {
    // Columns are sized to share the full width of the view. Without
    // this, egui's Grid sizes each column to its content's *minimum*
    // wrapped width, which squeezes every cell into a narrow ribbon no
    // matter how wide the window is.
    let ncols = header
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0))
        .max(1);
    let avail = (ui.available_width() - TABLE_FRAME_PAD).max(0.0);
    let col_width = table_col_width(avail, ncols);

    ui.add_space(4.0);
    egui::Frame::NONE
        .fill(theme::bg_quote())
        .corner_radius(4.0)
        .inner_margin(8)
        .show(ui, |ui| {
            ui.set_min_width(avail);
            egui::Grid::new(("md_table", table_idx))
                .striped(true)
                .num_columns(ncols)
                .spacing([TABLE_H_SPACING, 6.0])
                .min_col_width(col_width)
                .max_col_width(col_width)
                .show(ui, |ui| {
                    // Explicit wrap: grid cells count as horizontal layout,
                    // where labels would otherwise extend off screen.
                    for cell in header {
                        ui.add(
                            egui::Label::new(layout_spans(
                                cell,
                                theme::FONT_SIZE,
                                theme::text_strong(),
                                ctx,
                            ))
                            .wrap(),
                        );
                    }
                    ui.end_row();
                    for row in rows {
                        for cell in row {
                            ui.add(
                                egui::Label::new(layout_spans(
                                    cell,
                                    theme::FONT_SIZE,
                                    theme::text_primary(),
                                    ctx,
                                ))
                                .wrap(),
                            );
                        }
                        ui.end_row();
                    }
                });
        });
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_columns_share_the_available_width() {
        // Three columns in a wide view: each gets roughly a third, and
        // the total (columns + gaps) fills the space rather than
        // collapsing to content width.
        let avail = 900.0;
        let w = table_col_width(avail, 3);
        let used = w * 3.0 + TABLE_H_SPACING * 2.0;
        assert!(
            (used - avail).abs() < 0.5,
            "columns should fill {avail}, used {used}"
        );
        assert!(w > 250.0, "each column should be wide, got {w}");
    }

    #[test]
    fn table_columns_have_a_floor_when_cramped() {
        // Many columns in a narrow view fall back to the minimum rather
        // than going to zero (or negative).
        let w = table_col_width(120.0, 8);
        assert_eq!(w, TABLE_MIN_COL);
    }

    #[test]
    fn single_column_uses_everything() {
        assert!((table_col_width(500.0, 1) - 500.0).abs() < f32::EPSILON);
    }
}
