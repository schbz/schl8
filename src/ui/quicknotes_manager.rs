//! Quick Notes manager: the registry of active quicknote files that the
//! menu-bar submenu and jot window offer as targets.
//!
//! Each entry names an encrypted file and (optionally) explicit
//! encryption rules — up to [`MAX_QUICKNOTE_KEYS`] keys, each with its
//! own destination path(s), all overwritten on every append. The manager
//! also creates brand-new quicknote files: pick key(s) + location(s) and
//! the encrypted file is created on the spot.

use std::path::PathBuf;

use egui::{Align2, RichText, Vec2};

use super::theme;
use crate::config::{QuickNoteFile, SaveRule, MAX_QUICKNOTES, MAX_QUICKNOTE_KEYS};
use crate::crypto::keys::PublicKey;

/// Which destination list a file-picker result should land in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestSlot {
    /// A rule of an existing registry entry.
    Existing { note_idx: usize, rule_idx: usize },
    /// A rule of the not-yet-created draft note.
    New { rule_idx: usize },
}

/// What the app should do after rendering this frame.
pub enum ManagerAction {
    None,
    /// Open a save-file picker; the chosen path is added via
    /// [`QuickNotesManager::add_destination`].
    PickDestination(DestSlot),
    /// Open an open-file picker to register an existing encrypted file.
    AddExistingFile,
    /// Persist the edited registry.
    Apply(Vec<QuickNoteFile>),
    /// Create the draft note's encrypted file(s), then register it.
    Create(QuickNoteFile),
}

pub struct QuickNotesManager {
    pub open: bool,
    draft: Vec<QuickNoteFile>,
    /// Keyring public keys for the key dropdowns (loaded on open).
    keys: Vec<PublicKey>,
    /// Stored age recipients (label, `age1…`) for the key dropdowns.
    age_recipients: Vec<(String, String)>,
    error: Option<String>,
    /// Whether the "new quicknote" form is expanded.
    creating: bool,
    new_note_name: String,
    /// Inner file type for the new note: "md" or "txt". Drives the
    /// suggested destination filename and the starter content.
    new_note_kind: &'static str,
    new_rules: Vec<SaveRule>,
    /// The main global quick-note hotkey (for conflict checks).
    main_hotkey: String,
    /// Index of the note whose hotkey button is capturing a keypress.
    capturing: Option<usize>,
}

impl QuickNotesManager {
    pub fn new() -> Self {
        Self {
            open: false,
            draft: Vec::new(),
            keys: Vec::new(),
            age_recipients: Vec::new(),
            error: None,
            creating: false,
            new_note_name: String::new(),
            new_note_kind: "md",
            new_rules: Vec::new(),
            main_hotkey: String::new(),
            capturing: None,
        }
    }

    /// Open the manager seeded with the current registry. `main_hotkey`
    /// is the global quick-note combo, used for conflict checks against
    /// per-note hotkeys.
    pub fn open_with(
        &mut self,
        notes: &[QuickNoteFile],
        main_hotkey: &str,
        age_recipients: Vec<(String, String)>,
    ) {
        self.keys = crate::crypto::keys::list_public_keys().unwrap_or_default();
        self.age_recipients = age_recipients;
        self.draft = notes.to_vec();
        self.error = None;
        self.creating = false;
        self.new_note_name.clear();
        self.new_note_kind = "md";
        self.new_rules = vec![SaveRule::default()];
        self.main_hotkey = main_hotkey.to_string();
        self.capturing = None;
        self.open = true;
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
            if let Some(note) = self.draft.get_mut(idx) {
                note.hotkey.clear();
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
            self.error = Some("A global hotkey needs a modifier (e.g. ctrl+cmd+1)".to_string());
            return;
        }
        if let Err(e) = crate::hotkey::parse(&spec) {
            self.error = Some(format!("Invalid hotkey: {e}"));
            return;
        }
        if let Some(note) = self.draft.get_mut(idx) {
            note.hotkey = spec;
        }
        self.error = None;
    }

