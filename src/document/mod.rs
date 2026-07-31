pub mod append;
pub mod archive;
pub mod loader;
pub mod markdown;
pub mod multisave;
pub mod naming;
pub mod spool;
pub mod stash;
pub mod stats;

use std::path::PathBuf;

use crate::crypto::secure_buf::SecureBuffer;

/// The type of document based on file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    PlainText,
    Markdown,
}

/// A loaded, decrypted document held entirely in secure memory.
pub struct Document {
    pub content: SecureBuffer,
    pub file_type: FileType,
    pub source_path: PathBuf,
    /// Recipient key IDs the source file was encrypted to, when known.
    /// `Some` enables in-place Save (re-encrypt to the same keys);
    /// `None` for new documents and files opened as plaintext.
    pub recipients: Option<Vec<String>>,
    /// Authenticity of the file's signature, if any.
    pub signature: crate::crypto::gpg::SignatureStatus,
}

/// Result of decrypting a file: a single document, or a folder archive
/// containing multiple text files.
pub enum LoadedDocument {
    Single(Document),
    Archive(archive::ArchiveDocument),
}

// Decrypted documents are produced on the background decrypt thread and
// handed to the UI over an mpsc channel — this whole payload must stay
// Send. If a future field (e.g. something holding a raw pointer or an Rc)
// broke that, the failure would otherwise surface as a confusing trait
// error at the channel; fail clearly here instead.
static_assertions::assert_impl_all!(LoadedDocument: Send);

/// Classify a plain (non-encrypted) filename as markdown or text.
/// Returns None for anything that isn't a recognized text format.
pub fn detect_file_type_from_name(name: &str) -> Option<FileType> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".md") || lower.ends_with(".markdown") {
        Some(FileType::Markdown)
    } else if lower.ends_with(".txt") || lower.ends_with(".text") {
        Some(FileType::PlainText)
    } else {
        None
    }
}
