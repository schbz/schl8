//! Quick-note ("jot") window: type a small blurb, pick a target file,
//! and append-encrypt it without opening the document.
//!
//! Enter submits, Shift+Enter inserts a newline, Esc closes. The text
//! lives in a `SecureString` and is dropped (zeroized) on submit/cancel.

use std::path::PathBuf;

use egui::RichText;

use super::theme;
use crate::config::QuickNote;
use crate::crypto::secure_buf::SecureString;

/// What a Momentum expiry does to a jot.
///
/// The rule the feature was asked for by name: "if it locks before I
/// type anything then the jot is canceled, otherwise whatever I have
/// typed is saved." Whitespace counts as nothing — an accidental space
/// or a stray newline is not a note, and appending it would timestamp
/// an empty entry into the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JotOutcome {
    /// Nothing written: close the jot, save nothing.
    Cancel,
    /// Something written: append it, exactly as pressing Enter would.
    Submit,
}

/// Decide what a pause should do with the jot's current text.
pub fn expiry_outcome(text: &str) -> JotOutcome {
    if text.trim().is_empty() {
        JotOutcome::Cancel
    } else {
        JotOutcome::Submit
    }
}

/// What the app should do after rendering the jot window this frame.
pub enum JotAction {
    None,
    /// Open a file picker to add a target.
    BrowseTarget,
    /// Append the current text to the selected target.
    Submit,
    /// Open the quicknote-files manager (in the main window).
    Manage,
}

pub struct JotWindow {
    pub open: bool,
    buffer: SecureString,
    pub include_timestamp: bool,
    pub selected_target: Option<PathBuf>,
    /// Error (or info) line shown inside the window.
    pub status: Option<String>,
    /// True while a background append is running.
    pub busy: bool,
    /// Frames left to keep asking for keyboard focus.
    ///
    /// A single request on the first frame is not enough. The window is
    /// created and focused by the OS asynchronously, so on that frame it
    /// is often not the key window yet and the request is dropped —
    /// leaving the caret nowhere and the user's typing going to the app
    /// they came from. Re-asking for a few frames costs nothing and
    /// survives that race.
    focus_frames: u8,
}

impl JotWindow {
    pub fn new(cfg: &QuickNote) -> Self {
        Self {
            open: false,
            buffer: SecureString::empty(),
            include_timestamp: cfg.include_timestamp,
            selected_target: cfg.last_target.clone(),
            status: None,
            busy: false,
            focus_frames: 0,
        }
    }

    /// Open (or re-focus) the jot window.
    pub fn show(&mut self) {
        self.open = true;
        self.status = None;
        // ~8 frames is a few dozen milliseconds at any refresh rate:
        // long enough for the window to become key, short enough that it
        // cannot fight a deliberate click into another field.
        self.focus_frames = 8;
    }

    /// True while the jot is still trying to take keyboard focus.
    pub fn wants_focus(&self) -> bool {
        self.focus_frames > 0
    }

    pub fn text(&self) -> &str {
        self.buffer.as_str()
    }

    /// Drop the note text (zeroized) and reset for next time.
    pub fn clear_text(&mut self) {
        self.buffer = SecureString::empty();
    }

    /// Replace the note text, e.g. when restoring a jot that was held
    /// encrypted across a session lock. The old buffer is zeroized as it
    /// is dropped, and the new one is mlock'd like any other.
    pub fn set_text(&mut self, text: &str) {
        let mut buffer = SecureString::empty();
        buffer.push_str(text);
        self.buffer = buffer;
    }

    /// Whether the current text is submittable.
    fn can_submit(&self) -> bool {
        !self.busy && self.selected_target.is_some() && !self.buffer.as_str().trim().is_empty()
    }

