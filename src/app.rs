use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use eframe::egui;

use zeroize::Zeroize;

use crate::config::{self, Config};
use crate::crypto::keys;
use crate::crypto::secure_buf::{SecureBuffer, SecureString};
use crate::document::archive::ArchiveDocument;
use crate::document::naming::{
    ensure_text_extension, is_age_source, leaf_stem, source_is_binary, suggest_encrypted_name,
};
use crate::document::{detect_file_type_from_name, Document, FileType, LoadedDocument};
use crate::ui::stamp::{FileStampCache, RecentStamps};
use crate::ui::textnav::{byte_to_line, count_lines, find_matches};
use crate::ui::{dialogs, filetree, keybindings, menu, quicknote, statusbar, theme, viewer};

/// Result of a background decryption attempt.
type DecryptResult = Result<LoadedDocument, String>;

/// Application states.
enum State {
    /// No file loaded — show the file picker screen.
    FilePicker,
    /// A file has been selected and decryption is in progress.
    Decrypting {
        path: PathBuf,
        receiver: mpsc::Receiver<DecryptResult>,
    },
    /// Document loaded and ready to view (or edit).
    Viewing {
        doc: Document,
        scroll_offset: f32,
        lines_count: usize,
        current_line: usize,
        /// When Some, the user is in edit mode with a mutable buffer.
        edit_buffer: Option<SecureString>,
        /// True if edit_buffer content differs from the original.
        modified: bool,
    },
    /// A decrypted folder archive with a sidebar file tree. The selected
    /// entry can be edited; saving rebuilds the tar (preserving all other
    /// entries) and re-encrypts the whole archive in place.
    ViewingArchive {
        archive: ArchiveDocument,
        tree: filetree::TreeNode,
        selected: usize,
        /// Folder highlighted for folder-level operations (rename/delete),
        /// independent of the file being viewed.
        selected_dir: Option<String>,
        scroll_offset: f32,
        lines_count: usize,
        current_line: usize,
        /// When Some, the selected entry is being edited.
        edit_buffer: Option<SecureString>,
        /// True if edit_buffer content differs from the entry.
        modified: bool,
    },
    /// The session was auto-locked (idle/sleep/panic): all plaintext has
    /// been dropped and zeroized. `relock_path` is the file to re-open on
    /// unlock, when one was loaded; `held` describes any unsaved edits
    /// that were encrypted to disk on the way out.
    Locked {
        /// Set when locking discarded unsaved text it could not encrypt.
        warning: Option<String>,
        relock_path: Option<PathBuf>,
        held: Option<crate::document::stash::StashSummary>,
    },
    /// An error occurred — show it with options to retry or pick another file.
    Error {
        message: String,
        failed_path: PathBuf,
    },
}

/// Fresh, empty document opened directly in edit mode ("New File").
/// It has no on-disk source yet — the first Encrypt & Save names it and
/// chooses the encryption method (GPG or age).
/// The file to reopen after unlocking, if there is one.
///
/// A document that has never been saved carries a placeholder path —
/// `new_empty_document` invents `untitled.md.gpg` — that has never
/// existed on disk. Remembering it makes the unlock try to decrypt a
/// file that is not there, which surfaces as gpg's "No such file or
/// directory" and reads, to the person who just locked their screen,
/// like the app lost their work.
fn relock_target(source: &std::path::Path) -> Option<PathBuf> {
    source.exists().then(|| source.to_path_buf())
}

fn new_empty_document(file_type: FileType) -> State {
    let ext = match file_type {
        FileType::Markdown => "md",
        FileType::PlainText => "txt",
    };
    let doc = Document {
        content: SecureBuffer::from_bytes(Vec::new()),
        file_type,
        source_path: PathBuf::from(format!("untitled.{ext}.gpg")),
        recipients: None,
        signature: crate::crypto::gpg::SignatureStatus::Unsigned,
    };
    let edit_buffer = SecureString::from_secure_buffer(&doc.content).ok();
    State::Viewing {
        doc,
        scroll_offset: 0.0,
        lines_count: 1,
        current_line: 1,
        edit_buffer,
        modified: false,
    }
}

/// Count lines in a secure buffer for the status bar.
/// State of the find & replace bar (Cmd+F).
#[derive(Default)]
struct FindBar {
    open: bool,
    query: String,
    replace: String,
    /// Focus the query field on the next frame.
    want_focus: bool,
    /// 0-based index of the current match.
    active: usize,
}

/// Main application.
pub struct App {
    state: State,
    encrypt_dialog: dialogs::EncryptDialog,
    key_manager: dialogs::KeyManagerDialog,
    about_dialog: dialogs::AboutDialog,
    /// "A newer release exists" dialog with upgrade instructions.
    update_dialog: dialogs::UpdateDialog,
    /// Confirmation for permanently deleting spooled entries.
    discard_spool_dialog: dialogs::DiscardSpoolDialog,
    /// Prompt for adding/renaming a file inside an open vault.
    vault_prompt: dialogs::VaultPromptDialog,
    /// Pending vault delete awaiting confirmation.
    vault_confirm_delete: Option<VaultDeleteTarget>,
    /// (path, length, mtime) of the open document's file as it was when
    /// Schl8 loaded it or last wrote it. Compared before every
    /// overwrite so a save can't silently clobber a change made by
    /// another window, a sync client, or a `schl8 append` merge.
    /// None for documents with no file yet.
    source_identity: Option<(PathBuf, u64, std::time::SystemTime)>,
    /// Source path awaiting an overwrite-conflict decision.
    save_conflict: Option<PathBuf>,
    /// Set when the user confirms overwriting a changed file; consumed by
    /// the next save.
    force_overwrite: bool,
    /// Per-note filesystem state (pending spool count + whether the file
    /// still exists), refreshed on a timer. Stat/read_dir per note, so it
    /// must not run every frame; every list the user can pick a note from
    /// is built from this cache, so deleted files drop out of the UI
    /// within one scan interval.
    pending_cache: Vec<(PathBuf, usize, bool)>,
    /// egui-clock time of the last spool scan; `None` forces a rescan.
    pending_scanned_at: Option<f64>,
    /// In-flight update check (Help → Check for Updates…).
    update_rx: Option<std::sync::mpsc::Receiver<Result<crate::update::CheckOutcome, String>>>,
    install_help_dialog: dialogs::InstallHelpDialog,
    cli_tool_dialog: dialogs::CliToolDialog,
    toolkit_dialog: dialogs::ToolkitDialog,
    backup_dialog: dialogs::BackupDialog,
    uninstall_dialog: dialogs::UninstallDialog,
    settings_dialog: crate::ui::settings::SettingsDialog,
    discard_dialog: dialogs::DiscardDialog,
    quit_dialog: dialogs::QuitDialog,
    /// When true, exit edit mode after a successful encrypt & save.
    exit_edit_after_save: bool,
    /// When true, the next window close request proceeds without confirmation.
    allow_close: bool,
    /// Toast-style notification shown briefly after operations.
    toast: Option<(String, bool, f64)>, // (message, is_error, show_until_time)
    /// Persistent settings (quick-note targets, templates, hotkey).
    config: Config,
    /// Quick-note window state.
    jot: quicknote::JotWindow,
    /// Background quick-note append in flight.
    jot_rx: Option<mpsc::Receiver<Result<(), String>>>,
    /// Menu-bar status item + global hotkey (None until initialized, or
    /// when residency is disabled/unavailable).
    resident: Option<crate::tray::Resident>,
    resident_failed: bool,
    /// True once the user explicitly chose Quit — the next close request
    /// exits instead of hiding to the menu bar.
    quit_requested: bool,
    /// egui time (seconds) of the last observed user activity, for the
    /// idle auto-lock. `None` until the first frame sets it.
    last_activity: Option<f64>,
    /// Cache for the live text-statistics computation.
    stats_cache: crate::document::stats::StatsCache,
    /// Cache of the opened file's on-disk hash + mtime for the status bar.
    file_stamp: FileStampCache,
    /// Whether a usable `gpg` binary was found at startup. When false the
    /// app runs in age-only mode: GPG UI is hidden/disabled and Encrypt &
    /// Save defaults to age.
    gpg_available: bool,
    /// Show the "running in age-only mode" hint once, on the first frame.
    gpg_hint_pending: bool,
    /// The age seed-phrase identity, held in mlock'd memory while unlocked.
    /// `None` until the user unlocks; cleared on quit or "Forget".
    age_identity: Option<crate::crypto::age_backend::AgeIdentity>,
    /// The age unlock / export-public-key dialog.
    age_dialog: crate::ui::age_dialog::AgeDialog,
    /// When the AGE identity was unlocked, on the egui input clock — used
    /// for the "forget N minutes after unlocking" ceiling.
    age_unlocked_at: Option<f64>,
    /// A quicknote submit deferred until the AGE identity is unlocked.
    jot_pending_unlock: bool,
    /// An age file waiting to open until the identity is unlocked.
    pending_age_open: Option<PathBuf>,
    /// Per-path stamp cache for the picker's recents list.
    recent_stamps: RecentStamps,
    /// Find & replace bar state.
    find: FindBar,
    /// One-shot absolute scroll target (find navigation), consumed by the
    /// viewer/editor on the next frame.
    pending_jump: Option<f32>,
    /// Last frame's (content height, viewport height) of the text view,
    /// used to convert a match's line into a scroll offset.
    view_metrics: (f32, f32, f32),
    /// Focus mode: fullscreen, chrome hidden, readable column.
    focus_mode: bool,
    /// Animated reading mode (see ui/crawl.rs).
    crawl: crate::ui::crawl::Crawl,
    /// Frame time of the previous update, for the crawl's dt.
    last_frame_time: Option<f64>,
    /// Debug preview: start crawling once the document has loaded.
    #[cfg(debug_assertions)]
    crawl_on_launch: bool,
    /// Whether copying decrypted text to the clipboard is allowed this
    /// session (seeded from config.security.allow_copy_default).
    allow_copy: bool,
    /// The copy-enable security warning dialog.
    copy_warning: dialogs::CopyWarningDialog,
    /// Per-file save-plan editor (File → Save Targets…).
    save_targets: crate::ui::save_targets::SaveTargetsDialog,
    /// Quicknote-registry editor (File → Quick Note Files…, tray).
    quicknotes_manager: crate::ui::quicknotes_manager::QuickNotesManager,
    favorites_manager: crate::ui::favorites_manager::FavoritesManager,
    agent_help: crate::ui::agent_help::AgentHelp,
    /// Set by the tray "Manage Quick Notes…" item; opens the manager on
    /// the next frame (after the main window is visible again).
    open_quicknotes_manager: bool,
    open_favorites_manager: bool,
    /// Held edits decrypted and waiting for their document to finish
    /// opening, so they can go back into the editor.
    pending_restore: Option<crate::document::stash::HeldEdits>,
    /// Set when a restore needs the AGE identity: retried once unlocked.
    restore_after_unlock: bool,
    /// A menu-bar request to open or create a document, staged until the
    /// frame's transition is built (the tray is polled before that point).
    pending_tray_action: Option<Transition>,
    /// Whether the jot window was open last frame (to persist its
    /// geometry once, when it closes).
    jot_was_open: bool,
    /// Last observed jot window geometry (origin, inner size).
    jot_last_geometry: Option<(egui::Pos2, egui::Vec2)>,
    /// True once the deferred-auto-lock toast has been shown for the
    /// current stretch of unsaved work (reset when edits are saved or
    /// discarded).
    lock_deferred_notified: bool,
    /// Intended visibility of the main window (independent of the jot
    /// window). The jot floats in its own window and hides the main one
    /// while open; this remembers what to restore to.
    main_visible: bool,
    /// True while the main window is hidden specifically because the jot
    /// window is open.
    main_hidden_for_jot: bool,
}

impl App {
    pub fn new(initial_file: Option<PathBuf>) -> Self {
        let state = match initial_file {
            Some(path) => {
                let receiver = spawn_decrypt(path.clone());
                State::Decrypting { path, receiver }
            }
            None => State::FilePicker,
        };
        let config = Config::load();
        let config_allow_copy = config.security.allow_copy_default;
        let jot = quicknote::JotWindow::new(&config.quick_note);
        App {
            state,
            encrypt_dialog: dialogs::EncryptDialog::new(),
            key_manager: dialogs::KeyManagerDialog::new(),
            about_dialog: dialogs::AboutDialog::new(),
            update_dialog: dialogs::UpdateDialog::new(),
            discard_spool_dialog: dialogs::DiscardSpoolDialog::new(),
            vault_prompt: dialogs::VaultPromptDialog::new(),
            vault_confirm_delete: None,
            source_identity: None,
            save_conflict: None,
            force_overwrite: false,
            pending_cache: Vec::new(),
            pending_scanned_at: None,
            update_rx: None,
            install_help_dialog: dialogs::InstallHelpDialog::new(),
            cli_tool_dialog: dialogs::CliToolDialog::new(),
            toolkit_dialog: dialogs::ToolkitDialog::new(),
            backup_dialog: dialogs::BackupDialog::new(),
            uninstall_dialog: dialogs::UninstallDialog::new(),
            settings_dialog: crate::ui::settings::SettingsDialog::new(),
            discard_dialog: dialogs::DiscardDialog::new(),
            quit_dialog: dialogs::QuitDialog::new(),
            exit_edit_after_save: false,
            allow_close: false,
            toast: None,
            config,
            jot,
            jot_rx: None,
            resident: None,
            resident_failed: false,
            quit_requested: false,
            last_activity: None,
            stats_cache: crate::document::stats::StatsCache::default(),
            file_stamp: FileStampCache::default(),
            recent_stamps: RecentStamps::default(),
            gpg_available: crate::crypto::gpg::gpg_available(),
            gpg_hint_pending: !crate::crypto::gpg::gpg_available(),
            age_identity: None,
            age_dialog: crate::ui::age_dialog::AgeDialog::new(),
            age_unlocked_at: None,
            jot_pending_unlock: false,
            pending_age_open: None,
            find: FindBar::default(),
            pending_jump: None,
            view_metrics: (0.0, 0.0, 0.0),
            focus_mode: false,
            crawl: crate::ui::crawl::Crawl::default(),
            last_frame_time: None,
            #[cfg(debug_assertions)]
            crawl_on_launch: false,
            allow_copy: config_allow_copy,
            copy_warning: dialogs::CopyWarningDialog::new(),
            save_targets: crate::ui::save_targets::SaveTargetsDialog::new(),
            quicknotes_manager: crate::ui::quicknotes_manager::QuickNotesManager::new(),
            favorites_manager: crate::ui::favorites_manager::FavoritesManager::new(),
            agent_help: crate::ui::agent_help::AgentHelp::new(),
            open_quicknotes_manager: false,
            open_favorites_manager: false,
            pending_restore: None,
            restore_after_unlock: false,
            pending_tray_action: None,
            jot_was_open: false,
            jot_last_geometry: None,
            lock_deferred_notified: false,
            main_visible: true,
            main_hidden_for_jot: false,
        }
    }

    /// Debug-only: start directly in the viewer with an embedded sample
    /// markdown document, for UI development without decrypting anything.
    #[cfg(debug_assertions)]
    pub fn new_sample() -> Self {
        use crate::crypto::secure_buf::SecureBuffer;
        use crate::document::FileType;

        const SAMPLE: &str = include_str!("../assets/sample.md");
        let content = SecureBuffer::from_bytes(SAMPLE.as_bytes().to_vec());
        let lines_count = SAMPLE.lines().count();

        let mut app = App::new(None);
        app.state = State::Viewing {
            doc: Document {
                content,
                file_type: FileType::Markdown,
                source_path: PathBuf::from("sample.md.gpg"),
                recipients: None,
                // Sample shows the verified-signature badge for UI preview.
                signature: crate::crypto::gpg::SignatureStatus::Valid {
                    signer: "Sample Signer <sample@example.com>".to_string(),
                },
            },
            scroll_offset: 0.0,
            lines_count,
            current_line: 1,
            edit_buffer: None,
            modified: false,
        };
        app
    }

