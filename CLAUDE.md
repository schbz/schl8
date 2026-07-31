# Schl8

Secure macOS-native GUI (egui/eframe) for viewing, editing, and re-encrypting
encrypted text/markdown files. Two backends: GPG (YubiKey and other smart
cards via gpg-agent) and AGE derived from a 12-word seed phrase. GPG is
optional at runtime — with no `gpg` installed the app runs AGE-only.

`AGENTS.md` is a pointer to this file; this one is canonical.
[CONTRIBUTING.md](CONTRIBUTING.md) covers the same invariants for outside
contributors.

## Commands

- Build: `cargo build` — Run: `cargo run -- [file.md.gpg]`
- Test: `cargo test`
- Lint (CI-enforced): `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
- Test fixtures live in `test_files/` (encrypted to the developer's key)
- Never run the app against the real config while testing — set
  `XDG_CONFIG_HOME` to a scratch directory first

## Architecture

- `src/app.rs` — eframe App: state machine (`FilePicker → Decrypting →
  Viewing/ViewingArchive/Error`), menu/keyboard action dispatch, dialog and
  quick-note orchestration. All UI runs on the main thread; decryption and
  quick-note appends run on background threads via mpsc.
- `src/crypto/gpg.rs` — decryption + recipient listing via the system `gpg`
  binary (subprocess pipes; `--list-packets --list-only` never needs a PIN).
- `src/crypto/keys.rs` — key listing/import/delete, encryption
  (`gpg --encrypt`), `encrypt_overwrite` (temp file + atomic rename).
- `src/crypto/secure_buf.rs` — `SecureBuffer` (read-only) and `SecureString` (editable):
  mlock'd, zeroize-on-drop containers for plaintext.
- `src/security/memory.rs` — mlock/munlock wrappers, core-dump disable.
- `src/document/` — loading (encrypted, plaintext import, folder archives),
  `append.rs` (quick-note append flow), `archive.rs` (in-memory tar.gz
  extraction), `spool.rs` (offline append segments, age or GPG),
  `naming.rs` (encrypted-extension rules), `stash.rs` (unsaved edits
  encrypted to the document's own key when the session locks), file-type
  detection (double extension: `.md.gpg`).
- `src/ui/` — viewer, markdown renderer, filetree sidebar, quicknote window,
  dialogs, menu, statusbar, keybindings, theme, `favorites_manager.rs`,
  `stamp.rs` (cached on-disk hash/size), `textnav.rs` (line/search math),
  `agent_help.rs` (copyable agent briefings — now two entry points plus
  a list of setup suggestions — and the AGENT-GUIDE fallback it writes
  into the config dir), `crawl.rs` (animated reading — the motion is
  pure arithmetic over a scroll offset, kept out of the egui code so it
  is testable).
- `src/agent_brief.rs` — `schl8 agent brief` (the live briefing an
  assistant reads instead of pasted text, generated from config into
  `assets/agent-brief.md`) and `schl8 agent init` (writes an
  `AGENTS.md` that points at `brief` rather than copying it). Recipient
  *labels* are deliberately excluded from the output — they carry names
  and email addresses and it is expected to reach a third-party service.
- `src/agent_toolkit.rs` — `schl8 agent toolkit [--json]`: a
  *platform-neutral* spec for a persistent toolkit (capabilities, exact
  commands, live notes/keys, invariants). Schl8 deliberately does not
  know where any platform stores skills — the local agent reads this and
  builds it with its own machinery, which is the only thing that works
  for platforms this repo has never heard of. A test asserts the spec
  never names a platform's file layout.
- `src/agent_skills.rs` — the one exception: writes the toolkit straight
  into Claude Code's `~/.claude/skills` + `~/.claude/commands/schl8`,
  the only layout this build can verify. Every generated file carries an
  ownership marker; uninstall removes only marked files, so a file the
  user wrote under the same name is never deleted. Not a pattern to
  repeat per platform.
- `src/config_backup.rs` — File → Back Up Settings. Bundles the config
  directory (settings + held edits + a plain-text manifest) as a
  `.tar.gz` built in memory, optionally encrypted to any registered GPG
  or age recipient. The bundle is a normal archive, so an encrypted
  backup is also a vault the app can open.
- `src/uninstall.rs` — Help → Uninstall. `plan()` reports every path
  that belongs to the app without touching disk; `execute()` moves them
  to the Trash via Finder (recoverable, and the only way the app can
  remove its own running bundle), leaving notes and keys alone.
- `src/cli_install.rs` — Help → Install Command Line Tool. Symlinks the
  binary onto PATH, preferring a directory that is already writable and
  visible to the login shell over prompting for an administrator. Asks
  the login shell for PATH, because a GUI app does not inherit it.
- `src/config.rs` — `~/.config/schl8/config.toml` (paths/quicknotes/hotkey
  only — never content or key material). Also favorites, interface
  scale, and `app.notes_dir` (default `~/Documents/Schl8`) — where new
  files go and the one place an agent is told it may write;
  `clamp_limits` keeps a hand-edited scale from hiding the settings
  window that would fix it.
- `src/tray.rs` + `src/hotkey.rs` — menu-bar residency and the global
  hotkeys: the quick-note combo plus per-quicknote and per-favorite ones.
  Submenus for Quick Note, Favorites and New. While hidden, no plaintext is
  retained (documents are closed before the window hides).
- `src/macos_open.rs` — Finder "Open With" AppleEvent (`odoc`) handler.
- `scripts/bundle.sh` — builds `Schl8.app` with file associations.
- `.github/workflows/` — `ci.yml` (fmt/clippy/test + cargo-deny),
  `release.yml` (tag → version check → universal app + CLI tarballs +
  SHA256SUMS → *draft* release), `test-build.yml` (manual, builds what a
  release would ship without tagging), `homebrew.yml` (formula bump, on
  release *published* — a draft's tarball URL 404s).

## Security invariants — preserve these in every change

1. **Plaintext never touches disk.** Decrypted content flows gpg-stdout → memory;
   saves always go through `gpg --encrypt`. Never add plaintext export, temp files,
   or logging of document content.
2. **Plaintext lives only in `SecureBuffer`/`SecureString`.** Don't copy it into
   plain `String`/`Vec` (UI code borrows `&str` per frame — immediate mode).
3. **After mutating a `SecureString`** (e.g. egui TextEdit), call
   `relock_if_moved()` so the mlock follows any reallocation.
4. **Locking never destroys unsaved work, and unsaved work never blocks a
   lock.** On lock, unsaved edits are encrypted into `stash/` and the
   plaintext is dropped; restoring them requires the private key. The
   stash key follows the **document's own backend** (a GPG file stashes
   to GPG even when its save plan also fans out to age), unless
   `security.stash_key.use_fixed` names one key for every file. If no key
   is available the lock is deferred instead — never a silent discard.
5. **No clipboard for document content.** `Copy`/`Cut` events are stripped
   in `App::update` and `selectable_labels` is false, so no widget can put
   decrypted text on the clipboard. The one exception is
   `ui/agent_help.rs`, which copies fixed strings compiled into the binary
   (agent instructions) via `ctx.copy_text` — an output command, not one
   of the filtered input events. Nothing there derives from a document or
   a key. Don't extend that exception.
6. **Core dumps stay disabled**; keep `lock_down()` first in `main`.
7. Error/toast messages may include filenames but never document content.

## Conventions

- Rust 2021, `anyhow` for app errors, `thiserror` for typed gpg errors.
- UI style constants live in `src/ui/theme.rs` — no hardcoded colors in new UI
  (some legacy hardcoded colors remain in dialogs).
- Keep clippy clean with `-D warnings`; run `cargo fmt` before finishing.
- See `ROADMAP.md` for planned features and their intended design notes.
