use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

use anyhow::Result;
use thiserror::Error;

use super::secure_buf::SecureBuffer;

/// Absolute paths we look for gpg at, in order. Deliberately does NOT
/// consult `$PATH`: resolving `gpg` through the inherited PATH is both a
/// binary-planting risk (a writable dir earlier in PATH wins) and
/// unreliable for GUI launches from Finder, which get a minimal PATH
/// that excludes Homebrew (`/opt/homebrew/bin`).
const GPG_CANDIDATES: &[&str] = &[
    "/opt/homebrew/bin/gpg", // Apple-silicon Homebrew
    "/usr/local/bin/gpg",    // Intel Homebrew / manual installs
    "/opt/local/bin/gpg",    // MacPorts
    "/usr/bin/gpg",          // system
];

/// Resolve the gpg binary to a single absolute path, verified to exist
/// and be executable. Overridable with the `SCHL8_GPG` environment
/// variable (an explicit, trusted opt-in). Cached after first success.
pub fn gpg_bin() -> Result<&'static Path> {
    static RESOLVED: OnceLock<Option<PathBuf>> = OnceLock::new();

    let slot = RESOLVED.get_or_init(|| {
        if let Some(override_path) = std::env::var_os("SCHL8_GPG") {
            let p = PathBuf::from(override_path);
            if is_executable(&p) {
                return Some(p);
            }
        }
        GPG_CANDIDATES
            .iter()
            .map(PathBuf::from)
            .find(|p| is_executable(p))
    });

    slot.as_deref().ok_or_else(|| GpgError::NotFound.into())
}

/// True if `path` is a regular file with an execute bit set.
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Start a `gpg` Command at the resolved absolute path.
pub fn gpg_command() -> Result<Command> {
    Ok(Command::new(gpg_bin()?))
}

/// Whether a usable `gpg` binary was found. When false, Schl8 runs in
/// age-only mode: GPG encrypt/decrypt and key management are unavailable,
/// but the seed-phrase age backend works fully.
///
/// Setting `SCHL8_FORCE_NO_GPG=1` forces age-only mode even when gpg is
/// installed — useful for testing the age-only UX on a machine that has
/// gpg.
pub fn gpg_available() -> bool {
    if std::env::var_os("SCHL8_FORCE_NO_GPG").is_some() {
        return false;
    }
    gpg_bin().is_ok()
}

#[derive(Error, Debug)]
pub enum GpgError {
    #[error("gpg binary not found — is GnuPG installed? (brew install gnupg). Set SCHL8_GPG to override.")]
    NotFound,

    #[error("no secret key available — is your YubiKey inserted?")]
    NoSecretKey,

    #[error("smartcard error — check your YubiKey connection")]
    CardError,

    #[error("bad passphrase or PIN")]
    BadPassphrase,

    #[error("decryption failed — file may not be encrypted to your key")]
    DecryptionFailed,

    #[error("gpg error: {0}")]
    Other(String),
}

/// Authenticity of a decrypted file, from any OpenPGP signature it carries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SignatureStatus {
    /// The file carried no signature (confidentiality only).
    #[default]
    Unsigned,
    /// A good signature from a known key; `signer` is the UID text.
    Valid { signer: String },
    /// A signature was present but did not verify (bad, expired, or made
    /// by a key not in the keyring). `reason` is a short label.
    Invalid { reason: String },
}

/// A decrypted document plus the authenticity of any signature on it.
pub struct Decrypted {
    pub content: SecureBuffer,
    pub signature: SignatureStatus,
}

/// Decrypt a GPG-encrypted file. Plaintext is returned in a SecureBuffer
/// (mlock'd, zeroize-on-drop) via a subprocess pipe — it never touches
/// disk. See `decrypt_file_verified` for the signature-aware variant.
pub fn decrypt_file(path: &Path) -> Result<SecureBuffer> {
    Ok(decrypt_file_verified(path)?.content)
}