    /// Debug-only: start directly in the archive browser with embedded
    /// sample files, for UI development without decrypting anything.
    #[cfg(debug_assertions)]
    pub fn new_sample_archive() -> Self {
        use crate::document::archive::ArchiveEntry;

        const SAMPLE: &str = include_str!("../assets/sample.md");
        let files: &[(&str, &str)] = &[
            ("vault/README.md", SAMPLE),
            (
                "vault/notes/meeting-notes.md",
                "# Meeting Notes\n\n## 2026-07-01\n\n- Discussed **roadmap**\n- Next: `directory browsing`\n",
            ),
            (
                "vault/notes/deep/nested/todo.txt",
                "buy milk\nrotate keys\nfix the bug\n",
            ),
            ("vault/servers.txt", "host: example.com\nport: 22\n"),
        ];

        let mut entries: Vec<ArchiveEntry> = files
            .iter()
            .map(|(path, content)| ArchiveEntry {
                rel_path: path.to_string(),
                file_type: detect_file_type_from_name(path).unwrap_or(FileType::PlainText),
                content: SecureBuffer::from_bytes(content.as_bytes().to_vec()),
            })
            .collect();
        entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

        // Build a real in-memory tar so the edit flow works in the sample.
        let mut builder = tar::Builder::new(Vec::new());
        for (path, content) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, path, content.as_bytes())
                .expect("sample tar");
        }
        let raw_tar = SecureBuffer::from_bytes(builder.into_inner().expect("sample tar"));

        let archive = ArchiveDocument {
            source_path: PathBuf::from("vault.tar.gz.gpg"),
            entries,
            raw_tar,
            gzip: true,
            dirs: Vec::new(),
            recipients: None,
            hidden: Default::default(),
        };
        let tree = filetree::build_tree(&archive.entries, &archive.dirs);
        let lines_count = count_lines(&archive.entries[0].content);

        let mut app = App::new(None);
        app.state = State::ViewingArchive {
            archive,
            tree,
            selected: 0,
            selected_dir: None,
            scroll_offset: 0.0,
            lines_count,
            current_line: 1,
            edit_buffer: None,
            modified: false,
        };
        app
    }

    /// Debug-only: open the quick-note window on launch (UI preview).
    #[cfg(debug_assertions)]
    pub fn open_jot_on_launch(&mut self) {
        self.jot.show();
    }

    /// Debug-only: start on the locked screen (UI preview).
    #[cfg(debug_assertions)]
    pub fn start_locked_preview(&mut self) {
        self.state = State::Locked {
            relock_path: Some(PathBuf::from("notes.md.gpg")),
            held: None,
            warning: None,
        };
    }

    /// Debug-only: begin crawling as soon as the document is open, so
    /// the animation can be checked without driving the UI by hand.
    #[cfg(debug_assertions)]
    pub fn start_crawl_on_launch(&mut self) {
        self.crawl_on_launch = true;
    }

    /// Debug-only: open the Settings window on launch (UI preview).
    #[cfg(debug_assertions)]
    pub fn open_settings_on_launch(&mut self) {
        self.settings_dialog.open(&self.config);
    }

    /// Debug-only: open the Save Targets window on launch (UI preview),
    /// seeded with a fake document path and the real keyring.
    #[cfg(debug_assertions)]
    pub fn open_save_targets_on_launch(&mut self) {
        self.save_targets.open_for(
            std::path::Path::new("/tmp/sample.md.gpg"),
            None,
            None,
            Vec::new(),
            None,
        );
    }

    /// True when a document (single or archive) is currently open.
    fn document_open(&self) -> bool {
        matches!(
            self.state,
            State::Viewing { .. } | State::ViewingArchive { .. }
        )
    }

    /// True when there are unsaved edits that auto-lock must not silently
    /// discard.
    fn has_unsaved_edits(&self) -> bool {
        matches!(
            self.state,
            State::Viewing {
                edit_buffer: Some(_),
                modified: true,
                ..
            } | State::ViewingArchive {
                edit_buffer: Some(_),
                modified: true,
                ..
            }
        )
    }

    /// Public key(s) the open document's unsaved edits can be stashed to,
    /// and which backend they belong to.
    ///
    /// Encrypting needs only the public half, so this never prompts. A
    /// configured save plan wins (it is the user's explicit statement of
    /// which keys this file belongs to); otherwise an age document goes to
    /// the unlocked identity's own recipient and a GPG document to the
    /// recipients recorded when it was decrypted.
    ///
    /// None means the edits cannot be stashed — a brand-new document with
    /// no key yet, or an age document whose identity has already been
    /// forgotten. Callers fall back to deferring the lock rather than
    /// destroying the text.
    fn stash_recipients(&self) -> Option<(Vec<String>, crate::document::spool::SegmentFormat)> {
        use crate::document::spool::SegmentFormat;

        // An explicit override wins over everything: the user has said
        // which key holds their in-progress work, for every file.
        if let Some(fixed) = self.config.security.stash_key.fixed_recipient() {
            return Some(fixed);
        }

        let (source, doc_recipients) = match &self.state {
            State::Viewing { doc, .. } => (doc.source_path.clone(), doc.recipients.clone()),
            State::ViewingArchive { archive, .. } => {
                (archive.source_path.clone(), archive.recipients.clone())
            }
            _ => return None,
        };

        // The backend follows the DOCUMENT, not whatever the save plan
        // happens to list first. A GPG file whose plan also fans out to an
        // age destination is still a GPG file: stashing it to age would
        // demand a seed phrase to recover edits to a document that only
        // ever needed the GPG key.
        let want_age = is_age_source(&source);

        if let Some(plan) = self.config.plan_for(&source) {
            if want_age {
                let age: Vec<String> = plan
                    .rules
                    .iter()
                    .filter(|r| r.is_age())
                    .map(|r| r.age_recipient.clone())
                    .collect();
                if !age.is_empty() {
                    return Some((age, SegmentFormat::Age));
                }
            } else {
                let gpg: Vec<String> = plan
                    .rules
                    .iter()
                    .filter(|r| !r.key_fingerprint.is_empty())
                    .map(|r| r.key_fingerprint.clone())
                    .collect();
                if !gpg.is_empty() {
                    return Some((gpg, SegmentFormat::Gpg));
                }
            }
        }

        if want_age {
            // age files record no recipient, so the only key we can be
            // sure of is the identity currently unlocked.
            let id = self.age_identity.as_ref()?;
            return Some((vec![id.recipient().to_string()], SegmentFormat::Age));
        }

        let recips = doc_recipients?;
        if recips.is_empty() {
            return None;
        }
        Some((recips, SegmentFormat::Gpg))
    }

    /// Encrypt unsaved work to the document's own public key and write it
    /// beside the config, so the session can really lock without losing it.
    ///
    /// Returns Ok(false) when there was nothing to hold. An Err means the
    /// edits could NOT be secured — callers must then keep deferring the
    /// lock, because locking would destroy them.
    fn stash_unsaved_work(&mut self) -> anyhow::Result<bool> {
        use crate::document::stash;

        let jot_text = self.jot.text().to_string();
        let jot_body = (!jot_text.trim().is_empty()).then(|| jot_text.clone());
        let doc_body = match &self.state {
            State::Viewing {
                edit_buffer: Some(buf),
                modified: true,
                ..
            } => Some(buf.as_bytes().to_vec()),
            State::ViewingArchive {
                edit_buffer: Some(buf),
                modified: true,
                ..
            } => Some(buf.as_bytes().to_vec()),
            _ => None,
        };
        if doc_body.is_none() && jot_body.is_none() {
            return Ok(false);
        }

        let (recipients, format) = self
            .stash_recipients()
            .ok_or_else(|| anyhow::anyhow!("this document has no key to secure the edits with"))?;
        let refs: Vec<&str> = recipients.iter().map(String::as_str).collect();

        let meta = stash::StashMeta {
            source: match &self.state {
                State::Viewing { doc, .. } => Some(doc.source_path.clone()),
                State::ViewingArchive { archive, .. } => Some(archive.source_path.clone()),
                _ => None,
            },
            entry: match &self.state {
                State::ViewingArchive {
                    archive, selected, ..
                } => archive.entries.get(*selected).map(|e| e.rel_path.clone()),
                _ => None,
            },
            jot_target: jot_body.as_ref().and(self.jot.selected_target.clone()),
        };

        let written = chrono::Utc::now().to_rfc3339();
        // The envelope is plaintext, so it lives in a SecureBuffer and is
        // handed straight to the encrypter — it is never written as-is.
        let envelope = stash::envelope(
            &written,
            &meta,
            doc_body.as_deref(),
            jot_body.as_ref().map(|s| s.as_bytes()),
        );
        let ciphertext = match format {
            crate::document::spool::SegmentFormat::Age => {
                crate::crypto::age_backend::encrypt_to_recipients(envelope.as_bytes(), &refs)
            }
            crate::document::spool::SegmentFormat::Gpg => {
                crate::crypto::keys::encrypt_to_bytes(envelope.as_bytes(), &refs, false)
            }
        };
        let mut doc_body = doc_body;
        if let Some(b) = doc_body.as_mut() {
            b.zeroize();
        }
        let mut jot_text = jot_text;
        jot_text.zeroize();
        stash::write(&ciphertext?, format)?;
        Ok(true)
    }

    /// Paint the crawl's edge fade and its transient control hints on a
    /// layer above the document.
    ///
    /// The fade is drawn rather than achieved with a shader: lines should
    /// arrive and leave softly instead of being chopped off at the window
    /// edge, and a stack of translucent bands over the theme background
    /// does that with no dependency on how the text was rendered.
    fn paint_crawl_overlay(&self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("crawl_overlay"),
        ));

        if self.config.crawl.fade_edges {
            let bg = theme::bg_primary();
            let band = (screen.height() * 0.16).clamp(40.0, 180.0);
            const STEPS: usize = 24;
            for i in 0..STEPS {
                let t = i as f32 / STEPS as f32;
                let h = band / STEPS as f32;
                // Opaque at the very edge, clear where the text is read.
                let alpha = ((1.0 - t) * 235.0) as u8;
                let c = egui::Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), alpha);
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(screen.left(), screen.top() + t * band),
                        egui::pos2(screen.right(), screen.top() + t * band + h),
                    ),
                    0.0,
                    c,
                );
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(screen.left(), screen.bottom() - t * band - h),
                        egui::pos2(screen.right(), screen.bottom() - t * band),
                    ),
                    0.0,
                    c,
                );
            }
        }

        let now = ctx.input(|i| i.time);
        if !self.config.crawl.show_hud || !self.crawl.hud_visible(now) {
            return;
        }
        // A single centred pill, low enough not to sit in the reading
        // line, and it fades out on its own so it never becomes chrome.
        let text = if self.crawl.paused {
            format!("\u{23F8} {}", self.crawl.hud_msg)
        } else {
            self.crawl.hud_msg.clone()
        };
        let galley =
            painter.layout_no_wrap(text, egui::FontId::proportional(14.0), theme::badge_text());
        let pad = egui::vec2(14.0, 8.0);
        let size = galley.size() + pad * 2.0;
        let pos = egui::pos2(
            screen.center().x - size.x / 2.0,
            screen.bottom() - size.y - 36.0,
        );
        let rect = egui::Rect::from_min_size(pos, size);
        painter.rect_filled(rect, 8.0, theme::badge_bg().gamma_multiply(0.92));
        painter.galley(rect.min + pad, galley, theme::badge_text());
    }

    /// Enter or leave crawl mode.
    fn toggle_crawl(&mut self, ctx: &egui::Context) {
        if self.crawl.active {
            self.end_crawl(ctx);
            return;
        }
        // Only meaningful with a document to read, and never over an
        // editor — the crawl moves the view out from under the caret.
        if !self.document_open() || self.is_editing() {
            self.show_toast(
                "Open a document (and leave edit mode) to start crawling".to_string(),
                true,
                ctx,
            );
            return;
        }
        let now = ctx.input(|i| i.time);
        let base_zoom = self.config.appearance.font_scale;
        self.crawl
            .start(&self.config.crawl, self.view_metrics.2, now, base_zoom);
        // Text size is applied as interface zoom: the chrome is hidden
        // while crawling, so scaling everything is invisible and it works
        // for rendered markdown and plaintext alike, which per-style
        // sizing would not.
        theme::apply_font_scale(ctx, base_zoom * self.crawl.text_scale);
        if self.config.crawl.fullscreen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
    }

    fn end_crawl(&mut self, ctx: &egui::Context) {
        if !self.crawl.active {
            return;
        }
        let restore = self.crawl.restore_zoom;
        self.crawl.stop();
        theme::apply_font_scale(ctx, restore);
        if self.config.crawl.fullscreen && !self.focus_mode {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
    }

    /// Advance the crawl and handle its live controls. No-op when idle.
    fn drive_crawl(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let dt = self.last_frame_time.map(|t| now - t).unwrap_or(0.0);
        self.last_frame_time = Some(now);
        if !self.crawl.active {
            return;
        }
        // Editing while crawling would fight over the viewport, so the
        // crawl yields.
        if self.is_editing() || !self.document_open() {
            self.end_crawl(ctx);
            return;
        }

        // ── Live controls ────────────────────────────────────────────
        let mut scale_changed = false;
        let (mut quit, mut toggled) = (false, false);
        ctx.input(|i| {
            for ev in &i.events {
                let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = ev
                else {
                    continue;
                };
                // Modified combos belong to the app's own shortcuts.
                if modifiers.command || modifiers.ctrl || modifiers.alt {
                    continue;
                }
                match key {
                    egui::Key::Space => toggled = true,
                    egui::Key::Escape | egui::Key::Q => quit = true,
                    egui::Key::ArrowUp => self.crawl.nudge_speed(true, now),
                    egui::Key::ArrowDown => self.crawl.nudge_speed(false, now),
                    egui::Key::Plus | egui::Key::Equals => {
                        self.crawl.nudge_scale(true, now);
                        scale_changed = true;
                    }
                    egui::Key::Minus => {
                        self.crawl.nudge_scale(false, now);
                        scale_changed = true;
                    }
                    egui::Key::R => self.crawl.reverse(now),
                    // Jump to either end without leaving the crawl.
                    egui::Key::Home => self.crawl.offset = 0.0,
                    egui::Key::End => self.crawl.offset = self.view_metrics.0 as f64,
                    _ => {}
                }
            }
        });
        if toggled {
            self.crawl.toggle_pause(now);
        }
        if scale_changed {
            theme::apply_font_scale(ctx, self.crawl.restore_zoom * self.crawl.text_scale);
        }
        if quit {
            self.end_crawl(ctx);
            return;
        }

        // ── Manual scrolling wins ────────────────────────────────────
        // Any wheel/trackpad movement adopts the reader's position, so
        // the next frame continues from there instead of snapping back.
        let scrolled = ctx.input(|i| i.raw_scroll_delta.y.abs() > 0.5);
        if scrolled {
            self.crawl
                .user_scrolled(self.view_metrics.2, now, &self.config.crawl);
        }

        self.crawl.tick_resume(now);

        let (content_h, viewport_h, _) = self.view_metrics;
        // The metrics come from the previous frame's render, so on the
        // first frame after starting they are still zero. Stepping now
        // would read as "a document with nothing to scroll" and stop the
        // crawl before it ever moved — which is exactly what it did.
        if content_h <= 0.0 {
            ctx.request_repaint();
            return;
        }
        let max_scroll = (content_h - viewport_h).max(0.0);

        // While the reader has the view — paused, or just after a wheel
        // event — track their position rather than our own, so resuming
        // continues from where they actually are instead of snapping
        // back to where the crawl left off.
        if !self.crawl.drives_scroll(now) {
            self.crawl.offset = self.view_metrics.2 as f64;
            // A frame is still needed to notice the hold expiring.
            if self.crawl.active && !self.crawl.paused {
                ctx.request_repaint();
            }
            return;
        }

        match self.crawl.step(dt, max_scroll, &self.config.crawl) {
            crate::ui::crawl::Step::Finished => {
                // Only "stop" gets here; reverse and loop keep moving.
                // Say why, because a silently frozen crawl reads as a bug
                // — and Space or R will start it again.
                if !self.crawl.paused {
                    self.crawl.paused = true;
                    self.crawl.note(
                        "End of document \u{2014} Space or R to keep going".to_string(),
                        now,
                    );
                }
            }
            crate::ui::crawl::Step::Reversed => {
                self.crawl.note(
                    if self.crawl.direction_up {
                        "Turned around \u{2014} going forward".to_string()
                    } else {
                        "Turned around \u{2014} going back".to_string()
                    },
                    now,
                );
            }
            crate::ui::crawl::Step::Looped => self.crawl.note("Looped".to_string(), now),
            crate::ui::crawl::Step::Running => {}
        }

        // Animation needs a frame even with no input.
        if !self.crawl.paused {
            ctx.request_repaint();
        }
    }

    /// Start opening `path`, choosing the age or background-GPG route.
    /// Shared by the transition handler and the restore flow.
    fn begin_open(&mut self, path: PathBuf, ctx: &egui::Context) {
        if crate::document::loader::is_age_file(&path) {
            self.open_age_file(&path, ctx);
        } else {
            let receiver = spawn_decrypt(path.clone());
            self.state = State::Decrypting { path, receiver };
        }
    }

    /// Decrypt the held edits and reopen the document they belong to.
    ///
    /// The stash file is deliberately NOT deleted here — only once the
    /// text is back in the editor. If the document fails to open, or the
    /// app dies in between, the held copy is still on disk.
    fn restore_held_edits(&mut self, ctx: &egui::Context) {
        use crate::document::spool::SegmentFormat;
        let Some(summary) = crate::document::stash::find() else {
            self.show_toast("There are no held edits to restore".to_string(), true, ctx);
            return;
        };

        // age needs the in-memory identity; ask for it and come back.
        if summary.format == SegmentFormat::Age && self.age_identity.is_none() {
            self.restore_after_unlock = true;
            self.age_dialog.show_unlock();
            self.show_toast(
                "Enter your seed phrase to restore the held edits".to_string(),
                false,
                ctx,
            );
            return;
        }

        let held = match crate::document::stash::read(
            &summary.path,
            summary.format,
            self.age_identity.as_ref(),
        ) {
            Ok(h) => h,
            Err(e) => {
                self.show_toast(format!("Could not restore held edits: {e:#}"), true, ctx);
                return;
            }
        };

        let source = held.source.clone();
        self.pending_restore = Some(held);
        match source {
            Some(path) => self.begin_open(path, ctx),
            None => {
                // The edits belonged to a document that had never been
                // saved, so there is nothing to reopen — start a fresh one
                // and let the applier fill it in.
                self.state = new_empty_document(FileType::Markdown);
            }
        }
    }

    /// Put decrypted held edits back into the editor once its document is
    /// open. Runs every frame; a no-op until both sides are ready.
    fn apply_pending_restore(&mut self, ctx: &egui::Context) {
        if self.pending_restore.is_none() {
            return;
        }
        // Wait until the document has actually loaded.
        if matches!(self.state, State::Decrypting { .. }) {
            return;
        }
        let Some(held) = self.pending_restore.take() else {
            return;
        };

        let mut restored_doc = false;
        if let Some(text) = &held.doc {
            match &mut self.state {
                State::Viewing {
                    edit_buffer,
                    modified,
                    lines_count,
                    ..
                } => {
                    if let Ok(buf) = SecureString::from_secure_buffer(text) {
                        *lines_count = buf.as_str().lines().count();
                        *edit_buffer = Some(buf);
                        *modified = true;
                        restored_doc = true;
                    }
                }
                State::ViewingArchive {
                    archive,
                    selected,
                    edit_buffer,
                    modified,
                    lines_count,
                    ..
                } => {
                    // Put the edit back on the entry it came from, not
                    // whichever one the browser happens to select first.
                    if let Some(entry) = &held.entry {
                        if let Some(idx) = archive.entries.iter().position(|e| &e.rel_path == entry)
                        {
                            *selected = idx;
                        }
                    }
                    if let Ok(buf) = SecureString::from_secure_buffer(text) {
                        *lines_count = buf.as_str().lines().count();
                        *edit_buffer = Some(buf);
                        *modified = true;
                        restored_doc = true;
                    }
                }
                _ => {
                    // The document could not be reopened; keep the stash so
                    // the edits are not lost and say so.
                    self.show_toast(
                        "Held edits are still saved — reopen the document to restore them"
                            .to_string(),
                        true,
                        ctx,
                    );
                    self.pending_restore = Some(held);
                    return;
                }
            }
        }

        let mut restored_jot = false;
        if let Some(text) = &held.jot {
            if let Ok(s) = text.as_str() {
                self.jot.selected_target = held.jot_target.clone();
                self.jot.set_text(s);
                self.jot.show();
                restored_jot = true;
            }
        }

        // Only now is the plaintext genuinely back in the app, so the
        // encrypted copy can go.
        crate::document::stash::clear();
        // The time comes from inside the envelope, not the file's mtime:
        // a copy or a sync client can rewrite an mtime, but the envelope
        // says when the edits were actually taken.
        let when = chrono::DateTime::parse_from_rfc3339(&held.saved)
            .map(|t| {
                t.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|_| held.saved.clone());
        let msg = match (restored_doc, restored_jot) {
            (true, true) => format!("Restored your unsaved edits and quick note from {when}"),
            (true, false) => format!(
                "Restored your unsaved edits from {when} \u{2014} still unsaved, so save \
                 when ready"
            ),
            (false, true) => format!("Restored your unsaved quick note from {when}"),
            (false, false) => "Nothing to restore".to_string(),
        };
        self.show_toast(msg, false, ctx);
    }

    /// Lock the session: drop (zeroize) all plaintext and show the locked
    /// screen, remembering the file to re-open on unlock. Also drops any
    /// unsaved jot text. No-op if nothing is open.
    fn lock_session(&mut self, ctx: &egui::Context) {
        let relock_path = match &self.state {
            State::Viewing { doc, .. } => Some(doc.source_path.clone()),
            State::ViewingArchive { archive, .. } => Some(archive.source_path.clone()),
            _ => None,
        }
        .and_then(|p| relock_target(&p));
        // Secure any unsaved work before the plaintext is dropped. A
        // failure here is reported but does not stop the lock: callers
        // only reach this point having decided the session must lock, and
        // the alternative — staying unlocked — is the worse outcome.
        // (`can_secure_unsaved_work` is what keeps the *automatic* locks
        // from taking that trade without asking.)
        let mut warning = None;
        let held = match self.stash_unsaved_work() {
            Ok(true) => crate::document::stash::find(),
            Ok(false) => None,
            Err(e) => {
                // Reaching here means unsaved work existed and could not
                // be encrypted — most often a brand-new file, which has
                // no key of its own to stash to. The lock still happens
                // (the caller has decided the session must lock), so the
                // text is gone. That must be said on screen: a warning on
                // stderr is a warning nobody sees.
                eprintln!("warning: could not secure unsaved edits: {e:#}");
                warning = Some(format!(
                    "Unsaved text could not be held and was discarded — {e}. \
                     A new file has no key of its own yet; save it once, or set \
                     a fixed stash key in Settings, and this cannot happen again."
                ));
                None
            }
        };
        // Close the jot window and zeroize its text too.
        self.jot.open = false;
        self.jot.clear_text();
        ctx.send_viewport_cmd(egui::ViewportCommand::Title("Schl8 — Locked".to_string()));
        self.state = State::Locked {
            relock_path,
            held,
            warning,
        };
    }

    /// Whether unsaved work could be encrypted to disk right now.
    ///
    /// The automatic locks consult this before deciding to lock: with a
    /// key available the text can be secured and the session may lock;
    /// without one the old behavior stands and the lock is deferred, so
    /// an idle timer never destroys typed text.
    fn can_secure_unsaved_work(&self) -> bool {
        self.stash_recipients().is_some()
    }

    /// Launch the GUI window. Blocks until the window is closed.
    pub fn run(self) -> anyhow::Result<()> {
        let icon_data = load_icon_data();
        // Always opaque: translucency would let other windows shine
        // through next to decrypted text, so it's not supported at all.
        let mut viewport = egui::ViewportBuilder::default()
            .with_title("Schl8")
            .with_inner_size([820.0, 620.0])
            .with_min_inner_size([420.0, 320.0]);
        if let Some(icon) = icon_data {
            viewport = viewport.with_icon(std::sync::Arc::new(icon));
        }
        let options = eframe::NativeOptions {
            viewport,
            ..Default::default()
        };

        eframe::run_native(
            "Schl8",
            options,
            Box::new(|cc| {
                configure_style(&cc.egui_ctx);
                theme::apply_font(&cc.egui_ctx, &self.config.appearance.font);
                theme::apply_font_scale(&cc.egui_ctx, self.config.appearance.font_scale);
                // Receive Finder "open document" events (Open With…,
                // double-click on associated files, drops on Dock icon).
                crate::macos_open::install(&cc.egui_ctx);
                // Observe sleep / screen-lock to auto-lock on those events.
                crate::macos_power::install(&cc.egui_ctx);
                Ok(Box::new(self))
            }),
        )
        .map_err(|e| anyhow::anyhow!("GUI error: {e}"))
    }

    /// Put `schl8` on PATH, and say plainly what happened.
    ///
    /// The three outcomes are genuinely different and the user needs to
    /// know which one they got: a link they can use immediately, a link
    /// that needs a password, or a link in a folder their shell cannot
    /// see yet. Reporting all three as "done" would leave someone
    /// wondering why their agent still gets "command not found".
    fn install_cli_tool(&mut self) {
        use crate::cli_install::{self, Method, Status};

        if let Status::Foreign(path) = cli_install::status() {
            self.cli_tool_dialog.show(
                "Something else is already there",
                format!(
                    "{} exists but was not created by this copy of Schl8. \
                     It may be an older install or a build you placed by \
                     hand. Remove it yourself and try again — overwriting \
                     someone else's file is not this button's job.",
                    path.display()
                ),
                None,
                true,
            );
            return;
        }

        let plan = match cli_install::plan() {
            Ok(p) => p,
            Err(e) => {
                self.cli_tool_dialog.show(
                    "Could not work out where to install",
                    e.to_string(),
                    None,
                    true,
                );
                return;
            }
        };

        let method = plan.method.clone();
        match cli_install::install(&plan) {
            Ok(link) => match method {
                Method::Direct | Method::Admin => self.cli_tool_dialog.show(
                    "Command line tool installed",
                    format!(
                        "`schl8` now runs from any terminal, linked at {}. \
                         Your assistant can start with `schl8 agent brief`.",
                        link.display()
                    ),
                    Some("schl8 --version".to_string()),
                    false,
                ),
                Method::NeedsPathEdit {
                    export_line,
                    profile,
                } => self.cli_tool_dialog.show(
                    "Almost — one line to add",
                    format!(
                        "Linked at {}, but that folder is not on your PATH, so \
                         a terminal still will not find it. Add this line to \
                         {profile} and open a new terminal:",
                        link.display()
                    ),
                    Some(export_line),
                    false,
                ),
            },
            Err(e) if e.to_string() == "cancelled" => {
                self.cli_tool_dialog.show(
                    "Cancelled",
                    "Nothing was changed. The only place available needed an \
                     administrator password."
                        .to_string(),
                    None,
                    false,
                );
            }
            Err(e) => {
                self.cli_tool_dialog.show(
                    "Could not install the command line tool",
                    format!("{e}"),
                    None,
                    true,
                );
            }
        }
    }

    /// Recompute what an install would touch, for the dialog's preview.
    fn refresh_toolkit_plan(&mut self) {
        let cfg = crate::config::Config::load();
        let planned = crate::agent_skills::plan(&cfg).unwrap_or_default();
        self.toolkit_dialog.installed = planned
            .iter()
            .any(|p| p.action == crate::agent_skills::Action::Refresh);
        self.toolkit_dialog.planned = planned
            .iter()
            .map(|p| (format!("{:?}", p.action), p.path.display().to_string()))
            .collect();
    }

    fn render_toolkit_dialog(&mut self, ctx: &egui::Context) {
        match self.toolkit_dialog.render(ctx) {
            dialogs::ToolkitAction::None => {}
            dialogs::ToolkitAction::Install => {
                let cfg = crate::config::Config::load();
                // force=false: a file we did not write is never replaced
                // without the user going to the CLI and saying so.
                self.toolkit_dialog.status =
                    Some(match crate::agent_skills::install(&cfg, false) {
                        Ok(w) => (
                            format!(
                                "Wrote {} file(s). Try /schl8:jot in a new session.",
                                w.len()
                            ),
                            false,
                        ),
                        Err(e) => (format!("{e}"), true),
                    });
                self.refresh_toolkit_plan();
            }
            dialogs::ToolkitAction::Uninstall => {
                self.toolkit_dialog.status = Some(match crate::agent_skills::uninstall() {
                    Ok((removed, skipped)) => {
                        let mut msg = format!("Removed {} file(s).", removed.len());
                        if !skipped.is_empty() {
                            msg.push_str(&format!(
                                " Left {} alone — not written by Schl8.",
                                skipped.len()
                            ));
                        }
                        (msg, false)
                    }
                    Err(e) => (format!("{e}"), true),
                });
                self.refresh_toolkit_plan();
            }
        }
    }

    fn render_backup_dialog(&mut self, ctx: &egui::Context) {
        if self.backup_dialog.render(ctx) != dialogs::BackupAction::Save {
            return;
        }
        let protection = self.backup_dialog.protection();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let name = crate::config_backup::suggested_name(&protection, &today);

        let mut dialog = rfd::FileDialog::new()
            .set_title("Save settings backup")
            .set_file_name(&name);
        if let Ok(dir) = self.config.ensure_notes_dir() {
            dialog = dialog.set_directory(dir);
        }
        let Some(dest) = dialog.save_file() else {
            return; // cancelled
        };

        self.backup_dialog.status = Some(match crate::config_backup::write(&dest, &protection) {
            Ok(()) => {
                let sealed = protection != crate::config_backup::Protection::None;
                if self.backup_dialog.close_when_done {
                    self.backup_dialog.open = false;
                }
                (
                    format!(
                        "Saved to {}{}",
                        dest.display(),
                        if sealed { " (encrypted)" } else { "" }
                    ),
                    false,
                )
            }
            Err(e) => (format!("{e:#}"), true),
        });
    }

    fn render_uninstall_dialog(&mut self, ctx: &egui::Context) {
        match self.uninstall_dialog.render(ctx) {
            dialogs::UninstallAction::None => {}
            dialogs::UninstallAction::BackUpFirst => {
                // Leave the uninstall window open behind it, so the
                // backup is a detour rather than a restart.
                self.backup_dialog.close_when_done = false;
                self.backup_dialog.open_with(&self.config);
            }
            dialogs::UninstallAction::Remove => {
                let outcome = crate::uninstall::execute(&self.uninstall_dialog.plan);
                if outcome.failed.is_empty() {
                    // Nothing left to run from; quitting is the honest end.
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                } else {
                    let detail = outcome
                        .failed
                        .iter()
                        .map(|(p, e)| format!("{}: {e}", p.display()))
                        .collect::<Vec<_>>()
                        .join("; ");
                    self.uninstall_dialog.status = Some((
                        format!(
                            "Removed {} item(s); {} could not be moved — {detail}",
                            outcome.removed.len(),
                            outcome.failed.len()
                        ),
                        true,
                    ));
                    self.uninstall_dialog.plan = crate::uninstall::plan();
                }
            }
        }
    }

    fn show_toast(&mut self, message: String, is_error: bool, ctx: &egui::Context) {
        self.show_toast_for(message, is_error, 4.0, ctx);
    }

    /// A toast that lingers. Four seconds suits "Saved"; a warning about
    /// losing work needs long enough to actually be read.
    fn show_toast_for(
        &mut self,
        message: String,
        is_error: bool,
        seconds: f64,
        ctx: &egui::Context,
    ) {
        let now = ctx.input(|i| i.time);
        self.toast = Some((message, is_error, now + seconds));
    }

    /// Suggested filename when adding a save-plan destination: the current
    /// document's own file name.
    fn save_targets_suggested_name(&self) -> Option<String> {
        if let State::Viewing { doc, .. } = &self.state {
            doc.source_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_string)
        } else {
            None
        }
    }

    /// Compute stats for the current document/entry and draw the stats card.
    fn render_stats_card(&mut self, ctx: &egui::Context) {
        use crate::ui::stats_card::{self, FileMeta};

        match &self.state {
            State::Viewing {
                doc, edit_buffer, ..
            } => {
                let text = match edit_buffer {
                    Some(buf) => buf.as_str(),
                    None => doc.content.as_str().unwrap_or(""),
                };
                let stats = self.stats_cache.get(text);
                let on_disk = doc.source_path.is_absolute() && doc.source_path.exists();
                let meta = FileMeta {
                    path: &doc.source_path,
                    file_type: doc.file_type,
                    signature: Some(&doc.signature),
                    recipient_count: doc.recipients.as_ref().map(|r| r.len()),
                    on_disk,
                };
                stats_card::show(ctx, &stats, &meta);
            }
            State::ViewingArchive {
                archive, selected, ..
            } => {
                // An emptied vault has no entry to describe.
                let Some(entry) = archive.entries.get(*selected) else {
                    return;
                };
                let text = entry.content.as_str().unwrap_or("");
                let stats = self.stats_cache.get(text);
                let meta = FileMeta {
                    path: std::path::Path::new(&entry.rel_path),
                    file_type: entry.file_type,
                    signature: None,
                    recipient_count: None,
                    on_disk: false,
                };
                stats_card::show(ctx, &stats, &meta);
            }
            _ => {}
        }
    }

    /// Render the quick-note as its own borderless, translucent, always-on-top
    /// floating window (a separate egui viewport). Returns the jot's action.
    fn render_jot_viewport(&mut self, ctx: &egui::Context) -> quicknote::JotAction {
        // Restore the persisted geometry (size/position survive restarts).
        let size = self.config.quick_note.window_size.unwrap_or([500.0, 340.0]);
        let mut builder = egui::ViewportBuilder::default()
            .with_title("Quick Note")
            .with_inner_size(size)
            .with_min_inner_size([420.0, 300.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(true)
            .with_always_on_top()
            .with_taskbar(false);
        if let Some([x, y]) = self.config.quick_note.window_pos {
            builder = builder.with_position([x, y]);
        }

        // Only offer files that still exist — a deleted note must not be
        // selectable (appending to it could never work).
        let notes: Vec<(String, PathBuf)> = self
            .config
            .quick_note
            .notes
            .iter()
            .filter(|n| self.cached_exists(&n.source))
            .map(|n| (n.name.clone(), n.source.clone()))
            .collect();
        let jot = &mut self.jot;
        let mut action = quicknote::JotAction::None;
        let mut seen_rect: Option<(egui::Pos2, egui::Vec2)> = None;

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("schl8_jot"),
            builder,
            |vctx, _class| {
                // Track live geometry so it can be persisted on close.
                vctx.input(|i| {
                    let vp = i.viewport();
                    if let (Some(outer), Some(inner)) = (vp.outer_rect, vp.inner_rect) {
                        seen_rect = Some((outer.min, inner.size()));
                    }
                });

                // Window-manager close button / Cmd+W on the jot window.
                if vctx.input(|i| i.viewport().close_requested()) && !jot.busy {
                    jot.open = false;
                    jot.clear_text();
                }

                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::NONE
                            .fill(theme::bg_primary())
                            .stroke(egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.5)))
                            .corner_radius(theme::RADIUS + 4.0)
                            .inner_margin(18.0),
                    )
                    .show(vctx, |ui| {
                        // Accent gradient hairline across the top edge.
                        let top = ui.max_rect();
                        let line = egui::Rect::from_min_max(
                            egui::pos2(top.left(), top.top() - 10.0),
                            egui::pos2(top.right(), top.top() - 7.0),
                        );
                        theme::paint_accent_gradient(ui.painter(), line);

                        action = jot.render_contents(ui, &notes);
                    });
            },
        );

        // Remember the latest observed geometry; persisted when the jot
        // closes (writing config on every frame would be wasteful).
        if let Some((pos, size)) = seen_rect {
            self.jot_last_geometry = Some((pos, size));
        }

        action
    }

    /// Save the open document without asking anything: a configured save
    /// plan if present, else re-encrypting to the file's own recipients
    /// in place. Returns `NeedsDialog` when neither applies (a new file
    /// or plaintext import — the caller opens Encrypt & Save As).
    fn save_in_place(&mut self, ctx: &egui::Context) -> SaveOutcome {
        // Refuse to overwrite a file that changed underneath us. Another
        // window, a sync client, or a `schl8 append` merge may have
        // written it since we loaded; blindly encrypting our copy over
        // the top would destroy that work with no warning. The user can
        // still choose to overwrite from the conflict dialog.
        if let Some(src) = self.current_source_path() {
            if self.force_overwrite {
                self.force_overwrite = false;
            } else if self.source_changed_on_disk(&src) {
                self.save_conflict = Some(src);
                return SaveOutcome::Failed;
            }
        }

        // Without a configured save plan, age documents follow their own
        // path: encrypt to the unlocked identity (default-to-self). A plan
        // takes precedence for every backend (it can fan out to age AND
        // gpg destinations), so it's handled by the multisave path below.
        if let State::Viewing { doc, .. } = &self.state {
            if is_age_source(&doc.source_path) && self.config.plan_for(&doc.source_path).is_none() {
                return self.save_age(ctx);
            }
        }

        let mut outcome = SaveOutcome::NeedsDialog;
        let mut toast_after: Option<(String, bool)> = None;
        let plan = match &self.state {
            State::Viewing { doc, .. } => self.config.plan_for(&doc.source_path).cloned(),
            State::ViewingArchive { archive, .. } => {
                self.config.plan_for(&archive.source_path).cloned()
            }
            _ => None,
        };
        // AGE archives record no recipients, so the archive save branch
        // can't fall back to them — resolve the identity's own recipient
        // here (before the &mut borrow of self.state below) so an
        // AGE-encrypted vault re-encrypts to self on save.
        let archive_age_recipient = match &self.state {
            State::ViewingArchive { archive, .. } if is_age_source(&archive.source_path) => self
                .age_identity
                .as_ref()
                .map(|id| id.recipient().to_string()),
            _ => None,
        };
        if let State::Viewing {
            doc,
            edit_buffer,
            modified,
            ..
        } = &mut self.state
        {
            // A configured save plan takes precedence: encrypt to
            // each rule's key and overwrite all its destinations.
            if let Some(plan) = &plan {
                let plaintext: &[u8] = match edit_buffer {
                    Some(buf) => buf.as_bytes(),
                    None => doc.content.as_bytes(),
                };
                let results = crate::document::multisave::execute(plaintext, plan);
                let total = results.len();
                let failures: Vec<String> = results
                    .iter()
                    .filter_map(|r| {
                        r.result
                            .as_ref()
                            .err()
                            .map(|e| format!("{}: {e:#}", r.destination.display()))
                    })
                    .collect();
                if failures.is_empty() {
                    if let Some(buf) = edit_buffer {
                        doc.content = SecureBuffer::from_bytes(buf.as_bytes().to_vec());
                    }
                    *modified = false;
                    outcome = SaveOutcome::Saved;
                    toast_after = Some((
                        format!(
                            "Saved to {total} destination{}",
                            if total == 1 { "" } else { "s" }
                        ),
                        false,
                    ));
                } else {
                    eprintln!("save plan: {} of {total} targets failed", failures.len());
                    outcome = SaveOutcome::Failed;
                    toast_after = Some((
                        format!(
                            "Saved {} of {total}; failed: {}",
                            total - failures.len(),
                            failures.join("; ")
                        ),
                        true,
                    ));
                }
            } else {
                match doc.recipients.clone() {
                    Some(recipients) => {
                        let plaintext: &[u8] = match edit_buffer {
                            Some(buf) => buf.as_bytes(),
                            None => doc.content.as_bytes(),
                        };
                        let armor =
                            doc.source_path.extension().and_then(|e| e.to_str()) == Some("asc");
                        let recips: Vec<&str> = recipients.iter().map(String::as_str).collect();
                        match keys::encrypt_overwrite(plaintext, &recips, &doc.source_path, armor) {
                            Ok(()) => {
                                // Adopt the edited content; stay in
                                // edit mode if we were editing.
                                if let Some(buf) = edit_buffer {
                                    doc.content = SecureBuffer::from_bytes(buf.as_bytes().to_vec());
                                }
                                *modified = false;
                                outcome = SaveOutcome::Saved;
                                let name = doc
                                    .source_path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("file");
                                toast_after = Some((format!("Saved {name}"), false));
                            }
                            Err(e) => {
                                outcome = SaveOutcome::Failed;
                                toast_after = Some((format!("Save failed: {e:#}"), true));
                            }
                        }
                    }
                    // No known recipients (new file or opened as
                    // plaintext) — the caller opens Encrypt & Save As.
                    None => outcome = SaveOutcome::NeedsDialog,
                }
            }
        } else if let State::ViewingArchive {
            archive,
            selected,
            edit_buffer,
            modified,
            ..
        } = &mut self.state
        {
            // Archive-entry save: rebuild the tar with the edited entry
            // (preserving all other entries, including non-text files),
            // re-compress if the source was compressed, and re-encrypt the
            // whole archive — via its save plan when configured, else to
            // its original recipients, atomically overwriting in place.
            if let (Some(buf), Some(entry)) = (edit_buffer.as_ref(), archive.entries.get(*selected))
            {
                match crate::document::archive::rebuild_with_edit(
                    archive.raw_tar.as_bytes(),
                    &entry.rel_path,
                    buf.as_bytes(),
                    archive.gzip,
                ) {
                    Ok(rebuilt) => {
                        let write_result: Result<String, String> = if let Some(plan) = &plan {
                            let results = crate::document::multisave::execute(
                                rebuilt.payload.as_bytes(),
                                plan,
                            );
                            let total = results.len();
                            let failures: Vec<String> = results
                                .iter()
                                .filter_map(|r| {
                                    r.result
                                        .as_ref()
                                        .err()
                                        .map(|e| format!("{}: {e:#}", r.destination.display()))
                                })
                                .collect();
                            if failures.is_empty() {
                                Ok(format!(
                                    "Saved archive to {total} destination{}",
                                    if total == 1 { "" } else { "s" }
                                ))
                            } else {
                                Err(format!(
                                    "Saved {} of {total}; failed: {}",
                                    total - failures.len(),
                                    failures.join("; ")
                                ))
                            }
                        } else if let Some(recipient) = &archive_age_recipient {
                            // AGE vault: re-encrypt the rebuilt tar to the
                            // unlocked identity (AGE records no recipients).
                            let name = archive
                                .source_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("archive")
                                .to_string();
                            crate::crypto::age_backend::encrypt_to_recipients(
                                rebuilt.payload.as_bytes(),
                                &[recipient.as_str()],
                            )
                            .and_then(|ct| keys::atomic_write(&archive.source_path, &ct))
                            .map(|()| format!("Saved {name}"))
                            .map_err(|e| format!("Save failed: {e:#}"))
                        } else if is_age_source(&archive.source_path) {
                            Err("Unlock your AGE identity to save this vault".to_string())
                        } else if let Some(recipients) = archive.recipients.clone() {
                            let armor = archive.source_path.extension().and_then(|e| e.to_str())
                                == Some("asc");
                            let recips: Vec<&str> = recipients.iter().map(String::as_str).collect();
                            keys::encrypt_overwrite(
                                rebuilt.payload.as_bytes(),
                                &recips,
                                &archive.source_path,
                                armor,
                            )
                            .map(|()| {
                                format!(
                                    "Saved {}",
                                    archive
                                        .source_path
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("archive")
                                )
                            })
                            .map_err(|e| format!("Save failed: {e:#}"))
                        } else {
                            Err("Cannot re-encrypt: the archive's recipients are unknown. \
                                 Use Encrypt & Save As to export this entry instead."
                                .to_string())
                        };

                        match write_result {
                            Ok(msg) => {
                                // Adopt the edit in memory: the entry's
                                // content and the raw tar both move to the
                                // saved version.
                                if let Some(entry) = archive.entries.get_mut(*selected) {
                                    entry.content =
                                        SecureBuffer::from_bytes(buf.as_bytes().to_vec());
                                }
                                archive.raw_tar = rebuilt.tar;
                                *modified = false;
                                outcome = SaveOutcome::Saved;
                                toast_after = Some((msg, false));
                            }
                            Err(msg) => {
                                outcome = SaveOutcome::Failed;
                                toast_after = Some((msg, true));
                            }
                        }
                    }
                    Err(e) => {
                        outcome = SaveOutcome::Failed;
                        toast_after = Some((format!("Could not rebuild archive: {e:#}"), true));
                    }
                }
            }
        }
        if let Some((msg, is_err)) = toast_after {
            self.show_toast(msg, is_err, ctx);
        }

        // Post-save hooks (background; paths only, never content): the
        // plan's own command first, then the app-wide one.
        if outcome == SaveOutcome::Saved {
            let source = match &self.state {
                State::Viewing { doc, .. } => Some(doc.source_path.clone()),
                State::ViewingArchive { archive, .. } => Some(archive.source_path.clone()),
                _ => None,
            };
            // Our write is now the version of record.
            if let Some(source) = &source {
                self.remember_source_identity(source);
            }
            if let Some(source) = source {
                let dests = plan
                    .as_ref()
                    .map(crate::hooks::plan_destinations)
                    .unwrap_or_else(|| vec![source.clone()]);
                if let Some(p) = &plan {
                    crate::hooks::run_post_save(&p.post_save_command, &source, &dests);
                }
                crate::hooks::run_post_save(&self.config.app.post_save_command, &source, &dests);
            }
        }
        outcome
    }

    /// Render the find & replace bar and act on it: match counting,
    /// next/prev navigation (scrolls the view/editor to the match), and —
    /// in edit mode — replace one / replace all on the secure buffer.
    fn update_find_bar(&mut self, ctx: &egui::Context) {
        if !self.find.open {
            return;
        }
        // Only single-document view supports find (for now).
        let State::Viewing {
            doc,
            edit_buffer,
            lines_count,
            ..
        } = &self.state
        else {
            self.find.open = false;
            return;
        };
        let is_editing = edit_buffer.is_some();
        let total_lines = (*lines_count).max(1);

        // Esc anywhere closes the bar.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.find.open = false;
            return;
        }

        // Match offsets (borrow of the plaintext ends here).
        let matches = {
            let text: &str = match edit_buffer {
                Some(buf) => buf.as_str(),
                None => doc.content.as_str().unwrap_or(""),
            };
            find_matches(text, &self.find.query)
        };
        if !matches.is_empty() && self.find.active >= matches.len() {
            self.find.active = matches.len() - 1;
        }

        #[derive(PartialEq)]
        enum Act {
            None,
            Next,
            Prev,
            ReplaceOne,
            ReplaceAll,
            Close,
        }
        let mut act = Act::None;

        egui::TopBottomPanel::top("findbar")
            .frame(
                egui::Frame::NONE
                    .fill(theme::bg_statusbar())
                    .inner_margin(egui::Margin::symmetric(8, 5)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Find")
                            .size(theme::FONT_SIZE_STATUS)
                            .color(theme::text_dim()),
                    );
                    let q = ui.add(
                        egui::TextEdit::singleline(&mut self.find.query)
                            .desired_width(180.0)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("Find…"),
                    );
                    if self.find.want_focus {
                        q.request_focus();
                        self.find.want_focus = false;
                    }
                    // Enter jumps to the next match and keeps typing focus.
                    if q.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        act = Act::Next;
                        q.request_focus();
                    }

                    let count_label = if self.find.query.is_empty() {
                        String::new()
                    } else if matches.is_empty() {
                        "0 matches".to_string()
                    } else {
                        format!("{}/{}", self.find.active + 1, matches.len())
                    };
                    ui.label(
                        egui::RichText::new(count_label)
                            .size(theme::FONT_SIZE_STATUS)
                            .monospace()
                            .color(if matches.is_empty() && !self.find.query.is_empty() {
                                theme::accent_red()
                            } else {
                                theme::text_dim()
                            }),
                    );
                    // Plain words rather than arrow glyphs: the bundled
                    // fonts don't cover every symbol, and a missing one
                    // renders as an empty box.
                    if ui
                        .add_enabled(
                            !matches.is_empty(),
                            egui::Button::new(
                                egui::RichText::new("Prev").size(theme::FONT_SIZE_STATUS),
                            ),
                        )
                        .on_hover_text("Previous match (Shift+Enter)")
                        .clicked()
                    {
                        act = Act::Prev;
                    }
                    if ui
                        .add_enabled(
                            !matches.is_empty(),
                            egui::Button::new(
                                egui::RichText::new("Next").size(theme::FONT_SIZE_STATUS),
                            ),
                        )
                        .on_hover_text("Next match (Enter)")
                        .clicked()
                    {
                        act = Act::Next;
                    }

                    if is_editing {
                        ui.separator();
                        ui.label(
                            egui::RichText::new("Replace")
                                .size(theme::FONT_SIZE_STATUS)
                                .color(theme::text_dim()),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.find.replace)
                                .desired_width(160.0)
                                .font(egui::TextStyle::Monospace)
                                .hint_text("Replace with…"),
                        );
                        if ui
                            .add_enabled(!matches.is_empty(), egui::Button::new("Replace"))
                            .on_hover_text("Replace the current match")
                            .clicked()
                        {
                            act = Act::ReplaceOne;
                        }
                        if ui
                            .add_enabled(!matches.is_empty(), egui::Button::new("All"))
                            .on_hover_text("Replace every match")
                            .clicked()
                        {
                            act = Act::ReplaceAll;
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(egui::RichText::new("Close").size(theme::FONT_SIZE_STATUS))
                            .on_hover_text("Close the find bar (Esc)")
                            .clicked()
                        {
                            act = Act::Close;
                        }
                        if !is_editing {
                            ui.label(
                                egui::RichText::new("(enter edit mode to replace)")
                                    .size(10.5)
                                    .color(theme::text_dim()),
                            );
                        }
                    });
                });
            });

        // Shift+Enter → previous match.
        if act == Act::None
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter))
        {
            act = Act::Prev;
        }

        let qlen = self.find.query.len();
        match act {
            Act::None => {}
            Act::Close => self.find.open = false,
            Act::Next | Act::Prev if !matches.is_empty() => {
                self.find.active = if act == Act::Next {
                    (self.find.active + 1) % matches.len()
                } else {
                    (self.find.active + matches.len() - 1) % matches.len()
                };
                // Scroll so the match's line is in view (proportional).
                if let State::Viewing {
                    doc, edit_buffer, ..
                } = &self.state
                {
                    let text: &str = match edit_buffer {
                        Some(buf) => buf.as_str(),
                        None => doc.content.as_str().unwrap_or(""),
                    };
                    let line = byte_to_line(text, matches[self.find.active]);
                    let (content_h, viewport_h, _) = self.view_metrics;
                    if content_h > 0.0 {
                        let frac = line as f32 / total_lines as f32;
                        self.pending_jump = Some((frac * content_h - viewport_h * 0.35).max(0.0));
                    }
                }
            }
            Act::Next | Act::Prev => {}
            Act::ReplaceOne | Act::ReplaceAll if !matches.is_empty() => {
                let replacement = self.find.replace.clone();
                if let State::Viewing {
                    edit_buffer: Some(buf),
                    modified,
                    ..
                } = &mut self.state
                {
                    let s = buf.as_mut_string();
                    if act == Act::ReplaceAll {
                        // Back to front so earlier offsets stay valid.
                        for &off in matches.iter().rev() {
                            s.replace_range(off..off + qlen, &replacement);
                        }
                    } else {
                        let off = matches[self.find.active];
                        s.replace_range(off..off + qlen, &replacement);
                    }
                    buf.relock_if_moved();
                    *modified = true;
                }
            }
            Act::ReplaceOne | Act::ReplaceAll => {}
        }
    }

    /// Open an age-encrypted file on the main thread. If the identity
    /// isn't unlocked, remember the file and prompt for the seed phrase;
    /// it opens automatically after a successful unlock.
    fn open_age_file(&mut self, path: &std::path::Path, ctx: &egui::Context) {
        let Some(identity) = &self.age_identity else {
            self.pending_age_open = Some(path.to_path_buf());
            self.age_dialog.show_unlock();
            self.show_toast(
                "Enter your seed phrase to open this AGE file".to_string(),
                false,
                ctx,
            );
            return;
        };
        match crate::document::loader::load_age(path, identity) {
            Ok(LoadedDocument::Single(doc)) => {
                let filename = doc
                    .source_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "Schl8 \u{2014} {filename}"
                )));
                let lines_count = count_lines(&doc.content);
                self.config.add_recent(&doc.source_path);
                let _ = self.config.save();
                self.remember_source_identity(&doc.source_path);
                self.state = State::Viewing {
                    doc,
                    scroll_offset: 0.0,
                    lines_count,
                    current_line: 1,
                    edit_buffer: None,
                    modified: false,
                };
            }
            Ok(LoadedDocument::Archive(archive)) => {
                let filename = archive
                    .source_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "Schl8 \u{2014} {filename} ({} files)",
                    archive.entries.len()
                )));
                // An empty vault opens like any other (matching the GPG
                // path) so files can be added back into it; the browser
                // renders an empty view rather than refusing to open.
                if archive.entries.is_empty() {
                    self.show_toast("This vault has no text files yet".to_string(), false, ctx);
                }
                let tree = filetree::build_tree(&archive.entries, &archive.dirs);
                let lines_count = archive
                    .entries
                    .first()
                    .map(|e| count_lines(&e.content))
                    .unwrap_or(0);
                self.config.add_recent(&archive.source_path);
                let _ = self.config.save();
                self.remember_source_identity(&archive.source_path);
                self.state = State::ViewingArchive {
                    archive,
                    tree,
                    selected: 0,
                    selected_dir: None,
                    scroll_offset: 0.0,
                    lines_count,
                    current_line: 1,
                    edit_buffer: None,
                    modified: false,
                };
            }
            Err(e) => {
                self.state = State::Error {
                    message: format!("{e:#}"),
                    failed_path: path.to_path_buf(),
                };
            }
        }
    }

    /// Available age recipients for the encrypt dialog: stored recipients
    /// plus the unlocked identity's own key (labeled), deduped.
    fn available_age_recipients(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        if let Some(id) = &self.age_identity {
            out.push((
                "This device (seed phrase)".to_string(),
                id.recipient().to_string(),
            ));
        }
        for r in &self.config.age_recipients {
            if !out.iter().any(|(_, rec)| rec == &r.recipient) {
                out.push((r.label.clone(), r.recipient.clone()));
            }
        }
        out
    }

    /// Write the jot's current text into `target`'s spool, encrypted to
    /// the rules' AGE recipients. Returns the new pending count.
    ///
    /// Nothing here decrypts: the recipients come from config, so this
    /// works with the identity locked — that is the whole point.
    fn spool_jot(
        &mut self,
        target: &std::path::Path,
        rules: &[crate::config::SaveRule],
    ) -> anyhow::Result<usize> {
        let blurb = config::render_blurb(
            &self.config.quick_note,
            target,
            self.jot.text(),
            self.jot.include_timestamp,
        );
        let written = chrono::Utc::now().to_rfc3339();
        let mut envelope = crate::document::spool::envelope(&written, &blurb);
        // Backend comes from the note's own save plan — age when it has an
        // age recipient, GPG otherwise. Either way encrypting needs only a
        // public key, which is the whole point of the spool.
        let encrypted = crate::document::spool::encrypt_segment(rules, envelope.as_bytes());
        envelope.zeroize();
        let (ciphertext, format) = encrypted?;
        crate::document::spool::write_segment(
            target,
            &ciphertext,
            format,
            self.config.quick_note.max_pending,
        )?;
        self.invalidate_pending();
        Ok(crate::document::spool::pending_count(target))
    }

    /// Merge every note's pending spool entries into it, using the
    /// unlocked identity. Reports what merged and what could not be read.
    fn merge_all_spools(&mut self, ctx: &egui::Context) {
        let notes: Vec<PathBuf> = self
            .config
            .quick_note
            .notes
            .iter()
            .map(|n| n.source.clone())
            .collect();
        let identity = self.age_identity.as_ref();
        // GPG segments open through gpg-agent, so a locked AGE identity
        // only blocks the age ones. Bail early only when nothing could be
        // merged anyway — otherwise merge what is readable and say what
        // stayed behind.
        if identity.is_none() {
            let gpg_pending: usize = notes
                .iter()
                .map(|n| {
                    crate::document::spool::pending_count_of(
                        n,
                        crate::document::spool::SegmentFormat::Gpg,
                    )
                })
                .sum();
            if gpg_pending == 0 {
                self.show_toast(
                    "Unlock your AGE identity to merge pending entries".to_string(),
                    true,
                    ctx,
                );
                return;
            }
        }

        let mut merged = 0usize;
        let mut unreadable = 0usize;
        let mut failures: Vec<String> = Vec::new();

        for note in notes {
            let (segments, failed) = crate::document::spool::read_segments(&note, identity);
            unreadable += failed.len();
            if segments.is_empty() {
                continue;
            }
            let text = crate::document::spool::merged_text(&segments);
            let rules = self
                .config
                .quicknote_for(&note)
                .map(|n| n.rules.clone())
                .unwrap_or_default();
            let age_clone = identity.and_then(|id| id.try_clone().ok());
            // Reuse the ordinary append path so the save plan, atomic
            // writes, and post-save hooks all behave identically.
            match crate::document::append::append_blurb_with_rules(
                &note,
                &text,
                &rules,
                age_clone.as_ref(),
            ) {
                Ok(()) => {
                    let paths: Vec<PathBuf> = segments.iter().map(|s| s.path.clone()).collect();
                    // Only now, with the note durably written, is it safe
                    // to drop the segments.
                    if let Err(e) = crate::document::spool::remove_segments(&paths) {
                        failures.push(format!("{}: {e:#}", note.display()));
                    }
                    merged += segments.len();
                }
                Err(e) => failures.push(format!("{}: {e:#}", note.display())),
            }
        }

        let msg = if merged == 0 && failures.is_empty() && unreadable == 0 {
            "No pending entries to merge".to_string()
        } else {
            let mut m = format!(
                "Merged {merged} offline entr{}",
                if merged == 1 { "y" } else { "ies" }
            );
            // Provenance, not decoration: these were written without the
            // key, so they carry no proof of who wrote them.
            m.push_str(" (written while locked)");
            if unreadable > 0 {
                m.push_str(&format!("; {unreadable} unreadable, left in place"));
            }
            if !failures.is_empty() {
                m.push_str(&format!("; failed: {}", failures.join("; ")));
            }
            m
        };
        self.invalidate_pending();
        self.show_toast(msg, !failures.is_empty(), ctx);
    }

    /// Total pending entries across every registered quicknote, scanned
    /// live. Key-free but hits the filesystem — use for one-off actions,
    /// not per frame.
    fn total_pending(&self) -> usize {
        self.config
            .quick_note
            .notes
            .iter()
            .map(|n| crate::document::spool::pending_count(&n.source))
            .sum()
    }

    /// Invalidate the cached spool counts so the next frame rescans.
    fn invalidate_pending(&mut self) {
        self.pending_scanned_at = None;
    }

    /// Refresh the cached per-note pending counts at most every few
    /// seconds. The menu bar reads the cache, so a quiet app does no
    /// filesystem work.
    fn refresh_pending(&mut self, now: f64) {
        const SCAN_INTERVAL: f64 = 3.0;
        if let Some(at) = self.pending_scanned_at {
            if now - at < SCAN_INTERVAL {
                return;
            }
        }
        self.pending_cache = self
            .config
            .quick_note
            .notes
            .iter()
            .map(|n| {
                (
                    n.source.clone(),
                    crate::document::spool::pending_count(&n.source),
                    n.source.exists(),
                )
            })
            .collect();
        self.pending_scanned_at = Some(now);

        // A deleted file can't stay the implicit jot target.
        if let Some(last) = &self.config.quick_note.last_target {
            if !self.cached_exists(last) {
                self.config.quick_note.last_target = None;
            }
        }
        if let Some(sel) = &self.jot.selected_target {
            if !self.cached_exists(sel) {
                self.jot.selected_target = None;
            }
        }
    }

    /// Cached pending count for one note.
    fn cached_pending(&self, note: &std::path::Path) -> usize {
        self.pending_cache
            .iter()
            .find(|(p, _, _)| p == note)
            .map_or(0, |(_, c, _)| *c)
    }

    /// Whether the note's file existed at the last scan. Unknown paths
    /// (not scanned yet) count as existing so nothing flickers out of the
    /// UI on the first frame.
    fn cached_exists(&self, note: &std::path::Path) -> bool {
        self.pending_cache
            .iter()
            .find(|(p, _, _)| p == note)
            .map_or(true, |(_, _, e)| *e)
    }

    /// Cached total across all notes.
    fn cached_pending_total(&self) -> usize {
        self.pending_cache.iter().map(|(_, c, _)| *c).sum()
    }

    /// Wipe the in-memory AGE identity. Dropping the `AgeIdentity`
    /// zeroizes its mlock'd key buffer; nothing has to be erased on disk
    /// because the key was never written there.
    fn forget_age_identity(&mut self, ctx: &egui::Context, why: &str) {
        if self.age_identity.take().is_some() {
            self.age_unlocked_at = None;
            self.show_toast(format!("AGE identity forgotten ({why})"), false, ctx);
        }
    }

    /// Apply the configured AGE lock policy: idle timeout and the absolute
    /// ceiling since unlock. Sleep and window-close are handled where those
    /// events are detected.
    fn enforce_age_lock(&mut self, ctx: &egui::Context, now: f64, last_active: f64) {
        if self.age_identity.is_none() {
            return;
        }
        let policy = self.config.age_lock.clone();
        let idle_limit = policy.forget_idle_minutes;
        if idle_limit > 0 && now - last_active >= f64::from(idle_limit) * 60.0 {
            self.forget_age_identity(ctx, "idle");
            return;
        }
        let cap = policy.forget_after_minutes;
        if cap > 0 {
            if let Some(at) = self.age_unlocked_at {
                if now - at >= f64::from(cap) * 60.0 {
                    self.forget_age_identity(ctx, "time limit");
                }
            }
        }
    }

    /// Append the jot window's current text to the selected quicknote.
    ///
    /// When the target is AGE-encrypted and the identity is locked, this
    /// puts the seed-phrase prompt up instead of appending, and records
    /// that the submit should resume once the unlock succeeds. The typed
    /// note is never copied out — it stays in the jot's own buffer and is
    /// re-rendered on resume.
    fn submit_jot(&mut self, ctx: &egui::Context) {
        let Some(target) = self.jot.selected_target.clone() else {
            return;
        };
        // A note whose file vanished must fail clearly, not with a gpg or
        // AGE decrypt error — and spooling to it would strand the entry,
        // since a merge appends to a file that isn't there.
        if !target.exists() {
            self.jot.selected_target = None;
            self.invalidate_pending();
            self.jot.status = Some(
                "That note's file no longer exists \u{2014} pick another, or remove it \
                 via Quick Note Files\u{2026}"
                    .to_string(),
            );
            return;
        }
        // Registry entries with explicit rules fan the append out to each
        // key's destinations.
        let rules = self
            .config
            .quicknote_for(&target)
            .map(|n| n.rules.clone())
            .unwrap_or_default();

        // An AGE quicknote needs the identity unlocked, because appending
        // means decrypt-then-re-encrypt.
        let needs_age = rules.iter().any(|r| r.is_age()) || is_age_source(&target);
        if needs_age && self.age_identity.is_none() {
            // Encrypting needs no private key — only reading does. So by
            // default the entry is spooled beside the note and merged on
            // the next unlocked session, keeping the jot instant.
            // Set when spooling was tried and could not work; the note is
            // then saved the ordinary way instead of being stranded.
            let mut offline_error: Option<String> = None;
            if self.config.quick_note.spool_when_locked {
                match self.spool_jot(&target, &rules) {
                    Ok(pending) => {
                        // A spooled entry IS a successful save, so the jot
                        // closes exactly like a normal append — the badge
                        // and menu carry the pending state from here.
                        self.jot.busy = false;
                        self.jot.clear_text();
                        self.jot.open = false;
                        // Past four fifths of the cap, say so — and say it
                        // as a warning, because further entries will be
                        // refused once the spool is full.
                        let max = self.config.quick_note.max_pending;
                        let nagging = crate::document::spool::should_nag(pending, max);
                        let msg = if nagging {
                            format!(
                                "Saved offline \u{2014} {pending} of {max} pending, \
                                 unlock and merge soon"
                            )
                        } else {
                            format!(
                                "Saved offline \u{2014} {pending} pending, merges when you unlock"
                            )
                        };
                        self.show_toast(msg, nagging, ctx);
                        self.config.quick_note.include_timestamp = self.jot.include_timestamp;
                        self.config.add_target(target);
                        if let Err(e) = self.config.save() {
                            eprintln!("warning: could not save config: {e:#}");
                        }
                    }
                    // Spooling is a convenience, never the only way out: a
                    // note with no usable recipient (or a spool directory
                    // that can't be written) must still be savable, so
                    // fall through to the seed-phrase prompt rather than
                    // dead-ending with the typed entry unsaved.
                    Err(e) => offline_error = Some(format!("{e:#}")),
                }
            }
            // Ask for the phrase — either because spooling is turned off,
            // or because it was tried and could not work. The jot is its
            // own viewport and the unlock dialog lives on the main window,
            // so the main window has to be revealed or the prompt would be
            // invisible.
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.main_visible = true;
            self.age_dialog.show_unlock();
            self.jot_pending_unlock = true;
            self.jot.status = Some(match offline_error {
                Some(e) => format!(
                    "Could not save offline ({e}) \u{2014} enter your seed phrase to save this note"
                ),
                None => "Enter your seed phrase to save this note".to_string(),
            });
            return;
        }

        let blurb = config::render_blurb(
            &self.config.quick_note,
            &target,
            self.jot.text(),
            self.jot.include_timestamp,
        );
        let age_clone = self
            .age_identity
            .as_ref()
            .and_then(|id| id.try_clone().ok());
        self.jot.busy = true;
        self.jot.status = None;
        self.jot_rx = Some(spawn_append(
            target.clone(),
            blurb,
            rules,
            self.config.app.post_save_command.clone(),
            age_clone,
        ));

        // Remember preferences for next time
        self.config.quick_note.include_timestamp = self.jot.include_timestamp;
        self.config.add_target(target);
        if let Err(e) = self.config.save() {
            eprintln!("warning: could not save config: {e:#}");
        }
    }

    /// Encrypt the current document/vault content to every destination in
    /// `plan` right now, so newly configured keys and locations exist on
    /// disk the moment the plan is applied. Returns how many destinations
    /// were written. Plaintext for the archive case is the (re-compressed)
    /// tar payload.
    fn materialize_plan(&self, plan: &crate::config::SavePlan) -> anyhow::Result<usize> {
        // Build the plaintext to encrypt, owned so no borrow is held across
        // the multisave call. It goes in a SecureBuffer, not a bare Vec:
        // this is a full copy of the document (or the whole vault), and a
        // plain Vec would leave it unlocked and un-zeroized on the heap.
        let payload: SecureBuffer = match &self.state {
            State::Viewing {
                doc, edit_buffer, ..
            } => SecureBuffer::from_bytes(match edit_buffer {
                Some(buf) => buf.as_bytes().to_vec(),
                None => doc.content.as_bytes().to_vec(),
            }),
            State::ViewingArchive { archive, .. } => {
                // Compress the current tar to match the archive's format.
                SecureBuffer::from_bytes(crate::document::archive::compress_payload(
                    archive.raw_tar.as_bytes(),
                    archive.gzip,
                )?)
            }
            _ => anyhow::bail!("no open document"),
        };

        let results = crate::document::multisave::execute(payload.as_bytes(), plan);
        let failures: Vec<String> = results
            .iter()
            .filter_map(|r| {
                r.result
                    .as_ref()
                    .err()
                    .map(|e| format!("{}: {e:#}", r.destination.display()))
            })
            .collect();
        if failures.is_empty() {
            Ok(results.len())
        } else {
            anyhow::bail!("{}", failures.join("; "))
        }
    }

    /// Record the on-disk identity of `path` as the version Schl8 is
    /// working from. Called after a load and after every successful write.
    fn remember_source_identity(&mut self, path: &std::path::Path) {
        self.source_identity =
            file_identity(path).map(|(len, mtime)| (path.to_path_buf(), len, mtime));
    }

    /// Whether `path` differs from the version Schl8 loaded or last
    /// wrote. False when nothing was recorded (a new file), when the
    /// recorded path is a different file, or when the file is now missing
    /// — none of those can lose someone else's edit.
    fn source_changed_on_disk(&self, path: &std::path::Path) -> bool {
        let Some((known, len, mtime)) = &self.source_identity else {
            return false;
        };
        if known != path {
            return false;
        }
        match file_identity(path) {
            Some((now_len, now_mtime)) => now_len != *len || now_mtime != *mtime,
            None => false,
        }
    }

    /// The path an in-place save would overwrite, if any.
    fn current_source_path(&self) -> Option<PathBuf> {
        match &self.state {
            State::Viewing { doc, .. } => Some(doc.source_path.clone()),
            State::ViewingArchive { archive, .. } => Some(archive.source_path.clone()),
            _ => None,
        }
    }

    /// The highlighted vault folder, if any.
    fn selected_vault_folder(&self) -> Option<String> {
        match &self.state {
            State::ViewingArchive { selected_dir, .. } => selected_dir.clone(),
            _ => None,
        }
    }

    /// The selected vault entry's rel_path, if a vault is open.
    fn selected_vault_entry(&self) -> Option<String> {
        match &self.state {
            State::ViewingArchive {
                archive, selected, ..
            } => archive.entries.get(*selected).map(|e| e.rel_path.clone()),
            _ => None,
        }
    }

    /// The folder of the selected vault entry (empty for a root file), so
    /// new files default beside the current one.
    fn selected_vault_dir(&self) -> String {
        self.selected_vault_entry()
            .and_then(|rel| rel.rsplit_once('/').map(|(dir, _)| dir.to_string()))
            .unwrap_or_default()
    }

    /// Encrypt a rebuilt vault payload to its source and overwrite it,
    /// using the same backend the vault was opened with. Structural
    /// changes persist immediately (no separate Save step).
    fn write_vault_payload(&self, source: &std::path::Path, payload: &[u8]) -> anyhow::Result<()> {
        if is_age_source(source) {
            let recipient = self
                .age_identity
                .as_ref()
                .map(|id| id.recipient().to_string())
                .ok_or_else(|| anyhow::anyhow!("unlock your AGE identity to change this vault"))?;
            let ct = crate::crypto::age_backend::encrypt_to_recipients(payload, &[&recipient])?;
            keys::atomic_write(source, &ct)
        } else if let State::ViewingArchive { archive, .. } = &self.state {
            let recipients = archive
                .recipients
                .clone()
                .ok_or_else(|| anyhow::anyhow!("the vault's recipients are unknown"))?;
            let armor = source.extension().and_then(|e| e.to_str()) == Some("asc");
            let recips: Vec<&str> = recipients.iter().map(String::as_str).collect();
            keys::encrypt_overwrite(payload, &recips, source, armor)
        } else {
            anyhow::bail!("no open vault")
        }
    }

    /// Apply a structural vault change (add / rename / delete), re-encrypt
    /// to the source, and refresh the in-memory archive. Blocked while an
    /// entry has unsaved edits, so a mutation can't discard them.
    fn apply_vault_op(&mut self, op: dialogs::VaultPromptAction, ctx: &egui::Context) {
        // Gather what the mutation needs without holding a borrow across
        // the re-encryption (which reads self for recipients/identity).
        // The tar copy is the entire decrypted vault, so it lives in a
        // SecureBuffer — a bare Vec would leave every file in the vault
        // unlocked and un-zeroized on the heap.
        let (source, raw_tar, gzip, modified) = match &self.state {
            State::ViewingArchive {
                archive, modified, ..
            } => (
                archive.source_path.clone(),
                SecureBuffer::from_bytes(archive.raw_tar.as_bytes().to_vec()),
                archive.gzip,
                *modified,
            ),
            _ => return,
        };
        let raw_tar = raw_tar.as_bytes();
        if modified {
            self.show_toast(
                "Save or discard the current edit before changing the vault".to_string(),
                true,
                ctx,
            );
            return;
        }
        // A structural change rewrites the whole vault, so it would clobber
        // anything written since we loaded. Mutations are cheap to redo, so
        // refuse outright rather than offering an overwrite.
        if self.source_changed_on_disk(&source) {
            self.show_toast(
                "This vault changed on disk since you opened it \u{2014} reopen it before \
                 making changes"
                    .to_string(),
                true,
                ctx,
            );
            return;
        }

        use crate::document::archive as arch;
        let result = match &op {
            dialogs::VaultPromptAction::Add { rel_path, markdown } => {
                // Give a bare name the right extension; a name with one is
                // left as typed.
                let path = ensure_text_extension(rel_path, *markdown);
                let starter = if *markdown {
                    format!("# {}\n", leaf_stem(&path))
                } else {
                    String::new()
                };
                arch::add_entry(raw_tar, &path, starter.as_bytes(), gzip)
            }
            dialogs::VaultPromptAction::AddFolder { rel_path } => {
                arch::add_dir(raw_tar, rel_path, gzip)
            }
            dialogs::VaultPromptAction::Rename { from, to, folder } => {
                if *folder {
                    arch::rename_prefix(raw_tar, from, to, gzip)
                } else {
                    let to = if to.contains('.') {
                        to.clone()
                    } else {
                        // Renaming a file without an extension keeps the old one.
                        match from.rsplit_once('.') {
                            Some((_, ext)) => format!("{to}.{ext}"),
                            None => to.clone(),
                        }
                    };
                    arch::rename_entry(raw_tar, from, &to, gzip)
                }
            }
            dialogs::VaultPromptAction::None => return,
        };

        let rebuilt = match result {
            Ok(r) => r,
            Err(e) => {
                self.vault_prompt.set_error(format!("{e:#}"));
                return;
            }
        };

        // Persist, then swap the in-memory archive to the rebuilt tar.
        if let Err(e) = self.write_vault_payload(&source, rebuilt.payload.as_bytes()) {
            self.vault_prompt.set_error(format!("{e:#}"));
            return;
        }
        self.remember_source_identity(&source);
        self.reload_archive_from_tar(rebuilt.tar.as_bytes(), ctx);
        self.show_toast("Vault updated".to_string(), false, ctx);
    }

    /// Delete the named vault entry, re-encrypt, and refresh.
    fn delete_vault_target(&mut self, target: &VaultDeleteTarget, ctx: &egui::Context) {
        // SecureBuffer, not Vec: this copy is the whole decrypted vault.
        let (source, raw_tar, gzip) = match &self.state {
            State::ViewingArchive { archive, .. } => (
                archive.source_path.clone(),
                SecureBuffer::from_bytes(archive.raw_tar.as_bytes().to_vec()),
                archive.gzip,
            ),
            _ => return,
        };
        let raw_tar = raw_tar.as_bytes();
        use crate::document::archive as arch;
        let (result, label) = match target {
            VaultDeleteTarget::File(rel) => (arch::remove_entry(raw_tar, rel, gzip), rel.clone()),
            VaultDeleteTarget::Folder(dir) => {
                (arch::remove_prefix(raw_tar, dir, gzip), format!("{dir}/"))
            }
        };
        match result {
            Ok(rebuilt) => {
                if let Err(e) = self.write_vault_payload(&source, rebuilt.payload.as_bytes()) {
                    self.show_toast(format!("Delete failed: {e:#}"), true, ctx);
                    return;
                }
                self.remember_source_identity(&source);
                // A folder delete may have removed the highlighted folder.
                if let State::ViewingArchive { selected_dir, .. } = &mut self.state {
                    *selected_dir = None;
                }
                self.reload_archive_from_tar(rebuilt.tar.as_bytes(), ctx);
                self.show_toast(format!("Deleted {label}"), false, ctx);
            }
            Err(e) => self.show_toast(format!("Delete failed: {e:#}"), true, ctx),
        }
    }

    /// Rebuild the ViewingArchive state from a new raw tar, keeping the
    /// selection in bounds.
    fn reload_archive_from_tar(&mut self, raw_tar: &[u8], _ctx: &egui::Context) {
        let extracted = match crate::document::archive::extract_text_entries(raw_tar) {
            Ok(e) => e,
            Err(e) => {
                self.show_toast(format!("Could not re-read vault: {e:#}"), true, _ctx);
                return;
            }
        };
        if let State::ViewingArchive {
            archive,
            tree,
            selected,
            selected_dir,
            edit_buffer,
            modified,
            lines_count,
            ..
        } = &mut self.state
        {
            archive.raw_tar = SecureBuffer::from_bytes(raw_tar.to_vec());
            archive.entries = extracted.entries;
            // Keep the hidden-file tally in step: a save that drops the
            // last binary must stop claiming one is still in there.
            archive.hidden = extracted.hidden;
            archive.dirs = crate::document::archive::extract_dir_entries(raw_tar);
            // A rename/delete can invalidate the highlighted folder path.
            *selected_dir = None;
            // The sidebar renders this cached tree, not entries directly —
            // without rebuilding it the mutation is invisible.
            *tree = filetree::build_tree(&archive.entries, &archive.dirs);
            *selected = (*selected).min(archive.entries.len().saturating_sub(1));
            *edit_buffer = None;
            *modified = false;
            *lines_count = archive
                .entries
                .get(*selected)
                .map(|e| count_lines(&e.content))
                .unwrap_or(0);
        }
    }

    /// The AGE recipient the current document was decrypted with, when it
    /// is an AGE file and the identity is unlocked. AGE ciphertext records
    /// no recipient, so this is how Save Options learns the file's own key.
    fn own_age_recipient_for_state(&self) -> Option<String> {
        let is_age = match &self.state {
            State::Viewing { doc, .. } => is_age_source(&doc.source_path),
            _ => false,
        };
        if !is_age {
            return None;
        }
        self.age_identity
            .as_ref()
            .map(|id| id.recipient().to_string())
    }

    /// Open the Encrypt & Save As dialog, seeding it with the available
    /// age recipients and defaulting to age when the source is an age file.
    fn open_encrypt_dialog(&mut self, default_armor: bool) {
        let default_age = match &self.state {
            State::Viewing { doc, .. } => is_age_source(&doc.source_path),
            _ => false,
        };
        let age_recips = self.available_age_recipients();
        self.encrypt_dialog.show_with_format(
            default_armor,
            age_recips,
            default_age,
            self.gpg_available,
        );
    }

    /// Save an age document, encrypting to the unlocked identity (the
    /// default-to-self policy — multi-recipient age comes with the
    /// recipient store). A new file (no absolute path yet) prompts for a
    /// location first; an existing one is overwritten atomically.
    fn save_age(&mut self, ctx: &egui::Context) -> SaveOutcome {
        // Copy the plaintext out of the state borrow so the identity
        // (a separate field) can be borrowed for encryption.
        let (mut plaintext, source, editing) = match &self.state {
            State::Viewing {
                doc, edit_buffer, ..
            } => {
                let pt = match edit_buffer {
                    Some(buf) => buf.as_bytes().to_vec(),
                    None => doc.content.as_bytes().to_vec(),
                };
                (pt, doc.source_path.clone(), edit_buffer.is_some())
            }
            _ => return SaveOutcome::NeedsDialog,
        };

        let Some(identity) = &self.age_identity else {
            plaintext.zeroize();
            self.show_toast(
                "Unlock your AGE identity to save this note".to_string(),
                true,
                ctx,
            );
            return SaveOutcome::Failed;
        };
        let recipient = identity.recipient().to_string();

        // New file → pick a destination; existing → overwrite in place.
        let dest = if source.is_absolute() {
            source.clone()
        } else {
            let suggested = source
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("untitled.md.age");
            match pick_save_path(suggested, "age") {
                Some(p) => p,
                None => {
                    plaintext.zeroize();
                    return SaveOutcome::Failed; // user cancelled
                }
            }
        };

        let result = crate::crypto::age_backend::encrypt_to_recipients(&plaintext, &[&recipient])
            .and_then(|ct| keys::atomic_write(&dest, &ct));
        plaintext.zeroize();

        match result {
            Ok(()) => {
                let name = dest
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string();
                if let State::Viewing {
                    doc,
                    edit_buffer,
                    modified,
                    ..
                } = &mut self.state
                {
                    if let Some(buf) = edit_buffer {
                        doc.content = SecureBuffer::from_bytes(buf.as_bytes().to_vec());
                    }
                    doc.source_path = dest.clone();
                    *modified = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                        "Schl8 \u{2014} {name}"
                    )));
                }
                self.remember_source_identity(&dest);
                self.config.add_recent(&dest);
                let _ = self.config.save();
                let _ = editing;
                self.show_toast(format!("Saved {name} (AGE)"), false, ctx);
                SaveOutcome::Saved
            }
            Err(e) => {
                self.show_toast(format!("Save failed: {e:#}"), true, ctx);
                SaveOutcome::Failed
            }
        }
    }

    fn is_editing(&self) -> bool {
        matches!(
            self.state,
            State::Viewing {
                edit_buffer: Some(_),
                ..
            } | State::ViewingArchive {
                edit_buffer: Some(_),
                ..
            }
        )
    }

    fn toggle_edit_mode(&mut self) {
        match &mut self.state {
            State::Viewing {
                doc,
                edit_buffer,
                modified,
                ..
            } => {
                if edit_buffer.is_some() {
                    // Exit edit mode — drop the edit buffer (zeroized on drop)
                    *edit_buffer = None;
                    // Note: modified stays true so the user knows content was changed
                } else {
                    // Enter edit mode — copy content into editable SecureString
                    match SecureString::from_secure_buffer(&doc.content) {
                        Ok(buf) => {
                            *edit_buffer = Some(buf);
                            *modified = false;
                        }
                        Err(e) => {
                            eprintln!("failed to enter edit mode: {e}");
                        }
                    }
                }
            }
            State::ViewingArchive {
                archive,
                selected,
                edit_buffer,
                modified,
                ..
            } => {
                if edit_buffer.is_some() {
                    *edit_buffer = None;
                } else if let Some(entry) = archive.entries.get(*selected) {
                    match SecureString::from_secure_buffer(&entry.content) {
                        Ok(buf) => {
                            *edit_buffer = Some(buf);
                            *modified = false;
                        }
                        Err(e) => {
                            eprintln!("failed to enter edit mode: {e}");
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Build egui's widget visuals from the ACTIVE theme. Must be re-run
/// whenever the theme changes (not just at startup), or buttons, default
/// text, and window chrome keep the previous palette's colors — dark
/// text on dark fills, or pale text on white, depending on direction.
fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    // Base on the matching egui preset so derived colors (shadows,
    // scrollbars, text edit cursors, …) fit the theme's brightness.
    let mut v = if theme::is_light() {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };

    v.panel_fill = theme::bg_primary();
    v.window_fill = theme::bg_primary();
    // Framed inputs (dialogs, forms, the jot) render override_text_color
    // = text_primary, so their fill must contrast with THAT — derive it
    // from bg_primary (text_primary's guaranteed partner) with a visible
    // shift, never from the editor palette, which pairs with text_editor
    // and clashes on themes where the two diverge.
    v.extreme_bg_color = if theme::is_light() {
        theme::bg_primary().gamma_multiply(0.93)
    } else {
        theme::bg_primary().gamma_multiply(1.55)
    };
    v.faint_bg_color = theme::bg_raised();
    v.window_stroke = egui::Stroke::new(1.0, theme::bg_raised());
    v.window_corner_radius = egui::CornerRadius::same(theme::RADIUS as u8);
    v.selection.bg_fill = theme::accent().gamma_multiply(0.35);
    v.selection.stroke = egui::Stroke::new(1.0, theme::accent());
    v.hyperlink_color = theme::accent();
    v.override_text_color = Some(theme::text_primary());

    // Accent-tinted, rounded interactive widgets.
    let r = egui::CornerRadius::same(6);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.corner_radius = r;
    v.widgets.inactive.bg_fill = theme::bg_raised();
    v.widgets.inactive.weak_bg_fill = theme::bg_raised();
    // Hover: darken on light themes, brighten on dark ones — always a
    // visible shift from the resting fill.
    let hover_mul = if theme::is_light() { 0.92 } else { 1.35 };
    v.widgets.hovered.bg_fill = theme::bg_raised().gamma_multiply(hover_mul);
    v.widgets.hovered.weak_bg_fill = theme::bg_raised().gamma_multiply(hover_mul);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.7));
    v.widgets.active.bg_fill = theme::accent().gamma_multiply(0.55);
    v.widgets.active.weak_bg_fill = theme::accent().gamma_multiply(0.55);
    // Widget label colors must track the theme, not the visuals preset.
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.fg_stroke.color = theme::text_primary();
    }
    v.widgets.hovered.fg_stroke.color = theme::text_strong();
    v.widgets.active.fg_stroke.color = theme::text_strong();

    style.visuals = v;
    style.interaction.selectable_labels = false;
    style.spacing.button_padding = egui::vec2(10.0, 6.0);

    ctx.set_style(style);
}

/// Spawn a background thread to decrypt the given file.
fn spawn_decrypt(path: PathBuf) -> mpsc::Receiver<DecryptResult> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = crate::document::loader::load(&path).map_err(|e| format!("{e:#}"));
        let _ = tx.send(result);
    });
    rx
}

