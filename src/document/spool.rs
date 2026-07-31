//! Offline append: pending note entries written without the private key.
//!
//! Appending normally means decrypt → concatenate → re-encrypt, which
//! needs the private key. Encrypting to a recipient does not. So when the
//! identity is locked, an entry is encrypted on its own and dropped into a
//! *spool* beside the note; the next unlocked session merges the spool
//! into the note and deletes it.
//!
//! Every file involved — the note and each pending segment — stays an
//! ordinary standalone age/GPG file readable by standard tooling. See
//! `docs/SPOOL-DESIGN.md` for the format and the reasoning; the segment
//! envelope below is frozen as v1.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

/// First line of every segment's plaintext. Frozen.
const MAGIC: &str = "schl8-spool/v1";
/// Header carrying the RFC 3339 UTC write time — the ordering key.
const HDR_WRITTEN: &str = "written";

/// Which backend a pending segment is encrypted with.
///
/// The spool is deliberately backend-agnostic: a segment is an ordinary
/// standalone age or GPG file, and its extension records which, so a merge
/// knows how to open it without a manifest or any other state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentFormat {
    Age,
    Gpg,
}

impl SegmentFormat {
    /// The segment filename extension for this backend.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Age => "age",
            Self::Gpg => "gpg",
        }
    }

    /// Recognize a segment by extension. Anything else in the spool
    /// directory is not ours and is left strictly alone.
    fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "age" => Some(Self::Age),
            "gpg" => Some(Self::Gpg),
            _ => None,
        }
    }
}

/// A decrypted pending entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// RFC 3339 UTC timestamp from the envelope.
    pub written: String,
    /// The entry text, byte-for-byte as it was handed to `envelope`.
    pub body: String,
    /// File the segment was read from (tie-breaks equal timestamps).
    pub path: PathBuf,
}

/// Directory holding pending segments for `note`.
///
/// Dot-prefixed so it stays out of the way in Finder, and a sibling so it
/// travels with the note when copied, synced, or backed up.
pub fn spool_dir(note: &Path) -> PathBuf {
    let name = note.file_name().and_then(|n| n.to_str()).unwrap_or("note");
    note.with_file_name(format!(".{name}.spool"))
}

/// Wrap `body` in the frozen v1 envelope. `written` must be RFC 3339 UTC.
///
/// The body is copied verbatim after the blank line — never re-wrapped or
/// re-encoded — so a merge reproduces exactly what was typed.
pub fn envelope(written: &str, body: &str) -> String {
    format!("{MAGIC}\n{HDR_WRITTEN}: {written}\n\n{body}")
}

/// Like [`envelope`], with a `source:` header naming the surface that
/// wrote the entry ("cli", "mcp", …). v1 readers ignore unknown headers,
/// so this is forward-compatible; it exists so provenance of agent
/// entries is recorded from day one (AGENT-DESIGN invariant 4).
pub fn envelope_from(written: &str, source: &str, body: &str) -> String {
    format!("{MAGIC}\n{HDR_WRITTEN}: {written}\nsource: {source}\n\n{body}")
}

/// Parse a decrypted segment. Unknown headers are ignored so later
/// versions can add fields without breaking v1 readers.
pub fn parse_envelope(plaintext: &str, path: &Path) -> Result<Segment> {
    let mut lines = plaintext.split_inclusive('\n');
    let first = lines
        .next()
        .ok_or_else(|| anyhow!("empty spool segment"))?
        .trim_end_matches(['\n', '\r']);
    if first != MAGIC {
        return Err(anyhow!(
            "not a Schl8 spool segment (expected {MAGIC:?}, got {first:?})"
        ));
    }

    let mut written = None;
    let mut consumed = first.len();
    // Account for the newline the magic line ended with.
    consumed += plaintext[consumed..].starts_with("\r\n") as usize * 2
        + plaintext[consumed..].starts_with('\n') as usize;

    for line in lines {
        consumed += line.len();
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.is_empty() {
            // Blank line ends the header block; the body is the rest.
            let body = plaintext[consumed..].to_string();
            let written =
                written.ok_or_else(|| anyhow!("spool segment has no `{HDR_WRITTEN}` header"))?;
            return Ok(Segment {
                written,
                body,
                path: path.to_path_buf(),
            });
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            if key.trim() == HDR_WRITTEN {
                written = Some(value.trim().to_string());
            }
            // Any other key is ignored on purpose (forward compatibility).
        }
    }
    Err(anyhow!("spool segment has no blank line after its headers"))
}

