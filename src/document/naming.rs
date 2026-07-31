//! Filename reasoning: encrypted-extension handling, vault path leaves,
//! and format detection by name.
//!
//! Pure string/path logic with no UI and no I/O, split out of `app.rs` so
//! it can be tested directly. Every one of these decides what a *file on
//! disk* is called or which backend opens it, which is exactly the kind
//! of thing that fails quietly — an earlier bug here stacked `.age.age`
//! onto re-encrypted files — so each rule below has a test.

use std::path::Path;

/// Encrypted-container extensions Schl8 writes or reads. Order matters
/// only for stripping, and each is stripped at most once.
const ENCRYPTED_EXTENSIONS: [&str; 3] = [".gpg", ".asc", ".age"];

/// Give a bare vault filename the right extension; a name that already
/// has one is left untouched. `notes/plan` + markdown → `notes/plan.md`.
pub fn ensure_text_extension(rel: &str, markdown: bool) -> String {
    let leaf = rel.rsplit('/').next().unwrap_or(rel);
    if leaf.contains('.') {
        rel.to_string()
    } else if markdown {
        format!("{rel}.md")
    } else {
        format!("{rel}.txt")
    }
}

/// The file-name stem of a vault path, for a starter markdown heading.
pub fn leaf_stem(rel: &str) -> String {
    let leaf = rel.rsplit('/').next().unwrap_or(rel);
    leaf.rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(leaf)
        .to_string()
}

/// Derive a suggested encrypted filename from `source_path` using the
/// chosen extension, e.g. (`document.md.gpg`, `asc`) → `document.md.asc`.
///
/// The outer encrypted extension is stripped first so re-encrypting an
/// already-encrypted file — including age → age — doesn't stack
/// extensions onto the name.
pub fn suggest_encrypted_name(source_path: &Path, ext: &str) -> String {
    let name = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("encrypted");

    let base = ENCRYPTED_EXTENSIONS
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))
        .unwrap_or(name);

    format!("{base}.{ext}")
}

/// Whether a document's source is an age file (by `.age` extension). New
/// age documents carry a `.age` placeholder name, so this also identifies
/// unsaved age notes.
pub fn is_age_source(source_path: &Path) -> bool {
    source_path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".age"))
}

/// Whether the source file uses binary (`.gpg`) format rather than
/// ASCII armor.
pub fn source_is_binary(source_path: &Path) -> bool {
    source_path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".gpg"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_name_swaps_encrypted_extension() {
        assert_eq!(
            suggest_encrypted_name(Path::new("/tmp/document.md.gpg"), "asc"),
            "document.md.asc"
        );
        assert_eq!(
            suggest_encrypted_name(Path::new("plan.asc"), "gpg"),
            "plan.gpg"
        );
        assert_eq!(
            suggest_encrypted_name(Path::new("notes.txt"), "gpg"),
            "notes.txt.gpg"
        );
        // Re-encrypting an age file must not stack extensions — this was
        // a real bug, producing `.age.age`.
        assert_eq!(
            suggest_encrypted_name(Path::new("/tmp/test.md.age"), "age"),
            "test.md.age"
        );
        assert_eq!(
            suggest_encrypted_name(Path::new("test.md.age"), "gpg"),
            "test.md.gpg"
        );
    }

    #[test]
    fn suggested_name_strips_only_the_outer_container() {
        // The inner extension is part of the document's identity and must
        // survive: `.md.gpg` → `.md.age`, never bare `.age`.
        assert_eq!(
            suggest_encrypted_name(Path::new("notes.md.gpg"), "age"),
            "notes.md.age"
        );
        // Only one container is stripped, so a doubled name loses exactly
        // one layer rather than being flattened.
        assert_eq!(
            suggest_encrypted_name(Path::new("odd.age.age"), "age"),
            "odd.age.age"
        );
        // A path with no filename at all still yields something usable.
        assert_eq!(
            suggest_encrypted_name(Path::new("/"), "gpg"),
            "encrypted.gpg"
        );
    }

    #[test]
    fn text_extension_is_added_only_when_missing() {
        assert_eq!(ensure_text_extension("notes/plan", true), "notes/plan.md");
        assert_eq!(ensure_text_extension("notes/plan", false), "notes/plan.txt");
        // Already extended: left exactly as-is, whichever type is asked.
        assert_eq!(
            ensure_text_extension("notes/plan.txt", true),
            "notes/plan.txt"
        );
        assert_eq!(ensure_text_extension("a.md", false), "a.md");
        // A dot in a *directory* name is not an extension on the leaf.
        assert_eq!(ensure_text_extension("v1.0/plan", true), "v1.0/plan.md");
    }

    #[test]
    fn leaf_stem_takes_the_last_component_without_its_extension() {
        assert_eq!(leaf_stem("notes/2026/plan.md"), "plan");
        assert_eq!(leaf_stem("plan.md"), "plan");
        assert_eq!(leaf_stem("plan"), "plan");
        // Multiple dots: only the final extension is dropped.
        assert_eq!(leaf_stem("archive.tar.gz"), "archive.tar");
        assert_eq!(leaf_stem("notes/README"), "README");
    }

    #[test]
    fn format_detection_by_name() {
        assert!(is_age_source(Path::new("a.md.age")));
        assert!(is_age_source(Path::new("/long/path/untitled.age")));
        assert!(!is_age_source(Path::new("a.md.gpg")));
        // `.age` must be the *suffix*, not merely present.
        assert!(!is_age_source(Path::new("age.md.gpg")));

        assert!(source_is_binary(Path::new("a.md.gpg")));
        assert!(!source_is_binary(Path::new("a.md.asc")));
        assert!(!source_is_binary(Path::new("a.md.age")));
    }
}