/// Spawn a background thread that appends a quick-note blurb to an
/// encrypted file (decrypt → append → re-encrypt → atomic overwrite).
/// The blurb is zeroized when the thread finishes with it.
fn spawn_append(
    path: PathBuf,
    blurb: String,
    rules: Vec<crate::config::SaveRule>,
    post_save_command: String,
    age_identity: Option<crate::crypto::age_backend::AgeIdentity>,
) -> mpsc::Receiver<Result<(), String>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut blurb = blurb;
        let result = crate::document::append::append_blurb_with_rules(
            &path,
            &blurb,
            &rules,
            age_identity.as_ref(),
        )
        .map_err(|e| format!("{e:#}"));
        blurb.zeroize();
        if result.is_ok() {
            let dests: Vec<PathBuf> = if rules.is_empty() {
                vec![path.clone()]
            } else {
                rules.iter().flat_map(|r| r.destinations.clone()).collect()
            };
            crate::hooks::run_post_save(&post_save_command, &path, &dests);
        }
        let _ = tx.send(result);
    });
    rx
}

/// Show the file picker dialog (native OS dialog via rfd).
fn pick_open_file() -> Option<PathBuf> {
    // No extension filters, deliberately.
    //
    // rfd's macOS backend flattens every `add_filter` into ONE
    // `setAllowedFileTypes` allow-list — there is no per-filter dropdown —
    // and macOS treats "*" as a literal extension rather than a wildcard,
    // so an "All files" filter allows nothing. The result was a hard
    // allow-list that greyed out files macOS types differently from their
    // last extension, which double extensions like `journal.md.gpg` hit.
    //
    // Filtering was only ever cosmetic: the loader detects the format from
    // the content (age header magic, GPG packets, plaintext fallback), so
    // an unopenable pick fails with a clear message rather than silently.
    rfd::FileDialog::new().set_title("Open file").pick_file()
}

