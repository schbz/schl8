# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.43] - 2026-08-02

### Added

- **Momentum — a writing mode that locks the document if you stop
  typing.** Switched on per session from View. A pause longer than the
  configured few seconds (three by default) locks the session; unsaved
  text goes through the ordinary lock path, so it is encrypted into the
  stash first and the penalty is having to unlock, never lost words.

  It exists for the blank page rather than for security. A first draft
  stalls when you stop to judge the sentence you just wrote, and the
  cure writers actually use is to keep the hand moving — so stopping
  now costs something. Only typing counts (mouse movement holds off the
  idle lock but is not writing), it only counts while the editor is
  open, and it refuses to arm on a document with no key to stash to,
  where the lock would be deferred and the mode would look broken.

  The clock starts when the editor opens, not when the lock fired —
  otherwise the grace period spends itself while you are still typing
  a passphrase, and the mode re-locks the instant your text comes
  back. Returning to the editor gets three times the configured grace,
  because restarting is harder than starting. The pause is measured
  from the end of that grace rather than from your last keystroke, so
  the grace grants time instead of merely postponing the reckoning;
  measured the other way, a long grace had already spent the pause by
  the time it ended, and the countdown went straight to zero without
  ever appearing.

  The countdown is its own overlay rather than a status-bar item
  specifically so it survives focus mode — the two are meant to be used
  together. Pause, grace period and countdown visibility are in
  Settings; the mode itself is never persisted, because a setting that
  locks your document on a pause should not be inherited from the last
  time the app ran.
- **A keyboard-shortcut panel** (View → Keyboard Shortcuts), listing
  the shortcuts that work *right now* — read from your own bindings and
  your keyboard layout, and filtered by what the app is doing.
- **The idle auto-lock timeout is now in Settings**, alongside the
  lock-on-sleep toggle. It had always been configurable and had never
  been reachable except by editing `config.toml` by hand.
- **A countdown beside Lock Now**, showing how long until the session
  locks itself for being idle. It appears only after thirty seconds of
  inactivity: a counter that starts the moment you stop typing is a
  distraction, and one that appears with seconds left is a jump scare.

### Changed

- The picker's main button now reads **"Open a Note or Vault"**. It has
  always also opened plain `.md`/`.txt` (encrypted on their first save)
  and folder vaults, so "Open Encrypted File" told a third of the truth.

### Removed

- **The keybinding hints in the status bar.** Two fixed strings that
  were wrong twice over: they named the QWERTY motion keys to everyone,
  though those keys are remapped to physical position on Dvorak,
  Colemak and Workman, and they named `Cmd+E` whatever the user had
  actually rebound it to. The panel above replaces them with something
  that asks the config.

## [1.0.42] - 2026-08-02

### Added

- **A generated AGE key can now carry an extra passphrase**, the same
  optional "25th word" the unlock screen already accepts — so a key made
  in Schl8 can be protected the same way as one brought to it. The words
  and the passphrase together open the key; either alone opens nothing.

  It has to be typed twice. A mistyped BIP-39 passphrase does not fail —
  it silently derives a *different, perfectly valid* key. When
  unlocking that costs you a retry; at generation it is unrecoverable,
  because you would write down twelve words, store a recipient, and
  never be able to reach the key again. Generation is refused outright
  if the two copies differ or the box is ticked with nothing typed,
  which would otherwise produce an unprotected key that looked
  protected. Changing the passphrase after generating keeps the same
  twelve words and re-derives the recipient, rather than leaving a
  stale `age1…` on screen for someone to copy into their keyring.

### Fixed

- **A long warning turned the toast into a tall column across the
  middle of the window.** The notification had no width of its own, so
  egui wrapped it at whatever narrow default was going, and the
  200-character "save before locking" warning shown on every new file
  stood there for twelve seconds covering the document it was talking
  about. Toasts are now explicitly wide and short, sit low against the
  bottom edge, and carry a ✕ — twelve seconds is long enough to read
  something, and far too long to be stuck with it.
- **"Generate new AGE key…" crashed the app.** Two faults stacked on
  top of each other. The dialog asked for `f32::INFINITY` width for its
  entropy meter — an idiom `TextEdit` reads as "fill the row", but
  `ProgressBar` takes literally, laying out an infinitely wide rect that
  became NaN one subtraction later and tripped an assertion inside egui.
  The dialog killed the process before drawing a frame of itself. And
  because it also opened centred at the default window order, directly
  behind the larger Manage Public Keys window that launched it, there
  was nothing on screen to suggest what had happened — the button simply
  appeared inert. The meter now sizes to the available width, and the
  dialog draws in the foreground layer. Four tests render every mode of
  the dialog headlessly, which is what would have caught this.
- **Placeholder text in input boxes was too faint, on every theme.**
  egui derives a field's hint by blending the text colour halfway
  toward `noninteractive.weak_bg_fill` — and that was the one widget
  fill the theme never set, so it stayed at egui's own default grey,
  a colour belonging to none of the sixteen palettes. Hints came out
  at 3.4:1 on the default dark theme and 2.6–2.8:1 on all four light
  ones — the "Name" and "age1…" prompts in Manage Public Keys being
  the ones people actually noticed. The fade target is now each
  palette's `text_dim`, which lands the blend halfway between body
  text and that dim colour: 6.5–9.4:1 across every theme, against
  10.6–14.4:1 for text you have typed. Placeholders stay visibly
  secondary without being a squint.

  A test now checks both the typed text and the placeholder against
  the actual input-box background for all sixteen palettes, in the app
  window and in the jot, and holds placeholders to the same 4.5:1 as
  real text rather than the 3:1 large-text allowance — a hint that
  clears 3:1 on paper can still be unreadable on screen. The existing
  readability test only ever compared text against `bg_primary`, which
  is not what a text field is painted on; that is how this shipped.