    /// Fill rules that have destinations but no key selected with the
    /// source file's own encryption key — "no key chosen" means "keep the
    /// key this file already uses". Errors only when that key can't be
    /// determined (file missing, recipient not in the keyring, …).
    fn fill_default_keys(&mut self) -> Result<(), String> {
        for note in &mut self.draft {
            let needs_default = note
                .rules
                .iter()
                .any(|r| !r.has_key() && !r.destinations.is_empty());
            if !needs_default {
                continue;
            }
            // age files don't record their recipient in the ciphertext, so
            // there's no "file's own key" to fall back on — the user must
            // pick the age key explicitly.
            if crate::document::loader::is_age_file(&note.source) {
                return Err(format!(
                    "\"{}\": choose the AGE key this file is encrypted to \
                     (AGE files don't record their recipient)",
                    note.name
                ));
            }
            let recips =
                crate::crypto::keys::recipients_for_reencrypt(&note.source).map_err(|e| {
                    format!(
                        "\"{}\": a destination has no key selected, and the file's own \
                         key could not be determined to use as the default ({e:#})",
                        note.name
                    )
                })?;
            let Some(fpr) = recips.first() else {
                return Err(format!(
                    "\"{}\": a destination has no key selected, and the file's own \
                     key could not be determined to use as the default",
                    note.name
                ));
            };
            let label = self
                .keys
                .iter()
                .find(|k| k.fingerprint.eq_ignore_ascii_case(fpr))
                .map(|k| k.uid.clone())
                .unwrap_or_else(|| format!("file's key ({})", short_fpr(fpr)));
            for r in &mut note.rules {
                if r.key_fingerprint.is_empty() && !r.destinations.is_empty() {
                    r.key_fingerprint = fpr.clone();
                    r.key_label = label.clone();
                }
            }
        }
        Ok(())
    }

    /// Validate the whole draft before Apply. Returns the first problem.
    fn validate_draft(&self) -> Option<String> {
        for note in &self.draft {
            if note
                .rules
                .iter()
                .any(|r| !r.has_key() && !r.destinations.is_empty())
            {
                return Some("Every destination list needs a key selected".to_string());
            }
            if let Some(dup) = crate::config::duplicate_destination(&note.rules) {
                return Some(format!(
                    "\"{}\": {} is a destination for more than one key — each \
                     key needs its own file",
                    note.name,
                    dup.display()
                ));
            }
            let spec = note.hotkey.trim();
            if !spec.is_empty() {
                if let Err(e) = crate::hotkey::parse(spec) {
                    return Some(format!("\"{}\": invalid hotkey — {e}", note.name));
                }
                if spec.eq_ignore_ascii_case(self.main_hotkey.trim()) {
                    return Some(format!(
                        "\"{}\": hotkey {spec} is already the main Quick Note hotkey",
                        note.name
                    ));
                }
                let clashes = self
                    .draft
                    .iter()
                    .filter(|n| n.hotkey.trim().eq_ignore_ascii_case(spec))
                    .count();
                if clashes > 1 {
                    return Some(format!("Hotkey {spec} is used by more than one quicknote"));
                }
            }
        }
        None
    }

