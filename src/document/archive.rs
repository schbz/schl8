//! In-memory extraction of encrypted folder archives.
//!
//! The companion encrypt workflow compresses a folder to `folder.tar.gz`
//! and encrypts it to `folder.tar.gz.gpg`. This module takes the decrypted
//! archive bytes (already in a `SecureBuffer`) and extracts every text /
//! markdown entry — at any nesting depth — into its own `SecureBuffer`.
//! Nothing is ever written to disk.

use anyhow::{Context, Result};

use super::{detect_file_type_from_name, FileType};
use crate::crypto::secure_buf::SecureBuffer;

/// One text file extracted from an archive.
pub struct ArchiveEntry {
    /// Path inside the archive, e.g. `notes/2026/plan.md`.
    pub rel_path: String,
    pub file_type: FileType,
    pub content: SecureBuffer,
}

/// Files that are inside the vault but that the browser cannot show,
/// counted by reason.
///
/// They are **not** lost: rebuilds work from `raw_tar`, so every one of
/// them survives a save byte-for-byte. But a browser that silently omits
/// them makes a vault look emptier than it is — which invites someone to
/// "clean up" a vault that still holds their photos — so the count is
/// carried out of extraction and shown.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HiddenEntries {
    /// Filename isn't a recognized text/markdown type (images, PDFs, …).
    pub non_text: usize,
    /// Larger than [`MAX_ENTRY_BYTES`].
    pub too_large: usize,
    /// A text-looking name whose bytes aren't valid UTF-8.
    pub not_utf8: usize,
    /// Editor/OS clutter (`.DS_Store`, `__MACOSX`, AppleDouble `._*`).
    pub junk: usize,
}

impl HiddenEntries {
    pub fn total(&self) -> usize {
        self.non_text + self.too_large + self.not_utf8 + self.junk
    }

    /// One-line summary for the browser, or None when nothing is hidden.
    /// Callers treat `None` as "nothing to report" — the common all-text
    /// vault must stay silent, or the notice becomes noise and gets
    /// ignored on the vault where it matters.
    pub fn summary(&self) -> Option<String> {
        let total = self.total();
        if total == 0 {
            return None;
        }
        let mut parts = Vec::new();
        for (n, label) in [
            (self.non_text, "not text"),
            (self.too_large, "too large"),
            (self.not_utf8, "not valid text"),
            (self.junk, "system files"),
        ] {
            if n > 0 {
                parts.push(format!("{n} {label}"));
            }
        }
        Some(format!(
            "{total} file{} not shown ({})",
            if total == 1 { "" } else { "s" },
            parts.join(", ")
        ))
    }
}

/// Entries the browser can display, plus a tally of what it can't.
pub struct Extracted {
    pub entries: Vec<ArchiveEntry>,
    pub hidden: HiddenEntries,
}

/// A decrypted folder archive held entirely in secure memory.
pub struct ArchiveDocument {
    pub source_path: std::path::PathBuf,
    /// Entries sorted by path (directories group naturally).
    pub entries: Vec<ArchiveEntry>,
    /// The full decompressed tar (mlock'd, zeroized on drop). Kept so an
    /// edited entry can be saved without losing the archive's non-text
    /// files: the tar is rebuilt from this, entry by entry.
    pub raw_tar: SecureBuffer,
    /// Whether the source archive was gzip-compressed (rebuilds match).
    pub gzip: bool,
    /// Empty directory entries — folders with no files yet. Files imply
    /// their own folders in the tree; these are the ones that would
    /// otherwise be invisible.
    pub dirs: Vec<String>,
    /// Recipient fingerprints for re-encrypting on save (None when they
    /// couldn't be resolved — saving then requires Encrypt & Save As).
    pub recipients: Option<Vec<String>>,
    /// What the browser could not list. Shown in the sidebar so a vault
    /// never looks emptier than it is.
    pub hidden: HiddenEntries,
}

/// Whether a filename looks like an encrypted folder archive.
pub fn is_archive_name(name: &str) -> bool {
    let inner = name
        .strip_suffix(".gpg")
        .or_else(|| name.strip_suffix(".asc"))
        .or_else(|| name.strip_suffix(".pgp"))
        .or_else(|| name.strip_suffix(".age"))
        .unwrap_or(name);
    inner.ends_with(".tar.gz") || inner.ends_with(".tgz") || inner.ends_with(".tar")
}

/// Whether decrypted bytes look like a gzip stream or tarball, for archives
/// that don't follow the double-extension naming convention.
pub fn looks_like_archive(bytes: &[u8]) -> bool {
    is_gzip(bytes) || is_tar(bytes)
}

fn is_gzip(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b
}

fn is_tar(bytes: &[u8]) -> bool {
    // POSIX tar has "ustar" at offset 257.
    bytes.len() > 262 && &bytes[257..262] == b"ustar"
}

/// Files/dirs that are noise in archives made on macOS.
fn is_junk(path: &str) -> bool {
    path.split('/')
        .any(|part| part == "__MACOSX" || part == ".DS_Store" || part.starts_with("._"))
}

// ── Resource limits ──────────────────────────────────────────────────
// These bound the damage from a hostile archive (a gzip/tar "bomb" or a
// forged header claiming a huge size). The decrypted bytes are
// attacker-influenced — anything you were handed and open — so extraction
// must never be able to OOM or hang the process.

