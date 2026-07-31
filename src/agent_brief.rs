//! The briefing an assistant reads instead of pasted text.
//!
//! `schl8 agent brief` prints [`assets/agent-brief.md`] with this
//! machine's real notes folder, recipients and quicknotes substituted
//! in. The point is freshness: a briefing the user copies into a chat is
//! a snapshot that starts rotting immediately — it names keys they have
//! since replaced and notes they have since renamed — while a command
//! re-reads config every time it runs. What the human pastes shrinks to
//! one line telling the assistant to run it.
//!
//! **What is deliberately left out.** Key *labels* — GPG uids and age
//! nicknames — hold real names and email addresses, and this output is
//! expected to land in a third party's chat log. Recipients are printed
//! as bare fingerprints and `age1…` strings, which is everything needed
//! to encrypt and nothing needed to identify anyone. If the assistant
//! needs to know which key is which, the human can run
//! `schl8 recipients list` and say.
//!
//! Nothing here decrypts, and nothing here reads a note's contents:
//! it is config plus the public half of the keyring.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::config::Config;
use crate::document::spool;

/// The prose. Placeholders are `{{NAME}}`.
const TEMPLATE: &str = include_str!("../assets/agent-brief.md");

/// What `agent init` drops into a project directory.
const AGENTS_MD: &str = include_str!("../assets/agents-md-template.md");

/// Render the briefing for the given config.
pub fn render(cfg: &Config) -> String {
    let notes_dir = cfg
        .notes_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/Documents/Schl8".to_string());

    TEMPLATE
        .replace("{{VERSION}}", env!("CARGO_PKG_VERSION"))
        .replace(
            "{{GPG}}",
            if crate::crypto::gpg::gpg_available() {
                "yes"
            } else {
                "no — this machine is age-only, so ignore every GPG step below"
            },
        )
        .replace("{{NOTES_DIR}}", &notes_dir)
        .replace("{{RECIPIENTS}}", &recipients_block(cfg))
        .replace("{{NOTES}}", &notes_block(cfg))
        .replace("{{PENDING}}", &pending_block(cfg))
}

/// The recipient list: bare keys, numbered so they can be referred to.
///
/// The gathering is split from the formatting because the GPG half
/// comes from the system keyring, which is not empty just because
/// config is — a test that asserted on the "no keys" wording while
/// reading the developer's real keyring was testing the machine, not
/// the code.
fn recipients_block(cfg: &Config) -> String {
    let age: Vec<String> = cfg
        .age_recipients
        .iter()
        .map(|r| r.recipient.clone())
        .collect();
    let gpg: Vec<String> = if crate::crypto::gpg::gpg_available() {
        crate::crypto::keys::list_public_keys()
            .unwrap_or_default()
            .into_iter()
            .map(|k| k.fingerprint)
            .collect()
    } else {
        Vec::new()
    };
    format_recipients(&age, &gpg)
}

fn format_recipients(age: &[String], gpg: &[String]) -> String {
    let lines: Vec<String> = age
        .iter()
        .map(|r| ("age", r))
        .chain(gpg.iter().map(|f| ("gpg", f)))
        .enumerate()
        .map(|(i, (kind, key))| format!("    [{}] {kind}  {key}", i + 1))
        .collect();

    if lines.is_empty() {
        "    (none registered yet — they add them in Keys → Manage Public \
         Keys…, or generate an age key with Keys → Export AGE Public Key. \
         Until then you cannot encrypt anything.)"
            .to_string()
    } else {
        lines.join("\n")
    }
}

