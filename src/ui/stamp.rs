//! Cached on-disk identity (SHA-256 + mtime) of encrypted files, for the
//! status bar and the picker's recents list.
//!
//! SECURITY: the hash covers the **ciphertext** bytes on disk. No
//! plaintext is read, decrypted, or hashed here, and nothing derived from
//! document content is displayed — only what any observer of the file
//! could already compute.
//!
//! Both caches re-hash only when a file's mtime changes, so the status
//! bar can ask every frame without reading the file every frame.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::statusbar::FileStamp;

/// Digest + formatted mtime of the encrypted file at `path`.
///
/// `None` when the file can't be read — callers show the entry dimmed
/// rather than treating it as an error.
pub fn compute_stamp(path: &Path, mtime: SystemTime) -> Option<FileStamp> {
    use sha2::Digest;
    let bytes = std::fs::read(path).ok()?;
    let digest = sha2::Sha256::digest(&bytes);
    let mut full = [0u8; 32];
    full.copy_from_slice(&digest);
    let modified = chrono::DateTime::<chrono::Local>::from(mtime)
        .format("%Y-%m-%d %H:%M")
        .to_string();
    // The bytes are already in hand for the hash, so the size costs
    // nothing extra — no second stat, and it stays consistent with the
    // hash even if the file changes between calls.
    Some(FileStamp {
        modified,
        bytes: bytes.len() as u64,
        digest: full,
    })
}

/// The SHA-256 of the encrypted file at `path`, as lowercase hex.
///
/// Separate from [`compute_stamp`] because the change-detection path
/// wants only the digest and runs once per open or save, not per frame.
pub fn digest_hex(path: &Path) -> Option<String> {
    use sha2::Digest;
    let bytes = std::fs::read(path).ok()?;
    Some(
        sha2::Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

/// Single-file stamp cache for the open document's status bar.
#[derive(Default)]
pub struct FileStampCache {
    key: Option<(PathBuf, SystemTime)>,
    stamp: Option<FileStamp>,
}

impl FileStampCache {
    /// The stamp for `path`, re-hashing only if the path or its mtime
    /// changed since the last call.
    pub fn get(&mut self, path: &Path) -> Option<FileStamp> {
        let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
        let key = (path.to_path_buf(), mtime);
        if self.key.as_ref() != Some(&key) {
            self.stamp = compute_stamp(path, mtime);
            self.key = Some(key);
        }
        self.stamp.clone()
    }
}

/// Like [`FileStampCache`], but for the picker's recents list: one cached
/// stamp per path, re-hashed only when that file's mtime changes. Missing
/// files yield `None` (shown dimmed, still removable from the list).
#[derive(Default)]
pub struct RecentStamps(HashMap<PathBuf, (SystemTime, FileStamp)>);

impl RecentStamps {
    pub fn get(&mut self, path: &Path) -> Option<FileStamp> {
        let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
        match self.0.get(path) {
            Some((cached_mtime, stamp)) if *cached_mtime == mtime => Some(stamp.clone()),
            _ => {
                let stamp = compute_stamp(path, mtime)?;
                self.0.insert(path.to_path_buf(), (mtime, stamp.clone()));
                Some(stamp)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("schl8-stamp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn stamp_is_content_derived_and_stable() {
        let dir = tmp("basic");
        let a = dir.join("a.gpg");
        let b = dir.join("b.gpg");
        std::fs::write(&a, b"ciphertext").unwrap();
        std::fs::write(&b, b"ciphertext").unwrap();

        let mtime = std::fs::metadata(&a).unwrap().modified().unwrap();
        let s1 = compute_stamp(&a, mtime).expect("stamp");
        assert_ne!(s1.digest, [0u8; 32], "a real digest, not a default");

        // Same bytes hash the same regardless of path; different bytes don't.
        let s2 = compute_stamp(&b, mtime).unwrap();
        assert_eq!(s1.digest, s2.digest);
        std::fs::write(&b, b"different ciphertext").unwrap();
        assert_ne!(s1.digest, compute_stamp(&b, mtime).unwrap().digest);

        // A file that isn't there has no stamp.
        assert!(compute_stamp(&dir.join("missing.gpg"), mtime).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_refreshes_when_the_file_changes() {
        let dir = tmp("cache");
        let f = dir.join("note.md.gpg");
        std::fs::write(&f, b"first").unwrap();

        let mut cache = FileStampCache::default();
        let before = cache.get(&f).expect("stamp");
        assert_eq!(cache.get(&f).unwrap().digest, before.digest, "cached");

        // A rewrite must be picked up, not served from the cache — the
        // status bar showing a stale hash would misreport what's on disk.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&f, b"second").unwrap();
        assert_ne!(cache.get(&f).unwrap().digest, before.digest);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recents_cache_is_per_path_and_tolerates_missing_files() {
        let dir = tmp("recents");
        let a = dir.join("a.gpg");
        let b = dir.join("b.gpg");
        std::fs::write(&a, b"aaa").unwrap();
        std::fs::write(&b, b"bbb").unwrap();

        let mut stamps = RecentStamps::default();
        let sa = stamps.get(&a).expect("a");
        let sb = stamps.get(&b).expect("b");
        assert_ne!(sa.digest, sb.digest, "entries don't share a cache slot");
        assert_eq!(stamps.get(&a).unwrap().digest, sa.digest);

        // Editing one entry must not disturb the other's cached stamp.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&a, b"aaa-changed").unwrap();
        assert_ne!(stamps.get(&a).unwrap().digest, sa.digest);
        assert_eq!(stamps.get(&b).unwrap().digest, sb.digest);

        // A recent whose file was deleted: None, and no panic.
        std::fs::remove_file(&b).unwrap();
        assert!(stamps.get(&b).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
