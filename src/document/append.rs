//! Append a quick-note blurb to an encrypted text file.
//!
//! Flow: read the file's recipient key IDs from the ciphertext (fails
//! fast, no PIN), decrypt into a SecureBuffer (pinentry may prompt),
//! append the rendered blurb, re-encrypt to the *original* recipients,
//! and atomically overwrite the source. The combined plaintext lives in
//! a transient buffer that is zeroized before returning; nothing
//! unencrypted touches disk.

use std::path::Path;

use anyhow::{Context, Result};

use super::archive;
use crate::crypto::age_backend::AgeIdentity;
use crate::crypto::secure_buf::SecureString;
use crate::crypto::{gpg, keys};

/// Append `blurb` (already template-rendered) to the encrypted file at
/// `path`.
///
/// With empty `rules`, re-encrypts to the file's own recipients and
/// overwrites it in place. With rules (the quicknote-registry flow), the
/// combined content is instead encrypted to each rule's key and written
/// to all of that rule's destinations; the source file should be among
/// them — the registry normalizer guarantees it — so subsequent appends
/// read the fresh copy.
///
/// The combined (existing + appended) plaintext is assembled in a
/// `SecureString` — mlock'd and zeroized on drop — so the full document
/// never lives in ordinary swappable heap memory.
pub fn append_blurb_with_rules(
    path: &Path,
    blurb: &str,
    rules: &[crate::config::SaveRule],
    age_identity: Option<&AgeIdentity>,
) -> Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if archive::is_archive_name(name) {
        anyhow::bail!("cannot append to a folder archive");
    }

    // Detect age by extension or the age header magic, so the flow works
    // even when an age file carries a `.gpg`/`.md` name.
    let is_age = path.exists() && super::loader::is_age_file(path);

    // Fail fast (and PIN-free) if we won't be able to re-encrypt.
    // Resolves the file's stored key IDs to primary-key fingerprints.
    // With explicit rules the keys come from the rules instead, so this
    // check is skipped (a missing rule key fails in the encrypt step).
    // age files carry no recipient in the ciphertext, so an empty-rules
    // append re-encrypts to the identity's own recipient.
    let recipients = if rules.is_empty() && !is_age {
        keys::recipients_for_reencrypt(path)
            .with_context(|| format!("cannot determine recipients of {}", path.display()))?
    } else {
        Vec::new()
    };

    let content = if is_age {
        let identity = age_identity
            .context("this quicknote is AGE-encrypted — unlock your AGE identity first")?;
        let ciphertext =
            std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
        identity
            .decrypt(&ciphertext)
            .with_context(|| format!("failed to decrypt {}", path.display()))?
    } else {
        gpg::decrypt_file(path).with_context(|| format!("failed to decrypt {}", path.display()))?
    };

    // Assemble the new content in secure (mlock'd, zeroize-on-drop) memory.
    let mut combined =
        SecureString::from_secure_buffer(&content).context("existing content is not valid text")?;
    let needs_newline = {
        let s = combined.as_str();
        !s.is_empty() && !s.ends_with('\n')
    };
    if needs_newline {
        combined.push_str("\n");
    }
    combined.push_str(blurb);

    if rules.is_empty() && is_age {
        // No explicit rules: re-encrypt an age file to its own identity.
        let identity = age_identity
            .context("this quicknote is AGE-encrypted — unlock your AGE identity first")?;
        let ciphertext = crate::crypto::age_backend::encrypt_to_recipients(
            combined.as_bytes(),
            &[identity.recipient()],
        )?;
        keys::atomic_write(path, &ciphertext)
    } else if rules.is_empty() {
        let armor = name.ends_with(".asc");
        let recips: Vec<&str> = recipients.iter().map(String::as_str).collect();
        keys::encrypt_overwrite(combined.as_bytes(), &recips, path, armor)
    } else {
        let plan = crate::config::SavePlan {
            source: path.to_path_buf(),
            rules: rules.to_vec(),
            ..Default::default()
        };
        let results = super::multisave::execute(combined.as_bytes(), &plan);
        let failures: Vec<String> = results
            .iter()
            .filter_map(|r| {
                r.result
                    .as_ref()
                    .err()
                    .map(|e| format!("{}: {e:#}", r.destination.display()))
            })
            .collect();
        if failures.is_empty() {
            Ok(())
        } else {
            anyhow::bail!(
                "{} of {} destinations failed — {}",
                failures.len(),
                results.len(),
                failures.join("; ")
            )
        }
    }
    // `combined` and `content` are zeroized as they drop here.
}

#[cfg(test)]
mod tests {
    use super::*;

    // A valid BIP-39 test vector (see docs/AGE-DESIGN.md §3.1).
    const MNEMONIC: &str =
        "legal winner thank year wave sausage worth useful legal winner thank yellow";

    #[test]
    fn age_quicknote_create_then_append_roundtrips_without_gpg() {
        let id = AgeIdentity::from_mnemonic(MNEMONIC, "").unwrap();
        let dir = std::env::temp_dir().join(format!("schl8-age-append-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let note = dir.join("journal.md.age");

        let rules = vec![crate::config::SaveRule {
            key_fingerprint: String::new(),
            key_label: "my age key".to_string(),
            age_recipient: id.recipient().to_string(),
            destinations: vec![note.clone()],
        }];

        // First write (file doesn't exist yet): encrypt the starter.
        crate::document::multisave::execute(
            b"# Journal\n",
            &crate::config::SavePlan {
                source: note.clone(),
                rules: rules.clone(),
                ..Default::default()
            },
        );
        assert!(super::super::loader::is_age_file(&note), "written as age");

        // Append: decrypts with the identity, re-encrypts, overwrites.
        append_blurb_with_rules(&note, "\nfirst entry\n", &rules, Some(&id)).unwrap();
        append_blurb_with_rules(&note, "second entry\n", &rules, Some(&id)).unwrap();

        let ct = std::fs::read(&note).unwrap();
        let plain = id.decrypt(&ct).unwrap();
        let text = plain.as_str().unwrap();
        assert!(text.contains("# Journal"), "starter preserved");
        assert!(text.contains("first entry"), "first append preserved");
        assert!(text.contains("second entry"), "second append present");

        // A locked identity (None) must refuse, not panic or corrupt.
        assert!(append_blurb_with_rules(&note, "x\n", &rules, None).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