- **Menu bar submenus sometimes refused to open in fullscreen.** A
  macOS fullscreen window owns the whole screen, including the strip
  where the system menu bar reveals itself on hover — so a click on our
  menu bar could go to that reveal instead of to us. In fullscreen the
  menu bar now sits below the strip. Windows merely zoomed to fill the
  screen are unaffected: they sit under a permanently visible system
  menu bar and need no inset.

## [1.0.41] - 2026-08-01

### Added

- **File fingerprints.** Every hash is now drawn instead of printed —
  in the status bar, and inside every picker and quicknote card. Eight
  hex digits are precise and nearly useless to a human eye: nobody
  remembers `dfdc256a`, so nobody notices when it becomes `9e805359`,
  and noticing is the point.

  The mark is a small **circuit**: six nodes placed by digest bytes,
  joined in sequence by right-angle traces the way tracks are routed on
  a board, each node a square pad or a round star as its own byte
  decides. Identity lives in three channels at once — position, shape,
  colour — and the first two survive with all colour removed, so the
  mark still works for the roughly one man in twelve with a
  colour-vision deficiency, and on a washed-out projector. The
  background is transparent, so one design serves every theme; colours
  are generated in OKLCH at fixed lightness and chroma rather than from
  raw RGB bytes, and a test checks every one of the 256 hues clears the
  worst-case background of both theme families.

  Hovering gives the full 64-character digest, a spoken three-word name
  for the file (`golden-elm-folds`) for saying aloud or checking with
  someone on the phone, and a sentence explaining what any of it means.

- **Schl8 now says when a file changed while you weren't looking.**
  It remembers what each file's ciphertext hashed to when you last
  opened it — up to 200 files — and flags a difference in the status
  bar, with the previous fingerprint in the tooltip. Relying on someone
  to notice that a small picture looks different is relying on luck.

  Your own saves never trigger it, and a file Schl8 has never seen
  before is not a change: both would train people to dismiss the
  warning. It catches the case worth catching — a sync client that
  resolved a conflict, another machine writing to a shared
  destination, or a save plan doing something other than you thought.

### Changed

- **The hex badge is gone from the status bar.** The fingerprint is what
  gets recognised at a glance, and the only moment anyone wants digits
  is the moment they are comparing them — at which point they want all
  sixty-four, which is what hovering gives. Eight of them sitting there
  permanently was noise.
- `FileStamp` now carries the whole 32-byte digest instead of the four
  bytes behind the old `hash8` field. The picture is drawn from all of
  it: at four bytes, two unrelated files sharing a prefix would have
  drawn as the same fingerprint. Short forms are derived from the digest
  where needed, so there is no second copy to fall out of step.

### Fixed

- **The home screen's two lists overlapped in a narrow window.** Each
  card enforces a minimum width, so once half the window dropped below
  it, Recent's cards ran into the Quick Notes column. Below 520 points
  the lists now stack vertically instead, each card getting the full
  width, in one shared scroll.
- **The jot window's "esc" did nothing when clicked.** It looked like a
  control and behaved like a caption: the title-bar drag handle is added
  after the header, so it sat on top and swallowed the click, leaving
  the physical Esc key as the only way to close the window. It is now a
  real control, and the drag handle cuts its rect out.

Nothing here reads plaintext. The hash covers the **ciphertext** on
disk, so every byte drawn is something any observer of the encrypted
file could compute themselves.

## [1.0.40] - 2026-08-01

Documentation and repository work; no change to the program itself.

### Added

- **A build is now published on every commit.** A permanent `nightly`
  pre-release is rebuilt whenever code lands on `master` and the test
  suite passes, so there is always a current `Schl8.app` to download at
  a URL that never changes. Doc-only commits skip the build. It stays a
  *pre-release* deliberately, which keeps it out of `/releases/latest`
  so a development build can never be served in place of a tagged one.
- **A much longer README.** How the name is pronounced (*schlate* — the
  `8` is *ate*, and SCHLATE is what the acronym spells) and every
  shipped feature, with what it is for rather than only what it does.
  Crawl, the encrypted stash, favorites, settings backup, uninstall,
  interface scale and Lock Now had not been documented anywhere.
- `CONTRIBUTING.md`, leading with the security invariants a change must
  not break, and `CODE_OF_CONDUCT.md`.
- Feature-request and PR templates; an issue-template config that routes
  vulnerability reports to a private advisory instead of the public
  tracker.
- Dependabot for cargo and GitHub Actions. It pairs with the cargo-deny
  gate already in CI: cargo-deny says a dependency became dangerous,
  Dependabot opens the pull request that fixes it.
- CI and nightly status badges in the README.

### Changed

- `AGENTS.md` was a byte-for-byte duplicate of `CLAUDE.md` — two copies
  that were certain to drift apart eventually. It now points at it.
- `CLAUDE.md` described the app as GPG-only in its opening line, written
  before the AGE backend existed.

### Removed

- **Homebrew.** There was no tap, so the workflow and the `brew install`
  instructions pointed at something that did not exist. Installing from a
  tap would also mean trusting an unread formula to fetch an un-notarized
  binary, which is a worse trade than downloading the build deliberately
  or compiling it. Worth revisiting once the app is signed and notarized.

### Fixed

- The README listed GnuPG as a requirement. It has been optional since
  the AGE backend landed — with no `gpg` installed, Schl8 runs AGE-only.

## [1.0.4] - 2026-07-31

### Fixed

