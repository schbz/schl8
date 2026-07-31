//! Floating statistics card, toggled via View → Statistics.
//!
//! A compact, translucent panel pinned to the top-right of the content
//! area showing live counts plus file metadata. Only filenames/metadata
//! are shown — never content.

use std::path::Path;

use egui::{Align2, RichText};

use super::theme;
use crate::crypto::gpg::SignatureStatus;
use crate::document::stats::{compact, TextStats};
use crate::document::FileType;

/// File-level metadata rows (all optional so archive entries and unsaved
/// new documents can show what they have).
pub struct FileMeta<'a> {
    pub path: &'a Path,
    pub file_type: FileType,
    pub signature: Option<&'a SignatureStatus>,
    pub recipient_count: Option<usize>,
    /// False for archive entries and unsaved new files (no on-disk source).
    pub on_disk: bool,
}

/// Render the stats card overlay.
pub fn show(ctx: &egui::Context, stats: &TextStats, meta: &FileMeta<'_>) {
    egui::Area::new(egui::Id::new("stats_card"))
        .anchor(Align2::RIGHT_TOP, egui::vec2(-14.0, 44.0))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(theme::bg_raised().gamma_multiply(0.92))
                .stroke(egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.35)))
                .corner_radius(theme::RADIUS)
                .inner_margin(egui::Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.set_max_width(210.0);
                    ui.spacing_mut().item_spacing.y = 3.0;

                    ui.label(theme::gradient_text("Statistics", 13.0));
                    ui.add_space(2.0);

                    row(ui, "Words", &stats.words.to_string());
                    row(ui, "Characters", &stats.chars.to_string());
                    row(ui, "No spaces", &stats.chars_no_ws.to_string());
                    row(ui, "Lines", &stats.lines.to_string());
                    row(ui, "Reading", &stats.reading_label());

                    ui.add_space(3.0);
                    ui.separator();
                    ui.add_space(1.0);

                    row(
                        ui,
                        "Type",
                        match meta.file_type {
                            FileType::Markdown => "Markdown",
                            FileType::PlainText => "Plain text",
                        },
                    );

                    if let Some(n) = meta.recipient_count {
                        row(ui, "Recipients", &n.to_string());
                    }
                    if let Some(sig) = meta.signature {
                        let (label, color) = match sig {
                            SignatureStatus::Valid { .. } => ("verified", theme::accent_green()),
                            SignatureStatus::Invalid { .. } => ("INVALID", theme::accent_red()),
                            SignatureStatus::Unsigned => ("none", theme::text_dim()),
                        };
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Signature")
                                    .size(11.0)
                                    .color(theme::text_dim()),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(RichText::new(label).size(11.0).color(color));
                                },
                            );
                        });
                    }

                    // On-disk facts (ciphertext size + last save time).
                    if meta.on_disk {
                        if let Ok(fs_meta) = std::fs::metadata(meta.path) {
                            row(ui, "Encrypted size", &compact(fs_meta.len() as usize));
                            if let Ok(modified) = fs_meta.modified() {
                                let dt: chrono::DateTime<chrono::Local> = modified.into();
                                row(ui, "Saved", &dt.format("%Y-%m-%d %H:%M").to_string());
                            }
                        }
                    } else {
                        row(ui, "Saved", "not yet");
                    }
                });
        });
}

fn row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(11.0).color(theme::text_dim()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new(value)
                    .size(11.0)
                    .monospace()
                    .color(theme::text_primary()),
            );
        });
    });
}