    /// Make the header behave like a title bar.
    ///
    /// The jot viewport is borderless (`with_decorations(false)`), so macOS
    /// gives it no title bar to grab and the window can only be moved if the
    /// app asks for it. Everything below the header is covered by widgets —
    /// the note editor requests infinite width and fills the remaining
    /// height — so without this the only draggable pixels were the few gaps
    /// between rows, which is why moving the window felt unreliable.
    ///
    /// The handle spans the full panel width and reaches out into the frame's
    /// margin, so the whole strip above the target selector is grabbable,
    /// including the padding at the very top edge.
    ///
    /// `exclude` is the close control's rect. The handle is added after
    /// the header, so it sits on top of everything in it — anything
    /// inside the title bar that must stay clickable has to be cut out
    /// here, or dragging silently swallows the click.
    fn drag_handle(&self, ui: &mut egui::Ui, header: egui::Rect, exclude: egui::Rect) {
        // The frame's inner margin (18pt) sits outside `max_rect`; covering
        // it means a press on the visible padding drags too, rather than
        // landing on dead pixels a few points from the title.
        const MARGIN: f32 = 18.0;
        let panel = ui.max_rect();
        // The handle STOPS at the close control rather than covering it
        // and declining to drag there. Merely suppressing the drag is not
        // enough: an overlapping `interact` rect still claims the pointer,
        // so the control underneath never sees a click at all — it just
        // becomes dead. Geometry is what actually leaves it clickable.
        let right = (exclude.left() - 6.0).max(panel.left());
        let handle = egui::Rect::from_min_max(
            egui::pos2(panel.left() - MARGIN, panel.top() - MARGIN),
            egui::pos2(right, header.bottom()),
        );

        let response = ui.interact(
            handle,
            ui.id().with("jot_title_bar"),
            egui::Sense::click_and_drag(),
        );

        // `StartDrag` is only honored if the primary button went down
        // immediately before, so it has to fire on the press itself —
        // reacting to `dragged()` instead misses the window entirely.
        if response.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        }
    }

    /// Render the jot UI into `ui` (used inside its own floating viewport).
    /// Handles Esc (close) and Enter (submit) via `ui.ctx()`. `notes` are
    /// the registry entries as (display name, source path). Returns the
    /// action the app should take.
    /// `momentum` is Some((seconds_left, urgency)) while a Momentum
    /// quicknote is counting down; the jot draws the same draining bar
    /// the editor shows, so the deal is visible while you type.
    pub fn render_contents(
        &mut self,
        ui: &mut egui::Ui,
        notes: &[(String, PathBuf)],
        momentum: Option<(f32, f32)>,
    ) -> JotAction {
        let mut action = JotAction::None;
        let ctx = ui.ctx().clone();

        // Esc closes (unless an append is running)
        if !self.busy && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.open = false;
            self.clear_text();
            return action;
        }
        // Enter submits; Shift+Enter falls through to the editor as a newline.
        let submit_key =
            !self.busy && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));

        ui.spacing_mut().item_spacing.y = 9.0;

        // ── Header, which is also the title bar ──────────────────────
        let header = ui.horizontal(|ui| {
            ui.label(
                RichText::new("\u{1F4DD}") // memo
                    .size(18.0)
                    .color(theme::accent()),
            );
            ui.label(theme::gradient_text("Quick Note", 19.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // A real button, so it looks like the thing it is. The
                // keyboard Esc still works and is still what the label
                // says — this only adds the click people expect from
                // something sitting where a close button sits.
                let close = ui
                    .add(
                        egui::Button::new(
                            RichText::new("esc")
                                .size(11.0)
                                .color(theme::text_dim())
                                .monospace(),
                        )
                        .fill(theme::bg_raised().gamma_multiply(0.6))
                        .corner_radius(theme::RADIUS),
                    )
                    .on_hover_text("Close (or press Esc)");
                if close.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                (close.clicked(), close.rect)
            })
            .inner
        });
        let (close_clicked, close_rect) = header.inner;
        // ── Momentum drain bar ───────────────────────────────────────
        // Directly under the title, full width: the jot is small, and a
        // mode that will close it on a pause should be impossible to
        // miss while it runs.
        if let Some((left, urgency)) = momentum {
            let calm = theme::accent();
            let alarm = theme::accent_red();
            let mix = |a: egui::Color32, b: egui::Color32, t: f32| {
                let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
                egui::Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
            };
            let color = mix(calm, alarm, urgency * urgency);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Momentum")
                        .size(10.5)
                        .color(theme::text_dim()),
                );
                ui.label(
                    RichText::new(format!("{left:.1}s"))
                        .size(11.5)
                        .monospace()
                        .strong()
                        .color(color),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(if self.buffer.as_str().trim().is_empty() {
                            "pause cancels"
                        } else {
                            "pause saves"
                        })
                        .size(10.5)
                        .color(theme::text_dim()),
                    );
                });
            });
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 3.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect, 1.5, theme::bg_raised());
            let mut filled = rect;
            filled.set_width(rect.width() * (1.0 - urgency).clamp(0.0, 1.0));
            p.rect_filled(filled, 1.5, color);
        }
        if close_clicked && !self.busy {
            self.open = false;
            self.clear_text();
            return action;
        }
        self.drag_handle(ui, header.response.rect, close_rect);
        ui.add_space(2.0);

        // ── Target selector ──────────────────────────────────────────
        ui.horizontal(|ui| {
            let selected_name = self
                .selected_target
                .as_ref()
                .map(|sel| {
                    notes
                        .iter()
                        .find(|(_, path)| path == sel)
                        .map(|(name, _)| name.clone())
                        .unwrap_or_else(|| {
                            sel.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("?")
                                .to_string()
                        })
                })
                .unwrap_or_else(|| "Choose a file…".to_string());
            // Buttons are reserved from the right, then the label and
            // selector fill the space that remains from the left.
            //
            // Two things this arrangement gets right. The selector used to
            // be a fixed 240pt, which needed more room than the window's
            // own 420pt minimum leaves, so the rightmost button was cut in
            // half; reserving the buttons first cannot overflow at any
            // width or interface scale. And the label is laid out with the
            // selector rather than at the row's far left, so widening the
            // window opens a gap between the selector and the buttons —
            // where empty space belongs — instead of stranding the label
            // on its own.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Manage…"))
                    .on_hover_text("Add, remove, or configure quicknote files")
                    .clicked()
                {
                    action = JotAction::Manage;
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Browse…"))
                    .clicked()
                {
                    action = JotAction::BrowseTarget;
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new("Append to")
                            .size(12.0)
                            .color(theme::text_dim()),
                    );
                    // Capped so a wide window keeps the original
                    // proportions, floored so the file name stays readable
                    // on a narrow one.
                    let width = ui.available_width().clamp(80.0, 240.0);
                    egui::ComboBox::from_id_salt("jot_target")
                        .selected_text(RichText::new(selected_name).size(13.0))
                        .width(width)
                        .show_ui(ui, |ui| {
                            for (name, target) in notes {
                                let is_sel = self.selected_target.as_ref() == Some(target);
                                if ui
                                    .selectable_label(is_sel, RichText::new(name).size(13.0))
                                    .on_hover_text(target.display().to_string())
                                    .clicked()
                                {
                                    self.selected_target = Some(target.clone());
                                }
                            }
                        });
                });
            });
        });

        // ── Note text ────────────────────────────────────────────────
        // Fills the space between the header rows above and the fixed
        // rows below (options + actions ≈ 74 px), scrolling when the
        // text outgrows it — long pastes stay reachable.
        let want_focus = self.focus_frames > 0;
        let text_height = (ui.available_height() - 74.0).max(90.0);
        ui.add_enabled_ui(!self.busy, |ui| {
            ui.visuals_mut().extreme_bg_color = theme::bg_editor();
            egui::ScrollArea::vertical()
                .max_height(text_height)
                .min_scrolled_height(text_height)
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    // Make the editable area cover the visible height even
                    // when nearly empty, so clicking anywhere focuses it.
                    let row_h = ui.text_style_height(&egui::TextStyle::Monospace);
                    let fill_rows = ((text_height / row_h).floor() as usize).max(4);
                    let response =
                        super::secure_edit::multiline(ui, &mut self.buffer, None, |te| {
                            te.font(egui::TextStyle::Monospace)
                                .text_color(theme::text_editor())
                                .desired_width(f32::INFINITY)
                                .desired_rows(fill_rows)
                                .hint_text("Type a note… (Enter appends · Shift+Enter newline)")
                                .lock_focus(true)
                        });
                    if want_focus {
                        response.request_focus();
                    }
                });
        });
        self.focus_frames = self.focus_frames.saturating_sub(1);

        // ── Options + status ─────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut self.include_timestamp,
                RichText::new("Include date & time")
                    .size(12.0)
                    .color(theme::text_primary()),
            );
            if let Some(msg) = &self.status {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(msg.as_str())
                            .size(12.0)
                            .color(theme::accent_red()),
                    );
                });
            }
        });

        ui.add_space(2.0);

        // ── Actions ──────────────────────────────────────────────────
        ui.horizontal(|ui| {
            let can = self.can_submit();
            let submit_btn = egui::Button::new(
                RichText::new("  Append & Encrypt  ")
                    .size(13.0)
                    .color(theme::badge_text())
                    .strong(),
            )
            .fill(theme::badge_bg())
            .corner_radius(theme::RADIUS);
            if ui.add_enabled(can, submit_btn).clicked() {
                action = JotAction::Submit;
            }

            if self.busy {
                ui.spinner();
                ui.label(
                    RichText::new("Encrypting… (enter your PIN if prompted)")
                        .size(12.0)
                        .color(theme::text_dim()),
                );
            } else if self.selected_target.is_none() {
                ui.label(
                    RichText::new("Pick a target with Browse…")
                        .size(12.0)
                        .color(theme::text_dim()),
                );
            }
        });

        if submit_key && self.can_submit() {
            action = JotAction::Submit;
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule as requested: nothing typed cancels, anything typed
    /// saves — and whitespace is nothing, because appending a stray
    /// space would timestamp an empty entry into the note.
    #[test]
    fn a_pause_saves_writing_and_cancels_nothing() {
        assert_eq!(expiry_outcome(""), JotOutcome::Cancel);
        assert_eq!(expiry_outcome("   \n\t "), JotOutcome::Cancel);
        assert_eq!(expiry_outcome("bought stamps"), JotOutcome::Submit);
        assert_eq!(expiry_outcome("  x  "), JotOutcome::Submit);
    }
}
