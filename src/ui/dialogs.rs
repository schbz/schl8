use egui::{self, Align2, RichText, Vec2};

use super::theme;
use crate::crypto::keys::{self, PublicKey};

// ── Encrypt Dialog ───────────────────────────────────────────────────────────

/// Which encryption backend the "Encrypt & Save As" dialog will use.
#[derive(PartialEq, Clone, Copy)]
pub enum EncryptBackend {
    Gpg,
    Age,
}

/// State for the "Encrypt & Save As" dialog.
pub struct EncryptDialog {
    pub open: bool,
    pub available_keys: Vec<PublicKey>,
    pub selected: Vec<bool>,
    pub status_message: Option<(String, bool)>, // (message, is_error)
    /// true = ASCII armor (.asc), false = binary (.gpg)
    pub use_armor: bool,
    /// Chosen encryption method.
    backend: EncryptBackend,
    /// Available age recipients as (label, `age1…`), incl. own identity.
    age_recipients: Vec<(String, String)>,
    age_selected: Vec<bool>,
    /// Whether GPG is available. When false the dialog is age-only.
    gpg_available: bool,
}

impl EncryptDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            available_keys: Vec::new(),
            selected: Vec::new(),
            status_message: None,
            use_armor: false,
            backend: EncryptBackend::Gpg,
            age_recipients: Vec::new(),
            age_selected: Vec::new(),
            gpg_available: true,
        }
    }

    /// Open the dialog. `default_armor` seeds the GPG format; `age_recips`
    /// are the available age recipients (label, `age1…`); `default_age`
    /// pre-selects age (e.g. when re-saving an existing age file);
    /// `gpg_available` false forces age-only mode.
    pub fn show_with_format(
        &mut self,
        default_armor: bool,
        age_recips: Vec<(String, String)>,
        default_age: bool,
        gpg_available: bool,
    ) {
        self.open = true;
        self.use_armor = default_armor;
        self.status_message = None;
        self.age_selected = vec![false; age_recips.len()];
        self.age_recipients = age_recips;
        self.gpg_available = gpg_available;
        self.backend = if default_age || !gpg_available {
            EncryptBackend::Age
        } else {
            EncryptBackend::Gpg
        };
        self.refresh_keys();
    }

    pub fn refresh_keys(&mut self) {
        match keys::list_public_keys() {
            Ok(key_list) => {
                self.selected = vec![false; key_list.len()];
                self.available_keys = key_list;
            }
            Err(_e) => {
                // GPG may be unavailable (age-only machine) — not an error
                // here; the user can still choose the age backend.
                self.available_keys.clear();
                self.selected.clear();
            }
        }
    }

    /// Get the fingerprints of all selected GPG recipients.
    pub fn selected_fingerprints(&self) -> Vec<String> {
        self.available_keys
            .iter()
            .zip(self.selected.iter())
            .filter(|(_, &sel)| sel)
            .map(|(k, _)| k.fingerprint.clone())
            .collect()
    }

    /// The `age1…` strings of all selected age recipients.
    pub fn selected_age_recipients(&self) -> Vec<String> {
        self.age_recipients
            .iter()
            .zip(self.age_selected.iter())
            .filter(|(_, &sel)| sel)
            .map(|((_, r), _)| r.clone())
            .collect()
    }

    /// Whether the age backend is selected.
    pub fn is_age(&self) -> bool {
        self.backend == EncryptBackend::Age
    }

    pub fn any_selected(&self) -> bool {
        match self.backend {
            EncryptBackend::Gpg => self.selected.iter().any(|&s| s),
            EncryptBackend::Age => self.age_selected.iter().any(|&s| s),
        }
    }

    /// Render the dialog. Returns true if the user clicked "Encrypt & Save".
    pub fn render(&mut self, ctx: &egui::Context) -> bool {
        let mut do_encrypt = false;
        let mut close = false;

        if !self.open {
            return false;
        }

        let mut is_open = self.open;

        egui::Window::new("Encrypt & Save As")
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(true)
            .default_size([500.0, 400.0])
            .collapsible(false)
            .frame(egui::Frame::window(&ctx.style()).fill(theme::bg_primary()))
            .show(ctx, |ui| {
                ui.visuals_mut().widgets.noninteractive.fg_stroke.color = theme::text_primary();
                ui.spacing_mut().item_spacing.y = 8.0;

                // ── Encryption method ────────────────────────────────
                // With GPG unavailable the app is age-only, so the choice
                // is fixed and the selector is hidden.
                if self.gpg_available {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Encryption method:")
                                .size(13.0)
                                .color(theme::text_primary()),
                        );
                        ui.add_space(8.0);
                        ui.radio_value(
                            &mut self.backend,
                            EncryptBackend::Gpg,
                            RichText::new("GPG / YubiKey").size(13.0).color(theme::text_primary()),
                        );
                        ui.add_space(4.0);
                        ui.radio_value(
                            &mut self.backend,
                            EncryptBackend::Age,
                            RichText::new("AGE (seed phrase)")
                                .size(13.0)
                                .color(theme::text_primary()),
                        );
                    });
                } else {
                    ui.label(
                        RichText::new("Encryption method: AGE (seed phrase) — GPG not installed")
                            .size(12.5)
                            .color(theme::text_dim()),
                    );
                }
                ui.separator();

                ui.label(
                    RichText::new("Select recipients to encrypt for:")
                        .size(14.0)
                        .color(theme::text_strong())
                        .strong(),
                );
                ui.add_space(4.0);

                match self.backend {
                    EncryptBackend::Gpg => {
                        if self.available_keys.is_empty() {
                            ui.label(
                                RichText::new(
                                    "No GPG public keys found.\nUse Import GPG Key… or switch to AGE.",
                                )
                                .size(13.0)
                                .color(theme::text_dim()),
                            );
                        } else {
                            egui::ScrollArea::vertical()
                                .id_salt("gpg_recips")
                                .max_height(220.0)
                                .show(ui, |ui| {
                                    for (i, key) in self.available_keys.iter().enumerate() {
                                        let mut checked = self.selected[i];
                                        ui.horizontal(|ui| {
                                            ui.checkbox(&mut checked, "");
                                            ui.vertical(|ui| {
                                                let uid = RichText::new(&key.uid).size(13.0);
                                                let uid = if key.valid {
                                                    uid.color(theme::text_strong())
                                                } else {
                                                    uid.color(theme::text_dim()).strikethrough()
                                                };
                                                ui.label(uid);
                                                let fp_short = if key.fingerprint.len() >= 16 {
                                                    &key.fingerprint[key.fingerprint.len() - 16..]
                                                } else {
                                                    &key.fingerprint
                                                };
                                                ui.label(
                                                    RichText::new(fp_short)
                                                        .size(11.0)
                                                        .color(theme::accent())
                                                        .monospace(),
                                                );
                                            });
                                        });
                                        self.selected[i] = checked;
                                    }
                                });
                        }
                    }
                    EncryptBackend::Age => {
                        if self.age_recipients.is_empty() {
                            ui.label(
                                RichText::new(
                                    "No age recipients available.\nUnlock your identity or add \
                                     an age key under Manage Public Keys.",
                                )
                                .size(13.0)
                                .color(theme::text_dim()),
                            );
                        } else {
                            egui::ScrollArea::vertical()
                                .id_salt("age_recips_encrypt")
                                .max_height(220.0)
                                .show(ui, |ui| {
                                    for (i, (label, recipient)) in
                                        self.age_recipients.iter().enumerate()
                                    {
                                        let mut checked = self.age_selected[i];
                                        ui.horizontal(|ui| {
                                            ui.checkbox(&mut checked, "");
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    RichText::new(label)
                                                        .size(13.0)
                                                        .color(theme::text_strong()),
                                                );
                                                ui.label(
                                                    RichText::new(recipient)
                                                        .size(10.5)
                                                        .color(theme::accent())
                                                        .monospace(),
                                                );
                                            });
                                        });
                                        self.age_selected[i] = checked;
                                    }
                                });
                        }
                    }
                }

                ui.add_space(4.0);
                ui.separator();

                // Output format selector — GPG only (age has one format).
                if self.backend == EncryptBackend::Gpg {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Output format:")
                                .size(13.0)
                                .color(theme::text_primary()),
                        );
                        ui.add_space(8.0);
                        ui.radio_value(
                            &mut self.use_armor,
                            false,
                            RichText::new(".gpg (binary)").size(13.0).color(theme::text_primary()),
                        );
                        ui.add_space(4.0);
                        ui.radio_value(
                            &mut self.use_armor,
                            true,
                            RichText::new(".asc (ASCII armor)")
                                .size(13.0)
                                .color(theme::text_primary()),
                        );
                    });
                } else {
                    ui.label(
                        RichText::new("Output: .age")
                            .size(12.0)
                            .color(theme::text_dim()),
                    );
                }

                ui.add_space(4.0);

                // Status message
                if let Some((msg, is_error)) = &self.status_message {
                    let color = if *is_error {
                        super::theme::accent_red()
                    } else {
                        super::theme::accent_green()
                    };
                    ui.label(RichText::new(msg.as_str()).size(12.0).color(color));
                    ui.add_space(4.0);
                }

                ui.separator();

                ui.horizontal(|ui| {
                    let can_encrypt = self.any_selected();

                    let encrypt_btn = egui::Button::new(
                        RichText::new("  Encrypt & Save  ")
                            .size(14.0)
                            .color(theme::badge_text()),
                    )
                    .fill(theme::badge_bg())
                    .corner_radius(4.0);

                    if ui.add_enabled(can_encrypt, encrypt_btn).clicked() {
                        do_encrypt = true;
                    }

                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        self.open = is_open && !close;
        do_encrypt
    }
}

