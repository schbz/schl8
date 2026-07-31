//! Headless agent commands: the write-only automation surface.
//!
//! Everything here runs without a window and without prompts, and none of
//! it can read a note or touch key material — encryption needs only the
//! public recipients registered in config, and appends go through the
//! offline spool exactly like a locked quicknote. See
//! `docs/AGENT-DESIGN.md` for the model and its invariants.

use std::io::{Read, Write};
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use zeroize::Zeroize;

use crate::cli::{AgentAction, AgentCmd, ListAction, SkillsAction};
use crate::config::Config;
use crate::crypto::{age_backend, keys};
use crate::document::spool;

/// Which encryption backend a recipient string names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    Age,
    Gpg,
}

/// Classify a `--to` argument. AGE recipients are validated fully; GPG
/// fingerprints only shallowly (gpg itself is the authority and fails
/// encryption cleanly for an unknown key).
fn classify_recipient(s: &str) -> Result<Backend> {
    let s = s.trim();
    if s.starts_with("age1") {
        age_backend::validate_recipient(s)?;
        return Ok(Backend::Age);
    }
    let hex = s.len() >= 16 && s.chars().all(|c| c.is_ascii_hexdigit());
    if hex {
        return Ok(Backend::Gpg);
    }
    Err(anyhow!(
        "unrecognized recipient {s:?} — expected an AGE key (age1…) or a \
         GPG fingerprint (hex)"
    ))
}

/// Find a registered quicknote by name (case-insensitive), exact path, or
/// file name.
fn resolve_note<'a>(cfg: &'a Config, wanted: &str) -> Result<&'a crate::config::QuickNoteFile> {
    let notes = &cfg.quick_note.notes;
    let by_name = notes.iter().find(|n| n.name.eq_ignore_ascii_case(wanted));
    let by_path = notes.iter().find(|n| n.source == Path::new(wanted));
    let by_file = notes.iter().find(|n| {
        n.source
            .file_name()
            .and_then(|f| f.to_str())
            .is_some_and(|f| f == wanted)
    });
    by_name.or(by_path).or(by_file).ok_or_else(|| {
        let known: Vec<&str> = notes.iter().map(|n| n.name.as_str()).collect();
        anyhow!(
            "no registered quicknote matches {wanted:?} (known: {}) — run \
             `schl8 notes list`",
            if known.is_empty() {
                "none".to_string()
            } else {
                known.join(", ")
            }
        )
    })
}

/// Best-effort backend label for a note, from its rules or extension.
fn note_backend(note: &crate::config::QuickNoteFile) -> &'static str {
    if note.rules.iter().any(|r| r.is_age()) {
        "age"
    } else if !note.rules.is_empty() {
        "gpg"
    } else if note
        .source
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e == "age")
    {
        "age"
    } else {
        "gpg"
    }
}

/// Run one headless command. Returns the process exit code.
pub fn run(cmd: AgentCmd) -> i32 {
    match dispatch(cmd) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e:#}");
            1
        }
    }
}

fn dispatch(cmd: AgentCmd) -> Result<()> {
    match cmd {
        AgentCmd::Encrypt { to, out, armor } => encrypt(&to, out.as_deref(), armor),
        AgentCmd::Append { note, raw } => append(&note, raw),
        AgentCmd::Notes {
            action: ListAction::List { json },
        } => notes_list(json),
        AgentCmd::Recipients {
            action: ListAction::List { json },
        } => recipients_list(json),
        AgentCmd::Pending { json } => pending(json),
        AgentCmd::Agent { action } => match action {
            AgentAction::Brief => brief(),
            AgentAction::Toolkit { json } => toolkit(json),
            AgentAction::Skills { action } => skills(action),
            AgentAction::Init { dir, force, claude } => init(dir.as_deref(), force, claude),
        },
    }
}

/// Read all of stdin. The caller must zeroize the buffer when done —
/// this is the only plaintext the agent surface ever holds.
fn read_stdin() -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    std::io::stdin()
        .read_to_end(&mut buf)
        .context("reading stdin")?;
    if buf.is_empty() {
        bail!("stdin was empty — pipe the plaintext in, e.g. `echo hi | schl8 encrypt …`");
    }
    Ok(buf)
}

