use egui::{Align, Layout, RichText, Ui};

use super::theme;
use crate::crypto::gpg::SignatureStatus;

/// Actions the status bar can trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusAction {
    DiscardEdits,
    /// Lock the session immediately (the panic button).
    PanicLock,
    /// Save with the file's own key(s) to its own location (or its save
    /// plan), then leave edit mode.
    SaveAndExit,
    /// Enter edit mode (the view-mode "Edit" button).
    EnterEdit,
    /// Open the Save Options window (the per-file save plan: keys,
    /// destinations, post-save hook). Offered in both view and edit mode.
    OpenSaveOptions,
}

/// On-disk identity of the opened encrypted file, shown at the left of
/// the bar: (short content hash, last-modified timestamp, size).
///
/// Every field describes the *ciphertext* on disk — nothing here is
/// derived from decrypted content.
#[derive(Clone)]
pub struct FileStamp {
    pub hash8: String,
    pub modified: String,
    /// Size of the encrypted file in bytes.
    pub bytes: u64,
}

impl FileStamp {
    /// The size as a short human-readable string ("4 KB", "1.2 MB").
    pub fn size_label(&self) -> String {
        format_size(self.bytes)
    }
}

/// Format a byte count compactly for list rows.
///
/// KB is the useful unit for notes, so anything under a megabyte reads as
/// whole kilobytes — but a file that exists is never shown as "0 KB",
/// since that reads as empty when it is merely small.
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes == 0 {
        "0 KB".to_string()
    } else {
        format!("{} KB", bytes.div_ceil(KB))
    }
}

/// Below this width the bar lays out as two rows instead of one.
///
/// Edit mode needs far more: four buttons plus a hint, against a view
/// mode of two. Using one threshold for both meant edit mode crammed its
/// buttons into whatever the file info left over, which is how they
/// ended up overlapping.
pub const COMPACT_WIDTH: f32 = 720.0;
pub const COMPACT_WIDTH_EDIT: f32 = 980.0;

/// Whether the bar will use the two-row (compact) layout at `width`.
pub fn is_compact(width: f32, is_editing: bool) -> bool {
    width
        < if is_editing {
            COMPACT_WIDTH_EDIT
        } else {
            COMPACT_WIDTH
        }
}

/// Minimum height for the bottom panel — the single-row size. The panel
/// is NOT given a fixed height: in compact mode it grows to fit the extra
/// button row, and the wrapped info row can itself take more than one
/// line at very narrow widths. Pinning an exact height would push those
/// extra lines below the window edge.
pub fn min_bar_height(is_editing: bool) -> f32 {
    if is_editing {
        theme::STATUSBAR_HEIGHT_EDIT
    } else {
        theme::STATUSBAR_HEIGHT
    }
}

