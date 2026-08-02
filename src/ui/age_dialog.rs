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
    /// Second copy of the passphrase, generate mode only.
    ///
    /// A mistyped BIP-39 passphrase does not fail — it silently derives
    /// a *different, perfectly valid* key. When unlocking that is
    /// recoverable: you try again. At generation it is not. You would
    /// write down twelve words, store a recipient, and never be able to
    /// reach the key again, because the passphrase you meant to set was
    /// never the one that made it. Hence typing it twice.
    passphrase_confirm: SecureString,
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
            passphrase_confirm: SecureString::empty(),
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
        self.passphrase_confirm = SecureString::empty();
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
        self.passphrase_confirm = SecureString::empty();
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
            // Above the other windows, not merely after them. "Generate
            // new AGE key…" is launched *from* the Manage Public Keys
            // window, which is centred too and larger — so at the
            // default order this dialog opened exactly behind it and the
            // button looked like it did nothing at all.
            .order(egui::Order::Foreground)
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
        // `available_width()`, NOT `f32::INFINITY`. TextEdit treats an
        // infinite desired width as "fill the row", so the same idiom a
        // few lines up is fine — ProgressBar takes it literally and lays
        // out an infinitely wide rect. The next subtraction in egui's
        // layout is then ∞ − ∞ = NaN, which trips a debug assertion and
        // takes the whole process down: opening this dialog crashed the
        // app before it had drawn a single frame of itself.
        ui.add(
            egui::ProgressBar::new(frac)
                .desired_width(ui.available_width())
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

        // ── Optional passphrase (25th word) ──────────────────
        ui.add_space(2.0);
        let toggled = ui
            .checkbox(
                &mut self.show_passphrase_field,
                RichText::new("Protect this key with an extra passphrase (25th word)")
                    .size(12.0)
                    .color(theme::text_primary()),
            )
            .changed();
        let mut passphrase_edited = toggled;
        if self.show_passphrase_field {
            ui.label(
                RichText::new(
                    "The passphrase is not stored in the twelve words and cannot be \
                     recovered from them, changed later, or reset. Both together open \
                     the key; either alone opens nothing. Write it down with the words.",
                )
                .size(11.0)
                .color(theme::accent_yellow()),
            );
            passphrase_edited |=
                super::secure_edit::multiline(ui, &mut self.passphrase, None, |te| {
                    te.font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(1)
                        .hint_text("passphrase")
                        .password(true)
                        .lock_focus(true)
                })
                .changed();
            passphrase_edited |=
                super::secure_edit::multiline(ui, &mut self.passphrase_confirm, None, |te| {
                    te.font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(1)
                        .hint_text("type it again")
                        .password(true)
                        .lock_focus(true)
                })
                .changed();
            if let Some(problem) = self.passphrase_problem() {
                ui.label(
                    RichText::new(problem)
                        .size(11.5)
                        .color(theme::accent_red())
                        .strong(),
                );
            }
        }
        // A key already on screen was derived with whatever the
        // passphrase was a moment ago. Change it and the recipient below
        // is no longer this key's recipient — so re-derive from the same
        // twelve words rather than leave a stale `age1…` on display for
        // someone to copy into their keyring.
        if passphrase_edited && self.generated_phrase.is_some() {
            self.rederive_recipient();
        }

        if let Some(err) = &self.error {
            ui.label(RichText::new(err).size(12.0).color(theme::accent_red()));
        }

        // ── Generated phrase (after Generate) ────────────────
        let with_passphrase = self.show_passphrase_field;
        if let Some(phrase) = &self.generated_phrase {
            ui.separator();
            ui.label(
                RichText::new(if with_passphrase {
                    "⚠  Write these 12 words AND your passphrase down now"
                } else {
                    "⚠  Write these 12 words down now"
                })
                .size(13.0)
                .color(theme::accent_red())
                .strong(),
            );
            ui.label(
                RichText::new(if with_passphrase {
                    "These twelve words and your passphrase together are the ONLY way to \
                     unlock this key. Neither is saved to disk, and the words alone will \
                     not do it — lose either one and the key, and everything encrypted to \
                     it, are gone for good."
                } else {
                    "This recovery phrase is the ONLY way to unlock this key. It is never \
                     saved to disk — if you lose it, the key and everything encrypted to it \
                     are gone for good."
                })
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

    /// The passphrase this key is being generated with — empty when the
    /// user has not asked for one.
    fn effective_passphrase(&self) -> &str {
        if self.show_passphrase_field {
            self.passphrase.as_str()
        } else {
            ""
        }
    }

    /// Why the passphrase is not usable yet, if it isn't.
    ///
    /// Only meaningful with the passphrase enabled: an empty one there
    /// means the box is ticked and nothing typed, which would silently
    /// generate an unprotected key while looking protected.
    fn passphrase_problem(&self) -> Option<&'static str> {
        if !self.show_passphrase_field {
            return None;
        }
        if self.passphrase.as_str().is_empty() {
            Some("Enter a passphrase, or clear the checkbox above.")
        } else if self.passphrase.as_str() != self.passphrase_confirm.as_str() {
            Some("The two passphrases do not match.")
        } else {
            None
        }
    }

    /// Recompute the shown recipient from the phrase already generated.
    ///
    /// Used when the passphrase changes after generation: same twelve
    /// words, different key. While the passphrase is unusable there is
    /// no honest recipient to show, so none is shown.
    fn rederive_recipient(&mut self) {
        if self.passphrase_problem().is_some() {
            self.derived_recipient = None;
            return;
        }
        let derived = self.generated_phrase.as_ref().map(|phrase| {
            age_backend::recipient_from_mnemonic(phrase.as_str(), self.effective_passphrase())
        });
        match derived {
            Some(Ok(recipient)) => {
                self.derived_recipient = Some(recipient);
                self.error = None;
            }
            Some(Err(e)) => {
                self.derived_recipient = None;
                self.error = Some(format!("{e:#}"));
            }
            None => self.derived_recipient = None,
        }
    }

    /// Generate mode: create a fresh mnemonic from OS randomness plus the
    /// typed entropy, and derive its public recipient for display.
    fn generate(&mut self) {
        self.error = None;
        // Checked before burning randomness: generating a key the user
        // cannot open is worse than refusing to generate one.
        if let Some(problem) = self.passphrase_problem() {
            self.error = Some(problem.to_string());
            return;
        }
        match age_backend::generate_mnemonic_with_entropy(self.entropy_input.as_bytes()) {
            Ok(mnemonic) => {
                match age_backend::recipient_from_mnemonic(
                    mnemonic.as_str(),
                    self.effective_passphrase(),
                ) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Render `dialog` for a few frames, as the app would.
    ///
    /// More than one frame because egui defers work to the frame after a
    /// widget first appears — a single pass can miss a crash.
    fn run_frames(dialog: &mut AgeDialog) {
        let ctx = egui::Context::default();
        for _ in 0..3 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                dialog.render(ctx);
            });
        }
    }

    /// Opening this dialog must not take the application down with it.
    ///
    /// It did: the entropy meter asked for `f32::INFINITY` width, which
    /// `TextEdit` reads as "fill the row" but `ProgressBar` takes
    /// literally, and the infinite rect became a NaN one subtraction
    /// later. The dialog crashed the process before drawing a frame of
    /// itself, which looked from outside like the button doing nothing.
    #[test]
    fn opening_the_generate_dialog_does_not_crash() {
        let mut dialog = AgeDialog::new();
        dialog.show_generate();
        run_frames(&mut dialog);
        assert!(dialog.open, "the dialog should still be open");
    }

    /// The same screen once a key exists: more widgets, same hazard.
    #[test]
    fn showing_a_generated_key_does_not_crash() {
        let mut dialog = AgeDialog::new();
        dialog.show_generate();
        dialog.generate();
        assert!(
            dialog.generated_phrase.is_some(),
            "generation should have produced a phrase: {:?}",
            dialog.error
        );
        run_frames(&mut dialog);
    }

    /// The other two modes share the layout code; neither should crash
    /// either, and nothing here should depend on which ran first.
    #[test]
    fn the_unlock_and_export_dialogs_do_not_crash() {
        for open in [AgeDialog::show_unlock, AgeDialog::show_export] {
            let mut dialog = AgeDialog::new();
            open(&mut dialog);
            run_frames(&mut dialog);
        }
    }

    /// Entropy estimation runs every frame over whatever has been typed,
    /// including nothing at all — and feeds a progress bar, where a NaN
    /// or an out-of-range fraction is a panic rather than a wrong pixel.
    #[test]
    fn the_entropy_meter_fraction_is_always_drawable() {
        for input in [
            "".as_bytes(),
            "a".as_bytes(),
            "aaaaaaaaaaaaaaaaaaaa".as_bytes(),
            &[0u8; 4096],
        ] {
            let bits = age_backend::estimate_entropy_bits(input);
            assert!(
                bits.is_finite(),
                "entropy was {bits} for {} bytes",
                input.len()
            );
            let frac = (bits / 128.0).clamp(0.0, 1.0) as f32;
            assert!(
                frac.is_finite() && (0.0..=1.0).contains(&frac),
                "fraction {frac} is not a drawable progress value"
            );
        }
    }

    /// Set the passphrase pair as the user would have typed it.
    fn type_passphrase(dialog: &mut AgeDialog, pass: &str, confirm: &str) {
        dialog.show_passphrase_field = true;
        dialog.passphrase = SecureString::empty();
        dialog.passphrase.push_str(pass);
        dialog.passphrase_confirm = SecureString::empty();
        dialog.passphrase_confirm.push_str(confirm);
    }

    /// The whole point of the feature: the passphrase has to reach the
    /// derivation. A key generated "with" one that was silently ignored
    /// would be unprotected while looking protected.
    #[test]
    fn the_passphrase_changes_which_key_is_generated() {
        let mut plain = AgeDialog::new();
        plain.show_generate();
        plain.generate();
        let phrase = plain
            .generated_phrase
            .as_ref()
            .unwrap()
            .as_str()
            .to_string();
        let without = plain.derived_recipient.clone().unwrap();

        // Same twelve words, a passphrase added.
        let with = age_backend::recipient_from_mnemonic(&phrase, "correct horse").unwrap();
        assert_ne!(
            without, with,
            "the passphrase made no difference to the derived key"
        );
    }

    /// The recipient shown at generation must be the one the user gets
    /// back when they later unlock with those same secrets. If these
    /// ever disagree, someone encrypts to a key they cannot open.
    #[test]
    fn the_shown_recipient_is_what_unlocking_reproduces() {
        let mut dialog = AgeDialog::new();
        dialog.show_generate();
        type_passphrase(&mut dialog, "a passphrase", "a passphrase");
        dialog.generate();

        let shown = dialog.derived_recipient.clone().expect("a recipient");
        let phrase = dialog
            .generated_phrase
            .as_ref()
            .unwrap()
            .as_str()
            .to_string();
        let identity =
            age_backend::AgeIdentity::from_mnemonic(&phrase, "a passphrase").expect("unlock");
        assert_eq!(identity.recipient(), shown);

        // And the words alone are a different key entirely — which is
        // exactly why both have to be written down.
        let no_pass = age_backend::AgeIdentity::from_mnemonic(&phrase, "").expect("unlock");
        assert_ne!(no_pass.recipient(), shown);
    }

    /// A typo in the confirmation must stop generation. A mistyped BIP-39
    /// passphrase does not fail, it derives a different valid key — so
    /// without this check the user writes down words that open nothing
    /// they can reach.
    #[test]
    fn a_mismatched_confirmation_refuses_to_generate() {
        let mut dialog = AgeDialog::new();
        dialog.show_generate();
        type_passphrase(&mut dialog, "hunter2", "hunter3");
        dialog.generate();
        assert!(dialog.generated_phrase.is_none(), "generated anyway");
        assert!(dialog.derived_recipient.is_none());
        assert!(dialog
            .error
            .as_deref()
            .unwrap_or("")
            .contains("do not match"));
    }

    /// Ticking the box and typing nothing would quietly produce an
    /// unprotected key while the screen said it was protected.
    #[test]
    fn an_empty_passphrase_refuses_to_generate() {
        let mut dialog = AgeDialog::new();
        dialog.show_generate();
        type_passphrase(&mut dialog, "", "");
        dialog.generate();
        assert!(dialog.generated_phrase.is_none(), "generated anyway");
        assert!(dialog.error.is_some());
    }

    /// Changing the passphrase after generating keeps the twelve words
    /// and re-derives the key. Leaving the old recipient on screen would
    /// invite copying an `age1…` that belongs to a different key.
    #[test]
    fn editing_the_passphrase_after_generating_rederives_the_recipient() {
        let mut dialog = AgeDialog::new();
        dialog.show_generate();
        type_passphrase(&mut dialog, "first", "first");
        dialog.generate();
        let phrase = dialog
            .generated_phrase
            .as_ref()
            .unwrap()
            .as_str()
            .to_string();
        let first = dialog.derived_recipient.clone().unwrap();

        type_passphrase(&mut dialog, "second", "second");
        dialog.rederive_recipient();
        let second = dialog.derived_recipient.clone().unwrap();

        assert_eq!(
            dialog.generated_phrase.as_ref().unwrap().as_str(),
            phrase,
            "the words should not change"
        );
        assert_ne!(first, second, "the key should have been re-derived");
        assert_eq!(
            second,
            age_backend::AgeIdentity::from_mnemonic(&phrase, "second")
                .unwrap()
                .recipient()
        );

        // Half-typed confirmation: show no recipient at all rather than
        // one that does not correspond to what will be saved.
        type_passphrase(&mut dialog, "third", "thi");
        dialog.rederive_recipient();
        assert!(dialog.derived_recipient.is_none());
    }

    /// Closing must zeroize the confirmation too, not just the original.
    #[test]
    fn closing_clears_both_passphrase_copies() {
        let mut dialog = AgeDialog::new();
        dialog.show_generate();
        type_passphrase(&mut dialog, "secret", "secret");
        dialog.close();
        assert!(dialog.passphrase.as_str().is_empty());
        assert!(dialog.passphrase_confirm.as_str().is_empty());
    }
}