/// Decrypt a file and report the authenticity of any signature it carries.
/// gpg's machine-readable status is captured on fd 2 (merged with stderr)
/// and parsed for GOODSIG/VALIDSIG/BADSIG/ERRSIG/EXPSIG.
pub fn decrypt_file_verified(path: &Path) -> Result<Decrypted> {
    let output = gpg_command()?
        .args(["--decrypt", "--quiet", "--yes", "--status-fd", "2"])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GpgError::NotFound.into()
            } else {
                anyhow::anyhow!("failed to execute gpg: {e}")
            }
        })?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        return Err(map_gpg_error(&stderr).context("gpg decryption failed"));
    }

    Ok(Decrypted {
        content: SecureBuffer::from_bytes(output.stdout),
        signature: parse_signature_status(&stderr),
    })
}

/// Parse gpg `--status-fd` lines (prefixed `[GNUPG:]`) into a
/// `SignatureStatus`. Absence of any signature line means Unsigned.
fn parse_signature_status(status: &str) -> SignatureStatus {
    let mut good_signer: Option<String> = None;
    let mut validated = false;
    let mut invalid: Option<String> = None;

    for line in status.lines() {
        let Some(rest) = line.strip_prefix("[GNUPG:] ") else {
            continue;
        };
        let mut parts = rest.splitn(2, ' ');
        let tag = parts.next().unwrap_or("");
        let args = parts.next().unwrap_or("");
        match tag {
            // GOODSIG <keyid> <username...>
            "GOODSIG" => {
                let signer = args.split_once(' ').map(|x| x.1).unwrap_or("").trim();
                good_signer = Some(if signer.is_empty() {
                    "unknown signer".to_string()
                } else {
                    signer.to_string()
                });
            }
            "VALIDSIG" => validated = true,
            "BADSIG" => invalid = Some("bad signature".to_string()),
            "ERRSIG" => invalid = Some("unverifiable (signer key missing)".to_string()),
            "EXPSIG" => invalid = Some("expired signature".to_string()),
            "EXPKEYSIG" => invalid = Some("signed with an expired key".to_string()),
            "REVKEYSIG" => invalid = Some("signed with a revoked key".to_string()),
            _ => {}
        }
    }

    if let Some(reason) = invalid {
        SignatureStatus::Invalid { reason }
    } else if let Some(signer) = good_signer {
        if validated {
            SignatureStatus::Valid { signer }
        } else {
            SignatureStatus::Invalid {
                reason: "unvalidated signature".to_string(),
            }
        }
    } else {
        SignatureStatus::Unsigned
    }
}

/// List the key IDs a file is encrypted to by parsing the ciphertext
/// packet headers. Works on the encrypted bytes only — the plaintext is
/// never needed and no PIN is requested.
pub fn list_recipients(path: &Path) -> Result<Vec<String>> {
    // Fail with a clear message (rather than a cryptic gpg error) when the
    // target is missing or unreadable.
    if !path.exists() {
        anyhow::bail!("file not found: {}", path.display());
    }

    let output = gpg_command()?
        .args(["--batch", "--list-only", "--list-packets"])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GpgError::NotFound.into()
            } else {
                anyhow::anyhow!("failed to execute gpg: {e}")
            }
        })?;

    if !output.status.success() {
        // Surface gpg's actual reason — this usually points at a broken gpg
        // environment (e.g. a stale GNUPGHOME) rather than the file itself.
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim();
        if detail.is_empty() {
            anyhow::bail!(
                "gpg could not read {} (exit {})",
                path.display(),
                output.status.code().unwrap_or(-1)
            );
        }
        anyhow::bail!("gpg could not read {}: {detail}", path.display());
    }

    parse_recipient_keyids(&String::from_utf8_lossy(&output.stdout))
}

/// Extract recipient key IDs from `gpg --list-packets` output lines like
/// `:pubkey enc packet: version 3, algo 1, keyid 1B9B7C008E278EBF`.
fn parse_recipient_keyids(listing: &str) -> Result<Vec<String>> {
    let mut ids = Vec::new();
    for line in listing.lines() {
        if !line.starts_with(":pubkey enc packet:") {
            continue;
        }
        let Some(pos) = line.find("keyid ") else {
            continue;
        };
        let id = line[pos + 6..]
            .split(|c: char| !c.is_ascii_hexdigit())
            .next()
            .unwrap_or("");
        if id.is_empty() {
            continue;
        }
        if id.chars().all(|c| c == '0') {
            // Anonymous ("throw-keyid") recipient — we can't know who to
            // re-encrypt to.
            anyhow::bail!(
                "file uses an anonymous recipient; re-encrypt it manually via Encrypt & Save As"
            );
        }
        ids.push(id.to_string());
    }

    if ids.is_empty() {
        anyhow::bail!("no public-key recipients found in file");
    }
    Ok(ids)
}

