//! Menu-bar residency: a status-bar item plus a system-wide quick-note
//! hotkey. While resident, closing the window hides Schl8 instead of
//! quitting — any open document is closed (and its buffers zeroized)
//! first, so a hidden Schl8 never holds plaintext.

use std::sync::Mutex;

use std::path::PathBuf;

use anyhow::{Context, Result};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};

/// Ids of global hotkeys pressed since the last poll (the main quick-note
/// hotkey and any per-quicknote hotkeys are distinguished by id).
static HOTKEYS_FIRED: Mutex<Vec<u32>> = Mutex::new(Vec::new());
static MENU_CLICKS: Mutex<Vec<MenuId>> = Mutex::new(Vec::new());

/// Decode an embedded template glyph into a tray icon.
fn decode_glyph(png: &[u8]) -> anyhow::Result<tray_icon::Icon> {
    let img = image::load_from_memory(png)
        .context("embedded tray icon")?
        .to_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).context("tray icon rgba")
}

/// "y" / "ies" for entry counts.
fn plural(n: usize) -> &'static str {
    if n == 1 {
        "y"
    } else {
        "ies"
    }
}

/// Events surfaced to the app each frame.
pub enum ResidentEvent {
    /// Open the jot window on the last-used quicknote.
    QuickNote,
    /// Open the jot window preselected on a specific registry file.
    QuickNoteFor(PathBuf),
    /// Open the main window with the quicknotes manager showing.
    ManageQuickNotes,
    /// Open a favorite file for reading/editing.
    OpenFavorite(PathBuf),
    /// Open the main window with the favorites manager showing.
    ManageFavorites,
    /// Create a new unsaved document of this type.
    NewFile(crate::document::FileType),
    /// Merge every note's spooled (offline) entries, unlocking if needed.
    MergePending,
    /// Discard every spooled entry — destructive, so the app confirms.
    DiscardPending,
    ShowWindow,
    Quit,
}

pub struct Resident {
    tray: TrayIcon,
    hotkeys: GlobalHotKeyManager,
    /// Currently-registered global hotkey, so it can be unregistered on change.
    current_hotkey: global_hotkey::hotkey::HotKey,
    /// The "Quick Note ▸" submenu, so its per-file items can be rebuilt
    /// when the registry changes.
    quick_menu: Submenu,
    /// Live per-file submenu items: (menu id, registry source path).
    note_items: Vec<(MenuId, PathBuf)>,
    /// The "Favorites ▸" submenu, rebuilt when the list changes.
    fav_menu: Submenu,
    /// Live favorite items: (menu id, file to open).
    fav_items: Vec<(MenuId, PathBuf)>,
    /// Favorites snapshot the submenu was last built from.
    fav_snapshot: Vec<(String, PathBuf)>,
    /// Registered per-favorite global hotkeys: (hotkey, file to open).
    fav_hotkeys: Vec<(global_hotkey::hotkey::HotKey, PathBuf)>,
    /// Hotkey-spec snapshot for favorites.
    fav_hotkey_snapshot: Vec<(String, PathBuf)>,
    id_manage_favs: MenuId,
    id_new_md: MenuId,
    id_new_txt: MenuId,
    /// Registry snapshot the submenu was last built from (skip rebuilds).
    note_snapshot: Vec<(String, PathBuf)>,
    /// Registered per-quicknote hotkeys: (hotkey, jot target path).
    note_hotkeys: Vec<(global_hotkey::hotkey::HotKey, PathBuf)>,
    /// "Merge / Discard pending" items and the separator above them; they
    /// live in the menu permanently and are enabled only when the spool
    /// is non-empty, which is cheaper and less jumpy than inserting and
    /// removing them.
    merge_item: MenuItem,
    discard_item: MenuItem,
    /// Plain and badged menu-bar glyphs, decoded once and swapped when
    /// offline entries appear or clear.
    icon_plain: tray_icon::Icon,
    icon_pending: tray_icon::Icon,
    /// Pending total the labels were last built for (skip needless work).
    pending_snapshot: usize,
    /// Hotkey-spec snapshot the registrations were last built from.
    hotkey_snapshot: Vec<(String, PathBuf)>,
    id_quick: MenuId,
    id_manage: MenuId,
    id_show: MenuId,
    id_quit: MenuId,
}

