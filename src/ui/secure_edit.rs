//! A `TextEdit` wrapper for editing secret text.
//!
//! egui's `TextEdit` keeps an internal undo history (`Undoer`) that clones
//! the edited `String` on changes into egui's memory arena. Those clones
//! are ordinary, un-mlock'd, un-zeroized heap allocations — so every
//! intermediate version of a secret would otherwise linger in cleartext.
//!
//! We clear the widget's undoer every frame. This disables in-editor undo
//! (an acceptable trade for a security tool) and, more importantly,
//! prevents the undo history from *accumulating* every keystroke's text.
//!
//! Residual, documented limitation: egui still creates one transient
//! clone during the frame it records a change; that copy is dropped
//! immediately (not zeroized). Fully eliminating it would require a custom
//! text widget whose backing store is the `SecureString` itself.

use std::sync::Arc;

use egui::{Galley, Response, TextEdit, Ui};

use crate::crypto::secure_buf::SecureString;

/// A custom text layouter (used for search highlighting). Taken as a
/// parameter rather than applied inside `build`, because a `TextEdit`'s
/// layouter borrow is tied to the widget's own lifetime — which is the
/// internal borrow of the buffer, not anything the caller can name.
pub type Layouter<'a> = &'a mut dyn FnMut(&Ui, &str, f32) -> Arc<Galley>;

/// Add a multiline `TextEdit` bound to a `SecureString`, then (a) keep the
/// mlock following any reallocation and (b) clear the widget's undo
/// history so secret edits aren't retained in cleartext.
pub fn multiline(
    ui: &mut Ui,
    buffer: &mut SecureString,
    layouter: Option<Layouter<'_>>,
    build: impl FnOnce(TextEdit) -> TextEdit,
) -> Response {
    let mut te = TextEdit::multiline(buffer.as_mut_string());
    if let Some(layouter) = layouter {
        te = te.layouter(layouter);
    }
    let text_edit = build(te);
    let response = ui.add(text_edit);

    // The edit may have grown/moved the String — relock the live region.
    buffer.relock_if_moved();

    // Drop any undo snapshot egui just recorded.
    if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), response.id) {
        state.clear_undoer();
        state.store(ui.ctx(), response.id);
    }

    response
}
