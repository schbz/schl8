//! Persistent app configuration at `~/.config/schl8/config.toml`
//! (respects `$XDG_CONFIG_HOME`).
//!
//! Only non-secret data lives here: file *paths*, key combos, and
//! formatting templates. Never store document content or key material.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Most quicknote files the registry will hold.
pub const MAX_QUICKNOTES: usize = 25;
/// Most encryption keys (rules) a single quicknote file may have.
pub const MAX_QUICKNOTE_KEYS: usize = 5;

/// Most favorites the menu-bar submenu will hold.
pub const MAX_FAVORITES: usize = 20;

/// Default cap on pending spool segments per note.
///
/// Chosen to be far past ordinary use — 500 jots without ever unlocking —
/// while still bounding a runaway agent loop. At roughly 200 bytes of
/// crypto overhead per segment the cap is about 100 KB of overhead, so
/// it's a guard against pathology, not a storage budget.
pub const DEFAULT_MAX_PENDING: usize = 500;

/// A registered quicknote file: a display name, the encrypted file that
/// quick-note appends read and extend, and (optionally) explicit
/// encryption rules — up to [`MAX_QUICKNOTE_KEYS`] keys, each with its
/// own destination path(s). With no rules, appends re-encrypt to the
/// file's own recipients in place (the classic behavior).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QuickNoteFile {
    pub name: String,
    pub source: PathBuf,
    pub rules: Vec<SaveRule>,
    /// Optional dedicated system-wide hotkey (e.g. "ctrl+cmd+1") that
    /// opens the jot window preselected on this file — handy for keypad
    /// macros that jot into a specific note. Empty = none.
    pub hotkey: String,
}

impl QuickNoteFile {
    /// A rule-less entry for an existing encrypted file.
    pub fn for_existing(source: PathBuf) -> Self {
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("note")
            .to_string();
        Self {
            name,
            source,
            rules: Vec::new(),
            hotkey: String::new(),
        }
    }

    /// Clamp to the key limit and make sure the source file itself is
    /// among the destinations (otherwise appends would read an
    /// ever-staler source copy).
    pub fn normalize(&mut self) {
        self.rules
            .retain(|r| r.has_key() && !r.destinations.is_empty());
        self.rules.truncate(MAX_QUICKNOTE_KEYS);
        if !self.rules.is_empty() {
            let covered = self
                .rules
                .iter()
                .any(|r| r.destinations.contains(&self.source));
            if !covered {
                self.rules[0].destinations.insert(0, self.source.clone());
            }
        }
    }
}

/// Quick-note ("jot") settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QuickNote {
    /// System-wide hotkey that summons the jot window (parsed by
    /// `hotkey::parse`), e.g. "ctrl+cmd+j".
    pub hotkey: String,
    /// Legacy flat target list (pre-registry configs). Migrated into
    /// `notes` on load; kept only so old configs still parse.
    pub targets: Vec<PathBuf>,
    /// The quicknote registry (append targets with optional per-key
    /// destination rules). At most [`MAX_QUICKNOTES`] entries.
    pub notes: Vec<QuickNoteFile>,
    /// The most recently used target (preselected next time).
    pub last_target: Option<PathBuf>,
    /// When the AGE identity is locked, write the entry to the note's
    /// spool instead of prompting for the seed phrase. Encrypting needs
    /// no private key, so jotting stays instant; the entries merge into
    /// the note on the next unlocked session. Set false to be prompted to
    /// unlock instead. See `docs/SPOOL-DESIGN.md`.
    pub spool_when_locked: bool,
    /// Most pending segments one note may accumulate before Schl8
    /// refuses to spool another. A note that is never unlocked would
    /// otherwise grow without bound — and a looping agent could fill the
    /// disk through the write-only CLI. Refusing is the safe end of that
    /// trade: in the app a refused spool falls back to the seed-phrase
    /// prompt, so the entry is still saved. 0 disables the cap.
    pub max_pending: usize,
    /// Persisted jot window geometry (logical points): [w, h] / [x, y].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_size: Option<[f32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_pos: Option<[f32; 2]>,
    /// Whether the timestamp template is applied (the jot window's
    /// checkbox remembers its state here).
    pub include_timestamp: bool,
    /// Blurb template for markdown targets. Placeholders: {date},
    /// {time}, {text}.
    pub template_markdown: String,
    /// Blurb template for plain-text targets.
    pub template_text: String,
    /// chrono format strings for {date} and {time}.
    pub date_format: String,
    pub time_format: String,
}

impl Default for QuickNote {
    fn default() -> Self {
        Self {
            hotkey: "ctrl+cmd+j".to_string(),
            targets: Vec::new(),
            notes: Vec::new(),
            last_target: None,
            spool_when_locked: true,
            max_pending: DEFAULT_MAX_PENDING,
            window_size: None,
            window_pos: None,
            include_timestamp: true,
            template_markdown: "\n## {date} {time}\n\n{text}\n".to_string(),
            template_text: "\n[{date} {time}]\n{text}\n".to_string(),
            date_format: "%Y-%m-%d".to_string(),
            time_format: "%H:%M".to_string(),
        }
    }
}

/// When the unlocked AGE identity is wiped from memory.
///
/// SECURITY: the seed phrase and the private key derived from it are
/// NEVER written to disk — not to the config, not to any cache. They live
/// only in an mlock'd buffer that zeroizes on drop, so quitting Schl8
/// always wipes them. These settings only bound how long the derived key
/// stays resident *while the app is running* — which matters because
/// closing the window with menu-bar residency on does not quit the app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgeLockSection {
    /// Wipe after this many minutes with no keyboard/mouse input.
    /// 0 disables the idle timeout.
    pub forget_idle_minutes: u32,
    /// Wipe this many minutes after unlocking regardless of activity — a
    /// hard ceiling on how long a key can stay resident. 0 disables.
    pub forget_after_minutes: u32,
    /// Wipe when the main window is closed to the menu bar. Hiding the
    /// window to show the quick-note jot does NOT count, or every note
    /// would re-prompt.
    pub forget_on_window_close: bool,
    /// Wipe when the display sleeps or the screen locks.
    pub forget_on_sleep: bool,
}

impl Default for AgeLockSection {
    fn default() -> Self {
        Self {
            forget_idle_minutes: 15,
            forget_after_minutes: 0,
            forget_on_window_close: false,
            forget_on_sleep: true,
        }
    }
}

