use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};

use super::gpg;

/// A GPG public key from the user's keyring.
#[derive(Clone)]
#[allow(dead_code)]
pub struct PublicKey {
    /// Long key ID (16 hex chars).
    pub key_id: String,
    /// User ID string (e.g., "Alice <alice@example.com>").
    pub uid: String,
    /// Key fingerprint (40 hex chars).
    pub fingerprint: String,
    /// Whether the key is valid (not expired/revoked).
    pub valid: bool,
}

/// List all public keys in the GPG keyring.
pub fn list_public_keys() -> Result<Vec<PublicKey>> {
    let output = gpg::gpg_command()?
        .args([
            "--list-keys",
            "--with-colons",
            "--fixed-list-mode",
            "--batch",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run gpg --list-keys")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Empty keyring is not an error
        if stderr.contains("trustdb") || stderr.contains("no ultimately trusted keys") {
            // gpg may print warnings even on success
        } else if !stderr.trim().is_empty() {
            anyhow::bail!("gpg --list-keys failed: {}", stderr.trim());
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_colon_listing(&stdout))
}

/// Import a public key from a file (.asc or .gpg).
pub fn import_key(path: &Path) -> Result<String> {
    let output = gpg::gpg_command()?
        .args(["--import", "--batch"])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run gpg --import")?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        anyhow::bail!("key import failed: {}", stderr.trim());
    }

    Ok(stderr.trim().to_string())
}

/// Import a public key from raw text (armored ASCII block).
#[allow(dead_code)]
pub fn import_key_from_text(armored_text: &str) -> Result<String> {
    use std::io::Write;

    let mut child = gpg::gpg_command()?
        .args(["--import", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run gpg --import")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(armored_text.as_bytes())
            .context("failed to write key data to gpg")?;
    }

    let output = child.wait_with_output().context("gpg --import failed")?;
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        anyhow::bail!("key import failed: {}", stderr.trim());
    }

    Ok(stderr.trim().to_string())
}

/// Delete a public key from the keyring by fingerprint.
pub fn delete_key(fingerprint: &str) -> Result<()> {
    let output = gpg::gpg_command()?
        .args(["--batch", "--yes", "--delete-keys", fingerprint])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run gpg --delete-keys")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("failed to delete key: {}", stderr.trim());
    }

    Ok(())
}

/// Encrypt data to one or more recipients, returning the ciphertext.
/// gpg writes to stdout (no plaintext or ciphertext temp file of its own),
/// so the caller controls exactly how the result reaches disk.
/// Ciphertext is not sensitive, so a plain `Vec<u8>` is fine.
pub fn encrypt_to_bytes(plaintext: &[u8], recipients: &[&str], armor: bool) -> Result<Vec<u8>> {
    use std::io::Write;

    if recipients.is_empty() {
        anyhow::bail!("at least one recipient is required");
    }

    let mut cmd = gpg::gpg_command()?;
    cmd.args(["--encrypt", "--batch", "--yes", "--trust-model", "always"]);
    if armor {
        cmd.arg("--armor");
    }
    for recipient in recipients {
        cmd.arg("--recipient").arg(recipient);
    }

    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run gpg --encrypt")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(plaintext)
            .context("failed to write plaintext to gpg")?;
    }

    let output = child.wait_with_output().context("gpg --encrypt failed")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("encryption failed: {}", stderr.trim());
    }

    Ok(output.stdout)
}

/// Encrypt data to one or more recipients and write to the given output path.
/// When `armor` is true, produces ASCII-armored output (.asc); otherwise binary (.gpg).
/// Used for Save As (a fresh, user-chosen path).
pub fn encrypt_to_file(
    plaintext: &[u8],
    recipients: &[&str],
    output_path: &Path,
    armor: bool,
) -> Result<()> {
    let ciphertext = encrypt_to_bytes(plaintext, recipients, armor)?;
    atomic_write(output_path, &ciphertext)
        .with_context(|| format!("failed to write {}", output_path.display()))
}

