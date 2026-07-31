//! End-to-end crypto tests against a real `gpg` and an ephemeral,
//! passphrase-less keyring in a temporary `GNUPGHOME`.
//!
//! These exercise the actual subprocess round-trips that the pure-logic
//! unit tests can't: encrypt → decrypt, atomic overwrite, quick-note
//! append, and recipient-fingerprint resolution. They skip gracefully
//! when gpg isn't installed so `cargo test` still passes on a bare box.
//!
//! Everything runs inside ONE `#[test]` because it mutates the global
//! `GNUPGHOME` env var; a single serial test avoids racing other tests.

use std::path::{Path, PathBuf};

use crate::crypto::{gpg, keys};

/// Skip (return None) if gpg isn't available in this environment.
fn gpg_available() -> bool {
    gpg::gpg_bin().is_ok()
}

/// A temporary GNUPGHOME that is removed (and its agent killed) on drop.
struct EphemeralKeyring {
    home: PathBuf,
}

impl EphemeralKeyring {
    fn new() -> Self {
        // Unique dir per run; keep it short (gpg-agent socket path limits).
        let home = std::env::temp_dir().join(format!("schl8-it-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        // Point every subsequent gpg subprocess at this keyring.
        std::env::set_var("GNUPGHOME", &home);

        let kr = EphemeralKeyring { home };
        kr.generate_key();
        kr
    }

    fn gpg(&self) -> std::process::Command {
        let mut c = std::process::Command::new(gpg::gpg_bin().unwrap());
        c.env("GNUPGHOME", &self.home);
        c
    }

    /// Generate a passphrase-less signing+encryption key.
    fn generate_key(&self) {
        let out = self
            .gpg()
            .args([
                "--batch",
                "--pinentry-mode",
                "loopback",
                "--passphrase",
                "",
                "--quick-generate-key",
                "Schl8 Test <test@schl8.local>",
                "ed25519",
                "sign", // primary can sign + certify (needed for --sign)
                "never",
            ])
            .output()
            .expect("gpg quick-generate-key");
        assert!(
            out.status.success(),
            "key gen failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        let fpr = self.primary_fingerprint();
        let out = self
            .gpg()
            .args([
                "--batch",
                "--pinentry-mode",
                "loopback",
                "--passphrase",
                "",
                "--quick-add-key",
                &fpr,
                "cv25519",
                "encr",
                "never",
            ])
            .output()
            .expect("gpg quick-add-key");
        assert!(
            out.status.success(),
            "subkey add failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn primary_fingerprint(&self) -> String {
        let out = self
            .gpg()
            .args(["--list-keys", "--with-colons"])
            .output()
            .expect("gpg --list-keys");
        let listing = String::from_utf8_lossy(&out.stdout);
        for line in listing.lines() {
            let f: Vec<&str> = line.split(':').collect();
            if f.first() == Some(&"fpr") {
                return f.get(9).unwrap_or(&"").to_string();
            }
        }
        panic!("no fingerprint in keyring");
    }
}

impl Drop for EphemeralKeyring {
    fn drop(&mut self) {
        // Stop ALL daemons gpg auto-started for this keyring (gpg-agent,
        // dirmngr, keyboxd, …) so none are left orphaned pointing at the
        // temp dir once it's removed.
        let _ = std::process::Command::new("gpgconf")
            .env("GNUPGHOME", &self.home)
            .args(["--kill", "all"])
            .output();
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn write_encrypted(path: &Path, plaintext: &[u8], fpr: &str, armor: bool) {
    let ct = keys::encrypt_to_bytes(plaintext, &[fpr], armor).expect("encrypt_to_bytes");
    keys::atomic_write(path, &ct).expect("atomic_write");
}

fn decrypt_to_string(path: &Path) -> String {
    let buf = gpg::decrypt_file(path).expect("decrypt_file");
    buf.as_str().expect("utf8").to_string()
}

#[test]
fn gpg_end_to_end_roundtrip() {
    if !gpg_available() {
        eprintln!("skipping gpg integration test — gpg not found");
        return;
    }

    let kr = EphemeralKeyring::new();
    let fpr = kr.primary_fingerprint();
    let dir = kr.home.clone();

    // ── 1. encrypt_to_bytes → decrypt_file round-trips ───────────────
    let note = dir.join("note.md.gpg");
    write_encrypted(&note, b"# Title\n\nfirst line\n", &fpr, false);
    assert_eq!(decrypt_to_string(&note), "# Title\n\nfirst line\n");

    // ── 2. recipient parsing + fingerprint resolution ────────────────
    let recips = gpg::list_recipients(&note).expect("list_recipients");
    assert!(!recips.is_empty(), "should list at least one recipient");
    let resolved = keys::recipients_for_reencrypt(&note).expect("resolve recipients");
    assert_eq!(
        resolved,
        vec![fpr.clone()],
        "the file's (subkey) recipient must resolve to the primary fingerprint"
    );

    // ── 3. quick-note append re-encrypts to the same key ─────────────
    crate::document::append::append_blurb_with_rules(
        &note,
        "\n## 2026-07-05 10:00\n\nappended note\n",
        &[],
        None,
    )
    .expect("append (no rules)");
    let after = decrypt_to_string(&note);
    assert!(after.starts_with("# Title"), "original content preserved");
    assert!(after.contains("appended note"), "blurb appended");
    // Still decryptable and still targeting our key.
    assert_eq!(
        keys::recipients_for_reencrypt(&note).unwrap(),
        vec![fpr.clone()]
    );

    // ── 3b. registry-style append fans out per rule ──────────────────
    // One rule, two destinations (the source itself + a backup copy):
    // both must hold the combined content afterwards.
    let backup = dir.join("note-backup.md.gpg");
    let rules = vec![crate::config::SaveRule {
        key_fingerprint: fpr.clone(),
        key_label: String::new(),
        age_recipient: String::new(),
        destinations: vec![note.clone(), backup.clone()],
    }];
    crate::document::append::append_blurb_with_rules(&note, "\nsecond note\n", &rules, None)
        .expect("append_blurb_with_rules");
    let fanned = decrypt_to_string(&note);
    assert!(fanned.contains("appended note"), "earlier append preserved");
    assert!(fanned.contains("second note"), "new blurb appended");
    assert_eq!(
        decrypt_to_string(&backup),
        fanned,
        "backup destination holds the same combined content"
    );

    // ── 4. encrypt_overwrite replaces in place, stays decryptable ────
    keys::encrypt_overwrite(b"replaced entirely\n", &[fpr.as_str()], &note, false)
        .expect("encrypt_overwrite");
    assert_eq!(decrypt_to_string(&note), "replaced entirely\n");

    // ── 5. ASCII-armored (.asc) path ─────────────────────────────────
    let armored = dir.join("armored.txt.asc");
    write_encrypted(&armored, b"armored content\n", &fpr, true);
    let raw = std::fs::read(&armored).unwrap();
    assert!(
        raw.starts_with(b"-----BEGIN PGP MESSAGE-----"),
        "armored output should be ASCII-armored"
    );
    assert_eq!(decrypt_to_string(&armored), "armored content\n");

    // ── 6. loader dispatches a real encrypted file to Single ─────────
    match crate::document::loader::load(&note).expect("load") {
        crate::document::LoadedDocument::Single(doc) => {
            assert_eq!(doc.content.as_str().unwrap(), "replaced entirely\n");
            assert_eq!(doc.recipients, Some(vec![fpr.clone()]));
            // This file was encrypted but not signed.
            assert_eq!(doc.signature, gpg::SignatureStatus::Unsigned);
        }
        _ => panic!("expected a single document"),
    }

    // ── 7. signature verification: sign+encrypt, then decrypt ────────
    let signed = dir.join("signed.md.gpg");
    let ct = kr
        .gpg()
        .args([
            "--batch",
            "--yes",
            "--pinentry-mode",
            "loopback",
            "--passphrase",
            "",
            "--trust-model",
            "always",
            "--local-user",
            &fpr,
            "--sign",
            "--encrypt",
            "--recipient",
            &fpr,
            "--output",
        ])
        .arg(&signed)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            c.stdin.take().unwrap().write_all(b"signed content\n")?;
            c.wait_with_output()
        })
        .expect("sign+encrypt");
    assert!(ct.status.success(), "sign+encrypt failed");

    let dec = gpg::decrypt_file_verified(&signed).expect("decrypt signed");
    assert_eq!(dec.content.as_str().unwrap(), "signed content\n");
    match dec.signature {
        gpg::SignatureStatus::Valid { signer } => {
            assert!(
                signer.contains("test@schl8.local"),
                "signer should be our test uid, got: {signer}"
            );
        }
        other => panic!("expected a valid signature, got {other:?}"),
    }

    // ── 8. save plan: fan out to multiple destinations, mixed armor ──
    let dest_a = dir.join("plan-a.md.gpg");
    let dest_b = dir.join("backups").join("plan-b.md.asc");
    std::fs::create_dir_all(dir.join("backups")).unwrap();
    // Pre-seed dest_a to prove the plan overwrites existing files.
    std::fs::write(&dest_a, b"old junk").unwrap();

    let plan = crate::config::SavePlan {
        source: note.clone(),
        rules: vec![crate::config::SaveRule {
            key_fingerprint: fpr.clone(),
            key_label: "test key".to_string(),
            age_recipient: String::new(),
            destinations: vec![dest_a.clone(), dest_b.clone()],
        }],
        ..Default::default()
    };
    let results = crate::document::multisave::execute(b"plan content\n", &plan);
    assert_eq!(results.len(), 2);
    for r in &results {
        assert!(
            r.result.is_ok(),
            "target {} failed: {:?}",
            r.destination.display(),
            r.result.as_ref().err()
        );
    }
    // Both destinations decrypt to the same content; .asc is armored.
    assert_eq!(decrypt_to_string(&dest_a), "plan content\n");
    assert_eq!(decrypt_to_string(&dest_b), "plan content\n");
    assert!(std::fs::read(&dest_b)
        .unwrap()
        .starts_with(b"-----BEGIN PGP MESSAGE-----"));

    // ── 9. archive-entry edit: rebuild + re-encrypt + reload ─────────
    // Build a tar.gz with a text file and a binary file, encrypt it,
    // load it as an archive, save an edited entry, and confirm the
    // binary survived and the edit round-trips through gpg.
    let mut builder = tar::Builder::new(Vec::new());
    for (path, content) in [
        ("vault/note.md", b"# Original\n".as_slice()),
        ("vault/photo.png", b"\x89PNG-ish binary \x00\x01\x02"),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path, content).unwrap();
    }
    let tar_bytes = builder.into_inner().unwrap();
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut enc, &tar_bytes).unwrap();
    let tgz = enc.finish().unwrap();

    let vault = dir.join("vault.tar.gz.gpg");
    write_encrypted(&vault, &tgz, &fpr, false);

    let loaded = crate::document::loader::load(&vault).expect("load archive");
    let archive = match loaded {
        crate::document::LoadedDocument::Archive(a) => a,
        _ => panic!("expected an archive"),
    };
    assert!(archive.gzip);
    assert_eq!(archive.recipients, Some(vec![fpr.clone()]));
    assert_eq!(archive.entries.len(), 1, "only the text entry is listed");
    // …and the loader reports the one it could not list. The binary is
    // still in the vault and survives every save; the browser just can't
    // render it, so a vault that looks like it holds one file must say
    // that it actually holds two.
    assert_eq!(archive.hidden.non_text, 1, "the .png is counted");
    assert_eq!(archive.hidden.total(), 1);
    assert_eq!(
        archive.hidden.summary().as_deref(),
        Some("1 file not shown (1 not text)")
    );

    // Simulate the in-app save: rebuild with the edit, re-encrypt in place.
    let rebuilt = crate::document::archive::rebuild_with_edit(
        archive.raw_tar.as_bytes(),
        "vault/note.md",
        b"# Edited inside the archive\n",
        archive.gzip,
    )
    .expect("rebuild");
    keys::encrypt_overwrite(rebuilt.payload.as_bytes(), &[fpr.as_str()], &vault, false)
        .expect("re-encrypt archive");

    // Reload: the edit is there and the binary entry survived.
    let reloaded = match crate::document::loader::load(&vault).expect("reload archive") {
        crate::document::LoadedDocument::Archive(a) => a,
        _ => panic!("expected an archive"),
    };
    assert_eq!(
        reloaded.entries[0].content.as_str().unwrap(),
        "# Edited inside the archive\n"
    );
    let mut names = Vec::new();
    let mut png = Vec::new();
    let mut ar = tar::Archive::new(reloaded.raw_tar.as_bytes());
    for e in ar.entries().unwrap() {
        let mut e = e.unwrap();
        let p = e.path().unwrap().to_string_lossy().into_owned();
        if p == "vault/photo.png" {
            std::io::Read::read_to_end(&mut e, &mut png).unwrap();
        }
        names.push(p);
    }
    assert!(names.contains(&"vault/photo.png".to_string()));
    assert_eq!(png, b"\x89PNG-ish binary \x00\x01\x02");

    // ── 10. GPG offline spool: write without the key, merge with it ──
    // A GPG-only quicknote used to be refused outright by the spool,
    // which left `schl8 append` with no way to reach such a note at
    // all. Writing a segment needs only the public key, so it must work
    // the same way age does.
    use crate::document::spool::{self, SegmentFormat};
    let spooled_note = dir.join("journal.md.gpg");
    write_encrypted(&spooled_note, b"# Journal\n", &fpr, false);

    let rules = vec![crate::config::SaveRule {
        key_fingerprint: fpr.clone(),
        destinations: vec![spooled_note.clone()],
        ..Default::default()
    }];

    for (when, body) in [
        ("2026-07-22T09:00:00Z", "second entry\n"),
        ("2026-07-22T08:00:00Z", "first entry\n"),
    ] {
        let env = spool::envelope(when, body);
        let (ct, format) = spool::encrypt_segment(&rules, env.as_bytes()).expect("encrypt segment");
        assert_eq!(format, SegmentFormat::Gpg, "a GPG-only plan spools as GPG");
        spool::write_segment(&spooled_note, &ct, format, 0).expect("write segment");
    }
    assert_eq!(
        spool::pending_count(&spooled_note),
        2,
        "counting needs no key"
    );
    assert_eq!(
        spool::pending_count_of(&spooled_note, SegmentFormat::Gpg),
        2
    );

    // Merging a GPG spool must NOT require an unlocked age identity —
    // that requirement is what made GPG notes unmergeable.
    let (segments, failed) = spool::read_segments(&spooled_note, None);
    assert!(failed.is_empty(), "all GPG segments readable: {failed:?}");
    assert_eq!(segments.len(), 2);
    // Written out of order, merged in timestamp order.
    assert_eq!(
        spool::merged_text(&segments),
        "first entry\nsecond entry\n",
        "segments merge in written order"
    );

    // The ordinary append path lands them in the note and the spool clears.
    crate::document::append::append_blurb_with_rules(
        &spooled_note,
        &spool::merged_text(&segments),
        &rules,
        None,
    )
    .expect("merge into the note");
    let merged_paths: Vec<PathBuf> = segments.iter().map(|s| s.path.clone()).collect();
    spool::remove_segments(&merged_paths).expect("clear merged segments");
    assert_eq!(spool::pending_count(&spooled_note), 0);

    let final_text = decrypt_to_string(&spooled_note);
    assert!(
        final_text.contains("# Journal"),
        "original kept: {final_text}"
    );
    assert!(final_text.contains("first entry"), "got {final_text}");
    assert!(final_text.contains("second entry"), "got {final_text}");
}