// ── Key Manager Dialog ───────────────────────────────────────────────────────

/// State for the key management dialog.
/// What the app should do after the key manager renders this frame.
pub enum KeyManagerAction {
    None,
    /// Open a file picker to import a GPG public key.
    ImportGpgFile,
    /// Store a new age recipient (public key + name).
    AddAge {
        label: String,
        recipient: String,
    },
    /// Remove the age recipient with this `age1…` string.
    DeleteAge(String),
    /// Open the generate-new-age-key dialog.
    GenerateAge,
}

pub struct KeyManagerDialog {
    pub open: bool,
    pub keys: Vec<PublicKey>,
    pub status_message: Option<(String, bool)>,
    pub confirm_delete: Option<usize>,
    /// Inputs for the "add AGE public key" form.
    new_age_label: String,
    new_age_recipient: String,
}

impl KeyManagerDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            keys: Vec::new(),
            status_message: None,
            confirm_delete: None,
            new_age_label: String::new(),
            new_age_recipient: String::new(),
        }
    }

    pub fn show(&mut self) {
        self.open = true;
        self.status_message = None;
        self.confirm_delete = None;
        self.refresh_keys();
    }

    pub fn refresh_keys(&mut self) {
        match keys::list_public_keys() {
            Ok(key_list) => {
                self.keys = key_list;
            }
            Err(e) => {
                self.keys.clear();
                self.status_message = Some((format!("Failed to list keys: {e}"), true));
            }
        }
    }

    /// Render the key manager dialog. Returns true if a key import from file was requested.
    pub fn render(
        &mut self,
        ctx: &egui::Context,
        age_recipients: &[crate::config::AgeRecipient],
        gpg_available: bool,
    ) -> KeyManagerAction {
        let mut action = KeyManagerAction::None;
        let mut close = false;

        if !self.open {
            return KeyManagerAction::None;
        }

        let mut is_open = self.open;

        egui::Window::new("Manage Public Keys")
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(true)
            .default_size([560.0, 420.0])
            .collapsible(false)
            .frame(egui::Frame::window(&ctx.style()).fill(theme::bg_primary()))
            .show(ctx, |ui| {
                ui.visuals_mut().widgets.noninteractive.fg_stroke.color = theme::text_primary();
                ui.spacing_mut().item_spacing.y = 6.0;

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Public keys in your GPG keyring:")
                            .size(14.0)
                            .color(theme::text_primary()),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(RichText::new("\u{21BB} Refresh").size(12.0))
                            .clicked()
                        {
                            self.refresh_keys();
                        }
                    });
                });

                ui.add_space(4.0);

                if self.keys.is_empty() {
                    ui.label(
                        RichText::new(if gpg_available {
                            "No public keys found."
                        } else {
                            "GPG is not installed — running in AGE-only mode. \
                             Use the AGE public keys below."
                        })
                        .size(13.0)
                        .color(theme::text_dim()),
                    );
                } else {
                    egui::ScrollArea::vertical()
                        .max_height(280.0)
                        .show(ui, |ui| {
                            let mut delete_idx = None;

                            for (i, key) in self.keys.iter().enumerate() {
                                egui::Frame::NONE
                                    .fill(super::theme::bg_raised())
                                    .corner_radius(4.0)
                                    .inner_margin(8.0)
                                    .outer_margin(egui::Margin::symmetric(0, 2))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.vertical(|ui| {
                                                let uid_text = RichText::new(&key.uid)
                                                    .size(13.0)
                                                    .color(theme::text_primary());
                                                ui.label(uid_text);

                                                let fp_short = if key.fingerprint.len() >= 16 {
                                                    &key.fingerprint[key.fingerprint.len() - 16..]
                                                } else {
                                                    &key.fingerprint
                                                };
                                                ui.label(
                                                    RichText::new(fp_short)
                                                        .size(11.0)
                                                        .color(theme::text_dim())
                                                        .monospace(),
                                                );

                                                if !key.valid {
                                                    ui.label(
                                                        RichText::new("expired / revoked")
                                                            .size(11.0)
                                                            .color(theme::accent_yellow()),
                                                    );
                                                }
                                            });

                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if self.confirm_delete == Some(i) {
                                                        if ui
                                                            .button(
                                                                RichText::new("Confirm Delete")
                                                                    .size(12.0)
                                                                    .color(
                                                                        super::theme::accent_red(),
                                                                    ),
                                                            )
                                                            .clicked()
                                                        {
                                                            delete_idx = Some(i);
                                                        }
                                                        if ui
                                                            .button(
                                                                RichText::new("Cancel").size(12.0),
                                                            )
                                                            .clicked()
                                                        {
                                                            self.confirm_delete = None;
                                                        }
                                                    } else if ui
                                                        .button(RichText::new("Delete").size(12.0))
                                                        .clicked()
                                                    {
                                                        self.confirm_delete = Some(i);
                                                    }
                                                },
                                            );
                                        });
                                    });
                            }

                            if let Some(idx) = delete_idx {
                                let fp = self.keys[idx].fingerprint.clone();
                                match keys::delete_key(&fp) {
                                    Ok(()) => {
                                        self.status_message =
                                            Some(("Key deleted.".to_string(), false));
                                        self.confirm_delete = None;
                                        self.refresh_keys();
                                    }
                                    Err(e) => {
                                        self.status_message =
                                            Some((format!("Delete failed: {e}"), true));
                                    }
                                }
                            }
                        });
                }

                ui.add_space(6.0);
                ui.separator();

                // ── age recipient public keys ────────────────────────
                ui.label(
                    RichText::new("AGE public keys (seed-phrase encryption):")
                        .size(14.0)
                        .color(theme::text_primary()),
                );
                ui.add_space(2.0);
                if age_recipients.is_empty() {
                    ui.label(
                        RichText::new("No AGE recipients added yet.")
                            .size(12.5)
                            .color(theme::text_dim()),
                    );
                } else {
                    let mut delete_recipient: Option<String> = None;
                    egui::ScrollArea::vertical()
                        .id_salt("age_recipients")
                        .max_height(150.0)
                        .show(ui, |ui| {
                            for r in age_recipients {
                                egui::Frame::NONE
                                    .fill(super::theme::bg_raised())
                                    .corner_radius(4.0)
                                    .inner_margin(8.0)
                                    .outer_margin(egui::Margin::symmetric(0, 2))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    RichText::new(&r.label)
                                                        .size(13.0)
                                                        .color(theme::text_primary()),
                                                );
                                                ui.label(
                                                    RichText::new(&r.recipient)
                                                        .size(10.5)
                                                        .color(theme::text_dim())
                                                        .monospace(),
                                                );
                                                let added =
                                                    chrono::DateTime::parse_from_rfc3339(&r.added)
                                                        .map(|t| {
                                                            t.with_timezone(&chrono::Local)
                                                                .format("added %Y-%m-%d")
                                                                .to_string()
                                                        })
                                                        .unwrap_or_default();
                                                ui.label(
                                                    RichText::new(added)
                                                        .size(10.5)
                                                        .color(theme::text_dim()),
                                                );
                                            });
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    if ui
                                                        .button(RichText::new("Delete").size(12.0))
                                                        .clicked()
                                                    {
                                                        delete_recipient =
                                                            Some(r.recipient.clone());
                                                    }
                                                },
                                            );
                                        });
                                    });
                            }
                        });
                    if let Some(r) = delete_recipient {
                        action = KeyManagerAction::DeleteAge(r);
                    }
                }

                // Add-an-age-key form.
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_age_label)
                            .desired_width(120.0)
                            .hint_text("Name"),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_age_recipient)
                            .desired_width(240.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("age1…"),
                    );
                    if ui.button("Add AGE key").clicked() {
                        let recipient = self.new_age_recipient.trim().to_string();
                        match crate::crypto::age_backend::validate_recipient(&recipient) {
                            Ok(()) => {
                                action = KeyManagerAction::AddAge {
                                    label: self.new_age_label.trim().to_string(),
                                    recipient,
                                };
                                self.new_age_label.clear();
                                self.new_age_recipient.clear();
                            }
                            Err(e) => {
                                self.status_message = Some((format!("{e:#}"), true));
                            }
                        }
                    }
                });

                ui.add_space(4.0);
                if ui
                    .button("Generate new AGE key…")
                    .on_hover_text(
                        "Create a brand-new seed-phrase key from secure randomness \
                         (with an optional entropy contribution of your own)",
                    )
                    .clicked()
                {
                    action = KeyManagerAction::GenerateAge;
                }

                ui.add_space(4.0);

                // Status message
                if let Some((msg, is_error)) = &self.status_message {
                    let color = if *is_error {
                        super::theme::accent_red()
                    } else {
                        super::theme::accent_green()
                    };
                    ui.label(RichText::new(msg.as_str()).size(12.0).color(color));
                }

                ui.separator();

                ui.horizontal(|ui| {
                    let import_btn = egui::Button::new(
                        RichText::new("  Import GPG Key from File…  ")
                            .size(13.0)
                            .color(theme::badge_text()),
                    )
                    .fill(theme::badge_bg())
                    .corner_radius(4.0);

                    if ui
                        .add_enabled(gpg_available, import_btn)
                        .on_hover_text(if gpg_available {
                            ""
                        } else {
                            "GPG is not installed — Schl8 is running in AGE-only mode"
                        })
                        .clicked()
                    {
                        action = KeyManagerAction::ImportGpgFile;
                    }

                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });

        self.open = is_open && !close;
        action
    }
}