/// Encrypt and atomically replace `path`, re-encrypting to the given
/// recipients. The previous version stays intact on any failure.
pub fn encrypt_overwrite(
    plaintext: &[u8],
    recipients: &[&str],
    path: &Path,
    armor: bool,
) -> Result<()> {
    let ciphertext = encrypt_to_bytes(plaintext, recipients, armor)?;
    atomic_write(path, &ciphertext).with_context(|| format!("failed to replace {}", path.display()))
}

/// Serializes concurrent `atomic_write` calls so two quick-note appends
/// (e.g. from a mashed global hotkey) can't race on the temp file. A
/// single global lock is sufficient — writes are brief and infrequent.
static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Durably and atomically write `bytes` to `path`:
/// write to a unique temp file in the same directory at mode 0600
/// (preserving an existing file's mode), fsync it, rename over the target,
/// then fsync the directory. A crash at any point leaves either the old
/// file or the fully-written new one — never a partial or world-readable
/// intermediate. Callers pass ciphertext only.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .context("invalid target filename")?;

    // Unique temp name so concurrent or stale temps never collide, and
    // O_EXCL so a pre-seeded symlink can't redirect our write.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = dir.join(format!(
        ".{file_name}.schl8-{}-{seq}.tmp",
        std::process::id()
    ));

    // Preserve an existing file's mode; default to owner-only.
    let mode = std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o777)
        .unwrap_or(0o600);

    let write_result = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true) // O_EXCL
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.flush()?;
        f.sync_all()?;
        // Apply the preserved mode (create used 0600).
        f.set_permissions(std::fs::Permissions::from_mode(mode))?;
        Ok(())
    })();

    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp);
        return Err(anyhow::anyhow!("failed to write temp file: {e}"));
    }

    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        anyhow::anyhow!("rename failed: {e}")
    })?;

    // Flush the directory entry so the rename itself is durable.
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }

    Ok(())
}

/// Determine the recipients to re-encrypt a file to: read the key IDs
/// stored in its ciphertext, then resolve each to the full fingerprint of
/// the *primary* key that owns it (via the keyring). Resolving away from
/// the 64-bit key ID avoids short-key-ID collisions, and mapping a
/// subkey ID up to its primary means re-encryption follows the identity
/// (and its current encryption subkey), not a specific — possibly
/// rotated — subkey.
pub fn recipients_for_reencrypt(path: &Path) -> Result<Vec<String>> {
    let key_ids = gpg::list_recipients(path)?;
    let listing = list_keys_colon()?;
    let map = build_keyid_fingerprint_map(&listing);

    let mut fingerprints = Vec::new();
    for id in &key_ids {
        let key = id.to_ascii_uppercase();
        match map.get(&key) {
            Some(fpr) if !fingerprints.contains(fpr) => fingerprints.push(fpr.clone()),
            Some(_) => {} // duplicate recipient, already have it
            None => anyhow::bail!(
                "the key this file is encrypted to (ID {id}) is not in your keyring; \
                 use Encrypt & Save As to choose recipients"
            ),
        }
    }

    if fingerprints.is_empty() {
        anyhow::bail!("could not resolve any recipients for {}", path.display());
    }
    Ok(fingerprints)
}