- **Typing after the quick-note hotkey went to the wrong app.** Pressing
  the global hotkey opened the jot window but left the keystrokes behind
  in whatever application you were already in, so the note you typed
  landed in someone else's search box.

  Three things have to happen for a hotkey-summoned window to receive
  typing, and the hotkey path did none of them. macOS routes keystrokes
  to the active *application*, so Schl8 has to come forward first — a
  focused window inside a background app still watches you type
  somewhere else. The jot's viewport then has to become the key window;
  every other menu-bar action already asked for that, and this one never
  did. Finally the text field takes the caret, and that request is now
  repeated for a few frames rather than made once: the window is not key
  on the frame it is created, so a single request arrives too early and
  is dropped.

  Verified against the real path — another app frontmost, hotkey pressed
  — where Schl8 now comes forward with the caret in the field, and
  without the fix the other app stays frontmost and keeps the typing.

## [1.0.3] - 2026-07-31

### Added

- **New files now say when their text is not protected yet.** Until a
  file is saved for the first time it has no key of its own, so unsaved
  text cannot be encrypted into the lock stash and *Lock Now* would
  discard it. 1.0.2 made that visible after the fact; this makes it
  visible before.

  Creating a new file with no stash key configured shows a warning that
  stays up long enough to read, naming both ways out: save the file, or
  set a stash key under Settings › Security. It is skipped entirely when
  a stash key is already configured, because then there is no gap.

  The risk lasts until the first save, which is longer than any message
  stays on screen, so the status bar carries an **UNPROTECTED** chip for
  as long as it applies. It replaces the existing "LOCK PAUSED" chip in
  that state rather than sitting beside it: that one's explanation
  promises Schl8 "never silently discards your edits", which is exactly
  what is not true here.

## [1.0.2] - 2026-07-31

### Fixed

- **Locking a never-saved file made unlocking fail.** Create a new note,
  lock the session, unlock — and the app reported that encryption failed
  because the file was not found. On locking it remembered the document's
  path so it could reopen it afterwards, but a document that has never
  been saved carries a placeholder name (`untitled.md.gpg`) that has
  never existed on disk. Unlock then asked gpg to decrypt it, and gpg
  answered "No such file or directory" — which reads, to someone who has
  just locked their screen, like the app losing their work. It now
  remembers a path only when that file actually exists, so unlocking a
  new file simply returns to an empty session. Saved files are still
  reopened as before, and a file deleted or unmounted while locked is
  forgotten rather than failing on the way back in.

- **Unsaved text in a new file could vanish silently.** The same gap had
  a worse half. A brand-new file has no key of its own, so the encrypted
  stash that normally protects unsaved edits across a lock had nothing to
  encrypt to. The automatic locks already refuse to lock in that state,
  but *Lock Now* proceeded, and the failure went only to stderr — the
  text was gone with nothing on screen to say so. The locked screen now
  states plainly that unsaved text was discarded, and why, and how to
  stop it happening again: save the file once, or set a fixed stash key
  in Settings.

## [1.0.1] - 2026-07-31

### Added

- **File → Back Up Settings…** writes everything needed to rebuild this
  setup on another Mac — quicknotes, save plans, recipients, hotkeys,
  and any edits held from a locked session — as a single `.tar.gz`.
  Losing the config loses no encrypted file, but it loses the *map*:
  which of forty `.gpg` files was the journal, and which key opens it.

  The backup can be **encrypted to any registered key**, GPG or age, and
  defaults to one when a key exists. The config holds no key material
  and no note text, but it does hold file paths and the names on your
  keys, which is worth sealing before it reaches a backup disk. The
  archive is built in memory, so an encrypted backup never passes
  through a plaintext temp file. A plain-text `ABOUT-THIS-BACKUP.txt`
  inside explains how to restore it, for whoever opens it later with no
  memory of today.

  Because the bundle is an ordinary `.tar.gz`, an encrypted backup is
  also a vault Schl8 can already open and browse — recovery does not
  depend on a restore command existing by then.

- **Help → Uninstall Schl8…** removes the application and everything it
  installed: settings and held edits, the login item, preferences, the
  command-line symlink, and the Claude Code skill and slash commands.
  Dragging the bundle to the Trash leaves all of that behind, and
  nothing else on the machine knows those files belong together.

  The confirmation screen lists the real paths, says in its first line
  that **encrypted notes and keys are never touched**, highlights the
  two things that cannot be rebuilt, offers a backup without losing your
  place, and requires typing a word rather than clicking a checkbox.
  Everything goes to the Trash rather than being unlinked, so an
  uninstall regretted thirty seconds later is a drag back out. The app
  bundle is trashed last: had it gone first, a later failure would leave
  litter with no app left to clean it up.

## [1.0.0] - 2026-07-31

First release under the name **Schl8**. The application is the one
documented in the entries below; this release renames it, settles the
public interfaces, and marks them stable.

### Changed

- **The name, throughout.** Binary and command are `schl8`, the bundle
  is `Schl8.app` with identifier `com.functiondesk.schl8`, settings live
  in `~/.config/schl8/`, new files default to `~/Documents/Schl8`, and
  post-save hooks receive `$SCHL8_SOURCE` / `$SCHL8_DESTINATIONS`.

### BREAKING

- **Seed-phrase AGE identities are not interchangeable with the earlier
  project.** The HKDF salt is now `schl8.age.identity.v1`. The salt is an
  input to the derivation, so the same twelve words produce a *different*
  X25519 identity here — `.age` files written by the predecessor cannot
  be opened by Schl8, and vice versa. Anything you want to carry across
  must be decrypted with the original tool and re-encrypted here. This
  was deliberate, and the constant is frozen for v1: see
  `docs/AGE-DESIGN.md` §3.
- Offline spool segments and lock stashes use the `schl8-spool/v1` and
  `schl8-stash/v1` envelopes. Queued entries and held edits belonging to
  the predecessor are not read by Schl8; merge or restore them there
  first.