/// Show a save dialog for the encrypted output file.
/// Enforces the chosen encrypted extension — plaintext files must never be written.
fn pick_save_path(suggested_name: &str, chosen_ext: &str) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .set_title("Save encrypted file")
        .set_file_name(suggested_name);
    // Open in the notes folder so new files land together by default —
    // created on demand, because a directory macOS cannot see is one the
    // dialog silently ignores. A failure here is not worth reporting: the
    // dialog just opens wherever it last was.
    if let Ok(dir) = crate::config::Config::load().ensure_notes_dir() {
        dialog = dialog.set_directory(dir);
    }
    let path = dialog.save_file()?;

    // Enforce encrypted extension — never save plaintext to disk
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext == "asc" || ext == "gpg" || ext == "age" {
        Some(path)
    } else {
        Some(path.with_extension(chosen_ext))
    }
}

/// Cheap on-disk identity of a file: (length, mtime). Enough to notice
/// that a file changed underneath an open document without re-reading and
/// hashing it on every save.
fn file_identity(path: &std::path::Path) -> Option<(u64, std::time::SystemTime)> {
    let md = std::fs::metadata(path).ok()?;
    Some((md.len(), md.modified().ok()?))
}

/// Show a file picker for importing a public key.
fn pick_key_file() -> Option<PathBuf> {
    // Unfiltered for the same reason as `pick_open_file` — the "All files"
    // filter is a no-op on macOS, so any key file with an unexpected
    // extension would be unselectable. gpg validates the import anyway.
    rfd::FileDialog::new()
        .set_title("Import GPG public key")
        .pick_file()
}

/// Transition request returned by render functions.
/// What a pending vault delete removes.
#[derive(Clone)]
enum VaultDeleteTarget {
    File(String),
    Folder(String),
}

