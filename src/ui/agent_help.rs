//! Paste-into-your-agent instruction blocks.
//!
//! Each entry is a short briefing written *to an AI assistant*, in the
//! second person, that the user copies into Claude, ChatGPT, or whatever
//! they use. They are not documentation for the user — they are prompts,
//! and they read that way: what the agent may do, what it may not, and
//! the exact commands.
//!
//! CLIPBOARD, deliberately. The app's fifth security invariant says no
//! clipboard, and strips Copy/Cut input events so no widget can put
//! decrypted text there. That protects *document plaintext*. What is
//! copied here is a fixed string compiled into the binary: no document
//! content, no key material, nothing derived from a file the user has
//! open. It also goes out through `ctx.copy_text`, an output command,
//! rather than through the input events the invariant filters — so the
//! protection stays exactly as strong as it was. Do not extend this
//! module to copy anything that comes from a document.

use egui::{Align2, RichText, Vec2};

use super::theme;

/// The long-form guide, written into the config directory so an agent
/// with filesystem access can read it. Shipped in the binary so it can
/// never drift from the version of the app that wrote it.
const GUIDE: &str = include_str!("../../assets/agent-guide.md");

/// Where the guide is written. Beside the config, which is a path the
/// user can already be told about without revealing anything.
pub fn guide_path() -> Option<std::path::PathBuf> {
    crate::config::config_path().and_then(|p| p.parent().map(|d| d.join("AGENT-GUIDE.md")))
}