/// App-level behavior settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSection {
    /// Keep a menu-bar status item and global hotkey; closing the window
    /// hides Schl8 instead of quitting.
    pub menu_bar_resident: bool,
    /// Auto-lock (close the document + zeroize buffers) after this many
    /// minutes of no keyboard/mouse input. 0 disables the idle timeout.
    pub auto_lock_minutes: u32,
    /// Also lock immediately when the display sleeps / screen saver starts.
    pub lock_on_sleep: bool,
    /// Show the floating statistics card in the viewer (View → Statistics).
    pub show_stats: bool,
    /// Keyboard layout for position-based navigation keys:
    /// "qwerty" (default), "dvorak", "colemak", "workman".
    pub keyboard_layout: String,
    /// Shell command run (via `/bin/sh -c`, in the background) after
    /// every successful save or quick-note append — e.g. trigger a
    /// backup or sync agent. `$SCHL8_SOURCE` and `$SCHL8_DESTINATIONS`
    /// are set; document content is never passed. Empty = disabled.
    pub post_save_command: String,
    /// Where new encrypted files go by default, and the one location an
    /// agent may write to without asking. Empty means
    /// [`default_notes_dir`]. Leading `~` is expanded.
    ///
    /// Only ever holds ciphertext — it is an ordinary directory, not a
    /// secure one, and nothing about it relaxes the plaintext rules.
    pub notes_dir: PathBuf,
}

impl Default for AppSection {
    fn default() -> Self {
        Self {
            menu_bar_resident: true,
            auto_lock_minutes: 5,
            lock_on_sleep: true,
            show_stats: false,
            keyboard_layout: "qwerty".to_string(),
            post_save_command: String::new(),
            notes_dir: PathBuf::new(),
        }
    }
}

/// `~/Documents/Schl8` — the fallback when nothing is configured.
///
/// Documents rather than the config directory: these are the user's own
/// files, they show up in Finder and in backups, and putting them under
/// a dotfile directory hides them from the person who owns them.
pub fn default_notes_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Documents").join("Schl8"))
}

/// Expand a leading `~` against `$HOME`. Anything else passes through.
fn expand_tilde(p: &Path) -> PathBuf {
    let Ok(rest) = p.strip_prefix("~") else {
        return p.to_path_buf();
    };
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(rest),
        None => p.to_path_buf(),
    }
}

/// A favorite: an encrypted file you open often, listed in the menu-bar
/// Favorites submenu and openable by an optional global hotkey.
///
/// Distinct from a quicknote: a quicknote is an *append* target (the jot
/// window adds an entry to it), while a favorite simply opens the file
/// for reading and editing. The same file may legitimately be both.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Favorite {
    /// Display name in the submenu. Blank falls back to the file name.
    pub name: String,
    /// The encrypted file this favorite opens.
    pub path: PathBuf,
    /// Optional system-wide hotkey (e.g. "ctrl+cmd+2") that opens this
    /// file from anywhere. Empty = none.
    pub hotkey: String,
}

impl Favorite {
    /// A favorite for `path`, named after the file.
    pub fn for_path(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        Self {
            name,
            path,
            hotkey: String::new(),
        }
    }

    /// The label to show in menus: the name, or the file name if blank.
    pub fn label(&self) -> String {
        if self.name.trim().is_empty() {
            self.path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string()
        } else {
            self.name.trim().to_string()
        }
    }
}

/// Bounds for crawl mode's live controls. Speed is in points per
/// second: 8 is a slow drift, 400 outruns comfortable reading but is
/// useful for skipping ahead.
pub const MIN_CRAWL_SPEED: f32 = 8.0;
pub const MAX_CRAWL_SPEED: f32 = 400.0;
pub const MIN_CRAWL_SCALE: f32 = 0.8;
pub const MAX_CRAWL_SCALE: f32 = 3.0;

/// Crawl mode: the document scrolls by itself so it can be read without
/// touching anything — the opening-titles effect, applied to notes.
///
/// Every field here is a *starting* value. The live controls in crawl
/// mode change the session's speed and text size without writing to the
/// config, so experimenting never disturbs the saved defaults; Settings
/// is where the defaults themselves are changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CrawlSection {
    /// Scroll rate in points per second.
    pub speed: f32,
    /// True: the text rises up the screen (reading forward). False: it
    /// descends, walking back toward the start.
    pub direction_up: bool,
    /// Text size multiplier while crawling — bigger by default, since
    /// the point is reading from a distance without leaning in.
    pub text_scale: f32,
    /// Width of the reading column in points. 0 uses the full width.
    pub column_width: f32,
    /// Scrolling by hand pauses the animation.
    pub pause_on_scroll: bool,
    /// Seconds of stillness after a manual scroll before the crawl picks
    /// itself back up. 0 keeps it paused until the reader says so.
    pub resume_after_seconds: f32,
    /// What happens at either end: "stop", "reverse" (turn around and
    /// keep going) or "loop" (jump back to the other end).
    pub end_action: String,
    /// Fade the top and bottom edges so lines enter and leave softly.
    pub fade_edges: bool,
    /// Go fullscreen and hide the chrome, like focus mode.
    pub fullscreen: bool,
    /// Show the transient control hints when a setting changes.
    pub show_hud: bool,
}

impl Default for CrawlSection {
    fn default() -> Self {
        Self {
            // ~40 pt/s is roughly a comfortable line every two seconds
            // at the default text size.
            speed: 40.0,
            direction_up: true,
            text_scale: 1.3,
            column_width: 720.0,
            pause_on_scroll: true,
            // Scrolling to check something should not end the crawl —
            // it gets out of the way, then carries on by itself once the
            // reader stops moving.
            resume_after_seconds: 2.0,
            // Reaching an end should not be a dead stop you have to
            // rescue the crawl from; turning around keeps it readable.
            end_action: "reverse".to_string(),
            fade_edges: true,
            fullscreen: true,
            show_hud: true,
        }
    }
}

impl CrawlSection {
    /// Bring hand-edited values into range. Speed and scale are clamped
    /// for the same reason the interface scale is: crawl mode hides the
    /// chrome, so an unusable value would be hard to escape from.
    pub fn clamped(&self) -> Self {
        let mut c = self.clone();
        c.speed = if c.speed.is_finite() {
            c.speed.clamp(MIN_CRAWL_SPEED, MAX_CRAWL_SPEED)
        } else {
            40.0
        };
        c.text_scale = if c.text_scale.is_finite() {
            c.text_scale.clamp(MIN_CRAWL_SCALE, MAX_CRAWL_SCALE)
        } else {
            1.3
        };
        c.column_width = if c.column_width.is_finite() {
            c.column_width.clamp(0.0, 4000.0)
        } else {
            720.0
        };
        c.resume_after_seconds = if c.resume_after_seconds.is_finite() {
            c.resume_after_seconds.clamp(0.0, 600.0)
        } else {
            0.0
        };
        if !matches!(c.end_action.as_str(), "stop" | "reverse" | "loop") {
            c.end_action = "reverse".to_string();
        }
        c
    }

    /// Whether the crawl turns around at an end.
    pub fn reverses_at_end(&self) -> bool {
        self.end_action == "reverse"
    }

    /// Whether the crawl jumps to the other end and continues.
    pub fn loops_at_end(&self) -> bool {
        self.end_action == "loop"
    }
}

/// Smallest and largest interface scale the settings offer. Below ~0.7
/// the status bar stops being readable; above ~2.0 dialogs no longer fit
/// on a laptop display, and both extremes are hard to escape from
/// because the settings window itself scales with everything else.
pub const MIN_FONT_SCALE: f32 = 0.7;
pub const MAX_FONT_SCALE: f32 = 2.0;

