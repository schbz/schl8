//! "Save Targets" window: configure the current document's save plan —
//! which key(s) it is encrypted to on Save, and which destination path(s)
//! each key's ciphertext is written to (all overwritten on every Save).

use std::path::{Path, PathBuf};

use egui::{Align2, RichText, Vec2};

use super::theme;
use crate::config::{SavePlan, SaveRule};
use crate::crypto::keys::PublicKey;

/// What the app should do after rendering this frame.
pub enum PlanAction {
    None,
    /// Open a save-file picker to add a destination to rule `rule_idx`.
    AddDestination {
        rule_idx: usize,
    },
    /// Persist the edited plan (empty plans mean "remove").
    Apply(SavePlan),
}

pub struct SaveTargetsDialog {
    pub open: bool,
    draft: SavePlan,
    /// Keyring public keys for the key dropdowns (loaded on open).
    keys: Vec<PublicKey>,
    /// Stored age recipients (label, `age1…`) for the key dropdowns.
    age_recipients: Vec<(String, String)>,
    /// The document's original recipient fingerprints — the default for
    /// rules whose key is left unset.
    original_recipients: Vec<String>,
    error: Option<String>,
}

impl SaveTargetsDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            draft: SavePlan::default(),
            keys: Vec::new(),
            age_recipients: Vec::new(),
            original_recipients: Vec::new(),
            error: None,
        }
    }

    /// Open the dialog for `source`. Seeds from the existing plan, or —
    /// when none exists — with a starter rule targeting the document's
    /// current key and its own path, so "keep saving as-is" is one click.
    pub fn open_for(
        &mut self,
        source: &Path,
        existing: Option<&SavePlan>,
        current_recipients: Option<&[String]>,
        age_recipients: Vec<(String, String)>,
        // For an AGE document: the recipient of the identity it was
        // decrypted with. AGE ciphertext records no recipient, so this is
        // the only way to know the file's "own key".
        current_age_recipient: Option<&str>,
    ) {
        self.keys = crate::crypto::keys::list_public_keys().unwrap_or_default();
        self.age_recipients = age_recipients;
        self.original_recipients = current_recipients
            .map(<[String]>::to_vec)
            .unwrap_or_default();
        self.error = None;

        self.draft = match existing {
            Some(plan) => plan.clone(),
            None => {
                let mut rules = Vec::new();
                if let Some(fprs) = current_recipients {
                    // Each plan rule encrypts to ONE key, so only the first
                    // recipient is seeded with the source path — a second
                    // key writing the same file would overwrite the first
                    // key's copy. Additional recipients start with no
                    // destination for the user to fill in.
                    for (i, fpr) in fprs.iter().enumerate() {
                        rules.push(SaveRule {
                            key_fingerprint: fpr.clone(),
                            key_label: self.label_for(fpr),
                            age_recipient: String::new(),
                            destinations: if i == 0 {
                                vec![source.to_path_buf()]
                            } else {
                                Vec::new()
                            },
                        });
                    }
                }
                if rules.is_empty() {
                    // An AGE file has no recipients in its ciphertext, so the
                    // GPG "file's own key" default can't apply. Seed the
                    // identity it was actually decrypted with, so opening
                    // Save Options on an AGE file starts on the right key
                    // instead of "Choose a key…".
                    match current_age_recipient {
                        Some(recipient) => {
                            let label = self
                                .age_recipients
                                .iter()
                                .find(|(_, r)| r == recipient)
                                .map(|(l, _)| format!("{l} (AGE)"))
                                .unwrap_or_else(|| "This device (AGE)".to_string());
                            rules.push(SaveRule {
                                age_recipient: recipient.to_string(),
                                key_label: label,
                                destinations: vec![source.to_path_buf()],
                                ..Default::default()
                            });
                        }
                        None => rules.push(SaveRule {
                            destinations: vec![source.to_path_buf()],
                            ..Default::default()
                        }),
                    }
                }
                SavePlan {
                    source: source.to_path_buf(),
                    rules,
                    ..Default::default()
                }
            }
        };
        self.open = true;
    }

    /// Whether rule `rule_idx` encrypts with age (so its destinations
    /// should default to a `.age` name).
    pub fn rule_is_age(&self, rule_idx: usize) -> bool {
        self.draft.rules.get(rule_idx).is_some_and(SaveRule::is_age)
    }

    /// Called by the app after the destination picker returns.
    pub fn add_destination(&mut self, rule_idx: usize, path: PathBuf) {
        if let Some(rule) = self.draft.rules.get_mut(rule_idx) {
            if !rule.destinations.contains(&path) {
                rule.destinations.push(path);
            }
        }
    }

    fn label_for(&self, fingerprint: &str) -> String {
        self.keys
            .iter()
            .find(|k| k.fingerprint.eq_ignore_ascii_case(fingerprint))
            .map(|k| k.uid.clone())
            .unwrap_or_else(|| short_fpr(fingerprint))
    }

    /// Render. Returns the action for the app to take.
    pub fn render(&mut self, ctx: &egui::Context) -> PlanAction {
        if !self.open {
            return PlanAction::None;
        }

        let mut action = PlanAction::None;
        let mut is_open = self.open;
        let max_height = (ctx.screen_rect().height() - 90.0).max(240.0);

        let source_name = self
            .draft
            .source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("document")
            .to_string();

        egui::Window::new("Save Targets")
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .default_width(theme::dialog_width(ctx, 520.0))
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

                ui.label(theme::gradient_text(&source_name, 16.0));
                ui.label(
                    RichText::new(
                        "On every Save, this document is encrypted to each key below \
                         and written to all of that key's destinations, overwriting \
                         existing files.",
                    )
                    .size(11.5)
                    .color(theme::text_dim()),
                );
                ui.add_space(2.0);

                let mut remove_rule: Option<usize> = None;
                let mut remove_dest: Option<(usize, usize)> = None;

                let key_choices = self.keys.clone();
                let age_choices = self.age_recipients.clone();
                let has_default_key = !self.original_recipients.is_empty();
                for (ri, rule) in self.draft.rules.iter_mut().enumerate() {
                    egui::Frame::NONE
                        .fill(theme::bg_raised().gamma_multiply(0.6))
                        .corner_radius(theme::RADIUS)
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 5.0;

                            // ── Key selector ─────────────────────────
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("\u{1F511} Key")
                                        .size(12.0)
                                        .color(theme::text_dim()),
                                );
                                let selected = if rule.has_key() {
                                    rule.key_label.clone()
                                } else if has_default_key {
                                    "File's own key (default)".to_string()
                                } else {
                                    "Choose a key…".to_string()
                                };
                                egui::ComboBox::from_id_salt(("plan_key", ri))
                                    .selected_text(RichText::new(selected).size(12.5))
                                    .width(260.0)
                                    .show_ui(ui, |ui| {
                                        for k in &key_choices {
                                            let text = format!(
                                                "{}  ({})",
                                                k.uid,
                                                short_fpr(&k.fingerprint)
                                            );
                                            let is_sel = !rule.is_age()
                                                && rule
                                                    .key_fingerprint
                                                    .eq_ignore_ascii_case(&k.fingerprint);
                                            if ui
                                                .selectable_label(
                                                    is_sel,
                                                    RichText::new(text).size(12.5),
                                                )
                                                .clicked()
                                            {
                                                rule.key_fingerprint = k.fingerprint.clone();
                                                rule.age_recipient.clear();
                                                rule.key_label = k.uid.clone();
                                            }
                                        }
                                        if !age_choices.is_empty() {
                                            ui.separator();
                                            ui.label(
                                                RichText::new("AGE keys")
                                                    .size(11.0)
                                                    .color(theme::text_dim()),
                                            );
                                            for (label, recipient) in &age_choices {
                                                let short = if recipient.len() > 14 {
                                                    format!("{}…", &recipient[..14])
                                                } else {
                                                    recipient.clone()
                                                };
                                                let text = format!("{label}  (AGE: {short})");
                                                let is_sel = rule.age_recipient == *recipient;
                                                if ui
                                                    .selectable_label(
                                                        is_sel,
                                                        RichText::new(text).size(12.5),
                                                    )
                                                    .clicked()
                                                {
                                                    rule.age_recipient = recipient.clone();
                                                    rule.key_fingerprint.clear();
                                                    rule.key_label = format!("{label} (AGE)");
                                                }
                                            }
                                        }
                                    });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .button(RichText::new("Remove key").size(11.5))
                                            .clicked()
                                        {
                                            remove_rule = Some(ri);
                                        }
                                    },
                                );
                            });

                            // ── Destinations ─────────────────────────
                            for (di, dest) in rule.destinations.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.add_space(4.0);
                                    ui.label(
                                        RichText::new("\u{2022}").size(12.0).color(theme::accent()),
                                    );
                                    ui.label(
                                        RichText::new(dest.display().to_string())
                                            .size(11.5)
                                            .monospace()
                                            .color(theme::text_primary()),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .button(RichText::new("x").size(10.0))
                                                .on_hover_text("Remove destination")
                                                .clicked()
                                            {
                                                remove_dest = Some((ri, di));
                                            }
                                        },
                                    );
                                });
                            }
                            if ui
                                .button(RichText::new("+ Add destination…").size(11.5))
                                .clicked()
                            {
                                action = PlanAction::AddDestination { rule_idx: ri };
                            }
                        });
                }

                if let Some(ri) = remove_rule {
                    self.draft.rules.remove(ri);
                }
                if let Some((ri, di)) = remove_dest {
                    if let Some(rule) = self.draft.rules.get_mut(ri) {
                        rule.destinations.remove(di);
                    }
                }

                if ui
                    .button(RichText::new("+ Add another key").size(12.0))
                    .clicked()
                {
                    self.draft.rules.push(SaveRule::default());
                }

                // ── Post-save command ────────────────────────────────
                ui.separator();
                ui.label(
                    RichText::new("After every save, run (optional)")
                        .size(12.0)
                        .color(theme::text_dim()),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.draft.post_save_command)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace)
                        .hint_text("e.g. rsync -a \"$SCHL8_SOURCE\" backup-host:notes/"),
                )
                .on_hover_text(
                    "Shell command run in the background after each successful \
                     save of this file — for backups, server uploads, git \
                     commits, etc. $SCHL8_SOURCE is the document path and \
                     $SCHL8_DESTINATIONS lists the written files (one per \
                     line). Only encrypted file paths are exposed — never \
                     document content.",
                );

                if let Some(err) = &self.error {
                    ui.label(RichText::new(err).size(12.0).color(theme::accent_red()));
                }

                ui.separator();
                ui.horizontal(|ui| {
                    let apply = egui::Button::new(
                        RichText::new("  Apply & Save Plan  ")
                            .size(13.0)
                            .color(theme::badge_text())
                            .strong(),
                    )
                    .fill(theme::badge_bg())
                    .corner_radius(theme::RADIUS);
                    if ui.add(apply).clicked() {
                        // A rule with destinations but no chosen key defaults
                        // to the file's original key — "no key" means "keep
                        // the key this file already uses".
                        if let Some(fpr) = self.original_recipients.first() {
                            let label = self
                                .keys
                                .iter()
                                .find(|k| k.fingerprint.eq_ignore_ascii_case(fpr))
                                .map(|k| k.uid.clone())
                                .unwrap_or_else(|| format!("file's key ({})", short_fpr(fpr)));
                            for r in &mut self.draft.rules {
                                if !r.has_key() && !r.destinations.is_empty() {
                                    r.key_fingerprint = fpr.clone();
                                    r.key_label = label.clone();
                                }
                            }
                        }
                        // Validate: every rule with destinations needs a key
                        // (only reachable when no original key exists to
                        // default to), and no destination may serve two
                        // different keys (the copies would overwrite each
                        // other and only the last key could read the file).
                        let incomplete = self
                            .draft
                            .rules
                            .iter()
                            .any(|r| !r.has_key() && !r.destinations.is_empty());
                        if incomplete {
                            self.error = Some(
                                "Choose a key for every destination (this file has no \
                                 original key to default to)"
                                    .to_string(),
                            );
                        } else if let Some(dup) =
                            crate::config::duplicate_destination(&self.draft.rules)
                        {
                            self.error = Some(format!(
                                "{} is a destination for more than one key — the copies \
                                 would overwrite each other. Give each key its own file",
                                dup.display()
                            ));
                        } else {
                            action = PlanAction::Apply(self.draft.clone());
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.open = false;
                    }
                    if ui
                        .button("Remove plan")
                        .on_hover_text("Delete this plan; Save reverts to re-encrypting in place")
                        .clicked()
                    {
                        action = PlanAction::Apply(SavePlan {
                            source: self.draft.source.clone(),
                            rules: Vec::new(),
                            ..Default::default()
                        });
                    }
                });
            });

        if matches!(action, PlanAction::Apply(_)) {
            self.open = false;
        } else {
            self.open = is_open && self.open;
        }
        action
    }
}

