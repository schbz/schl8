use egui::text::TextFormat;
use egui::{FontId, RichText, ScrollArea, TextStyle, Ui};

use super::highlight::{Counter, Highlight};
use super::theme;
use crate::crypto::secure_buf::SecureString;
use crate::document::FileType;

/// The read-only view's actual scroll position after this frame — lets
/// the caller keep the "Line x of y" display in sync with mouse/trackpad
/// scrolling, not just keyboard actions.
pub struct ScrollInfo {
    pub offset_y: f32,
    pub content_height: f32,
    pub viewport_height: f32,
}

impl ScrollInfo {
    /// The 1-based top line implied by the scroll position, proportional
    /// to the scrollable range (exact for uniform line heights, a close
    /// approximation for rendered markdown).
    pub fn current_line(&self, total_lines: usize) -> usize {
        let max_scroll = (self.content_height - self.viewport_height).max(0.0);
        if max_scroll <= 0.0 || total_lines <= 1 {
            return 1;
        }
        let frac = (self.offset_y / max_scroll).clamp(0.0, 1.0);
        1 + (frac * (total_lines - 1) as f32).round() as usize
    }
}

/// Display options for the text views, straight from `[appearance]`,
/// plus the live search (when the find bar is open).
#[derive(Clone, Copy)]
pub struct ViewOptions<'a> {
    /// Wrap long lines (off = horizontal scrolling).
    pub word_wrap: bool,
    /// Line-number gutter (plaintext view always; editor when wrap is
    /// off, where rows map 1:1 to lines).
    pub line_numbers: bool,
    /// Search matches to paint, if a find is in progress.
    pub find: Option<Highlight<'a>>,
}

/// Render document content in a read-only scrollable area.
/// Borrows &str from the caller's SecureBuffer — no copies of plaintext.
/// `jump_to` scrolls to an absolute vertical offset (find navigation).
pub fn render(
    ui: &mut Ui,
    content: &str,
    file_type: FileType,
    pending_scroll: &mut f32,
    jump_to: &mut Option<f32>,
    opts: ViewOptions<'_>,
) -> ScrollInfo {
    let delta = std::mem::take(pending_scroll);

    let mut area = if opts.word_wrap {
        ScrollArea::vertical()
    } else {
        ScrollArea::both()
    }
    .auto_shrink([false, false]);

    if let Some(offset) = jump_to.take() {
        area = area.vertical_scroll_offset(offset.max(0.0));
    } else if delta == f32::MIN {
        // GoToTop
        area = area.vertical_scroll_offset(0.0);
    } else if delta == f32::MAX {
        // GoToBottom — set a very large offset, egui will clamp it
        area = area.vertical_scroll_offset(f32::MAX);
    }

    let output = area.show(ui, |ui| {
        // Apply incremental scroll delta via fake scroll event
        if delta != 0.0 && delta != f32::MIN && delta != f32::MAX {
            ui.scroll_with_delta(egui::vec2(0.0, -delta));
        }

        match file_type {
            FileType::PlainText => {
                ui.style_mut().override_text_style = Some(TextStyle::Monospace);
                ui.style_mut().spacing.item_spacing.y = theme::LINE_SPACING;
                render_plaintext(ui, content, opts);
            }
            FileType::Markdown => {
                super::markdown::render(ui, content, opts.word_wrap, opts.line_numbers, opts.find)
            }
        }
    });

    ScrollInfo {
        offset_y: output.state.offset.y,
        content_height: output.content_size.y,
        viewport_height: output.inner_rect.height(),
    }
}

