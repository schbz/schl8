//! Backing up the app's own settings.
//!
//! Everything Schl8 knows about your setup — which notes exist, which
//! keys they encrypt to, where the copies go, your hotkeys — lives in
//! one `config.toml`. Losing it does not lose a single encrypted file,
//! but it does lose the map: which of forty `.gpg` files on a disk was
//! the journal, and which key opened it. Rebuilding that by hand is
//! miserable, so it is worth one command.
//!
//! **Why encryption is offered.** The config holds no key material and
//! no document text; that is a standing invariant. It does hold the
//! *shape* of a private life: note names, file paths, and the labels of
//! your keys, which are real names and email addresses. That is worth
//! sealing before it goes to a backup disk or a sync folder, so
//! encrypting to one of your own recipients is one radio button away.
//!
//! The bundle is an ordinary `.tar.gz`, so an encrypted backup is also
//! a vault Schl8 can already open and browse — recovery does not need a
//! restore command that might not exist by then.

use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::config::Config;

/// How a backup is protected.
#[derive(Debug, Clone, PartialEq)]
pub enum Protection {
    /// Plain `.tar.gz`. The config is already plaintext on disk, so this
    /// exposes nothing new — but it travels, and backups travel further
    /// than the file they came from.
    None,
    /// Encrypted to a GPG key (by fingerprint).
    Gpg(String),
    /// Encrypted to an `age1…` recipient.
    Age(String),
}

impl Protection {
    /// The extension the finished bundle should carry.
    pub fn extension(&self) -> &'static str {
        match self {
            Protection::None => "tar.gz",
            Protection::Gpg(_) => "tar.gz.gpg",
            Protection::Age(_) => "tar.gz.age",
        }
    }
}

/// A file to place in the bundle: archive-relative name and contents.
struct Entry {
    name: String,
    bytes: Vec<u8>,
}

/// Everything worth keeping from the config directory.
///
/// The agent guide is deliberately skipped: it is generated on demand
/// and a stale copy is worse than none. Held edits *are* included —
/// they are already encrypted to the document's own key, and a backup
/// that silently dropped unsaved work would be a trap.
fn collect() -> Result<Vec<Entry>> {
    let config = crate::config::config_path()
        .ok_or_else(|| anyhow::anyhow!("no configuration directory on this system"))?;
    let dir = config
        .parent()
        .ok_or_else(|| anyhow::anyhow!("malformed configuration path"))?;

    let mut entries = Vec::new();
    if config.exists() {
        entries.push(Entry {
            name: "config.toml".to_string(),
            bytes: std::fs::read(&config)
                .with_context(|| format!("reading {}", config.display()))?,
        });
    }

    let stash = dir.join("stash");
    if let Ok(read) = std::fs::read_dir(&stash) {
        for e in read.flatten() {
            let p = e.path();
            if p.is_file() {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("held");
                entries.push(Entry {
                    name: format!("stash/{name}"),
                    bytes: std::fs::read(&p).with_context(|| format!("reading {}", p.display()))?,
                });
            }
        }
    }

    if entries.is_empty() {
        bail!("there is nothing to back up yet — no settings have been saved");
    }
    Ok(entries)
}

/// A short note placed in the bundle, for whoever opens it later.
fn manifest(entries: &[Entry], protection: &Protection) -> Entry {
    let listed = entries
        .iter()
        .map(|e| format!("  {}", e.name))
        .collect::<Vec<_>>()
        .join("\n");
    let sealed = match protection {
        Protection::None => "This backup is NOT encrypted.".to_string(),
        Protection::Gpg(fpr) => format!("Encrypted to GPG key {fpr}."),
        Protection::Age(r) => format!("Encrypted to {r}."),
    };
    let body = format!(
        "Schl8 settings backup
=====================

Version: {}
Contains:
{listed}

{sealed}

To restore: put config.toml back at ~/.config/schl8/config.toml with
Schl8 closed, then reopen it. Any stash/ files go back beside it, in a
`stash` folder — those are unsaved edits, encrypted to the key of the
document they came from.

This backup holds no key material and no note contents. It does hold
file paths and the labels of your keys, which is why encrypting it is
offered.
",
        env!("CARGO_PKG_VERSION"),
    );
    Entry {
        name: "ABOUT-THIS-BACKUP.txt".to_string(),
        bytes: body.into_bytes(),
    }
}

/// Build the `.tar.gz` in memory.
///
/// In memory, not staged on disk: an encrypted backup that passed
/// through a plaintext temp file would leave that file behind in the
/// place a backup is least examined.
fn build_archive(entries: &[Entry]) -> Result<Vec<u8>> {
    let mut tar = tar::Builder::new(Vec::new());
    for e in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(e.bytes.len() as u64);
        header.set_mode(0o600);
        header.set_mtime(0);
        header.set_cksum();
        tar.append_data(&mut header, &e.name, e.bytes.as_slice())
            .with_context(|| format!("adding {} to the backup", e.name))?;
    }
    let tar_bytes = tar.into_inner().context("building the backup archive")?;
    crate::document::archive::compress_payload(&tar_bytes, true)
}