// ── About Dialog ─────────────────────────────────────────────────────────────

pub struct AboutDialog {
    pub open: bool,
}

impl AboutDialog {
    pub fn new() -> Self {
        Self { open: false }
    }

    /// Returns a URL to open when the user clicks a link.
    pub fn render(&mut self, ctx: &egui::Context) -> Option<String> {
        let mut open_url = None;
        if !self.open {
            return None;
        }

        egui::Window::new("About Schl8")
            .open(&mut self.open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .fixed_size([320.0, 260.0])
            .frame(egui::Frame::window(&ctx.style()).fill(theme::bg_primary()))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(12.0);

                    // Load cached icon texture for about dialog (avoid nested ctx locks)
                    let texture: egui::TextureHandle = {
                        let cached: Option<egui::TextureHandle> = ctx.memory(|mem| {
                            mem.data.get_temp(egui::Id::new("schl8_icon_tex"))
                        });
                        if let Some(t) = cached {
                            t
                        } else {
                            let png_bytes = include_bytes!(
                                "../../assets/schl8.iconset/icon_128x128.png"
                            );
                            let img = image::load_from_memory(png_bytes)
                                .expect("embedded PNG is valid");
                            let rgba = img.to_rgba8();
                            let (w, h) = rgba.dimensions();
                            let ci = egui::ColorImage::from_rgba_unmultiplied(
                                [w as usize, h as usize],
                                rgba.as_raw(),
                            );
                            let tex = ctx.load_texture(
                                "schl8-icon",
                                ci,
                                egui::TextureOptions::LINEAR,
                            );
                            ctx.memory_mut(|mem| {
                                mem.data.insert_temp(
                                    egui::Id::new("schl8_icon_tex"),
                                    tex.clone(),
                                );
                            });
                            tex
                        }
                    };
                    ui.add(
                        egui::Image::new(&texture)
                            .fit_to_exact_size(egui::vec2(64.0, 64.0)),
                    );

                    ui.add_space(8.0);

                    ui.label(
                        RichText::new("Schl8")
                            .size(24.0)
                            .color(theme::text_primary())
                            .strong(),
                    );

                    // The version doubles as the changelog entry point.
                    if ui
                        .link(
                            RichText::new(format!("Version {} — changelog", env!("CARGO_PKG_VERSION")))
                                .size(12.0),
                        )
                        .on_hover_text("Open the changelog on GitHub")
                        .clicked()
                    {
                        open_url = Some(crate::update::changelog_url());
                    }

                    ui.add_space(12.0);

                    ui.label(
                        RichText::new("Schuyler's Lightweight\nArmored Text Editor")
                            .size(13.0)
                            .color(theme::accent()),
                    );

                    ui.add_space(8.0);

                    ui.label(
                        RichText::new(
                            "Secure viewer and encryptor for GPG- and\nAGE-encrypted files, with hardware security\nkey support (YubiKey tested).",
                        )
                        .size(12.0)
                        .color(theme::text_dim()),
                    );

                    ui.add_space(12.0);

                    ui.label(
                        RichText::new("\u{00A9} 2026 Schuyler J Sloane")
                            .size(11.0)
                            .color(theme::text_dim()),
                    );

                    ui.label(
                        RichText::new("MIT License")
                            .size(11.0)
                            .color(theme::text_dim()),
                    );
                });
            });
        open_url
    }
}