enum Transition {
    None,
    StartDecrypt(PathBuf),
    /// Remove a (missing) entry from the picker's recents list.
    RemoveRecent(PathBuf),
    /// Vault file management (open archives only).
    VaultAddPrompt,
    VaultAddFolderPrompt,
    VaultRenamePrompt,
    VaultDeletePrompt,
    /// Open a fresh, empty document in edit mode.
    NewFile(FileType),
    /// Open the quick-note window.
    OpenJot,
    /// Quit the app (bypasses hide-to-menu-bar).
    Quit,
    CloseDocument,
    /// User clicked "Discard Edits" in the status bar.
    RequestDiscard,
    /// User clicked "Encrypt & Save" in the status bar (save then exit edit).
    RequestSaveAndExit,
    /// User explicitly asked to save under a different key/location.
    /// User clicked the status bar's "Edit" button in view mode.
    EnterEdit,
    /// User clicked "Re-encrypt" — open the Save Targets window.
    OpenSaveTargets,
    /// Authenticate, reopen the document, and put the held edits back.
    RestoreHeldEdits,
    /// Erase the held edits and go back to the last saved version.
    DiscardHeldEdits,
    /// Lock the session immediately (the panic button / its shortcut).
    PanicLock,
}

/// Result of an in-place (no-questions) save attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveOutcome {
    Saved,
    Failed,
    /// No plan and no known recipients — Encrypt & Save As must run.
    NeedsDialog,
}

impl eframe::App for App {
    /// Clear to transparent: the jot viewport needs it for its rounded
    /// corners, and every panel paints a fully opaque background fill, so
    /// document content can never show anything through it.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Refresh the per-note filesystem cache first: the jot list, the
        // tray, and the vanished-file guards all read it this frame.
        // Throttled, so a quiet app does no filesystem work.
        self.refresh_pending(ctx.input(|i| i.time));

        // One-time notice: GPG wasn't found, so only the age backend works.
        if self.gpg_hint_pending {
            self.gpg_hint_pending = false;
            self.show_toast(
                "GPG not found — running in AGE-only mode. Use Keys › Unlock AGE Identity."
                    .to_string(),
                false,
                ctx,
            );
        }

        // ── Enforce no-clipboard policy ──────────────────────────────────
        // Strip Copy/Cut events before any widget can place plaintext on the
        // system clipboard — unless the user has explicitly opted in this
        // session via View → Allow Copying.
        if !self.allow_copy {
            ctx.input_mut(|i| {
                i.events
                    .retain(|e| !matches!(e, egui::Event::Copy | egui::Event::Cut));
            });
        }
        // Selectable labels follow the same opt-in (needed to select text
        // to copy in read mode).
        ctx.style_mut(|s| s.interaction.selectable_labels = self.allow_copy);

        // Unsaved work that locking would destroy: editor changes, or
        // typed-but-unsubmitted quick-note text. Both defer auto-lock —
        // losing user text is worse than keeping plaintext in memory a
        // little longer (the statusbar/jot show a warning while deferred).
        // (Not gated on `jot.open`: "Manage…" hides the window but keeps
        // the typed text, which still deserves protection.)
        let jot_has_text = !self.jot.text().trim().is_empty();
        let unsaved_work = self.has_unsaved_edits() || jot_has_text;

        // ── Lock on sleep / screen-lock ──────────────────────────────────
        // A macOS power/lock notification takes precedence over the idle
        // timer and locks immediately (unless unsaved work would be lost).
        let sleep_lock_requested = crate::macos_power::take_lock_request();
        if sleep_lock_requested && self.config.age_lock.forget_on_sleep {
            self.forget_age_identity(ctx, "display slept");
        }
        if sleep_lock_requested && self.config.app.lock_on_sleep {
            // Unsaved work no longer blocks a lock outright: if it can be
            // encrypted to the document's own key first, the session locks
            // and the text is held safely. Only work with no key to secure
            // it still defers, which is the case where locking really
            // would destroy something.
            if unsaved_work && !self.can_secure_unsaved_work() {
                if !self.lock_deferred_notified {
                    self.lock_deferred_notified = true;
                    self.show_toast(
                        "Lock deferred — unsaved text is kept until you save or discard it"
                            .to_string(),
                        true,
                        ctx,
                    );
                }
            } else if self.document_open() {
                self.lock_session(ctx);
            }
        }