/// Suggested file name, e.g. `schl8-settings-2026-07-31.tar.gz.age`.
pub fn suggested_name(protection: &Protection, today: &str) -> String {
    format!("schl8-settings-{today}.{}", protection.extension())
}

/// Write a backup of the configuration directory to `dest`.
pub fn write(dest: &Path, protection: &Protection) -> Result<()> {
    let mut entries = collect()?;
    entries.push(manifest(&entries, protection));
    let archive = build_archive(&entries)?;

    let bytes = match protection {
        Protection::None => archive,
        Protection::Gpg(fpr) => crate::crypto::keys::encrypt_to_bytes(&archive, &[fpr], false)
            .context("encrypting the backup to your GPG key")?,
        Protection::Age(recipient) => {
            crate::crypto::age_backend::encrypt_to_recipients(&archive, &[recipient])
                .context("encrypting the backup to your age recipient")?
        }
    };

    crate::crypto::keys::atomic_write(dest, &bytes)
        .with_context(|| format!("writing {}", dest.display()))
}

/// Recipients a backup may be encrypted to: (label, protection).
///
/// The label is for the picker only. GPG keys are listed by uid because
/// this list never leaves the machine — unlike the agent briefing,
/// which strips them.
pub fn available_recipients(cfg: &Config) -> Vec<(String, Protection)> {
    let mut out = Vec::new();
    for r in &cfg.age_recipients {
        out.push((
            format!("{} (age)", r.label),
            Protection::Age(r.recipient.clone()),
        ));
    }
    if crate::crypto::gpg::gpg_available() {
        for k in crate::crypto::keys::list_public_keys().unwrap_or_default() {
            out.push((k.uid.clone(), Protection::Gpg(k.fingerprint.clone())));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn read_back(gz: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(gz)
            .read_to_end(&mut raw)
            .unwrap();
        let mut out = Vec::new();
        for e in tar::Archive::new(raw.as_slice()).entries().unwrap() {
            let mut e = e.unwrap();
            let name = e.path().unwrap().to_string_lossy().into_owned();
            let mut b = Vec::new();
            e.read_to_end(&mut b).unwrap();
            out.push((name, b));
        }
        out
    }

    #[test]
    fn the_archive_round_trips_with_its_manifest() {
        let entries = vec![
            Entry {
                name: "config.toml".into(),
                bytes: b"hotkey = \"ctrl+cmd+j\"\n".to_vec(),
            },
            Entry {
                name: "stash/held.age".into(),
                bytes: b"ciphertext".to_vec(),
            },
        ];
        let mut all = entries;
        all.push(manifest(&all, &Protection::None));
        let files = read_back(&build_archive(&all).unwrap());

        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"config.toml"));
        assert!(names.contains(&"stash/held.age"));
        assert!(names.contains(&"ABOUT-THIS-BACKUP.txt"));

        let config = &files.iter().find(|(n, _)| n == "config.toml").unwrap().1;
        assert_eq!(config, b"hotkey = \"ctrl+cmd+j\"\n");
    }

    /// Held edits are the one thing in the config directory that cannot
    /// be reconstructed. A backup that quietly skipped them would be
    /// worse than no backup, because it would look complete.
    #[test]
    fn held_edits_are_included() {
        let entries = vec![Entry {
            name: "stash/held.age".into(),
            bytes: b"x".to_vec(),
        }];
        let files = read_back(&build_archive(&entries).unwrap());
        assert!(files.iter().any(|(n, _)| n == "stash/held.age"));
    }

    #[test]
    fn extensions_say_how_the_file_is_protected() {
        assert_eq!(Protection::None.extension(), "tar.gz");
        assert_eq!(Protection::Gpg("F".into()).extension(), "tar.gz.gpg");
        assert_eq!(Protection::Age("age1x".into()).extension(), "tar.gz.age");
        // The double extension is what makes Schl8 open the decrypted
        // result as an archive rather than as text.
        assert!(
            suggested_name(&Protection::Age("age1x".into()), "2026-07-31").ends_with(".tar.gz.age")
        );
    }

    /// The manifest is read by someone with no context months later, so
    /// it must say what is inside and how to put it back.
    #[test]
    fn the_manifest_explains_itself() {
        let entries = vec![Entry {
            name: "config.toml".into(),
            bytes: vec![],
        }];
        let m = manifest(&entries, &Protection::None);
        let text = String::from_utf8(m.bytes).unwrap();
        assert!(text.contains("config.toml"), "lists what it contains");
        assert!(
            text.contains("~/.config/schl8/config.toml"),
            "says where it goes"
        );
        assert!(text.contains("NOT encrypted"), "states its protection");
        assert!(
            text.contains("no key material"),
            "reassures about what it does not hold"
        );
    }

    #[test]
    fn an_encrypted_manifest_names_the_recipient() {
        let entries = vec![Entry {
            name: "config.toml".into(),
            bytes: vec![],
        }];
        let m = manifest(&entries, &Protection::Age("age1abc".into()));
        assert!(String::from_utf8(m.bytes).unwrap().contains("age1abc"));
    }
}
