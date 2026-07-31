use egui::{self, RichText, Ui};

/// Actions that the menu bar can trigger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    OpenFile,
    NewMarkdown,
    NewText,
    QuickNote,
    ManageQuickNotes,
    ManageFavorites,
    /// Lock the session immediately.
    PanicLock,
    /// Toggle the animated reading (crawl) mode.
    ToggleCrawl,
    /// Show a copyable instruction block for an AI assistant, by index
    /// into `agent_help::all()`.
    AgentHelp(usize),
    /// Symlink the binary onto PATH so the agent surface is reachable.
    InstallCliTool,
    /// Open the agent-toolkit window (make Schl8 permanent).
    AgentToolkit,
    Save,
    SaveTargets,
    EncryptAndSave,
    ToggleEdit,
    CloseDocument,
    Quit,
    ManageKeys,
    ImportKey,
    Settings,
    InstallHelp,
    ReportIssue,
    CheckForUpdates,
    About,
    ToggleStats,
    ToggleFocus,
    ToggleCopy,
    ToggleWrap,
    ToggleLineNumbers,
    Find,
    UnlockAge,
    ForgetAge,
    ExportAgePublicKey,
}

/// Which menu items are currently applicable.
#[derive(Clone, Copy)]
pub struct MenuFlags {
    pub has_document: bool,
    /// False for archive browsing, where in-place editing is unsupported.
    pub can_edit: bool,
    pub is_editing: bool,
    /// True when the document's original recipients are known, so Save
    /// can re-encrypt in place.
    pub can_save: bool,
    /// Checkmark states for the View menu.
    pub show_stats: bool,
    pub focus_mode: bool,
    pub allow_copy: bool,
    pub word_wrap: bool,
    pub line_numbers: bool,
    /// True when an age seed-phrase identity is held in memory.
    pub age_unlocked: bool,
    /// False when no gpg binary was found — the app runs age-only.
    pub gpg_available: bool,
}

