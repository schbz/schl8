use std::path::Path;

use anyhow::{Context, Result};

use super::archive::{self, ArchiveDocument};
use super::{detect_file_type_from_name, Document, FileType, LoadedDocument};
use crate::crypto::gpg;

/// Load and decrypt a GPG-encrypted document or folder archive.
/// Single files: type detected from the double extension (.txt.gpg, .md.gpg).
/// Folder archives (.tar.gz.gpg and friends, or gzip/tar content) are
/// extracted in memory into one SecureBuffer per text file.
pub fn load(path: &Path) -> Result<LoadedDocument> {
    // Validate the file exists
    if !path.exists() {
        anyhow::bail!("file not found: {}", path.display());
    }

    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Plain (unencrypted) text/markdown file? Load it directly — viewing
    // and editing work as usual, but saving always goes through
    // encryption; Schl8 never writes plaintext.
    let is_encrypted_name =
        name.ends_with(".gpg") || name.ends_with(".asc") || name.ends_with(".pgp");
    if !is_encrypted_name {
        if let Some(file_type) = detect_file_type_from_name(name) {
            let bytes = std::fs::read(path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let content = crate::crypto::secure_buf::SecureBuffer::from_bytes(bytes);
            content
                .as_str()
                .with_context(|| format!("content of {} is not valid text", path.display()))?;
            return Ok(LoadedDocument::Single(Document {
                content,
                file_type,
                source_path: path.to_path_buf(),
                recipients: None,
                signature: gpg::SignatureStatus::Unsigned,
            }));
        }
    }

    // Decrypt the file — plaintext stays in SecureBuffer (mlock'd memory);
    // capture any signature's authenticity at the same time.
    let decrypted = gpg::decrypt_file_verified(path)
        .with_context(|| format!("failed to decrypt {}", path.display()))?;

    // Remember who the file was encrypted to (as primary-key fingerprints)
    // so Save can re-encrypt to the same keys. Non-fatal if unavailable
    // (anonymous recipients, or a recipient key not in the keyring) — Save
    // simply falls back to Encrypt & Save As.
    let recipients = crate::crypto::keys::recipients_for_reencrypt(path).ok();

    finalize_decrypted(path, decrypted.content, recipients, decrypted.signature)
}

/// Decide whether already-decrypted bytes are a folder archive or a single
/// text document, and build the [`LoadedDocument`]. Shared by the GPG and
/// AGE loaders so both handle vaults (`.tar.gz.gpg`, `.tar.gz.age`, or
/// archive content under any name) identically.
fn finalize_decrypted(
    path: &Path,
    content: crate::crypto::secure_buf::SecureBuffer,
    recipients: Option<Vec<String>>,
    signature: gpg::SignatureStatus,
) -> Result<LoadedDocument> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Folder archive? Detect by name, or by content magic when the name
    // gives no hint (e.g. `backup.age` containing a tarball).
    let named_archive = archive::is_archive_name(name);
    if named_archive
        || (detect_inner_type(name).is_none() && archive::looks_like_archive(content.as_bytes()))
    {
        // Keep the raw tar so an edited entry can be re-tarred without
        // losing the archive's non-text files.
        let (raw_tar, gzip) = archive::decompress_to_tar(content.as_bytes())
            .with_context(|| format!("failed to decompress archive {}", path.display()))?;
        let extracted = archive::extract_text_entries(raw_tar.as_bytes())
            .with_context(|| format!("failed to extract archive {}", path.display()))?;
        let dirs = archive::extract_dir_entries(raw_tar.as_bytes());
        return Ok(LoadedDocument::Archive(ArchiveDocument {
            source_path: path.to_path_buf(),
            entries: extracted.entries,
            raw_tar,
            gzip,
            dirs,
            recipients,
            hidden: extracted.hidden,
        }));
    }

    // Not an archive: a single text document must be valid UTF-8.
    content
        .as_str()
        .with_context(|| format!("decrypted content of {} is not valid text", path.display()))?;

    Ok(LoadedDocument::Single(Document {
        content,
        file_type: detect_inner_type(name).unwrap_or(FileType::PlainText),
        source_path: path.to_path_buf(),
        recipients,
        signature,
    }))
}

/// Detect the inner file type from a double extension (e.g. `.md.gpg`,
/// `.md.age`). None when the name carries no recognizable text extension.
fn detect_inner_type(name: &str) -> Option<FileType> {
    let inner = name
        .strip_suffix(".gpg")
        .or_else(|| name.strip_suffix(".asc"))
        .or_else(|| name.strip_suffix(".pgp"))
        .or_else(|| name.strip_suffix(".age"))
        .unwrap_or(name);
    detect_file_type_from_name(inner)
}

/// Whether `path` is an age-encrypted file — by the `.age` extension or,
/// for other names, the age header magic (the age format's cleartext
/// prefix, readable without decrypting).
pub fn is_age_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.ends_with(".age") {
        return true;
    }
    // Cheap content sniff: read only the first bytes.
    if let Ok(mut f) = std::fs::File::open(path) {
        use std::io::Read;
        let mut head = [0u8; 64];
        if let Ok(n) = f.read(&mut head) {
            return crate::crypto::age_backend::looks_like_age(&head[..n]);
        }
    }
    false
}