### Added

- **Release automation.** A tag push builds a universal (Apple Silicon +
  Intel) app bundle and per-architecture CLI tarballs, publishes
  `SHA256SUMS` alongside them, and leaves a **draft** release to review
  before anything is downloadable. The run refuses to start when the tag
  and `Cargo.toml` disagree, because a published release's assets cannot
  be replaced afterwards.
- **`test-build` workflow** — builds exactly what a release would ship,
  as a throwaway artifact, without tagging anything.
- **Notarization support**, dormant until Apple credentials are
  configured; the build stays unsigned and works as before without them.
- The Homebrew formula bump moved to its own workflow, triggered when a
  release is *published* rather than drafted.

## [0.9.9] - 2026-07-31

### Added

- **Quick capture, expanded.** Pin files to a menu-bar **Favorites**
  submenu with drag-and-drop reordering and optional per-favorite global
  hotkeys; the homepage now shows recent files and quicknotes as two
  lists with file sizes; and an interface **font scale** setting resizes
  the whole UI.
- **Unsaved edits survive locking.** When the session locks (idle,
  sleep, or the new File → Lock Now), unsaved edits are first encrypted
  to the document's own key into a stash and restored on the next
  unlock — locking never discards work, and work never blocks a lock.
  A `security.stash_key` setting can pin one fixed key for every file.
- **Crawl mode** (View → Crawl): the document auto-scrolls at a chosen
  speed for hands-free reading, with keyboard control of speed, size,
  direction, and pause; manual scrolling takes over and the crawl
  resumes when you stop.
- **Offline appends now work for GPG notes too.** The spool that queues
  entries while the app is locked writes segments with either backend,
  so `schl8 append` covers every registered quicknote.
- **A notes folder.** New files default to a configurable directory
  (`~/Documents/Schl8` out of the box, Settings → Files), which is
  also the one place agents are told they may write without asking.
- **Help → Install Command Line Tool…** links `schl8` into a directory
  on your PATH — asking your login shell which directories those are,
  and preferring one that needs no administrator password.
- **`schl8 agent brief`** prints a complete assistant briefing
  generated from the machine's live config (real keys as bare
  fingerprints, real notes, the real notes folder), so instructions can
  never go stale; **`schl8 agent init`** writes an `AGENTS.md` that
  points at it; **`schl8 agent toolkit [--json]`** prints a
  platform-neutral spec an assistant can turn into standing skills on
  any platform; and **`schl8 agent skills install|uninstall`** writes
  (and cleanly removes) a Claude Code skill and `/schl8:jot` commands
  directly. Recipient labels are excluded from all agent output — keys
  travel as fingerprints, never as names and email addresses.
- **Help → Agent Toolkit…** and a reorganized **Instructions for your
  agent** menu: two entry points (one-line command, paste-only
  fallback) plus copyable briefings for backups, capture setup, vaults,
  the recovery drill, and key safety.

