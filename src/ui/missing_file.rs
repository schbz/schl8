//! The missing-file alert: "it isn't there — but these are."
//!
//! Opening a file that has been deleted or moved used to fail silently
//! from the menu-bar submenus, which reads as the app being broken
//! rather than the file being gone. Worse, silence wastes what the
//! configuration already knows: save plans and quicknote rules fan each
//! save out to several destinations, so when the working copy vanishes
//! the other copies are usually still on disk, listed in config, one
//! click away.
//!
//! So this dialog says plainly that the file is not at its recorded
//! location, and then offers every surviving copy the configuration
//! knows about — computed by [`crate::config::Config::alternate_locations`],
//! which only returns paths that exist right now.

use std::path::{Path, PathBuf};

use egui::{Align2, RichText, Vec2};

use super::theme;

/// What the user chose, if anything.
pub enum MissingAction {
    None,
    /// Open this surviving copy instead.
    OpenAlternate(PathBuf),
}

#[derive(Default)]
pub struct MissingFileDialog {
    pub open: bool,
    /// The path that was asked for and is not there.
    path: PathBuf,
    /// Copies of the same file that do exist, per the config.
    alternates: Vec<PathBuf>,
}

impl MissingFileDialog {
    /// Show the alert for `path`, offering `alternates` (already
    /// filtered to files that exist).
    pub fn show(&mut self, path: PathBuf, alternates: Vec<PathBuf>) {
        self.path = path;
        self.alternates = alternates;
        self.open = true;
    }

    pub fn render(&mut self, ctx: &egui::Context) -> MissingAction {
        if !self.open {
            return MissingAction::None;
        }
        let mut action = MissingAction::None;
        let mut is_open = self.open;

        egui::Window::new("File Not Found")
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            // Above whatever launched it — same lesson as the AGE
            // dialog, which opened invisibly behind its parent window.
            .order(egui::Order::Foreground)
            .resizable(false)
            .collapsible(false)
            .default_width(theme::dialog_width(ctx, 480.0))
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
                    RichText::new("This file is no longer at its recorded location.")
                        .size(13.5)
                        .color(theme::text_primary())
                        .strong(),
                );
                ui.label(
                    RichText::new(self.path.display().to_string())
                        .size(11.5)
                        .monospace()
                        .color(theme::text_dim()),
                );
                ui.label(
                    RichText::new(
                        "It may have been deleted, moved, or be on a disk that is \
                         not connected. Schl8 never removes note files itself.",
                    )
                    .size(11.5)
                    .color(theme::text_dim()),
                );

                if self.alternates.is_empty() {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(
                            "No other copies are known. If this file had a save plan \
                             or quicknote rules writing it to more places, those \
                             copies would be offered here.",
                        )
                        .size(11.5)
                        .color(theme::text_dim()),
                    );
                } else {
                    ui.add_space(2.0);
                    ui.separator();
                    ui.label(
                        RichText::new(
                            "Your save options for this file list other copies, and \
                             these still exist:",
                        )
                        .size(12.5)
                        .color(theme::text_primary()),
                    );
                    for alt in &self.alternates {
                        ui.horizontal(|ui| {
                            if ui
                                .button(RichText::new("Open").size(12.0))
                                .on_hover_text("Open this copy instead")
                                .clicked()
                            {
                                action = MissingAction::OpenAlternate(alt.clone());
                            }
                            ui.label(
                                RichText::new(compact_path(alt))
                                    .size(11.5)
                                    .monospace()
                                    .color(theme::text_primary()),
                            )
                            .on_hover_text(alt.display().to_string());
                        });
                    }
                    ui.label(
                        RichText::new(
                            "A copy opened here keeps its own path — update the \
                             file's save options if the move is permanent.",
                        )
                        .size(11.0)
                        .color(theme::text_dim()),
                    );
                }

                ui.add_space(4.0);
                if ui.button("Close").clicked() {
                    action = MissingAction::None;
                    self.open = false;
                }
            });

        if !is_open {
            self.open = false;
        }
        if matches!(action, MissingAction::OpenAlternate(_)) {
            self.open = false;
        }
        action
    }
}

/// `~`-relative display so a deep home path doesn't blow the dialog wide.
fn compact_path(p: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        if let Ok(rest) = p.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    p.display().to_string()
}