/// Visual appearance settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// Palette preset name; see `ui::theme::PRESETS` for the full list.
    /// Unknown values fall back to "slate".
    pub theme: String,
    /// Optional accent override as "#RRGGBB" (empty = preset accent).
    pub accent: String,
    /// Legacy setting, no longer applied: the window is always fully
    /// opaque (translucency would let other windows shine through near
    /// decrypted text). Kept so old configs still parse.
    pub opacity: f32,
    /// Font family: "" (built-in Hack) or one of the system fonts listed
    /// in `theme::FONT_CHOICES` ("monaco", "courier", "arial", "georgia",
    /// "verdana", "times").
    pub font: String,
    /// Interface scale, applied as egui's zoom factor so every part of
    /// the app grows together — menus, lists, dialogs and document text.
    /// Scaling text alone would do almost nothing here, since nearly all
    /// of this UI sets an explicit point size per label.
    /// Clamped to [`MIN_FONT_SCALE`]..=[`MAX_FONT_SCALE`] on load.
    pub font_scale: f32,
    /// Wrap long lines in the viewer/editor (off = horizontal scrolling).
    pub word_wrap: bool,
    /// Show line numbers in the left gutter (plaintext view + editor).
    pub line_numbers: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "slate".to_string(),
            accent: String::new(),
            opacity: 1.0,
            font: String::new(),
            font_scale: 1.0,
            word_wrap: true,
            line_numbers: false,
        }
    }
}

/// In-app keyboard shortcuts (each a combo like "cmd+s"). The system-wide
/// quick-note hotkey lives in `[quick_note].hotkey`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Keybindings {
    pub open_file: String,
    pub new_markdown: String,
    pub new_text: String,
    pub quick_note: String,
    pub save: String,
    pub save_as: String,
    pub toggle_edit: String,
    pub close_document: String,
    pub settings: String,
    pub find: String,
    /// Start/stop crawl mode (the animated reading view).
    pub crawl: String,
    /// Lock the session immediately (the panic button's shortcut).
    /// Unsaved work is encrypted to the document's own key first.
    pub panic_lock: String,
}

impl Default for Keybindings {
    fn default() -> Self {
        Self {
            open_file: "cmd+o".to_string(),
            new_markdown: "cmd+n".to_string(),
            new_text: "cmd+shift+n".to_string(),
            quick_note: "cmd+j".to_string(),
            save: "cmd+s".to_string(),
            save_as: "cmd+shift+s".to_string(),
            toggle_edit: "cmd+e".to_string(),
            close_document: "cmd+w".to_string(),
            settings: "cmd+comma".to_string(),
            find: "cmd+f".to_string(),
            crawl: "cmd+shift+r".to_string(),
            // Deliberately not a bare or single-modifier combo: this
            // wipes the screen, so it must not be reachable by a slip.
            panic_lock: "ctrl+cmd+l".to_string(),
        }
    }
}

/// Security-related preferences. Both default to false — Schl8's
/// no-clipboard stance and always-warn behavior.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Security {
    /// Persisted default for whether copying to the clipboard is allowed
    /// at startup.
    pub allow_copy_default: bool,
    /// Skip the security warning shown when enabling copying.
    pub suppress_copy_warning: bool,
    /// Which key secures unsaved edits when the session locks.
    pub stash_key: StashKey,
}

/// Which public key unsaved edits are encrypted to when the session
/// locks (see `document/stash.rs`).
///
/// The default follows each document's own key, so recovering an edit
/// needs exactly the credential that opens the document itself. A fixed
/// key is offered for people who would rather unlock in-progress work
/// with one credential regardless of which file it came from — typically
/// an AGE seed phrase, so a hardware key isn't needed just to get a draft
/// back.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StashKey {
    /// True to always use the key below instead of the document's own.
    pub use_fixed: bool,
    /// age recipient (`age1…`) when the fixed key is an age key.
    pub age_recipient: String,
    /// GPG fingerprint when the fixed key is a GPG key.
    pub key_fingerprint: String,
    /// Cached display label for the settings UI only.
    pub key_label: String,
}

impl StashKey {
    /// The configured fixed recipient, if the option is on and a key was
    /// actually chosen. Returns the recipient list and its backend.
    ///
    /// Returns None when `use_fixed` is set but no key was picked — a
    /// half-configured override must fall back to the document's own key
    /// rather than silently failing to protect anything.
    pub fn fixed_recipient(&self) -> Option<(Vec<String>, crate::document::spool::SegmentFormat)> {
        if !self.use_fixed {
            return None;
        }
        if !self.age_recipient.trim().is_empty() {
            return Some((
                vec![self.age_recipient.trim().to_string()],
                crate::document::spool::SegmentFormat::Age,
            ));
        }
        if !self.key_fingerprint.trim().is_empty() {
            return Some((
                vec![self.key_fingerprint.trim().to_string()],
                crate::document::spool::SegmentFormat::Gpg,
            ));
        }
        None
    }

    /// True when the option is on but unusable, so the UI can say so.
    pub fn is_incomplete(&self) -> bool {
        self.use_fixed && self.fixed_recipient().is_none()
    }
}

/// One encryption target within a file's save plan: a key plus the
/// destinations that copies encrypted to that key are written to.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveRule {
    /// Recipient key fingerprint (40 hex chars). Empty for an age rule.
    pub key_fingerprint: String,
    /// Cached display label for the key (UID text) — for the GUI only;
    /// encryption always uses the fingerprint (GPG) or recipient (age).
    pub key_label: String,
    /// age recipient (`age1…`). When non-empty this rule encrypts with age
    /// instead of GPG, and `key_fingerprint` is empty. The two are mutually
    /// exclusive.
    pub age_recipient: String,
    /// Encrypted output paths. Armor is chosen per destination by its
    /// extension (.asc → armored); age output is always binary. Existing
    /// files are overwritten.
    pub destinations: Vec<PathBuf>,
}

impl SaveRule {
    /// True when this rule encrypts with age (a recipient is set).
    pub fn is_age(&self) -> bool {
        !self.age_recipient.is_empty()
    }

    /// True when the rule has an encryption key of either kind selected.
    pub fn has_key(&self) -> bool {
        !self.key_fingerprint.is_empty() || self.is_age()
    }
}

/// A per-file save plan: on every Save, the document is encrypted to each
/// rule's key and written to all of that rule's destinations.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavePlan {
    /// The document this plan belongs to (its open path).
    pub source: PathBuf,
    pub rules: Vec<SaveRule>,
    /// Shell command run (via `/bin/sh -c`, in the background) after every
    /// successful save of this plan — e.g. an rsync backup or server
    /// upload. `$SCHL8_SOURCE` and `$SCHL8_DESTINATIONS` (one path per
    /// line) are set; document content is never passed. Empty = disabled.
    pub post_save_command: String,
}