/// Render the status bar at the bottom of the window.
/// Returns an action if the user clicked a status bar button.
#[allow(clippy::too_many_arguments)] // display-only fn; params are one flat status line
pub fn render(
    ui: &mut Ui,
    filename: &str,
    // Absolute path shown when hovering the filename.
    full_path: &str,
    current_line: usize,
    total_lines: usize,
    is_editing: bool,
    is_modified: bool,
    // When browsing a folder archive: (1-based selected index, total files)
    archive_info: Option<(usize, usize)>,
    signature: &SignatureStatus,
    // True when unsaved edits are deferring auto-lock/sleep-lock — shows
    // a warning chip explaining that the text is kept but unprotected.
    lock_deferred: bool,
    // Hash + mtime of the encrypted file on disk (None for unsaved files).
    stamp: Option<&FileStamp>,
    // Two-row layout, decided by the caller so the panel height matches.
    compact: bool,
) -> Option<StatusAction> {
    let mut action = None;

    // Draw order adapts to width: wide windows put the action buttons on
    // the same row as the file info; narrow ones give the buttons their
    // own row above, and let the info wrap, so nothing is ever clipped.
    let draw_info = |ui: &mut Ui| {
        // On-disk identity: short content hash + last-modified time of
        // the encrypted file.
        //
        // An unsaved file has no on-disk identity, and nothing goes in
        // its place: the app's name is already in the title bar, and this
        // bar is for facts about the file in front of you.
        if let Some(s) = stamp {
            ui.label(
                RichText::new(format!(" #{} ", s.hash8))
                    .color(theme::badge_text())
                    .background_color(theme::badge_bg())
                    .size(theme::FONT_SIZE_STATUS)
                    .monospace()
                    .strong(),
            )
            .on_hover_text(
                "SHA-256 (first 8 hex digits) of the encrypted file on disk \
                 — compare it across machines to confirm you're looking at \
                 the same version",
            );
            ui.label(
                RichText::new(&s.modified)
                    .color(theme::text_dim())
                    .size(theme::FONT_SIZE_STATUS)
                    .monospace(),
            )
            .on_hover_text("When the encrypted file was last modified");
            ui.separator();
        }

        // Folder-archive badge
        if let Some((idx, total)) = archive_info {
            ui.label(
                RichText::new(format!(" FOLDER {idx}/{total} "))
                    .color(theme::contrast_text(theme::accent_purple()))
                    .background_color(theme::accent_purple())
                    .size(theme::FONT_SIZE_STATUS)
                    .strong(),
            );
            ui.separator();
        }

        // Mode badge — reads as a state you are in, not a label for a
        // thing that exists.
        if is_editing {
            ui.label(
                RichText::new(" EDITING ")
                    .color(theme::contrast_text(theme::accent_yellow()))
                    .background_color(theme::accent_yellow())
                    .size(theme::FONT_SIZE_STATUS)
                    .strong(),
            );
            ui.separator();
        }

        // Modified indicator
        if is_modified {
            ui.label(
                RichText::new("*")
                    .color(theme::accent_yellow())
                    .size(theme::FONT_SIZE_STATUS)
                    .strong(),
            );
        }

        // Unsaved-edits / auto-lock warning
        if lock_deferred {
            ui.label(
                RichText::new(" LOCK PAUSED ")
                    .color(theme::contrast_text(theme::accent_yellow()))
                    .background_color(theme::accent_yellow())
                    .size(theme::FONT_SIZE_STATUS)
                    .strong(),
            )
            .on_hover_text(
                "You have unsaved text, so auto-lock and sleep-lock are paused — \
                 Schl8 never silently discards your edits. The decrypted text \
                 stays in (locked) memory until you save or discard, so save when \
                 you step away.",
            );
            ui.separator();
        }

        // Filename (hover shows the absolute location)
        ui.label(
            RichText::new(filename)
                .color(theme::text_primary())
                .size(theme::FONT_SIZE_STATUS),
        )
        .on_hover_text(full_path);

        // No file-type tag: the extension in the filename beside it
        // already says whether this is markdown or plain text.

        // Signature badge (only when the file carried a signature)
        match signature {
            SignatureStatus::Unsigned => {}
            SignatureStatus::Valid { signer } => {
                ui.separator();
                ui.label(
                    RichText::new(" \u{2714} SIGNED ")
                        .color(theme::contrast_text(theme::accent_green()))
                        .background_color(theme::accent_green())
                        .size(theme::FONT_SIZE_STATUS)
                        .strong(),
                )
                .on_hover_text(format!("Verified signature from {signer}"));
            }
            SignatureStatus::Invalid { reason } => {
                ui.separator();
                ui.label(
                    RichText::new(" \u{26A0} BAD SIG ")
                        .color(theme::contrast_text(theme::accent_red()))
                        .background_color(theme::accent_red())
                        .size(theme::FONT_SIZE_STATUS)
                        .strong(),
                )
                .on_hover_text(format!("Signature problem: {reason}"));
            }
        }

        ui.separator();

        // Line position
        ui.label(
            RichText::new(format!("Line {} of {}", current_line, total_lines))
                .color(theme::text_dim())
                .size(theme::FONT_SIZE_STATUS),
        );
    };

    // Both layouts wrap rather than clip. The bar is a bottom panel with
    // a *minimum* height, so a wrapped row grows it instead of pushing
    // content out of the window — which is the only way to guarantee
    // nothing is hidden at an arbitrary window width.
    if compact {
        // Buttons first and wrapped: at narrow widths they take as many
        // rows as they need rather than running off the edge.
        ui.horizontal_wrapped(|ui| {
            if let Some(a) = render_actions(ui, is_editing, false) {
                action = Some(a);
            }
        });
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| draw_info(ui));
    } else {
        ui.horizontal(|ui| {
            // The actions claim their width from the right FIRST, then the
            // info wraps into whatever is left. Drawing the info first —
            // as this did — let a long filename consume the whole row and
            // leave the buttons drawing on top of it.
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(a) = render_actions(ui, is_editing, true) {
                    action = Some(a);
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.horizontal_wrapped(|ui| draw_info(ui));
                });
            });
        });
    }

    action
}

