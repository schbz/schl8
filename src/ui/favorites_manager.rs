//! Favorites manager: the files pinned to the menu-bar Favorites submenu.
//!
//! A favorite is a file you *open* often — distinct from a quicknote,
//! which is a file you *append* to. Each carries an optional system-wide
//! hotkey, and the list order is the submenu order, changed by dragging.

use egui::{Align2, RichText, Vec2};

use super::theme;
use crate::config::{Favorite, MAX_FAVORITES};

/// What the app should do after rendering this frame.
pub enum FavoritesAction {
    None,
    /// Open a file picker; the chosen path is added via [`add_file`].
    ///
    /// [`add_file`]: FavoritesManager::add_file
    AddFile,
    /// Persist the edited list (already in submenu order).
    Apply(Vec<Favorite>),
}

pub struct FavoritesManager {
    pub open: bool,
    draft: Vec<Favorite>,
    error: Option<String>,
    /// Index of the favorite whose hotkey button is capturing a keypress.
    capturing: Option<usize>,
    /// The main global quick-note hotkey, for conflict checks.
    main_hotkey: String,
    /// Quicknote hotkeys already registered, so a favorite can't silently
    /// steal a combo that belongs to a note.
    note_hotkeys: Vec<(String, String)>,
}

impl FavoritesManager {
    pub fn new() -> Self {
        Self {
            open: false,
            draft: Vec::new(),
            error: None,
            capturing: None,
            main_hotkey: String::new(),
            note_hotkeys: Vec::new(),
        }
    }

    /// Open the manager seeded with the current list. `note_hotkeys` is
    /// `(name, spec)` for every quicknote binding, used for conflict
    /// checks — two global hotkeys with the same combo means one of them
    /// silently never fires.
    pub fn open_with(
        &mut self,
        favorites: &[Favorite],
        main_hotkey: &str,
        note_hotkeys: Vec<(String, String)>,
    ) {
        self.draft = favorites.to_vec();
        self.error = None;
        self.capturing = None;
        self.main_hotkey = main_hotkey.to_string();
        self.note_hotkeys = note_hotkeys;
        self.open = true;
    }

    /// Called by the app after the add-file picker returns.
    pub fn add_file(&mut self, path: std::path::PathBuf) {
        if self.draft.len() >= MAX_FAVORITES {
            self.error = Some(format!("Favorites are full ({MAX_FAVORITES})"));
            return;
        }
        if self.draft.iter().any(|f| f.path == path) {
            self.error = Some("That file is already a favorite".to_string());
            return;
        }
        self.draft.push(Favorite::for_path(path));
        self.error = None;
    }

