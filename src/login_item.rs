//! Start-at-login via a per-user LaunchAgent.
//!
//! Enabling writes `~/Library/LaunchAgents/com.functiondesk.schl8.plist`
//! with `RunAtLoad`, pointing at the installed app binary (preferring
//! `/Applications/Schl8.app`, falling back to the current executable).
//! Disabling removes the plist. No sudo, no system domains — this is the
//! plain, user-serviceable mechanism, and deleting the file by hand is
//! always enough to undo it.

use std::path::PathBuf;

use anyhow::{Context, Result};

const LABEL: &str = "com.functiondesk.schl8";

fn plist_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library/LaunchAgents")
            .join(format!("{LABEL}.plist")),
    )
}

/// The binary the agent should launch: the installed app bundle if
/// present, else whatever is currently running.
fn launch_binary() -> Result<PathBuf> {
    let bundled = PathBuf::from("/Applications/Schl8.app/Contents/MacOS/schl8");
    if bundled.exists() {
        return Ok(bundled);
    }
    std::env::current_exe().context("could not determine the running executable")
}

/// Whether the login item is currently installed.
pub fn is_enabled() -> bool {
    plist_path().map(|p| p.exists()).unwrap_or(false)
}

/// Install or remove the login item. Takes effect at the next login
/// (enabling also loads it immediately via launchctl, best-effort).
pub fn set_enabled(enable: bool) -> Result<()> {
    let path = plist_path().context("could not determine ~/Library/LaunchAgents")?;
    if enable {
        let bin = launch_binary()?;
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array><string>{}</string></array>
    <key>RunAtLoad</key><true/>
    <key>LimitLoadToSessionType</key><string>Aqua</string>
</dict>
</plist>
"#,
            bin.display()
        );
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }
        std::fs::write(&path, plist)
            .with_context(|| format!("failed to write {}", path.display()))?;
        // Best-effort immediate load; ignore failure (works at next login).
        let _ = std::process::Command::new("launchctl")
            .args(["load", "-w"])
            .arg(&path)
            .output();
    } else if path.exists() {
        let _ = std::process::Command::new("launchctl")
            .args(["unload"])
            .arg(&path)
            .output();
        std::fs::remove_file(&path)
            .with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}