- **AGE encryption as a second backend, driven by a 12-word seed
  phrase.** Alongside GPG, Schl8 can now encrypt with
  [age](https://age-encryption.org) using an X25519 identity derived from
  a BIP-39 mnemonic (derivation frozen as v1 — see
  `docs/AGE-DESIGN.md`). The private key is rebuilt on unlock into an
  mlock'd buffer and **never written to disk**; quitting wipes it. An
  optional 25th-word passphrase is supported.
- **Choose the encryption method and key when you save.** Encrypt & Save
  As offers a GPG/AGE selector; Save Options (per-file save plans) and
  quick notes both accept AGE recipients, so one document can fan out to
  a GPG working copy and an AGE backup at different locations.
- **Generate a new AGE key** from Manage Public Keys: 32 bytes of OS
  CSPRNG mixed with optional user-typed entropy via HKDF-SHA256, with a
  live entropy meter. The phrase is shown once for write-down.
- **Import AGE public keys** (`age1…`) with a name and date, stored
  alongside GPG keys.
- **AGE-only mode.** With no `gpg` installed Schl8 stays fully usable:
  the backend selector is fixed to AGE, GPG import is disabled with an
  explanation, and a startup notice points at Unlock AGE Identity.
- **Configurable AGE identity lifetime** (`[age_lock]`, in Settings):
  forget after idle, a hard ceiling since unlock, on display sleep, or on
  closing the window to the menu bar. Relevant because closing the window
  with menu-bar residency **does not quit the app**.
- **Check for Updates** (Help menu) and a changelog link on the About
  dialog's version line. The check is manual only, never automatic, and
  uses the system `curl` rather than linking a TLS stack.
- Finder "Open With" now offers Schl8 for `.age` files.

- **Report an issue**: a link on the start screen and a Help →
  "Report an Issue…" menu item open the project's GitHub issue tracker
  in the browser, pre-filled with a bug-report template (which reminds
  reporters never to paste decrypted content). A `.vscode` "Preview
  Homepage" task serves `docs/` locally for previewing the future
  GitHub Pages site

- **Recent files** on the start screen: the last 10 opened files, each
  showing its short SHA-256 hash, last-saved time, and when you last
  opened it — click to reopen. Only paths and open times are stored in
  the config; hash and saved-time are computed live from the encrypted
  file so they can never go stale. Missing files show dimmed
- **New logo**: a folded-corner note page with a keyhole in the
  cyan→violet identity gradient, on a neutral dark squircle that sits
  well beside every theme. The menu-bar item now uses a dedicated
  one-tone template glyph, so macOS renders it crisply in both light and
  dark menu bars. All sizes are generated procedurally
  (`cargo run --bin gen_icons`) so the logo has one source of truth

- **One-click default-app registration**: Help → "Install & Default
  Editor…" → "Make Schl8 the default" registers Schl8 via
  LaunchServices as the default for encrypted files (.gpg, .pgp, .asc —
  including encrypted folder archives), markdown (.md), and plain text
  (.txt). Reversible in Finder's Get Info like any other app
- The app bundle now declares proper **Uniform Type Identifiers**:
  exported UTIs for OpenPGP files (macOS ships none) and
  `LSItemContentTypes` for text/markdown, so Schl8 reliably appears in
  Finder's "Open With" list and can be chosen as the default. `--install`
  re-registers the bundle with LaunchServices (unregistering first, so
  stale claims from an older build can't linger)

- **List items, table cells, and line-numbered rows now actually wrap**:
  egui labels inside horizontal layouts default to extending off screen
  rather than wrapping, so bulleted/numbered markdown items (and rows
  with the line-number gutter) ignored word wrap and ran past the window
  edge with no way to reach the text. Every text label now sets its wrap
  mode explicitly from the Word Wrap setting
- **Focus mode always wraps**: it renders a fixed readable column, where
  horizontal scrolling isn't available — with wrap off, long markdown
  lines were unreachable. Word Wrap is now forced on while focus mode is
  active (the setting itself is untouched)

- **Readable on every theme**: egui's widget visuals (button fills,
  default text, window chrome, hover states) are now rebuilt from the
  active palette whenever the theme changes — previously they were set
  once at startup, so switching themes (especially to a light one) left
  buttons and labels with the old palette's colors. Toasts, the Discard
  button, statusbar badges, dialogs, and the error/picker screens now
  derive their colors from the theme with contrast-checked text instead
  of hardcoded dark fills
- **Line numbers now appear in view mode for markdown** files too: each
  rendered block (heading, paragraph, code block, …) shows its source
  line in the gutter. Plaintext view numbers every line as before
- **Word wrap now applies to rendered markdown**: with wrap off,
  headings, paragraphs, and code blocks extend horizontally with
  scrolling instead of silently wrapping
- The editor's line-number gutter uses the editor's exact font metrics,
  so gutter rows align with text rows

- **Find & replace** (`Cmd+F`, Edit menu): case-insensitive find with a
  match counter and next/previous jumping in both view and edit mode;
  replace one / replace all while editing (operating directly on the
  secure buffer — matching never copies plaintext into unlocked memory)
- **Light themes**: `paper` (white background, near-black text, blue
  accents) and `linen` (warm off-white with terracotta accents)
- **Font choice** (Settings → Appearance): built-in Hack, or system
  fonts — Monaco, Courier New, Arial, Georgia, Verdana, Times New
  Roman — applied live
- **Word Wrap toggle** (View menu, persisted): off switches the
  plaintext viewer and editor to horizontal scrolling
- **Line Numbers toggle** (View menu, persisted): left gutter in the
  plaintext view (always) and the editor (when wrap is off, where rows
  map 1:1 to lines)

- **Re-encrypt button** in view mode: opens Save Targets to save an
  identical copy of the open content to a new location and/or under a
  different key (works for archives too)
- **Settings "Apply" button**: try settings live with the window still
  open, without writing them to disk; "Apply & Save" persists as before

- **Edit inside encrypted folder archives**: files in a
  `folder.tar.gz.gpg` archive can now be edited and saved. Saving
  rebuilds the tar in memory — preserving every other entry
  byte-for-byte, including images and other non-text files —
  re-compresses it if the source was compressed, and re-encrypts the
  whole archive to its original recipients (or its save plan),
  overwriting atomically. Switching files with unsaved edits is blocked
  until you save or discard
- Status bar shows the **short SHA-256 hash and last-modified time** of
  the encrypted file on disk (in place of the SCHL8 wordmark), so you
  can verify at a glance which version you're looking at; hovering the
  **filename shows its absolute path**
- An **Edit button** in the status bar when viewing (same as Cmd+E),
  including in archive view

- **Post-save commands**: run a shell command in the background after
  successful saves — app-wide (Settings → Automation, runs after every
  save and quick-note append) and/or per save plan (Save Targets window).
  The command gets `$SCHL8_SOURCE` and `$SCHL8_DESTINATIONS` (paths
  only — never document content), enabling automatic backups, server
  uploads, git commits, etc.
- **Per-quicknote global hotkeys**: each registered quicknote file can
  have its own system-wide hotkey (e.g. `ctrl+cmd+1`) that opens the jot
  window preselected on that file — built for programmable keypads that
  jot into specific notes with one physical key
- **Start at login** (Settings → Automation): installs a per-user
  LaunchAgent so Schl8 starts with macOS and its hotkeys are always
  available; unchecking removes it
- **Project homepage** in `docs/` ready for GitHub Pages (Settings →
  Pages → deploy from `master` `/docs`)

- **Quicknote registry** (File → Quick Note Files…, or the menu-bar
  tray): register up to 25 quicknote files, each with up to 5 encryption
  keys and one or more destination paths per key. Appending a quick note
  to a registered file re-encrypts the combined content to every key and
  overwrites all of that key's destinations (e.g. your copy + a backup
  volume). Files without explicit keys keep the classic behavior
  (re-encrypt in place to their own recipients)
- The menu-bar **Quick Note item is now a submenu** listing every
  registered quicknote — pick one to jot straight into it — plus
  "New Quick Note" and "Manage Quick Notes…" entries
- The manager can **create a new encrypted quicknote file from scratch**:
  choose key(s) and location(s), and the file is encrypted and written
  immediately (a markdown starter heading for `.md` files)
- The registry lives in the config as `[[quick_note.notes]]`; the old
  flat `targets` list migrates automatically

### Changed

- The status bar was decluttered: the SCHL8 wordmark and MD/TXT tag
  are gone, EDIT reads EDITING, and compact layouts kick in earlier so
  controls never overlap at narrow widths.
