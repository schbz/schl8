//! Held edits: unsaved work encrypted to disk when the session locks.
//!
//! Locking used to be *deferred* whenever there were unsaved edits, so
//! typed text was never silently destroyed — but that meant a document
//! left mid-edit kept the session unlocked indefinitely, which is the
//! opposite of what a lock is for. The stash resolves that: on lock the
//! unsaved text is encrypted to the document's **own public key** and
//! written beside the config, then the plaintext is dropped and the
//! session really locks. Nothing is lost, and nothing stays readable.
//!
//! Encrypting needs only a public key, exactly like `spool.rs`, so
//! stashing never prompts for a PIN or a seed phrase. *Reading* the stash
//! back does — which is the point: recovering the edits is an
//! authenticated act.
//!
//! SECURITY: the file written here is ciphertext. The envelope below is
//! the *plaintext* that goes inside it, and it lives only in a
//! `SecureBuffer`. Header values (paths) are inside the encryption too,
//! so a stash file on disk reveals only that some edit is held.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use super::spool::SegmentFormat;
use crate::crypto::secure_buf::SecureBuffer;

/// First line of the envelope. Frozen.
const MAGIC: &str = "schl8-stash/v1";

/// Refuse to load a stash bigger than this. The envelope is
/// attacker-influenced in the same sense an archive is (anything on
/// disk), so parsing must not be able to allocate without bound.
const MAX_STASH_BYTES: usize = 64 * 1024 * 1024;

/// What a stash holds. Bodies are plaintext and stay in `SecureBuffer`s.
pub struct HeldEdits {
    /// RFC 3339 UTC time the stash was written.
    pub saved: String,
    /// The document being edited (None for a never-saved new document).
    pub source: Option<PathBuf>,
    /// For a vault, the entry inside it that was being edited.
    pub entry: Option<String>,
    /// The quicknote the jot text was aimed at, if any.
    pub jot_target: Option<PathBuf>,
    /// Unsaved editor text.
    pub doc: Option<SecureBuffer>,
    /// Unsaved quick-note text.
    pub jot: Option<SecureBuffer>,
}

/// Key-free summary of a stash, for the locked screen.
///
/// Deliberately says nothing about *content* — only that something is
/// held, from when, and for which file. Reading it needs no key, so the
/// locked screen can describe what's waiting before the user authenticates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashSummary {
    pub saved: String,
    pub format: SegmentFormat,
    pub path: PathBuf,
}

/// Fields describing what is being stashed (everything but the bodies).
#[derive(Default)]
pub struct StashMeta {
    pub source: Option<PathBuf>,
    pub entry: Option<String>,
    pub jot_target: Option<PathBuf>,
}

/// Directory holding the stash, beside the config file.
pub fn dir() -> Option<PathBuf> {
    crate::config::config_path().and_then(|p| p.parent().map(|d| d.join("stash")))
}

/// Where a stash of this backend lives. One document is open at a time,
/// so one stash per backend is enough, and a fixed name means a crash
/// can never leave a pile of them behind.
pub fn path_for(format: SegmentFormat) -> Option<PathBuf> {
    dir().map(|d| d.join(format!("held.{}", format.extension())))
}