        // ── Idle auto-lock ───────────────────────────────────────────────
        // Track activity and lock (drop + zeroize plaintext) after the
        // configured idle period. Unsaved edits defer the lock so work is
        // never silently discarded. A repaint is scheduled so the check
        // runs even with no input.
        {
            let now = ctx.input(|i| i.time);
            let active =
                ctx.input(|i| !i.events.is_empty() || i.pointer.delta() != egui::Vec2::ZERO);
            let last = *self.last_activity.get_or_insert(now);
            if active {
                self.last_activity = Some(now);
            }

            // The AGE identity has its own, independent policy: a document
            // may be closed while a key stays resident, or vice versa.
            self.enforce_age_lock(ctx, now, last);

            let limit_min = self.config.app.auto_lock_minutes;
            let guarding = self.document_open() || (self.jot.open && !self.jot.busy);
            if limit_min > 0 && guarding {
                let limit = limit_min as f64 * 60.0;
                if now - last >= limit {
                    if unsaved_work && !self.can_secure_unsaved_work() {
                        // Never silently destroy typed text. With a key
                        // available the lock goes ahead and the text is
                        // stashed encrypted; without one it is deferred,
                        // and said so (once per stretch of work).
                        if !self.lock_deferred_notified {
                            self.lock_deferred_notified = true;
                            self.show_toast(
                                "Auto-lock deferred — unsaved text is kept until you save \
                                 or discard it"
                                    .to_string(),
                                true,
                                ctx,
                            );
                        }
                    } else if self.document_open() {
                        self.lock_session(ctx);
                    } else {
                        // Only the jot window is open, with no typed
                        // text — safe to close it.
                        self.jot.open = false;
                        self.jot.clear_text();
                    }
                } else {
                    // Wake up around when the timeout would elapse.
                    let remaining = (limit - (now - last)).max(1.0);
                    ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                        remaining.min(30.0),
                    ));
                }
            }
            if !unsaved_work {
                self.lock_deferred_notified = false;
            }
        }

        // ── Dock icon clicked with no window showing ─────────────────────
        // While resident, closing the window only hides it; AppKit then has
        // nothing to un-minimize, so the Dock click would otherwise do
        // nothing. Bring the window back and focus it.
        if crate::macos_open::take_reopen_request() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            self.main_visible = true;
        }

        // ── Menu-bar residency & global hotkey ───────────────────────────
        if self.config.app.menu_bar_resident && self.resident.is_none() && !self.resident_failed {
            match crate::tray::Resident::init(ctx, &self.config.quick_note.hotkey) {
                Ok(r) => self.resident = Some(r),
                Err(e) => {
                    self.resident_failed = true;
                    eprintln!("warning: menu-bar residency unavailable: {e:#}");
                }
            }
        }
        let pending_total = self.cached_pending_total();
        // Built before the &mut borrow of `resident` below.
        let entries: Vec<(String, std::path::PathBuf)> = self
            .config
            .quick_note
            .notes
            .iter()
            .filter(|n| self.cached_exists(&n.source))
            .map(|n| {
                // Offline entries show inline, so the count is visible
                // without opening the app.
                let pending = self.cached_pending(&n.source);
                let label = if pending > 0 {
                    format!("{} \u{2014} {pending} pending", n.name)
                } else {
                    n.name.clone()
                };
                (label, n.source.clone())
            })
            .collect();

        // Hotkeys for missing files are unregistered too — a global key
        // must not summon the jot on a note that can't be appended to.
        let hotkeys: Vec<(String, std::path::PathBuf)> = self
            .config
            .quick_note
            .notes
            .iter()
            .filter(|n| self.cached_exists(&n.source))
            .map(|n| (n.hotkey.clone(), n.source.clone()))
            .collect();

        // Favorites: same treatment, and a missing file is likewise
        // dropped from the menu and unbound rather than offering to open
        // something that isn't there.
        let fav_entries: Vec<(String, std::path::PathBuf)> = self
            .config
            .favorites
            .iter()
            .filter(|f| self.cached_exists(&f.path))
            .map(|f| (f.label(), f.path.clone()))
            .collect();
        let fav_hotkeys: Vec<(String, std::path::PathBuf)> = self
            .config
            .favorites
            .iter()
            .filter(|f| self.cached_exists(&f.path))
            .map(|f| (f.hotkey.clone(), f.path.clone()))
            .collect();

        // Collected while the tray is mutably borrowed, surfaced after.
        let mut hotkey_errors: Vec<String> = Vec::new();
        if let Some(resident) = &mut self.resident {
            // Keep the tray's Quick Note submenu and the per-note global
            // hotkeys in sync with the registry (no-ops unless changed).
            resident.sync_quicknotes(&entries);
            resident.sync_pending(pending_total);
            hotkey_errors.extend(
                resident
                    .sync_note_hotkeys(&hotkeys)
                    .into_iter()
                    .map(|e| format!("Quicknote hotkey {e}")),
            );
            resident.sync_favorites(&fav_entries);
            hotkey_errors.extend(
                resident
                    .sync_favorite_hotkeys(&fav_hotkeys)
                    .into_iter()
                    .map(|e| format!("Favorite hotkey {e}")),
            );
        }
        for err in hotkey_errors {
            self.show_toast(err, true, ctx);
        }
        if let Some(resident) = &self.resident {
            for event in resident.poll() {
                match event {
                    crate::tray::ResidentEvent::QuickNote => {
                        // Open just the floating jot; leave the main window
                        // in whatever visibility it's currently in.
                        self.jot.show();
                    }
                    crate::tray::ResidentEvent::QuickNoteFor(path) => {
                        self.jot.selected_target = Some(path);
                        self.jot.show();
                    }
                    crate::tray::ResidentEvent::ManageQuickNotes => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        self.main_visible = true;
                        self.open_quicknotes_manager = true;
                    }
                    crate::tray::ResidentEvent::OpenFavorite(path) => {
                        // Opening needs the window: decryption may prompt,
                        // and the document has to be somewhere to read.
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        self.main_visible = true;
                        self.pending_tray_action = Some(Transition::StartDecrypt(path));
                    }
                    crate::tray::ResidentEvent::ManageFavorites => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        self.main_visible = true;
                        self.open_favorites_manager = true;
                    }
                    crate::tray::ResidentEvent::NewFile(kind) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        self.main_visible = true;
                        self.pending_tray_action = Some(Transition::NewFile(kind));
                    }
                    crate::tray::ResidentEvent::ShowWindow => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        self.main_visible = true;
                    }
                    crate::tray::ResidentEvent::MergePending => {
                        if self.age_identity.is_none() {
                            // Merging must decrypt, so it needs the key.
                            // Reveal the main window or the prompt would
                            // be invisible behind the hidden viewport.
                            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                            self.main_visible = true;
                            self.age_dialog.show_unlock();
                        } else {
                            self.merge_all_spools(ctx);
                        }
                    }
                    crate::tray::ResidentEvent::DiscardPending => {
                        // Destructive and irreversible — these entries
                        // cannot be read back without the key, so the
                        // user is told exactly how many they'd destroy.
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        self.main_visible = true;
                        self.discard_spool_dialog.open(self.total_pending());
                    }
                    crate::tray::ResidentEvent::Quit => {
                        self.quit_requested = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        }

        // ── Crawl overlay: edge fade + control hints ─────────────────────
        if self.crawl.active {
            self.paint_crawl_overlay(ctx);
        }

        // ── Intercept window close ───────────────────────────────────────
        // Covers Cmd+Q, the menu Quit item, and the window close button.
        // Unsaved edits always get a confirmation. Otherwise, when
        // resident in the menu bar, the close button hides the window —
        // closing any open document first so a hidden Schl8 never
        // holds plaintext.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            let has_unsaved_edits = self.has_unsaved_edits();
            if has_unsaved_edits {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.quit_dialog.open = true;
            } else if self.resident.is_some() && !self.quit_requested {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                if !matches!(self.state, State::FilePicker) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title("Schl8".to_string()));
                    self.state = State::FilePicker;
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.main_visible = false;
                // Closing to the menu bar is not quitting — the process
                // (and any unlocked key) stays alive — so honour the
                // policy here. This branch is the real close button; the
                // jot's temporary hide goes through main_hidden_for_jot
                // and deliberately does not wipe the identity.
                if self.config.age_lock.forget_on_window_close {
                    self.forget_age_identity(ctx, "window closed");
                }
            }
        }

        // ── Check for decryption completion ──────────────────────────────
        let new_state = match &self.state {
            State::Decrypting { receiver, path } => match receiver.try_recv() {
                Ok(Ok(LoadedDocument::Single(doc))) => {
                    let filename = doc
                        .source_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                        "Schl8 \u{2014} {filename}"
                    )));

                    let lines_count = count_lines(&doc.content);

                    Some(State::Viewing {
                        doc,
                        scroll_offset: 0.0,
                        lines_count,
                        current_line: 1,
                        edit_buffer: None,
                        modified: false,
                    })
                }
                Ok(Ok(LoadedDocument::Archive(archive))) => {
                    let filename = archive
                        .source_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                        "Schl8 \u{2014} {filename} ({} files)",
                        archive.entries.len()
                    )));

                    let tree = filetree::build_tree(&archive.entries, &archive.dirs);
                    // A vault can legitimately hold no text files (all
                    // binary, or emptied) — don't index into nothing.
                    let lines_count = archive
                        .entries
                        .first()
                        .map(|e| count_lines(&e.content))
                        .unwrap_or(0);

                    Some(State::ViewingArchive {
                        archive,
                        tree,
                        selected: 0,
                        selected_dir: None,
                        scroll_offset: 0.0,
                        lines_count,
                        current_line: 1,
                        edit_buffer: None,
                        modified: false,
                    })
                }
                Ok(Err(msg)) => Some(State::Error {
                    message: msg,
                    failed_path: path.clone(),
                }),
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint();
                    None
                }
                Err(mpsc::TryRecvError::Disconnected) => Some(State::Error {
                    message: "Decryption thread terminated unexpectedly".to_string(),
                    failed_path: path.clone(),
                }),
            },
            _ => None,
        };

        if let Some(s) = new_state {
            self.state = s;
            // Remember the on-disk version we just loaded, so a later save
            // can tell whether anything else has written the file since.
            if let Some(src) = self.current_source_path() {
                self.remember_source_identity(&src);
            }
            // Record successful opens in the recents list (paths only).
            let opened = match &self.state {
                State::Viewing { doc, .. } => Some(doc.source_path.clone()),
                State::ViewingArchive { archive, .. } => Some(archive.source_path.clone()),
                _ => None,
            };
            if let Some(path) = opened {
                self.config.add_recent(&path);
                if let Err(e) = self.config.save() {
                    eprintln!("warning: could not save config: {e:#}");
                }
            }
        }

        // ── Crawl mode: advance, and take its live controls ──────────────
        #[cfg(debug_assertions)]
        if self.crawl_on_launch && self.document_open() && !self.crawl.active {
            self.crawl_on_launch = false;
            self.toggle_crawl(ctx);
        }
        self.drive_crawl(ctx);

        // ── In-app keyboard shortcuts (config-driven) ────────────────────
        let is_editing = self.is_editing();
        let mut kb_action = None;
        ctx.input(|input| {
            let kb = &self.config.keybindings;
            let hit = |spec: &str| {
                crate::keybind::KeyCombo::parse(spec).is_some_and(|c| c.matches(input))
            };
            // Exact-modifier matching makes these mutually exclusive, so at
            // most one fires (e.g. cmd+shift+s never triggers cmd+s).
            if hit(&kb.save_as) {
                kb_action = Some(menu::MenuAction::EncryptAndSave);
            }
            if hit(&kb.save) {
                kb_action = Some(menu::MenuAction::Save);
            }
            if hit(&kb.toggle_edit) {
                kb_action = Some(menu::MenuAction::ToggleEdit);
            }
            if hit(&kb.find) {
                kb_action = Some(menu::MenuAction::Find);
            }
            if hit(&kb.quick_note) {
                kb_action = Some(menu::MenuAction::QuickNote);
            }
            if hit(&kb.open_file) {
                kb_action = Some(menu::MenuAction::OpenFile);
            }
            if hit(&kb.settings) {
                kb_action = Some(menu::MenuAction::Settings);
            }
            // Panic: works in edit mode too (that is when it matters), so
            // it sits outside the !is_editing guard below.
            if hit(&kb.panic_lock) {
                kb_action = Some(menu::MenuAction::PanicLock);
            }
            if hit(&kb.crawl) {
                kb_action = Some(menu::MenuAction::ToggleCrawl);
            }
            // Focus mode: Ctrl+Cmd+F to toggle; Esc to exit when active.
            if input.modifiers.ctrl
                && (input.modifiers.command || input.modifiers.mac_cmd)
                && input.key_pressed(egui::Key::F)
            {
                kb_action = Some(menu::MenuAction::ToggleFocus);
            }
            if self.focus_mode && input.key_pressed(egui::Key::Escape) {
                kb_action = Some(menu::MenuAction::ToggleFocus);
            }
            // These could destroy an in-progress edit, so only when idle.
            if !is_editing {
                if hit(&kb.close_document) {
                    kb_action = Some(menu::MenuAction::CloseDocument);
                }
                if hit(&kb.new_markdown) {
                    kb_action = Some(menu::MenuAction::NewMarkdown);
                }
                if hit(&kb.new_text) {
                    kb_action = Some(menu::MenuAction::NewText);
                }
            }
        });

        let has_document = matches!(
            self.state,
            State::Viewing { .. } | State::ViewingArchive { .. }
        );
        let can_edit = matches!(self.state, State::Viewing { .. });
        // Save works when the original recipients are known OR a save plan
        // is configured for this document.
        let can_save = match &self.state {
            State::Viewing { doc, .. } => {
                doc.recipients.is_some() || self.config.plan_for(&doc.source_path).is_some()
            }
            _ => false,
        };

        // ── Menu bar ─────────────────────────────────────────────────────
        let menu_flags = menu::MenuFlags {
            has_document,
            can_edit,
            is_editing,
            can_save,
            show_stats: self.config.app.show_stats,
            focus_mode: self.focus_mode,
            allow_copy: self.allow_copy,
            word_wrap: self.config.appearance.word_wrap,
            line_numbers: self.config.appearance.line_numbers,
            age_unlocked: self.age_identity.is_some(),
            gpg_available: self.gpg_available,
        };
        // Focus mode hides the menu bar (and status bar) for distraction-free
        // reading; shortcuts still work.
        let menu_action = if self.focus_mode || self.crawl.active {
            None
        } else {
            let menu_resp = egui::TopBottomPanel::top("menubar")
                .exact_height(26.0)
                .frame(
                    egui::Frame::NONE
                        .fill(theme::bg_statusbar())
                        .inner_margin(egui::Margin::symmetric(8, 3)),
                )
                .show(ctx, |ui| menu::render(ui, menu_flags));
            let r = menu_resp.response.rect;
            let line = egui::Rect::from_min_max(
                egui::pos2(r.left(), r.bottom() - 1.5),
                egui::pos2(r.right(), r.bottom()),
            );
            theme::paint_accent_gradient(&ctx.layer_painter(egui::LayerId::background()), line);
            menu_resp.inner
        };

        // Merge keyboard shortcut with menu action (menu takes priority)
        let action = menu_action.or(kb_action);

        // ── Handle menu/keyboard actions ─────────────────────────────────
        let mut transition = Transition::None;

        // A menu-bar Favorites/New click, staged earlier this frame.
        // Dropped while editing for the same reason a Finder open is:
        // switching documents out from under unsaved text would destroy
        // it with no prompt.
        if let Some(staged) = self.pending_tray_action.take() {
            if !is_editing {
                transition = staged;
            } else {
                self.show_toast("Finish or discard your edits first".to_string(), true, ctx);
            }
        }

        if let Some(action) = action {
            match action {
                menu::MenuAction::OpenFile => {
                    if let Some(path) = pick_open_file() {
                        transition = Transition::StartDecrypt(path);
                    }
                }
                menu::MenuAction::NewMarkdown => {
                    if !is_editing {
                        transition = Transition::NewFile(FileType::Markdown);
                    }
                }
                menu::MenuAction::NewText => {
                    if !is_editing {
                        transition = Transition::NewFile(FileType::PlainText);
                    }
                }
                menu::MenuAction::QuickNote => {
                    self.jot.show();
                }
                menu::MenuAction::ManageFavorites => {
                    self.open_favorites_manager = true;
                }
                menu::MenuAction::PanicLock => {
                    transition = Transition::PanicLock;
                }
                menu::MenuAction::ToggleCrawl => {
                    self.toggle_crawl(ctx);
                }
                menu::MenuAction::AgentHelp(idx) => {
                    self.agent_help.open_at(idx);
                }
                menu::MenuAction::InstallCliTool => {
                    self.install_cli_tool();
                }
                menu::MenuAction::BackUpSettings => {
                    self.backup_dialog.close_when_done = true;
                    self.backup_dialog.open_with(&self.config);
                }
                menu::MenuAction::Uninstall => {
                    self.uninstall_dialog.open_with(crate::uninstall::plan());
                }
                menu::MenuAction::AgentToolkit => {
                    self.refresh_toolkit_plan();
                    self.toolkit_dialog.status = None;
                    self.toolkit_dialog.open = true;
                }
                menu::MenuAction::ManageQuickNotes => {
                    self.open_quicknotes_manager = true;
                }
                menu::MenuAction::Save => {
                    if self.save_in_place(ctx) == SaveOutcome::NeedsDialog {
                        if let State::Viewing { doc, .. } = &self.state {
                            let default_armor = !source_is_binary(&doc.source_path);
                            self.open_encrypt_dialog(default_armor);
                        }
                    }
                }
                menu::MenuAction::EncryptAndSave => match &self.state {
                    State::Viewing { doc, .. } => {
                        let default_armor = !source_is_binary(&doc.source_path);
                        self.open_encrypt_dialog(default_armor);
                    }
                    State::ViewingArchive { .. } => {
                        self.open_encrypt_dialog(false);
                    }
                    _ => {}
                },
                menu::MenuAction::ToggleEdit => {
                    if has_document {
                        if is_editing {
                            // Leaving edit mode discards the buffer — route
                            // through the confirmation when there are
                            // unsaved changes (no-op confirm otherwise).
                            transition = Transition::RequestDiscard;
                        } else {
                            self.toggle_edit_mode();
                        }
                    }
                }
                menu::MenuAction::CloseDocument => {
                    if has_document {
                        transition = Transition::CloseDocument;
                    }
                }
                menu::MenuAction::Quit => {
                    self.quit_requested = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                menu::MenuAction::ManageKeys => {
                    self.key_manager.show();
                }
                menu::MenuAction::ImportKey => {
                    if let Some(path) = pick_key_file() {
                        match keys::import_key(&path) {
                            Ok(msg) => self.show_toast(format!("Key imported: {msg}"), false, ctx),
                            Err(e) => self.show_toast(format!("Import failed: {e}"), true, ctx),
                        }
                    }
                }
                menu::MenuAction::SaveTargets => {
                    let age_recips = self.available_age_recipients();
                    let age_self = self.own_age_recipient_for_state();
                    if let State::Viewing { doc, .. } = &self.state {
                        if doc.source_path.is_absolute() {
                            self.save_targets.open_for(
                                &doc.source_path,
                                self.config.plan_for(&doc.source_path),
                                doc.recipients.as_deref(),
                                age_recips,
                                age_self.as_deref(),
                            );
                        } else {
                            self.show_toast(
                                "Save the new file once (Encrypt & Save As) before configuring targets".to_string(),
                                true,
                                ctx,
                            );
                        }
                    }
                }
                menu::MenuAction::Settings => {
                    self.settings_dialog.open(&self.config);
                }
                menu::MenuAction::ToggleStats => {
                    self.config.app.show_stats = !self.config.app.show_stats;
                    let _ = self.config.save();
                }
                menu::MenuAction::ToggleFocus => {
                    self.focus_mode = !self.focus_mode;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.focus_mode));
                }
                menu::MenuAction::ToggleCopy => {
                    if self.allow_copy {
                        // Turning copy off needs no warning.
                        self.allow_copy = false;
                    } else if self.config.security.suppress_copy_warning {
                        self.allow_copy = true;
                    } else {
                        self.copy_warning.show();
                    }
                }
                menu::MenuAction::ToggleWrap => {
                    self.config.appearance.word_wrap = !self.config.appearance.word_wrap;
                    if let Err(e) = self.config.save() {
                        eprintln!("warning: could not save config: {e:#}");
                    }
                }
                menu::MenuAction::ToggleLineNumbers => {
                    self.config.appearance.line_numbers = !self.config.appearance.line_numbers;
                    if let Err(e) = self.config.save() {
                        eprintln!("warning: could not save config: {e:#}");
                    }
                }
                menu::MenuAction::Find => {
                    if matches!(self.state, State::Viewing { .. }) {
                        self.find.open = true;
                        self.find.want_focus = true;
                    }
                }
                menu::MenuAction::ReportIssue => {
                    open_url(ISSUES_URL);
                }
                menu::MenuAction::CheckForUpdates => {
                    if self.update_rx.is_none() {
                        self.show_toast("Checking for updates…".to_string(), false, ctx);
                        self.update_rx = Some(crate::update::spawn_check());
                    }
                }
                menu::MenuAction::UnlockAge => {
                    self.age_dialog.show_unlock();
                }
                menu::MenuAction::ForgetAge => {
                    self.forget_age_identity(ctx, "manual");
                }
                menu::MenuAction::ExportAgePublicKey => {
                    self.age_dialog.show_export();
                }
                menu::MenuAction::InstallHelp => {
                    self.install_help_dialog.open = true;
                }
                menu::MenuAction::About => {
                    self.about_dialog.open = true;
                }
            }
        }

        // ── Find & replace bar ───────────────────────────────────────────
        self.update_find_bar(ctx);

        // ── Files opened from Finder (Open With / double-click) ──────────
        // Ignored while editing so an unexpected open can't destroy edits.
        for path in crate::macos_open::drain_requests() {
            if !is_editing && matches!(transition, Transition::None) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.main_visible = true;
                transition = Transition::StartDecrypt(path);
            }
        }

        // ── Drag & drop: open a dropped encrypted file ───────────────────
        // Disabled while editing so a stray drop can't destroy unsaved edits.
        if !is_editing {
            if matches!(transition, Transition::None) {
                let dropped =
                    ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()));
                if let Some(path) = dropped {
                    transition = Transition::StartDecrypt(path);
                }
            }

            if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
                let screen = ctx.screen_rect();
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    egui::Id::new("dnd_overlay"),
                ));
                painter.rect_filled(screen, 0.0, egui::Color32::from_black_alpha(140));
                painter.text(
                    screen.center(),
                    egui::Align2::CENTER_CENTER,
                    "Drop to open",
                    egui::FontId::proportional(22.0),
                    theme::text_primary(),
                );
            }
        }

        // ── Render dialogs ───────────────────────────────────────────────
        if self.encrypt_dialog.render(ctx) {
            // User clicked "Encrypt & Save" — encrypt via the chosen backend.
            let is_age = self.encrypt_dialog.is_age();
            let armor = self.encrypt_dialog.use_armor;
            let ext = if is_age {
                "age"
            } else if armor {
                "asc"
            } else {
                "gpg"
            };

            // Copy plaintext out of the state borrow (zeroized after use).
            let request: Option<(Vec<u8>, String)> = match &self.state {
                State::Viewing {
                    doc, edit_buffer, ..
                } => {
                    let pt = match edit_buffer {
                        Some(buf) => buf.as_bytes().to_vec(),
                        None => doc.content.as_bytes().to_vec(),
                    };
                    Some((pt, suggest_encrypted_name(&doc.source_path, ext)))
                }
                State::ViewingArchive {
                    archive,
                    selected,
                    edit_buffer,
                    ..
                } => {
                    // None when the vault has been emptied.
                    archive.entries.get(*selected).map(|entry| {
                        let pt = match edit_buffer {
                            Some(buf) => buf.as_bytes().to_vec(),
                            None => entry.content.as_bytes().to_vec(),
                        };
                        let base = std::path::Path::new(&entry.rel_path);
                        (pt, suggest_encrypted_name(base, ext))
                    })
                }
                _ => None,
            };

            if let Some((mut plaintext, suggested)) = request {
                let gpg_fprs = self.encrypt_dialog.selected_fingerprints();
                let age_recips = self.encrypt_dialog.selected_age_recipients();

                let mut adopted_source: Option<PathBuf> = None;
                if let Some(save_path) = pick_save_path(&suggested, ext) {
                    let result: anyhow::Result<()> = if is_age {
                        let recips: Vec<&str> = age_recips.iter().map(|s| s.as_str()).collect();
                        crate::crypto::age_backend::encrypt_to_recipients(&plaintext, &recips)
                            .and_then(|ct| keys::atomic_write(&save_path, &ct))
                    } else {
                        let recips: Vec<&str> = gpg_fprs.iter().map(|s| s.as_str()).collect();
                        keys::encrypt_to_file(&plaintext, &recips, &save_path, armor)
                    };
                    plaintext.zeroize();

                    match result {
                        Ok(()) => {
                            self.encrypt_dialog.open = false;
                            let saved_name = save_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("file")
                                .to_string();
                            self.show_toast(
                                format!("Encrypted and saved to {saved_name}"),
                                false,
                                ctx,
                            );
                            // Adopt the saved file as the current document
                            // (Save As semantics); archives are untouched.
                            if let State::Viewing {
                                doc,
                                modified,
                                edit_buffer,
                                ..
                            } = &mut self.state
                            {
                                if let Some(buf) = edit_buffer {
                                    doc.content = SecureBuffer::from_bytes(buf.as_bytes().to_vec());
                                }
                                let inner = saved_name
                                    .strip_suffix(".gpg")
                                    .or_else(|| saved_name.strip_suffix(".asc"))
                                    .or_else(|| saved_name.strip_suffix(".age"))
                                    .unwrap_or(&saved_name);
                                if let Some(t) = detect_file_type_from_name(inner) {
                                    doc.file_type = t;
                                }
                                doc.source_path = save_path.clone();
                                adopted_source = Some(save_path.clone());
                                // age hides recipients; GPG stores them so
                                // future Saves re-encrypt in place.
                                doc.recipients = if is_age { None } else { Some(gpg_fprs.clone()) };
                                *modified = false;
                                if self.exit_edit_after_save {
                                    *edit_buffer = None;
                                    self.exit_edit_after_save = false;
                                }
                                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                                    "Schl8 \u{2014} {saved_name}"
                                )));
                            }
                            self.config.add_recent(&save_path);
                            let _ = self.config.save();
                            // Save As adopts a new file — that write is now
                            // the version of record for staleness checks.
                            if let Some(p) = adopted_source.take() {
                                self.remember_source_identity(&p);
                            }
                            // App-wide post-save hook (paths only).
                            crate::hooks::run_post_save(
                                &self.config.app.post_save_command,
                                &save_path,
                                std::slice::from_ref(&save_path),
                            );
                        }
                        Err(e) => {
                            self.encrypt_dialog.status_message =
                                Some((format!("Encryption failed: {e:#}"), true));
                        }
                    }
                } else {
                    plaintext.zeroize();
                }
            }
        }

        match self
            .key_manager
            .render(ctx, &self.config.age_recipients, self.gpg_available)
        {
            crate::ui::dialogs::KeyManagerAction::None => {}
            crate::ui::dialogs::KeyManagerAction::ImportGpgFile => {
                if let Some(path) = pick_key_file() {
                    match keys::import_key(&path) {
                        Ok(msg) => {
                            self.key_manager.status_message =
                                Some((format!("Imported: {msg}"), false));
                            self.key_manager.refresh_keys();
                        }
                        Err(e) => {
                            self.key_manager.status_message =
                                Some((format!("Import failed: {e}"), true));
                        }
                    }
                }
            }
            crate::ui::dialogs::KeyManagerAction::AddAge { label, recipient } => {
                if self.config.add_age_recipient(&label, &recipient) {
                    let _ = self.config.save();
                    self.key_manager.status_message = Some(("AGE key added".to_string(), false));
                } else {
                    self.key_manager.status_message =
                        Some(("that AGE key is already stored".to_string(), true));
                }
            }
            crate::ui::dialogs::KeyManagerAction::DeleteAge(recipient) => {
                self.config.remove_age_recipient(&recipient);
                let _ = self.config.save();
                self.key_manager.status_message = Some(("AGE key removed".to_string(), false));
            }
            crate::ui::dialogs::KeyManagerAction::GenerateAge => {
                self.age_dialog.show_generate();
            }
        }

        // If the user dismissed the seed-phrase prompt without unlocking,
        // drop the deferred quicknote save rather than letting an
        // unrelated unlock later fire it.
        if self.jot_pending_unlock && !self.age_dialog.open && self.age_identity.is_none() {
            self.jot_pending_unlock = false;
        }

        // Age seed-phrase dialog (unlock / export public key).
        match self.age_dialog.render(ctx) {
            crate::ui::age_dialog::AgeAction::None => {}
            crate::ui::age_dialog::AgeAction::Unlock => {
                let (phrase, passphrase) = self.age_dialog.secrets();
                match crate::crypto::age_backend::AgeIdentity::from_mnemonic(phrase, passphrase) {
                    Ok(identity) => {
                        let recipient = identity.recipient().to_string();
                        self.age_identity = Some(identity);
                        self.age_unlocked_at = Some(ctx.input(|i| i.time));
                        self.age_dialog.close();
                        self.show_toast(format!("AGE identity unlocked ({recipient})"), false, ctx);
                        if let Some(path) = self.pending_age_open.take() {
                            self.open_age_file(&path, ctx);
                        }
                        // A restore that was waiting on the seed phrase
                        // can now decrypt the held edits.
                        if self.restore_after_unlock {
                            self.restore_after_unlock = false;
                            self.restore_held_edits(ctx);
                        }
                        // Now that a key is available, fold in anything
                        // jotted while locked. Silent when nothing is
                        // pending, so ordinary unlocks stay quiet.
                        if self.total_pending() > 0 {
                            self.merge_all_spools(ctx);
                        }
                        // Resume a quicknote save that was waiting on the
                        // seed phrase — the note is still in the jot buffer.
                        if self.jot_pending_unlock {
                            self.jot_pending_unlock = false;
                            self.submit_jot(ctx);
                        }
                    }
                    Err(e) => self.age_dialog.set_error(format!("{e:#}")),
                }
            }
            crate::ui::age_dialog::AgeAction::SaveRecipient(recipient) => {
                if let Some(path) = pick_save_path("age-public-key.txt", "txt") {
                    match keys::atomic_write(&path, format!("{recipient}\n").as_bytes()) {
                        Ok(()) => self.show_toast("Public key saved".to_string(), false, ctx),
                        Err(e) => self.show_toast(format!("Save failed: {e:#}"), true, ctx),
                    }
                }
            }
            crate::ui::age_dialog::AgeAction::AddRecipient(recipient) => {
                if self.config.add_age_recipient("My age key", &recipient) {
                    let _ = self.config.save();
                    self.show_toast("Added to your keys".to_string(), false, ctx);
                } else {
                    self.show_toast("That key is already stored".to_string(), false, ctx);
                }
            }
        }

        // ── Update check result ──────────────────────────────────────
        if let Some(rx) = &self.update_rx {
            if let Ok(result) = rx.try_recv() {
                self.update_rx = None;
                match result {
                    Ok(crate::update::CheckOutcome::UpdateAvailable(latest)) => {
                        self.update_dialog.show(latest);
                    }
                    Ok(crate::update::CheckOutcome::UpToDate) => self.show_toast(
                        format!(
                            "Schl8 {} is the latest version",
                            crate::update::current_version()
                        ),
                        false,
                        ctx,
                    ),
                    Err(e) => self.show_toast(format!("Update check failed: {e}"), true, ctx),
                }
            } else {
                // The check runs on a worker thread; keep repainting so the
                // result is picked up promptly rather than on the next input.
                ctx.request_repaint_after(std::time::Duration::from_millis(150));
            }
        }
        // Vault file management.
        match self.vault_prompt.render(ctx) {
            dialogs::VaultPromptAction::None => {}
            other => self.apply_vault_op(other, ctx),
        }
        // Overwrite conflict: the file changed under an open document.
        if let Some(path) = self.save_conflict.clone() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("This file")
                .to_string();
            let message = format!(
                "\u{201C}{name}\u{201D} has changed on disk since you opened it \u{2014} another \
                 window, a sync client, or a quick-note merge may have written it.\n\n\
                 Saving now replaces that version with yours. To keep both, cancel and \
                 reopen the file in a new window first."
            );
            match crate::ui::dialogs::confirm_modal(
                ctx,
                "File changed on disk",
                &message,
                "Overwrite",
            ) {
                Some(true) => {
                    self.save_conflict = None;
                    self.force_overwrite = true;
                    self.save_in_place(ctx);
                }
                Some(false) => self.save_conflict = None,
                None => {}
            }
        }

        if let Some(target) = self.vault_confirm_delete.clone() {
            let (title, message) = match &target {
                VaultDeleteTarget::File(rel) => (
                    "Delete file",
                    format!("Delete \u{201C}{rel}\u{201D} from the vault? This cannot be undone."),
                ),
                VaultDeleteTarget::Folder(dir) => (
                    "Delete folder",
                    format!(
                        "Delete the folder \u{201C}{dir}\u{201D} and everything inside it? \
                         This cannot be undone."
                    ),
                ),
            };
            match crate::ui::dialogs::confirm_modal(ctx, title, &message, "Delete") {
                Some(true) => {
                    self.vault_confirm_delete = None;
                    self.delete_vault_target(&target, ctx);
                }
                Some(false) => self.vault_confirm_delete = None,
                None => {}
            }
        }

        if self.discard_spool_dialog.render(ctx) {
            let notes: Vec<PathBuf> = self
                .config
                .quick_note
                .notes
                .iter()
                .map(|n| n.source.clone())
                .collect();
            let mut removed = 0usize;
            for note in notes {
                let paths = crate::document::spool::segment_paths(&note);
                removed += paths.len();
                if let Err(e) = crate::document::spool::remove_segments(&paths) {
                    eprintln!("warning: discarding spool for {}: {e:#}", note.display());
                }
            }
            self.invalidate_pending();
            self.show_toast(format!("Discarded {removed} offline entries"), false, ctx);
        }

        if let dialogs::UpdateAction::Open(url) = self.update_dialog.render(ctx) {
            open_url(&url);
        }

        if let Some(url) = self.about_dialog.render(ctx) {
            open_url(&url);
        }
        self.install_help_dialog.render(ctx);
        self.cli_tool_dialog.render(ctx);
        self.render_toolkit_dialog(ctx);
        self.render_backup_dialog(ctx);
        self.render_uninstall_dialog(ctx);

        // Settings dialog — on Apply, persist and apply changes live.
        if let Some((new_config, persist)) = self.settings_dialog.render(ctx) {
            let hotkey_changed = new_config.quick_note.hotkey != self.config.quick_note.hotkey;
            // The start-at-login change (LaunchAgent, not config) only
            // happens on a real save — it isn't a "test live" setting.
            if persist {
                if let Some(enable) = self.settings_dialog.login_item_change() {
                    match crate::login_item::set_enabled(enable) {
                        Ok(()) => self.show_toast(
                            if enable {
                                "Schl8 will start at login".to_string()
                            } else {
                                "Start-at-login disabled".to_string()
                            },
                            false,
                            ctx,
                        ),
                        Err(e) => {
                            self.show_toast(format!("Login item not changed: {e:#}"), true, ctx)
                        }
                    }
                }
            }
            self.config = new_config;

            // Live-apply in both modes — colors, fonts, layout, and the
            // global hotkey take effect immediately so the user can judge
            // them. configure_style rebuilds the widget visuals so every
            // button/label re-derives its colors from the new palette.
            theme::set(&self.config.appearance);
            configure_style(ctx);
            theme::apply_font(ctx, &self.config.appearance.font);
            theme::apply_font_scale(ctx, self.config.appearance.font_scale);
            if hotkey_changed {
                if let Some(resident) = &mut self.resident {
                    if let Err(e) = resident.set_hotkey(&self.config.quick_note.hotkey) {
                        self.show_toast(format!("Hotkey not applied: {e}"), true, ctx);
                    }
                }
            }

            if persist {
                if let Err(e) = self.config.save() {
                    self.show_toast(format!("Could not save settings: {e}"), true, ctx);
                } else {
                    self.show_toast("Settings saved".to_string(), false, ctx);
                }
            } else {
                self.show_toast(
                    "Settings applied for this session — Apply & Save to keep them".to_string(),
                    false,
                    ctx,
                );
            }
        }

        // Save Targets (per-file save plan) editor.
        match self.save_targets.render(ctx) {
            crate::ui::save_targets::PlanAction::None => {}
            crate::ui::save_targets::PlanAction::AddDestination { rule_idx } => {
                let is_age = self.save_targets.rule_is_age(rule_idx);
                let ext = if is_age { "age" } else { "gpg" };
                let suggested = self
                    .save_targets_suggested_name()
                    .map(|n| {
                        // Re-suffix the file's own name to the rule's backend.
                        let base = n
                            .trim_end_matches(".gpg")
                            .trim_end_matches(".asc")
                            .trim_end_matches(".age");
                        format!("{base}.{ext}")
                    })
                    .unwrap_or_else(|| format!("encrypted.{ext}"));
                if let Some(path) = pick_save_path(&suggested, ext) {
                    self.save_targets.add_destination(rule_idx, path);
                }
            }
            crate::ui::save_targets::PlanAction::Apply(plan) => {
                let removed = plan.rules.is_empty();
                let saved_plan = plan.clone();
                self.config.set_plan(plan);
                if let Err(e) = self.config.save() {
                    self.show_toast(format!("Could not save plan: {e}"), true, ctx);
                } else if removed {
                    self.show_toast("Save plan removed".to_string(), false, ctx);
                } else {
                    // Materialize the plan now: encrypt the current content
                    // to every destination immediately, so a newly added
                    // key/location exists as soon as you apply it rather
                    // than only after the next edit + Save.
                    match self.materialize_plan(&saved_plan) {
                        Ok(n) => self.show_toast(
                            format!(
                                "Save plan applied \u{2014} wrote {n} destination{}",
                                if n == 1 { "" } else { "s" }
                            ),
                            false,
                            ctx,
                        ),
                        Err(e) => self.show_toast(
                            format!("Plan saved, but writing now failed: {e:#}"),
                            true,
                            ctx,
                        ),
                    }
                }
            }
        }

        // Favorites manager.
        if self.open_favorites_manager {
            self.open_favorites_manager = false;
            // Quicknote hotkeys are passed in so a favorite can't be given
            // a combo that a note already owns — the two lists live in
            // separate windows, so nothing else would notice the clash.
            let note_hotkeys: Vec<(String, String)> = self
                .config
                .quick_note
                .notes
                .iter()
                .filter(|n| !n.hotkey.trim().is_empty())
                .map(|n| (n.name.clone(), n.hotkey.clone()))
                .collect();
            self.favorites_manager.open_with(
                &self.config.favorites,
                &self.config.quick_note.hotkey,
                note_hotkeys,
            );
        }
        match self.favorites_manager.render(ctx) {
            crate::ui::favorites_manager::FavoritesAction::None => {}
            crate::ui::favorites_manager::FavoritesAction::AddFile => {
                if let Some(path) = pick_open_file() {
                    self.favorites_manager.add_file(path);
                }
            }
            crate::ui::favorites_manager::FavoritesAction::Apply(favorites) => {
                self.config.favorites = favorites;
                if let Err(e) = self.config.save() {
                    self.show_toast(format!("Could not save favorites: {e:#}"), true, ctx);
                } else {
                    self.show_toast("Favorites saved".to_string(), false, ctx);
                }
            }
        }

        self.agent_help.render(ctx);

        // Quick Notes registry manager.
        if self.open_quicknotes_manager {
            self.open_quicknotes_manager = false;
            let age_recips = self.available_age_recipients();
            self.quicknotes_manager.open_with(
                &self.config.quick_note.notes,
                &self.config.quick_note.hotkey,
                age_recips,
            );
        }
        match self.quicknotes_manager.render(ctx) {
            crate::ui::quicknotes_manager::ManagerAction::None => {}
            crate::ui::quicknotes_manager::ManagerAction::PickDestination(slot) => {
                let enc = if self.quicknotes_manager.slot_is_age(slot) {
                    "age"
                } else {
                    "gpg"
                };
                // New notes take their md/txt choice from the form; extra
                // destinations of an existing note keep its inner type so
                // every copy renders the same way.
                let inner = self.quicknotes_manager.slot_inner_kind(slot);
                let suggested = format!("quicknote.{inner}.{enc}");
                if let Some(path) = pick_save_path(&suggested, enc) {
                    self.quicknotes_manager.add_destination(slot, path);
                }
            }
            crate::ui::quicknotes_manager::ManagerAction::AddExistingFile => {
                if let Some(path) = pick_open_file() {
                    self.quicknotes_manager.add_existing(path);
                }
            }
            crate::ui::quicknotes_manager::ManagerAction::Apply(notes) => {
                // Entries dropped from the registry whose file is gone
                // leave a spool that can never merge (a merge appends to
                // the note, which no longer exists) — delete it so no
                // orphan ciphertext accumulates. Entries removed while
                // their file still exists keep their spool: re-adding the
                // note later can still merge those entries.
                let kept: std::collections::HashSet<PathBuf> =
                    notes.iter().map(|n| n.source.clone()).collect();
                for old in &self.config.quick_note.notes {
                    if !kept.contains(&old.source) && !old.source.exists() {
                        let dir = crate::document::spool::spool_dir(&old.source);
                        if dir.exists() {
                            if let Err(e) = std::fs::remove_dir_all(&dir) {
                                eprintln!(
                                    "warning: could not remove dead spool {}: {e}",
                                    dir.display()
                                );
                            }
                        }
                    }
                }
                self.config.set_quicknotes(notes);
                self.invalidate_pending();
                if let Err(e) = self.config.save() {
                    self.show_toast(format!("Could not save quicknotes: {e}"), true, ctx);
                } else {
                    self.show_toast("Quick note files updated".to_string(), false, ctx);
                }
            }
            crate::ui::quicknotes_manager::ManagerAction::Create(note) => {
                // Create the encrypted file(s) now: encrypt a starter
                // blurb to each rule's key and write all destinations.
                let inner = note
                    .source
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| {
                        n.trim_end_matches(".gpg")
                            .trim_end_matches(".asc")
                            .trim_end_matches(".age")
                    })
                    .unwrap_or("");
                let starter = match crate::document::detect_file_type_from_name(inner) {
                    Some(FileType::Markdown) => format!("# {}\n", note.name),
                    _ => String::new(),
                };
                let plan = crate::config::SavePlan {
                    source: note.source.clone(),
                    rules: note.rules.clone(),
                    ..Default::default()
                };
                let results = crate::document::multisave::execute(starter.as_bytes(), &plan);
                let failures: Vec<String> = results
                    .iter()
                    .filter_map(|r| {
                        r.result
                            .as_ref()
                            .err()
                            .map(|e| format!("{}: {e:#}", r.destination.display()))
                    })
                    .collect();
                if failures.is_empty() {
                    // Register immediately (not just in the draft) so a
                    // later Cancel can't orphan the created file.
                    let mut notes = self.config.quick_note.notes.clone();
                    notes.push(note.clone());
                    self.config.set_quicknotes(notes);
                    self.config.quick_note.last_target = Some(note.source.clone());
                    if let Err(e) = self.config.save() {
                        self.show_toast(format!("Could not save config: {e}"), true, ctx);
                    }
                    let name = note.name.clone();
                    self.quicknotes_manager.created(note);
                    self.invalidate_pending();
                    self.show_toast(format!("Created quicknote \"{name}\""), false, ctx);
                } else {
                    self.quicknotes_manager
                        .set_error(format!("Create failed — {}", failures.join("; ")));
                }
            }
        }

        // Copy-enable security warning.
        if let Some(choice) = self.copy_warning.render(ctx) {
            self.allow_copy = true;
            let mut dirty = false;
            if choice.suppress_future {
                self.config.security.suppress_copy_warning = true;
                dirty = true;
            }
            if choice.remember_default {
                self.config.security.allow_copy_default = true;
                dirty = true;
            }
            if dirty {
                let _ = self.config.save();
            }
        }

        // ── Quick-note window ────────────────────────────────────────────
        // Poll a running append first
        if let Some(rx) = &self.jot_rx {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    self.jot_rx = None;
                    self.jot.busy = false;
                    self.jot.clear_text();
                    self.jot.open = false;
                    let name = self
                        .config
                        .quick_note
                        .last_target
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .unwrap_or("file")
                        .to_string();
                    self.show_toast(format!("Note appended to {name}"), false, ctx);
                }
                Ok(Err(e)) => {
                    self.jot_rx = None;
                    self.jot.busy = false;
                    self.jot.status = Some(e);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint();
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.jot_rx = None;
                    self.jot.busy = false;
                    self.jot.status = Some("append task terminated unexpectedly".to_string());
                }
            }
        }

        // The jot window floats in its own borderless viewport; the main
        // window hides while it is open so the main GUI isn't visible.
        let jot_action = if self.jot.open {
            if self.main_visible && !self.main_hidden_for_jot {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.main_hidden_for_jot = true;
            }
            // Keep the loop ticking while the main window is hidden so the
            // child viewport keeps rendering.
            ctx.request_repaint();
            self.render_jot_viewport(ctx)
        } else {
            if self.main_hidden_for_jot {
                if self.main_visible {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                self.main_hidden_for_jot = false;
            }
            quicknote::JotAction::None
        };

        // Persist the jot window's geometry when it closes, so it reopens
        // with the same size and position next time (and next launch).
        if self.jot_was_open && !self.jot.open {
            if let Some((pos, size)) = self.jot_last_geometry {
                let new_pos = Some([pos.x, pos.y]);
                let new_size = Some([size.x, size.y]);
                if self.config.quick_note.window_pos != new_pos
                    || self.config.quick_note.window_size != new_size
                {
                    self.config.quick_note.window_pos = new_pos;
                    self.config.quick_note.window_size = new_size;
                    if let Err(e) = self.config.save() {
                        eprintln!("warning: could not save config: {e:#}");
                    }
                }
            }
        }
        self.jot_was_open = self.jot.open;

        match jot_action {
            quicknote::JotAction::None => {}
            quicknote::JotAction::BrowseTarget => {
                if let Some(path) = pick_open_file() {
                    if self.config.add_target(path.clone()) {
                        if let Err(e) = self.config.save() {
                            eprintln!("warning: could not save config: {e:#}");
                        }
                        self.jot.selected_target = Some(path);
                    } else {
                        self.jot.status = Some(format!(
                            "Registry is full ({} quicknotes) — remove one first",
                            crate::config::MAX_QUICKNOTES
                        ));
                    }
                }
            }
            quicknote::JotAction::Manage => {
                // Hide the jot (typed text is kept for when it reopens)
                // and open the manager in the main window.
                self.jot.open = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                self.main_visible = true;
                self.open_quicknotes_manager = true;
            }
            quicknote::JotAction::Submit => {
                self.submit_jot(ctx);
            }
        }

        // Quit confirmation dialog (unsaved edits)
        if self.quit_dialog.render(ctx) {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Discard confirmation dialog
        if self.discard_dialog.render(ctx) {
            // User confirmed discard — exit edit mode, drop buffer
            match &mut self.state {
                State::Viewing {
                    edit_buffer,
                    modified,
                    ..
                }
                | State::ViewingArchive {
                    edit_buffer,
                    modified,
                    ..
                } => {
                    *edit_buffer = None;
                    *modified = false;
                }
                _ => {}
            }
            self.show_toast("Edits discarded.".to_string(), false, ctx);
        }

        // ── Render main content and collect transition ────────────────────
        // On-disk identity for the status bar (cached; re-hashes only when
        // the file's path or mtime changes).
        let stamp = match &self.state {
            State::Viewing { doc, .. } => self.file_stamp.get(&doc.source_path),
            State::ViewingArchive { archive, .. } => self.file_stamp.get(&archive.source_path),
            _ => None,
        };

        let view_opts = viewer::ViewOptions {
            word_wrap: self.config.appearance.word_wrap,
            line_numbers: self.config.appearance.line_numbers,
            // Only while the find bar is open with a query, so the
            // highlighting text copy exists only during a search.
            find: (self.find.open && !self.find.query.is_empty()).then_some(
                crate::ui::highlight::Highlight {
                    query: &self.find.query,
                    active: self.find.active,
                },
            ),
        };
        // The crawl drives the scroll position only while it is actually
        // moving. Forcing an absolute offset every frame — including
        // while paused or mid-wheel — overwrote the reader's own
        // scrolling before it could take effect, which is why the wheel
        // felt dead. When it is not driving, the view is an ordinary
        // scroll view.
        if self.crawl.drives_scroll(ctx.input(|i| i.time)) {
            self.pending_jump = Some(self.crawl.offset as f32);
        }
        // Crawl reads like focus mode — one centered column, wrapped.
        let chromeless = self.focus_mode || self.crawl.active;
        // Focus mode keeps its fixed column; a crawl uses the configured
        // one (0 = full width), since reading a moving column is exactly
        // where line length matters most.
        let column_width = if self.crawl.active && self.config.crawl.column_width > 0.0 {
            self.config.crawl.column_width
        } else if self.crawl.active {
            f32::MAX
        } else {
            720.0
        };
        // Computed before the match, which borrows `self.state` mutably.
        // True only while there is unsaved text AND no key to encrypt it
        // to — the window in which Lock Now would discard it.
        let unsaved_unprotected = (self.has_unsaved_edits() || !self.jot.text().trim().is_empty())
            && !self.can_secure_unsaved_work();
        let content_transition = match &mut self.state {
            State::FilePicker => render_file_picker(ctx, &self.config, &mut self.recent_stamps),
            State::Decrypting { path, .. } => render_decrypting(ctx, path),
            State::Viewing {
                doc,
                scroll_offset,
                lines_count,
                current_line,
                edit_buffer,
                modified,
            } => render_viewing(
                ctx,
                doc,
                scroll_offset,
                lines_count,
                current_line,
                edit_buffer,
                modified,
                chromeless,
                keybindings::Layout::parse(&self.config.app.keyboard_layout),
                stamp.as_ref(),
                unsaved_unprotected,
                view_opts,
                &mut self.pending_jump,
                &mut self.view_metrics,
                column_width,
            ),
            State::ViewingArchive {
                archive,
                tree,
                selected,
                selected_dir,
                scroll_offset,
                lines_count,
                current_line,
                edit_buffer,
                modified,
            } => render_viewing_archive(
                ctx,
                archive,
                tree,
                selected,
                selected_dir,
                scroll_offset,
                lines_count,
                current_line,
                edit_buffer,
                modified,
                chromeless,
                keybindings::Layout::parse(&self.config.app.keyboard_layout),
                stamp.as_ref(),
                unsaved_unprotected,
                view_opts,
                &mut self.pending_jump,
                &mut self.view_metrics,
                column_width,
            ),
            State::Locked {
                relock_path,
                held,
                warning,
            } => render_locked(
                ctx,
                relock_path.as_deref(),
                held.as_ref(),
                warning.as_deref(),
            ),
            State::Error {
                message,
                failed_path,
            } => render_error(ctx, message, failed_path),
        };

        // Content transitions take effect only if menu didn't already set one
        if matches!(transition, Transition::None) {
            transition = content_transition;
        }

        // ── Live statistics card (View → Statistics) ─────────────────────
        if self.config.app.show_stats {
            self.render_stats_card(ctx);
        }

        // ── Render toast notification ────────────────────────────────────
        if let Some((msg, is_error, until)) = &self.toast {
            let now = ctx.input(|i| i.time);
            if now < *until {
                egui::Area::new(egui::Id::new("toast"))
                    .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -48.0))
                    .show(ctx, |ui| {
                        // Theme accents with contrast-checked text, so the
                        // toast is readable on every palette (the old
                        // hardcoded dark fills were illegible on light
                        // themes).
                        let bg = if *is_error {
                            theme::accent_red()
                        } else {
                            theme::accent_green()
                        };
                        egui::Frame::NONE
                            .fill(bg)
                            .corner_radius(6.0)
                            .stroke(egui::Stroke::new(
                                1.0,
                                theme::contrast_text(bg).gamma_multiply(0.25),
                            ))
                            .inner_margin(egui::Margin::symmetric(16, 8))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(msg.as_str())
                                        .size(13.0)
                                        .strong()
                                        .color(theme::contrast_text(bg)),
                                );
                            });
                    });
                ctx.request_repaint();
            } else {
                self.toast = None;
            }
        }

        // Held edits waiting for their document: install them as soon as
        // it has finished opening. Runs before the transitions below so a
        // restore triggered this frame gets its chance next frame.
        self.apply_pending_restore(ctx);

        // ── Apply transitions ────────────────────────────────────────────
        match transition {
            Transition::StartDecrypt(path) => {
                // age files decrypt synchronously on this thread with the
                // in-memory identity (fast, no subprocess/PIN). GPG and
                // plaintext keep the background path.
                if crate::document::loader::is_age_file(&path) {
                    self.open_age_file(&path, ctx);
                } else {
                    let receiver = spawn_decrypt(path.clone());
                    self.state = State::Decrypting { path, receiver };
                }
            }
            Transition::PanicLock => {
                if self.document_open() || self.jot.open {
                    self.lock_session(ctx);
                }
            }
            Transition::DiscardHeldEdits => {
                crate::document::stash::clear();
                // Drop the summary so the panel goes away, then reopen the
                // last saved version if there was one.
                let reopen = match &self.state {
                    State::Locked { relock_path, .. } => relock_path.clone(),
                    _ => None,
                };
                self.state = State::Locked {
                    relock_path: reopen.clone(),
                    held: None,
                    warning: None,
                };
                self.show_toast("Held edits discarded".to_string(), false, ctx);
                if let Some(path) = reopen {
                    self.begin_open(path, ctx);
                }
            }
            Transition::RestoreHeldEdits => {
                self.restore_held_edits(ctx);
            }
            Transition::NewFile(file_type) => {
                let label = match file_type {
                    FileType::Markdown => "untitled.md.gpg",
                    FileType::PlainText => "untitled.txt.gpg",
                };
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "Schl8 \u{2014} {label} (new)"
                )));
                self.state = new_empty_document(file_type);
                // A file with no key of its own cannot have its unsaved
                // text encrypted into the lock stash, so Lock Now would
                // discard it. Say so now, while saving is still one
                // keystroke away — afterwards it is too late to matter.
                if !self.can_secure_unsaved_work() {
                    self.show_toast_for(
                        "New file: save it before locking. Until the first save there \
                         is no key to protect unsaved text with, so Lock Now would \
                         discard it. Settings \u{203A} Security can set a stash key \
                         that removes this gap."
                            .to_string(),
                        true,
                        12.0,
                        ctx,
                    );
                }
            }
            Transition::OpenJot => {
                self.jot.show();
            }
            Transition::Quit => {
                self.quit_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Transition::CloseDocument => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Title("Schl8".to_string()));
                self.state = State::FilePicker;
            }
            Transition::RequestDiscard => {
                let modified = matches!(
                    &self.state,
                    State::Viewing { modified: true, .. }
                        | State::ViewingArchive { modified: true, .. }
                );
                if modified {
                    // Show confirmation dialog
                    self.discard_dialog.open = true;
                } else {
                    // Nothing to lose — just exit edit mode
                    match &mut self.state {
                        State::Viewing { edit_buffer, .. }
                        | State::ViewingArchive { edit_buffer, .. } => *edit_buffer = None,
                        _ => {}
                    }
                }
            }
            Transition::RequestSaveAndExit => {
                // Streamlined save: same key(s) + same location (or the
                // file's save plan) without asking. Only files with no
                // known recipients open the Encrypt & Save As dialog.
                match self.save_in_place(ctx) {
                    SaveOutcome::Saved => match &mut self.state {
                        State::Viewing { edit_buffer, .. }
                        | State::ViewingArchive { edit_buffer, .. } => *edit_buffer = None,
                        _ => {}
                    },
                    SaveOutcome::Failed => {} // stay in edit mode; toast shown
                    SaveOutcome::NeedsDialog => {
                        if let State::Viewing { doc, .. } = &self.state {
                            self.exit_edit_after_save = true;
                            let default_armor = !source_is_binary(&doc.source_path);
                            self.open_encrypt_dialog(default_armor);
                        }
                    }
                }
            }
            Transition::EnterEdit => {
                if !self.is_editing() {
                    self.toggle_edit_mode();
                }
            }
            Transition::OpenSaveTargets => {
                let age_recips = self.available_age_recipients();
                let age_self = self.own_age_recipient_for_state();
                match &self.state {
                    State::Viewing { doc, .. } => {
                        if doc.source_path.is_absolute() {
                            self.save_targets.open_for(
                                &doc.source_path,
                                self.config.plan_for(&doc.source_path),
                                doc.recipients.as_deref(),
                                age_recips,
                                age_self.as_deref(),
                            );
                        } else {
                            self.show_toast(
                                "Save the new file once (Encrypt & Save As) before configuring \
                                 targets"
                                    .to_string(),
                                true,
                                ctx,
                            );
                        }
                    }
                    State::ViewingArchive { archive, .. } => {
                        self.save_targets.open_for(
                            &archive.source_path,
                            self.config.plan_for(&archive.source_path),
                            archive.recipients.as_deref(),
                            age_recips,
                            None,
                        );
                    }
                    _ => {}
                }
            }
            Transition::RemoveRecent(path) => {
                self.config.remove_recent(&path);
                let _ = self.config.save();
            }
            Transition::VaultAddPrompt => {
                // New files default into the selected folder, else the
                // selected file's folder.
                let parent = self
                    .selected_vault_folder()
                    .unwrap_or_else(|| self.selected_vault_dir());
                self.vault_prompt.add_file(&parent);
            }
            Transition::VaultAddFolderPrompt => {
                let parent = self
                    .selected_vault_folder()
                    .unwrap_or_else(|| self.selected_vault_dir());
                self.vault_prompt.add_folder(&parent);
            }
            Transition::VaultRenamePrompt => {
                // Folder selection wins over the file selection.
                if let Some(dir) = self.selected_vault_folder() {
                    self.vault_prompt.rename(&dir, true);
                } else if let Some(rel) = self.selected_vault_entry() {
                    self.vault_prompt.rename(&rel, false);
                }
            }
            Transition::VaultDeletePrompt => {
                if let Some(dir) = self.selected_vault_folder() {
                    self.vault_confirm_delete = Some(VaultDeleteTarget::Folder(dir));
                } else if let Some(rel) = self.selected_vault_entry() {
                    self.vault_confirm_delete = Some(VaultDeleteTarget::File(rel));
                }
            }
            Transition::None => {}
        }
    }
}