/// The first destination path that appears under more than one key rule,
/// if any. Different keys writing to the same file would silently
/// overwrite each other — the last rule would win and the file would be
/// readable only by that key — so plan editors reject this.
pub fn duplicate_destination(rules: &[SaveRule]) -> Option<PathBuf> {
    let mut seen: Vec<&PathBuf> = Vec::new();
    for rule in rules {
        for dest in &rule.destinations {
            if seen.contains(&dest) {
                return Some(dest.clone());
            }
        }
        // Duplicates *within* one rule are harmless (same ciphertext);
        // only flag collisions across different rules.
        for dest in &rule.destinations {
            seen.push(dest);
        }
    }
    None
}

/// Most entries kept in the recently-opened list.
pub const MAX_RECENTS: usize = 10;

/// One recently opened file. Only the path and open time are stored —
/// hash and last-saved time are computed live from the encrypted file on
/// disk, so they can never go stale (and nothing sensitive is persisted).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecentFile {
    pub path: PathBuf,
    /// RFC3339 timestamp of when the file was last opened in Schl8.
    pub last_opened: String,
}

/// Files whose on-disk fingerprint is remembered between sessions.
///
/// Deliberately larger than [`MAX_RECENTS`]: the point is to notice a
/// change in a file you come back to every few weeks, and a ten-entry
/// window would have forgotten it long before you returned.
pub const MAX_REMEMBERED: usize = 200;

/// What a file's ciphertext hashed to the last time it was opened.
///
/// This is what lets Schl8 say "this changed since you last looked"
/// rather than leaving you to notice that a small picture is different.
///
/// Only a path and a hash of the *ciphertext* — the same public
/// information already shown in the status bar. It does add to the map
/// of which files you use, which is the thing `config_backup` offers to
/// encrypt; nothing here is derived from decrypted content.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeenFile {
    pub path: PathBuf,
    /// Full SHA-256 of the encrypted file, lowercase hex.
    pub digest: String,
    /// RFC3339 timestamp of when that digest was recorded.
    pub seen: String,
}

/// A stored age recipient public key (`age1…`) — the age equivalent of a
/// GPG public key, usable as an encryption identity. Public, non-secret;
/// persisted so it survives restarts, like the GPG keyring.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgeRecipient {
    /// User-given name.
    pub label: String,
    /// The `age1…` recipient string.
    pub recipient: String,
    /// RFC3339 date the key was added.
    pub added: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub app: AppSection,
    /// When the in-memory AGE identity is wiped (never persisted itself).
    pub age_lock: AgeLockSection,
    pub appearance: Appearance,
    pub keybindings: Keybindings,
    pub security: Security,
    pub quick_note: QuickNote,
    /// Per-file multi-key / multi-destination save plans.
    pub save_plans: Vec<SavePlan>,
    /// Recently opened files, most recent first.
    pub recent_files: Vec<RecentFile>,
    /// Imported age recipient public keys (shown alongside GPG keys).
    pub age_recipients: Vec<AgeRecipient>,
    /// Files pinned to the menu-bar Favorites submenu, in display order.
    /// At most [`MAX_FAVORITES`].
    pub favorites: Vec<Favorite>,
    /// Animated reading mode.
    pub crawl: CrawlSection,
    /// Remembered on-disk fingerprints, most recently seen first.
    pub seen_files: Vec<SeenFile>,
}

/// Where the config file lives: `$XDG_CONFIG_HOME/schl8/config.toml`
/// or `~/.config/schl8/config.toml`.
pub fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("schl8").join("config.toml"))
}

