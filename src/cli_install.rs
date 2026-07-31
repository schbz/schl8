//! Putting `schl8` on the user's PATH.
//!
//! Everything on the agent surface starts with the agent running
//! `schl8 agent brief`. Inside an app bundle the binary lives at
//! `/Applications/Schl8.app/Contents/MacOS/schl8`, which no shell
//! will find, so the first command an agent runs fails and the whole
//! integration dies at step one. This module is the fix: a symlink from
//! a directory the user's terminal can see.
//!
//! Two things make it less trivial than `ln -s`:
//!
//! 1. **A GUI app does not know the user's PATH.** Launched from Finder,
//!    Schl8 inherits a minimal environment (`/usr/bin:/bin:…`) rather
//!    than the PATH the login shell assembles from `.zprofile` and
//!    friends. Linking into a directory that is writable but invisible
//!    to the terminal looks like success and isn't. So we ask the login
//!    shell.
//! 2. **`/usr/local/bin` is root-owned.** It is the conventional home
//!    for this, but writing there needs an administrator. A security
//!    app should not ask for a password it doesn't need, so an already
//!    writable directory on PATH always wins — on a Homebrew Mac that
//!    is `/opt/homebrew/bin` and nobody is prompted at all.
//!
//! No plaintext, no key material, nothing encrypted: this module moves
//! one symlink.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// The name the symlink takes.
const LINK_NAME: &str = "schl8";

/// Directories to consider, best first.
///
/// `/opt/homebrew/bin` leads on Apple Silicon because it is both on PATH
/// and user-writable, which is the only combination that needs no
/// password. `/usr/local/bin` is the traditional answer and comes next.
fn candidates() -> Vec<PathBuf> {
    let mut v = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        v.push(home.join(".local/bin"));
        v.push(home.join("bin"));
    }
    v
}

/// The binary to point at: the installed bundle if there is one, else
/// whatever is running now (which is what a developer wants).
fn target_binary() -> Result<PathBuf> {
    let bundled = PathBuf::from("/Applications/Schl8.app/Contents/MacOS/schl8");
    if bundled.exists() {
        return Ok(bundled);
    }
    std::env::current_exe().context("could not determine the running executable")
}

/// The PATH a terminal will actually have, by asking the login shell.
///
/// `-l -c` runs the login startup files (`.zprofile`, `.profile`) and
/// exits, so it terminates on its own even if the user's setup is
/// unusual. Interactive files like `.zshrc` are deliberately not read:
/// an interactive shell can block on a prompt, and hanging the UI to
/// learn a PATH is a bad trade. If someone sets PATH only in `.zshrc`
/// we may under-report — which shows up as advice to edit their profile,
/// not as a silent failure.
fn shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").ok()?;
    let out = std::process::Command::new(shell)
        .args(["-l", "-c", "printf %s \"$PATH\""])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8(out.stdout).ok()?;
    if path.trim().is_empty() {
        None
    } else {
        Some(path)
    }
}

/// Is `dir` a component of `path_var`?
fn on_path(dir: &Path, path_var: &str) -> bool {
    path_var.split(':').any(|p| Path::new(p) == dir)
}

/// How the install will have to be done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    /// The directory is writable and on PATH: a plain symlink.
    Direct,
    /// On PATH but root-owned: needs an administrator prompt.
    Admin,
    /// Nothing suitable exists, so we create a user directory and the
    /// user has to add it to their shell profile. Carries the line.
    NeedsPathEdit {
        export_line: String,
        profile: String,
    },
}

/// What `install` is about to do.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Where the symlink goes (`<dir>/schl8`).
    pub link: PathBuf,
    /// The binary it points at.
    pub target: PathBuf,
    pub method: Method,
}

/// Current state of the command-line tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// A symlink of ours exists and points at this binary.
    Installed(PathBuf),
    /// A `schl8` exists on PATH but is not our symlink — an older
    /// copy, a Homebrew build, something hand-placed. Worth saying out
    /// loud rather than overwriting.
    Foreign(PathBuf),
    NotInstalled,
}

