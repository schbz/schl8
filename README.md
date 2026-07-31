# Schl8

**Schuyler's Lightweight Armored Text Editor**

A secure, macOS-native GUI for viewing, editing, and re-encrypting encrypted text and markdown files — GPG (with YubiKey support via gpg-agent) or AGE from a 12-word seed phrase.

Schl8 decrypts `.gpg`, `.asc`, and `.age` files in memory and displays them in a native window. Plaintext never touches disk, is locked in RAM to prevent swapping, and is securely erased when the document closes.

---

## ⚠️ Read this before using Schl8

**Schl8 is experimental hobby software. Do not rely on it for anything you cannot afford to lose or leak.**

Please understand what you are (and are not) getting:

- **No security audit.** Schl8 has never been independently audited. It was
  built carefully — with mlock'd buffers, zeroization, and a documented threat
  model — but "built carefully" is not the same as "verified by professionals."
  Treat every claim in this README as *design intent*, not certified fact.
- **Not a backup tool, not a vault.** Schl8 edits and re-encrypts files you
  already manage with GPG. If a bug corrupts a save, your data is only as safe
  as your own backups. **Never let a Schl8-managed file be the only copy of
  something important.**
- **Sharp edges by design.** Saving *always* overwrites encrypted files in
  place (atomically, but still in place). Per-file save plans fan one Save out
  to multiple destinations, overwriting all of them. Misconfigure a plan and
  Save will faithfully do the wrong thing everywhere at once.
- **One developer, personal tool first.** Schl8 exists because its author
  wanted it. Issues and PRs are welcome, but there is no support commitment,
  no guaranteed response time, and breaking changes may happen before 1.0.
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

## Features