// ── Install / Default-Editor Help Dialog ─────────────────────────────────────

pub struct InstallHelpDialog {
    pub open: bool,
    /// Result of the last "make default" attempt, shown in the dialog.
    status: Option<(String, bool)>,
}

impl InstallHelpDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            status: None,
        }
    }

    pub fn render(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }

        egui::Window::new("Install & Default Editor")
            .open(&mut self.open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .default_width(theme::dialog_width(ctx, 480.0))
            .max_width(theme::dialog_max_width(ctx))
            .frame(egui::Frame::window(&ctx.style()).fill(theme::bg_primary()))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;

                let section = |ui: &mut egui::Ui, title: &str| {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(title)
                            .size(14.0)
                            .color(theme::text_primary())
                            .strong(),
                    );
                };
                let body = |ui: &mut egui::Ui, text: &str| {
                    ui.label(RichText::new(text).size(13.0).color(theme::text_dim()));
                };
                let mono = |ui: &mut egui::Ui, text: &str| {
                    ui.label(
                        RichText::new(text)
                            .size(12.0)
                            .color(theme::accent())
                            .monospace(),
                    );
                };

                section(ui, "Install as a macOS app");
                body(
                    ui,
                    "From the project folder, build and install the app bundle:",
                );
                mono(ui, "./scripts/bundle.sh --install");
                body(
                    ui,
                    "Schl8 then appears in /Applications, in Spotlight, and in \
                     Finder's \u{201C}Open With\u{201D} menu for .gpg, .asc, .pgp, \
                     .txt, and .md files.",
                );

                section(ui, "Make Schl8 the default editor");
                body(
                    ui,
                    "One click registers Schl8 as the default app for every \
                     type it handles \u{2014} encrypted files (.gpg, .pgp, .asc, \
                     including encrypted folder archives), markdown (.md), and \
                     plain text (.txt). Requires the installed app bundle.",
                );
                ui.horizontal(|ui| {
                    let btn = egui::Button::new(
                        RichText::new("  Make Schl8 the default  ")
                            .size(13.0)
                            .color(theme::badge_text())
                            .strong(),
                    )
                    .fill(theme::badge_bg())
                    .corner_radius(theme::RADIUS);
                    if ui.add(btn).clicked() {
                        let failures = crate::macos_default_app::set_default_for_all();
                        self.status = Some(if failures.is_empty() {
                            (
                                "Schl8 is now the default for .gpg, .pgp, .asc, .md, and .txt"
                                    .to_string(),
                                false,
                            )
                        } else {
                            (failures.join("  ·  "), true)
                        });
                    }
                    ui.label(
                        RichText::new("(reversible in Finder -> Get Info)")
                            .size(11.0)
                            .color(theme::text_dim()),
                    );
                });
                if let Some((msg, is_error)) = &self.status {
                    ui.label(RichText::new(msg.as_str()).size(12.0).color(if *is_error {
                        theme::accent_red()
                    } else {
                        theme::accent_green()
                    }));
                }
                body(
                    ui,
                    "Or do it per type in Finder: select a file -> Get Info \
                     (\u{2318}I) -> \u{201C}Open with:\u{201D} -> \
                     choose Schl8 -> \u{201C}Change All\u{2026}\u{201D}.",
                );

                section(ui, "Quick-note hotkey");
                body(
                    ui,
                    "While Schl8 runs in the menu bar, press the global hotkey \
                     to jot a note from anywhere (Enter appends, Shift+Enter for \
                     a newline). Configure the combo and note templates in:",
                );
                mono(ui, "~/.config/schl8/config.toml");

                section(ui, "Stop repeated folder-access prompts");
                body(
                    ui,
                    "If macOS asks to allow writing to Desktop/Documents every time \
                     you rebuild Schl8, it's because the ad-hoc build changes its \
                     signature each time and macOS forgets the grant. Sign with a \
                     stable self-signed certificate once so the grant sticks:",
                );
                mono(ui, "./scripts/setup-signing.sh   # once");
                mono(ui, "./scripts/bundle.sh --install # then rebuild");
            });
    }
}