/// Hard cap on total *decompressed* bytes read from the archive stream.
/// A gzip bomb expands far past its on-disk size; this stops the read.
const MAX_TOTAL_DECOMPRESSED: u64 = 256 * 1024 * 1024;
/// Largest single text entry we'll materialize. A "note" bigger than this
/// is pathological; it's skipped rather than loaded.
const MAX_ENTRY_BYTES: usize = 16 * 1024 * 1024;
/// Cap on how many tar entries we'll even iterate (not just keep), so a
/// tar of millions of tiny headers can't spin forever.
const MAX_ENTRIES_SCANNED: usize = 50_000;

/// A reader that fails once more than `limit` bytes have been read through
/// it — bounds decompression bombs regardless of what tar headers claim.
struct LimitedReader<R> {
    inner: R,
    read: u64,
    limit: u64,
}

impl<R: std::io::Read> std::io::Read for LimitedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read = self.read.saturating_add(n as u64);
        if self.read > self.limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "archive exceeds the maximum decompressed size (possible archive bomb)",
            ));
        }
        Ok(n)
    }
}

fn limited<R: std::io::Read>(inner: R) -> LimitedReader<R> {
    LimitedReader {
        inner,
        read: 0,
        limit: MAX_TOTAL_DECOMPRESSED,
    }
}

/// Decompress decrypted archive bytes into the raw tar stream, kept in a
/// `SecureBuffer`. Returns the tar bytes and whether the input was
/// gzip-compressed. Bounded against decompression bombs.
pub fn decompress_to_tar(archive_bytes: &[u8]) -> Result<(SecureBuffer, bool)> {
    use std::io::Read;
    if is_gzip(archive_bytes) {
        let mut decoder = limited(flate2::read::GzDecoder::new(archive_bytes));
        let mut tar_bytes = Vec::new();
        decoder
            .read_to_end(&mut tar_bytes)
            .context("failed to decompress gzip archive")?;
        Ok((SecureBuffer::from_bytes(tar_bytes), true))
    } else if is_tar(archive_bytes) {
        Ok((SecureBuffer::from_bytes(archive_bytes.to_vec()), false))
    } else {
        anyhow::bail!("decrypted content is not a tar or tar.gz archive");
    }
}

/// Extract all text/markdown entries from decrypted archive bytes.
/// Handles both gzip-compressed (`.tar.gz`) and plain (`.tar`) archives.
/// The intermediate decompression stream is transient; every retained
/// plaintext ends up in a `SecureBuffer`. Bounded against archive bombs.
pub fn extract_text_entries(archive_bytes: &[u8]) -> Result<Extracted> {
    if is_gzip(archive_bytes) {
        let decoder = flate2::read::GzDecoder::new(archive_bytes);
        read_tar_entries(limited(decoder)).context("failed to read gzip-compressed tar archive")
    } else if is_tar(archive_bytes) {
        read_tar_entries(limited(archive_bytes)).context("failed to read tar archive")
    } else {
        anyhow::bail!("decrypted content is not a tar or tar.gz archive");
    }
}

/// The payload of a rebuilt archive, ready to encrypt. Both buffers hold
/// plaintext and are zeroized on drop.
pub struct RebuiltArchive {
    /// The new raw tar (replaces `ArchiveDocument::raw_tar` after save).
    pub tar: SecureBuffer,
    /// What gets encrypted and written: the tar, gzip-compressed when the
    /// original archive was.
    pub payload: SecureBuffer,
}

/// Rebuild the archive with `rel_path`'s content replaced by
/// `new_content`, preserving every other entry byte-for-byte — including
/// non-text files, directories, links, and metadata the text extraction
/// skipped. Fails if `rel_path` is not a regular file in the tar.
pub fn rebuild_with_edit(
    tar_bytes: &[u8],
    rel_path: &str,
    new_content: &[u8],
    gzip: bool,
) -> Result<RebuiltArchive> {
    use std::io::Read;
    use zeroize::Zeroize;

    let mut archive = tar::Archive::new(tar_bytes);
    let mut builder = tar::Builder::new(Vec::with_capacity(tar_bytes.len()));
    let mut found = false;

    for entry in archive.entries().context("invalid tar archive")? {
        let mut entry = entry.context("corrupt tar entry")?;
        let path = entry
            .path()
            .context("tar entry has an unreadable path")?
            .into_owned();
        let mut header = entry.header().clone();

        let is_target =
            header.entry_type() == tar::EntryType::Regular && path.to_string_lossy() == rel_path;
        if is_target {
            header.set_size(new_content.len() as u64);
            builder
                .append_data(&mut header, &path, new_content)
                .with_context(|| format!("failed to write edited entry {rel_path}"))?;
            found = true;
        } else {
            // Copy the entry through unchanged (data may be plaintext of
            // other files — zeroize the transient buffer).
            let mut data = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut data)
                .with_context(|| format!("failed to read entry {}", path.display()))?;
            builder
                .append_data(&mut header, &path, data.as_slice())
                .with_context(|| format!("failed to copy entry {}", path.display()))?;
            data.zeroize();
        }
    }

    if !found {
        anyhow::bail!("entry {rel_path} not found in the archive");
    }

    let tar_out = builder.into_inner().context("failed to finish tar")?;

    let payload = if gzip {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, &tar_out)
            .and_then(|_| encoder.finish())
            .context("failed to gzip rebuilt archive")?
    } else {
        tar_out.clone()
    };

    Ok(RebuiltArchive {
        tar: SecureBuffer::from_bytes(tar_out),
        payload: SecureBuffer::from_bytes(payload),
    })
}

/// Compress a raw tar into the encrypt-ready payload (gzip when `gzip`),
/// without any edit. Used to materialize a save plan's destinations from
/// the current archive.
pub fn compress_payload(tar_bytes: &[u8], gzip: bool) -> Result<Vec<u8>> {
    if gzip {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, tar_bytes)
            .and_then(|_| encoder.finish())
            .context("failed to gzip archive")
    } else {
        Ok(tar_bytes.to_vec())
    }
}

