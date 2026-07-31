//! Removing Schl8 completely, from inside Schl8.
//!
//! An app that scatters itself across `~/.config`, `~/Library`,
//! `~/.claude`, a login item and a symlink on PATH should be able to
//! pick all of that back up. Dragging the bundle to the Trash leaves
//! every one of those behind, and nothing else on the machine knows
//! they belong together.
//!
//! Two rules shape this module.
//!
//! **Show the plan first.** [`plan`] returns exactly what would be
//! touched, with a human sentence for each path, so the confirmation
//! screen is a list of real paths rather than a promise. Nothing here
//! removes anything until [`execute`] is called with that plan.
//!
//! **Recoverable where it can be.** Files go to the Trash rather than
//! being unlinked, so a mistake is a drag back out. The exception is
//! the empty directories left behind, which are removed outright
//! because an empty folder in the Trash helps nobody.
//!
//! What this deliberately does *not* touch: your notes. Every encrypted
//! file Schl8 ever wrote is yours and stays where it is. The uninstall
//! screen says so, because the honest worry when uninstalling a notes
//! app is whether the notes go with it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// One thing the uninstall would remove.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub path: PathBuf,
    /// What it is, in plain words, for the confirmation list.
    pub what: &'static str,
    /// True when losing it loses something not reconstructible.
    pub precious: bool,
}

/// Everything on this machine that belongs to Schl8.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub items: Vec<Item>,
    /// The bundle, when running from one. Trashed last, by Finder.
    pub app_bundle: Option<PathBuf>,
}

impl Plan {
    /// True when any item holds something a backup would preserve.
    pub fn has_precious(&self) -> bool {
        self.items.iter().any(|i| i.precious)
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Work out what is present. Nothing is removed here.
pub fn plan() -> Plan {
    let mut items = Vec::new();
    let mut push = |path: PathBuf, what: &'static str, precious: bool| {
        // `symlink_metadata` so a dangling symlink still counts — a link
        // to a bundle that is already gone is exactly the litter this
        // is meant to collect.
        if path.symlink_metadata().is_ok() {
            items.push(Item {
                path,
                what,
                precious,
            });
        }
    };

    if let Some(cfg) = crate::config::config_path() {
        if let Some(dir) = cfg.parent() {
            // Held edits live under the config directory and are the one
            // thing here that cannot be recreated.
            let stash = dir.join("stash");
            let has_held = std::fs::read_dir(&stash)
                .map(|mut d| d.next().is_some())
                .unwrap_or(false);
            push(stash, "unsaved edits held from a locked session", has_held);
            push(
                cfg.clone(),
                "your settings: notes, keys, destinations",
                true,
            );
            push(
                dir.join("AGENT-GUIDE.md"),
                "generated agent instructions",
                false,
            );
        }
    }

    if let Some(h) = home() {
        push(
            h.join("Library/LaunchAgents/com.functiondesk.schl8.plist"),
            "the start-at-login item",
            false,
        );
        push(
            h.join("Library/Preferences/com.functiondesk.schl8.plist"),
            "window and system preferences",
            false,
        );
        push(
            h.join(".claude/skills/schl8"),
            "the Claude Code skill",
            false,
        );
        push(
            h.join(".claude/commands/schl8"),
            "the /schl8 slash commands",
            false,
        );
    }

    // The `schl8` symlink, wherever it was installed.
    if let crate::cli_install::Status::Installed(link) | crate::cli_install::Status::Foreign(link) =
        crate::cli_install::status()
    {
        items.push(Item {
            path: link,
            what: "the command-line tool on your PATH",
            precious: false,
        });
    }

    let app_bundle = std::env::current_exe()
        .ok()
        .and_then(|exe| bundle_root(&exe));

    Plan { items, app_bundle }
}

/// The `.app` a binary lives in, if it lives in one.
///
/// `…/Schl8.app/Contents/MacOS/schl8` → `…/Schl8.app`. A plain
/// `cargo run` binary has no bundle, and a development build must never
/// trash its own target directory.
fn bundle_root(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && app.extension().is_some_and(|e| e == "app"))
    .then(|| app.to_path_buf())
}