// ── Quit Confirmation Dialog ─────────────────────────────────────────────────

/// Shown when the user tries to quit or close the window with unsaved edits.
pub struct QuitDialog {
    pub open: bool,
}

impl QuitDialog {
    pub fn new() -> Self {
        Self { open: false }
    }

    /// Render the confirmation dialog.
    /// Returns true if the user confirmed they want to quit without saving.
    pub fn render(&mut self, ctx: &egui::Context) -> bool {
        let mut confirmed = false;
        let mut close = false;

        if !self.open {
            return false;
        }

        let mut is_open = self.open;

        egui::Window::new("Quit Without Saving?")
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .fixed_size([340.0, 130.0])
            .frame(egui::Frame::window(&ctx.style()).fill(theme::bg_primary()))
            .show(ctx, |ui| {
                ui.add_space(8.0);

                ui.label(
                    RichText::new("Your edits have not been saved.")
                        .size(14.0)
                        .color(theme::text_primary()),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Quitting will permanently delete all changes.")
                        .size(13.0)
                        .color(theme::text_dim()),
                );

                ui.add_space(16.0);

                ui.horizontal(|ui| {
                    let quit_btn = egui::Button::new(
                        RichText::new("  Quit Without Saving  ")
                            .size(14.0)
                            .color(theme::badge_text()),
                    )
                    .fill(theme::accent_red())
                    .corner_radius(4.0);

                    if ui.add(quit_btn).clicked() {
                        confirmed = true;
                        close = true;
                    }

                    ui.add_space(8.0);

                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        self.open = is_open && !close;
        confirmed
    }
}

// ── Discard Confirmation Dialog ──────────────────────────────────────────────

pub struct DiscardDialog {
    pub open: bool,
}

impl DiscardDialog {
    pub fn new() -> Self {
        Self { open: false }
    }

    /// Render the confirmation dialog.
    /// Returns true if the user confirmed they want to discard edits.
    pub fn render(&mut self, ctx: &egui::Context) -> bool {
        let mut confirmed = false;
        let mut close = false;

        if !self.open {
            return false;
        }

        let mut is_open = self.open;

        egui::Window::new("Discard Edits?")
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .fixed_size([340.0, 130.0])
            .frame(egui::Frame::window(&ctx.style()).fill(theme::bg_primary()))
            .show(ctx, |ui| {
                ui.add_space(8.0);

                ui.label(
                    RichText::new("Your edits have not been saved.")
                        .size(14.0)
                        .color(theme::text_primary()),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Discarding will permanently delete all changes.")
                        .size(13.0)
                        .color(theme::text_dim()),
                );

                ui.add_space(16.0);

                ui.horizontal(|ui| {
                    let discard_btn = egui::Button::new(
                        RichText::new("  Discard  ")
                            .size(14.0)
                            .color(theme::badge_text()),
                    )
                    .fill(theme::accent_red())
                    .corner_radius(4.0);

                    if ui.add(discard_btn).clicked() {
                        confirmed = true;
                        close = true;
                    }

                    ui.add_space(8.0);

                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        self.open = is_open && !close;
        confirmed
    }
}

// ── Copy-Enable Security Warning ─────────────────────────────────────────────

/// The action a user chose in the copy-enable warning.
pub struct CopyChoice {
    /// Persist "copying allowed" as the startup default.
    pub remember_default: bool,
    /// Don't show this warning again.
    pub suppress_future: bool,
}

/// Warns before enabling clipboard copying (which defeats the no-clipboard
/// stance). Returns a choice when confirmed.
pub struct CopyWarningDialog {
    pub open: bool,
    remember_default: bool,
    suppress_future: bool,
}

impl CopyWarningDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            remember_default: false,
            suppress_future: false,
        }
    }

    pub fn show(&mut self) {
        self.open = true;
        self.remember_default = false;
        self.suppress_future = false;
    }

    /// Render. Returns Some(choice) when the user confirms enabling copy,
    /// None otherwise (including cancel).
    pub fn render(&mut self, ctx: &egui::Context) -> Option<CopyChoice> {
        if !self.open {
            return None;
        }
        let mut confirmed = None;
        let mut close = false;
        let mut is_open = self.open;

        egui::Window::new("Enable Copying?")
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .default_width(theme::dialog_width(ctx, 400.0))
            .max_width(theme::dialog_max_width(ctx))
            .frame(egui::Frame::window(&ctx.style()).fill(theme::bg_primary()))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.label(
                    RichText::new("\u{26A0} Security warning")
                        .size(15.0)
                        .strong()
                        .color(theme::accent_yellow()),
                );
                ui.label(
                    RichText::new(
                        "Copying places decrypted plaintext on the system clipboard, \
                         where other apps can read it and where it may be synced or \
                         retained. Schl8 blocks this by default.",
                    )
                    .size(13.0)
                    .color(theme::text_primary()),
                );
                ui.add_space(2.0);
                ui.checkbox(
                    &mut self.suppress_future,
                    RichText::new("Don't warn me again").size(12.0),
                );
                ui.checkbox(
                    &mut self.remember_default,
                    RichText::new("Allow copying by default on next launch").size(12.0),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    let enable = egui::Button::new(
                        RichText::new("  Enable Copying  ")
                            .size(13.0)
                            .color(theme::badge_text())
                            .strong(),
                    )
                    .fill(theme::accent_yellow())
                    .corner_radius(theme::RADIUS);
                    if ui.add(enable).clicked() {
                        confirmed = Some(CopyChoice {
                            remember_default: self.remember_default,
                            suppress_future: self.suppress_future,
                        });
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });

        self.open = is_open && !close;
        confirmed
    }
}

// ── Update Dialog ────────────────────────────────────────────────────────────

/// What the app should do after the update dialog renders.
pub enum UpdateAction {
    None,
    /// Open a URL in the browser.
    Open(String),
}

/// Shown when a newer release exists. Schl8 never replaces itself: it is
/// distributed un-notarized, so a silent self-update would be both hard to
/// verify and easy to spoof. Instead we spell out the two supported paths
/// (Homebrew, or download-and-replace) and let the user drive.
pub struct UpdateDialog {
    pub open: bool,
    /// Newest published version (no leading `v`).
    latest: String,
}

impl UpdateDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            latest: String::new(),
        }
    }

    pub fn show(&mut self, latest: String) {
        self.latest = latest;
        self.open = true;
    }

    pub fn render(&mut self, ctx: &egui::Context) -> UpdateAction {
        if !self.open {
            return UpdateAction::None;
        }
        let mut action = UpdateAction::None;
        let mut is_open = self.open;

        egui::Window::new("Update Available")
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .default_width(theme::dialog_width(ctx, 460.0))
            .max_width(theme::dialog_max_width(ctx))
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme::bg_primary())
                    .stroke(egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.4))),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;

