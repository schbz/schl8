//! Settings window: rebind keyboard shortcuts (in-app + the system-wide
//! quick-note hotkey) and tweak appearance, all with live apply.
//!
//! Rebinding is capture-based: click a shortcut's button, then press the
//! combo. Esc cancels a capture; Backspace/Delete clears the binding.

use egui::{Align2, RichText, Vec2};

use super::theme;
use crate::config::Config;
use crate::keybind::KeyCombo;

/// One rebindable in-app action: a label and accessors into the config.
struct Binding {
    label: &'static str,
    get: fn(&Config) -> &String,
    set: fn(&mut Config, String),
}

macro_rules! binding {
    ($label:expr, $field:ident) => {
        Binding {
            label: $label,
            get: |c| &c.keybindings.$field,
            set: |c, v| c.keybindings.$field = v,
        }
    };
}

fn bindings() -> Vec<Binding> {
    vec![
        binding!("Open file", open_file),
        binding!("New markdown", new_markdown),
        binding!("New text", new_text),
        binding!("Quick note (in-app)", quick_note),
        binding!("Save", save),
        binding!("Save As", save_as),
        binding!("Toggle edit", toggle_edit),
        binding!("Close document", close_document),
        binding!("Settings", settings),
        binding!("Find & replace", find),
        binding!("Lock now (panic)", panic_lock),
    ]
}

/// Last 16 hex digits of a fingerprint — enough to recognize a key
/// without a 40-character wall of hex.
fn short_fpr(fpr: &str) -> String {
    if fpr.len() >= 16 {
        fpr[fpr.len() - 16..].to_string()
    } else {
        fpr.to_string()
    }
}

/// Which field is currently capturing a keypress.
#[derive(PartialEq, Clone)]
enum Capturing {
    None,
    /// An in-app binding, by index into `bindings()`.
    InApp(usize),
    /// The system-wide global quick-note hotkey.
    Global,
}

pub struct SettingsDialog {
    pub open: bool,
    /// Working copy edited in the dialog; committed to the app on Apply.
    draft: Config,
    capturing: Capturing,
    error: Option<String>,
    /// Start-at-login checkbox state (backed by the LaunchAgent plist,
    /// not the config file; applied on Apply).
    start_at_login: bool,
    start_at_login_initial: bool,
}