/// Where a symlink named `schl8` currently lives, if anywhere.
fn existing_link() -> Option<PathBuf> {
    candidates()
        .into_iter()
        .map(|d| d.join(LINK_NAME))
        .find(|p| p.symlink_metadata().is_ok())
}

/// Is the command-line tool installed, and is it ours?
pub fn status() -> Status {
    let Some(link) = existing_link() else {
        return Status::NotInstalled;
    };
    match (std::fs::read_link(&link), target_binary()) {
        (Ok(dest), Ok(target)) if dest == target => Status::Installed(link),
        // A symlink somewhere else, or a real file. Either way it is not
        // the one we would write, so do not claim it.
        _ => Status::Foreign(link),
    }
}

/// Decide where the symlink should go and how to write it.
pub fn plan() -> Result<Plan> {
    let target = target_binary()?;
    let path_var = shell_path()
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();

    let dirs = candidates();

    // Best case: already visible to the shell and writable by us.
    if let Some(dir) = dirs
        .iter()
        .find(|d| d.is_dir() && on_path(d, &path_var) && is_writable(d))
    {
        return Ok(Plan {
            link: dir.join(LINK_NAME),
            target,
            method: Method::Direct,
        });
    }

    // On PATH but not ours to write: /usr/local/bin on a stock Mac.
    if let Some(dir) = dirs.iter().find(|d| on_path(d, &path_var)) {
        return Ok(Plan {
            link: dir.join(LINK_NAME),
            target,
            method: Method::Admin,
        });
    }

    // Nothing on PATH. Make a user-owned directory and hand back the
    // line that makes it visible.
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("no home directory"))?;
    let dir = home.join(".local/bin");
    Ok(Plan {
        link: dir.join(LINK_NAME),
        target,
        method: Method::NeedsPathEdit {
            export_line: format!("export PATH=\"{}:$PATH\"", dir.display()),
            profile: profile_file().display().to_string(),
        },
    })
}

/// The shell profile to suggest editing, from `$SHELL`.
fn profile_file() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"));
    let shell = std::env::var("SHELL").unwrap_or_default();
    if shell.ends_with("bash") {
        home.join(".bash_profile")
    } else {
        home.join(".zprofile")
    }
}

/// Can this process create a file in `dir`?
///
/// Asked by attempting it, not by reading the mode: group membership,
/// ACLs and sandboxing all make the mode bits an unreliable predictor,
/// and being wrong here means a confusing failure later.
fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".schl8-write-probe");
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Refuse paths that cannot be embedded safely in the AppleScript we
/// hand to `osascript`.
///
/// The admin path builds a shell command inside an AppleScript string —
/// two levels of quoting, run as root. Rather than try to escape
/// arbitrary bytes correctly, reject anything with a quote, a
/// backslash, or a newline in it. No real install path has these, and
/// the failure mode of getting it wrong is a root shell running
/// something we did not intend.
fn safe_for_applescript(p: &Path) -> bool {
    match p.to_str() {
        Some(s) => !s.contains(['"', '\\', '\n', '\r']),
        None => false,
    }
}