/// Raw `gpg --list-keys --with-colons` output.
fn list_keys_colon() -> Result<String> {
    let output = gpg::gpg_command()?
        .args(["--list-keys", "--with-colons", "--batch"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run gpg --list-keys")?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Map every key ID in the keyring — primary keys *and* subkeys — to the
/// fingerprint of the primary key that owns it. Keys are uppercase hex.
fn build_keyid_fingerprint_map(colon_listing: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut pending_primary_keyid: Option<String> = None;
    let mut current_primary_fpr: Option<String> = None;

    for line in colon_listing.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        match fields.first().copied() {
            Some("pub") => {
                pending_primary_keyid = fields
                    .get(4)
                    .map(|s| s.to_ascii_uppercase())
                    .filter(|s| !s.is_empty());
                current_primary_fpr = None;
            }
            Some("fpr") => {
                // The first fpr after a pub line is the primary fingerprint.
                if current_primary_fpr.is_none() {
                    if let Some(fpr) = fields.get(9).filter(|s| !s.is_empty()) {
                        let fpr = fpr.to_ascii_uppercase();
                        current_primary_fpr = Some(fpr.clone());
                        if let Some(keyid) = pending_primary_keyid.take() {
                            map.insert(keyid, fpr);
                        }
                    }
                }
            }
            Some("sub") => {
                if let (Some(subid), Some(fpr)) = (
                    fields.get(4).filter(|s| !s.is_empty()),
                    current_primary_fpr.as_ref(),
                ) {
                    map.insert(subid.to_ascii_uppercase(), fpr.clone());
                }
            }
            _ => {}
        }
    }

    map
}

/// Parse gpg --with-colons output into PublicKey structs.
fn parse_colon_listing(output: &str) -> Vec<PublicKey> {
    let mut keys = Vec::new();
    let mut current_key_id = String::new();
    let mut current_fingerprint = String::new();
    let mut current_valid = false;
    let mut in_pub_block = false;

    for line in output.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.is_empty() {
            continue;
        }

        match fields[0] {
            "pub" => {
                in_pub_block = true;
                current_key_id = fields.get(4).unwrap_or(&"").to_string();
                current_fingerprint.clear();
                // Validity: 'u' = ultimate, 'f' = full, 'm' = marginal, '-' = unknown
                // Expired = 'e', revoked = 'r', disabled = 'd'
                let validity = fields.get(1).unwrap_or(&"");
                current_valid = !matches!(*validity, "e" | "r" | "d" | "i");
            }
            "fpr" if in_pub_block => {
                if current_fingerprint.is_empty() {
                    current_fingerprint = fields.get(9).unwrap_or(&"").to_string();
                }
            }
            "uid" if in_pub_block => {
                let uid = fields.get(9).unwrap_or(&"").to_string();
                if !uid.is_empty() && !current_key_id.is_empty() {
                    keys.push(PublicKey {
                        key_id: current_key_id.clone(),
                        uid,
                        fingerprint: current_fingerprint.clone(),
                        valid: current_valid,
                    });
                }
            }
            // New pub or sub block ends the current uid collection
            "sub" => {
                in_pub_block = false;
            }
            _ => {}
        }
    }

    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn atomic_write_creates_owner_only_file() {
        let dir = std::env::temp_dir().join(format!("schl8-aw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("note.md.gpg");

        atomic_write(&target, b"ciphertext-v1").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"ciphertext-v1");
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "new file must be owner-only");

        // No temp files left behind.
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("schl8-"))
            .collect();
        assert!(leftovers.is_empty(), "temp files must be cleaned up");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn atomic_write_replaces_and_preserves_mode() {
        let dir = std::env::temp_dir().join(format!("schl8-aw2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("note.md.gpg");

        std::fs::write(&target, b"old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();

        atomic_write(&target, b"new-ciphertext").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new-ciphertext");
        // Existing mode is preserved across the replace.
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    const SAMPLE_LISTING: &str = "\
tru::1:1700000000:0:3:1:5
pub:u:255:22:AABBCCDDEEFF0011:1700000000:::u:::scESC:::::ed25519:::0:
fpr:::::::::0123456789ABCDEF0123456789ABCDEF01234567:
uid:u::::1700000000::HASH::Alice Example <alice@example.com>::::::::::0:
sub:u:255:18:1122334455667788:1700000000::::::e:::::cv25519::
fpr:::::::::89ABCDEF0123456789ABCDEF0123456789ABCDEF:
pub:e:255:22:FFEEDDCCBBAA9988:1600000000:1650000000::-:::sc:::::ed25519:::0:
fpr:::::::::FEDCBA9876543210FEDCBA9876543210FEDCBA98:
uid:e::::1600000000::HASH2::Bob Expired <bob@example.com>::::::::::0:
";

    #[test]
    fn parses_valid_and_expired_keys() {
        let keys = parse_colon_listing(SAMPLE_LISTING);
        assert_eq!(keys.len(), 2);

        assert_eq!(keys[0].key_id, "AABBCCDDEEFF0011");
        assert_eq!(keys[0].uid, "Alice Example <alice@example.com>");
        assert_eq!(
            keys[0].fingerprint,
            "0123456789ABCDEF0123456789ABCDEF01234567"
        );
        assert!(keys[0].valid);

        assert_eq!(keys[1].uid, "Bob Expired <bob@example.com>");
        assert!(!keys[1].valid);
    }

    #[test]
    fn subkey_fingerprint_does_not_replace_primary() {
        let keys = parse_colon_listing(SAMPLE_LISTING);
        // The sub/fpr lines after the primary must not overwrite the
        // primary key's fingerprint.
        assert_ne!(
            keys[0].fingerprint,
            "89ABCDEF0123456789ABCDEF0123456789ABCDEF"
        );
    }

    #[test]
    fn empty_listing_yields_no_keys() {
        assert!(parse_colon_listing("").is_empty());
        assert!(parse_colon_listing("tru::1:1700000000:0:3:1:5\n").is_empty());
    }

    // Mirrors a real `gpg --list-keys --with-colons` block: a primary key
    // with its fingerprint, then an encryption subkey with a *different*
    // key ID and its own fingerprint. Files are encrypted to the subkey.
    const KEYRING_LISTING: &str = "\
tru::1:1700000000:0:3:1:5
pub:u:4096:1:AAAABBBBCCCCDDDD:1600000000::::::scESC::::::23::0:
fpr:::::::::0011AA22BB33CC44DD990011AAAABBBBCCCCDDDD:
uid:u::::1600000000::HASH::Bob <bob@example.com>::::::::::0:
sub:u:4096:1:11AA22BB33CC44DD:1600000000::::::e::::::23:
fpr:::::::::AABBCCDDEEFF00112233AABB11AA22BB33CC44DD:
pub:u:255:22:AABBCCDDEEFF0011:1700000000:::u:::scESC:::::ed25519:::0:
fpr:::::::::0123456789ABCDEF0123456789ABCDEF01234567:
uid:u::::1700000000::HASH2::Alice <alice@example.com>::::::::::0:
";

    #[test]
    fn maps_primary_and_subkey_ids_to_primary_fingerprint() {
        let map = build_keyid_fingerprint_map(KEYRING_LISTING);

        // The encryption subkey ID (what a file is actually encrypted to)
        // resolves to the PRIMARY fingerprint, not the subkey's.
        assert_eq!(
            map.get("11AA22BB33CC44DD").map(String::as_str),
            Some("0011AA22BB33CC44DD990011AAAABBBBCCCCDDDD")
        );
        // The primary key ID resolves to its own fingerprint.
        assert_eq!(
            map.get("AAAABBBBCCCCDDDD").map(String::as_str),
            Some("0011AA22BB33CC44DD990011AAAABBBBCCCCDDDD")
        );
        // A second, unrelated primary key.
        assert_eq!(
            map.get("AABBCCDDEEFF0011").map(String::as_str),
            Some("0123456789ABCDEF0123456789ABCDEF01234567")
        );
    }

    #[test]
    fn keyid_lookup_is_case_insensitive() {
        let map = build_keyid_fingerprint_map(KEYRING_LISTING);
        // Recipient IDs from packets are uppercase; the map is keyed
        // uppercase, so a lowercased query must be uppercased by callers —
        // verify the stored keys are uppercase.
        assert!(map.contains_key("11AA22BB33CC44DD"));
        assert!(!map.contains_key("11aa22bb33cc44dd"));
    }
}