fn encrypt(to: &[String], out: Option<&Path>, armor: bool) -> Result<()> {
    let backends: Vec<Backend> = to
        .iter()
        .map(|r| classify_recipient(r))
        .collect::<Result<_>>()?;
    let backend = backends[0];
    if backends.iter().any(|b| *b != backend) {
        bail!(
            "recipients mix AGE and GPG — one output file can only use one \
             backend; run encrypt twice for two files"
        );
    }

    let mut plaintext = read_stdin()?;
    let refs: Vec<&str> = to.iter().map(|s| s.as_str()).collect();
    let result = match backend {
        Backend::Age => age_backend::encrypt_to_recipients(&plaintext, &refs),
        Backend::Gpg => keys::encrypt_to_bytes(&plaintext, &refs, armor),
    };
    plaintext.zeroize();
    let ciphertext = result?;

    match out {
        Some(path) => {
            keys::atomic_write(path, &ciphertext)
                .with_context(|| format!("writing {}", path.display()))?;
            eprintln!("encrypted {} bytes to {}", ciphertext.len(), path.display());
        }
        None => {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&ciphertext).context("writing stdout")?;
            stdout.flush().ok();
        }
    }
    Ok(())
}

fn append(wanted: &str, raw: bool) -> Result<()> {
    let cfg = Config::load();
    let note = resolve_note(&cfg, wanted)?;

    let bytes = read_stdin()?;
    let mut text = String::from_utf8(bytes).context("stdin must be UTF-8 text")?;

    // Same shape as a jotted entry unless --raw: agent entries should
    // read like every other entry in the note.
    let mut body = if raw {
        text.clone()
    } else {
        crate::config::render_blurb(&cfg.quick_note, &note.source, &text, true)
    };
    let written = chrono::Utc::now().to_rfc3339();
    let mut envelope = spool::envelope_from(&written, "cli", &body);
    // Whichever backend the note's plan uses — encrypting a segment needs
    // only a public key either way, so the write-only surface stays
    // write-only and no hardware key is touched.
    let result = spool::encrypt_segment(&note.rules, envelope.as_bytes());
    text.zeroize();
    body.zeroize();
    envelope.zeroize();
    let (ciphertext, format) = result?;
    spool::write_segment(
        &note.source,
        &ciphertext,
        format,
        cfg.quick_note.max_pending,
    )?;

    let pending = spool::pending_count(&note.source);
    // Backend-neutral wording: an age note merges when its identity is
    // unlocked, a GPG one whenever the user next merges — "unlock" would
    // be wrong for half the notes now that both can spool.
    println!(
        "spooled 1 entry for {:?} ({pending} pending — merges into the note in Schl8)",
        note.name
    );
    // Warn on the way up rather than only at the wall, so an agent (and
    // whoever reads its logs) sees the spool filling in time to merge.
    let max = cfg.quick_note.max_pending;
    if spool::should_nag(pending, max) {
        eprintln!("warning: {pending} of {max} pending entries — merge this note soon");
    }
    Ok(())
}