/// Carry out a plan. Returns the installed link path.
pub fn install(plan: &Plan) -> Result<PathBuf> {
    let dir = plan
        .link
        .parent()
        .ok_or_else(|| anyhow::anyhow!("bad link path"))?;

    match &plan.method {
        Method::Admin => {
            if !safe_for_applescript(&plan.link) || !safe_for_applescript(&plan.target) {
                bail!(
                    "refusing to run an administrator command with quotes or \
                     backslashes in the path"
                );
            }
            let script = format!(
                "do shell script \"mkdir -p '{}' && ln -sfn '{}' '{}'\" \
                 with administrator privileges",
                dir.display(),
                plan.target.display(),
                plan.link.display(),
            );
            let out = std::process::Command::new("osascript")
                .args(["-e", &script])
                .output()
                .context("could not run osascript")?;
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                // -128 is the user cancelling the password dialog.
                if err.contains("-128") {
                    bail!("cancelled");
                }
                bail!("{}", err.trim());
            }
        }
        Method::Direct | Method::NeedsPathEdit { .. } => {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("could not create {}", dir.display()))?;
            // Replace any existing link atomically-ish: remove then
            // create. `ln -sfn` semantics, without the subprocess.
            if plan.link.symlink_metadata().is_ok() {
                std::fs::remove_file(&plan.link)
                    .with_context(|| format!("could not replace {}", plan.link.display()))?;
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(&plan.target, &plan.link)
                .with_context(|| format!("could not link {}", plan.link.display()))?;
        }
    }
    Ok(plan.link.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Direct path really creates a working symlink, and really
    /// replaces a stale one.
    ///
    /// Run against a temp directory rather than a real bin dir: the
    /// interesting logic is create-and-replace, and a test that wrote
    /// into `/opt/homebrew/bin` would be modifying the machine to prove
    /// it can modify the machine.
    #[test]
    fn direct_install_creates_and_replaces_the_link() {
        let dir = std::env::temp_dir().join(format!("schl8-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let old_target = dir.join("old-binary");
        let new_target = dir.join("new-binary");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&old_target, b"old").unwrap();
        std::fs::write(&new_target, b"new").unwrap();

        let link = dir.join("bin").join(LINK_NAME);
        let plan_for = |target: &Path| Plan {
            link: link.clone(),
            target: target.to_path_buf(),
            method: Method::Direct,
        };

        // Creates the parent directory as well as the link.
        install(&plan_for(&old_target)).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), old_target);
        assert_eq!(std::fs::read(&link).unwrap(), b"old");

        // Replacing must not fail on the existing link — an app that
        // moved would otherwise be stuck pointing at a binary that is
        // no longer there, with no way to fix it from the menu.
        install(&plan_for(&new_target)).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), new_target);
        assert_eq!(std::fs::read(&link).unwrap(), b"new");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_membership_is_exact() {
        let p = "/opt/homebrew/bin:/usr/bin:/bin";
        assert!(on_path(Path::new("/opt/homebrew/bin"), p));
        assert!(on_path(Path::new("/usr/bin"), p));
        // A prefix of a real entry is not a member — `/usr` must not
        // match `/usr/bin`, or we would link into an unreachable place
        // and report success.
        assert!(!on_path(Path::new("/usr"), p));
        assert!(!on_path(Path::new("/opt/homebrew"), p));
        assert!(!on_path(Path::new("/nope"), p));
    }

    #[test]
    fn applescript_guard_rejects_quoting_tricks() {
        assert!(safe_for_applescript(Path::new("/usr/local/bin/schl8")));
        assert!(safe_for_applescript(Path::new("/Users/a b/bin/schl8")));
        // These are the shapes that could break out of the nested
        // quoting and run as root.
        assert!(!safe_for_applescript(Path::new("/tmp/a'\";touch x;'/s")));
        assert!(!safe_for_applescript(Path::new("/tmp/a\\b/schl8")));
        assert!(!safe_for_applescript(Path::new("/tmp/a\nb/schl8")));
    }

    #[test]
    fn candidates_are_absolute_and_ordered_no_auth_first() {
        let c = candidates();
        assert!(c.iter().all(|p| p.is_absolute()));
        // Homebrew before /usr/local: the first is writable without a
        // password on the Macs that have it, and prompting when we do
        // not have to is the thing this ordering exists to avoid.
        let hb = c.iter().position(|p| p.ends_with("homebrew/bin"));
        let ul = c.iter().position(|p| p == Path::new("/usr/local/bin"));
        assert!(hb < ul, "homebrew must be tried before /usr/local");
    }

    #[test]
    fn writability_is_probed_not_assumed() {
        let dir = std::env::temp_dir();
        assert!(is_writable(&dir), "temp dir should be writable");
        assert!(!is_writable(Path::new("/nonexistent-schl8-test-dir")));
        // The probe must not survive the check.
        assert!(!dir.join(".schl8-write-probe").exists());
    }
}