/// Render the document in an editable text area.
/// Returns (changed this frame, scroll info).
///
/// The editor is frameless and fills the whole content area — the same
/// single surface as read mode, with no inner text box. Edit mode is
/// signaled by the caret and the statusbar's EDIT badge instead.
pub fn render_editable(
    ui: &mut Ui,
    buffer: &mut SecureString,
    jump_to: &mut Option<f32>,
    opts: ViewOptions<'_>,
) -> (bool, ScrollInfo) {
    // Reserve enough rows that the editable region covers the visible
    // window even for short documents, so clicking anywhere lands in
    // the editor rather than on dead space below it.
    let row_height = ui.text_style_height(&TextStyle::Monospace);
    let fill_rows = ((ui.available_height() / row_height).ceil() as usize).max(4);

    // With wrap off, size the editor to its longest line (approximate
    // monospace advance) so horizontal scrolling covers the content
    // without a mile of dead space.
    let unwrapped_width = if opts.word_wrap {
        None
    } else {
        let max_chars = buffer.as_str().lines().map(str::len).max().unwrap_or(0);
        Some((max_chars as f32 * row_height * 0.62 + 80.0).max(ui.available_width()))
    };

    // Line numbers in the editor only when wrap is off — that's when
    // visual rows map 1:1 to logical lines, keeping the gutter honest.
    let gutter = (opts.line_numbers && !opts.word_wrap).then(|| {
        let lines = buffer.as_str().lines().count().max(1);
        let width = lines.to_string().len();
        let mut g = String::with_capacity(lines * (width + 1));
        for i in 1..=lines.max(fill_rows) {
            use std::fmt::Write;
            let _ = writeln!(g, "{i:>width$}");
        }
        g
    });

    let mut area = if opts.word_wrap {
        ScrollArea::vertical()
    } else {
        ScrollArea::both()
    }
    .auto_shrink([false, false]);
    if let Some(offset) = jump_to.take() {
        area = area.vertical_scroll_offset(offset.max(0.0));
    }

    let output = area.show(ui, |ui| {
        ui.horizontal_top(|ui| {
            if let Some(numbers) = &gutter {
                // Same font style/size as the TextEdit so gutter rows line
                // up exactly with editor rows.
                let mono = ui
                    .style()
                    .text_styles
                    .get(&TextStyle::Monospace)
                    .cloned()
                    .unwrap_or_else(|| egui::FontId::monospace(theme::FONT_SIZE));
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.label(
                    RichText::new(numbers.as_str())
                        .font(mono)
                        .color(theme::text_dim()),
                );
            }
            // Search highlighting needs a custom layouter, which copies
            // the text into a LayoutJob — so it's only installed while a
            // find is actually running.
            let counter = Counter::new();
            let find = opts.find;
            let mut layouter = |ui: &egui::Ui, text: &str, wrap_width: f32| {
                let base = TextFormat {
                    font_id: FontId::monospace(theme::FONT_SIZE),
                    color: theme::text_editor(),
                    ..Default::default()
                };
                counter.reset();
                let mut job = super::highlight::job_for(text, base, find, &counter);
                job.wrap.max_width = wrap_width;
                ui.fonts(|f| f.layout_job(job))
            };

            // secure_edit keeps the mlock following reallocation and clears
            // the undo history so secret edits aren't retained in cleartext.
            let layouter_arg: Option<super::secure_edit::Layouter<'_>> =
                find.is_some().then_some(&mut layouter);
            super::secure_edit::multiline(ui, buffer, layouter_arg, |te| {
                let te = te
                    .font(TextStyle::Monospace)
                    .text_color(theme::text_editor())
                    .desired_rows(fill_rows)
                    .frame(false)
                    .lock_focus(true)
                    .margin(egui::Margin::ZERO);
                match unwrapped_width {
                    Some(w) => te.desired_width(w),
                    None => te.desired_width(f32::INFINITY),
                }
            })
            .changed()
        })
        .inner
    });

    let info = ScrollInfo {
        offset_y: output.state.offset.y,
        content_height: output.content_size.y,
        viewport_height: output.inner_rect.height(),
    };
    (output.inner, info)
}

fn render_plaintext(ui: &mut Ui, content: &str, opts: ViewOptions<'_>) {
    let number_width = if opts.line_numbers {
        content.lines().count().max(1).to_string().len()
    } else {
        0
    };

    // One counter for the whole document so the active match can be told
    // apart from the rest.
    let counter = Counter::new();
    let base = TextFormat {
        font_id: FontId::monospace(theme::FONT_SIZE),
        color: theme::text_primary(),
        ..Default::default()
    };

    for (idx, line) in content.lines().enumerate() {
        let display = if line.is_empty() { " " } else { line };
        // Plain RichText while not searching (no extra text copy); a
        // highlighted LayoutJob only while a find is active.
        let widget: egui::Label = if opts.find.is_some() {
            egui::Label::new(super::highlight::job_for(
                display,
                base.clone(),
                opts.find,
                &counter,
            ))
        } else {
            egui::Label::new(
                RichText::new(display)
                    .color(theme::text_primary())
                    .size(theme::FONT_SIZE),
            )
        };
        let widget = if opts.word_wrap {
            widget.wrap()
        } else {
            widget.extend()
        };

        if opts.line_numbers {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.label(
                    RichText::new(format!("{:>number_width$}", idx + 1))
                        .font(FontId::monospace(theme::FONT_SIZE - 2.0))
                        .color(theme::text_dim()),
                );
                ui.add(widget);
            });
        } else {
            ui.add(widget);
        }
    }
}
