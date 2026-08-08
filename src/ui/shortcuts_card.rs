//! Floating keyboard-shortcut reference, toggled via View → Keyboard
//! Shortcuts.
//!
//! This replaced a fixed strip of text in the status bar that read
//! `j/k:scroll  d/u:page  g/G:top/end  Cmd+E:edit  q:quit`. That strip
//! was wrong in two ways at once, and both are the point of this module.
//!
//! It was wrong about *which* keys: the motion keys are remapped to
//! physical position for Dvorak, Colemak and Workman, so on three of the
//! four supported layouts the letters it named were not the letters that
//! worked. Everything here asks [`keybindings::nav_labels`] instead.
//!
//! And it was wrong about *when*: the motion keys do nothing in edit
//! mode, where the same letters are text you are typing, and `Cmd+E`
//! reads as "edit" whatever the user has rebound it to. So this list is
//! built from the live config and filtered by what the app is actually
//! doing right now — a shortcut is listed only where pressing it would
//! do something.

use egui::{Align2, RichText};

use super::{keybindings, theme};
use crate::config::Config;

/// What the app is doing, so the list can leave out what does not apply.
pub struct Context {
    pub has_document: bool,
    pub is_editing: bool,
    pub crawling: bool,
    /// False when no `gpg` binary was found — those menu items are gone.
    pub gpg_available: bool,
}

/// Render the shortcut reference overlay.
/// Movable, like the statistics card: drag it anywhere; the dropped
/// position sticks for the session.
pub fn show(ctx: &egui::Context, config: &Config, state: &Context) {
    egui::Area::new(egui::Id::new("shortcuts_card"))
        // Starts on the left, so it never fights the statistics card
        // for the same corner when both are on.
        .default_pos(egui::pos2(14.0, 44.0))
        .pivot(Align2::LEFT_TOP)
        .movable(true)
        .constrain(true)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(theme::bg_raised().gamma_multiply(0.92))
                .stroke(egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.35)))
                .corner_radius(theme::RADIUS)
                .inner_margin(egui::Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.set_max_width(248.0);
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.label(theme::gradient_text("Keyboard Shortcuts", 13.0));
                    ui.add_space(2.0);
                    body(ui, config, state);
                });
        });
}

fn body(ui: &mut egui::Ui, config: &Config, state: &Context) {
    let kb = &config.keybindings;

    if state.crawling {
        // Crawl swallows the keyboard entirely; nothing else applies.
        heading(ui, "While crawling");
        row(ui, "Space", "Pause / resume");
        row(ui, "Up / Down", "Faster / slower");
        row(ui, "+ / -", "Text size");
        row(ui, "R", "Reverse");
        row(ui, "Home / End", "Jump to start / end");
        row(ui, "Esc  Q", "Stop crawling");
        return;
    }

    // ── Reading motions ──────────────────────────────────────────────
    // Only in view mode: in the editor these letters are text, not
    // commands, which is exactly what made the old status-bar strip
    // misleading.
    if state.has_document && !state.is_editing {
        let layout = keybindings::Layout::parse(&config.app.keyboard_layout);
        let n = keybindings::nav_labels(layout);
        heading(ui, "Reading");
        row(ui, &format!("{} / {}", n.down, n.up), "Scroll down / up");
        row(ui, &format!("{} / {}", n.pgdn, n.pgup), "Page down / up");
        row(
            ui,
            &format!("{} / Shift+{}", n.goto, n.goto),
            "Top / bottom",
        );
        row(ui, "Arrows, PgUp/PgDn", "Also scroll");
        row(ui, "Home / End", "Top / bottom");
        ui.add_space(3.0);
    }

    // ── Whatever the document allows right now ───────────────────────
    heading(ui, "Document");
    if state.has_document {
        bind(
            ui,
            &kb.toggle_edit,
            if state.is_editing {
                "Leave edit mode"
            } else {
                "Edit"
            },
        );
        bind(ui, &kb.save, "Save");
        bind(ui, &kb.save_as, "Encrypt & Save As…");
        bind(ui, &kb.find, "Find & replace");
        if !state.is_editing {
            bind(ui, &kb.crawl, "Crawl (auto-scroll)");
        }
        bind(ui, &kb.close_document, "Close");
        bind(ui, &kb.panic_lock, "Lock now");
    }
    bind(ui, &kb.open_file, "Open…");
    bind(ui, &kb.new_markdown, "New markdown file");
    bind(ui, &kb.new_text, "New text file");
    bind(ui, &kb.quick_note, "Quick note");
    bind(ui, &kb.settings, "Settings");
    if state.has_document && !state.is_editing {
        row(ui, "Q", "Quit");
    }

    ui.add_space(3.0);
    heading(ui, "From anywhere");
    bind(ui, &config.quick_note.hotkey, "Quick note");
    let extra = config
        .quick_note
        .notes
        .iter()
        .filter(|n| !n.hotkey.trim().is_empty())
        .count()
        + config
            .favorites
            .iter()
            .filter(|f| !f.hotkey.trim().is_empty())
            .count();
    if extra > 0 {
        row(ui, "\u{2026}", &format!("{extra} more, set per file"));
    }

    if !state.gpg_available {
        ui.add_space(3.0);
        ui.label(
            RichText::new("GPG not found — running AGE-only.")
                .size(10.5)
                .color(theme::text_dim()),
        );
    }
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .size(10.5)
            .color(theme::text_dim())
            .strong(),
    );
}

/// One row for a configured binding. An unset binding is simply absent —
/// listing a blank key would be the same kind of lie as naming the wrong
/// one.
fn bind(ui: &mut egui::Ui, spec: &str, what: &str) {
    let Some(combo) = crate::keybind::KeyCombo::parse(spec) else {
        return;
    };
    row(ui, &combo.display(), what);
}

fn row(ui: &mut egui::Ui, keys: &str, what: &str) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(keys)
                .size(11.0)
                .monospace()
                .color(theme::text_primary()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(what).size(11.0).color(theme::text_dim()));
        });
    });
}