/// Map gpg stderr output to a specific error type.
fn map_gpg_error(stderr: &str) -> anyhow::Error {
    let stderr_lower = stderr.to_lowercase();

    if stderr_lower.contains("no secret key") {
        GpgError::NoSecretKey.into()
    } else if stderr_lower.contains("card error") || stderr_lower.contains("card removed") {
        GpgError::CardError.into()
    } else if stderr_lower.contains("bad passphrase") || stderr_lower.contains("bad pin") {
        GpgError::BadPassphrase.into()
    } else if stderr_lower.contains("decryption failed") {
        GpgError::DecryptionFailed.into()
    } else {
        GpgError::Other(stderr.trim().to_string()).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_executable_detects_exec_bit() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir();
        let exe = dir.join(format!("schl8-test-exe-{}", std::process::id()));
        let plain = dir.join(format!("schl8-test-plain-{}", std::process::id()));
        std::fs::write(&exe, b"#!/bin/sh\n").unwrap();
        std::fs::write(&plain, b"data").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o644)).unwrap();

        assert!(is_executable(&exe));
        assert!(!is_executable(&plain));
        assert!(!is_executable(Path::new("/nonexistent/schl8/gpg")));
        assert!(!is_executable(&dir)); // a directory is not a file

        let _ = std::fs::remove_file(&exe);
        let _ = std::fs::remove_file(&plain);
    }

    #[test]
    fn parses_recipient_keyids() {
        let listing = "\
# off=0 ctb=85 tag=1 hlen=3 plen=524
:pubkey enc packet: version 3, algo 1, keyid 1B9B7C008E278EBF
\tdata: [4095 bits]
# off=527 ctb=85 tag=1 hlen=3 plen=524
:pubkey enc packet: version 3, algo 1, keyid AABBCCDDEEFF0011
:aead encrypted packet: cipher=9 aead=2 cb=16
";
        let ids = parse_recipient_keyids(listing).unwrap();
        assert_eq!(ids, vec!["1B9B7C008E278EBF", "AABBCCDDEEFF0011"]);
    }

    #[test]
    fn rejects_anonymous_recipient() {
        let listing = ":pubkey enc packet: version 3, algo 1, keyid 0000000000000000\n";
        assert!(parse_recipient_keyids(listing).is_err());
    }

    #[test]
    fn rejects_no_recipients() {
        assert!(parse_recipient_keyids(":aead encrypted packet: cipher=9\n").is_err());
    }

    #[test]
    fn signature_status_unsigned_when_no_sig_lines() {
        let status = "[GNUPG:] DECRYPTION_OKAY\n[GNUPG:] GOODMDC\n";
        assert_eq!(parse_signature_status(status), SignatureStatus::Unsigned);
    }

    #[test]
    fn signature_status_valid_needs_good_and_valid() {
        let status = "\
[GNUPG:] GOODSIG 669318E9BF4EEF0A Sample Signer <sample@example.com>
[GNUPG:] VALIDSIG 322CE4E00C07EB997C79C6FE669318E9BF4EEF0A 2026-07-05
[GNUPG:] DECRYPTION_OKAY
";
        assert_eq!(
            parse_signature_status(status),
            SignatureStatus::Valid {
                signer: "Sample Signer <sample@example.com>".to_string()
            }
        );
    }

    #[test]
    fn signature_status_good_without_valid_is_invalid() {
        let status = "[GNUPG:] GOODSIG ABCD Someone <a@b.c>\n";
        assert!(matches!(
            parse_signature_status(status),
            SignatureStatus::Invalid { .. }
        ));
    }

    #[test]
    fn signature_status_bad_and_errsig() {
        assert!(matches!(
            parse_signature_status("[GNUPG:] BADSIG ABCD Someone\n"),
            SignatureStatus::Invalid { .. }
        ));
        assert!(matches!(
            parse_signature_status("[GNUPG:] ERRSIG ABCD 22 8 00 timestamp 9\n"),
            SignatureStatus::Invalid { .. }
        ));
    }
}