/// Paths of the pending segments for `note`, in filesystem order.
///
/// Cheap and key-free: counting pending entries never decrypts anything,
/// so the UI can show a badge while locked.
pub fn segment_paths(note: &Path) -> Vec<PathBuf> {
    let dir = spool_dir(note);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && segment_format(p).is_some())
        .collect();
    out.sort();
    out
}

/// The backend a segment path is encrypted with, or None if the file is
/// not a segment at all.
pub fn segment_format(path: &Path) -> Option<SegmentFormat> {
    path.extension()
        .and_then(|x| x.to_str())
        .and_then(SegmentFormat::from_extension)
}

/// How many entries are waiting to be merged into `note`.
pub fn pending_count(note: &Path) -> usize {
    segment_paths(note).len()
}

/// How many pending entries for `note` use a particular backend. Lets a
/// merge tell "nothing to do" apart from "waiting on a locked identity".
pub fn pending_count_of(note: &Path, format: SegmentFormat) -> usize {
    segment_paths(note)
        .iter()
        .filter(|p| segment_format(p) == Some(format))
        .count()
}

/// Encrypt one segment envelope for a quicknote's save rules, using the
/// backend that note's own plan already uses.
///
/// age is preferred when the plan has an age rule: appending to an age
/// note needs the identity at merge time anyway, so an age segment adds no
/// new unlock. A GPG-only note gets a GPG segment — writing one needs no
/// private key either, so a hardware key stays untouched until the merge.
pub fn encrypt_segment(
    rules: &[crate::config::SaveRule],
    plaintext: &[u8],
) -> Result<(Vec<u8>, SegmentFormat)> {
    let age: Vec<&str> = rules
        .iter()
        .filter(|r| r.is_age())
        .map(|r| r.age_recipient.as_str())
        .collect();
    if !age.is_empty() {
        let ct = crate::crypto::age_backend::encrypt_to_recipients(plaintext, &age)?;
        return Ok((ct, SegmentFormat::Age));
    }

    let gpg: Vec<&str> = rules
        .iter()
        .filter(|r| !r.key_fingerprint.is_empty())
        .map(|r| r.key_fingerprint.as_str())
        .collect();
    if !gpg.is_empty() {
        let ct = crate::crypto::keys::encrypt_to_bytes(plaintext, &gpg, false)?;
        return Ok((ct, SegmentFormat::Gpg));
    }

    Err(anyhow!(
        "this quicknote has no encryption key to write an offline entry to \
         \u{2014} open Quick Note Files\u{2026} in Schl8 and give it an AGE \
         recipient or a GPG key"
    ))
}

/// Fraction of the cap at which callers should start warning the user.
/// Four fifths: late enough not to nag over ordinary use, early enough
/// that there is room to act before entries start being refused.
const NAG_NUMERATOR: usize = 4;
const NAG_DENOMINATOR: usize = 5;

/// Whether `pending` is far enough into the cap to be worth mentioning.
pub fn should_nag(pending: usize, max: usize) -> bool {
    max > 0 && pending * NAG_DENOMINATOR >= max * NAG_NUMERATOR
}