                ui.label(
                    RichText::new(format!(
                        "Schl8 {} is available — you have {}.",
                        self.latest,
                        crate::update::current_version()
                    ))
                    .size(14.0)
                    .color(theme::text_strong())
                    .strong(),
                );

                if ui
                    .link(RichText::new("See what changed (changelog)").size(12.5))
                    .clicked()
                {
                    action = UpdateAction::Open(crate::update::changelog_url());
                }

                ui.separator();

                // ── Path 1: Homebrew ─────────────────────────────────
                ui.label(
                    RichText::new("If you installed with Homebrew")
                        .size(13.0)
                        .color(theme::text_primary())
                        .strong(),
                );
                ui.label(
                    RichText::new("Run this in Terminal, then reopen Schl8:")
                        .size(12.0)
                        .color(theme::text_dim()),
                );
                let mut cmd = crate::update::brew_command().to_string();
                ui.add(
                    egui::TextEdit::singleline(&mut cmd)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .interactive(false),
                );

                ui.add_space(2.0);

                // ── Path 2: manual download ──────────────────────────
                ui.label(
                    RichText::new("If you downloaded the app")
                        .size(13.0)
                        .color(theme::text_primary())
                        .strong(),
                );
                ui.label(
                    RichText::new(
                        "1. Open the release page and download the macOS .zip\n\
                         2. Unzip it and drag Schl8.app into Applications, replacing the old copy\n\
                         3. If macOS blocks it, run:  xattr -dr com.apple.quarantine /Applications/Schl8.app\n\
                         4. Quit and reopen Schl8",
                    )
                    .size(12.0)
                    .color(theme::text_dim()),
                );

                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Your notes and settings are untouched by an update — they live in your \
                         own files and ~/.config/schl8.",
                    )
                    .size(11.5)
                    .color(theme::text_dim()),
                );

                ui.separator();
                ui.horizontal(|ui| {
                    let btn = egui::Button::new(
                        RichText::new("  Open Download Page  ")
                            .size(13.0)
                            .color(theme::badge_text())
                            .strong(),
                    )
                    .fill(theme::badge_bg())
                    .corner_radius(theme::RADIUS);
                    if ui.add(btn).clicked() {
                        action = UpdateAction::Open(crate::update::latest_release_url());
                    }
                    if ui.button("Later").clicked() {
                        self.open = false;
                    }
                });
            });

        if !is_open {
            self.open = false;
        }
        action
    }
}

// ── Discard Pending (spool) Confirmation ─────────────────────────────────────

/// Confirms throwing away spooled entries.
///
/// These were written while the identity was locked, so the user cannot
/// read them back to check what they are — the dialog states the count
/// and that the loss is permanent, and defaults to keeping them.
pub struct DiscardSpoolDialog {
    pub open: bool,
    pending: usize,
}

impl DiscardSpoolDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            pending: 0,
        }
    }

    pub fn open(&mut self, pending: usize) {
        self.pending = pending;
        self.open = true;
    }

    /// Returns true when the user confirms the discard.
    pub fn render(&mut self, ctx: &egui::Context) -> bool {
        if !self.open {
            return false;
        }
        let mut confirmed = false;
        let mut is_open = self.open;
        let n = self.pending;

        egui::Window::new("Discard Pending Entries")
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .default_width(theme::dialog_width(ctx, 430.0))
            .max_width(theme::dialog_max_width(ctx))
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme::bg_primary())
                    .stroke(egui::Stroke::new(
                        1.0,
                        theme::accent_red().gamma_multiply(0.5),
                    )),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.label(
                    RichText::new(format!(
                        "Permanently delete {n} offline entr{}?",
                        if n == 1 { "y" } else { "ies" }
                    ))
                    .size(14.5)
                    .color(theme::text_strong())
                    .strong(),
                );
                ui.label(
                    RichText::new(
                        "These were written while your AGE identity was locked and have not \
                         been merged into any note yet. They are encrypted, so you cannot \
                         read them to check what they contain first.",
                    )
                    .size(12.5)
                    .color(theme::text_dim()),
                );
                ui.label(
                    RichText::new("This cannot be undone.")
                        .size(12.5)
                        .color(theme::accent_red()),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    // Keeping is the safe default, so it leads.
                    let keep = egui::Button::new(
                        RichText::new("  Keep Them  ")
                            .size(13.0)
                            .color(theme::badge_text())
                            .strong(),
                    )
                    .fill(theme::badge_bg())
                    .corner_radius(theme::RADIUS);
                    if ui.add(keep).clicked() {
                        self.open = false;
                    }
                    let bg = theme::accent_red().gamma_multiply(0.9);
                    let del = egui::Button::new(
                        RichText::new("  Delete Permanently  ")
                            .size(13.0)
                            .color(theme::contrast_text(bg)),
                    )
                    .fill(bg)
                    .corner_radius(theme::RADIUS);
                    if ui.add(del).clicked() {
                        confirmed = true;
                        self.open = false;
                    }
                });
            });

        if !is_open {
            self.open = false;
        }
        confirmed
    }
}

// ── Vault entry prompt (add / rename a file in a folder archive) ─────────────

/// What the vault prompt is collecting.
#[derive(Clone, Copy, PartialEq)]
pub enum VaultPromptMode {
    AddFile,
    AddFolder,
    Rename,
}

/// What the app should do when the prompt is confirmed.
pub enum VaultPromptAction {
    None,
    /// Add a new text file at this vault-relative path (may nest with `/`).
    Add {
        rel_path: String,
        markdown: bool,
    },
    /// Add an empty folder at this vault-relative path.
    AddFolder {
        rel_path: String,
    },
    /// Rename the current file or folder to this vault-relative path.
    Rename {
        from: String,
        to: String,
        folder: bool,
    },
}

/// Small modal for naming a new vault file or renaming an existing one.
/// Paths may contain `/` to create or move into folders.
pub struct VaultPromptDialog {
    pub open: bool,
    mode: VaultPromptMode,
    /// The entry being renamed (empty for AddFile/AddFolder).
    from: String,
    /// Whether the rename targets a folder (prefix rename).
    rename_folder: bool,
    name: String,
    markdown: bool,
    error: Option<String>,
    want_focus: bool,
}

