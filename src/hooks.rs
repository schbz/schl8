//! Post-save command hooks.
//!
//! Users can configure a shell command to run after successful saves —
//! globally (`[app] post_save_command`) and/or per save plan — to kick
//! off backups, server uploads, git commits, etc.
//!
//! Security posture: the command runs in the background via `/bin/sh -c`
//! and receives only *paths* of encrypted output files through the
//! environment (`SCHL8_SOURCE`, `SCHL8_DESTINATIONS`). Document
//! content is never passed, and the hook runs strictly after plaintext
//! has been encrypted and written. The command string itself comes from
//! the user's own config, which never stores content or key material.

use std::path::{Path, PathBuf};

/// Run `cmd` in the background (non-blocking). `source` is the saved
/// document's path; `destinations` are every encrypted file written by
/// the save (for an in-place save, just the source). No-op for blank
/// commands.
pub fn run_post_save(cmd: &str, source: &Path, destinations: &[PathBuf]) {
    let cmd = cmd.trim().to_string();
    if cmd.is_empty() {
        return;
    }
    let src = source.to_string_lossy().into_owned();
    let dests = destinations
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("\n");

    std::thread::spawn(move || {
        match std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&cmd)
            .env("SCHL8_SOURCE", &src)
            .env("SCHL8_DESTINATIONS", &dests)
            .stdin(std::process::Stdio::null())
            .output()
        {
            Ok(out) if !out.status.success() => {
                eprintln!(
                    "post-save command exited with {}: {}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            Err(e) => eprintln!("post-save command failed to start: {e}"),
            _ => {}
        }
    });
}

/// The destination set a save wrote: the plan's destinations when a plan
/// ran, else just the source file.
pub fn plan_destinations(plan: &crate::config::SavePlan) -> Vec<PathBuf> {
    plan.rules
        .iter()
        .flat_map(|r| r.destinations.iter().cloned())
        .collect()
}