- The homepage (GitHub Pages site) was rewritten around everyday
  capture, with diagrams for redundant archives, encrypted logs,
  vaults, and the recovery drill.
- The experimental document-templates feature was removed.

- The AGE backend is written **"AGE"** throughout the UI and site, to
  avoid it reading as the English word.
- **Save Options is consistent across view and edit mode.** Both now open
  the per-file save plan (keys, destinations, post-save hook); previously
  edit mode's button opened a different dialog under a similar name.
  Encrypt & Save As remains on Cmd+Shift+S.
- The status bar reflows to two rows on narrow windows instead of
  clipping, and the panel sizes to its content.
- New logo: the page mark is bent into a fat "S" while keeping the
  keyhole. Regenerated at every size, plus the menu-bar glyph.

- The start screen now leads with the **Recent files list** front and
  center; the large logo shows only on first run (before any history).
  The subtitle is now the full name it stands for —
  "Schuyler's Lightweight Armored Text Editor"

- The view-mode re-target button is renamed **"Save Targets"**
- **Translucency is fully removed**: the window is always opaque
  regardless of any `opacity` value in old configs — other windows can
  no longer shine through next to decrypted text

- A save-plan or quicknote rule with destinations but **no key selected
  now defaults to the file's own key** instead of erroring — "no key
  chosen" means "keep the key this file already uses". The key dropdown
  shows "File's own key (default)", and a keyless rule can no longer be
  stored and fail later at save time (multisave also rejects it cleanly
  as a last resort)
- The **opacity slider is removed from Settings** and the default is now
  fully opaque, so decrypted text never has other windows shining
  through it; `[appearance] opacity` in the config file remains for
  those who explicitly want translucency

- Per-quicknote hotkeys are now bound by **click-then-press capture**
  (like the Settings shortcuts) instead of typing the combo as text
- The statusbar's re-target button is renamed to **"Save Options…"** and
  uses explicit theme colors so its label is readable on every palette
  preset

- **Streamlined saving**: the statusbar "Encrypt & Save" button now
  saves with the file's own key(s) to its own location (or its save
  plan) without asking anything — matching Cmd+S. A new
  "Different Key/Location…" button beside it opens the recipient/
  location picker for the cases where you actually want to change them
- **Edit mode is now a single full-window surface** — the inner
  rectangular text box is gone; the editable text fills the content area
  exactly like read mode. Edit mode shows via the caret and the EDIT
  badge in the statusbar
- The **quick-note window is resizable**, scrolls when the note grows
  past the visible area (long pastes stay reachable), and remembers its
  size and position across restarts

### Fixed

- `schl8 notes list` reported every GPG-backed quicknote as
  `appendable: false` long after appends to them worked; the flag now
  mirrors exactly what the spool can encrypt.
- Several UI labels used characters missing from the bundled fonts and
  rendered as boxes; a test now scans every UI string literal and fails
  on any glyph either bundled font lacks.
- The stash asked for an AGE identity when locking a GPG document; it
  now follows the document's own backend.
- Crawl mode no longer sticks at the ends of the document, and the
  scroll wheel works normally while it runs.

- Re-encrypting an AGE file no longer stacks extensions (`.age.age`).
- Opening Save Options on an AGE file preselects the identity it was
  decrypted with (AGE ciphertext records no recipient, so the GPG
  "file's own key" default could not apply).
- Saving a quick note to a locked AGE target now prompts for the seed
  phrase and resumes the save, instead of only printing a warning.
- The Open dialog no longer greys out valid files. rfd flattens all
  filters into one macOS allow-list and treats `"*"` as a literal
  extension, so the "All files" option allowed nothing.

- **macOS stops re-prompting for Desktop/Documents access on every
  rebuild.** The prompt persists a grant keyed to the app's code
  signature; Schl8's ad-hoc signature changed on each rebuild, so
  macOS forgot the grant and re-prompted. `bundle.sh` now signs with a
  stable identity when one is available — a paid Developer ID via
  `SCHL8_SIGN_ID`, or a self-signed "Schl8 Code Signing" cert created
  by the new one-time `./scripts/setup-signing.sh` — falling back to
  ad-hoc with a clear note. Help → Install & Default Editor… and the
  README document the fix

- **Markdown tables now fill the window.** egui's grid sized each column
  to its content's minimum wrapped width, so tables stayed squeezed into
  a narrow ribbon however wide the window was. Columns now share the
  available width evenly (with a minimum so many-column tables in a
  narrow window stay legible)
- **Search matches are highlighted**, not just scrolled to: every match
  gets a tinted background in the plaintext view, the rendered markdown,
  and the editor, and the match you jumped to is drawn in the accent
  color with an underline so it stands out from the rest
- **Find-bar controls no longer render as empty boxes**: the arrow and
  ✕ glyphs aren't in the bundled fonts, so they showed as tofu. They're
  now plain "Prev" / "Next" / "Close" labels (the statusbar's pause glyph
  went the same way)