/// The stash currently on disk, if any. Needs no key: the filename says
/// which backend wrote it, and the mtime says when.
pub fn find() -> Option<StashSummary> {
    for format in [SegmentFormat::Age, SegmentFormat::Gpg] {
        let Some(p) = path_for(format) else { continue };
        let Ok(md) = std::fs::metadata(&p) else {
            continue;
        };
        if !md.is_file() || md.len() == 0 {
            continue;
        }
        let saved = md
            .modified()
            .map(|t| {
                chrono::DateTime::<chrono::Local>::from(t)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        return Some(StashSummary {
            saved,
            format,
            path: p,
        });
    }
    None
}

/// Build the plaintext envelope. Bodies are copied verbatim after the
/// blank line and located by byte length, so text containing blank lines,
/// the magic string, or anything else round-trips exactly.
pub fn envelope(
    saved: &str,
    meta: &StashMeta,
    doc: Option<&[u8]>,
    jot: Option<&[u8]>,
) -> SecureBuffer {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(MAGIC.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(format!("saved: {saved}\n").as_bytes());
    if let Some(s) = &meta.source {
        out.extend_from_slice(format!("source: {}\n", s.display()).as_bytes());
    }
    if let Some(e) = &meta.entry {
        out.extend_from_slice(format!("entry: {e}\n").as_bytes());
    }
    if let Some(t) = &meta.jot_target {
        out.extend_from_slice(format!("jot-target: {}\n", t.display()).as_bytes());
    }
    out.extend_from_slice(format!("doc-bytes: {}\n", doc.map_or(0, |d| d.len())).as_bytes());
    out.extend_from_slice(format!("jot-bytes: {}\n", jot.map_or(0, |j| j.len())).as_bytes());
    out.push(b'\n');
    if let Some(d) = doc {
        out.extend_from_slice(d);
    }
    if let Some(j) = jot {
        out.extend_from_slice(j);
    }
    // from_bytes zeroizes `out` as it takes ownership, so the plaintext
    // never lingers in an unlocked allocation.
    SecureBuffer::from_bytes(out)
}

/// Parse a decrypted envelope.
pub fn parse(plaintext: &[u8]) -> Result<HeldEdits> {
    if plaintext.len() > MAX_STASH_BYTES {
        return Err(anyhow!("stash is implausibly large"));
    }
    // Headers are ASCII; the bodies after the blank line may be any bytes,
    // so only the header block is treated as text.
    let split = find_blank_line(plaintext)
        .ok_or_else(|| anyhow!("stash has no blank line after its headers"))?;
    let (header_bytes, body) = plaintext.split_at(split.0);
    let body = &body[split.1..];
    let header = std::str::from_utf8(header_bytes).context("stash headers are not valid text")?;

    let mut lines = header.lines();
    let first = lines.next().unwrap_or_default();
    if first != MAGIC {
        return Err(anyhow!(
            "not a Schl8 stash (expected {MAGIC:?}, got {first:?})"
        ));
    }

    let mut saved = String::new();
    let mut meta = StashMeta::default();
    let mut doc_bytes = 0usize;
    let mut jot_bytes = 0usize;
    for line in lines {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "saved" => saved = value.to_string(),
            "source" if !value.is_empty() => meta.source = Some(PathBuf::from(value)),
            "entry" if !value.is_empty() => meta.entry = Some(value.to_string()),
            "jot-target" if !value.is_empty() => meta.jot_target = Some(PathBuf::from(value)),
            "doc-bytes" => doc_bytes = value.parse().unwrap_or(0),
            "jot-bytes" => jot_bytes = value.parse().unwrap_or(0),
            // Unknown headers are ignored so later versions can add fields.
            _ => {}
        }
    }

    // A truncated or tampered stash must fail cleanly, never slice out of
    // bounds or hand back one body's bytes as the other's.
    if doc_bytes.saturating_add(jot_bytes) != body.len() {
        return Err(anyhow!(
            "stash body is {} bytes but its headers claim {}",
            body.len(),
            doc_bytes + jot_bytes
        ));
    }

    let doc = (doc_bytes > 0).then(|| SecureBuffer::from_bytes(body[..doc_bytes].to_vec()));
    let jot = (jot_bytes > 0).then(|| SecureBuffer::from_bytes(body[doc_bytes..].to_vec()));

    Ok(HeldEdits {
        saved,
        source: meta.source,
        entry: meta.entry,
        jot_target: meta.jot_target,
        doc,
        jot,
    })
}

/// Byte offset of the header/body separator, and the separator's length.
fn find_blank_line(bytes: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            if bytes.get(i + 1) == Some(&b'\n') {
                return Some((i + 1, 1));
            }
            if bytes.get(i + 1) == Some(&b'\r') && bytes.get(i + 2) == Some(&b'\n') {
                return Some((i + 1, 2));
            }
        }
        i += 1;
    }
    None
}