impl Resident {
    /// Set up the status item and global hotkey. Must run on the main
    /// thread after the event loop has started (i.e. from `App::update`).
    pub fn init(ctx: &egui::Context, hotkey_spec: &str) -> Result<Self> {
        // ── Global hotkey ────────────────────────────────────────────
        let manager = GlobalHotKeyManager::new()
            .map_err(|e| anyhow::anyhow!("global hotkey manager: {e}"))?;
        let hotkey = crate::hotkey::parse(hotkey_spec)?;
        manager
            .register(hotkey)
            .map_err(|e| anyhow::anyhow!("registering hotkey \"{hotkey_spec}\": {e}"))?;
        {
            let ctx = ctx.clone();
            GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
                if event.state() == HotKeyState::Pressed {
                    if let Ok(mut fired) = HOTKEYS_FIRED.lock() {
                        fired.push(event.id());
                    }
                    ctx.request_repaint();
                }
            }));
        }

        // ── Status-bar item ──────────────────────────────────────────
        // "Quick Note" is a submenu: a default "New Quick Note" entry
        // (last-used file), the registered quicknote files, and a
        // shortcut to the manager. Per-file items are (re)built by
        // `sync_quicknotes`.
        let menu = Menu::new();
        let quick_menu = Submenu::new("Quick Note", true);
        let quick = MenuItem::new("New Quick Note", true, None);
        let manage = MenuItem::new("Manage Quick Notes…", true, None);
        quick_menu.append(&quick).context("tray menu")?;
        quick_menu
            .append(&PredefinedMenuItem::separator())
            .context("tray menu")?;
        quick_menu.append(&manage).context("tray menu")?;
        // Offline (spooled) entries: disabled until there are some.
        let merge_item = MenuItem::new("Merge Pending Entries…", false, None);
        let discard_item = MenuItem::new("Discard Pending Entries…", false, None);
        quick_menu
            .append(&PredefinedMenuItem::separator())
            .context("tray menu")?;
        quick_menu.append(&merge_item).context("tray menu")?;
        quick_menu.append(&discard_item).context("tray menu")?;
        // "Favorites": files you open often, plus a way to edit the list.
        // Per-file entries are (re)built by `sync_favorites`.
        let fav_menu = Submenu::new("Favorites", true);
        let manage_favs = MenuItem::new("Manage Favorites\u{2026}", true, None);
        fav_menu
            .append(&PredefinedMenuItem::separator())
            .context("tray menu")?;
        fav_menu.append(&manage_favs).context("tray menu")?;

        // "New": the same document kinds the File menu offers, so a new
        // note can be started without the window being open first.
        let new_menu = Submenu::new("New", true);
        let new_md = MenuItem::new("Markdown File", true, None);
        let new_txt = MenuItem::new("Text File", true, None);
        new_menu.append(&new_md).context("tray menu")?;
        new_menu.append(&new_txt).context("tray menu")?;

        let show = MenuItem::new("Open Schl8", true, None);
        let quit = MenuItem::new("Quit Schl8", true, None);
        menu.append(&quick_menu).context("tray menu")?;
        menu.append(&fav_menu).context("tray menu")?;
        menu.append(&new_menu).context("tray menu")?;
        menu.append(&PredefinedMenuItem::separator())
            .context("tray menu")?;
        menu.append(&show).context("tray menu")?;
        menu.append(&PredefinedMenuItem::separator())
            .context("tray menu")?;
        menu.append(&quit).context("tray menu")?;

        {
            let ctx = ctx.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                if let Ok(mut clicks) = MENU_CLICKS.lock() {
                    clicks.push(event.id().clone());
                }
                ctx.request_repaint();
            }));
        }

        // One-tone template glyph (black + alpha): macOS recolors it to
        // match the menu bar in light and dark mode. Generated by
        // `cargo run --bin gen_icons`.
        let icon = decode_glyph(include_bytes!("../assets/tray_glyph.png"))?;
        let icon_pending = decode_glyph(include_bytes!("../assets/tray_glyph_pending.png"))?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(icon.clone())
            .with_icon_as_template(true) // adapts to light/dark menu bar
            .with_tooltip("Schl8")
            .build()
            .map_err(|e| anyhow::anyhow!("status item: {e}"))?;

        Ok(Resident {
            tray,
            hotkeys: manager,
            current_hotkey: hotkey,
            quick_menu,
            note_items: Vec::new(),
            note_snapshot: Vec::new(),
            fav_menu,
            fav_items: Vec::new(),
            fav_snapshot: Vec::new(),
            fav_hotkeys: Vec::new(),
            fav_hotkey_snapshot: Vec::new(),
            id_manage_favs: manage_favs.id().clone(),
            id_new_md: new_md.id().clone(),
            id_new_txt: new_txt.id().clone(),
            note_hotkeys: Vec::new(),
            merge_item,
            discard_item,
            icon_plain: icon,
            icon_pending,
            pending_snapshot: 0,
            hotkey_snapshot: Vec::new(),
            id_quick: quick.id().clone(),
            id_manage: manage.id().clone(),
            id_show: show.id().clone(),
            id_quit: quit.id().clone(),
        })
    }

    /// Rebuild the per-file entries of the Quick Note submenu from the
    /// registry (`(display name, source path)` pairs). No-op when the
    /// registry hasn't changed since the last build.
    pub fn sync_quicknotes(&mut self, notes: &[(String, PathBuf)]) {
        if self.note_snapshot == notes {
            return;
        }

        // Remove the previous per-file items (they sit between the
        // default item + separator at the top and the trailing
        // separator + manage item).
        let items = self.quick_menu.items();
        for (id, _) in self.note_items.drain(..) {
            if let Some(item) = items
                .iter()
                .filter_map(|i| i.as_menuitem())
                .find(|i| i.id() == &id)
            {
                let _ = self.quick_menu.remove(item);
            }
        }

        // Insert fresh items after the leading "New Quick Note" +
        // separator (positions 0 and 1).
        for (idx, (name, path)) in notes.iter().enumerate() {
            let item = MenuItem::new(name.as_str(), true, None);
            if self.quick_menu.insert(&item, 2 + idx).is_ok() {
                self.note_items.push((item.id().clone(), path.clone()));
            }
        }
        self.note_snapshot = notes.to_vec();
    }

    /// Rebuild the Favorites submenu from `(label, path)` pairs. No-op
    /// when unchanged. Entries are inserted above the trailing separator
    /// and "Manage Favorites…", so the list order is the menu order.
    pub fn sync_favorites(&mut self, favorites: &[(String, PathBuf)]) {
        if self.fav_snapshot == favorites {
            return;
        }
        let items = self.fav_menu.items();
        for (id, _) in self.fav_items.drain(..) {
            if let Some(item) = items
                .iter()
                .filter_map(|i| i.as_menuitem())
                .find(|i| i.id() == &id)
            {
                let _ = self.fav_menu.remove(item);
            }
        }
        for (idx, (label, path)) in favorites.iter().enumerate() {
            let item = MenuItem::new(label.as_str(), true, None);
            if self.fav_menu.insert(&item, idx).is_ok() {
                self.fav_items.push((item.id().clone(), path.clone()));
            }
        }
        self.fav_snapshot = favorites.to_vec();
    }

    /// (Re)register per-favorite global hotkeys from `(spec, path)` pairs
    /// (blank specs skipped). Returns human-readable errors for combos
    /// that failed to register, which the caller surfaces once.
    pub fn sync_favorite_hotkeys(&mut self, specs: &[(String, PathBuf)]) -> Vec<String> {
        if self.fav_hotkey_snapshot == specs {
            return Vec::new();
        }
        let mut errors = Vec::new();
        for (hk, _) in self.fav_hotkeys.drain(..) {
            let _ = self.hotkeys.unregister(hk);
        }
        for (spec, path) in specs {
            let spec = spec.trim();
            if spec.is_empty() {
                continue;
            }
            match crate::hotkey::parse(spec) {
                Ok(hk) if hk == self.current_hotkey => {
                    errors.push(format!("{spec}: already the main Quick Note hotkey"));
                }
                Ok(hk) => match self.hotkeys.register(hk) {
                    Ok(()) => self.fav_hotkeys.push((hk, path.clone())),
                    Err(e) => errors.push(format!("{spec}: {e}")),
                },
                Err(e) => errors.push(format!("{spec}: {e}")),
            }
        }
        self.fav_hotkey_snapshot = specs.to_vec();
        errors
    }

    /// Reflect the number of spooled entries waiting to be merged: the
    /// two items enable only when there is something to act on, and the
    /// count is shown so the menu answers "how much is waiting?" without
    /// opening the app.
    pub fn sync_pending(&mut self, pending: usize) {
        if self.pending_snapshot == pending {
            return;
        }
        self.merge_item.set_enabled(pending > 0);
        self.discard_item.set_enabled(pending > 0);
        if pending > 0 {
            self.merge_item
                .set_text(format!("Merge {pending} Pending Entr{}…", plural(pending)));
            self.discard_item.set_text(format!(
                "Discard {pending} Pending Entr{}…",
                plural(pending)
            ));
        } else {
            self.merge_item.set_text("Merge Pending Entries…");
            self.discard_item.set_text("Discard Pending Entries…");
        }
        // Ambient signal: a badged glyph means work is waiting, visible
        // without opening the menu at all.
        let icon = if pending > 0 {
            self.icon_pending.clone()
        } else {
            self.icon_plain.clone()
        };
        if let Err(e) = self.tray.set_icon(Some(icon)) {
            eprintln!("warning: could not update menu-bar icon: {e}");
        }
        // set_icon resets template mode; without re-asserting it macOS
        // draws the raw black glyph instead of recoloring it to match the
        // menu bar (white on dark), unlike every neighbouring icon.
        self.tray.set_icon_as_template(true);
        self.pending_snapshot = pending;
    }

    /// (Re)register per-quicknote global hotkeys from `(spec, path)`
    /// pairs (blank specs skipped). No-op when unchanged. Returns
    /// human-readable errors for combos that failed to register (e.g.
    /// taken by another app) — the caller surfaces them once.
    pub fn sync_note_hotkeys(&mut self, specs: &[(String, PathBuf)]) -> Vec<String> {
        if self.hotkey_snapshot == specs {
            return Vec::new();
        }

        let mut errors = Vec::new();
        for (hk, _) in self.note_hotkeys.drain(..) {
            let _ = self.hotkeys.unregister(hk);
        }
        for (spec, path) in specs {
            let spec = spec.trim();
            if spec.is_empty() {
                continue;
            }
            match crate::hotkey::parse(spec) {
                Ok(hk) if hk == self.current_hotkey => {
                    errors.push(format!("{spec}: already the main Quick Note hotkey"));
                }
                Ok(hk) => match self.hotkeys.register(hk) {
                    Ok(()) => self.note_hotkeys.push((hk, path.clone())),
                    Err(e) => errors.push(format!("{spec}: {e}")),
                },
                Err(e) => errors.push(format!("{spec}: {e}")),
            }
        }
        self.hotkey_snapshot = specs.to_vec();
        errors
    }

    /// Re-register the system-wide quick-note hotkey to a new spec. The old
    /// binding is only removed once the new one registers successfully, so a
    /// bad/taken combo leaves the previous hotkey working.
    pub fn set_hotkey(&mut self, spec: &str) -> Result<()> {
        let new = crate::hotkey::parse(spec)?;
        if new == self.current_hotkey {
            return Ok(());
        }
        self.hotkeys
            .register(new)
            .map_err(|e| anyhow::anyhow!("registering hotkey \"{spec}\": {e}"))?;
        let _ = self.hotkeys.unregister(self.current_hotkey);
        self.current_hotkey = new;
        Ok(())
    }

    /// Drain events fired since the last frame.
    pub fn poll(&self) -> Vec<ResidentEvent> {
        let mut out = Vec::new();
        if let Ok(mut fired) = HOTKEYS_FIRED.lock() {
            for id in fired.drain(..) {
                if id == self.current_hotkey.id() {
                    out.push(ResidentEvent::QuickNote);
                } else if let Some((_, path)) =
                    self.note_hotkeys.iter().find(|(hk, _)| hk.id() == id)
                {
                    out.push(ResidentEvent::QuickNoteFor(path.clone()));
                } else if let Some((_, path)) =
                    self.fav_hotkeys.iter().find(|(hk, _)| hk.id() == id)
                {
                    out.push(ResidentEvent::OpenFavorite(path.clone()));
                }
            }
        }
        if let Ok(mut clicks) = MENU_CLICKS.lock() {
            for id in clicks.drain(..) {
                if id == self.id_quick {
                    out.push(ResidentEvent::QuickNote);
                } else if id == self.id_manage {
                    out.push(ResidentEvent::ManageQuickNotes);
                } else if id == self.merge_item.id() {
                    out.push(ResidentEvent::MergePending);
                } else if id == self.discard_item.id() {
                    out.push(ResidentEvent::DiscardPending);
                } else if id == self.id_show {
                    out.push(ResidentEvent::ShowWindow);
                } else if id == self.id_quit {
                    out.push(ResidentEvent::Quit);
                } else if id == self.id_manage_favs {
                    out.push(ResidentEvent::ManageFavorites);
                } else if id == self.id_new_md {
                    out.push(ResidentEvent::NewFile(crate::document::FileType::Markdown));
                } else if id == self.id_new_txt {
                    out.push(ResidentEvent::NewFile(crate::document::FileType::PlainText));
                } else if let Some((_, path)) =
                    self.note_items.iter().find(|(item_id, _)| item_id == &id)
                {
                    out.push(ResidentEvent::QuickNoteFor(path.clone()));
                } else if let Some((_, path)) =
                    self.fav_items.iter().find(|(item_id, _)| item_id == &id)
                {
                    out.push(ResidentEvent::OpenFavorite(path.clone()));
                }
            }
        }
        out
    }
}