/// The right-hand action buttons. `show_hints` adds the keybinding hint
/// text, which is dropped when the bar is laid out in compact (two-row)
/// mode. Both modes offer the same "Save Options…" entry point so the
/// save plan (keys, destinations, post-save hook) is reachable either way.
fn render_actions(ui: &mut Ui, is_editing: bool, show_hints: bool) -> Option<StatusAction> {
    let mut action = None;
    if is_editing {
        // "Encrypt & Save" button
        let save_btn = egui::Button::new(
            RichText::new("Encrypt & Save")
                .size(theme::FONT_SIZE_STATUS)
                .color(theme::badge_text()),
        )
        .fill(theme::badge_bg())
        .corner_radius(3.0);
        if ui
            .add(save_btn)
            .on_hover_text("Re-encrypts with the same key(s) to the same location")
            .clicked()
        {
            action = Some(StatusAction::SaveAndExit);
        }

        ui.add_space(4.0);

        // Explicit different-key / different-location save.
        // Explicit theme fill + stroke + strong text so the label is
        // readable on every palette preset (the default egui button
        // colors aren't guaranteed to contrast here).
        let diff_btn = egui::Button::new(
            RichText::new("Save Options…")
                .size(theme::FONT_SIZE_STATUS)
                .color(theme::text_strong()),
        )
        .fill(theme::bg_raised())
        .stroke(egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.6)))
        .corner_radius(3.0);
        if ui
            .add(diff_btn)
            .on_hover_text(
                "Choose which key(s), destination(s), and post-save hook \
                     this file's Save uses",
            )
            .clicked()
        {
            action = Some(StatusAction::OpenSaveOptions);
        }

        ui.add_space(4.0);

        // "Discard Edits" button — destructive red with
        // contrast-checked text, readable on every theme.
        let discard_bg = theme::accent_red().gamma_multiply(0.9);
        let discard_btn = egui::Button::new(
            RichText::new("Discard Edits")
                .size(theme::FONT_SIZE_STATUS)
                .color(theme::contrast_text(discard_bg)),
        )
        .fill(discard_bg)
        .corner_radius(3.0);
        if ui.add(discard_btn).clicked() {
            action = Some(StatusAction::DiscardEdits);
        }

        ui.add_space(4.0);

        // Panic: lock the session right now. Sits in edit mode because
        // that is when there is decrypted text on screen to hide. Unsaved
        // work is encrypted to this document's own key on the way out, so
        // pressing it never costs you the edit.
        let panic_bg = theme::accent_purple();
        let panic_btn = egui::Button::new(
            RichText::new("\u{1F512} Lock Now")
                .size(theme::FONT_SIZE_STATUS)
                .color(theme::contrast_text(panic_bg))
                .strong(),
        )
        .fill(panic_bg)
        .corner_radius(3.0);
        if ui
            .add(panic_btn)
            .on_hover_text(
                "Lock the session immediately. Decrypted text is cleared from \
                 memory and any unsaved edits are encrypted to this document's \
                 own key first, so nothing is lost \u{2014} you unlock to get \
                 them back.",
            )
            .clicked()
        {
            action = Some(StatusAction::PanicLock);
        }

        ui.add_space(8.0);

        // The hint is the first thing to go: it is a convenience, and
        // the buttons beside it are not.
        if show_hints && ui.available_width() > 220.0 {
            ui.label(
                RichText::new("Cmd+E: exit edit")
                    .color(theme::text_dim())
                    .size(theme::FONT_SIZE_STATUS),
            );
        }
    } else {
        // View mode: an explicit Edit button (same action as Cmd+E)
        let edit_btn = egui::Button::new(
            RichText::new("Edit")
                .size(theme::FONT_SIZE_STATUS)
                .color(theme::badge_text()),
        )
        .fill(theme::badge_bg())
        .corner_radius(3.0);
        if ui
            .add(edit_btn)
            .on_hover_text("Edit this file (Cmd+E)")
            .clicked()
        {
            action = Some(StatusAction::EnterEdit);
        }

        ui.add_space(4.0);

        // Save Targets: save an identical copy under different
        // keys and/or to different locations
        let reenc_btn = egui::Button::new(
            RichText::new("Save Options…")
                .size(theme::FONT_SIZE_STATUS)
                .color(theme::text_strong()),
        )
        .fill(theme::bg_raised())
        .stroke(egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.6)))
        .corner_radius(3.0);
        if ui
            .add(reenc_btn)
            .on_hover_text(
                "Choose which key(s), destination(s), and post-save hook \
                     this file's Save uses",
            )
            .clicked()
        {
            action = Some(StatusAction::OpenSaveOptions);
        }

        ui.add_space(8.0);

        if show_hints && ui.available_width() > 340.0 {
            // Keybinding hints only when there's room (long paths in
            // archive mode would otherwise collide with the left side)
            ui.label(
                RichText::new("j/k:scroll  d/u:page  g/G:top/end  Cmd+E:edit  q:quit")
                    .color(theme::text_dim())
                    .size(theme::FONT_SIZE_STATUS),
            );
        }
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_as_whole_kilobytes_up_to_a_megabyte() {
        assert_eq!(format_size(0), "0 KB");
        // A file that exists must never read as "0 KB" — that says empty
        // when it only means small, and these rows are how a user decides
        // whether a note still has their content in it.
        assert_eq!(format_size(1), "1 KB");
        assert_eq!(format_size(1024), "1 KB");
        assert_eq!(format_size(1025), "2 KB", "rounds up, never down to 0");
        assert_eq!(format_size(4096), "4 KB");
        assert_eq!(format_size(1024 * 1023), "1023 KB");
    }

    #[test]
    fn larger_sizes_switch_units() {
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(3 * 1024 * 1024 + 512 * 1024), "3.5 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_size(u64::MAX), "17179869184.0 GB", "no overflow");
    }

    #[test]
    fn edit_mode_goes_compact_far_earlier_than_view_mode() {
        // Edit mode carries four buttons plus a hint; view mode carries
        // two. One shared threshold is what let edit mode's buttons
        // overlap the file info at ordinary window widths.
        assert!(is_compact(800.0, true), "edit mode needs two rows at 800");
        assert!(!is_compact(800.0, false), "view mode is fine at 800");

        // Both agree at the extremes.
        assert!(is_compact(500.0, true));
        assert!(is_compact(500.0, false));
        assert!(!is_compact(1400.0, true));
        assert!(!is_compact(1400.0, false));

        // The edit threshold must be the wider of the two, or the split
        // above is backwards. Compared through a variable so this stays a
        // real assertion rather than a const clippy can fold away.
        let (edit, view) = (COMPACT_WIDTH_EDIT, COMPACT_WIDTH);
        assert!(edit > view, "edit mode must go compact sooner");
    }

    #[test]
    fn stamp_exposes_its_own_size_label() {
        let stamp = FileStamp {
            hash8: "deadbeef".into(),
            modified: "2026-07-27 09:00".into(),
            bytes: 2048,
        };
        assert_eq!(stamp.size_label(), "2 KB");
    }
}