fn notes_list(json: bool) -> Result<()> {
    let cfg = Config::load();
    if json {
        let items: Vec<serde_json::Value> = cfg
            .quick_note
            .notes
            .iter()
            .map(|n| {
                serde_json::json!({
                    "name": n.name,
                    "path": n.source,
                    "backend": note_backend(n),
                    "exists": n.source.exists(),
                    "pending": spool::pending_count(&n.source),
                    // Appendable means `spool::encrypt_segment` can build a
                    // segment for this note — which needs a key of EITHER
                    // backend, not an age one. This said `is_age()` until
                    // the spool learned to write GPG segments, and then
                    // kept saying it: the field told agents that every
                    // GPG note was unwritable while appends to them
                    // succeeded.
                    "appendable": n.rules.iter().any(|r| r.has_key()),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
    } else if cfg.quick_note.notes.is_empty() {
        println!("no quicknotes registered");
    } else {
        for n in &cfg.quick_note.notes {
            let pending = spool::pending_count(&n.source);
            println!(
                "{}\t{}\t{}{}{}",
                n.name,
                note_backend(n),
                n.source.display(),
                if n.source.exists() { "" } else { "\t(missing)" },
                if pending > 0 {
                    format!("\t({pending} pending)")
                } else {
                    String::new()
                },
            );
        }
    }
    Ok(())
}

fn recipients_list(json: bool) -> Result<()> {
    let cfg = Config::load();
    let gpg_keys = if crate::crypto::gpg::gpg_available() {
        keys::list_public_keys().unwrap_or_default()
    } else {
        Vec::new()
    };

    if json {
        let age: Vec<serde_json::Value> = cfg
            .age_recipients
            .iter()
            .map(|r| serde_json::json!({"label": r.label, "recipient": r.recipient}))
            .collect();
        let gpg: Vec<serde_json::Value> = gpg_keys
            .iter()
            .map(|k| serde_json::json!({"uid": k.uid, "fingerprint": k.fingerprint}))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "age": age,
                "gpg": gpg,
                "gpg_available": crate::crypto::gpg::gpg_available(),
            }))?
        );
    } else {
        for r in &cfg.age_recipients {
            println!("age\t{}\t{}", r.label, r.recipient);
        }
        for k in &gpg_keys {
            println!("gpg\t{}\t{}", k.uid, k.fingerprint);
        }
        if cfg.age_recipients.is_empty() && gpg_keys.is_empty() {
            println!("no recipients registered");
        }
    }
    Ok(())
}

/// Print the live briefing (see `agent_brief`).
fn brief() -> Result<()> {
    print!("{}", crate::agent_brief::render(&Config::load()));
    Ok(())
}

/// Print the persistent-toolkit spec (see `agent_toolkit`).
fn toolkit(json: bool) -> Result<()> {
    let cfg = Config::load();
    if json {
        println!("{}", crate::agent_toolkit::render_json(&cfg)?);
    } else {
        print!("{}", crate::agent_toolkit::render(&cfg));
    }
    Ok(())
}

/// Install or remove the Claude Code skill and slash commands.
fn skills(action: SkillsAction) -> Result<()> {
    match action {
        SkillsAction::Install { dry_run, force } => {
            let cfg = Config::load();
            let planned = crate::agent_skills::plan(&cfg)?;
            if dry_run {
                println!("Would write {} file(s):", planned.len());
                for p in &planned {
                    println!("  {:?}  {}", p.action, p.path.display());
                }
                println!(
                    "\nRefresh = written by Schl8 before, safe to replace. \
                     Conflict = someone else's file; needs --force."
                );
                return Ok(());
            }
            let written = crate::agent_skills::install(&cfg, force)?;
            for p in &written {
                println!("wrote {}", p.display());
            }
            println!(
                "\nStart a new Claude Code session to pick these up. The skill \
                 triggers on intent; the commands are /schl8:jot and one per \
                 quicknote. Re-run this after renaming a note."
            );
        }
        SkillsAction::Uninstall => {
            let (removed, skipped) = crate::agent_skills::uninstall()?;
            for p in &removed {
                println!("removed {}", p.display());
            }
            for p in &skipped {
                println!("left alone (not written by Schl8) {}", p.display());
            }
            if removed.is_empty() {
                println!("nothing to remove");
            }
        }
    }
    Ok(())
}

/// Drop an AGENTS.md into a directory so coding agents need no paste.
fn init(dir: Option<&Path>, force: bool, claude: bool) -> Result<()> {
    let written = crate::agent_brief::init(dir, force, claude)?;
    for p in &written {
        println!("wrote {}", p.display());
    }
    println!(
        "Your assistant will pick this up automatically in this directory. \
         It points at `schl8 agent brief` rather than copying it, so it \
         stays current."
    );
    Ok(())
}