    /// Inner extension for `slot`'s destination suggestion: the form's
    /// choice for a new note; an existing note's own inner type (every
    /// copy of a note should render the same way).
    pub fn slot_inner_kind(&self, slot: DestSlot) -> &'static str {
        match slot {
            DestSlot::New { .. } => self.new_note_kind,
            DestSlot::Existing { note_idx, .. } => self
                .draft
                .get(note_idx)
                .and_then(|n| n.source.file_name())
                .and_then(|f| f.to_str())
                .map(|name| {
                    let inner = name
                        .trim_end_matches(".gpg")
                        .trim_end_matches(".asc")
                        .trim_end_matches(".age");
                    if inner.ends_with(".txt") || inner.ends_with(".text") {
                        "txt"
                    } else {
                        "md"
                    }
                })
                .unwrap_or("md"),
        }
    }

    /// Whether the rule behind `slot` encrypts with age (so its
    /// destinations should default to a `.age` name).
    pub fn slot_is_age(&self, slot: DestSlot) -> bool {
        let rule = match slot {
            DestSlot::Existing { note_idx, rule_idx } => {
                self.draft.get(note_idx).and_then(|n| n.rules.get(rule_idx))
            }
            DestSlot::New { rule_idx } => self.new_rules.get(rule_idx),
        };
        rule.is_some_and(SaveRule::is_age)
    }

    /// Called by the app after a destination picker returns.
    pub fn add_destination(&mut self, slot: DestSlot, path: PathBuf) {
        let rule = match slot {
            DestSlot::Existing { note_idx, rule_idx } => self
                .draft
                .get_mut(note_idx)
                .and_then(|n| n.rules.get_mut(rule_idx)),
            DestSlot::New { rule_idx } => self.new_rules.get_mut(rule_idx),
        };
        if let Some(rule) = rule {
            if !rule.destinations.contains(&path) {
                rule.destinations.push(path);
            }
        }
    }

    /// Called by the app after the add-existing-file picker returns.
    pub fn add_existing(&mut self, path: PathBuf) {
        if self.draft.len() >= MAX_QUICKNOTES {
            self.error = Some(format!("Registry is full ({MAX_QUICKNOTES} quicknotes)"));
            return;
        }
        if self.draft.iter().any(|n| n.source == path) {
            self.error = Some("That file is already a quicknote".to_string());
            return;
        }
        self.draft.push(QuickNoteFile::for_existing(path));
    }

    /// Key selector + destination list for one rule. Returns
    /// (remove-this-rule, pick-a-destination).
    fn render_rule(
        ui: &mut egui::Ui,
        rule: &mut SaveRule,
        keys: &[PublicKey],
        age_recipients: &[(String, String)],
        id: (&str, usize, usize),
        removable_dests: bool,
        // When true, an unset key means "use the file's own key" instead
        // of being an error (existing files have an original key to fall
        // back on; brand-new files don't).
        default_key_hint: bool,
    ) -> (bool, bool) {
        let mut remove_rule = false;
        let mut pick_dest = false;

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("\u{1F511} Key")
                    .size(12.0)
                    .color(theme::text_dim()),
            );
            let selected = if rule.has_key() {
                rule.key_label.clone()
            } else if default_key_hint {
                "File's own key (default)".to_string()
            } else {
                "Choose a key…".to_string()
            };
            egui::ComboBox::from_id_salt(id)
                .selected_text(RichText::new(selected).size(12.5))
                .width(250.0)
                .show_ui(ui, |ui| {
                    for k in keys {
                        let text = format!("{}  ({})", k.uid, short_fpr(&k.fingerprint));
                        let is_sel = !rule.is_age()
                            && rule.key_fingerprint.eq_ignore_ascii_case(&k.fingerprint);
                        if ui
                            .selectable_label(is_sel, RichText::new(text).size(12.5))
                            .clicked()
                        {
                            rule.key_fingerprint = k.fingerprint.clone();
                            rule.age_recipient.clear();
                            rule.key_label = k.uid.clone();
                        }
                    }
                    if !age_recipients.is_empty() {
                        ui.separator();
                        ui.label(
                            RichText::new("AGE keys")
                                .size(11.0)
                                .color(theme::text_dim()),
                        );
                        for (label, recipient) in age_recipients {
                            let short = if recipient.len() > 14 {
                                format!("{}…", &recipient[..14])
                            } else {
                                recipient.clone()
                            };
                            let text = format!("{label}  (AGE: {short})");
                            let is_sel = rule.age_recipient == *recipient;
                            if ui
                                .selectable_label(is_sel, RichText::new(text).size(12.5))
                                .clicked()
                            {
                                rule.age_recipient = recipient.clone();
                                rule.key_fingerprint.clear();
                                rule.key_label = format!("{label} (AGE)");
                            }
                        }
                    }
                });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(RichText::new("Remove key").size(11.5)).clicked() {
                    remove_rule = true;
                }
            });
        });

        let mut remove_dest: Option<usize> = None;
        for (di, dest) in rule.destinations.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("\u{2022}").size(12.0).color(theme::accent()));
                ui.label(
                    RichText::new(dest.display().to_string())
                        .size(11.5)
                        .monospace()
                        .color(theme::text_primary()),
                );
                if removable_dests || di > 0 {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(RichText::new("x").size(10.0))
                            .on_hover_text("Remove destination")
                            .clicked()
                        {
                            remove_dest = Some(di);
                        }
                    });
                }
            });
        }
        if let Some(di) = remove_dest {
            rule.destinations.remove(di);
        }
        if ui
            .button(RichText::new("+ Add destination…").size(11.5))
            .clicked()
        {
            pick_dest = true;
        }

        (remove_rule, pick_dest)
    }

    /// Render. Returns the action for the app to take.
    pub fn render(&mut self, ctx: &egui::Context) -> ManagerAction {
        if !self.open {
            return ManagerAction::None;
        }

        self.handle_capture(ctx);
        let capturing = self.capturing;
        let mut capture_toggle: Option<usize> = None;

        let mut action = ManagerAction::None;
        let mut is_open = self.open;
        let max_height = (ctx.screen_rect().height() - 90.0).max(260.0);

        egui::Window::new("Quick Notes")
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .default_width(theme::dialog_width(ctx, 560.0))
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

                ui.label(theme::gradient_text("Quick Note Files", 16.0));
                ui.label(
                    RichText::new(format!(
                        "These files appear in the menu-bar Quick Note submenu \
                         ({}/{MAX_QUICKNOTES}). A note with explicit keys is \
                         re-encrypted to each key and written to all of that \
                         key's destinations on every append; a note without \
                         keys is re-encrypted in place to its own recipients.",
                        self.draft.len()
                    ))
                    .size(11.5)
                    .color(theme::text_dim()),
                );
                ui.add_space(2.0);

                // ── Existing entries ─────────────────────────────────
                let keys = self.keys.clone();
                let age_recipients = self.age_recipients.clone();
                let mut remove_note: Option<usize> = None;
                for (ni, note) in self.draft.iter_mut().enumerate() {
                    egui::Frame::NONE
                        .fill(theme::bg_raised().gamma_multiply(0.6))
                        .corner_radius(theme::RADIUS)
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 5.0;

                            // Checked per frame only while the manager is
                            // open — a stat per visible entry is cheap.
                            let missing = !note.source.exists();
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Name").size(12.0).color(theme::text_dim()));
                                ui.add(
                                    egui::TextEdit::singleline(&mut note.name)
                                        .desired_width(220.0)
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
                                        "The encrypted file was deleted or moved. This entry \
                                         is hidden from the quick-note lists until you remove \
                                         it or the file comes back.",
                                    );
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .button(RichText::new("Remove").size(11.5))
                                            .on_hover_text(
                                                "Remove from the registry (the file itself \
                                                 is not deleted)",
                                            )
                                            .clicked()
                                        {
                                            remove_note = Some(ni);
                                        }
                                    },
                                );
                            });
                            ui.label(
                                RichText::new(note.source.display().to_string())
                                    .size(11.0)
                                    .monospace()
                                    .color(theme::text_dim()),
                            );

                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Hotkey").size(12.0).color(theme::text_dim()),
                                );
                                let capturing_this = capturing == Some(ni);
                                let text = if capturing_this {
                                    "  press keys…  ".to_string()
                                } else if note.hotkey.is_empty() {
                                    "  —  ".to_string()
                                } else {
                                    format!("  {}  ", display_combo(&note.hotkey))
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
                                        "Optional system-wide hotkey that opens Quick Note \
                                         preselected on this file — works from anywhere \
                                         while Schl8 runs, so a programmable keypad key \
                                         can jot straight into it. Click, then press the \
                                         combo (needs a modifier, e.g. ctrl+cmd+1). \
                                         Esc cancels · Backspace clears.",
                                    )
                                    .clicked()
                                {
                                    capture_toggle = Some(ni);
                                }
                            });

                            let mut remove_rule: Option<usize> = None;
                            for (ri, rule) in note.rules.iter_mut().enumerate() {
                                ui.separator();
                                let (rm, pick) = Self::render_rule(
                                    ui,
                                    rule,
                                    &keys,
                                    &age_recipients,
                                    ("qn_rule", ni, ri),
                                    true,
                                    true,
                                );
                                if rm {
                                    remove_rule = Some(ri);
                                }
                                if pick {
                                    action = ManagerAction::PickDestination(DestSlot::Existing {
                                        note_idx: ni,
                                        rule_idx: ri,
                                    });
                                }
                            }
                            if let Some(ri) = remove_rule {
                                note.rules.remove(ri);
                            }

                            if note.rules.len() < MAX_QUICKNOTE_KEYS {
                                let label = if note.rules.is_empty() {
                                    "+ Add key (else: re-encrypts in place)"
                                } else {
                                    "+ Add key"
                                };
                                if ui.button(RichText::new(label).size(11.5)).clicked() {
                                    note.rules.push(SaveRule::default());
                                }
                            }
                        });
                }
                if let Some(ni) = remove_note {
                    self.draft.remove(ni);
                }

                // One-click cleanup after files were deleted on disk.
                let missing_count = self.draft.iter().filter(|n| !n.source.exists()).count();
                if missing_count > 0
                    && ui
                        .button(
                            RichText::new(format!(
                                "Remove {missing_count} entr{} whose file is missing",
                                if missing_count == 1 { "y" } else { "ies" }
                            ))
                            .size(12.0),
                        )
                        .on_hover_text(
                            "Drops the registry entries only — nothing on disk is touched. \
                             Takes effect on Apply & Save.",
                        )
                        .clicked()
                {
                    self.draft.retain(|n| n.source.exists());
                }

                // ── Add / create ─────────────────────────────────────
                ui.horizontal(|ui| {
                    let room = self.draft.len() < MAX_QUICKNOTES;
                    if ui
                        .add_enabled(
                            room,
                            egui::Button::new(RichText::new("+ Add existing file…").size(12.0)),
                        )
                        .clicked()
                    {
                        action = ManagerAction::AddExistingFile;
                    }
                    if ui
                        .add_enabled(
                            room && !self.creating,
                            egui::Button::new(RichText::new("+ New quicknote…").size(12.0)),
                        )
                        .clicked()
                    {
                        self.creating = true;
                        self.error = None;
                    }
                });

                // ── New-note form ────────────────────────────────────
                if self.creating {
                    egui::Frame::NONE
                        .fill(theme::bg_raised())
                        .corner_radius(theme::RADIUS)
                        .stroke(egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.35)))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 5.0;
                            ui.label(theme::gradient_text("New Quick Note", 14.0));
                            ui.label(
                                RichText::new(
                                    "Pick key(s) and where each key's encrypted copy \
                                     lives. The first destination of the first key is \
                                     the file the submenu opens and appends to.",
                                )
                                .size(11.0)
                                .color(theme::text_dim()),
                            );
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Name").size(12.0).color(theme::text_dim()));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.new_note_name)
                                        .desired_width(220.0)
                                        .hint_text("e.g. Journal"),
                                );
                                ui.add_space(8.0);
                                ui.label(RichText::new("Type").size(12.0).color(theme::text_dim()));
                                ui.radio_value(
                                    &mut self.new_note_kind,
                                    "md",
                                    RichText::new("Markdown").size(12.0),
                                );
                                ui.radio_value(
                                    &mut self.new_note_kind,
                                    "txt",
                                    RichText::new("Plain text").size(12.0),
                                );
                            });

                            let mut remove_rule: Option<usize> = None;
                            for (ri, rule) in self.new_rules.iter_mut().enumerate() {
                                ui.separator();
                                let (rm, pick) = Self::render_rule(
                                    ui,
                                    rule,
                                    &keys,
                                    &age_recipients,
                                    ("qn_new", 0, ri),
                                    false,
                                    false,
                                );
                                if rm {
                                    remove_rule = Some(ri);
                                }
                                if pick {
                                    action = ManagerAction::PickDestination(DestSlot::New {
                                        rule_idx: ri,
                                    });
                                }
                            }
                            if let Some(ri) = remove_rule {
                                self.new_rules.remove(ri);
                            }
                            if self.new_rules.len() < MAX_QUICKNOTE_KEYS
                                && ui.button(RichText::new("+ Add key").size(11.5)).clicked()
                            {
                                self.new_rules.push(SaveRule::default());
                            }

                            ui.add_space(2.0);
                            ui.horizontal(|ui| {
                                let create = egui::Button::new(
                                    RichText::new("  Create Encrypted File  ")
                                        .size(12.5)
                                        .color(theme::badge_text())
                                        .strong(),
                                )
                                .fill(theme::badge_bg())
                                .corner_radius(theme::RADIUS);
                                if ui.add(create).clicked() {
                                    match self.validated_new_note() {
                                        Ok(note) => {
                                            action = ManagerAction::Create(note);
                                        }
                                        Err(e) => self.error = Some(e),
                                    }
                                }
                                if ui.button("Cancel").clicked() {
                                    self.creating = false;
                                    self.error = None;
                                }
                            });
                        });
                }

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
                        if let Err(e) = self.fill_default_keys() {
                            self.error = Some(e);
                        } else if let Some(problem) = self.validate_draft() {
                            self.error = Some(problem);
                        } else {
                            action = ManagerAction::Apply(self.draft.clone());
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.open = false;
                    }
                });
            });

        if let Some(idx) = capture_toggle {
            self.capturing = if self.capturing == Some(idx) {
                None
            } else {
                Some(idx)
            };
            self.error = None;
        }

        if matches!(action, ManagerAction::Apply(_)) {
            self.open = false;
        } else {
            self.open = is_open && self.open;
        }
        action
    }

    /// Validate the new-note form into a registry entry (source = the
    /// first key's first destination).
    fn validated_new_note(&self) -> Result<QuickNoteFile, String> {
        let rules: Vec<SaveRule> = self
            .new_rules
            .iter()
            .filter(|r| r.has_key() || !r.destinations.is_empty())
            .cloned()
            .collect();
        if rules.is_empty() {
            return Err("Add at least one key with a destination".to_string());
        }
        for r in &rules {
            if !r.has_key() {
                return Err("Every destination list needs a key selected".to_string());
            }
            if r.destinations.is_empty() {
                return Err(format!(
                    "Key \"{}\" has no destination",
                    if r.key_label.is_empty() {
                        short_fpr(&r.key_fingerprint)
                    } else {
                        r.key_label.clone()
                    }
                ));
            }
        }
        if let Some(dup) = crate::config::duplicate_destination(&rules) {
            return Err(format!(
                "{} is a destination for more than one key — each key needs \
                 its own file",
                dup.display()
            ));
        }
        let source = rules[0].destinations[0].clone();
        let name = if self.new_note_name.trim().is_empty() {
            source
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("note")
                .to_string()
        } else {
            self.new_note_name.trim().to_string()
        };
        let mut note = QuickNoteFile {
            name,
            source,
            rules,
            ..Default::default()
        };
        note.normalize();
        Ok(note)
    }

    /// After the app successfully created the note's file(s): register it
    /// in the draft and collapse the form.
    pub fn created(&mut self, note: QuickNoteFile) {
        self.draft.push(note);
        self.creating = false;
        self.new_note_name.clear();
        self.new_note_kind = "md";
        self.new_rules = vec![SaveRule::default()];
        self.error = None;
    }

    /// Surface a creation error inside the window.
    pub fn set_error(&mut self, msg: String) {
        self.error = Some(msg);
    }
}

fn short_fpr(fpr: &str) -> String {
    if fpr.len() >= 16 {
        fpr[fpr.len() - 16..].to_string()
    } else {
        fpr.to_string()
    }
}

/// Render a config combo string as a glyph label, falling back to the raw
/// string if it doesn't parse.
fn display_combo(spec: &str) -> String {
    crate::keybind::KeyCombo::parse(spec)
        .map(|c| c.display())
        .unwrap_or_else(|| spec.to_string())
}