    /// While a hotkey button is capturing: consume the next keypress as
    /// the combo (Esc cancels, Backspace/Delete clears the binding).
    fn handle_capture(&mut self, ctx: &egui::Context) {
        let Some(idx) = self.capturing else { return };
        let result = ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                return Some(None); // cancel, keep current binding
            }
            if i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete) {
                return Some(Some(String::new())); // clear
            }
            for ev in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = ev
                {
                    if matches!(
                        key,
                        egui::Key::Escape | egui::Key::Backspace | egui::Key::Delete
                    ) {
                        continue;
                    }
                    let combo = crate::keybind::KeyCombo::from_event(*key, modifiers);
                    return Some(Some(combo.to_config_string()));
                }
            }
            None
        });
        let Some(outcome) = result else { return };
        self.capturing = None;
        let Some(spec) = outcome else { return };
        if spec.is_empty() {
            if let Some(fav) = self.draft.get_mut(idx) {
                fav.hotkey.clear();
            }
            self.error = None;
            return;
        }
        // A system-wide hotkey needs a modifier and must be registrable.
        let has_modifier = matches!(
            crate::keybind::KeyCombo::parse(&spec),
            Some(c) if c.has_modifier()
        );
        if !has_modifier {
            self.error = Some("A global hotkey needs a modifier (e.g. ctrl+cmd+2)".to_string());
            return;
        }
        if let Err(e) = crate::hotkey::parse(&spec) {
            self.error = Some(format!("Invalid hotkey: {e}"));
            return;
        }
        if let Some(fav) = self.draft.get_mut(idx) {
            fav.hotkey = spec;
        }
        self.error = None;
    }

    /// Validate the draft before Apply. Returns the first problem.
    ///
    /// Hotkey clashes are errors rather than warnings: two global bindings
    /// on one combo don't split the difference, one of them just never
    /// fires, and the user would have no way to tell which.
    fn validate(&self) -> Option<String> {
        for fav in &self.draft {
            let spec = fav.hotkey.trim();
            if spec.is_empty() {
                continue;
            }
            if let Err(e) = crate::hotkey::parse(spec) {
                return Some(format!("\"{}\": invalid hotkey — {e}", fav.label()));
            }
            if spec.eq_ignore_ascii_case(self.main_hotkey.trim()) {
                return Some(format!(
                    "\"{}\": {spec} is already the main Quick Note hotkey",
                    fav.label()
                ));
            }
            if let Some((note, _)) = self
                .note_hotkeys
                .iter()
                .find(|(_, s)| s.trim().eq_ignore_ascii_case(spec))
            {
                return Some(format!(
                    "\"{}\": {spec} is already the hotkey for the quicknote \"{note}\"",
                    fav.label()
                ));
            }
            let clashes = self
                .draft
                .iter()
                .filter(|f| f.hotkey.trim().eq_ignore_ascii_case(spec))
                .count();
            if clashes > 1 {
                return Some(format!("Hotkey {spec} is used by more than one favorite"));
            }
        }
        None
    }

    /// Render. Returns the action for the app to take.
    pub fn render(&mut self, ctx: &egui::Context) -> FavoritesAction {
        if !self.open {
            return FavoritesAction::None;
        }

        self.handle_capture(ctx);
        let capturing = self.capturing;
        let mut capture_toggle: Option<usize> = None;

        let mut action = FavoritesAction::None;
        let mut is_open = self.open;
        let max_height = (ctx.screen_rect().height() - 90.0).max(260.0);

        // Reorder decided this frame: (dragged-from, dropped-onto).
        let mut reorder: Option<(usize, usize)> = None;
        let mut remove: Option<usize> = None;

        egui::Window::new("Favorites")
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .default_width(theme::dialog_width(ctx, 540.0))
            .max_width(theme::dialog_max_width(ctx))
            .max_height(max_height)
            .vscroll(true)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme::bg_primary())
                    .stroke(egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.4))),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;

                ui.label(theme::gradient_text("Favorites", 16.0));
                ui.label(
                    RichText::new(format!(
                        "Files pinned to the menu-bar Favorites submenu \
                         ({}/{MAX_FAVORITES}). Clicking one — or pressing its \
                         hotkey — opens it. Drag the {} handle to reorder; the \
                         order here is the order in the menu.",
                        self.draft.len(),
                        "\u{2630}"
                    ))
                    .size(11.5)
                    .color(theme::text_dim()),
                );
                ui.add_space(2.0);

                if self.draft.is_empty() {
                    ui.label(
                        RichText::new("No favorites yet — add a file below.")
                            .size(12.0)
                            .color(theme::text_dim()),
                    );
                }

                for (i, fav) in self.draft.iter_mut().enumerate() {
                    let row_id = egui::Id::new(("fav_row", i));
                    let missing = !fav.path.exists();

                    let frame = egui::Frame::NONE
                        .fill(theme::bg_raised().gamma_multiply(0.6))
                        .corner_radius(theme::RADIUS)
                        .inner_margin(egui::Margin::symmetric(10, 8));

                    let resp = ui
                        .dnd_drag_source(row_id, i, |ui| {
                            frame.show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 5.0;
                                ui.horizontal(|ui| {
                                    // Drag handle. U+2630 is covered by the
                                    // bundled fonts in both families (the
                                    // usual ⠿ and ⋮ are not, and would ship
                                    // as tofu boxes).
                                    ui.label(
                                        RichText::new("\u{2630}")
                                            .size(14.0)
                                            .color(theme::text_dim()),
                                    )
                                    .on_hover_text("Drag to reorder");
                                    ui.label(
                                        RichText::new("\u{2605}").size(13.0).color(theme::accent()),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut fav.name)
                                            .desired_width(200.0)
                                            .hint_text("Display name")
                                            .font(egui::TextStyle::Body),
                                    );
                                    if missing {
                                        ui.label(
                                            RichText::new("\u{26A0} file missing")
                                                .size(11.5)
                                                .color(theme::accent_red())
                                                .strong(),
                                        )
                                        .on_hover_text(
                                            "The file was deleted or moved. The menu entry \
                                             stays until you remove it or the file returns.",
                                        );
                                    }
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .button(RichText::new("Remove").size(11.5))
                                                .on_hover_text(
                                                    "Remove from Favorites (the file itself \
                                                     is not deleted)",
                                                )
                                                .clicked()
                                            {
                                                remove = Some(i);
                                            }
                                        },
                                    );
                                });
                                ui.label(
                                    RichText::new(fav.path.display().to_string())
                                        .size(11.0)
                                        .monospace()
                                        .color(theme::text_dim()),
                                );
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new("Hotkey").size(12.0).color(theme::text_dim()),
                                    );
                                    let capturing_this = capturing == Some(i);
                                    let text = if capturing_this {
                                        "  press keys\u{2026}  ".to_string()
                                    } else if fav.hotkey.is_empty() {
                                        "  \u{2014}  ".to_string()
                                    } else {
                                        format!("  {}  ", display_combo(&fav.hotkey))
                                    };
                                    let btn = egui::Button::new(
                                        RichText::new(text).size(12.5).monospace().color(
                                            if capturing_this {
                                                theme::badge_text()
                                            } else {
                                                theme::accent()
                                            },
                                        ),
                                    )
                                    .fill(if capturing_this {
                                        theme::accent()
                                    } else {
                                        theme::bg_raised()
                                    })
                                    .corner_radius(6.0)
                                    .min_size(egui::vec2(110.0, 24.0));
                                    if ui
                                        .add(btn)
                                        .on_hover_text(
                                            "Optional system-wide hotkey that opens this file \
                                             from anywhere while Schl8 runs. Click, then \
                                             press the combo (needs a modifier, e.g. \
                                             ctrl+cmd+2). Esc cancels \u{B7} Backspace clears.",
                                        )
                                        .clicked()
                                    {
                                        capture_toggle = Some(i);
                                    }
                                });
                            });
                        })
                        .response;

                    // Dropping onto a row moves the dragged one here, which
                    // is enough to reach any order and needs no separate
                    // drop targets between rows.
                    if let Some(payload) = resp.dnd_release_payload::<usize>() {
                        reorder = Some((*payload, i));
                    }
                }

                ui.horizontal(|ui| {
                    let room = self.draft.len() < MAX_FAVORITES;
                    if ui
                        .add_enabled(
                            room,
                            egui::Button::new(RichText::new("+ Add file\u{2026}").size(12.0)),
                        )
                        .clicked()
                    {
                        action = FavoritesAction::AddFile;
                    }
                    let missing_count = self.draft.iter().filter(|f| !f.path.exists()).count();
                    if missing_count > 0
                        && ui
                            .button(
                                RichText::new(format!("Remove {missing_count} missing")).size(12.0),
                            )
                            .on_hover_text("Drops the entries only — nothing on disk is touched.")
                            .clicked()
                    {
                        self.draft.retain(|f| f.path.exists());
                    }
                });

                if let Some(err) = &self.error {
                    ui.label(RichText::new(err).size(12.0).color(theme::accent_red()));
                }

                ui.separator();
                ui.horizontal(|ui| {
                    let apply = egui::Button::new(
                        RichText::new("  Apply & Save  ")
                            .size(13.0)
                            .color(theme::badge_text())
                            .strong(),
                    )
                    .fill(theme::badge_bg())
                    .corner_radius(theme::RADIUS);
                    if ui.add(apply).clicked() {
                        if let Some(problem) = self.validate() {
                            self.error = Some(problem);
                        } else {
                            action = FavoritesAction::Apply(self.draft.clone());
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.open = false;
                    }
                });
            });

        if let Some((from, to)) = reorder {
            move_item(&mut self.draft, from, to);
        }
        if let Some(i) = remove {
            if i < self.draft.len() {
                self.draft.remove(i);
            }
        }
        if let Some(idx) = capture_toggle {
            self.capturing = if self.capturing == Some(idx) {
                None
            } else {
                Some(idx)
            };
            self.error = None;
        }

        if matches!(action, FavoritesAction::Apply(_)) {
            self.open = false;
        } else {
            self.open = is_open && self.open;
        }
        action
    }
}