// ── Render functions ──────────────────────────────────────────────────────

/// One row's worth of list data, shared by the Recent and Quick Notes
/// columns so both render identically.
struct ListRow {
    path: PathBuf,
    /// Bold first line (file name, or the quicknote's display name).
    title: String,
    /// Right-hand detail appended after the size, e.g. "opened <when>".
    trailing: String,
    /// Sort key: last-modified as "%Y-%m-%d %H:%M" (lexicographic order is
    /// chronological). Empty for files that are gone, which sort last.
    modified: String,
    stamp: Option<statusbar::FileStamp>,
    /// Offer a remove button (Recent entries whose file vanished).
    removable: bool,
}

fn render_file_picker(
    ctx: &egui::Context,
    config: &Config,
    stamps: &mut RecentStamps,
) -> Transition {
    let mut transition = Transition::None;

    // Load the icon texture once and cache it (avoid nested ctx locks)
    let texture: egui::TextureHandle = {
        let cached: Option<egui::TextureHandle> =
            ctx.memory(|mem| mem.data.get_temp(egui::Id::new("schl8_icon_tex")));
        if let Some(t) = cached {
            t
        } else {
            let png_bytes = include_bytes!("../assets/schl8.iconset/icon_256x256.png");
            let img = image::load_from_memory(png_bytes).expect("embedded PNG is valid");
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            let color_image =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
            let tex = ctx.load_texture("schl8-icon", color_image, egui::TextureOptions::LINEAR);
            ctx.memory_mut(|mem| {
                mem.data
                    .insert_temp(egui::Id::new("schl8_icon_tex"), tex.clone());
            });
            tex
        }
    };

    // ── Build both lists ─────────────────────────────────────────────
    // Stamps come from one cache, so a file listed in both columns is
    // read (and hashed) once per frame, not twice.
    let recent_rows: Vec<ListRow> = config
        .recent_files
        .iter()
        .map(|r| {
            let stamp = stamps.get(&r.path);
            let opened = chrono::DateTime::parse_from_rfc3339(&r.last_opened)
                .map(|t| {
                    t.with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            ListRow {
                title: r
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
                    .to_string(),
                trailing: if opened.is_empty() {
                    String::new()
                } else {
                    format!("opened {opened}")
                },
                modified: stamp
                    .as_ref()
                    .map(|s| s.modified.clone())
                    .unwrap_or_default(),
                stamp,
                path: r.path.clone(),
                // Recent is a history list, so a dead entry is clutter the
                // user should be able to clear; the quicknote registry is
                // managed in its own window instead.
                removable: true,
            }
        })
        .collect();

    let mut note_rows: Vec<ListRow> = config
        .quick_note
        .notes
        .iter()
        .map(|n| {
            let stamp = stamps.get(&n.source);
            let pending = crate::document::spool::pending_count(&n.source);
            ListRow {
                title: n.name.clone(),
                trailing: if pending > 0 {
                    format!("{pending} pending")
                } else {
                    String::new()
                },
                modified: stamp
                    .as_ref()
                    .map(|s| s.modified.clone())
                    .unwrap_or_default(),
                stamp,
                path: n.source.clone(),
                removable: false,
            }
        })
        .collect();
    // Most recently edited first. The timestamp is already formatted
    // "%Y-%m-%d %H:%M", so a plain string compare is chronological; notes
    // whose file is missing have no stamp and sort to the bottom.
    note_rows.sort_by(|a, b| b.modified.cmp(&a.modified));

    let have_lists = !recent_rows.is_empty() || !note_rows.is_empty();

    // Footer in its own bottom panel so it can never be pushed off a
    // short window — and, more importantly, so it never competes with
    // the buttons for vertical space.
    egui::TopBottomPanel::bottom("picker_footer")
        .frame(
            egui::Frame::NONE
                .fill(theme::bg_primary())
                .inner_margin(egui::Margin::symmetric(0, 8)),
        )
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.horizontal(|ui| {
                    let changelog = crate::update::changelog_url();
                    let approx = 260.0;
                    ui.add_space(((ui.available_width() - approx) / 2.0).max(0.0));
                    if ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "v{} \u{2014} changelog",
                                    crate::update::current_version()
                                ))
                                .size(12.0)
                                .color(theme::accent()),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_text(&changelog)
                        .clicked()
                    {
                        open_url(&changelog);
                    }
                    ui.label(
                        egui::RichText::new("\u{B7}")
                            .size(12.0)
                            .color(theme::text_dim()),
                    );
                    if ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new("Report an issue \u{2197}")
                                    .size(12.0)
                                    .color(theme::accent()),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_text(ISSUES_URL)
                        .clicked()
                    {
                        open_url(ISSUES_URL);
                    }
                });
            });
        });

    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(theme::bg_primary())
                .inner_margin(theme::CONTENT_PADDING),
        )
        .show(ctx, |ui| {
            // ── Header + buttons, pinned at the top ──────────────────
            // Everything the user needs to *act* lives above the lists,
            // so shrinking the window can only ever clip list rows —
            // and those scroll. Before this, the buttons sat under a
            // 320px-tall list and left the window entirely on a short
            // display.
            ui.vertical_centered(|ui| {
                if have_lists {
                    ui.add_space(14.0);
                    ui.label(theme::gradient_text("Schl8", 24.0));
                    ui.add_space(1.0);
                    ui.label(
                        egui::RichText::new("Schuyler's Lightweight Armored Text Editor")
                            .size(11.5)
                            .color(theme::text_dim()),
                    );
                    ui.add_space(12.0);
                } else {
                    // First run (no history): the full logo greets the user.
                    ui.add_space(40.0);
                    ui.add(egui::Image::new(&texture).fit_to_exact_size(egui::vec2(112.0, 112.0)));
                    ui.add_space(14.0);
                    ui.label(theme::gradient_text("Schl8", 32.0));
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("Schuyler's Lightweight Armored Text Editor")
                            .size(14.0)
                            .color(theme::text_dim()),
                    );
                    ui.add_space(22.0);
                }

                let button = egui::Button::new(
                    egui::RichText::new("  Open Encrypted File  ")
                        .size(16.0)
                        .color(theme::badge_text()),
                )
                .fill(theme::badge_bg())
                .corner_radius(6.0)
                .min_size(egui::vec2(220.0, 44.0));

                if ui.add(button).clicked() {
                    if let Some(path) = pick_open_file() {
                        transition = Transition::StartDecrypt(path);
                    }
                }

                ui.add_space(10.0);

                // New-file and quick-note buttons, centered as a row
                ui.horizontal(|ui| {
                    let total_width = 368.0;
                    let available = ui.available_width();
                    if available > total_width {
                        ui.add_space((available - total_width) / 2.0);
                    }

                    let secondary = |label: &str| {
                        egui::Button::new(
                            egui::RichText::new(label)
                                .size(13.0)
                                .color(theme::text_primary()),
                        )
                        .fill(theme::bg_raised())
                        .corner_radius(6.0)
                        .min_size(egui::vec2(112.0, 32.0))
                    };

                    if ui.add(secondary("New Markdown")).clicked() {
                        transition = Transition::NewFile(FileType::Markdown);
                    }
                    ui.add_space(10.0);
                    if ui.add(secondary("New Text")).clicked() {
                        transition = Transition::NewFile(FileType::PlainText);
                    }
                    ui.add_space(10.0);
                    // Named to match its neighbours: all three create
                    // something new, so all three read "New …".
                    if ui.add(secondary("New Quick Note")).clicked() {
                        transition = Transition::OpenJot;
                    }
                });
            });

            // ── The two lists, filling whatever height is left ───────
            if have_lists {
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);
                // Both columns scroll independently, so a long history
                // never pushes the other list (or anything above) away.
                ui.columns(2, |cols| {
                    render_list_column(
                        &mut cols[0],
                        "Recent",
                        "Files you opened recently.",
                        &recent_rows,
                        &mut transition,
                    );
                    render_list_column(
                        &mut cols[1],
                        "Quick Notes",
                        "Your quicknote files, most recently edited first.",
                        &note_rows,
                        &mut transition,
                    );
                });
            }
        });

    transition
}

/// Render one titled, scrollable column of file rows.
fn render_list_column(
    ui: &mut egui::Ui,
    title: &str,
    hint: &str,
    rows: &[ListRow],
    transition: &mut Transition,
) {
    ui.label(
        egui::RichText::new(title)
            .size(13.0)
            .strong()
            .color(theme::text_dim()),
    )
    .on_hover_text(hint);
    ui.add_space(6.0);

    if rows.is_empty() {
        ui.label(
            egui::RichText::new(if title == "Recent" {
                "Nothing opened yet."
            } else {
                "No quicknotes yet."
            })
            .size(11.5)
            .color(theme::text_dim()),
        );
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt(("picker_list", title))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for row in rows {
                let missing = row.stamp.is_none();
                let show_remove = missing && row.removable;
                // Width is decided BEFORE the text is laid out: a button
                // sizes to its content, so an unwrapped detail line would
                // push the row past its column and overlap the list next
                // to it. Wrapping to the column is what keeps two lists
                // side by side honest.
                let avail = ui.available_width();
                let btn_width = if show_remove {
                    (avail - 30.0).max(80.0)
                } else {
                    avail
                };
                let text_width = (btn_width - 16.0).max(40.0);

                // Name on top, identity line (size · saved · …) below.
                let mut job = egui::text::LayoutJob::default();
                job.wrap.max_width = text_width;
                job.wrap.max_rows = 2;
                job.wrap.overflow_character = Some('\u{2026}');
                job.append(
                    &row.title,
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::proportional(13.0),
                        color: if missing {
                            theme::text_dim()
                        } else {
                            theme::text_strong()
                        },
                        ..Default::default()
                    },
                );
                // Size leads the detail line: it's the one field that
                // says at a glance whether a note still holds anything.
                // The hash is deliberately not here — two columns don't
                // have the width, and it's in the hover text instead.
                let detail = match &row.stamp {
                    Some(st) => {
                        let mut d = format!("\n{}  \u{B7}  {}", st.size_label(), st.modified);
                        if !row.trailing.is_empty() {
                            d.push_str(&format!("  \u{B7}  {}", row.trailing));
                        }
                        d
                    }
                    None => {
                        let mut d = "\n(missing)".to_string();
                        if !row.trailing.is_empty() {
                            d.push_str(&format!("  \u{B7}  {}", row.trailing));
                        }
                        d
                    }
                };
                job.append(
                    &detail,
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::monospace(10.0),
                        color: theme::text_dim(),
                        ..Default::default()
                    },
                );

                // The row is compact, so the full identity lives in the
                // tooltip: path, hash and size, none of it truncated.
                let hover = match &row.stamp {
                    Some(st) => format!(
                        "{}\n#{}  \u{B7}  {}  \u{B7}  saved {}",
                        row.path.display(),
                        st.hash8,
                        st.size_label(),
                        st.modified
                    ),
                    None => format!("{}\n(file is missing)", row.path.display()),
                };

                ui.horizontal(|ui| {
                    let btn = egui::Button::new(job)
                        .fill(theme::bg_raised().gamma_multiply(0.55))
                        .corner_radius(theme::RADIUS)
                        .min_size(egui::vec2(btn_width, 40.0));
                    let resp = ui.add_enabled(!missing, btn).on_hover_text(hover);
                    if resp.clicked() {
                        *transition = Transition::StartDecrypt(row.path.clone());
                    }
                    if show_remove
                        && ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("x").size(12.0).color(theme::text_dim()),
                                )
                                .fill(theme::bg_raised().gamma_multiply(0.55))
                                .corner_radius(theme::RADIUS)
                                .min_size(egui::vec2(24.0, 40.0)),
                            )
                            .on_hover_text("Remove from Recent (the file is gone)")
                            .clicked()
                    {
                        *transition = Transition::RemoveRecent(row.path.clone());
                    }
                });
                ui.add_space(3.0);
            }
        });
}

/// The project's GitHub issue tracker (pre-filled for a bug report).
const ISSUES_URL: &str =
    "https://github.com/schbz/schl8/issues/new?labels=bug&template=bug_report.md";

/// Open a URL in the default browser (macOS `open`). Used only for fixed
/// project URLs — never document content, so nothing sensitive reaches
/// the browser or its history.
fn open_url(url: &str) {
    if let Err(e) = std::process::Command::new("/usr/bin/open").arg(url).spawn() {
        eprintln!("could not open {url}: {e}");
    }
}

fn render_decrypting(ctx: &egui::Context, path: &std::path::Path) -> Transition {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(theme::bg_primary())
                .inner_margin(theme::CONTENT_PADDING),
        )
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(160.0);

                ui.spinner();

                ui.add_space(16.0);

                ui.label(
                    egui::RichText::new("Decrypting\u{2026}")
                        .size(20.0)
                        .color(theme::text_primary()),
                );

                ui.add_space(8.0);

                ui.label(
                    egui::RichText::new(filename)
                        .size(14.0)
                        .color(theme::accent())
                        .monospace(),
                );

                ui.add_space(16.0);

                ui.label(
                    egui::RichText::new(
                        "If prompted, enter your YubiKey PIN in the pinentry dialog",
                    )
                    .size(12.0)
                    .color(theme::text_dim()),
                );
            });
        });

    Transition::None
}

#[allow(clippy::too_many_arguments)]
fn render_viewing(
    ctx: &egui::Context,
    doc: &Document,
    scroll_offset: &mut f32,
    lines_count: &mut usize,
    current_line: &mut usize,
    edit_buffer: &mut Option<SecureString>,
    modified: &mut bool,
    focus_mode: bool,
    layout: keybindings::Layout,
    stamp: Option<&statusbar::FileStamp>,
    // True when unsaved edits exist that cannot be encrypted into the
    // lock stash — an unsaved new file has no key of its own.
    unsaved_unprotected: bool,
    opts: viewer::ViewOptions,
    jump: &mut Option<f32>,
    metrics: &mut (f32, f32, f32),
    column_width: f32,
) -> Transition {
    let mut transition = Transition::None;
    let is_editing = edit_buffer.is_some();
    // Focus mode is a fixed readable column — horizontal scrolling makes
    // no sense there, so wrap is always on while it's active.
    let opts = if focus_mode {
        viewer::ViewOptions {
            word_wrap: true,
            ..opts
        }
    } else {
        opts
    };

    // Handle keyboard input only in view mode (not edit mode)
    if !is_editing {
        let any_dialog_open = ctx.memory(|m| m.any_popup_open());
        if !any_dialog_open {
            let mut action = None;
            ctx.input(|input| {
                for event in &input.events {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = event
                    {
                        // Don't handle keys that are menu shortcuts
                        if modifiers.command {
                            continue;
                        }
                        if let Some(a) = keybindings::map_key(*key, modifiers, layout) {
                            action = Some(a);
                        }
                    }
                }
            });

            if let Some(a) = action {
                let line_height = theme::FONT_SIZE + theme::LINE_SPACING;
                let page_lines = 20;

                match a {
                    keybindings::Action::ScrollDown => {
                        *scroll_offset += line_height;
                        *current_line = (*current_line + 1).min(*lines_count);
                    }
                    keybindings::Action::ScrollUp => {
                        *scroll_offset -= line_height;
                        *current_line = current_line.saturating_sub(1).max(1);
                    }
                    keybindings::Action::PageDown => {
                        *scroll_offset += line_height * page_lines as f32;
                        *current_line = (*current_line + page_lines).min(*lines_count);
                    }
                    keybindings::Action::PageUp => {
                        *scroll_offset -= line_height * page_lines as f32;
                        *current_line = current_line.saturating_sub(page_lines).max(1);
                    }
                    keybindings::Action::GoToTop => {
                        *scroll_offset = f32::MIN;
                        *current_line = 1;
                    }
                    keybindings::Action::GoToBottom => {
                        *scroll_offset = f32::MAX;
                        *current_line = *lines_count;
                    }
                    keybindings::Action::Quit => {
                        transition = Transition::Quit;
                    }
                }
            }
        }
    }

    let filename = doc
        .source_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    // Status bar at bottom (hidden in focus mode)
    if !focus_mode {
        // The bar switches to a two-row layout when narrow, so the panel
        // must size to its content rather than a fixed height — otherwise
        // the extra row is drawn below the bottom of the window.
        let compact = statusbar::is_compact(ctx.available_rect().width(), is_editing);
        let status_action = egui::TopBottomPanel::bottom("statusbar")
            .min_height(statusbar::min_bar_height(is_editing))
            .frame(
                egui::Frame::NONE
                    .fill(theme::bg_statusbar())
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show(ctx, |ui| {
                statusbar::render(
                    ui,
                    filename,
                    &doc.source_path.display().to_string(),
                    *current_line,
                    *lines_count,
                    is_editing,
                    *modified,
                    None,
                    &doc.signature,
                    is_editing && *modified,
                    unsaved_unprotected,
                    stamp,
                    compact,
                )
            })
            .inner;
        if let Some(sa) = status_action {
            match sa {
                statusbar::StatusAction::DiscardEdits => {
                    transition = Transition::RequestDiscard;
                }
                statusbar::StatusAction::SaveAndExit => {
                    transition = Transition::RequestSaveAndExit;
                }
                statusbar::StatusAction::EnterEdit => {
                    transition = Transition::EnterEdit;
                }
                statusbar::StatusAction::OpenSaveOptions => {
                    transition = Transition::OpenSaveTargets;
                }
                statusbar::StatusAction::PanicLock => {
                    transition = Transition::PanicLock;
                }
            }
        }
    }

    // Main content area
    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(theme::bg_primary())
                .inner_margin(theme::CONTENT_PADDING),
        )
        .show(ctx, |ui| {
            // In focus mode, cap the text to a comfortable reading column.
            let mut render_body = |ui: &mut egui::Ui| {
                if let Some(buf) = edit_buffer {
                    let (changed, info) = viewer::render_editable(ui, buf, jump, opts);
                    if changed {
                        *modified = true;
                    }
                    *lines_count = buf.as_str().lines().count().max(1);
                    *metrics = (info.content_height, info.viewport_height, info.offset_y);
                } else {
                    let content = doc.content.as_str().unwrap_or("[unable to decode content]");
                    let info =
                        viewer::render(ui, content, doc.file_type, scroll_offset, jump, opts);
                    // Track the line from the actual scroll position so
                    // mouse/trackpad scrolling updates it too.
                    *current_line = info.current_line(*lines_count);
                    *metrics = (info.content_height, info.viewport_height, info.offset_y);
                }
            };
            if focus_mode {
                let avail = ui.available_width();
                let column = column_width.min(avail);
                let pad = ((avail - column) / 2.0).max(0.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(pad);
                    ui.vertical(|ui| {
                        ui.set_max_width(column);
                        render_body(ui);
                    });
                });
            } else {
                render_body(ui);
            }
        });

    transition
}

