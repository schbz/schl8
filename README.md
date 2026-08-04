# Schl8

**Pronounced "schlate"** — the `8` is *ate*. It's a stylization of **SCHLATE**,
which is what the name actually stands for:

> **Sch**uyler's **L**ightweight **A**rmored **T**ext **E**ditor

[![CI](https://github.com/schbz/schl8/actions/workflows/ci.yml/badge.svg)](https://github.com/schbz/schl8/actions/workflows/ci.yml)
[![Nightly build](https://github.com/schbz/schl8/actions/workflows/nightly.yml/badge.svg)](https://github.com/schbz/schl8/releases/tag/nightly)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Platform: macOS 11+](https://img.shields.io/badge/platform-macOS%2011%2B-lightgrey.svg)

A secure, macOS-native GUI for viewing, editing, and re-encrypting encrypted
text and markdown files — using **GPG** (including YubiKey and other smart
cards via `gpg-agent`) or **AGE** derived from a 12-word seed phrase.

Schl8 decrypts `.gpg`, `.asc`, and `.age` files in memory and shows them in a
native window. Plaintext never touches disk, is locked in RAM so it can't be
swapped out, and is overwritten with zeros the moment the document closes.

**[schbz.github.io/schl8](https://schbz.github.io/schl8/)** — project site, if
you'd rather see it laid out than read a long file.

---

## Contents

- [Read this before using Schl8](#-read-this-before-using-schl8)
- [Requirements](#requirements)
- [Install](#install)
- [First five minutes](#first-five-minutes)
- [Features in depth](#features-in-depth)
  - [Opening and reading](#opening-and-reading)
  - [File fingerprints — noticing when something changed](#file-fingerprints--noticing-when-something-changed)
  - [Editing, encrypting, saving](#editing-encrypting-saving)
  - [Quick Note — capture without opening anything](#quick-note--capture-without-opening-anything)
  - [Favorites](#favorites)
  - [Momentum — keep writing or it locks](#momentum--keep-writing-or-it-locks)
  - [Crawl — hands-free reading](#crawl--hands-free-reading)
  - [Locking, and the encrypted stash](#locking-and-the-encrypted-stash)
  - [Encrypted folder archives](#encrypted-folder-archives)
  - [Working with an AI assistant](#working-with-an-ai-assistant)
  - [Appearance and reading comfort](#appearance-and-reading-comfort)
  - [Housekeeping](#housekeeping)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [Configuration reference](#configuration-reference)
- [Security model](#security-model)
- [Building your own](#building-your-own)
- [Contributing](#contributing)
- [License](#license)

---

## ⚠️ Read this before using Schl8

**Schl8 is experimental hobby software. Do not rely on it for anything you
cannot afford to lose or leak.**

Please understand what you are (and are not) getting:

- **No security audit.** Schl8 has never been independently audited. It was
  built carefully — with mlock'd buffers, zeroization, and a documented threat
  model — but "built carefully" is not the same as "verified by professionals."
  Treat every claim in this README as *design intent*, not certified fact.
- **Not a backup tool, not a vault.** Schl8 edits and re-encrypts files you
  already manage with GPG or AGE. If a bug corrupts a save, your data is only
  as safe as your own backups. **Never let a Schl8-managed file be the only
  copy of something important.**
- **Sharp edges by design.** Saving *always* overwrites encrypted files in
  place (atomically, but still in place). Per-file save plans fan one Save out
  to multiple destinations, overwriting all of them. Misconfigure a plan and
  Save will faithfully do the wrong thing everywhere at once.
- **One developer, personal tool first.** Schl8 exists because its author
  wanted it. Issues and PRs are welcome, but there is no support commitment
  and no guaranteed response time.
- **The security model has documented gaps.** See
  [Security model](#security-model) below and [SECURITY.md](SECURITY.md) —
  a root-level attacker, a compromised gpg/pinentry, hardware keyloggers, and
  certain transient-plaintext windows are all explicitly out of scope.

### A better way to think of this project

Schl8 may be most valuable as a **worked example**: a small, readable
codebase showing how to build a security-conscious desktop app in Rust —
mlock'd + zeroized buffers with compile-time trait locks, gpg subprocess
handling without plaintext temp files, atomic durable writes, in-memory
archive extraction with bomb limits, clipboard suppression in an
immediate-mode GUI, macOS Finder/AppleEvent integration, and CI with
supply-chain scanning. If your threat model is serious, **read the source,
take the ideas, and build (or audit) your own** rather than trusting a
stranger's hobby project. The [architecture map](CLAUDE.md) and
[technical critique](TECHNICAL-CRITIQUE.md) are good starting points.

---

## Requirements

- **macOS 11+** (Apple Silicon or Intel)
- **A key.** Either backend works on its own, and you can mix both:

  | Backend | What you need | Best for |
  |---|---|---|
  | **GPG** | [GnuPG](https://gnupg.org/) (`brew install gnupg`) and a key pair — local, or on a YubiKey / smart card via `gpg-agent` | Hardware-backed keys; files you already manage with GPG |
  | **AGE** | Nothing installed. A 12-word BIP-39 seed phrase you keep yourself, optionally with an extra passphrase | No dependencies, no keyring, and a key you can write on paper |

**GnuPG is optional.** With no `gpg` on the system, Schl8 runs AGE-only and
everything else works unchanged — the GPG-specific menus simply don't appear.

For GUI-based PIN entry (recommended when launching from Finder or Spotlight):

```sh
brew install pinentry-mac
```

Then add to `~/.gnupg/gpg-agent.conf`:

```
pinentry-program /opt/homebrew/bin/pinentry-mac
```

And reload the agent: `gpgconf --kill gpg-agent`

## Install

There is no Homebrew tap. Installing from a tap would mean trusting a formula
you didn't read to fetch a binary nobody notarized — for a program that holds
your keys, downloading it deliberately or compiling it yourself is the better
trade.

### Option 1 — Build it from source (recommended)

Requires [Rust](https://rustup.rs/) 1.75 or newer.

```sh
git clone https://github.com/schbz/schl8.git
cd schl8

# Read the code first — that's the point.

# Build + install the macOS app with Finder integration:
./scripts/bundle.sh --install

# …or just the CLI binary:
cargo install --path .

# …or a plain release build:
cargo build --release   # binary at target/release/schl8
```

Run the test suite — which includes real GPG round-trips against an ephemeral
keyring — with `cargo test`.

### Option 2 — Download a build

Two kinds of build are published:

- **[Nightly](https://github.com/schbz/schl8/releases/tag/nightly)** — rebuilt
  from the newest commit on `master` that passed the full test suite. Always
  current, never release-reviewed. The download link never changes.
- **[Tagged releases](https://github.com/schbz/schl8/releases)** — cut
  deliberately, with a [CHANGELOG](CHANGELOG.md) entry.

Either way:

1. Download the `…-macos-universal.zip` (one file works on both Apple Silicon
   and Intel).
2. Unzip it and drag `Schl8.app` into `/Applications`.
3. **Clear the quarantine flag.** The app is open source but not notarized
   with Apple (that requires a paid developer account), so macOS refuses to
   open a downloaded copy until you run:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Schl8.app
   ```

   Only do this if you're comfortable running un-notarized software — that
   trust decision is exactly what the warning at the top of this README is
   about. The paranoid alternative is Option 1.
4. Verify the download against the release's `SHA256SUMS` if you care to:

   ```sh
   shasum -a 256 -c SHA256SUMS --ignore-missing
   ```

Each build also ships a plain `schl8` terminal binary as a `.tar.gz`, if you'd
rather skip the app bundle.

#### Stop repeated "allow access to Desktop" prompts

macOS ties a folder-access grant to an app's code-signing identity. Schl8's
default **ad-hoc** signature changes on every rebuild, so macOS treats each new
build as a new app and re-prompts for Desktop/Documents/Downloads access. To
make the grant stick, create a one-time self-signed certificate — `bundle.sh`
then uses it automatically:

```sh
./scripts/setup-signing.sh        # once — creates a "Schl8 Code Signing" cert
./scripts/bundle.sh --install     # rebuilds signed with it
```

After that the first save to a protected folder prompts once and never again.
The certificate lives only in your login keychain (no admin rights, no system
settings); remove it anytime in Keychain Access. If you have a paid Apple
Developer ID, set `SCHL8_SIGN_ID="Developer ID Application: …"` and `bundle.sh`
uses that instead.

## First five minutes

```sh
schl8                      # launch with the file picker
schl8 document.md.gpg      # open a specific file
```

A reasonable first run:

1. **Launch Schl8.** It lands in the file picker, which also lists recent files
   and your favorites.
2. **Set your notes folder** in Settings (`Cmd+,`) — or accept the default,
   `~/Documents/Schl8`. This is where new files go.
3. **Create a note**: `Cmd+N` for markdown. Type something, then `Cmd+S`. Pick
   a key. The file is encrypted on that first save and never exists as
   plaintext on disk.
4. **Register it as a quicknote**: File → Quick Note Files… → add it. Now
   `ctrl+cmd+j` from *any* application opens a small jot window that appends
   straight into it.
5. **Leave Schl8 running in the menu bar.** Closing the window hides it and
   drops any decrypted content; the hotkeys keep working.

## Features in depth

### Opening and reading

**Open anything encrypted you have the key for.** `.gpg`, `.pgp`, `.asc`, and
`.age` files decrypt into memory and render immediately. Plain `.txt` and `.md`
open too — and saving one always encrypts it, which is the intended way to move
an existing plaintext note under protection.

**Styled markdown rendering** for `.md.gpg` / `.md.asc` / `.md.age`: headings,
emphasis, code blocks, lists, task lists, quotes, and tables. Links are styled
but deliberately **not clickable**, so a document's contents can never end up
in your browser history or a URL bar.

**Finder integration.** The bundle registers as a handler for `.gpg`, `.pgp`,
`.asc`, `.md` and `.txt`. Help → "Install & Default Editor…" makes Schl8 the
default for all of them in one click. Or right-click any encrypted file →
Open With → Schl8. Files can also be dragged onto the window.

**Signature badges.** A signed file shows a verified-signature badge with the
signer on hover, and flags a bad or unverifiable signature rather than
quietly rendering it.

**File identity at a glance.** The status bar shows the encrypted file's
fingerprint (below) and its last-modified time; hover the filename for the
absolute path. Useful when several near-identical backups exist and you need to
know which copy you're looking at.

### File fingerprints — noticing when something changed

Eight hex digits are precise and nearly useless to a human eye. Nobody
remembers `dfdc256a`, so nobody notices when it quietly becomes `9e805359` —
and noticing is the point.

So the hash is **drawn** instead of printed — in the status bar, and inside
every file card on the home screen. Each file gets a small **circuit**: six
nodes placed by digest bytes and joined in sequence by right-angle traces, the
way tracks are routed on a board. Each node is either a square pad or a round
star — its own byte decides — and every element takes its hue from the byte
that placed it.

The identity lives in three channels at once: where the nodes sit, which of
them are square, and what colour everything is. The first two survive with all
colour removed, so the mark still works for the roughly one man in twelve with
a colour-vision deficiency, and on a washed-out projector. The background is
transparent — the circuit sits directly on the theme's own surface — and the
colours are generated in OKLCH at a fixed lightness and chroma, which is why
they stay readable on all sixteen themes instead of coming out muddy on one
and glaring on another.

Because SHA-256 avalanches, one changed byte anywhere in the file redraws the
whole thing. A file you've worked in for a month looks like itself; the moment
it doesn't, you can tell without reading a single digit.

**Hovering** gives you the full 64-character digest, a spoken three-word name
for the file (`golden-elm-folds`), and a sentence explaining what it is. The
name is a memory aid and a way to check a file against someone over the phone —
the full hex is the comparison of record.

**And Schl8 tells you outright.** It remembers what each file hashed to the
last time you opened it, so it doesn't depend on you noticing that a small
picture looks different: a file that changed since your last visit gets a
`⚠ changed` marker in the status bar, and the tooltip says what it was before.
Your own saves don't trigger it — only a change that happened while you weren't
looking. That's the case worth knowing about: a sync client that resolved a
conflict, another machine writing to a shared destination, or a save plan that
isn't doing what you thought.

> **What it's for.** Confirming the note you just opened is the one you left.
> Spotting that a backup destination has drifted from your working copy.
> Checking two machines hold the same file without squinting at hex.

None of this touches plaintext: the hash is of the **ciphertext** on disk, so
every byte it draws is something any observer of the encrypted file could
compute themselves.

**Recent files and stats.** The picker lists recent files with sizes and dates.
View → Statistics adds a live word/character/line card while you read or write.

### Editing, encrypting, saving

**`Cmd+E` toggles edit mode.** The whole window becomes the editor — no inner
text box. Edits live in a `SecureString`: mlock'd, zeroized on drop, and
re-locked if the buffer ever reallocates.

**`Cmd+S` saves in place.** Re-encrypts to the same recipients and atomically
overwrites the source: owner-only temp file, `fsync` of both file and
directory, then rename. A crash mid-save leaves the old file intact.

**`Cmd+Shift+S` — Encrypt & Save As.** Choose recipients, choose `.gpg`
(binary) or `.asc` (armored) or `.age`, choose a location.

**Save Options (per-file save plans)** — File → Save Options…, or the button in
the status bar. A save plan says: encrypt this file to *these* keys, and write
each result to *these* destinations — all of them, atomically, on every Save.

> **Why you'd want it.** One `Cmd+S` writes your working copy on the internal
> disk, a copy on an external drive, and a copy in a synced folder — each
> encrypted to whichever key belongs there. Redundancy with no separate backup
> step and no plaintext intermediate. Duplicate destinations across keys are
> rejected, because that would mean two keys racing to overwrite one path.

**Post-save commands.** A shell command that runs after every save — app-wide,
or per save plan. The written **paths** are exported as `$SCHL8_SOURCE` and
`$SCHL8_DESTINATIONS`; document content is never passed. Good for `rsync` to a
server, a `git commit` in a notes repo, or kicking off a Time Machine-adjacent
backup script.

**Find & replace** (`Cmd+F`) — case-insensitive find with match counts and
jump-to-match; replace one or all while editing.

**Key management** — Keys menu: import, list, and delete GPG public keys, and
export your AGE public key, without leaving the app. "Generate new AGE key…"
creates one from the system CSPRNG, optionally stirring in randomness you type
yourself, and optionally protected by an extra passphrase — the same "25th
word" the unlock screen accepts. With a passphrase set, the twelve words alone
will not open the key, so both have to be written down.

### Quick Note — capture without opening anything

This is the feature most likely to change how you work.

Press the global hotkey (default `ctrl+cmd+j`) from **any** application — a
browser, a terminal, a meeting — and a small jot window appears. Pick a target
file, type, press Enter. The note is appended with an optional timestamp
header, re-encrypted, and atomically saved. The window is resizable, scrolls
for long notes, and remembers where you put it.

`Cmd+J` does the same thing from inside Schl8.

**The quicknote registry** (File → Quick Note Files…, also on the menu-bar
Quick Note submenu and the jot window's Manage… button) is where this gets
powerful. Register up to **25** files. Each one can have:

- **Its own global hotkey** — `ctrl+cmd+1`, `ctrl+cmd+2`, and so on. Bind them
  to the keys of a programmable macro pad and each key jots into a different
  encrypted note, from anywhere, with the window never in the way.
- **Up to 5 encryption keys**, each with one or more destination paths. Every
  append re-encrypts to all of them and overwrites all destinations — so a
  single jot can land in your working copy *and* a backup on another volume,
  under different keys.
- **No explicit keys at all**, in which case appends re-encrypt in place to the
  file's own existing recipients. This is the simple case and the default.

The same window creates brand-new encrypted quicknote files from scratch: pick
the key(s) and location(s) and the file exists immediately, encrypted, having
never been plaintext.

> **Ways people use this.** A `log.md.gpg` bound to one hotkey for work notes
> and `personal.md.age` bound to another. A "passwords I just rotated" note
> that fans out to two drives. A research log you dump URLs and thoughts into
> without breaking flow. A symptom or mood journal that never lands in a cloud
> notes app.

**Offline appends (the spool).** If the session is locked when an append
arrives — from the CLI, or from an agent — the entry is encrypted into a spool
segment instead of being lost or forcing an unlock prompt. It merges into the
real note the next time you unlock. `schl8 pending` says how many are waiting.
Configurable via `spool_when_locked` and `max_pending`.

### Favorites

File → Favorites… pins encrypted files to the menu-bar **Favorites** submenu,
in an order you set by dragging, each with an optional global hotkey.

Where quicknotes are for *writing without looking*, favorites are for *opening
to read or edit* in one keystroke. The same file can reasonably be both.

### Crawl — hands-free reading

`Cmd+Shift+R`, or View → Crawl (auto-scroll). The document begins scrolling by
itself, the way opening titles roll, so a long note can be read without
touching anything.

Everything is adjustable **while it runs**, because the right speed depends on
the document and the reader, not on a setting chosen in advance:

| Key | While crawling |
|---|---|
| `Space` | Pause / resume |
| `↑` / `↓` | Faster / slower |
| `+` / `−` | Larger / smaller text |
| `R` | Reverse direction |
| `Home` / `End` | Jump to the start / end |
| `Esc` or `Q` | Stop and return to normal reading |

Scrolling by hand pauses the crawl and hands the view back to you; it resumes
on its own after `resume_after_seconds` (set that to `0` to keep manual
control until you say otherwise). At either end it does whatever `end_action`
says: `reverse` (the default — turn around and keep going), `loop` (jump to the
other end), or `stop`.

Crawl only runs over a document you're *reading* — it refuses to start in edit
mode, since moving the view out from under the caret is never what you meant.
It goes fullscreen by default, softens the top and bottom edges so lines arrive
and leave rather than being chopped off, and shows a small hint pill that fades
out so it never becomes permanent chrome.

> **What it's for.** Re-reading a long document at a pace that doesn't invite
> skimming. Reviewing something at a distance from the screen — across a desk,
> or without reading glasses, with the text scaled up. Proof-reading, where
> the constant motion makes it much harder for your eye to skate over a
> sentence it has already read six times.

Every part of it is tunable in `[crawl]`; see the
[configuration reference](#configuration-reference).

### Momentum — keep writing or it locks

View → **Momentum**. While it's on, a pause longer than a few seconds (three
by default) locks the document.

That sounds hostile, and it's meant to be — mildly. A first draft stalls when
you stop to judge the sentence you just wrote, and the cure writers actually
use is freewriting: keep the hand moving and fix it later. Momentum makes
stopping cost something, so the easiest path is forward.

**Nothing is destroyed by it.** Locking runs the ordinary lock path, so unsaved
text is encrypted into the [stash](#locking-and-the-encrypted-stash) before the
plaintext is dropped. The penalty is having to unlock and find your thread
again — never lost words. For the same reason it refuses to switch on for a
document with no key to stash to: there the lock would just be deferred and the
mode would look broken. Save the file once, or set a stash key, and it arms.

A countdown sits near the bottom of the window the whole time the mode is armed
and the editor is open — full while you have time, calm until it's nearly out
of it. It's drawn as its own overlay rather than in the status bar specifically
so it survives **focus mode**; the two are meant to be used together, and focus
mode has no status bar to put it in.

The grace period grants time rather than postponing the reckoning: seconds
spent inside it aren't counted against the pause, so when it ends you get the
whole pause back, visibly.

Three settings, under Settings → Momentum:

| Setting | Default | What it does |
|---|---|---|
| Pause | `3s` | How long you may stop before it locks (1–60s) |
| Grace | `5s` | Breathing room after switching it on before the timer starts. Coming back to the editor — from a lock, a dialog, or the edit shortcut — gets three times this, because restarting is harder than starting |
| Countdown | on | Turn it off to make the pressure implicit; it still locks |

Only typing counts — mouse movement keeps the idle auto-lock at bay but not
this one, since jiggling the mouse isn't writing. And it only counts while the
editor is open: a document you're reading was never gathering momentum.

The mode is per session and never persisted. A setting that locks your
document on a pause shouldn't be inherited from the last time the app ran.

> **What it's for.** Morning pages. A first draft you keep rewriting the
> opening of. Any writing where the problem is not what to say but the habit of
> stopping to check.

### Locking, and the encrypted stash

Schl8 locks itself after an idle period (default 5 minutes), on system sleep or
screen lock, and on demand — File → **Lock Now**, or `ctrl+cmd+l`. Locking
closes the document and zeroizes its buffers.

**Unsaved edits are never destroyed, and never block a lock.** When the session
locks with unsaved work, the edits are encrypted into a **stash** under the
config directory and the plaintext is dropped. Restoring them requires the
private key, exactly like the document itself. Nothing is silently discarded
and nothing is left in the clear.

The stash key follows the document's own backend — a GPG file stashes to GPG
even when its save plan also fans out to AGE — unless you set one fixed key for
everything under `[security.stash_key]`. **If no key is available at all, the
lock is deferred rather than losing the work.**

Because that fallback exists, creating a brand-new file with no stash key
configured shows a warning: until it has been saved once, there is no key to
stash it to.

The AGE identity has its own forgetting rules under `[age_lock]` — after idle
time, after a fixed period, when the window closes, on sleep — so you can
decide how often you're willing to retype a seed phrase.

### Encrypted folder archives

Open a `folder.tar.gz.gpg` (or `.age`) and Schl8 extracts it **entirely in
memory** and shows every contained text and markdown file, at any nesting
depth, in a sidebar file tree.

Files can be edited in place. Saving rebuilds the tar — preserving every
non-text entry byte-for-byte — recompresses, re-encrypts, and atomically
replaces the whole archive. Decompression is bounded against zip bombs
(256 MiB total, 16 MiB per entry, 50 000 entries).

> **What it's for.** Keeping a whole notebook, project folder, or archive of
> old correspondence as a single encrypted file, while still being able to open
> it and fix one line.

### Working with an AI assistant

Schl8 gives an assistant a **write-only** surface. It can encrypt into your
notes and queue appends. It **cannot decrypt, unlock, or read anything**, and
there is no command that does.

Start with Help → Install Command Line Tool…, which puts `schl8` on your PATH.
Then:

| Command | What it does |
|---|---|
| `schl8 agent brief` | Prints a complete briefing generated from *your* machine: your notes folder, the keys it may encrypt to, the notes it may append to, and a menu of things it can set up for you. Because it's generated, it can't drift out of date the way pasted text does. |
| `schl8 agent toolkit` | Prints a platform-neutral capability spec, so an assistant can build Schl8 into its own skill / command / rules system and have it available in every future conversation — on platforms Schl8 has never heard of. `--json` for a machine-readable manifest. |
| `schl8 agent init [dir]` | Writes an `AGENTS.md` into a project that points at `brief`, so coding agents pick it up with no pasting at all. |
| `schl8 agent skills install` | For Claude Code specifically: writes the skill and `/schl8:jot` commands directly. Marked as Schl8's, and removable with `uninstall`. |
| `schl8 encrypt --to <recipient>` | Encrypts stdin and writes ciphertext (atomic, owner-only) or prints it. |
| `schl8 append --note <name>` | Appends stdin to a registered quicknote, via the spool if the session is locked. |
| `schl8 notes list` / `recipients list` / `pending` | Public metadata only. |

**Key labels are deliberately left out of the briefing** — they contain real
names and email addresses, and the briefing is expected to reach a third-party
service. Fingerprints and `age1…` recipients are enough to encrypt.

> **Ways people use this.** Have an assistant append a structured daily summary
> to an encrypted log at the end of a session. Keep a redundant encrypted
> archive of research it gathered, written to two drives at once through a save
> plan. Let it file notes into a multi-file system it can add to but never
> read back.

See [docs/AGENT-DESIGN.md](docs/AGENT-DESIGN.md) for the reasoning behind the
write-only boundary.

### Appearance and reading comfort

**16 themes** — 12 dark: `slate` (default), `midnight`, `plum`, `forest`,
`abyss`, `nebula`, `neon`, `ember`, `espresso`, `sakura`, `terminal`,
`phosphor`; and 4 light: `paper`, `linen`, `frost`, `moss`. Every one is
contrast-checked by a test, so none of them ships unreadable.

**Focus mode** (`Ctrl+Cmd+F`) — fullscreen, chrome hidden, text in a readable
column.

**Keyboard Shortcuts** (View → Keyboard Shortcuts) — a floating list of the
shortcuts that work *right now*, read from your own bindings and your keyboard
layout, and filtered by what the app is doing. The motion keys are hidden in
edit mode, where those letters are text rather than commands, and named
correctly on Dvorak, Colemak and Workman, where they aren't `j`/`k`.

**Interface scale** — the "Font size" slider in Settings, which zooms the whole
interface rather than only the body text, for high-density displays or tired
eyes. **Font choice** between the built-in face and
Monaco, Courier, Arial, Georgia, Verdana or Times. **Word wrap** and a
**line-number gutter**, both toggleable from the View menu.

**Keyboard-layout-aware navigation** — the vim-style nav keys follow your
layout: qwerty, dvorak, colemak, or workman.

**Every shortcut is rebindable**, including the global hotkeys: click the
binding in Settings and press the combination you want.

### Housekeeping

**Menu-bar residency.** Schl8 lives in the menu bar. Closing the window hides
the app — dropping any decrypted content first — and the Quick Note, Favorites
and New submenus stay available. Click the Dock or menu-bar icon to bring it
back.

**Start at login.** One checkbox in Settings installs a per-user LaunchAgent,
so Schl8 and its global hotkeys are there from boot.

**Back Up Settings** (File → Back Up Settings…). Your `config.toml` holds no
key material and no document text — but it does hold the *map*: which notes
exist, which keys open them, where the copies go. Rebuilding that by hand after
a disk failure is miserable. This bundles it, plus any held stash entries, into
one `.tar.gz` with a plain-English manifest, optionally encrypted to a key of
your choice. Since it's an ordinary archive, an encrypted backup is also
something Schl8 can open and browse — recovery doesn't depend on a restore
command existing.

**Uninstall** (Help → Uninstall Schl8…). Shows every path it would remove —
config, stash, LaunchAgent, preferences, installed agent skills, the PATH
symlink, the bundle — and removes them to the **Trash**, so a change of heart
is a drag back out. **Your notes are never touched**; every encrypted file
Schl8 ever wrote stays exactly where it is, and the screen says so.

**Check for Updates…** and **Report an Issue…** in the Help menu open the
relevant GitHub pages.

## Keyboard shortcuts

Reading (vim-style keys follow your configured keyboard layout):

| Key | Action |
|-----|--------|
| `j` / `Down` | Scroll down |
| `k` / `Up` | Scroll up |
| `d` / `PageDown` | Page down |
| `u` / `PageUp` | Page up |
| `g` / `Home` | Go to top |
| `G` / `End` | Go to bottom |

Commands:

| Key | Action |
|-----|--------|
| `Cmd+O` | Open file |
| `Cmd+N` | New markdown file |
| `Cmd+Shift+N` | New text file |
| `Cmd+J` | Quick note |
| `Cmd+E` | Toggle edit mode |
| `Cmd+F` | Find & replace |
| `Cmd+S` | Save (re-encrypt in place) |
| `Cmd+Shift+S` | Encrypt & Save As |
| `Cmd+Shift+R` | Crawl (auto-scroll) |
| `Cmd+W` | Close document |
| `Cmd+,` | Settings |
| `Ctrl+Cmd+F` | Focus mode |
| `Ctrl+Cmd+L` | Lock now |
| `Cmd+Q` / `q` | Quit |

Global (work from any application while Schl8 runs in the menu bar):

| Key | Action |
|-----|--------|
| `Ctrl+Cmd+J` | Quick note |
| *(unset)* | Per-quicknote and per-favorite hotkeys you assign |

Every in-app shortcut and every global hotkey can be rebound in Settings by
clicking the binding and pressing a new combination.

## Configuration reference

Everything lives in `~/.config/schl8/config.toml` and is also editable in the
Settings window (`Cmd+,`); changes apply live.

```toml
[app]
menu_bar_resident = true   # false disables the status item & global hotkeys
auto_lock_minutes = 5      # idle minutes before locking (0 disables)
lock_on_sleep = true       # also lock on system/display sleep & screen lock
show_shortcuts = false     # shortcut list (View → Keyboard Shortcuts)
show_stats = false         # live statistics card (View → Statistics)
keyboard_layout = "qwerty" # qwerty · dvorak · colemak · workman (nav keys)
post_save_command = ""     # shell command run after every save;
                           # $SCHL8_SOURCE / $SCHL8_DESTINATIONS hold the
                           # written paths (never content)
notes_dir = ""             # where new files go; "" = ~/Documents/Schl8.
                           # Also the one folder agents may write without asking

[appearance]
theme = "slate"            # dark:  slate (default) · midnight · plum · forest ·
                           #        abyss · nebula · neon · ember · espresso ·
                           #        sakura · terminal · phosphor
                           # light: paper · linen · frost · moss
accent = ""                # optional "#RRGGBB" accent override
font = ""                  # "" (built-in) · monaco · courier · arial ·
                           # georgia · verdana · times
font_scale = 1.0           # interface scale
word_wrap = true           # wrap long lines (off = horizontal scrolling)
line_numbers = false       # line-number gutter (plaintext view + editor)

[quick_note]
hotkey = "ctrl+cmd+j"      # global jot hotkey
spool_when_locked = true   # queue appends while locked instead of failing
max_pending = 500          # cap on queued offline appends
include_timestamp = true
template_markdown = "\n## {date} {time}\n\n{text}\n"
template_text = "\n[{date} {time}]\n{text}\n"
date_format = "%Y-%m-%d"
time_format = "%H:%M"

# Registered quicknotes (up to 25), managed in File → Quick Note Files…
# [[quick_note.notes]]
# name = "worklog"
# source = "/Users/you/Documents/Schl8/worklog.md.gpg"
# hotkey = "ctrl+cmd+1"

[momentum]                 # the writing mode; switched on per session
pause_seconds = 3.0        # stop for longer than this and it locks (1-60)
grace_seconds = 5.0        # breathing room before the timer starts counting
show_countdown = true

[crawl]
speed = 40.0               # points per second
direction_up = true        # text rises (false = falls)
text_scale = 1.3           # zoom applied while crawling
column_width = 720.0       # reading column width in points
pause_on_scroll = true     # manual scrolling takes over
resume_after_seconds = 2.0 # …and hands back after this (0 = never)
end_action = "reverse"     # "stop" · "reverse" · "loop"
fade_edges = true          # soften the top and bottom edges
fullscreen = true
show_hud = true            # transient hint pill on each adjustment

[security]
allow_copy_default = false # allow clipboard copying at startup
suppress_copy_warning = false

[security.stash_key]       # where unsaved edits go when the session locks
use_fixed = false          # true = always use the key below, whatever the
                           # document's own backend is
age_recipient = ""
key_fingerprint = ""
key_label = ""

[age_lock]                 # when the AGE identity is forgotten
forget_idle_minutes = 15
forget_after_minutes = 0   # 0 = no fixed expiry
forget_on_window_close = false
forget_on_sleep = true

[keybindings]              # every one is rebindable in Settings
open_file = "cmd+o"
new_markdown = "cmd+n"
new_text = "cmd+shift+n"
quick_note = "cmd+j"
save = "cmd+s"
save_as = "cmd+shift+s"
toggle_edit = "cmd+e"
close_document = "cmd+w"
settings = "cmd+comma"
find = "cmd+f"
crawl = "cmd+shift+r"
panic_lock = "ctrl+cmd+l"
```

Also stored, and managed through the GUI rather than by hand: `save_plans`,
`age_recipients`, `favorites`, `recent_files`, and `seen_files` — the last
being the remembered fingerprint of each file you have opened (a path and a
hash of the ciphertext, up to 200 entries), which is what lets Schl8 tell you
when one changed while you weren't looking.

The config file stores paths, templates, and preferences only — **never
document content or key material**. The window is always fully opaque;
translucency would let other windows shine through near decrypted text, so it
is not supported.

## Security model

Schl8 is designed to minimize the exposure window of decrypted plaintext:

- **No plaintext on disk** — decrypted content flows from `gpg` stdout (or the
  AGE decrypter) directly into memory; saved files are always encrypted
- **Memory locking** — sensitive buffers are mlock'd to prevent swap-out
- **Zeroization** — all sensitive memory (including edit buffers) is
  overwritten with zeros on drop via volatile writes
- **Compile-time trait locks** — `static_assertions` guarantee the secure
  buffer types can never gain `Clone`/`Debug`/`Display` (no accidental copies
  or logging) and that the editable buffer stays pinned to the UI thread
- **Core dumps disabled** — `RLIMIT_CORE` set to 0 at startup
- **No clipboard by default** — label text is not selectable and Copy/Cut
  events are stripped; copying is an explicit opt-in with a warning
- **Immediate-mode rendering** — egui borrows `&str` from the secure buffer
  each frame; no retained plaintext copies exist in the UI layer
- **gpg resolved to a verified absolute path** — not through `$PATH`
- **Hardened writes** — owner-only (0600) temp file, fsync of file and
  directory, atomic rename, serialized concurrent writes
- **Bounded archive extraction** — decompression/allocation bombs are capped
  (256 MiB total, 16 MiB/entry, 50 000 entries)
- **Unsaved work survives a lock without leaking** — held edits are encrypted
  to the document's own key, never written in the clear

### Not protected against

- Kernel-level memory inspection (root access)
- Hardware keyloggers or screen capture
- Compromised gpg-agent or pinentry binary
- Transient plaintext in the `gpg` subprocess and its stdout pipe before it
  reaches locked memory
- Edits that grow the buffer past its reserved capacity can leave a stale copy
  in freed (unlocked) memory until the OS reuses it
- A single transient cleartext copy that egui may create for one frame when
  recording a text change (the undo history itself is cleared each frame)

Note: encryption uses `--trust-model always`, so any imported public key is
accepted as a recipient — verify fingerprints out of band before encrypting
to a new key.

Note: post-save commands execute whatever is in your config file with your
user's privileges. The hook only ever receives encrypted-file *paths*, but
anyone who can edit `~/.config/schl8/config.toml` can make Schl8 run
commands — keep the config directory writable only by your user.

Found a vulnerability? See [SECURITY.md](SECURITY.md).

## Building your own

If you're here to learn rather than to install, start with:

- [CLAUDE.md](CLAUDE.md) — architecture map and the security invariants every
  change must preserve
- [TECHNICAL-CRITIQUE.md](TECHNICAL-CRITIQUE.md) — an honest self-review of
  the codebase's weaknesses and the fixes that followed
- [docs/AGE-DESIGN.md](docs/AGE-DESIGN.md) — seed-phrase key derivation, and
  why the salt is frozen
- [docs/SPOOL-DESIGN.md](docs/SPOOL-DESIGN.md) — how offline appends survive a
  locked session
- [docs/AGENT-DESIGN.md](docs/AGENT-DESIGN.md) — the write-only agent boundary
- `src/crypto/secure_buf.rs` — the mlock'd/zeroized buffer types and their
  compile-time trait assertions
- `src/crypto/gpg.rs` + `src/crypto/keys.rs` — subprocess-based GPG without
  plaintext temp files; atomic durable writes
- `.github/workflows/` — CI with fmt/clippy/tests and `cargo-deny`
  supply-chain scanning

## Contributing

Issues and pull requests are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md)
for the build/test loop and, more importantly, the **security invariants a
change must not break**. In short:

- CI enforces `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`
- `cargo test` must pass (integration tests need `gpg` installed)
- Plaintext never touches disk, and lives only in the secure buffer types

## License

[MIT](LICENSE) — Copyright (c) 2026 Schuyler J Sloane

---

[Project site](https://schbz.github.io/schl8/) ·
[Releases](https://github.com/schbz/schl8/releases) ·
[Changelog](CHANGELOG.md) ·
[Security policy](SECURITY.md) ·
[Contributing](CONTRIBUTING.md)