impl VaultPromptDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            mode: VaultPromptMode::AddFile,
            from: String::new(),
            rename_folder: false,
            name: String::new(),
            markdown: true,
            error: None,
            want_focus: false,
        }
    }

    /// Prompt for a new file. `parent` is prefilled (the selected file's
    /// folder) so new files land beside it by default.
    pub fn add_file(&mut self, parent: &str) {
        self.mode = VaultPromptMode::AddFile;
        self.from.clear();
        self.name = if parent.is_empty() {
            String::new()
        } else {
            format!("{parent}/")
        };
        self.markdown = true;
        self.error = None;
        self.want_focus = true;
        self.open = true;
    }

    /// Prompt for a new empty folder under `parent`.
    pub fn add_folder(&mut self, parent: &str) {
        self.mode = VaultPromptMode::AddFolder;
        self.from.clear();
        self.rename_folder = false;
        self.name = if parent.is_empty() {
            String::new()
        } else {
            format!("{parent}/")
        };
        self.error = None;
        self.want_focus = true;
        self.open = true;
    }

    /// Prompt to rename `rel_path` (a file, or a folder when `folder`).
    pub fn rename(&mut self, rel_path: &str, folder: bool) {
        self.mode = VaultPromptMode::Rename;
        self.from = rel_path.to_string();
        self.rename_folder = folder;
        self.name = rel_path.to_string();
        self.markdown = rel_path.ends_with(".md") || rel_path.ends_with(".markdown");
        self.error = None;
        self.want_focus = true;
        self.open = true;
    }

    pub fn set_error(&mut self, msg: String) {
        self.error = Some(msg);
    }

    pub fn render(&mut self, ctx: &egui::Context) -> VaultPromptAction {
        if !self.open {
            return VaultPromptAction::None;
        }
        let mut action = VaultPromptAction::None;
        let mut is_open = self.open;
        let title = match self.mode {
            VaultPromptMode::AddFile => "New File in Vault",
            VaultPromptMode::AddFolder => "New Folder in Vault",
            VaultPromptMode::Rename => "Rename",
        };

        egui::Window::new(title)
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .default_width(theme::dialog_width(ctx, 420.0))
            .max_width(theme::dialog_max_width(ctx))
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme::bg_primary())
                    .stroke(egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.4))),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.label(
                    RichText::new(
                        "Path inside the vault. Use / to place it in a folder \
                         (folders are created as needed).",
                    )
                    .size(12.0)
                    .color(theme::text_dim()),
                );
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.name)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace)
                        .hint_text(if self.mode == VaultPromptMode::AddFolder {
                            "notes/2026"
                        } else {
                            "notes/2026/plan.md"
                        }),
                );
                if self.want_focus {
                    resp.request_focus();
                    self.want_focus = false;
                }

                if self.mode == VaultPromptMode::AddFile {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Type").size(12.0).color(theme::text_dim()));
                        ui.radio_value(
                            &mut self.markdown,
                            true,
                            RichText::new("Markdown").size(12.0),
                        );
                        ui.radio_value(
                            &mut self.markdown,
                            false,
                            RichText::new("Plain text").size(12.0),
                        );
                    });
                }

                if let Some(err) = &self.error {
                    ui.label(RichText::new(err).size(12.0).color(theme::accent_red()));
                }

                let submit = ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.separator();
                ui.horizontal(|ui| {
                    let go = egui::Button::new(
                        RichText::new(if self.mode == VaultPromptMode::Rename {
                            "  Rename  "
                        } else {
                            "  Create  "
                        })
                        .size(13.0)
                        .color(theme::badge_text())
                        .strong(),
                    )
                    .fill(theme::badge_bg())
                    .corner_radius(theme::RADIUS);
                    if ui.add(go).clicked() || submit {
                        let name = self.name.trim().to_string();
                        if name.is_empty() {
                            self.error = Some("Enter a name".to_string());
                        } else {
                            action = match self.mode {
                                VaultPromptMode::AddFile => VaultPromptAction::Add {
                                    rel_path: name,
                                    markdown: self.markdown,
                                },
                                VaultPromptMode::AddFolder => {
                                    VaultPromptAction::AddFolder { rel_path: name }
                                }
                                VaultPromptMode::Rename => VaultPromptAction::Rename {
                                    from: self.from.clone(),
                                    to: name,
                                    folder: self.rename_folder,
                                },
                            };
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.open = false;
                    }
                });
            });

        if !is_open {
            self.open = false;
        }
        // Keep the window open on validation errors so the message shows.
        if !matches!(action, VaultPromptAction::None) && self.error.is_none() {
            self.open = false;
        }
        action
    }
}

/// A stateless confirm/cancel modal. Returns `Some(true)` on confirm,
/// `Some(false)` on cancel, `None` while still open. The caller owns the
/// "is it showing?" flag and stops calling this once it gets a result.
pub fn confirm_modal(
    ctx: &egui::Context,
    title: &str,
    message: &str,
    confirm_label: &str,
) -> Option<bool> {
    let mut result = None;
    egui::Window::new(title)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .resizable(false)
        .collapsible(false)
        .default_width(theme::dialog_width(ctx, 400.0))
        .max_width(theme::dialog_max_width(ctx))
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(theme::bg_primary())
                .stroke(egui::Stroke::new(
                    1.0,
                    theme::accent_red().gamma_multiply(0.5),
                )),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.label(
                RichText::new(message)
                    .size(13.0)
                    .color(theme::text_primary()),
            );
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    result = Some(false);
                }
                let bg = theme::accent_red().gamma_multiply(0.9);
                let del = egui::Button::new(
                    RichText::new(format!("  {confirm_label}  "))
                        .size(13.0)
                        .color(theme::contrast_text(bg))
                        .strong(),
                )
                .fill(bg)
                .corner_radius(theme::RADIUS);
                if ui.add(del).clicked() {
                    result = Some(true);
                }
            });
        });
    result
}

// ── Command Line Tool ────────────────────────────────────────────────────────

/// Result of installing the `schl8` symlink, shown to the user.
///
/// A dialog rather than a toast because two of the three outcomes have
/// something to read: the PATH-edit case needs the exact line, and a
/// failure needs its reason. There is deliberately no copy button — the
/// clipboard exception in `agent_help` is for fixed strings compiled
/// into the binary, and this text is built from the user's own paths.
pub struct CliToolDialog {
    pub open: bool,
    /// Headline: what happened.
    headline: String,
    /// Prose under it.
    detail: String,
    /// A shell line to type, if the user has work left to do.
    action_line: Option<String>,
    is_error: bool,
}