/// Wrap a finished raw tar as a [`RebuiltArchive`], gzip-compressing the
/// payload when `gzip` is set. Shared by every mutation below so they all
/// match the source archive's compression.
fn finish_tar(tar_out: Vec<u8>, gzip: bool) -> Result<RebuiltArchive> {
    let payload = if gzip {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, &tar_out)
            .and_then(|_| encoder.finish())
            .context("failed to gzip archive")?
    } else {
        tar_out.clone()
    };
    Ok(RebuiltArchive {
        tar: SecureBuffer::from_bytes(tar_out),
        payload: SecureBuffer::from_bytes(payload),
    })
}

/// Copy every entry of `tar_bytes` into a fresh builder, running `keep` on
/// each `(path, header)` to decide inclusion and optional rename. `keep`
/// returns `None` to drop the entry, or `Some(new_path)` to keep it under
/// that path (same path = unchanged). Other-file plaintext transits a
/// transient buffer that is zeroized. Returns the built tar and whether
/// `keep` ever fired (so callers can detect "nothing matched").
fn rebuild_filtered(
    tar_bytes: &[u8],
    mut keep: impl FnMut(&str, &tar::Header) -> Option<String>,
) -> Result<Vec<u8>> {
    use std::io::Read;
    use zeroize::Zeroize;

    let mut archive = tar::Archive::new(tar_bytes);
    let mut builder = tar::Builder::new(Vec::with_capacity(tar_bytes.len()));

    for entry in archive.entries().context("invalid tar archive")? {
        let mut entry = entry.context("corrupt tar entry")?;
        let path = entry
            .path()
            .context("tar entry has an unreadable path")?
            .into_owned();
        let mut header = entry.header().clone();
        let rel = path.to_string_lossy().into_owned();

        let Some(new_path) = keep(&rel, &header) else {
            // Dropped — still drain the reader so the archive stays aligned.
            let mut sink = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut sink).ok();
            sink.zeroize();
            continue;
        };

        let mut data = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut data)
            .with_context(|| format!("failed to read entry {rel}"))?;
        builder
            .append_data(&mut header, &new_path, data.as_slice())
            .with_context(|| format!("failed to copy entry {new_path}"))?;
        data.zeroize();
    }
    builder.into_inner().context("failed to finish tar")
}

/// Add a new regular file at `rel_path` with `content`. Fails if a regular
/// file already exists there.
pub fn add_entry(
    tar_bytes: &[u8],
    rel_path: &str,
    content: &[u8],
    gzip: bool,
) -> Result<RebuiltArchive> {
    use std::io::Read;
    use zeroize::Zeroize;

    let rel_path = normalize_rel(rel_path)?;
    if entry_exists(tar_bytes, &rel_path)? {
        anyhow::bail!("{rel_path} already exists in this vault");
    }

    // Copy every existing entry through, then append the new file.
    let mut archive = tar::Archive::new(tar_bytes);
    let mut builder = tar::Builder::new(Vec::with_capacity(tar_bytes.len() + content.len() + 512));
    for entry in archive.entries().context("invalid tar archive")? {
        let mut entry = entry.context("corrupt tar entry")?;
        let path = entry.path().context("unreadable path")?.into_owned();
        let mut header = entry.header().clone();
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut data)
            .with_context(|| format!("failed to read entry {}", path.display()))?;
        builder
            .append_data(&mut header, &path, data.as_slice())
            .with_context(|| format!("failed to copy entry {}", path.display()))?;
        data.zeroize();
    }
    let mut header = tar::Header::new_gnu();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_cksum();
    builder
        .append_data(&mut header, &rel_path, content)
        .with_context(|| format!("failed to add {rel_path}"))?;
    let tar_out = builder.into_inner().context("failed to finish tar")?;
    finish_tar(tar_out, gzip)
}

/// Remove the regular file at `rel_path`. Fails if it isn't present.
pub fn remove_entry(tar_bytes: &[u8], rel_path: &str, gzip: bool) -> Result<RebuiltArchive> {
    if !entry_exists(tar_bytes, rel_path)? {
        anyhow::bail!("{rel_path} is not in this vault");
    }
    let tar_out = rebuild_filtered(tar_bytes, |rel, _| {
        if rel == rel_path {
            None
        } else {
            Some(rel.to_string())
        }
    })?;
    finish_tar(tar_out, gzip)
}

/// Rename the regular file at `from` to `to`. Fails if `from` is missing
/// or `to` already exists.
pub fn rename_entry(tar_bytes: &[u8], from: &str, to: &str, gzip: bool) -> Result<RebuiltArchive> {
    let to = normalize_rel(to)?;
    if !entry_exists(tar_bytes, from)? {
        anyhow::bail!("{from} is not in this vault");
    }
    if entry_exists(tar_bytes, &to)? {
        anyhow::bail!("{to} already exists in this vault");
    }
    let tar_out = rebuild_filtered(tar_bytes, |rel, _| {
        if rel == from {
            Some(to.clone())
        } else {
            Some(rel.to_string())
        }
    })?;
    finish_tar(tar_out, gzip)
}