/// Render the top menu bar. Returns an action if the user clicked a menu item.
pub fn render(ui: &mut Ui, flags: MenuFlags) -> Option<MenuAction> {
    let MenuFlags {
        has_document,
        can_edit,
        is_editing,
        can_save,
        show_stats,
        focus_mode,
        allow_copy,
        word_wrap,
        line_numbers,
        age_unlocked,
        gpg_available,
    } = flags;
    let mut action = None;

    egui::menu::bar(ui, |ui| {
        ui.menu_button(RichText::new("File").size(13.0), |ui| {
            if ui
                .add_enabled(true, egui::Button::new("Open…\t\t\tCmd+O"))
                .clicked()
            {
                action = Some(MenuAction::OpenFile);
                ui.close_menu();
            }

            ui.separator();

            if ui.button("New Markdown File\tCmd+N").clicked() {
                action = Some(MenuAction::NewMarkdown);
                ui.close_menu();
            }
            if ui.button("New Text File").clicked() {
                action = Some(MenuAction::NewText);
                ui.close_menu();
            }

            ui.separator();

            if ui.button("Quick Note…\t\tCmd+J").clicked() {
                action = Some(MenuAction::QuickNote);
                ui.close_menu();
            }
            if ui
                .button("Quick Note Files…")
                .on_hover_text("Manage the quicknote files shown in the menu-bar submenu")
                .clicked()
            {
                action = Some(MenuAction::ManageQuickNotes);
                ui.close_menu();
            }
            if ui
                .button("Favorites\u{2026}")
                .on_hover_text(
                    "Manage the files pinned to the menu-bar Favorites submenu, \
                     their order, and their hotkeys",
                )
                .clicked()
            {
                action = Some(MenuAction::ManageFavorites);
                ui.close_menu();
            }

            ui.separator();

            if ui
                .add_enabled(can_save, egui::Button::new("Save\t\t\tCmd+S"))
                .clicked()
            {
                action = Some(MenuAction::Save);
                ui.close_menu();
            }

            if ui
                .add_enabled(
                    has_document,
                    egui::Button::new("Encrypt & Save As…\tCmd+Shift+S"),
                )
                .clicked()
            {
                action = Some(MenuAction::EncryptAndSave);
                ui.close_menu();
            }

            if ui
                .add_enabled(can_edit, egui::Button::new("Save Options…"))
                .on_hover_text(
                    "Choose which key(s), destination(s), and post-save hook this \
                     file's Save uses",
                )
                .clicked()
            {
                action = Some(MenuAction::SaveTargets);
                ui.close_menu();
            }

            ui.separator();

            if ui.button("Settings…\t\tCmd+,").clicked() {
                action = Some(MenuAction::Settings);
                ui.close_menu();
            }

            ui.separator();

            if ui
                .add_enabled(has_document, egui::Button::new("Close\t\t\tCmd+W"))
                .clicked()
            {
                action = Some(MenuAction::CloseDocument);
                ui.close_menu();
            }
            if ui
                .add_enabled(has_document, egui::Button::new("Lock Now"))
                .on_hover_text(
                    "Lock the session immediately. Unsaved edits are encrypted to the \
                     document's own key first, so nothing is lost.",
                )
                .clicked()
            {
                action = Some(MenuAction::PanicLock);
                ui.close_menu();
            }

            if ui.button("Quit\t\t\tCmd+Q").clicked() {
                action = Some(MenuAction::Quit);
                ui.close_menu();
            }
        });

        ui.menu_button(RichText::new("Edit").size(13.0), |ui| {
            let label = if is_editing {
                "Exit Edit Mode\t\tCmd+E"
            } else {
                "Edit Document\t\tCmd+E"
            };
            if ui
                .add_enabled(has_document && can_edit, egui::Button::new(label))
                .clicked()
            {
                action = Some(MenuAction::ToggleEdit);
                ui.close_menu();
            }
            ui.separator();
            if ui
                .add_enabled(
                    has_document && can_edit,
                    egui::Button::new("Find & Replace…\tCmd+F"),
                )
                .clicked()
            {
                action = Some(MenuAction::Find);
                ui.close_menu();
            }
        });

        ui.menu_button(RichText::new("View").size(13.0), |ui| {
            let mut stats = show_stats;
            if ui.checkbox(&mut stats, "Statistics").clicked() {
                action = Some(MenuAction::ToggleStats);
                ui.close_menu();
            }
            let mut focus = focus_mode;
            if ui.checkbox(&mut focus, "Focus Mode\tCtrl+Cmd+F").clicked() {
                action = Some(MenuAction::ToggleFocus);
                ui.close_menu();
            }
            if ui
                .add_enabled(
                    has_document,
                    egui::Button::new("Crawl (auto-scroll)\tCmd+Shift+R"),
                )
                .on_hover_text(
                    "Scroll the document by itself so you can just read. Space pauses, \
                     Up/Down change speed, +/- change text size, R reverses, Esc exits.",
                )
                .clicked()
            {
                action = Some(MenuAction::ToggleCrawl);
                ui.close_menu();
            }
            ui.separator();
            let mut wrap = word_wrap;
            if ui.checkbox(&mut wrap, "Word Wrap").clicked() {
                action = Some(MenuAction::ToggleWrap);
                ui.close_menu();
            }
            let mut numbers = line_numbers;
            if ui
                .checkbox(&mut numbers, "Line Numbers")
                .on_hover_text(
                    "Left gutter with line numbers (plaintext view; in the \
                     editor when Word Wrap is off)",
                )
                .clicked()
            {
                action = Some(MenuAction::ToggleLineNumbers);
                ui.close_menu();
            }
            ui.separator();
            let mut copy = allow_copy;
            if ui
                .checkbox(&mut copy, "Allow Copying (this session)")
                .on_hover_text("Copying places decrypted text on the system clipboard")
                .clicked()
            {
                action = Some(MenuAction::ToggleCopy);
                ui.close_menu();
            }
        });

        ui.menu_button(RichText::new("Keys").size(13.0), |ui| {
            if ui.button("Manage Public Keys…").clicked() {
                action = Some(MenuAction::ManageKeys);
                ui.close_menu();
            }
            if ui
                .add_enabled(
                    gpg_available,
                    egui::Button::new("Import GPG Key from File…"),
                )
                .on_hover_text(if gpg_available {
                    "Import a GPG public key into your keyring"
                } else {
                    "GPG is not installed — Schl8 is running in AGE-only mode"
                })
                .clicked()
            {
                action = Some(MenuAction::ImportKey);
                ui.close_menu();
            }
            ui.separator();
            // ── age (seed-phrase) identity ─────────────────────────
            let unlock_label = if age_unlocked {
                "AGE Identity: unlocked"
            } else {
                "Unlock AGE Identity…"
            };
            if ui
                .button(unlock_label)
                .on_hover_text(
                    "Enter your 12-word seed phrase to hold the AGE private key in memory",
                )
                .clicked()
            {
                action = Some(MenuAction::UnlockAge);
                ui.close_menu();
            }
            if ui
                .add_enabled(age_unlocked, egui::Button::new("Forget AGE Identity"))
                .on_hover_text("Wipe the derived AGE private key from memory")
                .clicked()
            {
                action = Some(MenuAction::ForgetAge);
                ui.close_menu();
            }
            if ui
                .button("Export AGE Public Key…")
                .on_hover_text("Show/save your AGE recipient (age1…) — enter the seed phrase")
                .clicked()
            {
                action = Some(MenuAction::ExportAgePublicKey);
                ui.close_menu();
            }
        });

        ui.menu_button(RichText::new("Help").size(13.0), |ui| {
            // Ready-made briefings the user pastes into their assistant.
            // A submenu rather than one dialog entry, so the thing they
            // want is one hop away and readable in the menu itself.
            ui.menu_button("Instructions for your agent", |ui| {
                for (i, title) in super::agent_help::AgentHelp::titles()
                    .into_iter()
                    .enumerate()
                {
                    if ui.button(title).clicked() {
                        action = Some(MenuAction::AgentHelp(i));
                        ui.close_menu();
                    }
                    // The first entry hands over the whole guide; the
                    // rest are single tasks.
                    if i == 0 {
                        ui.separator();
                    }
                }
            });
            // Nothing in that submenu works until a shell can find
            // `schl8`, so the fix sits directly under it. The label
            // reports state, because "install" on an already-installed
            // tool reads as a mistake.
            let installed = matches!(
                crate::cli_install::status(),
                crate::cli_install::Status::Installed(_)
            );
            let label = if installed {
                "Command Line Tool Installed ✔"
            } else {
                "Install Command Line Tool…"
            };
            if ui
                .button(label)
                .on_hover_text(
                    "Links `schl8` into a folder on your PATH so agents and \
                     scripts can run it. Without this, every command in the \
                     briefings above fails with \"command not found\".",
                )
                .clicked()
            {
                action = Some(MenuAction::InstallCliTool);
                ui.close_menu();
            }
            if ui
                .button("Agent Toolkit\u{2026}")
                .on_hover_text(
                    "Make Schl8 a standing part of your assistant, so it works in \
                     every future conversation without being re-explained",
                )
                .clicked()
            {
                action = Some(MenuAction::AgentToolkit);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Install & Default Editor…").clicked() {
                action = Some(MenuAction::InstallHelp);
                ui.close_menu();
            }
            if ui
                .button("Report an Issue…")
                .on_hover_text("Open the project's GitHub issue tracker in your browser")
                .clicked()
            {
                action = Some(MenuAction::ReportIssue);
                ui.close_menu();
            }
            if ui
                .button("Check for Updates…")
                .on_hover_text("Ask github.com whether a newer release exists")
                .clicked()
            {
                action = Some(MenuAction::CheckForUpdates);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("About Schl8").clicked() {
                action = Some(MenuAction::About);
                ui.close_menu();
            }
        });
    });

    action
}