/// Load and decrypt an age-encrypted single document with `identity`.
/// Runs synchronously (age decryption is fast and needs no subprocess or
/// PIN, unlike GPG). age files record no recipients, so `recipients` is
/// `None` and there is never a signature.
pub fn load_age(
    path: &Path,
    identity: &crate::crypto::age_backend::AgeIdentity,
) -> Result<LoadedDocument> {
    if !path.exists() {
        anyhow::bail!("file not found: {}", path.display());
    }
    let ciphertext =
        std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let content = identity
        .decrypt(&ciphertext)
        .with_context(|| format!("failed to decrypt {}", path.display()))?;

    // Same archive-vs-single routing as the GPG path — an AGE-encrypted
    // vault (`.tar.gz.age`, or archive content under any name) decrypts to
    // tar bytes, which are not UTF-8 and must not be treated as text.
    // recipients = None: AGE hides them, so Save defaults to own identity.
    finalize_decrypted(path, content, None, gpg::SignatureStatus::Unsigned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_markdown_from_double_extension() {
        assert_eq!(detect_inner_type("notes.md.gpg"), Some(FileType::Markdown));
        assert_eq!(
            detect_inner_type("notes.markdown.asc"),
            Some(FileType::Markdown)
        );
    }

    #[test]
    fn detects_plaintext_and_unknown() {
        assert_eq!(
            detect_inner_type("notes.txt.gpg"),
            Some(FileType::PlainText)
        );
        assert_eq!(detect_inner_type("notes.gpg"), None);
        assert_eq!(detect_inner_type("notes.md"), Some(FileType::Markdown));
    }

    #[test]
    fn detects_age_double_extension() {
        assert_eq!(detect_inner_type("notes.md.age"), Some(FileType::Markdown));
        assert_eq!(detect_inner_type("log.txt.age"), Some(FileType::PlainText));
    }

    #[test]
    fn age_encrypted_archive_loads_as_archive_not_text() {
        use crate::crypto::age_backend::{encrypt_to_recipients, AgeIdentity};
        let id = AgeIdentity::from_mnemonic(
            "legal winner thank year wave sausage worth useful legal winner thank yellow",
            "",
        )
        .unwrap();

        // Build a tiny gzip'd tar in memory with two text files.
        let mut tar = tar::Builder::new(Vec::new());
        for (name, body) in [("vault/a.md", b"# A\n" as &[u8]), ("vault/b.txt", b"hi\n")] {
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            tar.append_data(&mut h, name, body).unwrap();
        }
        let tar_bytes = tar.into_inner().unwrap();
        use flate2::{write::GzEncoder, Compression};
        use std::io::Write as _;
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(&tar_bytes).unwrap();
        let gzipped = gz.finish().unwrap();

        // Encrypt to the identity and write a `.tar.gz.age` file.
        let ct = encrypt_to_recipients(&gzipped, &[id.recipient()]).unwrap();
        let dir = std::env::temp_dir().join(format!("schl8-agevault-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("vault.tar.gz.age");
        std::fs::write(&path, &ct).unwrap();

        // The bug: this used to fail with "not valid text". It must load
        // as an archive with both entries.
        match load_age(&path, &id).expect("age vault must load") {
            LoadedDocument::Archive(a) => {
                let paths: Vec<&str> = a.entries.iter().map(|e| e.rel_path.as_str()).collect();
                assert!(paths.contains(&"vault/a.md"), "got {paths:?}");
                assert!(paths.contains(&"vault/b.txt"), "got {paths:?}");
                assert!(a.recipients.is_none(), "AGE hides recipients");
            }
            LoadedDocument::Single(_) => panic!("age vault loaded as a single text file"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn encrypt_write_detect_and_load_age_file() {
        use crate::crypto::age_backend::{encrypt_to_recipients, AgeIdentity};

        let id = AgeIdentity::from_mnemonic(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
            "",
        )
        .unwrap();
        let ct = encrypt_to_recipients(b"# Age note\n\nhello", &[id.recipient()]).unwrap();

        let dir = std::env::temp_dir().join(format!("schl8-age-load-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.md.age");
        std::fs::write(&path, &ct).unwrap();

        assert!(is_age_file(&path), "should detect .age by extension");
        // Also detects by content even without the extension.
        let noext = dir.join("note-noext");
        std::fs::write(&noext, &ct).unwrap();
        assert!(is_age_file(&noext), "should detect age by header magic");

        match load_age(&path, &id).unwrap() {
            LoadedDocument::Single(doc) => {
                assert_eq!(doc.content.as_str().unwrap(), "# Age note\n\nhello");
                assert_eq!(doc.file_type, FileType::Markdown);
                assert!(doc.recipients.is_none(), "age files record no recipients");
            }
            _ => panic!("expected a single document"),
        }

        // A different identity cannot open it.
        let stranger = AgeIdentity::from_mnemonic(
            "legal winner thank year wave sausage worth useful legal winner thank yellow",
            "",
        )
        .unwrap();
        assert!(load_age(&path, &stranger).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