impl Config {
    /// Load the config, falling back to defaults if the file is missing
    /// or unreadable (a broken config never blocks startup).
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Config::default();
        };
        let mut cfg: Config = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                eprintln!("warning: could not parse {}: {e}", path.display());
                Config::default()
            }),
            Err(_) => Config::default(),
        };
        cfg.migrate_legacy_targets();
        cfg.clamp_limits();
        cfg
    }

    /// Where new encrypted files go: the configured directory, or
    /// [`default_notes_dir`] when unset. Does not create anything.
    pub fn notes_dir(&self) -> Option<PathBuf> {
        let configured = &self.app.notes_dir;
        if configured.as_os_str().is_empty() {
            default_notes_dir()
        } else {
            Some(expand_tilde(configured))
        }
    }

    /// Same, but create the directory if it does not exist.
    ///
    /// 0700: the contents are ciphertext, so the mode is not what keeps
    /// them secret — but the *names* of a person's notes are worth as
    /// much as some of the contents, and there is no reason to publish
    /// them to every process on the machine.
    pub fn ensure_notes_dir(&self) -> Result<PathBuf> {
        let dir = self
            .notes_dir()
            .ok_or_else(|| anyhow::anyhow!("no home directory — cannot locate a notes folder"))?;
        if !dir.exists() {
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("could not create {}", dir.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
            }
        }
        Ok(dir)
    }

    /// Bring hand-edited or corrupt values back into range.
    ///
    /// font_scale matters most: the settings window scales with everything
    /// else, so a config holding 0.01 or 50.0 would leave the user unable
    /// to reach the control that fixes it. Clamping on load makes that
    /// unrecoverable state impossible. NaN would defeat a plain clamp, so
    /// it falls back to 1.0 explicitly.
    fn clamp_limits(&mut self) {
        let scale = self.appearance.font_scale;
        self.appearance.font_scale = if scale.is_finite() {
            scale.clamp(MIN_FONT_SCALE, MAX_FONT_SCALE)
        } else {
            1.0
        };
        self.favorites.truncate(MAX_FAVORITES);
        self.crawl = self.crawl.clamped();
    }

    /// Fold the pre-registry flat `targets` list into `notes` as
    /// rule-less entries, preserving order. Idempotent.
    fn migrate_legacy_targets(&mut self) {
        let legacy = std::mem::take(&mut self.quick_note.targets);
        for path in legacy {
            if self.quick_note.notes.len() >= MAX_QUICKNOTES {
                break;
            }
            if !self.quick_note.notes.iter().any(|n| n.source == path) {
                self.quick_note
                    .notes
                    .push(QuickNoteFile::for_existing(path));
            }
        }
    }

    /// Persist the config, creating the directory if needed.
    pub fn save(&self) -> Result<()> {
        let path = config_path().context("could not determine config directory")?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }
        let text = toml::to_string_pretty(self).context("failed to serialize config")?;
        std::fs::write(&path, text).with_context(|| format!("failed to write {}", path.display()))
    }

    /// Register an existing encrypted file as a rule-less quicknote (if
    /// there's room) and select it. Fails (returns false) only when the
    /// registry is full and the file isn't already in it.
    pub fn add_target(&mut self, path: PathBuf) -> bool {
        let known = self.quick_note.notes.iter().any(|n| n.source == path);
        if !known {
            if self.quick_note.notes.len() >= MAX_QUICKNOTES {
                return false;
            }
            self.quick_note
                .notes
                .push(QuickNoteFile::for_existing(path.clone()));
        }
        self.quick_note.last_target = Some(path);
        true
    }

    /// The registry entry whose source is `path`, if any.
    pub fn quicknote_for(&self, path: &Path) -> Option<&QuickNoteFile> {
        self.quick_note.notes.iter().find(|n| n.source == path)
    }

    /// Replace the whole registry (normalized, limits enforced).
    pub fn set_quicknotes(&mut self, mut notes: Vec<QuickNoteFile>) {
        for n in &mut notes {
            n.normalize();
            if n.name.trim().is_empty() {
                n.name = n
                    .source
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("note")
                    .to_string();
            }
        }
        notes.retain(|n| !n.source.as_os_str().is_empty());
        notes.truncate(MAX_QUICKNOTES);
        if let Some(last) = &self.quick_note.last_target {
            if !notes.iter().any(|n| &n.source == last) {
                self.quick_note.last_target = None;
            }
        }
        self.quick_note.notes = notes;
    }

    /// Record `path` as just-opened: moved to the front, deduplicated,
    /// capped at [`MAX_RECENTS`]. Relative paths (debug samples, unsaved
    /// new files) are ignored.
    pub fn add_recent(&mut self, path: &Path) {
        if !path.is_absolute() {
            return;
        }
        self.recent_files.retain(|r| r.path != path);
        self.recent_files.insert(
            0,
            RecentFile {
                path: path.to_path_buf(),
                last_opened: chrono::Local::now().to_rfc3339(),
            },
        );
        self.recent_files.truncate(MAX_RECENTS);
    }

    /// Drop `path` from the recents list (the picker's "✕" on entries
    /// whose file no longer exists).
    pub fn remove_recent(&mut self, path: &Path) {
        self.recent_files.retain(|r| r.path != path);
        // The entry is removed because the file is gone, so its
        // remembered fingerprint describes nothing and would otherwise
        // sit in the config forever.
        self.forget_digest(path);
    }

    /// What this file's ciphertext hashed to when it was last opened.
    pub fn remembered_digest(&self, path: &Path) -> Option<&str> {
        self.seen_files
            .iter()
            .find(|s| s.path == path)
            .map(|s| s.digest.as_str())
    }

    /// Record what `path` hashes to now, and report what it hashed to
    /// before **if that was different**.
    ///
    /// Recording and comparing are one call on purpose. Two calls invite
    /// the bug where a caller compares, forgets to record, and warns
    /// about the same change on every open forever.
    ///
    /// A first sighting returns `None`: a file Schl8 has never seen
    /// cannot have changed, and greeting a new file with a change
    /// warning would teach people to ignore the warning.
    pub fn remember_digest(&mut self, path: &Path, digest: &str) -> Option<String> {
        if !path.is_absolute() || digest.len() != 64 {
            return None;
        }
        let previous = self
            .seen_files
            .iter()
            .position(|s| s.path == path)
            .map(|i| self.seen_files.remove(i).digest)
            .filter(|d| d != digest);
        self.seen_files.insert(
            0,
            SeenFile {
                path: path.to_path_buf(),
                digest: digest.to_string(),
                seen: chrono::Local::now().to_rfc3339(),
            },
        );
        self.seen_files.truncate(MAX_REMEMBERED);
        previous
    }

    /// Forget a file's fingerprint (used when its entry is removed).
    pub fn forget_digest(&mut self, path: &Path) {
        self.seen_files.retain(|s| s.path != path);
    }

    /// Add an age recipient (deduplicated by recipient string). Returns
    /// false if it was already present.
    pub fn add_age_recipient(&mut self, label: &str, recipient: &str) -> bool {
        let recipient = recipient.trim().to_string();
        if self.age_recipients.iter().any(|r| r.recipient == recipient) {
            return false;
        }
        let label = if label.trim().is_empty() {
            "age key".to_string()
        } else {
            label.trim().to_string()
        };
        self.age_recipients.push(AgeRecipient {
            label,
            recipient,
            added: chrono::Local::now().to_rfc3339(),
        });
        true
    }

    /// Remove the age recipient with the given `age1…` string.
    pub fn remove_age_recipient(&mut self, recipient: &str) {
        self.age_recipients.retain(|r| r.recipient != recipient);
    }

    /// The save plan for a document, if one is configured.
    pub fn plan_for(&self, source: &Path) -> Option<&SavePlan> {
        self.save_plans.iter().find(|p| p.source == source)
    }

    /// Insert or replace the plan for `plan.source`. A plan with no usable
    /// rules (no key or no destinations) is removed instead of stored.
    pub fn set_plan(&mut self, mut plan: SavePlan) {
        plan.rules
            .retain(|r| r.has_key() && !r.destinations.is_empty());
        self.save_plans.retain(|p| p.source != plan.source);
        if !plan.rules.is_empty() {
            self.save_plans.push(plan);
        }
    }
}

