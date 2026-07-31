# Schl8 Roadmap

Working document for planned features. Ordered roughly by priority; each release
should stay small and shippable. Security invariants in `CLAUDE.md` apply to all
of these. For the wider brainstorm of candidate ideas, see
[FEATURE-IDEAS.md](FEATURE-IDEAS.md) — items graduate from there to here.

## 0.2 — Reading experience

- [x] **Markdown rendering** — custom renderer over `pulldown-cmark`
      (`src/document/markdown.rs` parses to blocks, `src/ui/markdown.rs`
      paints). Headings, emphasis, code, lists, task lists, quotes, tables,
      rules. Links styled but not clickable (URL opens would leak content
      into browser history). Polish ideas: real bold/italic font weights
      (egui defaults have one weight; would need to load system fonts),
      syntax highlighting in code blocks, readable max text width.
- [x] **Drag & drop** — drop a `.gpg`/`.asc` file onto the window to open it
      (ignored while editing to protect unsaved edits).
- [x] **Save back to source (Cmd+S)** — re-encrypts to the original
      recipients (parsed from ciphertext packets, no PIN) and atomically
      overwrites the source; falls back to Save As when recipients are
      unknown (new files, plaintext imports, anonymous recipients).
- [x] **Quick Note (jot)** — Cmd+J / global hotkey / menu-bar item appends a
      templated blurb to a chosen encrypted file (decrypt → append →
      re-encrypt → atomic overwrite). `src/document/append.rs`,
      `src/ui/quicknote.rs`, `src/tray.rs`, `src/hotkey.rs`.
- [x] **Settings file** — `~/.config/schl8/config.toml` (`src/config.rs`):
      quick-note targets/templates/hotkey, menu-bar residency.
- [x] **Font size controls** — interface font scale in Settings, persisted
      in the config.

## 0.3 — Directories & sessions

- [x] **New file** — "New Markdown"/"New Text" buttons on the start screen
      and File menu (Cmd+N); opens empty in edit mode, first Encrypt & Save
      names the file, which the app then adopts as the current document.
- [x] **Encrypted folder archives** — `folder.tar.gz.gpg` (the tar-then-encrypt
      workflow) opens in a sidebar file-tree browser; all text/markdown files
      at any nesting depth are extracted entirely in memory
      (`src/document/archive.rs`, `src/ui/filetree.rs`). Read-only for now;
      the selected file can be re-encrypted to its own `.gpg` via Save As.
      Follow-ups: edit-an-entry + re-pack archive, zip support, entry search.
- [ ] **Directory browsing (loose files)** — open a folder of individual
      `.gpg` files and show them in the same sidebar tree; navigate without
      leaving the app.
- [x] **Auto-lock** — idle timeout (default 5 min) plus lock on
      sleep/screen-lock (`src/macos_power.rs` observes NSWorkspace +
      distributed notifications); zeroizes buffers and shows a locked
      screen with re-decrypt unlock. Config: `auto_lock_minutes`,
      `lock_on_sleep`.
- [x] **In-document search** — Cmd+F find & replace with match counts and
      jump-to-match (single-document view). Search state must
      live in secure buffers too.

## 0.4 — Distribution & polish

- [x] **Proper .app bundle** — `./scripts/bundle.sh [--install]` builds
      `Schl8.app` with document-type associations; Finder opens arrive via
      an NSAppleEventManager `odoc` handler (`src/macos_open.rs`). Plain
      `.txt`/`.md` also open directly (saving always encrypts).
- [ ] **Code signing + notarization** so Gatekeeper doesn't block downloads
      (the bundle script currently applies an ad-hoc signature).
- [ ] **Homebrew cask** (for the .app) alongside the existing formula.
- [x] **Light theme** and theme switching — paper and linen presets, live
      switching in Settings.
- [x] **Signature verification** — decrypt captures `--status-fd` and shows
      a verified/​bad-signature badge in the status bar
      (`SignatureStatus`). Still open: optional **sign-on-save**
      (sign-then-encrypt), which needs the user's secret signing key.
- [ ] **YubiKey UX** — detect card presence, show touch-required hint during
      decrypt (gpg `--status-fd` gives progress events).

## Open-source readiness

- [ ] CONTRIBUTING.md (build, test, security-invariant expectations for PRs)
- [x] SECURITY.md (threat model — expand README section — and vuln reporting contact)
- [ ] Issue/PR templates; CI badge in README
- [ ] Screenshots/GIF in README before announcing

## Known limitations to document or fix

- `gpg` subprocess buffers plaintext in unlocked memory inside `Command::output()`
  before it reaches `SecureBuffer` (and pipe buffers are kernel-side). Acceptable,
  but should be stated in the threat model.
- `SecureString` reallocation during editing can strand an unzeroizable stale
  copy in freed memory; mitigated by 256 KiB reserved slack (`EDIT_SLACK`).
- Encryption uses `--trust-model always` — any imported key is accepted. Consider
  surfacing key trust in the recipient picker instead.
- Key manager shows one row per UID; a key with multiple UIDs appears multiple
  times and deleting one row deletes the whole key.