- **Clicking the Dock icon now brings Schl8's window back.** While
  resident in the menu bar, closing the window only hides it — AppKit
  then had no window to un-minimize and the Dock click did nothing, so
  the menu-bar item was the only way back in. Schl8 now implements
  `applicationShouldHandleReopen:hasVisibleWindows:` (injected into
  winit's app delegate alongside the existing open-document methods) and
  re-shows the window

- The status bar's **"Line x of y" now follows mouse/trackpad
  scrolling**, not just the keyboard navigation keys (derived from the
  live scroll position; exact for plaintext, proportional for rendered
  markdown)

- Save-plan editors (Save Targets and Quick Note Files) now reject a
  destination path used by more than one key: the copies would silently
  overwrite each other, leaving the file readable only by the last key.
  Seeding a plan for a multi-recipient file no longer pre-fills that
  conflicting layout

- **Quick-note text is no longer silently discarded** by the idle
  auto-lock or sleep-lock: typed-but-unsubmitted jot text now defers
  locking the same way unsaved editor changes always have
- Deferred locks are now visible instead of silent: a one-time toast
  explains that unsaved text is being kept, and the statusbar shows a
  "⏸ LOCK PAUSED" chip (with an explanation on hover) whenever unsaved
  edits are pausing auto-lock

## [0.8.1] - 2026-07-18

First public pre-release. Schl8 is experimental software — see the
README's project-status warning and SECURITY.md before trusting it with
anything important.

### Added

- **Downloadable app**: the release workflow now builds a universal
  (Apple Silicon + Intel) `Schl8.app`, zipped and attached to each
  GitHub Release alongside the plain CLI binaries
- **Per-file save targets** (File → Save Targets…): configure which
  key(s) a document is encrypted to on Save and which destination path(s)
  each key's copy is written to — one or many destinations per key, one
  or many keys per file. Every Save encrypts per key and atomically
  overwrites all destinations; individual target failures are reported
  without blocking the rest. Plans persist in the config
  (`[[save_plans]]`) and take precedence over the default
  re-encrypt-in-place; "Remove plan" restores the default behavior
- **View menu** with: a live **Statistics** card (words, characters,
  characters-without-spaces, lines, reading time, file type, signature,
  recipients, encrypted size, and last-saved time) pinned to the corner;
  **Focus Mode** (`Ctrl+Cmd+F`) — fullscreen, chrome hidden, centered
  readable column, Esc to exit; and an opt-in **Allow Copying** toggle
  that shows a security warning with "don't warn again" and "remember as
  default" options before enabling the clipboard
- **Keyboard layout** setting (qwerty/dvorak/colemak/workman) so the
  vim-style navigation keys land under the same fingers on any layout
- `[security]` config section

### Security

- Compile-time assertions (`static_assertions`) now lock in the secure
  buffers' trait properties: `SecureBuffer` and `SecureString` can never
  gain `Clone`/`Debug`/`Display` (no accidental plaintext copies or
  logging), `SecureString` can never become `Send`/`Sync` (its mlock
  bookkeeping is single-threaded by design), and `SecureBuffer` — plus
  the whole decrypt-channel payload (`LoadedDocument`) — must stay
  `Send`. Any future change violating these fails the build

### Fixed

- The Settings window now scrolls when the main window is small
- Toggling out of edit mode with unsaved changes now asks for
  confirmation instead of silently discarding them

### Added (earlier)

- **Settings window** (File → Settings…, `Cmd+,`): rebind every keyboard
  shortcut by click-to-capture — the in-app commands *and* the system-wide
  quick-note hotkey — plus theme/accent/opacity, all applied live and saved
  to config. Conflict detection prevents binding one combo to two actions
- In-app shortcuts are now data-driven from a `[keybindings]` config
  section instead of hardcoded; the global hotkey can be re-registered
  live without a restart
- **Theme engine** with a new `[appearance]` config section: palette
  presets (`slate` default, `midnight`, `plum`, `forest`), an optional
  `accent` override (`#RRGGBB`, with auto-contrast badge text), and window
  `opacity` (0.80–1.0) for a translucent window
- Visual refresh: cyan→violet gradient wordmark and accent hairline
  (matching the app icon), rounded accent-tinted widgets, richer selection
  and hyperlink colors
- **Quick Note is now a floating window** — a separate borderless,
  translucent, always-on-top panel (Spotlight-style); the main window is
  hidden while it's open, so the main GUI is never visible behind the note

### Fixed

- Finder "Open With → Schl8" (and double-click) now opens the file even
  on a cold launch. Previously macOS showed "cannot open files in the 'GPG
  Encrypted File' format" because winit's app delegate implements no
  open-document method and AppKit decides this during launch, before the
  app's code runs. Schl8 now swizzles `-[NSApplication setDelegate:]` and
  injects `application:openURLs:`/`openFile:`/`openFiles:` into the
  delegate's class the moment winit installs it — before `finishLaunching`,
  so the launch open is delivered (`src/macos_open.rs`).

### Security

- **Signature verification**: decrypting captures gpg's signature status
  and shows a badge in the status bar — a green ✔ SIGNED (with the signer's
  UID on hover) for a good, validated signature, or a red ⚠ BAD SIG (with
  the reason) for a bad/expired/unverifiable one. Files with no signature
  show nothing, as before
- **Auto-lock**: after a configurable idle period (default 5 min) the open
  document is closed and its buffers zeroized, showing a locked screen with
  an Unlock (re-decrypt) action; also locks immediately on system/display
  sleep and screen-lock. Unsaved edits defer the lock so work is never
  silently discarded. Configurable via `auto_lock_minutes` / `lock_on_sleep`
- gpg is now resolved to a verified absolute path (allow-list, `SCHL8_GPG`
  override) instead of through `$PATH` — closes a binary-planting vector
  and fixes the bundled app failing to find Homebrew's gpg under Finder's
  minimal PATH
- Quick-note append now assembles the combined document in a `SecureString`
  (mlock'd, zeroized) instead of a plain `Vec`; the editor and jot window
  clear egui's undo history each frame so secret edits aren't retained in
  cleartext
- Encrypted writes go through a hardened `atomic_write`: owner-only (0600)
  temp created with O_EXCL, fsync of file and directory, and a write lock
  serializing concurrent appends
- Folder-archive extraction is bounded against decompression/allocation
  bombs (256 MiB total, 16 MiB/entry, 50k entries)
- Re-encryption resolves the file's stored key IDs to primary-key
  fingerprints via the keyring (avoids 64-bit key-ID collisions and pinned
  subkeys)
- The Finder open-document handler is wrapped in `catch_unwind` so a panic
  can't unwind across the ObjC FFI boundary
- CI runs `cargo-deny` (advisories, licenses, bans, sources), scoped to the
  macOS targets and blocking; bumped anyhow to 1.0.103

### Added

- **Quick Note (jot)**: Cmd+J, a start-screen button, the File menu, the
  menu-bar item, or a customizable global hotkey (default ctrl+cmd+j) opens
  a small window to type a note and append it to a chosen encrypted file —
  decrypt, append, re-encrypt to the file's original recipients, atomic
  overwrite. Enter submits, Shift+Enter inserts a newline, Esc cancels.
  Timestamp templates per file type are configurable in
  `~/.config/schl8/config.toml`
- **Menu-bar residency**: Schl8 keeps a status-bar item (Quick Note /
  Open / Quit) and the global hotkey; closing the window hides the app —
  any open document is closed first so a hidden Schl8 never holds
  plaintext. Disable with `menu_bar_resident = false` in the config
- **Save (Cmd+S)**: re-encrypts to the document's original recipients and
  atomically overwrites the source file; falls back to Encrypt & Save As
  for new files or files opened as plaintext
- **Plaintext opening**: plain `.txt`/`.md` files open directly for viewing
  and editing; saving always goes through encryption (Schl8 never writes
  plaintext), making it easy to convert existing notes
- **macOS app bundle**: `./scripts/bundle.sh --install` builds and installs
  `Schl8.app` with Finder "Open With" associations for `.gpg`/`.asc`/
  `.pgp`/`.txt`/`.md`; files opened from Finder load via AppleEvents.
  Help → "Install & Default Editor…" explains making Schl8 the default
- Config file at `~/.config/schl8/config.toml` (quick-note targets,
  hotkey, templates, residency)

- **New encrypted files**: "New Markdown" / "New Text" buttons on the start
  screen (and File menu, Cmd+N) open an empty document in edit mode; the
  first Encrypt & Save names and creates the encrypted file
- **Encrypted folder browsing**: opening a `folder.tar.gz.gpg` archive (as
  produced by tar-then-encrypt workflows) extracts all text/markdown files —
  at any nesting depth — entirely in memory and shows them in a collapsible
  sidebar file tree; `.tgz`/`.tar` variants and unnamed gzip/tar payloads are
  detected by content magic. Junk entries (`.DS_Store`, `__MACOSX`, `._*`)
  and non-text files are skipped
- Saving now adopts the saved file as the current document (Save As
  semantics): title, path, and content follow the save
- Status bar shows a FOLDER badge with position (e.g. 2/7) while browsing
  archives; keybinding hints hide when space is tight
- Test fixture `test_files/sample-vault.tar.gz.gpg` and a debug-only
  `--sample-archive` flag for developing the browser UI without decryption

- Styled markdown rendering for `.md.gpg`/`.md.asc` files: headings, bold,
  italics, strikethrough, inline code, fenced code blocks, nested and ordered
  lists, task lists, block quotes, tables, and horizontal rules. Links are
  styled but intentionally not clickable (opening a URL would leak document
  content into browser history).
- Drag & drop: drop a `.gpg`/`.asc` file onto the window to open it
  (disabled while editing so a stray drop can't destroy unsaved edits)
- `--sample` flag (debug builds only) opens an embedded sample markdown
  document for UI development without decrypting anything

### Fixed

- `SecureBuffer` now correctly munlocks its memory region on drop (previously
  the unlock was skipped because zeroization clears the buffer first)
- The mlock on the edit buffer now follows the allocation when editing causes
  a reallocation; edit buffers reserve 256 KiB of slack so reallocation is rare
- Edit-mode change detection now uses the editor's change signal instead of a
  length heuristic, so same-length edits are no longer silently discardable
- Quitting or closing the window with unsaved edits now asks for confirmation

### Security

- Copy/Cut events are now stripped and labels made non-selectable, enforcing
  the documented no-clipboard policy in both view and edit mode
- README security model expanded with subprocess-pipe and reallocation caveats,
  and the `--trust-model always` recipient note

### Added

- CI workflow (fmt, clippy, test, build on macOS)
- Unit tests for key-listing parsing, file-type detection, and filename
  suggestion; secure-string roundtrip/relock tests
- ROADMAP.md and CLAUDE.md (architecture + security invariants)

## [0.1.0] - 2026-03-31

### Added

- Native macOS GUI for viewing GPG-encrypted files (.gpg, .asc)
- YubiKey/OpenPGP smart card support via gpg-agent
- Native file picker dialog when launched without arguments
- Markdown and plaintext file type detection from double extensions (.md.gpg, .txt.gpg)
- Secure memory handling: mlock'd buffers, zeroize-on-drop, core dump prevention
- Vim-style keyboard navigation (j/k, d/u, g/G, q)
- Status bar with file info, line position, and keybinding hints
- Dark theme with monospace text rendering
- Background decryption with loading spinner
- Error screen with retry and open-another-file options
- Menu bar with File, Edit, Keys, and Help menus
- In-memory text editing with secure editable buffer (Cmd+E to toggle)
- GPG encryption to selected public key recipients (Cmd+Shift+S)
- Output format selector: .gpg (binary) or .asc (ASCII armor)
- Enforced encrypted output — plaintext is never saved to disk
- GPG public key management: import from file, list, delete with confirmation
- Toast notifications for operation feedback
- About dialog with version info
- Discard edits confirmation dialog
- Cmd+W to close document and return to file picker
- Homebrew tap support (brew tap schbz/tap && brew install schl8)
- GitHub Actions release workflow with macOS arm64 and x86_64 builds