/// Render a decrypted folder archive: sidebar file tree + document view.
#[allow(clippy::too_many_arguments)]
fn render_viewing_archive(
    ctx: &egui::Context,
    archive: &ArchiveDocument,
    tree: &filetree::TreeNode,
    selected: &mut usize,
    selected_dir: &mut Option<String>,
    scroll_offset: &mut f32,
    lines_count: &mut usize,
    current_line: &mut usize,
    edit_buffer: &mut Option<SecureString>,
    modified: &mut bool,
    focus_mode: bool,
    layout: keybindings::Layout,
    stamp: Option<&statusbar::FileStamp>,
    // See `render_viewing`.
    unsaved_unprotected: bool,
    opts: viewer::ViewOptions,
    jump: &mut Option<f32>,
    metrics: &mut (f32, f32, f32),
    column_width: f32,
) -> Transition {
    let mut transition = Transition::None;
    let is_editing = edit_buffer.is_some();
    // Focus mode always wraps (fixed readable column, no h-scroll).
    let opts = if focus_mode {
        viewer::ViewOptions {
            word_wrap: true,
            ..opts
        }
    } else {
        opts
    };

    // Keyboard navigation (same vim-style bindings as single-file view);
    // disabled while editing so typing lands in the editor.
    let any_dialog_open = ctx.memory(|m| m.any_popup_open());
    if !is_editing && !any_dialog_open {
        let mut action = None;
        ctx.input(|input| {
            for event in &input.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    if modifiers.command {
                        continue;
                    }
                    if let Some(a) = keybindings::map_key(*key, modifiers, layout) {
                        action = Some(a);
                    }
                }
            }
        });

        if let Some(a) = action {
            let line_height = theme::FONT_SIZE + theme::LINE_SPACING;
            let page_lines = 20;

            match a {
                keybindings::Action::ScrollDown => {
                    *scroll_offset += line_height;
                    *current_line = (*current_line + 1).min(*lines_count);
                }
                keybindings::Action::ScrollUp => {
                    *scroll_offset -= line_height;
                    *current_line = current_line.saturating_sub(1).max(1);
                }
                keybindings::Action::PageDown => {
                    *scroll_offset += line_height * page_lines as f32;
                    *current_line = (*current_line + page_lines).min(*lines_count);
                }
                keybindings::Action::PageUp => {
                    *scroll_offset -= line_height * page_lines as f32;
                    *current_line = current_line.saturating_sub(page_lines).max(1);
                }
                keybindings::Action::GoToTop => {
                    *scroll_offset = f32::MIN;
                    *current_line = 1;
                }
                keybindings::Action::GoToBottom => {
                    *scroll_offset = f32::MAX;
                    *current_line = *lines_count;
                }
                keybindings::Action::Quit => {
                    transition = Transition::Quit;
                }
            }
        }
    }

    let archive_name = archive
        .source_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("archive");

    // Sidebar: file tree
    egui::SidePanel::left("filetree")
        .resizable(true)
        .default_width(230.0)
        .min_width(160.0)
        .frame(
            egui::Frame::NONE
                .fill(theme::bg_sidebar())
                .inner_margin(egui::Margin::symmetric(8, 8)),
        )
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(archive_name)
                    .size(12.0)
                    .color(theme::accent())
                    .monospace(),
            );
            ui.label(
                egui::RichText::new(format!("{} text files", archive.entries.len()))
                    .size(11.0)
                    .color(theme::text_dim()),
            );
            ui.add_space(6.0);

            // Vault file management. Blocked while editing so a structural
            // change can't discard an in-progress edit.
            let can_manage = edit_buffer.is_none();
            // The folder Rename/Delete act on the highlighted folder; the
            // file ones act on the selected file. A tooltip names the
            // current target so it's never ambiguous.
            let target_folder = selected_dir.clone();
            let target_hint = match &target_folder {
                Some(dir) => format!("folder \u{201C}{dir}\u{201D}"),
                None => archive
                    .entries
                    .get(*selected)
                    .map(|e| format!("file \u{201C}{}\u{201D}", e.rel_path))
                    .unwrap_or_else(|| "nothing selected".to_string()),
            };
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        can_manage,
                        egui::Button::new(egui::RichText::new("+ File").size(11.5)),
                    )
                    .on_hover_text("New text/markdown file (paths with / create folders)")
                    .clicked()
                {
                    transition = Transition::VaultAddPrompt;
                }
                if ui
                    .add_enabled(
                        can_manage,
                        egui::Button::new(egui::RichText::new("+ Folder").size(11.5)),
                    )
                    .on_hover_text("New empty folder")
                    .clicked()
                {
                    transition = Transition::VaultAddFolderPrompt;
                }
            });
            ui.horizontal(|ui| {
                let has_target =
                    target_folder.is_some() || archive.entries.get(*selected).is_some();
                if ui
                    .add_enabled(
                        can_manage && has_target,
                        egui::Button::new(egui::RichText::new("Rename").size(11.5)),
                    )
                    .on_hover_text(format!("Rename {target_hint}"))
                    .clicked()
                {
                    transition = Transition::VaultRenamePrompt;
                }
                if ui
                    .add_enabled(
                        can_manage && has_target,
                        egui::Button::new(egui::RichText::new("Delete").size(11.5)),
                    )
                    .on_hover_text(format!("Delete {target_hint}"))
                    .clicked()
                {
                    transition = Transition::VaultDeletePrompt;
                }
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    match filetree::render(
                        ui,
                        tree,
                        &archive.entries,
                        *selected,
                        selected_dir.as_deref(),
                    ) {
                        Some(filetree::TreeClick::File(idx)) => {
                            *selected_dir = None; // picking a file clears the folder target
                            if idx != *selected && !*modified {
                                *edit_buffer = None; // leaving edit mode is safe
                                *selected = idx;
                                *scroll_offset = f32::MIN; // jump to top of new file
                                *current_line = 1;
                                *lines_count = count_lines(&archive.entries[idx].content);
                            }
                            // While *modified, don't switch files — the
                            // statusbar buttons resolve the unsaved edit.
                        }
                        Some(filetree::TreeClick::Folder(path)) => {
                            // Toggle the folder selection off if re-clicked.
                            *selected_dir = if selected_dir.as_deref() == Some(path.as_str()) {
                                None
                            } else {
                                Some(path)
                            };
                        }
                        None => {}
                    }
                    if *modified {
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(
                                "Unsaved edits — save or discard before switching files",
                            )
                            .size(10.5)
                            .color(theme::accent_yellow()),
                        );
                    }
                    // The vault may hold files this browser can't list —
                    // images, oversized entries, non-text bytes. Say so,
                    // or the vault looks emptier than it is and someone
                    // may act on that.
                    if let Some(summary) = archive.hidden.summary() {
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(summary)
                                .size(10.5)
                                .color(theme::text_dim()),
                        )
                        .on_hover_text(
                            "These files are still in the vault and are preserved \
                             exactly when you save — Schl8 just can't display \
                             them. Extract the archive with `tar` to reach them.",
                        );
                    }
                });
        });

    // A vault with no text entries is a real state — you can delete the
    // last file, or open a vault that holds only binary files — so render
    // an empty view instead of indexing into nothing. The sidebar above
    // has already drawn, so "+ File" / "+ Folder" stay available to
    // refill it.
    let Some(entry) = archive.entries.get(*selected) else {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(theme::bg_primary())
                    .inner_margin(theme::CONTENT_PADDING),
            )
            .show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(
                            "This vault has no text files.\nUse \"+ File\" to add one.",
                        )
                        .size(14.0)
                        .color(theme::text_dim()),
                    );
                });
            });
        return transition;
    };

    // Status bar (hidden in focus mode)
    if !focus_mode {
        // The bar switches to a two-row layout when narrow, so the panel
        // must size to its content rather than a fixed height — otherwise
        // the extra row is drawn below the bottom of the window.
        let compact = statusbar::is_compact(ctx.available_rect().width(), is_editing);
        let status_action = egui::TopBottomPanel::bottom("statusbar")
            .min_height(statusbar::min_bar_height(is_editing))
            .frame(
                egui::Frame::NONE
                    .fill(theme::bg_statusbar())
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show(ctx, |ui| {
                statusbar::render(
                    ui,
                    &entry.rel_path,
                    &format!("{} › {}", archive.source_path.display(), entry.rel_path),
                    *current_line,
                    *lines_count,
                    is_editing,
                    *modified,
                    Some((*selected + 1, archive.entries.len())),
                    &crate::crypto::gpg::SignatureStatus::Unsigned,
                    is_editing && *modified,
                    unsaved_unprotected,
                    stamp,
                    compact,
                )
            })
            .inner;
        if let Some(sa) = status_action {
            match sa {
                statusbar::StatusAction::DiscardEdits => {
                    transition = Transition::RequestDiscard;
                }
                statusbar::StatusAction::SaveAndExit => {
                    transition = Transition::RequestSaveAndExit;
                }
                statusbar::StatusAction::EnterEdit => {
                    transition = Transition::EnterEdit;
                }
                statusbar::StatusAction::OpenSaveOptions => {
                    transition = Transition::OpenSaveTargets;
                }
                statusbar::StatusAction::PanicLock => {
                    transition = Transition::PanicLock;
                }
            }
        }
    }

    // Main content: editor when editing, read-only viewer otherwise
    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(theme::bg_primary())
                .inner_margin(theme::CONTENT_PADDING),
        )
        .show(ctx, |ui| {
            if let Some(buf) = edit_buffer {
                let (changed, info) = viewer::render_editable(ui, buf, jump, opts);
                if changed {
                    *modified = true;
                }
                *lines_count = buf.as_str().lines().count().max(1);
                *metrics = (info.content_height, info.viewport_height, info.offset_y);
            } else {
                let content = entry
                    .content
                    .as_str()
                    .unwrap_or("[unable to decode content]");
                // Same centered reading column as the single-document
                // view, so focus and crawl look the same inside a vault.
                let mut render_entry = |ui: &mut egui::Ui| {
                    let info =
                        viewer::render(ui, content, entry.file_type, scroll_offset, jump, opts);
                    *current_line = info.current_line(*lines_count);
                    *metrics = (info.content_height, info.viewport_height, info.offset_y);
                };
                if focus_mode {
                    let avail = ui.available_width();
                    let column = column_width.min(avail);
                    let pad = ((avail - column) / 2.0).max(0.0);
                    ui.horizontal_top(|ui| {
                        ui.add_space(pad);
                        ui.vertical(|ui| {
                            ui.set_max_width(column);
                            render_entry(ui);
                        });
                    });
                } else {
                    render_entry(ui);
                }
            }
        });

    transition
}

/// The locked screen shown after an auto-lock. No plaintext is held here.
fn render_locked(
    ctx: &egui::Context,
    relock_path: Option<&std::path::Path>,
    held: Option<&crate::document::stash::StashSummary>,
    warning: Option<&str>,
) -> Transition {
    let mut transition = Transition::None;

    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(theme::bg_primary())
                .inner_margin(theme::CONTENT_PADDING),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        // Held edits push the panel taller, so the top spacing
                        // shrinks rather than pushing the buttons off a short
                        // window.
                        ui.add_space(if held.is_some() { 40.0 } else { 130.0 });

                        ui.label(
                            egui::RichText::new("\u{1F512}") // lock
                                .size(56.0)
                                .color(theme::accent()),
                        );
                        ui.add_space(16.0);
                        ui.label(
                            egui::RichText::new("Session Locked")
                                .size(24.0)
                                .color(theme::text_primary())
                                .strong(),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("Decrypted content was cleared from memory.")
                                .size(13.0)
                                .color(theme::text_dim()),
                        );

                        // Unsaved text that could not be encrypted is gone.
                        // Say it here, plainly, rather than on stderr — this
                        // screen is where the person actually is.
                        if let Some(msg) = warning {
                            ui.add_space(14.0);
                            egui::Frame::NONE
                                .fill(theme::accent_red().gamma_multiply(0.10))
                                .stroke(egui::Stroke::new(1.0, theme::accent_red()))
                                .corner_radius(theme::RADIUS)
                                .inner_margin(14.0)
                                .show(ui, |ui| {
                                    ui.set_max_width(460.0);
                                    ui.label(
                                        egui::RichText::new("Unsaved text was lost")
                                            .size(14.0)
                                            .strong()
                                            .color(theme::accent_red()),
                                    );
                                    ui.add_space(4.0);
                                    ui.label(
                                        egui::RichText::new(msg)
                                            .size(12.5)
                                            .color(theme::text_primary()),
                                    );
                                });
                        }

                        ui.add_space(20.0);

                        // ── Held (encrypted) unsaved edits ───────────────────
                        if let Some(h) = held {
                            let backend = match h.format {
                                crate::document::spool::SegmentFormat::Age => {
                                    "your AGE seed phrase"
                                }
                                crate::document::spool::SegmentFormat::Gpg => "your GPG key",
                            };
                            egui::Frame::NONE
                                .fill(theme::bg_raised().gamma_multiply(0.7))
                                .corner_radius(theme::RADIUS)
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    theme::accent_yellow().gamma_multiply(0.5),
                                ))
                                .inner_margin(egui::Margin::symmetric(16, 12))
                                .show(ui, |ui| {
                                    ui.set_max_width(520.0);
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            egui::RichText::new(
                                                "\u{26A0} Unsaved edits are being held",
                                            )
                                            .size(14.0)
                                            .color(theme::accent_yellow())
                                            .strong(),
                                        );
                                        ui.add_space(6.0);
                                        ui.label(
                                            egui::RichText::new(format!(
                                        "When the session locked at {}, your unsaved changes \
                                         were encrypted to this document's own key and written \
                                         to disk. They were never stored as plain text.",
                                        h.saved
                                    ))
                                            .size(12.0)
                                            .color(theme::text_primary()),
                                        );
                                        ui.add_space(8.0);
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "Restoring them needs {backend} \u{2014} the same \
                                         authentication as opening the document itself. \
                                         Until then they stay unreadable, including to \
                                         anyone with this computer."
                                            ))
                                            .size(11.5)
                                            .color(theme::text_dim()),
                                        );

                                        ui.add_space(12.0);
                                        let restore = egui::Button::new(
                                            egui::RichText::new("  Unlock and Restore Edits  ")
                                                .size(14.0)
                                                .color(theme::badge_text())
                                                .strong(),
                                        )
                                        .fill(theme::badge_bg())
                                        .corner_radius(6.0)
                                        .min_size(egui::vec2(260.0, 40.0));
                                        if ui
                                            .add(restore)
                                            .on_hover_text(
                                                "Authenticate, reopen the document, and put your \
                                         unsaved changes back into the editor exactly as \
                                         they were.",
                                            )
                                            .clicked()
                                        {
                                            transition = Transition::RestoreHeldEdits;
                                        }
                                        ui.add_space(6.0);
                                        let discard = egui::Button::new(
                                            egui::RichText::new("  Discard Held Edits  ")
                                                .size(12.5)
                                                .color(theme::accent_red()),
                                        )
                                        .fill(theme::bg_raised())
                                        .corner_radius(6.0)
                                        .min_size(egui::vec2(200.0, 32.0));
                                        if ui
                                            .add(discard)
                                            .on_hover_text(
                                                "Delete the held changes and start again from the \
                                         document's last saved version. This cannot be \
                                         undone \u{2014} the held copy is erased.",
                                            )
                                            .clicked()
                                        {
                                            transition = Transition::DiscardHeldEdits;
                                        }
                                        ui.add_space(4.0);
                                        ui.label(
                                            egui::RichText::new(
                                                "Discarding is permanent. Opening the file below \
                                         without restoring leaves the held copy in place, \
                                         so you can decide later.",
                                            )
                                            .size(10.5)
                                            .color(theme::text_dim()),
                                        );
                                    });
                                });
                            ui.add_space(18.0);
                        }

                        if let Some(path) = relock_path {
                            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                            let label = if held.is_some() {
                                format!("  Open {name} without the edits  ")
                            } else {
                                format!("  Unlock {name}  ")
                            };
                            let unlock =
                                egui::Button::new(egui::RichText::new(label).size(15.0).color(
                                    if held.is_some() {
                                        theme::text_primary()
                                    } else {
                                        theme::badge_text()
                                    },
                                ))
                                .fill(if held.is_some() {
                                    theme::bg_raised()
                                } else {
                                    theme::badge_bg()
                                })
                                .corner_radius(6.0)
                                .min_size(egui::vec2(240.0, 42.0));
                            let resp = ui.add(unlock);
                            let resp = if held.is_some() {
                                resp.on_hover_text(
                                    "Reopens the last saved version. The held edits are kept, \
                             not discarded — you can restore them later.",
                                )
                            } else {
                                resp
                            };
                            if resp.clicked() {
                                transition = Transition::StartDecrypt(path.to_path_buf());
                            }
                            ui.add_space(10.0);
                        }

                        let open_other = egui::Button::new(
                            egui::RichText::new("  Open Another File  ")
                                .size(13.0)
                                .color(theme::text_primary()),
                        )
                        .fill(theme::bg_raised())
                        .corner_radius(6.0)
                        .min_size(egui::vec2(180.0, 36.0));
                        if ui.add(open_other).clicked() {
                            if let Some(path) = pick_open_file() {
                                transition = Transition::StartDecrypt(path);
                            }
                        }
                    });
                });
        });

    transition
}

fn render_error(ctx: &egui::Context, message: &str, failed_path: &std::path::Path) -> Transition {
    let mut transition = Transition::None;

    let filename = failed_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(theme::bg_primary())
                .inner_margin(theme::CONTENT_PADDING),
        )
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(120.0);

                ui.label(
                    egui::RichText::new("\u{26A0}")
                        .size(48.0)
                        .color(theme::accent_yellow()),
                );

                ui.add_space(16.0);

                ui.label(
                    egui::RichText::new("Decryption Failed")
                        .size(24.0)
                        .color(theme::text_primary())
                        .strong(),
                );

                ui.add_space(8.0);

                ui.label(
                    egui::RichText::new(filename)
                        .size(14.0)
                        .color(theme::accent())
                        .monospace(),
                );

                ui.add_space(16.0);

                egui::Frame::NONE
                    .fill(theme::bg_raised())
                    .corner_radius(4.0)
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.set_max_width(500.0);
                        ui.label(
                            egui::RichText::new(message)
                                .size(13.0)
                                .color(theme::accent_red())
                                .monospace(),
                        );
                    });

                ui.add_space(24.0);

                ui.horizontal(|ui| {
                    let total_width = 340.0;
                    let available = ui.available_width();
                    if available > total_width {
                        ui.add_space((available - total_width) / 2.0);
                    }

                    let retry_btn = egui::Button::new(
                        egui::RichText::new("  Retry  ")
                            .size(14.0)
                            .color(theme::badge_text()),
                    )
                    .fill(theme::badge_bg())
                    .corner_radius(6.0)
                    .min_size(egui::vec2(140.0, 36.0));

                    if ui.add(retry_btn).clicked() {
                        transition = Transition::StartDecrypt(failed_path.to_path_buf());
                    }

                    ui.add_space(16.0);

                    let open_btn = egui::Button::new(
                        egui::RichText::new("  Open Another File  ")
                            .size(14.0)
                            .color(theme::text_primary()),
                    )
                    .fill(theme::bg_raised())
                    .corner_radius(6.0)
                    .min_size(egui::vec2(160.0, 36.0));

                    if ui.add(open_btn).clicked() {
                        if let Some(path) = pick_open_file() {
                            transition = Transition::StartDecrypt(path);
                        }
                    }
                });
            });
        });

    transition
}

/// Decode the embedded app icon PNG into `egui::IconData` for the window taskbar/dock icon.
fn load_icon_data() -> Option<egui::IconData> {
    let png_bytes = include_bytes!("../assets/schl8.iconset/icon_256x256.png");
    let img = image::load_from_memory(png_bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(egui::IconData {
        rgba: rgba.into_raw(),
        width: w,
        height: h,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The primitive behind the overwrite guard: a file that another
    /// process rewrites must not look identical to the one we loaded.
    #[test]
    fn file_identity_changes_when_the_file_does() {
        let dir = std::env::temp_dir().join(format!("schl8-ident-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("note.md.age");

        std::fs::write(&f, b"first").unwrap();
        let before = file_identity(&f).expect("identity for an existing file");

        // Same length, different content, written later: mtime moves.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&f, b"secnd").unwrap();
        let after = file_identity(&f).expect("identity after rewrite");
        assert_ne!(before, after, "a rewrite must be detectable");

        // Different length is likewise detectable.
        std::fs::write(&f, b"a much longer body than before").unwrap();
        assert_ne!(after, file_identity(&f).unwrap());

        // A missing file has no identity (callers treat that as "nothing
        // to clobber" — the save simply recreates it).
        std::fs::remove_file(&f).unwrap();
        assert!(file_identity(&f).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod relock_tests {
    use super::*;

    /// The reported bug: create a new file, lock, unlock — and the app
    /// tried to decrypt `untitled.md.gpg`, a name that has never been on
    /// disk. gpg answered "No such file or directory", which read as the
    /// app losing the document.
    #[test]
    fn a_never_saved_document_has_nothing_to_reopen() {
        let placeholder = new_empty_document(FileType::Markdown);
        let State::Viewing { doc, .. } = &placeholder else {
            panic!("new documents open in the viewer");
        };
        assert_eq!(doc.source_path, PathBuf::from("untitled.md.gpg"));
        assert!(
            relock_target(&doc.source_path).is_none(),
            "a placeholder path must never become the file to reopen"
        );
        // The plain-text variant invents its own placeholder.
        let State::Viewing { doc, .. } = &new_empty_document(FileType::PlainText) else {
            panic!("new documents open in the viewer");
        };
        assert!(relock_target(&doc.source_path).is_none());
    }

    /// A real file is still remembered — the fix must not cost the
    /// ordinary case, where unlocking reopens what you were reading.
    #[test]
    fn a_saved_document_is_remembered() {
        let path = std::env::temp_dir().join(format!("schl8-relock-{}.md.gpg", std::process::id()));
        std::fs::write(&path, b"ciphertext").unwrap();
        assert_eq!(relock_target(&path), Some(path.clone()));

        // And once it is gone — deleted or on an unmounted volume — the
        // app forgets it rather than failing to open it on unlock.
        std::fs::remove_file(&path).unwrap();
        assert!(relock_target(&path).is_none());
    }
}
#[cfg(test)]
mod new_file_warning {
    use crate::config::StashKey;

    /// A new file's unsaved text can only be protected across a lock if
    /// a fixed stash key is configured — the file itself has no key
    /// until it is first saved. That is exactly the condition the
    /// warning fires on, so it is worth pinning: getting it backwards
    /// would either nag people who are safe or stay silent for the ones
    /// who are not.
    #[test]
    fn a_fixed_key_is_what_makes_a_new_file_safe() {
        // Nothing configured: no key, so no protection.
        let off = StashKey::default();
        assert!(
            off.fixed_recipient().is_none(),
            "an unconfigured stash key protects nothing"
        );

        // A key filled in but the toggle left off is still off — the
        // checkbox is the switch, not the presence of text.
        let filled_but_disabled = StashKey {
            use_fixed: false,
            age_recipient: "age1abc".into(),
            ..Default::default()
        };
        assert!(filled_but_disabled.fixed_recipient().is_none());

        // Enabled with an age recipient: protected.
        let age = StashKey {
            use_fixed: true,
            age_recipient: "age1abc".into(),
            ..Default::default()
        };
        assert!(age.fixed_recipient().is_some());

        // Enabled with a GPG fingerprint: also protected.
        let gpg = StashKey {
            use_fixed: true,
            key_fingerprint: "DEADBEEF".into(),
            ..Default::default()
        };
        assert!(gpg.fixed_recipient().is_some());

        // Enabled but blank protects nothing, and must not be mistaken
        // for protection just because the toggle is on.
        let blank = StashKey {
            use_fixed: true,
            ..Default::default()
        };
        assert!(
            blank.fixed_recipient().is_none(),
            "an enabled but empty stash key is still no key"
        );
    }
}
