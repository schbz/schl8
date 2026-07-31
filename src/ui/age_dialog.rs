//! Age seed-phrase dialogs: unlock (derive + hold the identity), export
//! public key (derive just the `age1…` recipient), and generate a brand
//! new key from the OS CSPRNG plus optional user entropy.
//!
//! The 12-word phrase and optional passphrase are typed into
//! `SecureString`s (mlock'd, zeroized) and cleared as soon as the
//! dialog closes or derivation completes. A freshly generated mnemonic is
//! likewise held only in a `SecureString` and never written to disk.

use egui::{Align2, RichText, Vec2};

use super::theme;
use crate::crypto::age_backend;
use crate::crypto::secure_buf::SecureString;

/// What the app should do after the dialog renders this frame.
pub enum AgeAction {
    None,
    /// Derive and hold the identity from these secrets (unlock flow).
    Unlock,
    /// Save the shown recipient string to a file (export flow).
    SaveRecipient(String),
    /// Store the shown recipient in the age recipient list (export /
    /// generate flow).
    AddRecipient(String),
}

/// Which flow the dialog is running.
#[derive(PartialEq, Clone, Copy)]
enum Mode {
    Unlock,
    Export,
    Generate,
}

pub struct AgeDialog {
    pub open: bool,
    mode: Mode,
    phrase: SecureString,
    passphrase: SecureString,
    show_passphrase_field: bool,
    /// Computed recipient (export / generate mode), shown read-only.
    derived_recipient: Option<String>,
    /// Optional user-supplied entropy typed in generate mode.
    entropy_input: SecureString,
    /// A freshly generated 12-word mnemonic (generate mode), shown once so
    /// the user can write it down. Held only in memory.
    generated_phrase: Option<SecureString>,
    error: Option<String>,
    want_focus: bool,
}