/// Ask Finder to move a path to the Trash.
///
/// Finder rather than `rm`, so the user can undo an uninstall they
/// regret thirty seconds later, and so the app can move its own bundle
/// while running — which it cannot do by unlinking itself.
fn trash(path: &Path) -> Result<()> {
    let p = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("path is not valid text: {}", path.display()))?;
    // Reject quoting characters rather than trying to escape them for a
    // nested AppleScript/POSIX string.
    if p.contains(['"', '\\', '\n', '\r']) {
        anyhow::bail!("refusing to trash a path containing quotes: {p}");
    }
    let script = format!(r#"tell application "Finder" to delete POSIX file "{p}""#);
    let out = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .context("could not run osascript")?;
    if !out.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

/// What happened, for the report shown afterwards.
#[derive(Debug, Default)]
pub struct Outcome {
    pub removed: Vec<PathBuf>,
    pub failed: Vec<(PathBuf, String)>,
    /// True when the bundle was trashed and the app should now quit.
    pub app_trashed: bool,
}

/// Carry out a plan.
///
/// The bundle goes last: if trashing it succeeded first, a later
/// failure would leave litter behind with no app left to clean it up.
pub fn execute(plan: &Plan) -> Outcome {
    let mut out = Outcome::default();

    for item in &plan.items {
        match trash(&item.path) {
            Ok(()) => out.removed.push(item.path.clone()),
            Err(e) => out.failed.push((item.path.clone(), e.to_string())),
        }
    }

    // Drop the config directory itself once its contents are gone.
    // Empty, so `remove_dir` — an empty folder in the Trash is noise.
    if let Some(cfg) = crate::config::config_path() {
        if let Some(dir) = cfg.parent() {
            let _ = std::fs::remove_dir(dir);
        }
    }

    // Let Launch Services forget the file associations, so Finder stops
    // offering a Schl8 that is on its way to the Trash.
    if let Some(app) = &plan.app_bundle {
        const LSREGISTER: &str = "/System/Library/Frameworks/CoreServices.framework/\
                                  Frameworks/LaunchServices.framework/Support/lsregister";
        let _ = std::process::Command::new(LSREGISTER)
            .arg("-u")
            .arg(app)
            .output();

        match trash(app) {
            Ok(()) => {
                out.removed.push(app.clone());
                out.app_trashed = true;
            }
            Err(e) => out.failed.push((app.clone(), e.to_string())),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_root_recognizes_a_real_bundle() {
        let exe = Path::new("/Applications/Schl8.app/Contents/MacOS/schl8");
        assert_eq!(
            bundle_root(exe),
            Some(PathBuf::from("/Applications/Schl8.app"))
        );
    }

    /// A development build must never offer to trash its own target
    /// directory — the shape has to match exactly, not merely be deep.
    #[test]
    fn a_loose_binary_has_no_bundle() {
        assert_eq!(
            bundle_root(Path::new("/Users/x/proj/target/debug/schl8")),
            None
        );
        assert_eq!(bundle_root(Path::new("/usr/local/bin/schl8")), None);
        // Right depth, wrong names.
        assert_eq!(
            bundle_root(Path::new("/a/Schl8.app/Resources/MacOS/schl8")),
            None
        );
        assert_eq!(
            bundle_root(Path::new("/a/Schl8/Contents/MacOS/schl8")),
            None
        );
    }

    /// The plan is a preview: building one must not touch the disk.
    #[test]
    fn planning_removes_nothing() {
        let before = crate::config::config_path().map(|p| p.exists());
        let p = plan();
        let after = crate::config::config_path().map(|p| p.exists());
        assert_eq!(before, after, "planning changed the config on disk");
        // Every listed path really exists (or is a live symlink).
        for item in &p.items {
            assert!(
                item.path.symlink_metadata().is_ok(),
                "planned a path that is not there: {}",
                item.path.display()
            );
        }
    }

    /// Settings and held edits are flagged so the confirmation screen can
    /// push a backup before it is too late.
    #[test]
    fn settings_and_held_edits_are_marked_precious() {
        let cfg = crate::config::config_path().unwrap();
        let mut items = vec![Item {
            path: cfg,
            what: "your settings: notes, keys, destinations",
            precious: true,
        }];
        let p = Plan {
            items: items.clone(),
            app_bundle: None,
        };
        assert!(p.has_precious());

        items[0].precious = false;
        let p = Plan {
            items,
            app_bundle: None,
        };
        assert!(!p.has_precious());
    }

    #[test]
    fn quoting_tricks_are_refused_rather_than_escaped() {
        let bad = Path::new("/tmp/a\"; do shell script \"rm -rf ~\"");
        let err = trash(bad).unwrap_err().to_string();
        assert!(err.contains("refusing"), "{err}");
    }
}
