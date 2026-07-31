//! Execute a per-file save plan: encrypt the document once per rule's key
//! and write the ciphertext to every destination of that rule, overwriting
//! existing files. Each write is atomic (temp + rename, owner-only) via
//! `keys::atomic_write`; only ciphertext ever touches disk.

use std::path::{Path, PathBuf};

use crate::config::SavePlan;
use crate::crypto::keys;

/// Whether a destination should be ASCII-armored, by extension.
pub fn armor_for(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("asc")
}

/// Outcome of one destination write.
pub struct TargetResult {
    pub destination: PathBuf,
    pub result: anyhow::Result<()>,
}

/// Save `plaintext` per the plan. Failures on one target never stop the
/// others; the caller summarizes the results. Encryption runs at most
/// twice per rule (binary and/or armored ciphertext, shared across that
/// rule's destinations).
pub fn execute(plaintext: &[u8], plan: &SavePlan) -> Vec<TargetResult> {
    let mut results = Vec::new();

    for rule in &plan.rules {
        // Defense in depth: the plan editors default or validate keys, but
        // a hand-edited config could still carry a keyless rule — fail it
        // clearly instead of passing an empty recipient to gpg.
        if !rule.has_key() {
            for dest in &rule.destinations {
                results.push(TargetResult {
                    destination: dest.clone(),
                    result: Err(anyhow::anyhow!(
                        "no encryption key configured for this destination"
                    )),
                });
            }
            continue;
        }

        // age rules encrypt once (binary only) to the recipient and share
        // that ciphertext across all destinations.
        if rule.is_age() {
            let ciphertext = crate::crypto::age_backend::encrypt_to_recipients(
                plaintext,
                &[rule.age_recipient.as_str()],
            );
            for dest in &rule.destinations {
                let result = match &ciphertext {
                    Ok(ct) => keys::atomic_write(dest, ct),
                    Err(e) => Err(anyhow::anyhow!(
                        "age encryption to {} failed: {e}",
                        rule.key_label
                    )),
                };
                results.push(TargetResult {
                    destination: dest.clone(),
                    result,
                });
            }
            continue;
        }

        let recipients = [rule.key_fingerprint.as_str()];

        // Encrypt lazily per armor variant, shared across destinations.
        let mut binary: Option<anyhow::Result<Vec<u8>>> = None;
        let mut armored: Option<anyhow::Result<Vec<u8>>> = None;

        for dest in &rule.destinations {
            let armor = armor_for(dest);
            let slot = if armor { &mut armored } else { &mut binary };
            if slot.is_none() {
                *slot = Some(keys::encrypt_to_bytes(plaintext, &recipients, armor));
            }

            let result = match slot.as_ref().expect("just filled") {
                Ok(ciphertext) => keys::atomic_write(dest, ciphertext),
                Err(e) => Err(anyhow::anyhow!(
                    "encryption to {} failed: {e}",
                    rule.key_label
                )),
            };
            results.push(TargetResult {
                destination: dest.clone(),
                result,
            });
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn armor_by_extension() {
        assert!(armor_for(Path::new("/a/notes.md.asc")));
        assert!(!armor_for(Path::new("/a/notes.md.gpg")));
        assert!(!armor_for(Path::new("/a/notes")));
    }
}