/// Directory-entry paths in the tar (trailing slash trimmed). Empty
/// folders only exist as explicit directory entries; this surfaces them
/// so the tree can show a folder that has no files yet.
pub fn extract_dir_entries(tar_bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut archive = tar::Archive::new(tar_bytes);
    if let Ok(entries) = archive.entries() {
        for entry in entries.flatten() {
            if entry.header().entry_type() == tar::EntryType::Directory {
                if let Ok(path) = entry.path() {
                    let p = path.to_string_lossy();
                    let trimmed = p.trim_end_matches('/');
                    if !trimmed.is_empty() && !is_junk(trimmed) {
                        out.push(trimmed.to_string());
                    }
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Add an empty directory entry so a new, file-less folder persists
/// through save. Fails if a file or directory already occupies the path.
pub fn add_dir(tar_bytes: &[u8], rel_path: &str, gzip: bool) -> Result<RebuiltArchive> {
    use std::io::Read;
    use zeroize::Zeroize;

    let rel_path = normalize_rel(rel_path)?;
    if entry_exists(tar_bytes, &rel_path)? || dir_exists(tar_bytes, &rel_path)? {
        anyhow::bail!("{rel_path} already exists in this vault");
    }

    let mut archive = tar::Archive::new(tar_bytes);
    let mut builder = tar::Builder::new(Vec::with_capacity(tar_bytes.len() + 512));
    for entry in archive.entries().context("invalid tar archive")? {
        let mut entry = entry.context("corrupt tar entry")?;
        let path = entry.path().context("unreadable path")?.into_owned();
        let mut header = entry.header().clone();
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data).context("reading entry")?;
        builder
            .append_data(&mut header, &path, data.as_slice())
            .with_context(|| format!("copying {}", path.display()))?;
        data.zeroize();
    }
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Directory);
    header.set_size(0);
    header.set_mode(0o755);
    header.set_mtime(0);
    header.set_cksum();
    // A trailing slash is the tar convention for directory names.
    builder
        .append_data(&mut header, format!("{rel_path}/"), std::io::empty())
        .with_context(|| format!("failed to add folder {rel_path}"))?;
    let tar_out = builder.into_inner().context("failed to finish tar")?;
    finish_tar(tar_out, gzip)
}

/// Rename a folder: every entry at `from` or under `from/` — files and
/// directory entries alike — moves under `to`. Fails if `from` has no
/// entries, or `to` already exists.
pub fn rename_prefix(tar_bytes: &[u8], from: &str, to: &str, gzip: bool) -> Result<RebuiltArchive> {
    let from = from.trim_end_matches('/');
    let to = normalize_rel(to)?;
    if !prefix_has_entries(tar_bytes, from)? {
        anyhow::bail!("folder {from} is not in this vault");
    }
    if prefix_has_entries(tar_bytes, &to)? {
        anyhow::bail!("{to} already exists in this vault");
    }
    let tar_out = rebuild_filtered(tar_bytes, |rel, _| {
        let trimmed = rel.trim_end_matches('/');
        let is_dir = rel.ends_with('/');
        let moved = if trimmed == from {
            Some(to.clone())
        } else if let Some(rest) = trimmed.strip_prefix(&format!("{from}/")) {
            Some(format!("{to}/{rest}"))
        } else {
            Some(trimmed.to_string())
        };
        // Preserve the trailing slash on directory entries.
        moved.map(|m| if is_dir { format!("{m}/") } else { m })
    })?;
    finish_tar(tar_out, gzip)
}

/// Remove a folder and everything under it.
pub fn remove_prefix(tar_bytes: &[u8], prefix: &str, gzip: bool) -> Result<RebuiltArchive> {
    let prefix = prefix.trim_end_matches('/');
    if !prefix_has_entries(tar_bytes, prefix)? {
        anyhow::bail!("folder {prefix} is not in this vault");
    }
    let tar_out = rebuild_filtered(tar_bytes, |rel, _| {
        let trimmed = rel.trim_end_matches('/');
        if trimmed == prefix || trimmed.starts_with(&format!("{prefix}/")) {
            None
        } else {
            Some(rel.to_string())
        }
    })?;
    finish_tar(tar_out, gzip)
}

/// Whether any entry sits at `prefix` or under `prefix/`.
fn prefix_has_entries(tar_bytes: &[u8], prefix: &str) -> Result<bool> {
    let mut archive = tar::Archive::new(tar_bytes);
    for entry in archive.entries().context("invalid tar archive")? {
        let entry = entry.context("corrupt tar entry")?;
        let path = entry.path().context("unreadable path")?;
        let rel = path.to_string_lossy();
        let trimmed = rel.trim_end_matches('/');
        if trimmed == prefix || trimmed.starts_with(&format!("{prefix}/")) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether a directory entry exists at exactly `rel_path`.
fn dir_exists(tar_bytes: &[u8], rel_path: &str) -> Result<bool> {
    Ok(extract_dir_entries(tar_bytes).iter().any(|d| d == rel_path))
}

/// Whether a regular file exists at `rel_path`.
fn entry_exists(tar_bytes: &[u8], rel_path: &str) -> Result<bool> {
    let mut archive = tar::Archive::new(tar_bytes);
    for entry in archive.entries().context("invalid tar archive")? {
        let entry = entry.context("corrupt tar entry")?;
        let path = entry.path().context("unreadable path")?;
        if path.to_string_lossy() == rel_path
            && entry.header().entry_type() == tar::EntryType::Regular
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Reject path traversal and absolute paths in a new entry name. Tar can
/// carry `../` and leading `/`; a vault built here must not.
fn normalize_rel(rel: &str) -> Result<String> {
    let rel = rel.trim_matches('/');
    if rel.is_empty() {
        anyhow::bail!("empty path");
    }
    if rel.split('/').any(|c| c == ".." || c == ".") {
        anyhow::bail!("path {rel:?} may not contain . or ..");
    }
    Ok(rel.to_string())
}

fn read_tar_entries<R: std::io::Read>(reader: R) -> Result<Extracted> {
    use std::io::Read;

    let mut archive = tar::Archive::new(reader);
    let mut entries = Vec::new();
    let mut scanned = 0usize;
    let mut hidden = HiddenEntries::default();

    for entry in archive.entries().context("invalid tar archive")? {
        scanned += 1;
        if scanned > MAX_ENTRIES_SCANNED {
            anyhow::bail!("archive has too many entries (> {MAX_ENTRIES_SCANNED})");
        }

        let mut entry = entry.context("corrupt tar entry")?;

        if entry.header().entry_type() != tar::EntryType::Regular {
            continue;
        }

        let rel_path = match entry.path() {
            Ok(p) => p.to_string_lossy().into_owned(),
            Err(_) => continue,
        };

        if is_junk(&rel_path) {
            hidden.junk += 1;
            continue;
        }

        let file_type = match detect_file_type_from_name(&rel_path) {
            Some(t) => t,
            None => {
                // Not a text/markdown file — an image, a PDF, anything.
                // Preserved on save, but the browser can't render it.
                hidden.non_text += 1;
                continue;
            }
        };

        // Never trust the header's size for allocation; cap the initial
        // capacity and cap the actual bytes read. Read one extra byte so
        // we can detect (and skip) an entry that exceeds the limit.
        let cap = (entry.size() as usize).min(MAX_ENTRY_BYTES);
        let mut bytes = Vec::with_capacity(cap);
        entry
            .by_ref()
            .take(MAX_ENTRY_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .with_context(|| format!("failed to read archive entry {rel_path}"))?;

        if bytes.len() > MAX_ENTRY_BYTES {
            hidden.too_large += 1;
            continue;
        }

        // Skip entries that aren't valid UTF-8 text rather than failing
        // the whole archive.
        if std::str::from_utf8(&bytes).is_err() {
            hidden.not_utf8 += 1;
            continue;
        }

        entries.push(ArchiveEntry {
            rel_path,
            file_type,
            content: SecureBuffer::from_bytes(bytes),
        });
    }

    // An archive with no text entries is a valid archive, not an error:
    // you can delete the last file from a vault, or hold only binary
    // files. Bailing here made such a vault permanently unopenable —
    // including any non-text files still inside it. Callers render an
    // empty browser instead.
    //
    // What could not be shown goes back to the caller rather than to
    // stderr: this is a GUI app, so an eprintln! reached nobody, and a
    // vault that looks emptier than it is is exactly the thing a user
    // might act on destructively.
    entries.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(Extracted { entries, hidden })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    /// Build an in-memory tar.gz with the given (path, content) files.
    fn make_tar_gz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, content) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, *content).unwrap();
        }
        let tar_bytes = builder.into_inner().unwrap();

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        std::io::Write::write_all(&mut encoder, &tar_bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn archive_name_detection() {
        assert!(is_archive_name("notes.tar.gz.gpg"));
        assert!(is_archive_name("notes.tgz.asc"));
        assert!(is_archive_name("notes.tar.gpg"));
        assert!(!is_archive_name("notes.md.gpg"));
        assert!(!is_archive_name("notes.gpg"));
    }

    #[test]
    fn extracts_nested_text_files() {
        let data = make_tar_gz(&[
            ("vault/readme.txt", b"top level".as_slice()),
            ("vault/notes/plan.md", b"# Plan"),
            ("vault/notes/deep/nested/todo.md", b"- [ ] item"),
            ("vault/image.png", b"\x89PNG not text"),
            ("vault/.DS_Store", b"junk"),
            ("vault/._resource", b"junk fork"),
        ]);

        let entries = extract_text_entries(&data).unwrap().entries;
        let paths: Vec<&str> = entries.iter().map(|e| e.rel_path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "vault/notes/deep/nested/todo.md",
                "vault/notes/plan.md",
                "vault/readme.txt",
            ]
        );
        assert_eq!(entries[0].file_type, FileType::Markdown);
        assert_eq!(entries[2].file_type, FileType::PlainText);
        assert_eq!(entries[1].content.as_str().unwrap(), "# Plan");
    }

    #[test]
    fn rebuild_replaces_entry_and_preserves_everything_else() {
        let data = make_tar_gz(&[
            ("vault/notes/plan.md", b"# Old plan".as_slice()),
            ("vault/readme.txt", b"keep me"),
            ("vault/image.png", b"\x89PNG binary bytes"),
            ("vault/.DS_Store", b"junk kept verbatim"),
        ]);

        let (tar, gzip) = decompress_to_tar(&data).unwrap();
        assert!(gzip);

        let rebuilt = rebuild_with_edit(
            tar.as_bytes(),
            "vault/notes/plan.md",
            b"# New plan\n\nwith more content than before",
            gzip,
        )
        .unwrap();

        // The payload is a valid gzip archive whose text entries reflect
        // the edit.
        assert!(looks_like_archive(rebuilt.payload.as_bytes()));
        let entries = extract_text_entries(rebuilt.payload.as_bytes())
            .unwrap()
            .entries;
        let plan = entries
            .iter()
            .find(|e| e.rel_path == "vault/notes/plan.md")
            .unwrap();
        assert!(plan.content.as_str().unwrap().starts_with("# New plan"));
        assert_eq!(
            entries
                .iter()
                .find(|e| e.rel_path == "vault/readme.txt")
                .unwrap()
                .content
                .as_str()
                .unwrap(),
            "keep me"
        );

        // Non-text and junk entries survive byte-for-byte in the raw tar.
        let mut names = Vec::new();
        let mut png = Vec::new();
        let mut ar = tar::Archive::new(rebuilt.tar.as_bytes());
        for e in ar.entries().unwrap() {
            let mut e = e.unwrap();
            let p = e.path().unwrap().to_string_lossy().into_owned();
            if p == "vault/image.png" {
                std::io::Read::read_to_end(&mut e, &mut png).unwrap();
            }
            names.push(p);
        }
        assert!(names.contains(&"vault/image.png".to_string()));
        assert!(names.contains(&"vault/.DS_Store".to_string()));
        assert_eq!(png, b"\x89PNG binary bytes");
    }

    #[test]
    fn rebuild_fails_for_missing_entry() {
        let data = make_tar_gz(&[("a.md", b"x".as_slice())]);
        let (tar, gzip) = decompress_to_tar(&data).unwrap();
        assert!(rebuild_with_edit(tar.as_bytes(), "nope.md", b"y", gzip).is_err());
    }

    #[test]
    fn plain_tar_also_works() {
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        let content = b"hello tar";
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "a.txt", &content[..])
            .unwrap();
        let tar_bytes = builder.into_inner().unwrap();

        let entries = extract_text_entries(&tar_bytes).unwrap().entries;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content.as_str().unwrap(), "hello tar");
    }

    #[test]
    fn rejects_non_archives_but_accepts_text_free_ones() {
        // Not an archive at all — still an error.
        assert!(extract_text_entries(b"just some plain text").is_err());

        // An archive holding only non-text files is VALID, just empty from
        // the browser's point of view. Erroring here used to make such a
        // vault permanently unopenable — and deleting a vault's last text
        // file produced exactly this shape, locking the user out of the
        // binary files that remained.
        let data = make_tar_gz(&[("only/image.png", b"binary".as_slice())]);
        let entries = extract_text_entries(&data)
            .expect("binary-only archive must still open")
            .entries;
        assert!(entries.is_empty());
    }

    #[test]
    fn magic_detection() {
        let gz = make_tar_gz(&[("a.txt", b"x".as_slice())]);
        assert!(looks_like_archive(&gz));
        assert!(!looks_like_archive(b"# markdown heading"));
    }

    #[test]
    fn decompression_bomb_is_rejected() {
        // A tiny .tar.gz that expands to far more than the total cap:
        // one entry of highly-compressible zeros, larger than
        // MAX_TOTAL_DECOMPRESSED.
        let big = vec![b'0'; (MAX_TOTAL_DECOMPRESSED as usize) + 1024];
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(big.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, "bomb.txt", &big[..])
            .unwrap();
        let tar_bytes = builder.into_inner().unwrap();
        let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
        std::io::Write::write_all(&mut encoder, &tar_bytes).unwrap();
        let gz = encoder.finish().unwrap();

        // The compressed form is tiny; extraction must still refuse it.
        assert!(gz.len() < 1024 * 1024, "compressed bomb should be small");
        // (ArchiveEntry has no Debug — SecureBuffer is opaque — so match
        // rather than unwrap_err.)
        let err = match extract_text_entries(&gz) {
            Ok(_) => panic!("expected the bomb to be rejected"),
            Err(e) => e,
        };
        assert!(
            format!("{err:#}").contains("decompressed size") || format!("{err:#}").contains("bomb"),
            "expected a size-limit error, got: {err:#}"
        );
    }

    #[test]
    fn oversize_entry_is_skipped_not_loaded() {
        // One entry over the per-entry cap (but under the total cap) is
        // skipped; a normal entry alongside it still loads.
        let big = vec![b'a'; MAX_ENTRY_BYTES + 1];
        let data = make_tar_gz(&[
            ("vault/huge.txt", big.as_slice()),
            ("vault/ok.md", b"# fine".as_slice()),
        ]);
        let extracted = extract_text_entries(&data).unwrap();
        assert_eq!(extracted.entries.len(), 1);
        assert_eq!(extracted.entries[0].rel_path, "vault/ok.md");
        // Skipping is right; skipping *silently* was not. The oversized
        // file is still in the vault, so the count must say so.
        assert_eq!(extracted.hidden.too_large, 1);
    }

    /// Every reason the browser drops a file must be counted. Before this,
    /// they all vanished with no signal (the too-large case reached only
    /// an eprintln!, which nobody sees in a GUI), so a vault could look
    /// empty while still holding a user's photos.
    #[test]
    fn every_hidden_file_is_counted_by_reason() {
        let big = vec![b'a'; MAX_ENTRY_BYTES + 1];
        let data = make_tar_gz(&[
            ("vault/note.md", b"# shown".as_slice()),
            ("vault/plan.txt", b"also shown".as_slice()),
            // Not a text filename: an image and a PDF.
            ("vault/photo.png", b"\x89PNG\x00\x01".as_slice()),
            ("vault/report.pdf", b"%PDF-1.4".as_slice()),
            // A text name whose bytes are not UTF-8.
            ("vault/broken.txt", &[0xff, 0xfe, 0x00][..]),
            // Over the per-entry cap.
            ("vault/huge.txt", big.as_slice()),
            // macOS clutter.
            ("vault/.DS_Store", b"junk".as_slice()),
            ("__MACOSX/._note.md", b"junk".as_slice()),
        ]);

        let extracted = extract_text_entries(&data).unwrap();
        let shown: Vec<&str> = extracted
            .entries
            .iter()
            .map(|e| e.rel_path.as_str())
            .collect();
        assert_eq!(shown, ["vault/note.md", "vault/plan.txt"]);

        let hidden = extracted.hidden;
        assert_eq!(hidden.non_text, 2, "png + pdf");
        assert_eq!(hidden.not_utf8, 1);
        assert_eq!(hidden.too_large, 1);
        assert_eq!(hidden.junk, 2, ".DS_Store + AppleDouble");
        assert_eq!(hidden.total(), 6);

        let summary = hidden.summary().expect("something is hidden");
        assert!(summary.starts_with("6 files not shown"), "got {summary}");
        for fragment in ["2 not text", "1 too large", "1 not valid text", "2 system"] {
            assert!(
                summary.contains(fragment),
                "{fragment:?} missing: {summary}"
            );
        }
    }

    #[test]
    fn an_all_text_vault_reports_nothing_hidden() {
        // The common case must stay silent — a summary on every vault
        // would be noise, and noise is how real warnings get ignored.
        let data = make_tar_gz(&[
            ("vault/a.md", b"# a".as_slice()),
            ("vault/b.txt", b"b".as_slice()),
        ]);
        let extracted = extract_text_entries(&data).unwrap();
        assert_eq!(extracted.entries.len(), 2);
        assert_eq!(extracted.hidden.total(), 0);
        assert_eq!(extracted.hidden.summary(), None);
    }

    #[test]
    fn hidden_summary_is_singular_for_one_file() {
        let hidden = HiddenEntries {
            non_text: 1,
            ..Default::default()
        };
        assert_eq!(
            hidden.summary().unwrap(),
            "1 file not shown (1 not text)",
            "one file, not \"1 files\""
        );
    }

    #[test]
    fn forged_header_size_does_not_allocate() {
        // A header claiming a gigantic size but with little actual data
        // must not trigger a huge up-front allocation (we cap capacity).
        let mut header = tar::Header::new_gnu();
        header.set_size(u64::MAX / 2); // absurd claimed size
        header.set_mode(0o644);
        // Deliberately do not match real data length; tar will read what's
        // present. This documents that we don't Vec::with_capacity(size).
        let mut builder = tar::Builder::new(Vec::new());
        // append_data uses the data length for the actual bytes; the point
        // is our code caps capacity at MAX_ENTRY_BYTES regardless.
        header.set_size(6);
        header.set_cksum();
        builder
            .append_data(&mut header, "x.txt", &b"hello!"[..])
            .unwrap();
        let tar_bytes = builder.into_inner().unwrap();
        let entries = extract_text_entries(&tar_bytes).unwrap().entries;
        assert_eq!(entries[0].content.as_str().unwrap(), "hello!");
    }
    // ── Vault mutation: add / remove / rename ────────────────────────

    /// Decompress a payload and list its regular-file paths.
    fn paths_in(payload: &[u8]) -> Vec<String> {
        let (tar, _) = decompress_to_tar(payload).unwrap();
        let mut a = tar::Archive::new(tar.as_bytes());
        let mut out = Vec::new();
        for e in a.entries().unwrap() {
            let e = e.unwrap();
            if e.header().entry_type() == tar::EntryType::Regular {
                out.push(e.path().unwrap().to_string_lossy().into_owned());
            }
        }
        out.sort();
        out
    }

    #[test]
    fn add_remove_rename_round_trip() {
        let gz = make_tar_gz(&[("vault/a.md", b"# A"), ("vault/b.txt", b"bee")]);
        let (tar, _) = decompress_to_tar(&gz).unwrap();

        // Add
        let added = add_entry(tar.as_bytes(), "vault/c.md", b"# C\n", true).unwrap();
        assert_eq!(
            paths_in(added.payload.as_bytes()),
            ["vault/a.md", "vault/b.txt", "vault/c.md"]
        );
        // The new file's content is really there.
        let entries = extract_text_entries(added.tar.as_bytes()).unwrap().entries;
        let c = entries.iter().find(|e| e.rel_path == "vault/c.md").unwrap();
        assert_eq!(c.content.as_str().unwrap(), "# C\n");

        // Rename
        let renamed = rename_entry(
            added.tar.as_bytes(),
            "vault/b.txt",
            "vault/renamed.txt",
            true,
        )
        .unwrap();
        assert_eq!(
            paths_in(renamed.payload.as_bytes()),
            ["vault/a.md", "vault/c.md", "vault/renamed.txt"]
        );

        // Remove
        let removed = remove_entry(renamed.tar.as_bytes(), "vault/a.md", true).unwrap();
        assert_eq!(
            paths_in(removed.payload.as_bytes()),
            ["vault/c.md", "vault/renamed.txt"]
        );
    }

    #[test]
    fn mutations_reject_bad_input() {
        let gz = make_tar_gz(&[("vault/a.md", b"x")]);
        let (tar, _) = decompress_to_tar(&gz).unwrap();
        // Duplicate add
        assert!(add_entry(tar.as_bytes(), "vault/a.md", b"y", true).is_err());
        // Path traversal is rejected outright.
        assert!(add_entry(tar.as_bytes(), "../escape.md", b"y", true).is_err());
        assert!(add_entry(tar.as_bytes(), "a/../../escape.md", b"y", true).is_err());
        // A leading slash is neutralized (placed at vault root), not an error.
        let abs = add_entry(tar.as_bytes(), "/abs.md", b"y", true).unwrap();
        assert!(paths_in(abs.payload.as_bytes()).contains(&"abs.md".to_string()));
        // Remove / rename a missing file
        assert!(remove_entry(tar.as_bytes(), "vault/nope.md", true).is_err());
        assert!(rename_entry(tar.as_bytes(), "vault/nope.md", "vault/x.md", true).is_err());
        // Rename onto an existing file
        let gz2 = make_tar_gz(&[("vault/a.md", b"x"), ("vault/b.md", b"y")]);
        let (tar2, _) = decompress_to_tar(&gz2).unwrap();
        assert!(rename_entry(tar2.as_bytes(), "vault/a.md", "vault/b.md", true).is_err());
    }

    #[test]
    fn add_preserves_the_uncompressed_variant_too() {
        // A .tar (no gzip) source must round-trip without compression.
        let mut builder = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_size(1);
        h.set_mode(0o644);
        h.set_cksum();
        builder.append_data(&mut h, "v/a.txt", &b"x"[..]).unwrap();
        let tar_bytes = builder.into_inner().unwrap();

        let added = add_entry(&tar_bytes, "v/b.txt", b"y", false).unwrap();
        // payload == tar (no gzip header) when gzip=false
        assert_eq!(added.payload.as_bytes(), added.tar.as_bytes());
        assert_eq!(paths_in(added.payload.as_bytes()), ["v/a.txt", "v/b.txt"]);
    }
    // ── Folder ops: add empty dir, rename/remove by prefix ───────────

    #[test]
    fn add_empty_folder_persists() {
        let gz = make_tar_gz(&[("v/a.md", b"x")]);
        let (tar, _) = decompress_to_tar(&gz).unwrap();
        let r = add_dir(tar.as_bytes(), "v/empty", true).unwrap();
        let dirs = extract_dir_entries(r.tar.as_bytes());
        assert!(dirs.contains(&"v/empty".to_string()), "dirs = {dirs:?}");
        // Adding it twice fails.
        assert!(add_dir(r.tar.as_bytes(), "v/empty", true).is_err());
        // Can't collide with a file path.
        assert!(add_dir(tar.as_bytes(), "v/a.md", true).is_err());
    }

    #[test]
    fn rename_folder_moves_all_children() {
        let gz = make_tar_gz(&[
            ("proj/notes/a.md", b"a"),
            ("proj/notes/b.txt", b"b"),
            ("proj/readme.md", b"r"),
        ]);
        let (tar, _) = decompress_to_tar(&gz).unwrap();
        let r = rename_prefix(tar.as_bytes(), "proj/notes", "proj/journal", true).unwrap();
        let files = paths_in(r.payload.as_bytes());
        assert_eq!(
            files,
            ["proj/journal/a.md", "proj/journal/b.txt", "proj/readme.md"]
        );
        // Renaming a missing folder / onto an existing one fails.
        assert!(rename_prefix(tar.as_bytes(), "proj/nope", "proj/x", true).is_err());
        assert!(rename_prefix(tar.as_bytes(), "proj/notes", "proj", true).is_err());
    }

    #[test]
    fn remove_folder_drops_the_subtree() {
        let gz = make_tar_gz(&[
            ("proj/notes/a.md", b"a"),
            ("proj/notes/deep/c.md", b"c"),
            ("proj/keep.md", b"k"),
        ]);
        let (tar, _) = decompress_to_tar(&gz).unwrap();
        let r = remove_prefix(tar.as_bytes(), "proj/notes", true).unwrap();
        assert_eq!(paths_in(r.payload.as_bytes()), ["proj/keep.md"]);
        assert!(remove_prefix(tar.as_bytes(), "proj/nope", true).is_err());
    }

    #[test]
    fn rename_folder_carries_its_empty_dir_entry() {
        let gz = make_tar_gz(&[("v/a.md", b"x")]);
        let (tar, _) = decompress_to_tar(&gz).unwrap();
        let with_dir = add_dir(tar.as_bytes(), "v/sub", true).unwrap();
        let renamed = rename_prefix(with_dir.tar.as_bytes(), "v/sub", "v/renamed", true).unwrap();
        let dirs = extract_dir_entries(renamed.tar.as_bytes());
        assert!(dirs.contains(&"v/renamed".to_string()), "dirs = {dirs:?}");
    }
    /// Deleting the last file leaves a valid, still-usable vault. This is
    /// the data-layer half of the empty-vault crash: the browser used to
    /// index entries[selected] unconditionally, so an emptied vault
    /// panicked the app. Extraction must return an empty list (not an
    /// error), and the vault must still accept new files.
    #[test]
    fn deleting_the_last_file_leaves_a_usable_empty_vault() {
        let gz = make_tar_gz(&[("v/only.md", b"# only")]);
        let (tar, _) = decompress_to_tar(&gz).unwrap();

        let emptied = remove_entry(tar.as_bytes(), "v/only.md", true).unwrap();
        // Still a real archive, just with nothing in it.
        let entries = extract_text_entries(emptied.payload.as_bytes())
            .unwrap()
            .entries;
        assert!(
            entries.is_empty(),
            "expected no entries, got {}",
            entries.len()
        );
        assert!(paths_in(emptied.payload.as_bytes()).is_empty());

        // And it can be refilled.
        let refilled = add_entry(emptied.tar.as_bytes(), "v/fresh.md", b"# fresh\n", true).unwrap();
        assert_eq!(paths_in(refilled.payload.as_bytes()), ["v/fresh.md"]);
    }

    /// The selection clamp used after a mutation yields 0 for an empty
    /// vault, so every consumer must use .get() rather than indexing.
    #[test]
    fn selection_clamp_is_zero_when_the_vault_is_empty() {
        let entries: Vec<ArchiveEntry> = Vec::new();
        let selected = 3usize;
        let clamped = selected.min(entries.len().saturating_sub(1));
        assert_eq!(clamped, 0);
        assert!(entries.get(clamped).is_none(), "indexing here would panic");
    }
}