/// The quicknote list: name, backend, whether an append will work.
///
/// `appendable` mirrors what `spool::encrypt_segment` can actually do —
/// it needs a key of either backend. Saying otherwise steers an
/// assistant away from notes that work perfectly well.
fn notes_block(cfg: &Config) -> String {
    let notes = &cfg.quick_note.notes;
    if notes.is_empty() {
        return "    (none yet — see §7 for how they create one. Until then, \
                write files with `encrypt --out` instead of appending.)"
            .to_string();
    }
    let width = notes.iter().map(|n| n.name.len()).max().unwrap_or(0);
    notes
        .iter()
        .map(|n| {
            let queued = spool::pending_count(&n.source);
            let appendable = if n.rules.iter().any(|r| r.has_key()) {
                "appendable"
            } else {
                "NO KEY — appends will fail"
            };
            format!(
                "    {:width$}  {:<3}  {appendable}{}",
                n.name,
                backend_of(n),
                if queued > 0 {
                    format!("  ({queued} queued)")
                } else {
                    String::new()
                },
                width = width,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn backend_of(n: &crate::config::QuickNoteFile) -> &'static str {
    if n.rules.iter().any(|r| r.is_age()) {
        "age"
    } else {
        "gpg"
    }
}

/// A sentence about the queue, because an assistant that appends needs
/// to know entries do not appear until the human unlocks.
fn pending_block(cfg: &Config) -> String {
    let total: usize = cfg
        .quick_note
        .notes
        .iter()
        .map(|n| spool::pending_count(&n.source))
        .sum();
    match total {
        0 => "Nothing is queued right now.".to_string(),
        1 => "One entry is already queued and will merge into its note the \
              next time they unlock Schl8."
            .to_string(),
        n => format!(
            "{n} entries are already queued and will merge into their notes \
             the next time they unlock Schl8."
        ),
    }
}

/// Write `AGENTS.md` (and optionally `CLAUDE.md`) into a directory.
///
/// A pointer rather than a copy of the briefing: the file says to run
/// `schl8 agent brief`, so it cannot go stale. What it *does* state
/// inline is the write-only boundary — that part has to survive even if
/// the command is missing, because an assistant that can't find
/// `schl8` should still know never to ask for a seed phrase.
pub fn init(dir: Option<&Path>, force: bool, claude: bool) -> Result<Vec<PathBuf>> {
    let dir = match dir {
        Some(d) => d.to_path_buf(),
        None => std::env::current_dir().context("no current directory")?,
    };
    if !dir.is_dir() {
        bail!("{} is not a directory", dir.display());
    }

    let cfg = Config::load();
    let notes_dir = cfg
        .notes_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/Documents/Schl8".to_string());
    let body = AGENTS_MD.replace("{{NOTES_DIR}}", &notes_dir);

    let mut targets = vec![dir.join("AGENTS.md")];
    if claude {
        targets.push(dir.join("CLAUDE.md"));
    }

    // Check every target before writing any, so a refusal on the second
    // file does not leave the first one already overwritten.
    if !force {
        for t in &targets {
            if t.exists() {
                bail!(
                    "{} already exists — pass --force to overwrite, or append \
                     the contents by hand if it has other instructions in it",
                    t.display()
                );
            }
        }
    }

    for t in &targets {
        std::fs::write(t, &body).with_context(|| format!("writing {}", t.display()))?;
    }
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AgeRecipient, QuickNoteFile, SaveRule};

    fn cfg_with_keys() -> Config {
        let mut cfg = Config::default();
        cfg.age_recipients.push(AgeRecipient {
            label: "Sky Sloane <someone@example.com>".into(),
            recipient: "age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p".into(),
            ..Default::default()
        });
        cfg
    }

    /// The privacy decision this module exists to enforce. A label is
    /// the one field here that carries a person's name, and the whole
    /// output is expected to be pasted into somebody else's service.
    #[test]
    fn recipient_labels_never_reach_the_output() {
        let cfg = cfg_with_keys();
        let out = render(&cfg);
        assert!(
            out.contains("age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p"),
            "the recipient itself must be there — it is what makes the \
             briefing usable"
        );
        assert!(
            !out.contains("someone@example.com"),
            "an email address leaked into the briefing"
        );
        assert!(
            !out.contains("Sky Sloane"),
            "a name leaked into the briefing"
        );
    }

    #[test]
    fn every_placeholder_gets_substituted() {
        let out = render(&cfg_with_keys());
        assert!(
            !out.contains("{{"),
            "an unsubstituted placeholder survived into the output"
        );
    }

    /// An empty machine must still produce something an assistant can
    /// act on — "no keys" is a state to explain, not a blank line.
    #[test]
    fn empty_config_still_briefs_usefully() {
        let out = render(&Config::default());
        assert!(!out.contains("{{"));
        assert!(
            out.contains("none yet"),
            "should say there are no quicknotes"
        );
        // Recipients are checked separately: config being empty says
        // nothing about the GPG keyring, so this cannot assert on the
        // rendered key list without asserting on the test machine.
        assert!(
            format_recipients(&[], &[]).contains("none registered yet"),
            "should say there are no keys and how to add them"
        );
    }

    #[test]
    fn recipients_are_numbered_across_both_backends() {
        let out = format_recipients(&["age1aaa".into()], &["FPR1".into(), "FPR2".into()]);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("[1] age  age1aaa"), "{}", lines[0]);
        // Numbering continues rather than restarting per backend — the
        // human says "use key 3" and both sides must mean the same key.
        assert!(lines[1].contains("[2] gpg  FPR1"), "{}", lines[1]);
        assert!(lines[2].contains("[3] gpg  FPR2"), "{}", lines[2]);
    }

    #[test]
    fn appendable_matches_the_spool_rule_not_the_backend() {
        let mut cfg = Config::default();
        cfg.quick_note.notes.push(QuickNoteFile {
            name: "gpgnote".into(),
            source: PathBuf::from("/n/a.md.gpg"),
            rules: vec![SaveRule {
                key_fingerprint: "0123456789ABCDEF0123456789ABCDEF01234567".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        cfg.quick_note.notes.push(QuickNoteFile {
            name: "keyless".into(),
            source: PathBuf::from("/n/b.md.gpg"),
            ..Default::default()
        });
        let block = notes_block(&cfg);
        let gpg_line = block.lines().find(|l| l.contains("gpgnote")).unwrap();
        let bad_line = block.lines().find(|l| l.contains("keyless")).unwrap();
        assert!(
            gpg_line.contains("appendable") && !gpg_line.contains("NO KEY"),
            "a GPG note with a key is appendable: {gpg_line}"
        );
        assert!(
            bad_line.contains("NO KEY"),
            "a note with no key must be flagged, not silently listed: {bad_line}"
        );
    }

    /// The briefing is only useful if it keeps saying the thing that
    /// makes the app safe to point an agent at.
    #[test]
    fn the_write_only_boundary_is_stated_in_both_documents() {
        let brief = render(&Config::default());
        assert!(brief.contains("cannot read"), "brief states the boundary");
        assert!(
            brief.contains("Never ask for a seed phrase")
                || brief.contains("never ask for a seed phrase")
                || brief.contains("seed phrase"),
            "brief warns about secrets"
        );
        assert!(
            AGENTS_MD.contains("decrypt"),
            "AGENTS.md must carry the boundary too — it is what an agent \
             reads when `schl8` is not on PATH"
        );
    }

    #[test]
    fn init_refuses_to_clobber_without_force() {
        let dir = std::env::temp_dir().join(format!("schl8-init-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("AGENTS.md");
        std::fs::write(&target, "someone else's instructions").unwrap();

        let err = init(Some(&dir), false, false).unwrap_err().to_string();
        assert!(err.contains("already exists"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "someone else's instructions",
            "the existing file must be untouched"
        );

        init(Some(&dir), true, false).unwrap();
        assert!(std::fs::read_to_string(&target).unwrap().contains("Schl8"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Writing two files must be all-or-nothing on the pre-check, or a
    /// refusal on CLAUDE.md leaves AGENTS.md already replaced.
    #[test]
    fn init_checks_all_targets_before_writing_any() {
        let dir = std::env::temp_dir().join(format!("schl8-init2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("CLAUDE.md"), "existing").unwrap();

        assert!(init(Some(&dir), false, true).is_err());
        assert!(
            !dir.join("AGENTS.md").exists(),
            "AGENTS.md must not have been written before the refusal"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