impl AgeDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            mode: Mode::Unlock,
            phrase: SecureString::empty(),
            passphrase: SecureString::empty(),
            show_passphrase_field: false,
            derived_recipient: None,
            entropy_input: SecureString::empty(),
            generated_phrase: None,
            error: None,
            want_focus: false,
        }
    }

    pub fn show_unlock(&mut self) {
        self.reset(Mode::Unlock);
    }

    pub fn show_export(&mut self) {
        self.reset(Mode::Export);
    }

    pub fn show_generate(&mut self) {
        self.reset(Mode::Generate);
    }

    fn reset(&mut self, mode: Mode) {
        self.mode = mode;
        self.phrase = SecureString::empty();
        self.passphrase = SecureString::empty();
        self.show_passphrase_field = false;
        self.derived_recipient = None;
        self.entropy_input = SecureString::empty();
        self.generated_phrase = None;
        self.error = None;
        self.want_focus = true;
        self.open = true;
    }

    /// The phrase / passphrase for the app to consume on Unlock. Cleared
    /// by `clear_secrets` afterwards.
    pub fn secrets(&self) -> (&str, &str) {
        (self.phrase.as_str(), self.passphrase.as_str())
    }

    /// Zeroize the entered/generated secrets (call after Unlock or close).
    pub fn clear_secrets(&mut self) {
        self.phrase = SecureString::empty();
        self.passphrase = SecureString::empty();
        self.entropy_input = SecureString::empty();
        self.generated_phrase = None;
    }

    pub fn set_error(&mut self, msg: String) {
        self.error = Some(msg);
    }

    pub fn close(&mut self) {
        self.open = false;
        self.clear_secrets();
        self.derived_recipient = None;
    }

    pub fn render(&mut self, ctx: &egui::Context) -> AgeAction {
        if !self.open {
            return AgeAction::None;
        }
        let mut action = AgeAction::None;
        let mut is_open = self.open;

        let title = match self.mode {
            Mode::Unlock => "Unlock AGE Identity",
            Mode::Export => "Export AGE Public Key",
            Mode::Generate => "Generate New AGE Key",
        };

        egui::Window::new(title)
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .default_width(theme::dialog_width(ctx, 500.0))
            .max_width(theme::dialog_max_width(ctx))
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme::bg_primary())
                    .stroke(egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.4))),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                if self.mode == Mode::Generate {
                    self.render_generate(ui, &mut action);
                } else {
                    self.render_phrase_entry(ui, &mut action);
                }
            });

        if !is_open {
            self.open = false;
        }
        if !self.open {
            self.clear_secrets();
            self.derived_recipient = None;
        }
        action
    }

    /// Unlock / Export bodies: the user types their existing seed phrase.
    fn render_phrase_entry(&mut self, ui: &mut egui::Ui, action: &mut AgeAction) {
        let intro = match self.mode {
            Mode::Unlock => {
                "Enter your 12-word BIP-39 seed phrase to derive your AGE \
                 private key and hold it in memory for this session. The \
                 phrase and key never touch disk."
            }
            _ => {
                "Enter your 12-word BIP-39 seed phrase to derive its public \
                 AGE recipient (age1…). Only the public key is shown — the \
                 private key is discarded immediately."
            }
        };
        ui.label(RichText::new(intro).size(12.5).color(theme::text_dim()));
        ui.add_space(2.0);

        // ── Phrase field ─────────────────────────────────────
        ui.label(
            RichText::new("Seed phrase (12 words)")
                .size(12.0)
                .color(theme::text_primary()),
        );
        let resp = super::secure_edit::multiline(ui, &mut self.phrase, None, |te| {
            te.font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .desired_rows(2)
                .hint_text("word1 word2 … word12")
                .lock_focus(true)
        });
        if self.want_focus {
            resp.request_focus();
            self.want_focus = false;
        }

        // ── Optional passphrase (25th word) ──────────────────
        ui.checkbox(
            &mut self.show_passphrase_field,
            RichText::new("I use an extra passphrase (25th word)")
                .size(12.0)
                .color(theme::text_primary()),
        );
        if self.show_passphrase_field {
            let _ = super::secure_edit::multiline(ui, &mut self.passphrase, None, |te| {
                te.font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(1)
                    .hint_text("extra passphrase")
                    .lock_focus(true)
            });
        }

        if let Some(err) = &self.error {
            ui.label(RichText::new(err).size(12.0).color(theme::accent_red()));
        }

        // ── Derived recipient (export mode) ──────────────────
        if let Some(recipient) = self.derived_recipient.clone() {
            self.show_recipient_result(ui, &recipient, action);
        }

        ui.separator();
        ui.horizontal(|ui| {
            match self.mode {
                Mode::Unlock => {
                    if ui.add(primary_button("  Unlock  ")).clicked() {
                        *action = AgeAction::Unlock;
                    }
                }
                _ => {
                    if ui.add(primary_button("  Derive Public Key  ")).clicked() {
                        self.derive_for_export();
                    }
                }
            }
            if ui.button("Close").clicked() {
                self.open = false;
            }
        });
    }

    /// Generate body: create a new key from OS randomness + optional user
    /// entropy, with a live strength meter.
    fn render_generate(&mut self, ui: &mut egui::Ui, action: &mut AgeAction) {
        ui.label(
            RichText::new(
                "Create a brand-new AGE key. A 12-word recovery phrase is generated \
                 from your system's secure random generator. You can optionally add \
                 your own randomness below — it can only strengthen the result, never \
                 weaken it.",
            )
            .size(12.5)
            .color(theme::text_dim()),
        );
        ui.add_space(2.0);

        // ── Optional user entropy ────────────────────────────
        ui.label(
            RichText::new("Extra randomness (optional)")
                .size(12.0)
                .color(theme::text_primary()),
        );
        let resp = super::secure_edit::multiline(ui, &mut self.entropy_input, None, |te| {
            te.font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .desired_rows(2)
                .hint_text("type or mash keys to add randomness…")
                .lock_focus(true)
        });
        if self.want_focus {
            resp.request_focus();
            self.want_focus = false;
        }

        // ── Live entropy meter ───────────────────────────────
        let bits = age_backend::estimate_entropy_bits(self.entropy_input.as_bytes());
        let (label, color) = entropy_quality(bits);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Your input: ~{bits:.0} bits"))
                    .size(12.0)
                    .color(theme::text_primary()),
            );
            ui.label(RichText::new(label).size(12.0).color(color).strong());
        });
        let frac = (bits / 128.0).clamp(0.0, 1.0) as f32;
        ui.add(
            egui::ProgressBar::new(frac)
                .desired_width(f32::INFINITY)
                .fill(color),
        );
        ui.label(
            RichText::new(
                "Combined with 256 bits from the system CSPRNG — the final key strength \
                 is always at least 128 bits, even with no input above.",
            )
            .size(11.0)
            .color(theme::text_dim()),
        );

        if let Some(err) = &self.error {
            ui.label(RichText::new(err).size(12.0).color(theme::accent_red()));
        }

        // ── Generated phrase (after Generate) ────────────────
        if let Some(phrase) = &self.generated_phrase {
            ui.separator();
            ui.label(
                RichText::new("⚠  Write these 12 words down now")
                    .size(13.0)
                    .color(theme::accent_red())
                    .strong(),
            );
            ui.label(
                RichText::new(
                    "This recovery phrase is the ONLY way to unlock this key. It is never \
                     saved to disk — if you lose it, the key and everything encrypted to it \
                     are gone for good.",
                )
                .size(11.5)
                .color(theme::text_dim()),
            );
            let mut shown = phrase.as_str().to_string();
            ui.add(
                egui::TextEdit::multiline(&mut shown)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(2)
                    .interactive(false),
            );
            if let Some(recipient) = self.derived_recipient.clone() {
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Public recipient (age1…)")
                        .size(12.0)
                        .color(theme::text_dim()),
                );
                let mut r = recipient.clone();
                ui.add(
                    egui::TextEdit::multiline(&mut r)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(2)
                        .interactive(false),
                );
                ui.horizontal(|ui| {
                    if ui
                        .button("Add public key to my keys")
                        .on_hover_text("Store the recipient under Manage Public Keys")
                        .clicked()
                    {
                        *action = AgeAction::AddRecipient(recipient.clone());
                    }
                    if ui
                        .button("Save recipient to file…")
                        .on_hover_text("Write the AGE recipient to a .txt file to share")
                        .clicked()
                    {
                        *action = AgeAction::SaveRecipient(recipient);
                    }
                });
            }
        }

        ui.separator();
        ui.horizontal(|ui| {
            let btn_label = if self.generated_phrase.is_some() {
                "  Regenerate  "
            } else {
                "  Generate Key  "
            };
            if ui.add(primary_button(btn_label)).clicked() {
                self.generate();
            }
            if ui.button("Close").clicked() {
                self.open = false;
            }
        });
    }

    /// Common recipient result block (export mode): read-only recipient
    /// with Save / Add buttons.
    fn show_recipient_result(&self, ui: &mut egui::Ui, recipient: &str, action: &mut AgeAction) {
        ui.separator();
        ui.label(
            RichText::new("Your AGE public recipient")
                .size(12.0)
                .color(theme::text_dim()),
        );
        let mut shown = recipient.to_string();
        ui.add(
            egui::TextEdit::multiline(&mut shown)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .desired_rows(2)
                .interactive(false),
        );
        ui.horizontal(|ui| {
            if ui
                .button("Save to file…")
                .on_hover_text("Write the AGE recipient to a .txt file to share")
                .clicked()
            {
                *action = AgeAction::SaveRecipient(recipient.to_string());
            }
            if ui
                .button("Add to my keys")
                .on_hover_text("Store it under Manage Public Keys so you can encrypt to it")
                .clicked()
            {
                *action = AgeAction::AddRecipient(recipient.to_string());
            }
        });
    }

    /// Export mode: derive the recipient and show it (no identity held).
    fn derive_for_export(&mut self) {
        self.error = None;
        let phrase = self.phrase.as_str();
        let passphrase = self.passphrase.as_str();
        match age_backend::recipient_from_mnemonic(phrase, passphrase) {
            Ok(recipient) => {
                self.derived_recipient = Some(recipient);
                // The phrase is no longer needed — wipe it, keep only the
                // public result on screen.
                self.phrase = SecureString::empty();
                self.passphrase = SecureString::empty();
            }
            Err(e) => {
                self.derived_recipient = None;
                self.error = Some(format!("{e:#}"));
            }
        }
    }

    /// Generate mode: create a fresh mnemonic from OS randomness plus the
    /// typed entropy, and derive its public recipient for display.
    fn generate(&mut self) {
        self.error = None;
        match age_backend::generate_mnemonic_with_entropy(self.entropy_input.as_bytes()) {
            Ok(mnemonic) => {
                match age_backend::recipient_from_mnemonic(mnemonic.as_str(), "") {
                    Ok(recipient) => self.derived_recipient = Some(recipient),
                    Err(e) => {
                        self.error = Some(format!("{e:#}"));
                        return;
                    }
                }
                self.generated_phrase = Some(mnemonic);
            }
            Err(e) => {
                self.generated_phrase = None;
                self.derived_recipient = None;
                self.error = Some(format!("{e:#}"));
            }
        }
    }
}

fn primary_button(label: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(label.to_string())
            .size(13.0)
            .color(theme::badge_text())
            .strong(),
    )
    .fill(theme::badge_bg())
    .corner_radius(theme::RADIUS)
}

/// Qualitative label + color for a user-entropy bit estimate.
fn entropy_quality(bits: f64) -> (&'static str, egui::Color32) {
    if bits < 32.0 {
        ("weak", theme::accent_red())
    } else if bits < 64.0 {
        ("fair", theme::accent())
    } else if bits < 96.0 {
        ("good", theme::accent())
    } else {
        ("strong", theme::badge_bg())
    }
}