/// Write one encrypted segment into `note`'s spool.
///
/// `ciphertext` must already be the encrypted [`envelope`]. The filename
/// is random hex and carries no ordering or timing information — a
/// directory listing reveals how many entries are pending, never when
/// they were written.
///
/// `max_pending` bounds the spool (0 disables it). The check lives here,
/// at the one place that creates segment files, so no caller can grow a
/// spool without passing the cap. Refusing is deliberate: the caller
/// still has somewhere to go (the app falls back to asking for the seed
/// phrase), whereas an unbounded spool has no recovery at all.
pub fn write_segment(
    note: &Path,
    ciphertext: &[u8],
    format: SegmentFormat,
    max_pending: usize,
) -> Result<PathBuf> {
    if max_pending > 0 {
        let pending = pending_count(note);
        if pending >= max_pending {
            return Err(anyhow!(
                "this note already has {pending} pending offline entries (the limit is \
                 {max_pending}) \u{2014} merge them in Schl8 before adding more"
            ));
        }
    }
    let dir = spool_dir(note);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating spool {}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }

    let mut name = [0u8; 16];
    getrandom::getrandom(&mut name).map_err(|e| anyhow!("OS randomness unavailable: {e}"))?;
    let path = dir.join(format!(
        "{}.{}",
        name.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        format.extension()
    ));
    crate::crypto::keys::atomic_write(&path, ciphertext)
        .with_context(|| format!("writing spool segment {}", path.display()))?;
    Ok(path)
}

/// Decrypt every pending segment for `note`, ordered ready to append.
///
/// Each segment is opened with the backend its extension names: age
/// segments need `identity` (pass None when it is still locked — those
/// segments are then simply reported as unreadable and left for a later
/// session), GPG segments go through gpg-agent.
///
/// A segment that fails to decrypt or parse is **left in place** and
/// reported, never dropped: it may belong to another key, or be a stray
/// file someone put in the directory. Losing a note is worse than
/// carrying an unreadable one, so only what merged is deleted.
pub fn read_segments(
    note: &Path,
    identity: Option<&crate::crypto::age_backend::AgeIdentity>,
) -> (Vec<Segment>, Vec<(PathBuf, String)>) {
    let mut ok = Vec::new();
    let mut failed = Vec::new();
    for path in segment_paths(note) {
        let plaintext = match segment_format(&path) {
            Some(SegmentFormat::Age) => match identity {
                Some(id) => std::fs::read(&path)
                    .map_err(|e| format!("{e}"))
                    .and_then(|ct| id.decrypt(&ct).map_err(|e| format!("{e:#}"))),
                None => Err("AGE identity is locked".to_string()),
            },
            // GPG needs no in-process key material — gpg-agent holds it,
            // and prompts for a PIN or hardware touch if it must.
            Some(SegmentFormat::Gpg) => {
                crate::crypto::gpg::decrypt_file(&path).map_err(|e| format!("{e:#}"))
            }
            None => Err("not a spool segment".to_string()),
        };
        let read = plaintext.and_then(|buf| {
            buf.as_str()
                .map_err(|e| format!("{e:#}"))
                .and_then(|txt| parse_envelope(txt, &path).map_err(|e| format!("{e:#}")))
        });
        match read {
            Ok(seg) => ok.push(seg),
            Err(e) => failed.push((path, e)),
        }
    }
    sort_segments(&mut ok);
    (ok, failed)
}

/// The text to append to a note for `segments`, in order.
///
/// Bodies are joined verbatim — nothing is injected into the user's note.
/// The provenance mitigation from `docs/SPOOL-DESIGN.md` §7 (the spool
/// gives up the authentication that needing the key used to imply) is
/// surfaced by the caller at merge time, not written into the document.
pub fn merged_text(segments: &[Segment]) -> String {
    let mut out = String::new();
    for seg in segments {
        if !out.ends_with('\n') && !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&seg.body);
    }
    out
}

/// Order decrypted segments the way a merge must apply them: by write
/// time, then by path so equal timestamps stay deterministic.
pub fn sort_segments(segments: &mut [Segment]) {
    segments.sort_by(|a, b| a.written.cmp(&b.written).then_with(|| a.path.cmp(&b.path)));
}

