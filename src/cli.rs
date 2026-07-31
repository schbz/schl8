use std::path::PathBuf;

use clap::Parser;

/// schl8 — Schuyler's Lightweight Armored Text Editor
#[derive(Parser)]
#[command(name = "schl8", version, about)]
pub struct Cli {
    /// Path to an encrypted file (.gpg/.age). If omitted, a file picker is shown.
    pub file: Option<PathBuf>,

    /// Headless agent commands (no window, no prompts). Write-only by
    /// design: nothing here can decrypt or touch key material — see
    /// docs/AGENT-DESIGN.md.
    #[command(subcommand)]
    pub command: Option<AgentCmd>,

    /// Open an embedded sample markdown document to preview rendering
    /// without decrypting anything. Debug builds only (UI development).
    #[cfg(debug_assertions)]
    #[arg(long)]
    pub sample: bool,

    /// Open an embedded sample folder archive to preview the file tree
    /// browser without decrypting anything. Debug builds only.
    #[cfg(debug_assertions)]
    #[arg(long)]
    pub sample_archive: bool,

    /// Open the quick-note window on launch (UI preview). Debug builds only.
    #[cfg(debug_assertions)]
    #[arg(long)]
    pub jot: bool,

    /// Start on the locked screen (UI preview). Debug builds only.
    #[cfg(debug_assertions)]
    #[arg(long)]
    pub locked: bool,

    /// Open the Settings window on launch (UI preview). Debug builds only.
    #[cfg(debug_assertions)]
    #[arg(long)]
    pub settings: bool,

    /// Start crawling the opened document immediately, for checking the
    /// animation without driving the UI. Debug builds only.
    #[cfg(debug_assertions)]
    #[arg(long)]
    pub crawl: bool,

    /// Open the Save Targets window on launch (UI preview). Debug builds only.
    #[cfg(debug_assertions)]
    #[arg(long)]
    pub save_targets: bool,
}

/// Headless, prompt-free commands for scripts and agentic platforms.
///
/// The whole surface is write-only: encryption needs only public
/// recipients, appends go through the offline spool, and the listing
/// commands expose public metadata. There is deliberately no `decrypt`.
#[derive(clap::Subcommand)]
pub enum AgentCmd {
    /// Encrypt stdin to one or more recipients and exit.
    Encrypt {
        /// Recipient: an AGE key (age1…) or a GPG fingerprint (hex).
        /// Repeatable, but all recipients must use the same backend.
        #[arg(long = "to", required = true)]
        to: Vec<String>,
        /// Write ciphertext here (atomic, owner-only). Default: stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// ASCII-armor GPG output (.asc). Ignored for AGE.
        #[arg(long)]
        armor: bool,
    },
    /// Append stdin to a registered quicknote via the offline spool.
    /// The entry merges into the note the next time the human unlocks.
    Append {
        /// Registry name or file path of the quicknote.
        #[arg(long)]
        note: String,
        /// Skip the note's blurb template; append stdin verbatim.
        #[arg(long)]
        raw: bool,
    },
    /// List registered quicknote files (public metadata only).
    Notes {
        #[command(subcommand)]
        action: ListAction,
    },
    /// List encryption recipients an agent may encrypt to.
    Recipients {
        #[command(subcommand)]
        action: ListAction,
    },
    /// Show how many offline entries are waiting to merge.
    Pending {
        #[arg(long)]
        json: bool,
    },
    /// Set up an AI assistant to work with Schl8.
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
}

/// The onboarding surface: how an assistant learns what it may do here.
///
/// This exists so the human does not have to paste a wall of text. They
/// say "run `schl8 agent brief`" and the assistant reads instructions
/// that are generated from this machine's actual config, so they cannot
/// drift out of date the way a pasted copy does.
#[derive(clap::Subcommand)]
pub enum AgentAction {
    /// Print a complete briefing, filled in with this machine's real
    /// notes folder, keys, and quicknotes. Safe to run at any time —
    /// it reads config and prints; it changes nothing.
    Brief,
    /// Print a platform-neutral specification for a *persistent*
    /// toolkit, so an assistant can build Schl8 into its own skill /
    /// command / rules system — whatever that happens to be.
    Toolkit {
        /// Emit a JSON manifest instead of prose, for generating from.
        #[arg(long)]
        json: bool,
    },
    /// Write the toolkit straight into Claude Code's skill and command
    /// directories. A convenience for the one layout this build can
    /// verify — every other platform uses `toolkit` instead.
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
    /// Write AGENTS.md into a directory so coding agents pick up the
    /// briefing with no pasting at all.
    Init {
        /// Directory to write into. Default: the current directory.
        dir: Option<PathBuf>,
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
        /// Also write CLAUDE.md with the same content.
        #[arg(long)]
        claude: bool,
    },
}

#[derive(clap::Subcommand)]
pub enum SkillsAction {
    /// Create (or refresh) the skill and slash commands.
    Install {
        /// Show what would be written, change nothing.
        #[arg(long)]
        dry_run: bool,
        /// Overwrite files that Schl8 did not write.
        #[arg(long)]
        force: bool,
    },
    /// Remove the generated files. Only touches ones Schl8 wrote.
    Uninstall,
}

#[derive(clap::Subcommand)]
pub enum ListAction {
    /// Print the list.
    List {
        #[arg(long)]
        json: bool,
    },
}
