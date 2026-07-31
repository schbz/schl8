//! Update check: ask GitHub what the newest released version is and
//! compare it to this build.
//!
//! Deliberately minimal and manual. Schl8 does not phone home on its
//! own — a check only happens when the user picks Help → Check for
//! Updates…, and all it sends is an ordinary HTTPS request to github.com
//! carrying no identifying information beyond what any browser visit
//! would.
//!
//! We shell out to the system `curl` at an absolute path rather than
//! linking an HTTP client. A TLS stack would add a large dependency
//! subtree to an app whose whole pitch is a small, auditable supply
//! chain, and macOS ships a maintained curl. This mirrors how the GPG
//! backend already shells out to a verified absolute binary.
//!
//! Rather than parsing the JSON API (rate-limited, more surface), we ask
//! curl to follow `/releases/latest` — which GitHub 302s to the newest
//! tag — and report only the final URL. The version is the tail of that
//! path.

use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::thread;

use anyhow::{anyhow, Context, Result};

/// `owner/repo` this build checks against.
pub const REPO: &str = "schbz/schl8";

const CURL: &str = "/usr/bin/curl";

/// The version this binary was built as (no leading `v`).
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Public URL of the changelog.
pub fn changelog_url() -> String {
    format!("https://github.com/{REPO}/blob/master/CHANGELOG.md")
}

/// Public URL of the latest release (the download page).
pub fn latest_release_url() -> String {
    format!("https://github.com/{REPO}/releases/latest")
}

/// The Homebrew upgrade command, for users who installed via the tap.
pub fn brew_command() -> &'static str {
    "brew upgrade schbz/tap/schl8"
}

/// Outcome of a completed check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// A newer release exists (its version, without the leading `v`).
    UpdateAvailable(String),
    /// This build is the newest published release (or newer than it).
    UpToDate,
}

/// Parse a `1.2.3` / `v1.2.3` version into comparable numbers. Trailing
/// pre-release text (`-rc1`) is ignored for ordering.
fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let core = v.split(['-', '+']).next()?;
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().unwrap_or("0").parse().ok()?;
    let patch = it.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// Whether `latest` is strictly newer than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// Extract the version from a release URL like
/// `https://github.com/o/r/releases/tag/v1.2.3`. Returns `None` when the
/// repo has no releases yet (GitHub redirects to `/releases`).
fn version_from_release_url(url: &str) -> Option<String> {
    let tail = url.trim_end_matches('/').rsplit('/').next()?;
    if tail == "releases" || tail.is_empty() {
        return None;
    }
    parse_version(tail)?; // reject anything that isn't a version
    Some(tail.trim_start_matches(['v', 'V']).to_string())
}

/// Ask GitHub for the newest released version. Blocking — call from
/// [`spawn_check`], not the UI thread.
fn fetch_latest_version() -> Result<String> {
    if !Path::new(CURL).exists() {
        return Err(anyhow!("{CURL} not found — cannot check for updates"));
    }
    let out = Command::new(CURL)
        .args([
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "10",
            "--max-redirs",
            "5",
            // Refuse to be downgraded off HTTPS by a redirect.
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--tlsv1.2",
            "--user-agent",
            concat!("schl8/", env!("CARGO_PKG_VERSION")),
            "--output",
            "/dev/null",
            "--write-out",
            "%{http_code} %{url_effective}",
            &latest_release_url(),
        ])
        .output()
        .context("running curl")?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!(
            "could not reach github.com ({})",
            err.trim().lines().next().unwrap_or("network error")
        ));
    }

    // "<http status> <final url>"
    let stdout = String::from_utf8_lossy(&out.stdout);
    let (code, url) = stdout
        .trim()
        .split_once(' ')
        .ok_or_else(|| anyhow!("unexpected response from github.com"))?;

    match code {
        "200" => version_from_release_url(url)
            .ok_or_else(|| anyhow!("{REPO} has no published releases yet")),
        // A private (or renamed/deleted) repo looks like a 404 to an
        // anonymous request — say so instead of blaming the network.
        "404" => Err(anyhow!(
            "no public releases for {REPO} — the repository may still be private"
        )),
        other => Err(anyhow!("github.com returned HTTP {other}")),
    }
}

/// Run the check on a background thread. The receiver yields exactly one
/// message.
pub fn spawn_check() -> mpsc::Receiver<Result<CheckOutcome, String>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = fetch_latest_version()
            .map(|latest| {
                if is_newer(&latest, current_version()) {
                    CheckOutcome::UpdateAvailable(latest)
                } else {
                    CheckOutcome::UpToDate
                }
            })
            .map_err(|e| format!("{e:#}"));
        let _ = tx.send(result);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_ordering() {
        assert!(is_newer("0.9.0", "0.8.1"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(is_newer("0.8.2", "0.8.1"));
        assert!(!is_newer("0.8.1", "0.8.1"));
        assert!(!is_newer("0.8.0", "0.8.1"));
        // Malformed input must never claim an update is available.
        assert!(!is_newer("not-a-version", "0.8.1"));
        assert!(!is_newer("", "0.8.1"));
    }

    #[test]
    fn short_and_prerelease_versions_parse() {
        assert_eq!(parse_version("v2"), Some((2, 0, 0)));
        assert_eq!(parse_version("1.4"), Some((1, 4, 0)));
        assert_eq!(parse_version("1.4.2-rc1"), Some((1, 4, 2)));
    }

    #[test]
    fn extracts_version_from_release_url() {
        assert_eq!(
            version_from_release_url("https://github.com/o/r/releases/tag/v1.2.3").as_deref(),
            Some("1.2.3")
        );
        // No releases yet → GitHub lands on the releases index.
        assert_eq!(
            version_from_release_url("https://github.com/o/r/releases"),
            None
        );
        // Junk tail is rejected rather than treated as a version.
        assert_eq!(
            version_from_release_url("https://github.com/o/r/releases/tag/nightly"),
            None
        );
    }
}