/// Remove segments after their contents have been durably written into
/// the note. Called only once the merged note is on disk, so a crash
/// re-merges (at worst duplicating an entry) rather than losing one.
pub fn remove_segments(paths: &[PathBuf]) -> Result<()> {
    for p in paths {
        std::fs::remove_file(p).with_context(|| format!("removing {}", p.display()))?;
    }
    // Drop the directory too if nothing else landed in it meanwhile.
    if let Some(dir) = paths.first().and_then(|p| p.parent()) {
        let _ = std::fs::remove_dir(dir);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("schl8-spool-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn envelope_round_trips_body_verbatim() {
        let body = "line one\n\nline three with: a colon\nand a trailing newline\n";
        let raw = envelope("2026-07-22T09:15:03.123Z", body);
        let seg = parse_envelope(&raw, Path::new("/x/a.age")).unwrap();
        assert_eq!(seg.written, "2026-07-22T09:15:03.123Z");
        // Byte-for-byte: blank lines and colons inside the body survive.
        assert_eq!(seg.body, body);
    }

    #[test]
    fn empty_body_is_allowed() {
        let seg = parse_envelope(&envelope("2026-01-01T00:00:00Z", ""), Path::new("/x/a")).unwrap();
        assert_eq!(seg.body, "");
    }

    #[test]
    fn source_tagged_envelope_parses_like_a_plain_one() {
        // The CLI writes a `source:` header; v1 readers must treat it as
        // just another ignorable header and still merge the entry.
        let raw = envelope_from("2026-01-01T00:00:00Z", "cli", "from an agent\n");
        let seg = parse_envelope(&raw, Path::new("/x/a.age")).unwrap();
        assert_eq!(seg.written, "2026-01-01T00:00:00Z");
        assert_eq!(seg.body, "from an agent\n");
    }

    #[test]
    fn unknown_headers_are_ignored_for_forward_compatibility() {
        let raw = format!("{MAGIC}\nwritten: 2026-01-01T00:00:00Z\nfuture: whatever\n\nbody\n");
        let seg = parse_envelope(&raw, Path::new("/x/a")).unwrap();
        assert_eq!(seg.written, "2026-01-01T00:00:00Z");
        assert_eq!(seg.body, "body\n");
    }

    #[test]
    fn rejects_foreign_or_malformed_segments() {
        // Not ours at all — e.g. a stray age file dropped in the spool.
        assert!(parse_envelope("hello world\n", Path::new("/x/a")).is_err());
        // Right magic, but no timestamp to order by.
        assert!(parse_envelope(&format!("{MAGIC}\n\nbody"), Path::new("/x/a")).is_err());
        // Headers that never terminate.
        assert!(parse_envelope(&format!("{MAGIC}\nwritten: t\n"), Path::new("/x/a")).is_err());
        assert!(parse_envelope("", Path::new("/x/a")).is_err());
    }

    #[test]
    fn merge_order_is_by_timestamp_then_path() {
        let mut segs = vec![
            Segment {
                written: "2026-01-02T00:00:00Z".into(),
                body: "second".into(),
                path: "/z".into(),
            },
            Segment {
                written: "2026-01-01T00:00:00Z".into(),
                body: "first".into(),
                path: "/b".into(),
            },
            // Same instant as the one above — path breaks the tie.
            Segment {
                written: "2026-01-01T00:00:00Z".into(),
                body: "first-a".into(),
                path: "/a".into(),
            },
        ];
        sort_segments(&mut segs);
        let order: Vec<&str> = segs.iter().map(|s| s.body.as_str()).collect();
        assert_eq!(order, ["first-a", "first", "second"]);
    }

    #[test]
    fn spool_dir_is_a_hidden_sibling() {
        let d = spool_dir(Path::new("/notes/journal.md.age"));
        assert_eq!(d, PathBuf::from("/notes/.journal.md.age.spool"));
    }

    /// The whole point of the feature: write entries with only the public
    /// recipient (no private key anywhere), then read them back with the
    /// key and get them in order.
    #[test]
    fn spool_without_the_key_then_merge_with_it() {
        use crate::crypto::age_backend::{encrypt_to_recipients, AgeIdentity};
        let id = AgeIdentity::from_mnemonic(
            "legal winner thank year wave sausage worth useful legal winner thank yellow",
            "",
        )
        .unwrap();
        let recipient = id.recipient().to_string();

        let dir = tmp("e2e");
        let note = dir.join("journal.md.age");

        // ── Locked: only the public recipient is used. ──────────────
        for (when, body) in [
            ("2026-07-22T09:00:00Z", "second entry\n"),
            ("2026-07-22T08:00:00Z", "first entry\n"),
        ] {
            let ct = encrypt_to_recipients(envelope(when, body).as_bytes(), &[&recipient]).unwrap();
            write_segment(&note, &ct, SegmentFormat::Age, 0).unwrap();
        }
        assert_eq!(pending_count(&note), 2, "counting needs no key");

        // ── Unlocked: decrypt, order, merge. ────────────────────────
        let (segs, failed) = read_segments(&note, Some(&id));
        assert!(failed.is_empty(), "all segments readable: {failed:?}");
        assert_eq!(segs.len(), 2);
        // Written out of order; merged in timestamp order.
        assert_eq!(merged_text(&segs), "first entry\nsecond entry\n");

        let paths: Vec<PathBuf> = segs.iter().map(|s| s.path.clone()).collect();
        remove_segments(&paths).unwrap();
        assert_eq!(pending_count(&note), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An unreadable segment must never be silently dropped.
    #[test]
    fn unreadable_segments_are_reported_not_discarded() {
        use crate::crypto::age_backend::AgeIdentity;
        let id = AgeIdentity::from_mnemonic(
            "legal winner thank year wave sausage worth useful legal winner thank yellow",
            "",
        )
        .unwrap();
        let dir = tmp("bad");
        let note = dir.join("journal.md.age");
        write_segment(&note, b"not an age file at all", SegmentFormat::Age, 0).unwrap();

        let (ok, failed) = read_segments(&note, Some(&id));
        assert!(ok.is_empty());
        assert_eq!(failed.len(), 1, "reported");
        assert_eq!(pending_count(&note), 1, "still on disk, not deleted");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The spool is backend-agnostic: both kinds of segment are counted
    /// and listed, and each is tagged by its own extension.
    #[test]
    fn both_backends_are_recognized_and_counted_separately() {
        let dir = tmp("mixed");
        let note = dir.join("journal.md.gpg");

        let a = write_segment(&note, b"ct-age", SegmentFormat::Age, 0).unwrap();
        let b = write_segment(&note, b"ct-gpg", SegmentFormat::Gpg, 0).unwrap();
        assert!(a.to_str().unwrap().ends_with(".age"));
        assert!(b.to_str().unwrap().ends_with(".gpg"));

        assert_eq!(pending_count(&note), 2, "both kinds count as pending");
        assert_eq!(pending_count_of(&note, SegmentFormat::Age), 1);
        assert_eq!(pending_count_of(&note, SegmentFormat::Gpg), 1);

        // A stray file that is neither is not ours and is ignored.
        std::fs::write(spool_dir(&note).join("notes.txt"), b"junk").unwrap();
        assert_eq!(pending_count(&note), 2, "stray files are not segments");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The backend follows the note's own save plan, so a GPG-only
    /// quicknote can spool too — it used to be rejected outright, which
    /// left `schl8 append` with no way to write to such a note at all.
    #[test]
    fn backend_follows_the_notes_save_plan() {
        use crate::config::SaveRule;

        let age_rule = SaveRule {
            age_recipient: "age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p".into(),
            ..Default::default()
        };
        let (_, format) = encrypt_segment(std::slice::from_ref(&age_rule), b"body").unwrap();
        assert_eq!(format, SegmentFormat::Age);

        // A plan with both prefers age: merging an age note needs the
        // identity anyway, so an age segment costs no extra unlock.
        let gpg_rule = SaveRule {
            key_fingerprint: "0123456789ABCDEF0123456789ABCDEF01234567".into(),
            ..Default::default()
        };
        let (_, format) = encrypt_segment(&[gpg_rule, age_rule], b"body").unwrap();
        assert_eq!(format, SegmentFormat::Age);

        // A note with no key at all still cannot spool, but the error now
        // names both backends rather than demanding an AGE recipient.
        let err = format!("{:#}", encrypt_segment(&[], b"body").unwrap_err());
        assert!(err.contains("AGE"), "got {err}");
        assert!(err.contains("GPG"), "got {err}");
    }

    /// A note that is never unlocked used to accumulate segments forever,
    /// and the write-only CLI gave a looping agent an unbounded way to do
    /// it. The cap is enforced where segments are created, so no caller
    /// can grow a spool past it.
    #[test]
    fn the_spool_is_capped_and_refuses_rather_than_growing() {
        let dir = tmp("cap");
        let note = dir.join("journal.md.age");

        for _ in 0..3 {
            write_segment(&note, b"ct", SegmentFormat::Age, 3).unwrap();
        }
        assert_eq!(pending_count(&note), 3);

        let err = write_segment(&note, b"ct", SegmentFormat::Age, 3).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains('3'), "names the limit: {msg}");
        assert!(msg.contains("merge"), "says what to do: {msg}");
        assert_eq!(pending_count(&note), 3, "nothing was written past the cap");

        // Merging frees room again — the cap bounds pending work, not
        // lifetime writes.
        remove_segments(&segment_paths(&note)[..1]).unwrap();
        write_segment(&note, b"ct", SegmentFormat::Age, 3).expect("room after a merge");
        assert_eq!(pending_count(&note), 3);

        // 0 is the escape hatch: no cap at all.
        write_segment(&note, b"ct", SegmentFormat::Age, 0).expect("0 disables the cap");
        assert_eq!(pending_count(&note), 4);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nagging_starts_at_four_fifths_of_the_cap() {
        assert!(!should_nag(399, 500));
        assert!(should_nag(400, 500), "four fifths is the trigger");
        assert!(should_nag(500, 500));
        // An uncapped spool never nags — there is no wall to warn about.
        assert!(!should_nag(100_000, 0));
    }

    /// A locked identity must not consume age segments: they stay on disk
    /// for a later session, reported rather than dropped.
    #[test]
    fn locked_identity_leaves_age_segments_pending() {
        let dir = tmp("locked");
        let note = dir.join("journal.md.age");
        write_segment(&note, b"ciphertext", SegmentFormat::Age, 0).unwrap();

        let (ok, failed) = read_segments(&note, None);
        assert!(ok.is_empty());
        assert_eq!(failed.len(), 1);
        assert!(failed[0].1.contains("locked"), "got {:?}", failed[0].1);
        assert_eq!(pending_count(&note), 1, "still on disk");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_list_count_and_remove() {
        let dir = tmp("rt");
        let note = dir.join("journal.md.age");

        assert_eq!(pending_count(&note), 0, "no spool yet");

        let a = write_segment(&note, b"ciphertext-a", SegmentFormat::Age, 0).unwrap();
        let b = write_segment(&note, b"ciphertext-b", SegmentFormat::Gpg, 0).unwrap();
        assert_ne!(a, b, "filenames must not collide");
        assert_eq!(pending_count(&note), 2);

        // Filenames carry no timing information.
        let name = a.file_name().unwrap().to_str().unwrap();
        assert!(name.ends_with(".age"));
        assert_eq!(name.len(), 32 + 4, "16 random bytes as hex + .age");
        assert!(name
            .trim_end_matches(".age")
            .chars()
            .all(|c| c.is_ascii_hexdigit()));

        let paths = segment_paths(&note);
        assert_eq!(paths.len(), 2);
        remove_segments(&paths).unwrap();
        assert_eq!(pending_count(&note), 0);
        assert!(!spool_dir(&note).exists(), "empty spool dir is cleaned up");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