/// Move `from` to `to`, shifting the rest — the reorder a drag implies.
///
/// Out-of-range indices are ignored rather than panicking: the indices
/// come from a drag payload that may refer to a row removed in the same
/// frame.
fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) {
    if from == to || from >= items.len() || to >= items.len() {
        return;
    }
    let item = items.remove(from);
    items.insert(to, item);
}

/// Render a config combo string as a glyph label, falling back to the raw
/// string if it doesn't parse.
fn display_combo(spec: &str) -> String {
    crate::keybind::KeyCombo::parse(spec)
        .map(|c| c.display())
        .unwrap_or_else(|| spec.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fav(name: &str, hotkey: &str) -> Favorite {
        Favorite {
            name: name.to_string(),
            path: PathBuf::from(format!("/notes/{name}.md.gpg")),
            hotkey: hotkey.to_string(),
        }
    }

    #[test]
    fn dragging_a_row_reorders_without_losing_entries() {
        let mut v = vec!["a", "b", "c", "d"];
        // Drag the last onto the first: it lands at the front, the rest
        // shift down — nothing is dropped or duplicated.
        move_item(&mut v, 3, 0);
        assert_eq!(v, ["d", "a", "b", "c"]);
        // Drag forward: the item lands at the target index.
        move_item(&mut v, 0, 2);
        assert_eq!(v, ["a", "b", "d", "c"]);
        // A drop onto itself changes nothing.
        move_item(&mut v, 1, 1);
        assert_eq!(v, ["a", "b", "d", "c"]);
    }

    #[test]
    fn out_of_range_drags_are_ignored_not_panics() {
        // The drag payload can name a row that was removed the same
        // frame, so this must be a no-op rather than a crash.
        let mut v = vec!["a", "b"];
        move_item(&mut v, 5, 0);
        move_item(&mut v, 0, 9);
        assert_eq!(v, ["a", "b"]);
        let mut empty: Vec<&str> = Vec::new();
        move_item(&mut empty, 0, 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn clashing_hotkeys_are_rejected_with_a_named_reason() {
        let mut m = FavoritesManager::new();

        // Two favorites on the same combo: one would silently never fire.
        m.open_with(
            &[fav("one", "ctrl+cmd+2"), fav("two", "ctrl+cmd+2")],
            "ctrl+cmd+j",
            Vec::new(),
        );
        let err = m.validate().expect("duplicate must be rejected");
        assert!(err.contains("more than one favorite"), "got {err}");

        // Colliding with the main quick-note hotkey.
        m.open_with(&[fav("one", "ctrl+cmd+j")], "ctrl+cmd+j", Vec::new());
        let err = m.validate().expect("main-hotkey clash must be rejected");
        assert!(err.contains("main Quick Note"), "got {err}");

        // Colliding with a quicknote's own hotkey — the two live in
        // different windows, so nothing else would catch this.
        m.open_with(
            &[fav("one", "ctrl+cmd+1")],
            "ctrl+cmd+j",
            vec![("Journal".to_string(), "ctrl+cmd+1".to_string())],
        );
        let err = m.validate().expect("quicknote clash must be rejected");
        assert!(err.contains("Journal"), "got {err}");

        // Distinct combos, and blank ones, are fine.
        m.open_with(
            &[fav("one", "ctrl+cmd+1"), fav("two", ""), fav("three", "")],
            "ctrl+cmd+j",
            Vec::new(),
        );
        assert!(m.validate().is_none());
    }

    #[test]
    fn adding_files_dedupes_and_respects_the_cap() {
        let mut m = FavoritesManager::new();
        m.open_with(&[], "ctrl+cmd+j", Vec::new());

        m.add_file(PathBuf::from("/notes/a.md.gpg"));
        assert_eq!(m.draft.len(), 1);
        // The same file twice would give two identical menu rows.
        m.add_file(PathBuf::from("/notes/a.md.gpg"));
        assert_eq!(m.draft.len(), 1, "duplicate rejected");
        assert!(m.error.is_some());

        for i in 0..MAX_FAVORITES + 5 {
            m.add_file(PathBuf::from(format!("/notes/f{i}.md.gpg")));
        }
        assert_eq!(m.draft.len(), MAX_FAVORITES, "cap holds");
    }
}