fn pending(json: bool) -> Result<()> {
    let cfg = Config::load();
    let per: Vec<(String, usize)> = cfg
        .quick_note
        .notes
        .iter()
        .map(|n| (n.name.clone(), spool::pending_count(&n.source)))
        .collect();
    let total: usize = per.iter().map(|(_, c)| c).sum();

    if json {
        let notes: Vec<serde_json::Value> = per
            .iter()
            .map(|(name, count)| serde_json::json!({"name": name, "pending": count}))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "total": total,
                "notes": notes,
            }))?
        );
    } else {
        println!("{total} pending");
        for (name, count) in per.iter().filter(|(_, c)| *c > 0) {
            println!("  {name}: {count}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{QuickNoteFile, SaveRule};
    use std::path::PathBuf;

    const AGE_OK: &str = "age1uwlr4jpxxu3q9v0wtlc8h2f6e72zwxsps05uwquf8jqa0f06p5cs82yjxq";

    #[test]
    fn recipient_classification() {
        assert_eq!(classify_recipient(AGE_OK).unwrap(), Backend::Age);
        assert_eq!(
            classify_recipient("6DDFDE6006C1911FE74CDA4651438F0920BA4582").unwrap(),
            Backend::Gpg
        );
        // Bad bech32 must fail validation, not be mistaken for GPG.
        assert!(classify_recipient("age1notreal").is_err());
        assert!(classify_recipient("hello").is_err());
        assert!(classify_recipient("").is_err());
    }

    fn cfg_with(notes: Vec<QuickNoteFile>) -> Config {
        let mut cfg = Config::default();
        cfg.quick_note.notes = notes;
        cfg
    }

    #[test]
    fn note_resolution_by_name_path_and_filename() {
        let note = QuickNoteFile {
            name: "Journal".into(),
            source: PathBuf::from("/notes/journal.md.age"),
            rules: vec![SaveRule {
                age_recipient: AGE_OK.into(),
                destinations: vec![PathBuf::from("/notes/journal.md.age")],
                ..Default::default()
            }],
            ..Default::default()
        };
        let cfg = cfg_with(vec![note]);
        assert!(resolve_note(&cfg, "journal").is_ok(), "name, any case");
        assert!(resolve_note(&cfg, "/notes/journal.md.age").is_ok(), "path");
        assert!(resolve_note(&cfg, "journal.md.age").is_ok(), "file name");
        let err = resolve_note(&cfg, "nope").unwrap_err().to_string();
        assert!(err.contains("Journal"), "error lists known notes: {err}");
    }

    /// Regression: `appendable` claimed every GPG note was unwritable.
    /// It tested `is_age()`, which was right only until the spool learned
    /// to write GPG segments — after that the field told agents to skip
    /// notes that appends were succeeding on. It must answer the same
    /// question `encrypt_segment` does: is there a key of either kind?
    #[test]
    fn appendable_tracks_what_encrypt_segment_can_actually_do() {
        use crate::config::SaveRule;

        let appendable = |rules: Vec<SaveRule>| {
            let note = QuickNoteFile {
                rules,
                ..Default::default()
            };
            let can_encrypt = spool::encrypt_segment(&note.rules, b"probe").is_ok();
            let claimed = note.rules.iter().any(|r| r.has_key());
            (claimed, can_encrypt)
        };

        // A GPG-only note: the field used to say false while the append
        // worked. Both must now agree.
        let gpg = vec![SaveRule {
            key_fingerprint: "0123456789ABCDEF0123456789ABCDEF01234567".into(),
            ..Default::default()
        }];
        let (claimed, _) = appendable(gpg);
        assert!(claimed, "a GPG note is appendable");

        // An age note, which was always reported correctly.
        let age = vec![SaveRule {
            age_recipient: "age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p".into(),
            ..Default::default()
        }];
        let (claimed, can) = appendable(age);
        assert!(claimed && can, "an age note is appendable, and really is");

        // No key at all: encrypt_segment bails, so the field must say so
        // rather than sending an agent at a note that cannot take one.
        let (claimed, can) = appendable(Vec::new());
        assert!(!claimed, "a key-less note is not appendable");
        assert!(!can, "and encrypt_segment agrees");
    }

    #[test]
    fn backend_labeling() {
        let age_note = QuickNoteFile {
            source: PathBuf::from("/n/a.md.age"),
            ..Default::default()
        };
        assert_eq!(note_backend(&age_note), "age");
        let gpg_note = QuickNoteFile {
            source: PathBuf::from("/n/a.md.gpg"),
            ..Default::default()
        };
        assert_eq!(note_backend(&gpg_note), "gpg");
    }
}