/// Render a jot blurb using the template for the given target file.
/// `include_timestamp = false` uses just the text with a trailing newline.
pub fn render_blurb(cfg: &QuickNote, target: &Path, text: &str, include_timestamp: bool) -> String {
    if !include_timestamp {
        let mut s = format!("\n{text}");
        if !s.ends_with('\n') {
            s.push('\n');
        }
        return s;
    }

    let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let inner = name
        .strip_suffix(".gpg")
        .or_else(|| name.strip_suffix(".asc"))
        .unwrap_or(name);
    let template = match crate::document::detect_file_type_from_name(inner) {
        Some(crate::document::FileType::Markdown) => &cfg.template_markdown,
        _ => &cfg.template_text,
    };

    let now = chrono::Local::now();
    let date = now.format(&cfg.date_format).to_string();
    let time = now.format(&cfg.time_format).to_string();

    let mut rendered = template
        .replace("{date}", &date)
        .replace("{time}", &time)
        .replace("{text}", text);
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip_through_toml() {
        let cfg = Config::default();
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.quick_note.hotkey, "ctrl+cmd+j");
        assert!(back.quick_note.include_timestamp);
    }

    #[test]
    fn partial_config_fills_defaults() {
        let cfg: Config = toml::from_str("[quick_note]\nhotkey = \"cmd+shift+k\"\n").unwrap();
        assert_eq!(cfg.quick_note.hotkey, "cmd+shift+k");
        assert_eq!(cfg.quick_note.date_format, "%Y-%m-%d");
    }

    #[test]
    fn render_markdown_blurb_has_heading_and_text() {
        let qn = QuickNote::default();
        let out = render_blurb(&qn, Path::new("notes.md.gpg"), "hello world", true);
        assert!(out.starts_with("\n## "));
        assert!(out.contains("hello world"));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn render_text_blurb_uses_bracket_template() {
        let qn = QuickNote::default();
        let out = render_blurb(&qn, Path::new("log.txt.gpg"), "entry", true);
        assert!(out.starts_with("\n["));
        assert!(out.contains("entry"));
    }

    #[test]
    fn render_without_timestamp_is_plain() {
        let qn = QuickNote::default();
        let out = render_blurb(&qn, Path::new("notes.md.gpg"), "just text", false);
        assert_eq!(out, "\njust text\n");
    }

    #[test]
    fn remove_recent_drops_only_that_path() {
        let mut cfg = Config::default();
        cfg.add_recent(Path::new("/a/x.md.gpg"));
        cfg.add_recent(Path::new("/a/y.md.age"));
        cfg.remove_recent(Path::new("/a/x.md.gpg"));
        let left: Vec<_> = cfg.recent_files.iter().map(|r| r.path.clone()).collect();
        assert_eq!(left, vec![PathBuf::from("/a/y.md.age")]);
    }

    #[test]
    fn add_target_dedupes_and_selects() {
        let mut cfg = Config::default();
        assert!(cfg.add_target(PathBuf::from("/a.md.gpg")));
        assert!(cfg.add_target(PathBuf::from("/b.md.gpg")));
        assert!(cfg.add_target(PathBuf::from("/a.md.gpg")));
        assert_eq!(cfg.quick_note.notes.len(), 2);
        assert_eq!(cfg.quick_note.last_target, Some(PathBuf::from("/a.md.gpg")));
    }

    #[test]
    fn add_target_respects_registry_cap() {
        let mut cfg = Config::default();
        for i in 0..MAX_QUICKNOTES {
            assert!(cfg.add_target(PathBuf::from(format!("/n{i}.md.gpg"))));
        }
        assert!(!cfg.add_target(PathBuf::from("/overflow.md.gpg")));
        assert_eq!(cfg.quick_note.notes.len(), MAX_QUICKNOTES);
        // Re-selecting an existing entry still works at the cap.
        assert!(cfg.add_target(PathBuf::from("/n3.md.gpg")));
    }

    #[test]
    fn legacy_targets_migrate_to_notes() {
        let mut cfg: Config =
            toml::from_str("[quick_note]\ntargets = [\"/old1.md.gpg\", \"/old2.txt.gpg\"]\n")
                .unwrap();
        cfg.migrate_legacy_targets();
        assert!(cfg.quick_note.targets.is_empty());
        assert_eq!(cfg.quick_note.notes.len(), 2);
        assert_eq!(
            cfg.quick_note.notes[0].source,
            PathBuf::from("/old1.md.gpg")
        );
        assert_eq!(cfg.quick_note.notes[0].name, "old1.md.gpg");
        assert!(cfg.quick_note.notes[0].rules.is_empty());
        // Idempotent.
        cfg.migrate_legacy_targets();
        assert_eq!(cfg.quick_note.notes.len(), 2);
    }

    #[test]
    fn quicknote_normalize_enforces_limits_and_source_coverage() {
        let rule = |fpr: &str, dest: &str| SaveRule {
            key_fingerprint: fpr.to_string(),
            key_label: String::new(),
            age_recipient: String::new(),
            destinations: vec![PathBuf::from(dest)],
        };
        let mut note = QuickNoteFile {
            name: "n".into(),
            source: PathBuf::from("/src.md.gpg"),
            rules: (0..7)
                .map(|i| rule(&format!("F{i}"), "/other.md.gpg"))
                .collect(),
            ..Default::default()
        };
        note.normalize();
        assert_eq!(note.rules.len(), MAX_QUICKNOTE_KEYS);
        // Source was inserted as the first rule's first destination.
        assert_eq!(note.rules[0].destinations[0], PathBuf::from("/src.md.gpg"));

        // Rule-less entries stay rule-less (no phantom rule invented).
        let mut plain = QuickNoteFile::for_existing(PathBuf::from("/plain.md.gpg"));
        plain.normalize();
        assert!(plain.rules.is_empty());
    }

    #[test]
    fn recents_dedupe_cap_and_order() {
        let mut cfg = Config::default();
        for i in 0..(MAX_RECENTS + 3) {
            cfg.add_recent(Path::new(&format!("/notes/n{i}.md.gpg")));
        }
        assert_eq!(cfg.recent_files.len(), MAX_RECENTS);
        // Most recent first.
        assert_eq!(
            cfg.recent_files[0].path,
            PathBuf::from(format!("/notes/n{}.md.gpg", MAX_RECENTS + 2))
        );

        // Re-opening an existing entry moves it to the front, no dup.
        let repeat = cfg.recent_files[3].path.clone();
        cfg.add_recent(&repeat);
        assert_eq!(cfg.recent_files[0].path, repeat);
        assert_eq!(
            cfg.recent_files.iter().filter(|r| r.path == repeat).count(),
            1
        );

        // Relative paths (debug samples) are ignored.
        cfg.add_recent(Path::new("sample.md.gpg"));
        assert!(cfg.recent_files.iter().all(|r| r.path.is_absolute()));

        // Round-trips through the config file.
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.recent_files, cfg.recent_files);
    }

    /// The fixed-key override must be all-or-nothing: turning it on
    /// without picking a key has to fall back to the document's own key,
    /// not silently leave unsaved edits unprotected.
    #[test]
    fn a_half_configured_stash_key_falls_back() {
        use crate::document::spool::SegmentFormat;

        let mut sk = StashKey::default();
        assert!(!sk.use_fixed, "documents' own keys by default");
        assert!(sk.fixed_recipient().is_none());
        assert!(!sk.is_incomplete());

        // On, but nothing chosen yet.
        sk.use_fixed = true;
        assert!(sk.fixed_recipient().is_none(), "no key means no override");
        assert!(sk.is_incomplete(), "and the UI must be able to say so");

        // An age key.
        sk.age_recipient = "age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p".into();
        let (recips, fmt) = sk.fixed_recipient().expect("age override");
        assert_eq!(fmt, SegmentFormat::Age);
        assert_eq!(recips.len(), 1);
        assert!(!sk.is_incomplete());

        // A GPG key, when no age key is set.
        let sk = StashKey {
            use_fixed: true,
            age_recipient: String::new(),
            key_fingerprint: "0123456789ABCDEF0123456789ABCDEF01234567".into(),
            key_label: "Me".into(),
        };
        let (recips, fmt) = sk.fixed_recipient().expect("gpg override");
        assert_eq!(fmt, SegmentFormat::Gpg);
        assert_eq!(recips[0], "0123456789ABCDEF0123456789ABCDEF01234567");

        // Off with a key still stored: the override stays inert.
        let sk = StashKey {
            use_fixed: false,
            ..sk
        };
        assert!(sk.fixed_recipient().is_none());
    }

    /// A hand-edited or corrupt font scale must never be able to lock the
    /// user out: the settings window scales with everything else, so an
    /// out-of-range value would hide the control that fixes it.
    #[test]
    fn font_scale_is_clamped_into_a_usable_range() {
        let mut cfg = Config::default();
        assert_eq!(cfg.appearance.font_scale, 1.0, "sane default");

        for (input, want) in [
            (0.0_f32, MIN_FONT_SCALE),
            (50.0, MAX_FONT_SCALE),
            (-3.0, MIN_FONT_SCALE),
            (1.25, 1.25),
        ] {
            cfg.appearance.font_scale = input;
            cfg.clamp_limits();
            assert_eq!(cfg.appearance.font_scale, want, "input {input}");
        }

        // NaN survives `clamp` on some paths, so it is handled explicitly.
        cfg.appearance.font_scale = f32::NAN;
        cfg.clamp_limits();
        assert_eq!(cfg.appearance.font_scale, 1.0);

        // Configs written before this setting existed still get 1.0.
        let old: Config = toml::from_str("[appearance]\ntheme = \"slate\"\n").unwrap();
        assert_eq!(old.appearance.font_scale, 1.0);
    }

    #[test]
    fn favorites_are_capped_and_labelled() {
        let mut cfg = Config::default();
        assert!(cfg.favorites.is_empty(), "none by default");
        cfg.favorites = (0..MAX_FAVORITES + 7)
            .map(|i| Favorite::for_path(PathBuf::from(format!("/n/{i}.md.gpg"))))
            .collect();
        cfg.clamp_limits();
        assert_eq!(cfg.favorites.len(), MAX_FAVORITES);

        // The menu label falls back to the file name when unnamed, so a
        // favorite can never render as a blank row.
        let fav = Favorite::for_path(PathBuf::from("/notes/journal.md.gpg"));
        assert_eq!(fav.label(), "journal.md.gpg");
        let blank = Favorite {
            name: "   ".into(),
            path: PathBuf::from("/notes/plan.md.age"),
            hotkey: String::new(),
        };
        assert_eq!(blank.label(), "plan.md.age");
        let named = Favorite {
            name: "Journal".into(),
            ..fav
        };
        assert_eq!(named.label(), "Journal");
    }

    /// Every config already on disk predates `max_pending`. Container
    /// `#[serde(default)]` must fill it from QuickNote::default() — if it
    /// fell back to usize's own default the cap would be 0 (disabled) for
    /// every existing user, making the bound silently inert.
    #[test]
    fn configs_without_max_pending_still_get_the_cap() {
        let cfg: Config =
            toml::from_str("[quick_note]\nhotkey = \"ctrl+cmd+j\"\n").expect("old config parses");
        assert_eq!(cfg.quick_note.max_pending, DEFAULT_MAX_PENDING);
        assert_ne!(cfg.quick_note.max_pending, 0, "cap must not be inert");
    }

    #[test]
    fn age_recipients_add_dedupe_remove_roundtrip() {
        let mut cfg = Config::default();
        assert!(cfg.add_age_recipient("Alice", "age1alice"));
        assert!(cfg.add_age_recipient("", "age1bob")); // blank label → default
        assert!(!cfg.add_age_recipient("Alice again", "age1alice")); // dup
        assert_eq!(cfg.age_recipients.len(), 2);
        assert_eq!(cfg.age_recipients[1].label, "age key");
        assert!(!cfg.age_recipients[0].added.is_empty());

        cfg.remove_age_recipient("age1alice");
        assert_eq!(cfg.age_recipients.len(), 1);
        assert_eq!(cfg.age_recipients[0].recipient, "age1bob");

        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.age_recipients, cfg.age_recipients);
    }

    #[test]
    fn duplicate_destination_flags_cross_rule_collisions_only() {
        let rule = |fpr: &str, dests: &[&str]| SaveRule {
            key_fingerprint: fpr.to_string(),
            key_label: String::new(),
            age_recipient: String::new(),
            destinations: dests.iter().map(PathBuf::from).collect(),
        };

        // Distinct destinations across rules: fine.
        assert_eq!(
            duplicate_destination(&[rule("A", &["/a.gpg", "/a2.gpg"]), rule("B", &["/b.gpg"])]),
            None
        );

        // The same path under two different keys: flagged.
        assert_eq!(
            duplicate_destination(&[rule("A", &["/x.gpg"]), rule("B", &["/x.gpg"])]),
            Some(PathBuf::from("/x.gpg"))
        );

        // A duplicate within one rule is not flagged (same ciphertext).
        assert_eq!(
            duplicate_destination(&[rule("A", &["/x.gpg", "/x.gpg"])]),
            None
        );
    }

    #[test]
    fn post_save_commands_roundtrip_through_toml() {
        let mut cfg = Config::default();
        cfg.app.post_save_command = "backup.sh".to_string();
        let mut plan = sample_plan("/n.md.gpg");
        plan.post_save_command = "rsync -a \"$SCHL8_SOURCE\" host:".to_string();
        cfg.set_plan(plan);
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.app.post_save_command, "backup.sh");
        assert_eq!(
            back.plan_for(Path::new("/n.md.gpg"))
                .unwrap()
                .post_save_command,
            "rsync -a \"$SCHL8_SOURCE\" host:"
        );
    }

    #[test]
    fn quicknote_registry_roundtrips_through_toml() {
        let mut cfg = Config::default();
        cfg.set_quicknotes(vec![QuickNoteFile {
            name: "Journal".into(),
            source: PathBuf::from("/j.md.gpg"),
            rules: vec![SaveRule {
                key_fingerprint: "AB".repeat(20),
                key_label: "Me".into(),
                age_recipient: String::new(),
                destinations: vec![
                    PathBuf::from("/j.md.gpg"),
                    PathBuf::from("/backup/j.md.gpg"),
                ],
            }],
            hotkey: "ctrl+cmd+1".into(),
        }]);
        cfg.quick_note.window_size = Some([520.0, 400.0]);
        cfg.quick_note.window_pos = Some([100.0, 80.0]);
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.quick_note.notes, cfg.quick_note.notes);
        assert_eq!(back.quick_note.window_size, Some([520.0, 400.0]));
        assert!(back.quicknote_for(Path::new("/j.md.gpg")).is_some());
    }

    fn sample_plan(source: &str) -> SavePlan {
        SavePlan {
            source: PathBuf::from(source),
            rules: vec![SaveRule {
                key_fingerprint: "AA11".repeat(10),
                key_label: "Alice <a@x>".to_string(),
                age_recipient: String::new(),
                destinations: vec![PathBuf::from("/one.md.gpg"), PathBuf::from("/two.md.asc")],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn save_plan_set_get_replace_remove() {
        let mut cfg = Config::default();
        assert!(cfg.plan_for(Path::new("/n.md.gpg")).is_none());

        cfg.set_plan(sample_plan("/n.md.gpg"));
        assert_eq!(
            cfg.plan_for(Path::new("/n.md.gpg")).unwrap().rules[0]
                .destinations
                .len(),
            2
        );

        // Replacing updates in place (still one plan for the source).
        let mut updated = sample_plan("/n.md.gpg");
        updated.rules[0].destinations.pop();
        cfg.set_plan(updated);
        assert_eq!(cfg.save_plans.len(), 1);
        assert_eq!(
            cfg.plan_for(Path::new("/n.md.gpg")).unwrap().rules[0]
                .destinations
                .len(),
            1
        );

        // A plan with no usable rules is removed.
        let mut empty = sample_plan("/n.md.gpg");
        empty.rules[0].destinations.clear();
        cfg.set_plan(empty);
        assert!(cfg.plan_for(Path::new("/n.md.gpg")).is_none());
        assert!(cfg.save_plans.is_empty());
    }

    #[test]
    fn save_plans_roundtrip_through_toml() {
        let mut cfg = Config::default();
        cfg.set_plan(sample_plan("/roundtrip.md.gpg"));
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.save_plans, cfg.save_plans);
    }
}

#[cfg(test)]
mod age_lock_tests {
    use super::*;

    /// Guard rail: serializing a full Config must never emit anything that
    /// looks like AGE private key material. The identity is RAM-only by
    /// construction (no Serialize on AgeIdentity), and this catches anyone
    /// later adding a field that would change that.
    #[test]
    fn serialized_config_contains_no_age_secrets() {
        let mut cfg = Config::default();
        cfg.add_age_recipient(
            "mine",
            "age1uwlr4jpxxu3q9v0wtlc8h2f6e72zwxsps05uwquf8jqa0f06p5cs82yjxq",
        );
        let toml = toml::to_string_pretty(&cfg).expect("config serializes");

        // The public recipient is expected; secret material is not.
        assert!(toml.contains("age1uwlr4jpxxu3q9v0wtlc8h2f6e72zwxsps05uwquf8jqa0f06p5cs82yjxq"));
        for forbidden in [
            "AGE-SECRET-KEY",
            "mnemonic",
            "seed_phrase",
            "passphrase",
            "private_key",
        ] {
            assert!(
                !toml.contains(forbidden),
                "config must never persist {forbidden}:\n{toml}"
            );
        }
    }

    #[test]
    fn age_lock_defaults_are_conservative_but_usable() {
        let d = AgeLockSection::default();
        // Idle wipe on by default; no surprise wipe mid-quicknote.
        assert_eq!(d.forget_idle_minutes, 15);
        assert!(d.forget_on_sleep);
        assert!(!d.forget_on_window_close);
        assert_eq!(d.forget_after_minutes, 0);
    }

    #[test]
    fn age_lock_survives_a_config_round_trip() {
        let mut cfg = Config::default();
        cfg.age_lock.forget_idle_minutes = 3;
        cfg.age_lock.forget_after_minutes = 60;
        cfg.age_lock.forget_on_window_close = true;
        cfg.age_lock.forget_on_sleep = false;

        let toml = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&toml).unwrap();
        assert_eq!(back.age_lock, cfg.age_lock);
    }
}

/// Remembering what a file's ciphertext hashed to last time, so a
/// change can be reported rather than left to be noticed.
#[cfg(test)]
mod fingerprint_memory_tests {
    use super::*;

    // ── Remembered fingerprints ──────────────────────────────────────

    const A: &str = "dfdc256a5219be4354e6f3c63e18a9c235a0f6fc3648288efcb04009c808f2a1";
    const B: &str = "9e805359ea4af972ec1a5ac8d55c73e475d13fdd504e1ec719415e9f9063b59b";

    /// A file Schl8 has never seen cannot have changed. Greeting every
    /// new file with a change warning is how a warning gets ignored.
    #[test]
    fn a_first_sighting_is_not_a_change() {
        let mut cfg = Config::default();
        let p = Path::new("/tmp/notes/first.md.gpg");
        assert_eq!(cfg.remember_digest(p, A), None);
        assert_eq!(cfg.remembered_digest(p), Some(A));
    }

    /// Reopening an untouched file must stay silent, however many times.
    #[test]
    fn reopening_an_untouched_file_reports_nothing() {
        let mut cfg = Config::default();
        let p = Path::new("/tmp/notes/steady.md.gpg");
        cfg.remember_digest(p, A);
        for _ in 0..5 {
            assert_eq!(cfg.remember_digest(p, A), None);
        }
        assert_eq!(cfg.seen_files.len(), 1, "no duplicate entries per path");
    }

    /// The case the feature exists for — and it must report the change
    /// exactly once, then treat the new digest as the baseline. A second
    /// report would nag about a change the user has already been told of.
    #[test]
    fn a_changed_file_is_reported_once_then_becomes_the_baseline() {
        let mut cfg = Config::default();
        let p = Path::new("/tmp/notes/changed.md.gpg");
        cfg.remember_digest(p, A);
        assert_eq!(cfg.remember_digest(p, B).as_deref(), Some(A));
        assert_eq!(cfg.remember_digest(p, B), None, "reported twice");
    }

    /// Files are tracked independently: editing one must not make
    /// another look changed.
    #[test]
    fn files_do_not_contaminate_each_other() {
        let mut cfg = Config::default();
        let (x, y) = (Path::new("/tmp/a.gpg"), Path::new("/tmp/b.gpg"));
        cfg.remember_digest(x, A);
        cfg.remember_digest(y, A);
        assert_eq!(cfg.remember_digest(x, B).as_deref(), Some(A));
        assert_eq!(cfg.remember_digest(y, A), None, "y was never touched");
    }

    #[test]
    fn the_list_is_capped_and_keeps_the_most_recent() {
        let mut cfg = Config::default();
        for i in 0..(MAX_REMEMBERED + 25) {
            cfg.remember_digest(&PathBuf::from(format!("/tmp/n{i}.gpg")), A);
        }
        assert_eq!(cfg.seen_files.len(), MAX_REMEMBERED);
        let newest = format!("/tmp/n{}.gpg", MAX_REMEMBERED + 24);
        assert_eq!(cfg.seen_files[0].path, PathBuf::from(newest));
    }

    /// Junk in must not be recorded: a truncated or non-hex digest would
    /// make every subsequent open look like a change.
    #[test]
    fn malformed_digests_are_refused() {
        let mut cfg = Config::default();
        let p = Path::new("/tmp/notes/junk.md.gpg");
        cfg.remember_digest(p, "deadbeef");
        cfg.remember_digest(Path::new("relative.gpg"), A);
        assert!(cfg.seen_files.is_empty());
    }

    /// Clearing a dead recent entry takes its fingerprint with it.
    #[test]
    fn removing_a_recent_entry_forgets_its_fingerprint() {
        let mut cfg = Config::default();
        let p = Path::new("/tmp/notes/gone.md.gpg");
        cfg.add_recent(p);
        cfg.remember_digest(p, A);
        cfg.remove_recent(p);
        assert_eq!(cfg.remembered_digest(p), None);
    }

    #[test]
    fn remembered_fingerprints_survive_a_config_round_trip() {
        let mut cfg = Config::default();
        cfg.remember_digest(Path::new("/tmp/notes/rt.md.gpg"), A);
        let back: Config = toml::from_str(&toml::to_string_pretty(&cfg).unwrap()).unwrap();
        assert_eq!(back.seen_files, cfg.seen_files);
    }
}