impl SettingsDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            draft: Config::default(),
            capturing: Capturing::None,
            error: None,
            start_at_login: false,
            start_at_login_initial: false,
        }
    }

    /// Open the dialog seeded from the current config.
    pub fn open(&mut self, config: &Config) {
        self.draft = config.clone();
        self.capturing = Capturing::None;
        self.error = None;
        self.start_at_login = crate::login_item::is_enabled();
        self.start_at_login_initial = self.start_at_login;
        self.open = true;
    }

    /// Whether Apply should change the login item, and to what.
    pub fn login_item_change(&self) -> Option<bool> {
        (self.start_at_login != self.start_at_login_initial).then_some(self.start_at_login)
    }

    /// Render the dialog. Returns Some((new_config, persist)) when the
    /// user applies changes: `persist = true` for "Apply & Save" (write
    /// to disk, close), `false` for "Apply" (test live with the dialog
    /// still open; nothing written).
    pub fn render(&mut self, ctx: &egui::Context) -> Option<(Config, bool)> {
        if !self.open {
            return None;
        }

        // ── Capture handling ─────────────────────────────────────────
        if self.capturing != Capturing::None {
            let captured = ctx.input(|i| {
                // Cancel on Esc; clear on Backspace/Delete.
                if i.key_pressed(egui::Key::Escape) {
                    return Some(CaptureResult::Cancel);
                }
                if i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete) {
                    return Some(CaptureResult::Clear);
                }
                // First non-modifier key press becomes the combo.
                for ev in &i.events {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = ev
                    {
                        if is_modifier_key(*key) {
                            continue;
                        }
                        return Some(CaptureResult::Combo(KeyCombo::from_event(*key, modifiers)));
                    }
                }
                None
            });
            if let Some(result) = captured {
                self.apply_capture(result);
            }
        }

        let mut applied = None;
        let mut is_open = self.open;

        // Cap both dimensions to the app window and scroll the contents,
        // so every control stays reachable in a small main window. Width
        // goes through the shared clamp in `theme`; see there for why a
        // centered dialog cannot rely on scrolling to rescue it.
        let max_height = (ctx.screen_rect().height() - 90.0).max(240.0);

        egui::Window::new("Settings")
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .default_width(theme::dialog_width(ctx, 480.0))
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

                // ── Keyboard shortcuts ───────────────────────────────
                ui.label(
                    RichText::new("Keyboard Shortcuts")
                        .size(15.0)
                        .strong()
                        .color(theme::text_strong()),
                );
                ui.label(
                    RichText::new(
                        "Click a shortcut, then press the new combo. \
                         Esc cancels · Backspace clears.",
                    )
                    .size(11.0)
                    .color(theme::text_dim()),
                );
                ui.add_space(2.0);

                // Global hotkey first (distinct: works system-wide).
                self.shortcut_row(
                    ui,
                    "Quick note (global hotkey)",
                    Capturing::Global,
                    &self.draft.quick_note.hotkey.clone(),
                );

                ui.separator();

                let defs = bindings();
                for (idx, b) in defs.iter().enumerate() {
                    let current = (b.get)(&self.draft).clone();
                    self.shortcut_row(ui, b.label, Capturing::InApp(idx), &current);
                }

                if let Some(err) = &self.error {
                    ui.add_space(2.0);
                    ui.label(RichText::new(err).size(12.0).color(theme::accent_red()));
                }

                ui.add_space(6.0);
                ui.separator();

                // ── Appearance ───────────────────────────────────────
                ui.label(
                    RichText::new("Appearance")
                        .size(15.0)
                        .strong()
                        .color(theme::text_strong()),
                );
                self.appearance_controls(ui);

                ui.add_space(6.0);
                ui.separator();

                // ── Crawl (animated reading) ─────────────────────────
                ui.label(
                    RichText::new("Crawl \u{2014} animated reading")
                        .size(15.0)
                        .strong()
                        .color(theme::text_strong()),
                );
                ui.label(
                    RichText::new(
                        "The document scrolls by itself so you can just read. These are the \
                         starting values \u{2014} while a crawl runs, Space pauses, Up/Down \
                         change speed, +/- change text size, R reverses and Esc exits, and \
                         those changes are not saved here.",
                    )
                    .size(11.5)
                    .color(theme::text_dim()),
                );
                ui.add_space(2.0);
                self.crawl_controls(ui);

                ui.add_space(6.0);
                ui.separator();

                // ── Security: held edits ─────────────────────────────
                ui.label(
                    RichText::new("Unsaved edits when the session locks")
                        .size(15.0)
                        .strong()
                        .color(theme::text_strong()),
                );
                ui.label(
                    RichText::new(
                        "Locking encrypts anything unsaved and drops the plaintext, so a \
                         lock never costs you work and unsaved work never keeps the session \
                         unlocked. Restoring it afterwards needs the matching private key.",
                    )
                    .size(11.5)
                    .color(theme::text_dim()),
                );
                ui.add_space(2.0);
                self.stash_key_controls(ui);

                ui.add_space(6.0);
                ui.separator();

                // ── Security: AGE identity lifetime ──────────────────
                ui.label(
                    RichText::new("AGE seed-phrase identity")
                        .size(15.0)
                        .strong()
                        .color(theme::text_strong()),
                );
                ui.label(
                    RichText::new(
                        "Your seed phrase and the key derived from it are never written to \
                         disk — they live only in locked memory and are wiped when Schl8 \
                         quits. These settings bound how long the key stays in memory while \
                         Schl8 is running (closing the window to the menu bar does not quit it).",
                    )
                    .size(11.5)
                    .color(theme::text_dim()),
                );
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut self.draft.age_lock.forget_idle_minutes)
                            .range(0..=1440)
                            .suffix(" min"),
                    )
                    .on_hover_text("0 = never forget on idle");
                    ui.label(
                        RichText::new("Forget after this long with no input (0 = never)")
                            .size(12.5)
                            .color(theme::text_primary()),
                    );
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut self.draft.age_lock.forget_after_minutes)
                            .range(0..=1440)
                            .suffix(" min"),
                    )
                    .on_hover_text(
                        "A hard ceiling: the key is wiped this long after you unlock it, \
                         however busy you are. 0 = no ceiling",
                    );
                    ui.label(
                        RichText::new("Forget this long after unlocking (0 = never)")
                            .size(12.5)
                            .color(theme::text_primary()),
                    );
                });
                ui.checkbox(
                    &mut self.draft.age_lock.forget_on_sleep,
                    RichText::new("Forget when the display sleeps or the screen locks")
                        .size(12.5)
                        .color(theme::text_primary()),
                );
                ui.checkbox(
                    &mut self.draft.age_lock.forget_on_window_close,
                    RichText::new("Forget when the window is closed to the menu bar")
                        .size(12.5)
                        .color(theme::text_primary()),
                )
                .on_hover_text(
                    "Opening the quick-note window hides the main window too, but that \
                     does not count — otherwise every quick note would re-prompt",
                );

                ui.separator();

                // ── Files ────────────────────────────────────────────
                ui.label(
                    RichText::new("Files")
                        .size(15.0)
                        .strong()
                        .color(theme::text_strong()),
                );
                ui.label(
                    RichText::new("Notes folder")
                        .size(12.0)
                        .color(theme::text_dim()),
                );
                let effective = self
                    .draft
                    .notes_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    let mut shown = self.draft.app.notes_dir.display().to_string();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut shown)
                                // Leave room for the Choose… button, but
                                // never ask for a negative width — in a
                                // narrow window that would push the
                                // button off the row instead of shrinking
                                // the field.
                                .desired_width((ui.available_width() - 84.0).max(120.0))
                                .font(egui::TextStyle::Monospace)
                                .hint_text(effective.clone()),
                        )
                        .changed()
                    {
                        self.draft.app.notes_dir = shown.trim().into();
                    }
                    if ui.button(RichText::new(" Choose… ").size(12.0)).clicked() {
                        if let Some(dir) = rfd::FileDialog::new()
                            .set_title("Choose notes folder")
                            .pick_folder()
                        {
                            self.draft.app.notes_dir = dir;
                        }
                    }
                });
                ui.label(
                    RichText::new(format!("New files default here — currently {effective}"))
                        .size(11.0)
                        .color(theme::text_dim()),
                )
                .on_hover_text(
                    "Where Schl8 puts new encrypted files, and the one place \
                     an agent working through `schl8 agent brief` is told it \
                     may write without asking. Leave blank for \
                     ~/Documents/Schl8. Only ever holds encrypted files.",
                );

                ui.add_space(8.0);
                ui.separator();

                // ── Automation ───────────────────────────────────────
                ui.label(
                    RichText::new("Automation")
                        .size(15.0)
                        .strong()
                        .color(theme::text_strong()),
                );
                ui.checkbox(
                    &mut self.start_at_login,
                    RichText::new("Start Schl8 at login")
                        .size(13.0)
                        .color(theme::text_primary()),
                )
                .on_hover_text(
                    "Installs a LaunchAgent (~/Library/LaunchAgents) so Schl8 \
                     starts with macOS and sits in the menu bar, keeping the \
                     global quick-note hotkeys available",
                );
                ui.label(
                    RichText::new("After every save, run (optional)")
                        .size(12.0)
                        .color(theme::text_dim()),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.draft.app.post_save_command)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace)
                        .hint_text("e.g. ~/bin/schl8-backup.sh"),
                )
                .on_hover_text(
                    "Shell command run in the background after every successful \
                     save or quick-note append — for backups, syncing to a \
                     server, etc. $SCHL8_SOURCE is the saved document's path \
                     and $SCHL8_DESTINATIONS lists every encrypted file \
                     written (one per line). Only paths are exposed — never \
                     document content.",
                );

                ui.add_space(8.0);
                ui.separator();

                // ── Actions ──────────────────────────────────────────
                ui.horizontal(|ui| {
                    let apply_save = egui::Button::new(
                        RichText::new("  Apply & Save  ")
                            .size(13.0)
                            .color(theme::badge_text())
                            .strong(),
                    )
                    .fill(theme::badge_bg())
                    .corner_radius(theme::RADIUS);
                    if ui.add(apply_save).clicked() {
                        applied = Some((self.draft.clone(), true));
                    }
                    let apply_test = egui::Button::new(
                        RichText::new("  Apply  ")
                            .size(13.0)
                            .color(theme::text_strong()),
                    )
                    .fill(theme::bg_raised())
                    .stroke(egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.6)))
                    .corner_radius(theme::RADIUS);
                    if ui
                        .add(apply_test)
                        .on_hover_text(
                            "Try these settings now without saving them — the window \
                             stays open. Apply & Save persists; quitting without \
                             saving reverts on next launch",
                        )
                        .clicked()
                    {
                        applied = Some((self.draft.clone(), false));
                    }
                    if ui.button("Cancel").clicked() {
                        self.open = false;
                    }
                    if ui.button("Reset to defaults").clicked() {
                        let hotkey = self.draft.quick_note.hotkey.clone();
                        self.draft.keybindings = crate::config::Keybindings::default();
                        // Keep the user's global hotkey unless they reset it too.
                        self.draft.quick_note.hotkey = hotkey;
                        self.error = None;
                    }
                });
            });

        // Only "Apply & Save" closes the dialog; a test-apply keeps it
        // open so the user can keep tweaking.
        if matches!(applied, Some((_, true))) {
            self.open = false;
        }
        self.open = is_open && self.open;
        applied
    }

    /// One shortcut row: label + a button showing the current combo that
    /// enters capture mode when clicked.
    fn shortcut_row(&mut self, ui: &mut egui::Ui, label: &str, slot: Capturing, current: &str) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(13.0).color(theme::text_primary()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let capturing = self.capturing == slot;
                let text = if capturing {
                    "  press keys…  ".to_string()
                } else if current.is_empty() {
                    "  —  ".to_string()
                } else {
                    format!("  {}  ", display_combo(current))
                };
                let btn = egui::Button::new(RichText::new(text).size(13.0).monospace().color(
                    if capturing {
                        theme::badge_text()
                    } else {
                        theme::accent()
                    },
                ))
                .fill(if capturing {
                    theme::accent()
                } else {
                    theme::bg_raised()
                })
                .corner_radius(6.0)
                .min_size(egui::vec2(130.0, 26.0));
                if ui.add(btn).clicked() {
                    self.capturing = if capturing { Capturing::None } else { slot };
                    self.error = None;
                }
            });
        });
    }

    fn appearance_controls(&mut self, ui: &mut egui::Ui) {
        let a = &mut self.draft.appearance;

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Theme")
                    .size(13.0)
                    .color(theme::text_primary()),
            );
            egui::ComboBox::from_id_salt("settings_theme")
                .selected_text(a.theme.clone())
                .show_ui(ui, |ui| {
                    for name in theme::PRESETS {
                        ui.selectable_value(&mut a.theme, name.to_string(), *name);
                    }
                });
        });

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Accent")
                    .size(13.0)
                    .color(theme::text_primary()),
            );
            let mut hex = a.accent.clone();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut hex)
                    .hint_text("#RRGGBB (blank = preset)")
                    .desired_width(120.0),
            );
            if resp.changed() {
                a.accent = hex;
            }
            // Live swatch when the hex parses.
            if let Some(c) = theme::parse_hex(&a.accent) {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(22.0, 18.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 4.0, c);
            }
        });

        // (No opacity control: the window is always fully opaque —
        // translucency would let other windows shine through near
        // decrypted text.)

        // Font family (built-in or a system font), applied live.
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Font")
                    .size(13.0)
                    .color(theme::text_primary()),
            );
            let current = theme::font_label(&a.font).to_string();
            egui::ComboBox::from_id_salt("settings_font")
                .selected_text(current)
                .width(180.0)
                .show_ui(ui, |ui| {
                    for (value, label, path) in theme::FONT_CHOICES {
                        // Offer only fonts actually present on this system.
                        if !path.is_empty() && !std::path::Path::new(path).exists() {
                            continue;
                        }
                        ui.selectable_value(&mut a.font, value.to_string(), *label);
                    }
                });
        });

        // Interface scale. Applied as egui's zoom factor, so it grows the
        // whole app rather than only the labels that happen to use a
        // default text style — see theme::apply_font_scale.
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Font size")
                    .size(13.0)
                    .color(theme::text_primary()),
            );
            ui.add(
                egui::Slider::new(
                    &mut a.font_scale,
                    crate::config::MIN_FONT_SCALE..=crate::config::MAX_FONT_SCALE,
                )
                .step_by(0.05)
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                .custom_parser(|s| {
                    s.trim_end_matches('%')
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .map(|p| p / 100.0)
                }),
            )
            .on_hover_text(
                "Scales the whole interface — menus, lists, dialogs and \
                 document text together. Applied live, so you can see it \
                 before saving.",
            );
            if ui
                .button(RichText::new("Reset").size(11.5))
                .on_hover_text("Back to 100%")
                .clicked()
            {
                a.font_scale = 1.0;
            }
        });

        // Keyboard layout (drives the vim-style navigation keys).
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Keyboard")
                    .size(13.0)
                    .color(theme::text_primary()),
            );
            let layout = &mut self.draft.app.keyboard_layout;
            egui::ComboBox::from_id_salt("settings_layout")
                .selected_text(layout.clone())
                .show_ui(ui, |ui| {
                    for name in ["qwerty", "dvorak", "colemak", "workman"] {
                        ui.selectable_value(layout, name.to_string(), name);
                    }
                });
            ui.label(
                RichText::new("(navigation keys)")
                    .size(10.0)
                    .color(theme::text_dim()),
            );
        });
    }

    /// Crawl-mode defaults.
    fn crawl_controls(&mut self, ui: &mut egui::Ui) {
        let c = &mut self.draft.crawl;

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Speed")
                    .size(13.0)
                    .color(theme::text_primary()),
            );
            ui.add(
                egui::Slider::new(
                    &mut c.speed,
                    crate::config::MIN_CRAWL_SPEED..=crate::config::MAX_CRAWL_SPEED,
                )
                .suffix(" pt/s")
                .logarithmic(true),
            )
            .on_hover_text(
                "How fast the text moves. Logarithmic, because the useful range \
                 runs from a slow drift to a fast skim.",
            );
        });

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Text size")
                    .size(13.0)
                    .color(theme::text_primary()),
            );
            ui.add(
                egui::Slider::new(
                    &mut c.text_scale,
                    crate::config::MIN_CRAWL_SCALE..=crate::config::MAX_CRAWL_SCALE,
                )
                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
            )
            .on_hover_text("Bigger text while crawling, for reading at a distance.");
        });

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Direction")
                    .size(13.0)
                    .color(theme::text_primary()),
            );
            ui.radio_value(
                &mut c.direction_up,
                true,
                RichText::new("Forward").size(12.5),
            )
            .on_hover_text("Text rises up the screen — ordinary reading.");
            ui.radio_value(
                &mut c.direction_up,
                false,
                RichText::new("Reverse").size(12.5),
            )
            .on_hover_text("Text descends, walking back toward the start.");
        });

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Column width")
                    .size(13.0)
                    .color(theme::text_primary()),
            );
            ui.add(
                egui::Slider::new(&mut c.column_width, 0.0..=1600.0)
                    .suffix(" pt")
                    .custom_formatter(|v, _| {
                        if v < 1.0 {
                            "full width".to_string()
                        } else {
                            format!("{v:.0} pt")
                        }
                    }),
            )
            .on_hover_text(
                "Line length of the reading column. 0 uses the whole window. \
                 Shorter lines are easier to follow while they move.",
            );
        });

        ui.checkbox(
            &mut c.pause_on_scroll,
            RichText::new("Scrolling by hand pauses the crawl").size(12.5),
        );
        if c.pause_on_scroll {
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new("Resume after")
                        .size(12.5)
                        .color(theme::text_dim()),
                );
                ui.add(
                    egui::Slider::new(&mut c.resume_after_seconds, 0.0..=60.0)
                        .suffix(" s")
                        .custom_formatter(|v, _| {
                            if v < 0.5 {
                                "stay paused".to_string()
                            } else {
                                format!("{v:.0} s")
                            }
                        }),
                )
                .on_hover_text(
                    "How long to wait after you stop scrolling before the crawl picks \
                     up again on its own. Set it to 0 to stay paused until you press \
                     Space instead.",
                );
            });
        }

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("At the end")
                    .size(13.0)
                    .color(theme::text_primary()),
            );
            for (value, label, hint) in [
                (
                    "reverse",
                    "Turn around",
                    "Reach an end and keep going the other way. Reading never \
                     comes to a dead stop.",
                ),
                (
                    "loop",
                    "Start over",
                    "Jump back to the other end and continue.",
                ),
                (
                    "stop",
                    "Stop",
                    "Park at the end. Space or R starts it moving again.",
                ),
            ] {
                let mut selected = c.end_action == value;
                if ui
                    .radio_value(&mut selected, true, RichText::new(label).size(12.5))
                    .on_hover_text(hint)
                    .clicked()
                {
                    c.end_action = value.to_string();
                }
            }
        });
        ui.checkbox(
            &mut c.fade_edges,
            RichText::new("Fade the top and bottom edges").size(12.5),
        )
        .on_hover_text("Lines arrive and leave softly instead of being cut off.");
        ui.checkbox(
            &mut c.fullscreen,
            RichText::new("Go fullscreen while crawling").size(12.5),
        );
        ui.checkbox(
            &mut c.show_hud,
            RichText::new("Show control hints when something changes").size(12.5),
        );
    }

    /// Which key secures unsaved edits across a lock.
    fn stash_key_controls(&mut self, ui: &mut egui::Ui) {
        // Imported age recipients, copied out before the &mut borrow of
        // the draft below.
        let age_recipients: Vec<(String, String)> = self
            .draft
            .age_recipients
            .iter()
            .map(|r| (r.label.clone(), r.recipient.clone()))
            .collect();
        let sk = &mut self.draft.security.stash_key;

        let mut use_fixed = sk.use_fixed;
        ui.radio_value(
            &mut use_fixed,
            false,
            RichText::new("Use each document's own key (recommended)").size(12.5),
        )
        .on_hover_text(
            "Unsaved edits are encrypted to the same key the document itself uses, \
             so getting them back needs exactly the credential that opens that \
             document — nothing new to remember, and nothing else can read them.",
        );
        ui.radio_value(
            &mut use_fixed,
            true,
            RichText::new("Always use one specific key, for every file").size(12.5),
        )
        .on_hover_text(
            "Every document's unsaved edits are encrypted to the key below instead. \
             Useful if you would rather recover in-progress work with a single \
             credential — an AGE seed phrase, say — without reaching for a hardware \
             key. Note this key can then read the in-progress text of any file you \
             edit, so pick one you trust as much as the documents themselves.",
        );
        sk.use_fixed = use_fixed;

        if !sk.use_fixed {
            return;
        }

        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.label(
                RichText::new("\u{1F511} Key")
                    .size(12.0)
                    .color(theme::text_dim()),
            );
            let selected = if !sk.key_label.trim().is_empty() {
                sk.key_label.clone()
            } else {
                "Choose a key\u{2026}".to_string()
            };
            egui::ComboBox::from_id_salt("settings_stash_key")
                .selected_text(RichText::new(selected).size(12.5))
                .width(260.0)
                .show_ui(ui, |ui| {
                    // Keys are read here rather than cached: the dialog is
                    // open rarely, and a key imported since launch should
                    // appear without a restart.
                    for k in crate::crypto::keys::list_public_keys().unwrap_or_default() {
                        let text = format!("{}  ({})", k.uid, short_fpr(&k.fingerprint));
                        let is_sel = sk.age_recipient.is_empty()
                            && sk.key_fingerprint.eq_ignore_ascii_case(&k.fingerprint);
                        if ui
                            .selectable_label(is_sel, RichText::new(text).size(12.5))
                            .clicked()
                        {
                            sk.key_fingerprint = k.fingerprint.clone();
                            sk.age_recipient.clear();
                            sk.key_label = k.uid.clone();
                        }
                    }
                    if !age_recipients.is_empty() {
                        ui.separator();
                        ui.label(
                            RichText::new("AGE keys")
                                .size(11.0)
                                .color(theme::text_dim()),
                        );
                        for (label, recipient) in &age_recipients {
                            let short = if recipient.len() > 14 {
                                format!("{}\u{2026}", &recipient[..14])
                            } else {
                                recipient.clone()
                            };
                            let is_sel = sk.age_recipient == *recipient;
                            if ui
                                .selectable_label(
                                    is_sel,
                                    RichText::new(format!("{label}  (AGE: {short})")).size(12.5),
                                )
                                .clicked()
                            {
                                sk.age_recipient = recipient.clone();
                                sk.key_fingerprint.clear();
                                sk.key_label = format!("{label} (AGE)");
                            }
                        }
                    }
                });
        });

        if self.draft.security.stash_key.is_incomplete() {
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                ui.label(
                    RichText::new(
                        "\u{26A0} No key chosen \u{2014} until you pick one, each \
                         document's own key is used.",
                    )
                    .size(11.0)
                    .color(theme::accent_yellow()),
                );
            });
        }
    }

    /// Apply a capture result to whichever slot is capturing.
    fn apply_capture(&mut self, result: CaptureResult) {
        let slot = self.capturing.clone();
        match result {
            CaptureResult::Cancel => {}
            CaptureResult::Clear => match slot {
                Capturing::InApp(idx) => {
                    if let Some(b) = bindings().get(idx) {
                        (b.set)(&mut self.draft, String::new());
                    }
                }
                Capturing::Global => { /* the global hotkey can't be empty */ }
                Capturing::None => {}
            },
            CaptureResult::Combo(combo) => {
                let spec = combo.to_config_string();
                match slot {
                    Capturing::Global => {
                        // A system-wide hotkey needs a modifier and must be
                        // registrable.
                        if !combo.has_modifier() {
                            self.error = Some("Global hotkey needs a modifier".into());
                            self.capturing = Capturing::None;
                            return;
                        }
                        if let Err(e) = crate::hotkey::parse(&spec) {
                            self.error = Some(format!("Invalid hotkey: {e}"));
                            self.capturing = Capturing::None;
                            return;
                        }
                        self.draft.quick_note.hotkey = spec;
                    }
                    Capturing::InApp(idx) => {
                        if let Some(dup) = self.conflict(&spec, idx) {
                            self.error = Some(format!("Already bound to “{dup}”"));
                        } else if let Some(b) = bindings().get(idx) {
                            (b.set)(&mut self.draft, spec);
                            self.error = None;
                        }
                    }
                    Capturing::None => {}
                }
            }
        }
        self.capturing = Capturing::None;
    }

    /// If `spec` is already used by a different in-app binding, return its
    /// label.
    fn conflict(&self, spec: &str, except: usize) -> Option<String> {
        let defs = bindings();
        for (i, b) in defs.iter().enumerate() {
            if i != except && (b.get)(&self.draft) == spec {
                return Some(b.label.to_string());
            }
        }
        None
    }
}

enum CaptureResult {
    Cancel,
    Clear,
    Combo(KeyCombo),
}

fn is_modifier_key(key: egui::Key) -> bool {
    // egui doesn't emit Key events for bare modifiers, but guard anyway.
    matches!(
        key,
        egui::Key::Escape | egui::Key::Backspace | egui::Key::Delete
    )
}

/// Render a config combo string as a glyph label, falling back to the raw
/// string if it doesn't parse.
fn display_combo(spec: &str) -> String {
    KeyCombo::parse(spec)
        .map(|c| c.display())
        .unwrap_or_else(|| spec.to_string())
}