- **Native macOS GUI** built with egui/eframe
- **GPG decryption** via the system `gpg` binary — supports passphrase and YubiKey PIN entry
- **In-memory editing** — edit decrypted content in a secure buffer, then re-encrypt and save
- **New encrypted notes** — create a markdown or text file from scratch; it's encrypted on first save and never exists as plaintext on disk
- **Encrypted folder browsing & editing** — open a `folder.tar.gz.gpg` archive and browse all contained text/markdown files (any nesting depth) in a sidebar file tree, extracted entirely in memory. Files can be edited in place: saving rebuilds the tar (preserving all non-text entries byte-for-byte), re-compresses, and re-encrypts the whole archive atomically
- **File identity at a glance** — the status bar shows the encrypted file's short SHA-256 and last-modified time; hover the filename for its absolute path
- **Quick Note** — Cmd+J (or a global hotkey from anywhere, default `ctrl+cmd+j`) opens a small jot window: pick a target file, type, press Enter — the note is appended, re-encrypted, and atomically saved. The window is resizable, scrolls for long notes, and remembers its size and position
- **Quicknote registry** — register up to 25 quicknote files, each pickable from the menu-bar Quick Note submenu and each with an optional dedicated global hotkey (e.g. `ctrl+cmd+1` — ideal for programmable keypad macros that jot into a specific note). Each file can specify up to 5 encryption keys with one or more destination paths per key: every append re-encrypts to all keys and overwrites all destinations (e.g. your working copy plus a backup). The manager (File → Quick Note Files…) also creates new encrypted quicknote files from scratch
- **Post-save commands** — run a shell command after every save (app-wide, or per save plan) with the saved paths in the environment: automatic backups, rsync to a server, git commits, whatever your workflow needs. Paths only — document content is never passed
- **Works with your AI assistant, write-only** — `Help → Install Command Line Tool…` puts `schl8` on your PATH, then one line ("run `schl8 agent brief`") gives an assistant a briefing generated from your actual setup: your notes folder, the keys it may encrypt to, the notes it may append to, and a menu of things it can set up for you. It can encrypt its output straight into your notes and queue appends; it **cannot** decrypt, unlock, or read anything, and there is no command that does. `schl8 agent init` drops an `AGENTS.md` into a project so coding agents pick it up with no pasting at all. Key labels are left out of the briefing so your name and email don't travel with it. `schl8 agent toolkit` goes further: it prints a *platform-neutral* spec — capabilities, exact commands, live data, and the rules that must survive — so your assistant builds Schl8 into its own skill/command/rules system and it's available in every future conversation, on platforms Schl8 has never heard of. For Claude Code specifically, `schl8 agent skills install` writes the skill and `/schl8:jot` commands directly (marked, and removable with `uninstall`). See [docs/AGENT-DESIGN.md](docs/AGENT-DESIGN.md)
- **Start at login** — one checkbox in Settings installs a per-user LaunchAgent so Schl8 (and its global hotkeys) are always available
- **Menu-bar residency** — Schl8 lives in the menu bar; closing the window hides it (dropping any decrypted content first). Click the Dock icon or the menu-bar item to bring it back
- **Finder integration** — installs as the handler for `.gpg`/`.pgp`/`.asc` (with its own declared file types), `.md`, and `.txt`; Help → "Install & Default Editor…" makes Schl8 the default for all of them in one click
- **Auto-lock** — after an idle period (default 5 min), or on system sleep / screen lock, the document is closed and its buffers zeroized
- **Signature badges** — signed files show a verified-signature badge (signer on hover) or flag a bad/unverifiable signature
- **Save in place (Cmd+S)** — re-encrypts to the same recipients and atomically overwrites the source
- **Save Targets** — per-file save plans: encrypt each Save to one or more chosen keys, each written to one or more destinations, all overwritten atomically on every Save
- **Plaintext import** — plain `.txt`/`.md` files open directly; saving always encrypts
- **Key management** — import, list, and delete GPG public keys from within the app
- **Secure memory** — mlock'd buffers, zeroize-on-drop, core dumps disabled, compile-time assertions locking the secure types' trait properties
- **Styled markdown rendering** for `.md.gpg`/`.md.asc` — headings, emphasis, code blocks, lists, task lists, quotes, and tables (links are styled but not clickable, so document contents can't leak into browser history)
- **Find & replace** (`Cmd+F`) — case-insensitive find with match counts and jump-to-match; replace one or all while editing
- **Focus mode, live statistics, light & dark themes, font choice, word wrap, line numbers, configurable shortcuts, and keyboard-layout-aware vim navigation** (qwerty/dvorak/colemak/workman)

## Requirements

- macOS 11+ (Apple Silicon or Intel)
- [GnuPG](https://gnupg.org/): `brew install gnupg`
- A GPG key pair (on a YubiKey/smart card via gpg-agent, or a local key)

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

### Option 1 — Download the app (easiest)

1. Go to the [**Releases** page](https://github.com/schbz/schl8/releases/latest)
   and download `Schl8-vX.Y.Z-macos-universal.zip` (one download works on
   both Apple Silicon and Intel).
2. Unzip it and drag `Schl8.app` into `/Applications`.
3. **Clear the quarantine flag.** The app is open source but not notarized
   with Apple (that requires a paid developer account), so macOS will refuse
   to open the downloaded copy until you run:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Schl8.app
   ```

   Only do this if you're comfortable running un-notarized software — that
   trust decision is exactly what the warning at the top of this README is
   about. The paranoid alternative is Option 2: build it yourself from source
   you've read.
4. Launch Schl8 from Applications or Spotlight. To open encrypted files
   from Finder: right-click a `.gpg` file → Open With → Schl8 (or Get Info →
   "Open with:" → Schl8 → "Change All…" to make it the default).

Each release also ships plain `schl8` terminal binaries as `.tar.gz` files
for both architectures if you'd rather skip the app bundle.

### Option 2 — Clone and build it yourself (recommended for the cautious)

Requires [Rust](https://rustup.rs/) 1.75+.

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

Run the test suite (includes real gpg round-trip tests against an ephemeral
keyring) with `cargo test`.

#### Stop repeated "allow access to Desktop" prompts

macOS ties a folder-access grant to an app's code-signing identity.
Schl8's default **ad-hoc** signature changes on every rebuild, so macOS
treats each new build as a new app and re-prompts for Desktop/Documents/
Downloads access. To make the grant stick, create a one-time self-signed
signing certificate — `bundle.sh` then uses it automatically:

```sh
./scripts/setup-signing.sh        # once — creates a "Schl8 Code Signing" cert
./scripts/bundle.sh --install     # rebuilds signed with it
```

After that the first save to a protected folder prompts once and never
again. The certificate lives only in your login keychain (no admin
rights, no system settings); remove it anytime in Keychain Access. If you
have a paid Apple Developer ID, set `SCHL8_SIGN_ID="Developer ID
Application: …"` and `bundle.sh` uses that instead.

### Option 3 — Homebrew

```sh
brew tap schbz/tap
brew install schl8
```

(The tap tracks tagged releases; it may lag the Releases page.)

## Usage

```sh
# Launch with file picker
schl8

# Open a specific file
schl8 document.md.gpg
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `j` / `Down` | Scroll down |
| `k` / `Up` | Scroll up |
| `d` / `PageDown` | Page down |
| `u` / `PageUp` | Page up |
| `g` / `Home` | Go to top |
| `G` / `End` | Go to bottom |
| `Cmd+O` | Open file |
| `Cmd+N` | New markdown file |
| `Cmd+J` | Quick note |
| `Cmd+E` | Toggle edit mode |
| `Cmd+F` | Find & replace |
| `Cmd+S` | Save (re-encrypt in place) |
| `Cmd+Shift+S` | Encrypt & Save As |
| `Cmd+W` | Close document |
| `Cmd+,` | Settings |
| `Ctrl+Cmd+F` | Focus mode |
| `q` | Quit |

Every in-app shortcut (and the global quick-note hotkey) can be rebound in
Settings by clicking the binding and pressing a new combo.

### Encrypting Files

1. Open an encrypted file (or enter edit mode to modify it)
2. **File > Encrypt & Save As** (or `Cmd+Shift+S`)
3. Select one or more recipient public keys
4. Choose output format: `.gpg` (binary) or `.asc` (ASCII armor)
5. Pick a save location — the file is always saved encrypted

### Quick Notes

Press `Cmd+J` in the app — or the global hotkey (default `ctrl+cmd+j`) from
anywhere while Schl8 runs in the menu bar — to jot a note. Pick a target
encrypted file (remembered between notes), type, and press Enter. The note
is appended with an optional timestamp header, re-encrypted, and saved
atomically.

The menu-bar **Quick Note** item is a submenu listing your registered
quicknote files — choose one to jot straight into it. **File → Quick Note
Files…** (also reachable from the tray and the jot window's Manage…
button) manages the registry: up to 25 files, each with either no explicit
keys (appends re-encrypt in place to the file's own recipients) or up to 5
keys, each key with its own destination path(s) that are all overwritten
on every append. The same window creates brand-new encrypted quicknote
files: pick the key(s) and location(s) and the file is created immediately —
it never exists unencrypted.

### Configuration

Everything lives in `~/.config/schl8/config.toml` and is also editable in
the Settings window (`Cmd+,`); changes apply live:

```toml
[quick_note]
hotkey = "ctrl+cmd+j"
template_markdown = "\n## {date} {time}\n\n{text}\n"
template_text = "\n[{date} {time}]\n{text}\n"
date_format = "%Y-%m-%d"
time_format = "%H:%M"

[app]
menu_bar_resident = true   # set false to disable the status item & hotkey
auto_lock_minutes = 5      # idle minutes before locking (0 disables)
lock_on_sleep = true       # also lock on system/display sleep & screen lock
show_stats = false         # show the live statistics card (View → Statistics)
keyboard_layout = "qwerty" # qwerty · dvorak · colemak · workman (nav keys)
post_save_command = ""     # optional shell command run after every save;
                           # $SCHL8_SOURCE / $SCHL8_DESTINATIONS hold the
                           # written paths (never content) — e.g. a backup hook
notes_dir = ""             # where new files go; "" = ~/Documents/Schl8.
                           # Also the one folder agents may write without asking

[security]
allow_copy_default = false # allow clipboard copying at startup
suppress_copy_warning = false

[appearance]
theme = "slate"            # dark:  slate (default) · midnight · plum · forest ·
                           #        abyss · nebula · neon · ember · espresso ·
                           #        sakura · terminal · phosphor
                           # light: paper · linen · frost · moss
accent = ""                # optional "#RRGGBB" accent override
font = ""                  # "" (built-in) · monaco · courier · arial ·
                           # georgia · verdana · times (system fonts)
word_wrap = true           # wrap long lines (off = horizontal scrolling)
line_numbers = false       # line-number gutter (plaintext view + editor)

[keybindings]              # in-app shortcuts (the global hotkey is above)
open_file = "cmd+o"
new_markdown = "cmd+n"
quick_note = "cmd+j"
save = "cmd+s"
save_as = "cmd+shift+s"
toggle_edit = "cmd+e"
close_document = "cmd+w"
settings = "cmd+comma"
```

The config file stores paths, templates, and preferences only — never
document content or key material. The window is always fully opaque —
translucency would let other windows shine through near decrypted text,
so it is not supported.

## Security model

Schl8 is designed to minimize the exposure window of decrypted plaintext:

- **No plaintext on disk** — decrypted content flows from `gpg` stdout directly into memory; saved files are always GPG-encrypted
- **Memory locking** — sensitive buffers are mlock'd to prevent swap-out
- **Zeroization** — all sensitive memory (including edit buffers) is overwritten with zeros on drop via volatile writes
- **Compile-time trait locks** — `static_assertions` guarantee the secure buffer types can never gain `Clone`/`Debug`/`Display` (no accidental copies or logging) and that the editable buffer stays pinned to the UI thread
- **Core dumps disabled** — RLIMIT_CORE set to 0 at startup
- **No clipboard by default** — label text is not selectable and Copy/Cut events are stripped; copying is an explicit opt-in with a warning
- **Immediate-mode rendering** — egui borrows `&str` from the secure buffer each frame; no retained plaintext copies exist in the UI layer
- **gpg resolved to a verified absolute path** — not through `$PATH`
- **Hardened writes** — owner-only (0600) temp file, fsync of file and directory, atomic rename, serialized concurrent writes
- **Bounded archive extraction** — decompression/allocation bombs are capped (256 MiB total, 16 MiB/entry, 50k entries)

### Not protected against

- Kernel-level memory inspection (root access)
- Hardware keyloggers or screen capture
- Compromised gpg-agent or pinentry binary
- Transient plaintext in the `gpg` subprocess and its stdout pipe before it reaches locked memory
- Edits that grow the buffer past its reserved capacity can leave a stale copy in freed (unlocked) memory until the OS reuses it
- A single transient cleartext copy that egui may create for one frame when recording a text change (the undo history itself is cleared each frame)

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
- `src/crypto/secure_buf.rs` — the mlock'd/zeroized buffer types and their
  compile-time trait assertions
- `src/crypto/gpg.rs` + `src/crypto/keys.rs` — subprocess-based GPG without
  plaintext temp files; atomic durable writes
- `.github/workflows/` — CI with fmt/clippy/tests and `cargo-deny`
  supply-chain scanning

## Contributing

Issues and pull requests are welcome. Keep in mind:

- CI enforces `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`
- Every change must preserve the security invariants listed in
  [CLAUDE.md](CLAUDE.md) — in particular: plaintext never touches disk, and
  plaintext lives only in the secure buffer types
- `cargo test` must pass (integration tests need `gpg` installed)

## License

[MIT](LICENSE) — Copyright (c) 2026 Schuyler J Sloane