/// Write the encrypted stash, replacing any previous one.
///
/// Both backends' files are cleared first so a stash written with one key
/// can't be shadowed by a stale one written with the other.
pub fn write(ciphertext: &[u8], format: SegmentFormat) -> Result<PathBuf> {
    let dir = dir().ok_or_else(|| anyhow!("no config directory to hold the stash"))?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    clear();
    let path = path_for(format).ok_or_else(|| anyhow!("no stash path"))?;
    crate::crypto::keys::atomic_write(&path, ciphertext)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// Remove any stash. Called after the edits are restored or discarded.
pub fn clear() {
    for format in [SegmentFormat::Age, SegmentFormat::Gpg] {
        if let Some(p) = path_for(format) {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Decrypt a stash file with whichever backend its extension names.
pub fn read(
    path: &Path,
    format: SegmentFormat,
    identity: Option<&crate::crypto::age_backend::AgeIdentity>,
) -> Result<HeldEdits> {
    let plaintext = match format {
        SegmentFormat::Age => {
            let id = identity.ok_or_else(|| anyhow!("the AGE identity is locked"))?;
            let ct = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            id.decrypt(&ct)?
        }
        SegmentFormat::Gpg => crate::crypto::gpg::decrypt_file(path)?,
    };
    parse(plaintext.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_both_bodies_verbatim() {
        let doc = "# Draft\n\nline with: a colon\n\nand a blank line above\n";
        let jot = "a jotted thought\n";
        let meta = StashMeta {
            source: Some(PathBuf::from("/notes/plan.md.gpg")),
            entry: None,
            jot_target: Some(PathBuf::from("/notes/journal.md.age")),
        };
        let env = envelope(
            "2026-07-28T09:00:00Z",
            &meta,
            Some(doc.as_bytes()),
            Some(jot.as_bytes()),
        );

        let held = parse(env.as_bytes()).expect("parses");
        assert_eq!(held.saved, "2026-07-28T09:00:00Z");
        assert_eq!(held.source, Some(PathBuf::from("/notes/plan.md.gpg")));
        assert_eq!(
            held.jot_target,
            Some(PathBuf::from("/notes/journal.md.age"))
        );
        // Byte-for-byte: blank lines inside the body must not be mistaken
        // for the header separator.
        assert_eq!(held.doc.unwrap().as_str().unwrap(), doc);
        assert_eq!(held.jot.unwrap().as_str().unwrap(), jot);
    }

    #[test]
    fn a_body_containing_the_magic_line_still_round_trips() {
        // Bodies are located by length, not by scanning, so text that
        // looks like an envelope cannot confuse the parser.
        let doc = format!("{MAGIC}\nsaved: fake\n\nnot really a stash\n");
        let env = envelope(
            "2026-01-01T00:00:00Z",
            &StashMeta::default(),
            Some(doc.as_bytes()),
            None,
        );
        let held = parse(env.as_bytes()).unwrap();
        assert_eq!(held.doc.unwrap().as_str().unwrap(), doc);
        assert!(held.jot.is_none());
    }

    #[test]
    fn document_only_and_jot_only_stashes_work() {
        let env = envelope("t", &StashMeta::default(), Some(b"just the doc"), None);
        let held = parse(env.as_bytes()).unwrap();
        assert_eq!(held.doc.unwrap().as_bytes(), b"just the doc");
        assert!(held.jot.is_none());

        let env = envelope("t", &StashMeta::default(), None, Some(b"just the jot"));
        let held = parse(env.as_bytes()).unwrap();
        assert!(held.doc.is_none());
        assert_eq!(held.jot.unwrap().as_bytes(), b"just the jot");
    }

    #[test]
    fn an_archive_entry_is_recorded_so_the_edit_returns_to_the_right_file() {
        let meta = StashMeta {
            source: Some(PathBuf::from("/v/vault.tar.gz.gpg")),
            entry: Some("vault/notes/plan.md".to_string()),
            jot_target: None,
        };
        let env = envelope("t", &meta, Some(b"edited"), None);
        let held = parse(env.as_bytes()).unwrap();
        assert_eq!(held.entry.as_deref(), Some("vault/notes/plan.md"));
        assert_eq!(held.source, Some(PathBuf::from("/v/vault.tar.gz.gpg")));
    }

    #[test]
    fn truncated_or_foreign_stashes_are_rejected_not_misread() {
        // Wrong magic.
        assert!(parse(b"hello\n\nbody").is_err());
        // No separator at all.
        assert!(parse(format!("{MAGIC}\nsaved: t\n").as_bytes()).is_err());
        // Lengths that don't match the body would otherwise slice out of
        // bounds or silently hand back the wrong bytes.
        let bad = format!("{MAGIC}\nsaved: t\ndoc-bytes: 99\njot-bytes: 0\n\nshort");
        // Deliberately not `unwrap_err`: that needs Debug on HeldEdits,
        // and deriving Debug there would make plaintext printable.
        let err = match parse(bad.as_bytes()) {
            Ok(_) => panic!("a truncated stash must not parse"),
            Err(e) => format!("{e:#}"),
        };
        assert!(err.contains("claim"), "got {err}");
        assert!(parse(b"").is_err());
    }

    #[test]
    fn stash_paths_are_backend_tagged_and_side_by_side_with_the_config() {
        // Both live in the same directory with distinct names, which is
        // what lets `find` report the backend without a key.
        let age = path_for(SegmentFormat::Age);
        let gpg = path_for(SegmentFormat::Gpg);
        if let (Some(a), Some(g)) = (age, gpg) {
            assert_ne!(a, g);
            assert_eq!(a.parent(), g.parent());
            assert!(a.to_str().unwrap().ends_with("held.age"));
            assert!(g.to_str().unwrap().ends_with("held.gpg"));
        }
    }
}