fn short_fpr(fpr: &str) -> String {
    if fpr.len() >= 16 {
        fpr[fpr.len() - 16..].to_string()
    } else {
        fpr.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECIPIENT: &str = "age1uwlr4jpxxu3q9v0wtlc8h2f6e72zwxsps05uwquf8jqa0f06p5cs82yjxq";

    /// Opening Save Options on an AGE file must preselect the identity it
    /// was decrypted with. AGE ciphertext carries no recipient, so without
    /// this the rule opened on "Choose a key…" and the user had to
    /// re-pick the key they had just unlocked with.
    #[test]
    fn age_document_seeds_its_own_identity() {
        let mut dlg = SaveTargetsDialog::new();
        let src = Path::new("/tmp/note.md.age");
        dlg.open_for(
            src,
            None,
            None, // AGE files expose no GPG recipients
            vec![(
                "This device (seed phrase)".to_string(),
                RECIPIENT.to_string(),
            )],
            Some(RECIPIENT),
        );

        assert_eq!(dlg.draft.rules.len(), 1);
        let rule = &dlg.draft.rules[0];
        assert!(rule.is_age(), "the seeded rule should use the AGE backend");
        assert_eq!(rule.age_recipient, RECIPIENT);
        assert_eq!(rule.destinations, vec![src.to_path_buf()]);
        assert!(
            rule.key_label.contains("This device"),
            "label should name the identity, got {:?}",
            rule.key_label
        );
    }

    /// A GPG file with no known recipients keeps the old behaviour: an
    /// empty rule the user (or the "file's own key" default) fills in.
    #[test]
    fn non_age_document_seeds_an_empty_rule() {
        let mut dlg = SaveTargetsDialog::new();
        let src = Path::new("/tmp/note.md.gpg");
        dlg.open_for(src, None, None, Vec::new(), None);

        assert_eq!(dlg.draft.rules.len(), 1);
        assert!(!dlg.draft.rules[0].has_key());
        assert_eq!(dlg.draft.rules[0].destinations, vec![src.to_path_buf()]);
    }
}