impl CliToolDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            headline: String::new(),
            detail: String::new(),
            action_line: None,
            is_error: false,
        }
    }

    pub fn show(
        &mut self,
        headline: impl Into<String>,
        detail: impl Into<String>,
        action_line: Option<String>,
        is_error: bool,
    ) {
        self.headline = headline.into();
        self.detail = detail.into();
        self.action_line = action_line;
        self.is_error = is_error;
        self.open = true;
    }

    pub fn render(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Command Line Tool")
            .open(&mut open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .default_width(theme::dialog_width(ctx, 460.0))
            .max_width(theme::dialog_max_width(ctx))
            .frame(egui::Frame::window(&ctx.style()).fill(theme::bg_primary()))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.label(RichText::new(&self.headline).size(14.0).strong().color(
                    if self.is_error {
                        theme::accent_red()
                    } else {
                        theme::text_strong()
                    },
                ));
                ui.label(
                    RichText::new(&self.detail)
                        .size(13.0)
                        .color(theme::text_dim()),
                );
                if let Some(line) = &self.action_line {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(line)
                            .size(12.0)
                            .monospace()
                            .color(theme::accent()),
                    );
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("  OK  ").size(13.0)).clicked() {
                        self.open = false;
                    }
                });
            });
        // Only let the titlebar X close it; the OK button above already
        // cleared the flag, and clobbering it here would reopen it.
        if !open {
            self.open = false;
        }
    }
}

impl Default for CliToolDialog {
    fn default() -> Self {
        Self::new()
    }
}

// ── Agent Toolkit ────────────────────────────────────────────────────────────

/// What the toolkit dialog is asking the app to do.
#[derive(PartialEq, Clone, Copy)]
pub enum ToolkitAction {
    None,
    Install,
    Uninstall,
}

/// "Make Schl8 available in every agent conversation."
///
/// Two routes, and the dialog is honest about which is which. The
/// universal one is a paste from Help → Instructions, which works on any
/// platform because the agent builds the toolkit with its own machinery.
/// The button here only covers Claude Code — the one layout this build
/// can verify — and says so rather than implying it configured
/// everything.
pub struct ToolkitDialog {
    pub open: bool,
    /// (action, path) for each file an install would touch.
    pub planned: Vec<(String, String)>,
    pub status: Option<(String, bool)>,
    /// True once anything of ours exists on disk.
    pub installed: bool,
}

impl ToolkitDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            planned: Vec::new(),
            status: None,
            installed: false,
        }
    }

    pub fn render(&mut self, ctx: &egui::Context) -> ToolkitAction {
        if !self.open {
            return ToolkitAction::None;
        }
        let mut action = ToolkitAction::None;
        let mut is_open = self.open;

        egui::Window::new("Agent Toolkit")
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .default_width(theme::dialog_width(ctx, 520.0))
            .max_width(theme::dialog_max_width(ctx))
            .max_height((ctx.screen_rect().height() - 90.0).max(240.0))
            .vscroll(true)
            .frame(egui::Frame::window(&ctx.style()).fill(theme::bg_primary()))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 7.0;

                ui.label(
                    RichText::new("Make Schl8 part of your assistant, permanently")
                        .size(14.0)
                        .strong()
                        .color(theme::text_strong()),
                );
                ui.label(
                    RichText::new(
                        "So \u{201C}save that to my notes\u{201D} works in every future \
                         conversation, without re-explaining Schl8 each time.",
                    )
                    .size(12.0)
                    .color(theme::text_dim()),
                );

                ui.add_space(4.0);
                ui.separator();

                ui.label(
                    RichText::new("Any assistant \u{2014} the portable way")
                        .size(13.0)
                        .strong()
                        .color(theme::text_primary()),
                );
                ui.label(
                    RichText::new(
                        "Help \u{203A} Instructions for your agent \u{203A} \
                         \u{201C}Make yourself permanently available\u{201D}. Paste that \
                         and your assistant runs the command below, then builds the \
                         toolkit using whatever skill or command system it actually \
                         has. This works on platforms Schl8 has never heard of, \
                         including your own.",
                    )
                    .size(12.0)
                    .color(theme::text_dim()),
                );
                ui.label(
                    RichText::new("schl8 agent toolkit")
                        .size(12.0)
                        .monospace()
                        .color(theme::accent()),
                );

                ui.add_space(4.0);
                ui.separator();

                ui.label(
                    RichText::new("Claude Code \u{2014} or let Schl8 write it")
                        .size(13.0)
                        .strong()
                        .color(theme::text_primary()),
                );
                ui.label(
                    RichText::new(
                        "Schl8 can write the skill and slash commands directly, \
                         because this is the one layout it can verify. Everything it \
                         writes is marked, and Remove only deletes files carrying that \
                         mark \u{2014} never one you wrote yourself.",
                    )
                    .size(12.0)
                    .color(theme::text_dim()),
                );

                if self.planned.is_empty() {
                    ui.label(
                        RichText::new("(nothing to write \u{2014} could not read the plan)")
                            .size(12.0)
                            .color(theme::text_dim()),
                    );
                } else {
                    for (act, path) in &self.planned {
                        ui.label(
                            RichText::new(format!("{act:<9} {path}"))
                                .size(11.0)
                                .monospace()
                                .color(if act == "Conflict" {
                                    theme::accent_red()
                                } else {
                                    theme::text_dim()
                                }),
                        );
                    }
                }

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let label = if self.installed {
                        "  Refresh  "
                    } else {
                        "  Install  "
                    };
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(label)
                                    .size(13.0)
                                    .color(theme::badge_text())
                                    .strong(),
                            )
                            .fill(theme::badge_bg())
                            .corner_radius(theme::RADIUS),
                        )
                        .on_hover_text(
                            "Write the skill and commands. Re-run this after renaming \
                             a quicknote \u{2014} the per-note shortcuts are generated.",
                        )
                        .clicked()
                    {
                        action = ToolkitAction::Install;
                    }
                    if ui
                        .add_enabled(
                            self.installed,
                            egui::Button::new(
                                RichText::new("  Remove  ")
                                    .size(13.0)
                                    .color(theme::text_primary()),
                            ),
                        )
                        .on_hover_text("Delete only the files Schl8 generated")
                        .clicked()
                    {
                        action = ToolkitAction::Uninstall;
                    }
                });

                if let Some((msg, is_err)) = &self.status {
                    ui.add_space(2.0);
                    ui.label(RichText::new(msg).size(12.0).color(if *is_err {
                        theme::accent_red()
                    } else {
                        theme::text_primary()
                    }));
                }

                ui.add_space(2.0);
                ui.label(
                    RichText::new(
                        "Start a new assistant session afterwards \u{2014} skills load at \
                         session start.",
                    )
                    .size(11.0)
                    .color(theme::text_dim()),
                );
            });

        self.open = is_open;
        action
    }
}

impl Default for ToolkitDialog {
    fn default() -> Self {
        Self::new()
    }
}