/// Write (or refresh) the guide on disk and return its path.
///
/// Rewritten every time rather than only when missing: the file is
/// generated, so a stale copy from an older version is worse than no
/// copy — it would describe commands that may no longer exist.
pub fn write_guide() -> anyhow::Result<std::path::PathBuf> {
    let path = guide_path().ok_or_else(|| anyhow::anyhow!("no config directory"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Substitute the configured notes folder, for the same reason the
    // snippets do: the guide used to name `~/Notes`, a directory almost
    // nobody has, so every example in it was one an agent could not
    // follow literally.
    let notes = crate::config::Config::load()
        .notes_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/Documents/Schl8".to_string());
    let body = GUIDE.replace("{{NOTES_DIR}}", &notes);
    crate::crypto::keys::atomic_write(&path, body.as_bytes())?;
    Ok(path)
}

/// One copyable briefing.
pub struct Snippet {
    /// Menu label.
    pub title: &'static str,
    /// One line under the title, for the user rather than the agent.
    pub summary: &'static str,
    /// The text that goes on the clipboard.
    pub body: String,
}

/// Every snippet, in menu order.
///
/// The first two are entry points — one for an assistant that can run a
/// shell command (which should be almost all of them now) and one for
/// an assistant that can only read pasted text. The rest are the
/// suggestions: each is phrased as the user asking for a thing to be
/// set up, because that is how the menu reads to the person browsing it.
pub fn all() -> Vec<Snippet> {
    let guide = guide_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/.config/schl8/AGENT-GUIDE.md".to_string());

    let mut snippets = vec![
        Snippet {
            title: "Start here — one line",
            summary: "Your agent runs one command and reads a briefing built from this machine",
            body:
"I use Schl8, an encrypted notes app on macOS, and I'd like your help
with it. Run this and follow what it says:

    schl8 agent brief

It prints current instructions generated from my actual setup — my notes
folder, the keys you may encrypt to, the notes you may append to, and a
list of things you can offer to set up for me. Read all of it before
running anything else.

Two things hold either way: you cannot decrypt or read my notes — there
is no command for it — and you will never need a password, PIN or seed
phrase from me, so never ask for one.

If the command isn't found, tell me: I need to open Schl8 and choose
Help \u{203A} Install Command Line Tool\u{2026} first."
                .to_string(),
        },
        Snippet {
            title: "Start here — if your agent can't run commands",
            summary: "The paste-only fallback: points at the full guide file on disk",
            body: format!(
"I use Schl8, an encrypted notes app on macOS, and I'd like your help with it.

Before doing anything, read this file:

    {guide}

It explains what Schl8 is, what you can and cannot do with it, and the
exact commands available to you. The short version: you can encrypt text
to my keys and append to my notes, but there is no command that decrypts
anything, and you must never ask me for a seed phrase, PIN or passphrase.

Once you've read it, get your bearings:

    schl8 recipients list --json
    schl8 notes list --json

Then ask me what I want to set up, and walk me through it one step at a
time. Tell me what you're about to run before you run it."
            ),
        },
        Snippet {
            title: "Make yourself permanently available",
            summary: "Your agent builds Schl8 into its own skill system, so it's there next time",
            body:
"I want Schl8 to be a standing part of your setup, not something I
re-explain every conversation. Build yourself a permanent toolkit.

Start here:

    schl8 agent toolkit

That prints a specification: the capabilities, the exact commands, my
current notes and keys, and the rules that have to survive into whatever
you build. Add --json if generating from a manifest is easier than
reading prose.

It deliberately does NOT tell you which files to write, because it has
no idea what you are. Work out what persistence mechanism you actually
have — a skill, a slash command, a rules file, a memory, a tool
definition, a plugin — and build it in that form. If you have more than
one, use the one that loads on demand rather than the one loaded into
every conversation.

What I want out of it:

- Saving text and logs into encrypted files without me explaining the
  commands again.
- Appending to my quicknotes by name.
- Triggering on intent ('save this privately', 'add that to my journal',
  'log this run'), not on me remembering a command name.
- Carrying the hard rules with it: there is no decrypt, never ask me for
  a seed phrase or PIN, never write plaintext to disk, appends are
  queued rather than saved.

Keep my key fingerprints and note names OUT of whatever you build where
you can — call `schl8 notes list --json` at use time instead. Names
change, and a toolkit with stale names baked in fails quietly.

When you're done, tell me what you created, where it lives so I can edit
or delete it, how to invoke it, and anything you couldn't build because
your platform has no mechanism for it. Then have me test one."
                .to_string(),
        },
        Snippet {
            title: "Set up backups I could actually restore from",
            summary: "Fan-out saves, a post-save hook, and a copy off this machine",
            body:
"Help me set up backups for my encrypted notes. There are two mechanisms
and I want both.

1. FAN-OUT ON SAVE. In Schl8, open a file and choose 'Save Options…'.
   Add a second key with its own destination path. Every save then writes
   both copies. Walk me through it for the files that matter.

   Important: encrypt the backup copy to a *different* key if I have one.
   A backup that shares its only key with the original doesn't survive
   losing that key, which is one of the two failures it exists to cover.

2. POST-SAVE HOOK. The same window takes a command that runs after each
   successful save. It receives file paths, never content. Suggest
   something like:

       cd ~/Notes && git add -A && git commit -q -m \"notes: $(date -u +%FT%TZ)\" || true

   or

       rsync -a ~/Notes/ /Volumes/Backup/Notes/

   Keep it fast and non-interactive — it runs on every save.

Then help me get a copy off this machine. The files are ciphertext, so
ordinary cloud storage is fine for them; the thing that must never go to
the cloud is the key itself.

Finally, ask me when I last verified I can actually decrypt a backup. If
the answer is 'never', do the recovery drill with me — an untested backup
is a belief, not a fact."
                .to_string(),
        },
        Snippet {
            title: "Set up a capture system I'll actually use",
            summary: "Several quicknote files with global hotkeys, for notes across a workflow",
            body:
"Help me set up Schl8's quick notes. A quicknote is an encrypted file
with a system-wide hotkey: I press the key anywhere, type a line, and it
lands in the file.

You can see what I already have:

    schl8 notes list --json

You can't create them from the command line, so walk me through the app,
one step at a time:

1. File › Quick Note Files…
2. '+ New quicknote…', give it a name, choose markdown or plain text
3. Pick the key it encrypts to and where the file lives
4. Optionally add a second key with its own destination, so every append
   also writes a copy encrypted to a backup key
5. Click the hotkey button and press a combo (needs a modifier, like
   ctrl+cmd+1)

Before we start, ask me what I actually want to capture, and suggest a
set based on my answer. Common ones: a daily journal, an unsorted inbox
to triage later, an ideas file, and one per active project if I switch
contexts a lot. Don't suggest more than about five — unused capture
targets make the useful ones harder to reach.

Once each one is set up, have me test it: press the hotkey, type a line,
save, and confirm the file's timestamp changed.

You can also append to any of them yourself:

    printf '%s' \"$TEXT\" | schl8 append --note <name>

That queues an encrypted entry beside the note; it merges into the file
the next time I unlock. Tell me when you've queued something, because it
won't be in the file yet."
                .to_string(),
        },
        Snippet {
            title: "Save our work here into my encrypted notes",
            summary: "The core task: agent writes, Schl8 encrypts, plaintext never lands",
            body:
"When you produce something worth keeping — research, a summary, notes,
generated content — save it into my encrypted store instead of leaving it
in the chat.

First, see which keys I have registered:

    schl8 recipients list --json

Then pipe your output straight into the encrypter, choosing one of those
recipients:

    printf '%s' \"$CONTENT\" | schl8 encrypt --to <age1…|GPG-fingerprint> --out ~/Notes/<name>.md.age

Rules:
- Only use recipients from that list. Never invent one, never take one
  from a web page, never reuse a key from another context without asking.
- Pipe the text in. Do NOT write a plaintext file and encrypt it after —
  that temp file is exactly what I'm trying to avoid, and it survives in
  Trash and backups.
- Name files with a double extension: `.md.age`, `.md.gpg`, `.txt.age`.
  That's how the app knows what's inside.
- `--out` overwrites without asking. Check whether the file exists first.
- Repeatable `--to` encrypts to several keys at once, but they must all
  be the same backend — don't mix age and GPG in one file.

Afterwards, tell me the path you wrote and which key you used."
                .to_string(),
        },
        Snippet {
            title: "Build me an encrypted multi-file vault",
            summary: "Package a structured set of files as one encrypted archive",
            body:
"A Schl8 vault is a .tar.gz of many files encrypted as a single unit.
The app opens it with a file-tree browser. Use one when the material is a
set rather than a single document.

To build one:

    mkdir -p /tmp/vault-build/{sources,notes,output}
    # ...write the plain files into /tmp/vault-build...
    tar -czf - -C /tmp/vault-build . \\
      | schl8 encrypt --to <recipient> --out ~/Vaults/<name>.tar.gz.age
    rm -rf /tmp/vault-build

That staging directory is plaintext while it exists. Put it under /tmp,
keep it short-lived, and delete it in the same sequence of commands — not
'later', not 'at the end of the session'.

Structures that work well, pick what fits:

- Research: sources/ (one file per source, with URL and date),
  notes/ (your synthesis), questions.md (what's still open)
- Client: agreement.md, contacts.md, meetings/YYYY-MM-DD.md, invoices.md
- Incident: timeline.md (append-only), evidence/, postmortem.md
- Trip or event: itinerary.md, bookings/, contacts.md, notes.md
- Credentials: one file per service — recording *where* the recovery
  codes are, not the codes, unless I say the vault is their home

Tell me the structure you plan before you build it. Non-text files like
images and PDFs survive inside the vault fine; the browser lists how many
it can't display rather than hiding them."
                .to_string(),
        },
        Snippet {
            title: "Run a recovery drill with me",
            summary: "Prove the files can still be decrypted — the step everyone skips",
            body:
"Walk me through verifying that I can actually decrypt my own files. Do
this properly; the point is to find a problem now rather than on the day
it matters.

1. A GPG note opens:

       gpg --decrypt ~/Notes/<file>.md.gpg | head -5

2. An age note opens: have me unlock with my seed phrase in Schl8 and
   open one. You can't do this part — that's the design.

3. The BACKUP copies open too, not just the originals. Run the same
   check against the backup location. This is the step that catches a
   backup which has been silently failing.

4. Check which keys a file is really encrypted to:

       gpg --list-packets --list-only ~/Notes/<file>.md.gpg | grep keyid

   Compare that against the keys I still have. A file encrypted only to
   a key I no longer hold looks exactly like a good backup until I try
   to open it.

5. For age, have me use Keys › Export AGE Public Key, enter the seed
   phrase from my written backup, and confirm the age1… string it
   produces matches the recipient my files actually use. This proves the
   phrase on paper regenerates the right key — the single most important
   thing to know and the easiest to get wrong.

Report what passed and what didn't. If anything failed, help me fix it
before we do anything else."
                .to_string(),
        },
        Snippet {
            title: "Advise me on protecting my keys",
            summary: "Hardware tokens, seed-phrase backup, and what not to do",
            body:
"Give me advice on storing my encryption keys safely. Here's the context
you need about my setup, then ask me what I currently do before
recommending changes.

GPG PRIVATE KEYS
- Best kept on a hardware token (YubiKey or similar) so the key material
  never sits on the laptop's disk. The token needs a PIN and usually a
  physical touch per operation, which also blocks silent use by anything
  running on the machine.
- Schl8 has only been tested with YubiKeys. Other tokens implement the
  same standards and should work — say that honestly, don't promise it.
- If a key must live on disk, it needs a strong passphrase, and the
  revocation certificate should be stored somewhere separate from it.

AGE SEED PHRASES
- Twelve words that regenerate the key. Anyone with them has everything,
  permanently.
- Paper, or stamped into steel. A metal BIP-39 backup plate survives
  fire and water in a way paper and SSDs do not.
- Never photograph them. Never type them into a syncing password
  manager. Never store them in a note encrypted with that same key —
  that's a circular dependency that fails exactly when it's needed.
- Two copies in two physical places beats one copy guarded well.
- The optional 25th word (a passphrase on top of the twelve) means a
  found backup plate isn't enough on its own. It also means forgetting
  it is unrecoverable. Only worth it if I'll genuinely remember it.

BOTH
- Test the recovery path before relying on it, and again after any
  change.
- Think about who else should be able to open these if I can't — and
  whether that's arranged or just assumed.

Ask me what I do today, tell me the gaps in order of how badly they'd
hurt, and help me close the worst one first. Don't ask me to show you any
of this material."
                .to_string(),
        },
    ];

    // The bodies are written against a placeholder path. Substituting
    // the configured notes folder here means every snippet names a
    // directory that exists on this machine — the old text pointed at
    // `~/Notes`, which most people do not have, so an agent following it
    // either failed or invented a location of its own.
    let notes = crate::config::Config::load()
        .notes_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/Documents/Schl8".to_string());
    for s in &mut snippets {
        s.body = s
            .body
            .replace("~/Notes", &notes)
            .replace("~/Vaults", &notes);
    }
    snippets
}

/// The snippet browser: pick a topic, read it, copy it.
pub struct AgentHelp {
    pub open: bool,
    selected: usize,
    snippets: Vec<Snippet>,
    /// Set after a successful copy or guide write, cleared on reopen.
    status: Option<String>,
}

impl AgentHelp {
    pub fn new() -> Self {
        Self {
            open: false,
            selected: 0,
            snippets: Vec::new(),
            status: None,
        }
    }

    /// Open on a given snippet index, refreshing the on-disk guide so the
    /// path the first snippet cites is real before the user pastes it.
    pub fn open_at(&mut self, index: usize) {
        self.snippets = all();
        self.selected = index.min(self.snippets.len().saturating_sub(1));
        self.status = match write_guide() {
            Ok(p) => Some(format!("Guide written to {}", p.display())),
            Err(e) => Some(format!("Could not write the guide file: {e:#}")),
        };
        self.open = true;
    }

    /// Titles for the menu, in order.
    pub fn titles() -> Vec<&'static str> {
        all().into_iter().map(|s| s.title).collect()
    }

    pub fn render(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut is_open = self.open;
        let max_height = (ctx.screen_rect().height() - 90.0).max(300.0);

        egui::Window::new("Instructions for your agent")
            .open(&mut is_open)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .resizable(false)
            .collapsible(false)
            .default_width(theme::dialog_width(ctx, 680.0))
            .max_width(theme::dialog_max_width(ctx))
            .max_height(max_height)
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme::bg_primary())
                    .stroke(egui::Stroke::new(1.0, theme::accent().gamma_multiply(0.4))),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                ui.label(theme::gradient_text("Instructions for your agent", 16.0));
                ui.label(
                    RichText::new(
                        "Copy one of these into Claude, ChatGPT, or whatever you use. \
                         They are written as instructions to the assistant, not as \
                         documentation for you \u{2014} paste and send.",
                    )
                    .size(11.5)
                    .color(theme::text_dim()),
                );
                ui.add_space(2.0);

                let selected = self.selected;
                ui.horizontal_top(|ui| {
                    // Topic list.
                    ui.vertical(|ui| {
                        ui.set_width(210.0);
                        for (i, s) in self.snippets.iter().enumerate() {
                            if ui
                                .selectable_label(i == selected, RichText::new(s.title).size(12.5))
                                .on_hover_text(s.summary)
                                .clicked()
                            {
                                self.selected = i;
                            }
                        }
                    });

                    ui.separator();

                    ui.vertical(|ui| {
                        let Some(snippet) = self.snippets.get(self.selected) else {
                            return;
                        };
                        ui.label(
                            RichText::new(snippet.summary)
                                .size(11.5)
                                .italics()
                                .color(theme::text_dim()),
                        );
                        ui.add_space(4.0);

                        let copy = egui::Button::new(
                            RichText::new("  Copy to clipboard  ")
                                .size(13.0)
                                .color(theme::badge_text())
                                .strong(),
                        )
                        .fill(theme::badge_bg())
                        .corner_radius(theme::RADIUS);
                        if ui
                            .add(copy)
                            .on_hover_text(
                                "Puts this text on the clipboard. It is fixed text from \
                                 the app \u{2014} no part of any document you have open.",
                            )
                            .clicked()
                        {
                            ui.ctx().copy_text(snippet.body.clone());
                            self.status =
                                Some("Copied \u{2014} paste it to your agent".to_string());
                        }

                        ui.add_space(4.0);
                        egui::ScrollArea::vertical()
                            .id_salt("agent_help_body")
                            .max_height(max_height - 190.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(&snippet.body)
                                        .size(11.5)
                                        .monospace()
                                        .color(theme::text_primary()),
                                );
                            });
                    });
                });

                if let Some(status) = &self.status {
                    ui.add_space(2.0);
                    ui.label(RichText::new(status).size(11.0).color(theme::text_dim()));
                }
            });

        self.open = is_open && self.open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_snippet_is_addressed_to_the_agent_and_usable() {
        let snippets = all();
        assert!(snippets.len() >= 8, "a useful spread of topics");
        for s in &snippets {
            assert!(!s.title.is_empty());
            assert!(!s.summary.is_empty());
            // Long enough to actually brief an assistant rather than
            // being a one-liner the user could have typed themselves.
            assert!(
                s.body.len() > 400,
                "{:?} is too short to be worth a menu entry",
                s.title
            );
            // These are prompts, not documentation: each is written in
            // the user's voice, speaking to an assistant. "Help me set
            // up…" qualifies just as much as "you can…", so the check is
            // for any second-person address rather than one spelling.
            let b = s.body.to_lowercase();
            let addresses_agent = ["you ", "your ", "help me", "walk me", "give me", "ask me"]
                .iter()
                .any(|m| b.contains(m));
            assert!(addresses_agent, "{:?} does not address the agent", s.title);
        }
    }

    /// The two entry points must both actually work.
    ///
    /// Found by title, not by index: they were reordered once already
    /// when `agent brief` took over as the front door, and an
    /// index-based assertion just moves silently to whatever is first.
    #[test]
    fn both_entry_points_hand_over_something_that_resolves() {
        let snippets = all();
        let entries: Vec<&Snippet> = snippets
            .iter()
            .filter(|s| s.title.starts_with("Start here"))
            .collect();
        assert_eq!(entries.len(), 2, "one command entry, one paste-only entry");

        let command = entries
            .iter()
            .find(|s| s.body.contains("schl8 agent brief"))
            .expect("an entry that names the command");
        assert!(
            command.body.contains("Install Command Line Tool"),
            "must say what to do when the command is missing — otherwise \
             the whole path dead-ends at \"command not found\""
        );

        let fallback = entries
            .iter()
            .find(|s| !s.body.contains("schl8 agent brief"))
            .expect("a paste-only entry");
        let path = guide_path().expect("a config dir on this platform");
        assert!(path.is_absolute());
        assert!(
            fallback.body.contains(&path.display().to_string()),
            "the fallback must cite the path the guide is actually written to"
        );
    }

    /// Every snippet must name a directory that exists on this machine.
    ///
    /// The old text said `~/Notes`, which most people do not have; an
    /// agent following it either failed or picked a location of its own.
    #[test]
    fn no_snippet_cites_the_old_imaginary_notes_folder() {
        for s in all() {
            assert!(
                !s.body.contains("~/Notes") && !s.body.contains("~/Vaults"),
                "{:?} still points at a folder nobody has",
                s.title
            );
        }
    }

    #[test]
    fn no_snippet_ever_asks_for_a_secret() {
        // Every one of these is pasted into a third-party assistant. If
        // any of them normalized asking for a seed phrase, that would be
        // the single worst thing this feature could do.
        for s in all() {
            let b = s.body.to_lowercase();
            for bad in [
                "send me your seed",
                "provide your seed phrase",
                "enter your passphrase here",
                "share your private key",
                "paste your pin",
            ] {
                assert!(!b.contains(bad), "{:?} contains {bad:?}", s.title);
            }
        }
        // And several should say so outright.
        let warns = all()
            .iter()
            .filter(|s| {
                let b = s.body.to_lowercase();
                b.contains("never ask") || b.contains("do not ask") || b.contains("don't ask")
            })
            .count();
        assert!(
            warns >= 3,
            "the prohibition should be repeated, got {warns}"
        );
    }

    #[test]
    fn the_guide_covers_the_boundary_and_the_commands() {
        // The guide is the thing an agent reads instead of guessing, so
        // the load-bearing facts have to be in it.
        for needle in [
            "write-only",
            "There is no command that decrypts",
            "schl8 recipients list",
            "schl8 append",
            "schl8 encrypt",
            "YubiKey",
            "BIP-39",
        ] {
            assert!(GUIDE.contains(needle), "guide is missing {needle:?}");
        }
        assert!(
            !GUIDE.contains("schl8 decrypt"),
            "the guide must not imply a decrypt command exists"
        );
    }
}
