# Schl8 — Technical Critique

A candid, extensive engineering review of the codebase as it stands
(~5,900 lines of Rust, 517 transitive crates, 42 unit tests, macOS/egui).
Written to be useful rather than flattering: it names real weaknesses,
rates them honestly against the actual threat model, and proposes
concrete improvements and features. Companion to [ROADMAP.md](ROADMAP.md)
and [FEATURE-IDEAS.md](FEATURE-IDEAS.md).

> Severity legend for security items:
> **[crit]** exploitable within the stated threat model ·
> **[high]** exploitable with a plausible extra assumption ·
> **[med]** defense-in-depth gap / narrow window ·
> **[low]** theoretical or requires capabilities that already defeat the app ·
> **[info]** hygiene / documentation.
>
> Honesty up front: Schl8's threat model is a *local, single-user* tool
> whose adversary does **not** have root, a debugger, or code execution as
> the user. Against that model the app is already fairly sound. Most items
> below are hardening, correctness, and "don't let the model quietly
> weaken" concerns — not five-alarm fires. Where something is genuinely
> exploitable, it says so.

> **Update — first hardening pass landed.** The following findings are now
> addressed in the codebase; the analysis text is kept for the record.
> - §2.1 (append leak) & §2.2 (edit-history leak) — appends assemble in a
>   `SecureString`; `ui::secure_edit` clears egui's undoer each frame.
> - §2.3 (gpg via `$PATH`) — all calls go through `gpg::gpg_command()`, an
>   absolute-path allow-list resolver (`SCHL8_GPG` override).
> - §2.4 (re-encrypt by short key ID) — `recipients_for_reencrypt` resolves
>   key IDs to primary-key fingerprints via the keyring.
> - §2.7 (archive bombs) — total/҂per-entry/entry-count limits in
>   `extract_text_entries`.
> - §2.8 (panic across ObjC FFI) — handler wrapped in `catch_unwind`.
> - §2.9 (atomic write) — `atomic_write`: O_EXCL 0600 temp, fsync file+dir,
>   global write lock.
> - §5 (supply chain) — `cargo-deny` job in CI, clean and blocking.
>
> **Update — second pass landed.**
> - §2.6 (authenticity) — decrypt now verifies signatures and shows a
>   status-bar badge (`SignatureStatus`); the *sign-on-save* write side
>   remains open.
> - §2.12 (auto-lock) — idle timeout + lock on sleep/screen-lock, with a
>   locked screen (`macos_power`).
> - §4 (integration tests) — real encrypt/decrypt/append/sign round-trip
>   against an ephemeral keyring now runs in-crate and in CI.
>
> Still open (good next targets): §2.5 `--trust-model always`, sign-on-save
> (the write half of §2.6), §4 *fuzzing* the archive/markdown parsers, §6
> notarization, and the §9 sequoia-openpgp backend.

---

## 1. Overall assessment

**Strengths.** The core security story is coherent and, unusually for a
hobby project, mostly *true*: plaintext flows gpg-stdout → `SecureBuffer`
→ borrowed `&str` per frame, saves always re-encrypt, core dumps are
disabled, and the no-clipboard policy is actually enforced (events
stripped + non-selectable labels), not just claimed. The module layout is
clean, the state machine is legible, background work is correctly off the
UI thread, and the recent additions (archives, quick-note, save-in-place,
tray, Finder integration) were landed without rotting the architecture.
The test suite covers the pure logic (parsers, templating, path decoding,
secure-string relock).

**The central tension.** Schl8's value proposition is *security*, but
the implementation leans heavily on a subprocess (`gpg`) it doesn't
control and a large GUI stack (egui/winit/wgpu, ~517 crates) it can't
audit. The memory-hygiene work on `SecureBuffer` is real but is islanded:
plaintext demonstrably exists in **several unlocked copies** during
decrypt/append/edit that the mlock'd buffer never touches. This is the
theme that recurs below — the guarantees are strongest exactly where the
code can see the bytes, and weakest at the boundaries (the gpg pipe,
egui's `TextEdit` undo buffer, the tar extractor's scratch allocations).

**The maturity gap for open-sourcing.** No `SECURITY.md`, no threat-model
doc, no supply-chain checks in CI, no integration tests that exercise gpg,
no fuzzing of the untrusted-input paths (archive extraction, markdown),
and unsafe FFI that can panic across an ObjC boundary. None are
catastrophic; all are things a security-conscious reviewer will flag the
day it goes public.

---

## 2. Security analysis

### 2.1 Plaintext exists in unlocked memory at multiple boundaries — [med]

The README already concedes the gpg-pipe window, but the reality is
broader than one transient. Trace a decrypt:

1. `Command::output()` reads gpg stdout into a std-allocated `Vec<u8>`
   (`gpg.rs:40`) — unlocked.
2. `SecureBuffer::from_bytes` **clones** that Vec (`secure_buf.rs:19`),
   then zeroizes the original. For the duration of the clone there are
   **two** unlocked plaintext copies, and the new one isn't mlock'd until
   *after* the copy completes (`secure_buf.rs:25`) — a page could be
   paged out in that window.
3. Quick-note append builds the combined document in a **plain
   `Vec<u8>`** (`append.rs:36`), not a `SecureBuffer` — the *entire*
   decrypted note plus the new blurb sits in unlocked, swappable memory
   until the function returns and calls `combined.zeroize()`.
4. Editing routes through egui's `TextEdit`, whose **undo/redo history
   retains `String` snapshots** in ordinary heap memory that Schl8 never
   sees and cannot zeroize (see 2.2).

None of these are exploitable without root or swap access, which the
threat model excludes — hence [med]. But the app's marketing implies
"plaintext lives only in `SecureBuffer`," and that is not literally true.
*Fixes:* have `decrypt_file` write gpg stdout directly into a
pre-`mlock`'d buffer via a manual pipe read (avoids the Command buffer and
the clone); make the append path assemble into a `SecureBuffer`/secure
Vec; and either disable `TextEdit` undo for secure buffers or ship a
custom editor. Also call `mlock` *before* filling the buffer where
possible so bytes are never written to unlocked pages.

### 2.2 egui `TextEdit` undo buffer leaks edit history — [med]

`SecureString` carefully mlocks and zeroizes the *current* text, but
egui's `TextEditState` keeps an internal `Undoer` that clones the string
on edits into egui's memory arena. Those clones are plain `String`s,
never zeroized, and survive until the widget state is evicted. Every
intermediate version of a secret you type is retained in cleartext. This
is the single biggest hole in the "editing is secure" claim.
*Fixes:* construct the `TextEdit` with undo disabled, and/or clear the
widget's `TextEditState` (including the undoer) on exit; longer term, a
purpose-built editor whose buffer *is* the `SecureString`.

### 2.3 `gpg` is resolved through `$PATH`, unpinned — [high]

Every gpg call is `Command::new("gpg")` (`gpg.rs`, `keys.rs`) with the
inherited environment. Two consequences:

- **Binary-planting / PATH order.** Any directory earlier in `$PATH` than
  the real gpg that an attacker (or a careless installer, or a poisoned
  shell profile) can write to yields code execution *as the user, with the
  YubiKey PIN about to be entered*. In a pure local-single-user model this
  is "you already own the account," hence not [crit] — but it becomes
  real the moment Schl8 is used on a shared or managed machine, or if any
  other app the user runs has a writable-directory-on-PATH bug.
- **The bundled-app PATH bug (also a functional defect).** GUI apps
  launched from Finder/LaunchServices inherit a *minimal* PATH
  (`/usr/bin:/bin:/usr/sbin:/sbin`) that does **not** include
  `/opt/homebrew/bin`. `Schl8.app` will therefore frequently fail to
  find Homebrew's gpg at all, surfacing as "gpg not found" or falling back
  to a system gpg with a different keyring. (This is plausibly related to
  the pinentry/agent confusion seen during development.)

*Fix:* resolve gpg once at startup to an absolute path — prefer a
config-declared `gpg_path`, then a small allow-list
(`/opt/homebrew/bin/gpg`, `/usr/local/bin/gpg`, `/usr/bin/gpg`), verify it
exists and is executable, and pass that absolute path to every `Command`.
Optionally pin `GNUPGHOME`. Surface a clear setup error if none found.

### 2.4 Re-encryption trusts the file's stored key IDs — [med]

`list_recipients` parses 64-bit key IDs from the ciphertext packets
(`gpg.rs`) and Save/append re-encrypt to those. Two issues:

- **Short key IDs are collision-prone.** 64-bit key IDs have documented
  real-world collisions; re-encrypting *by key ID* means a colliding key
  planted in the keyring could become the recipient. The packet only
  carries the key ID, so the mitigation is to **resolve each key ID to a
  full fingerprint via the keyring first**, confirm exactly one match, and
  encrypt to the fingerprint.
- **No recipient review on Save.** If a file was (maliciously or
  accidentally) encrypted to an extra key, Save silently preserves that
  recipient. A "this file is encrypted to N keys: …" confirmation on first
  Save would catch it. Ties into the `--trust-model always` note below.

### 2.5 `--trust-model always` on every encrypt — [med]

`encrypt_to_file` passes `--trust-model always` (`keys.rs:131`), so any
importable public key is accepted as a recipient with no trust check. The
README discloses this, which is good, but the app could do better than a
footnote: track a per-key "verified fingerprint" bit (the
[FEATURE-IDEAS](FEATURE-IDEAS.md) fingerprint-verification UX), warn when
encrypting to an unverified key, and drop `always` for keys the user has
confirmed.

### 2.6 No authenticity: decryption doesn't verify signatures — [med]

Schl8 decrypts confidentiality-only. A file delivered/modified by
someone else (e.g. a note synced through a compromised cloud folder) will
decrypt and display with no indication it isn't what you wrote. GPG's
AEAD gives *integrity of the ciphertext under the session key*, not
*authenticity of the author*. For a notes tool this is arguably fine, but
for the "review trade secrets / confidential documents" framing in the
README it's a gap. *Fix:* support sign-then-encrypt on save (optional),
and show a verified/unverified/anonymous badge in the status bar on
decrypt (`--status-fd` gives `GOODSIG`/`VALIDSIG`).

### 2.7 Untrusted archive extraction has no resource limits — [high]

`archive::extract_text_entries` runs a gzip decoder and tar reader over
**attacker-influenced** bytes (anything you're handed and open). Concrete
risks:

- **Decompression bomb.** A tiny `.tar.gz.gpg` can expand to gigabytes;
  `read_to_end` per entry (`archive.rs:~103`) will happily allocate it,
  and `Vec::with_capacity(entry.size())` trusts the tar header's declared
  size — a forged header requests a huge allocation up front. Either is an
  OOM/DoS, and this path runs on a background thread that will take the
  process down with it.
- **Entry count / total-size unbounded.** A tar with millions of tiny
  text entries builds an unbounded `Vec<ArchiveEntry>` of mlock'd buffers
  — and mlock has a per-process limit; you'll exhaust it and start
  silently failing to lock (falling back to swappable plaintext, quietly
  defeating the security property).

Right now the only thing you open is your own archive, so [high] not
[crit] — but "open any encrypted folder someone sent me" is a stated goal,
which makes this a pre-condition for that feature. *Fixes:* cap total
decompressed bytes (e.g. 64 MiB) and entry count, stream with a limited
reader, ignore the header size hint for allocation, and surface "archive
too large / too many files" instead of crashing.

### 2.8 `panic` can unwind across the ObjC FFI boundary — [med, soundness]

`macos_open.rs`'s `handle_open` is a Rust method invoked by Cocoa via
`declare_class!`. If anything in it panics, the unwind crosses back into
ObjC, which is **undefined behavior**. The current body is written
defensively (`if let Ok` on the mutex, no `unwrap`), so a panic is
unlikely — but "unlikely" isn't "sound." The same applies in principle to
any Rust closure the platform invokes. *Fix:* wrap the handler body in
`std::panic::catch_unwind` and swallow/log at the boundary. Consider the
same audit for the tray/hotkey callbacks.

### 2.9 Atomic-overwrite temp file: perms, durability, races — [med]

`encrypt_overwrite` (`keys.rs:170`) writes `.{name}.schl8-tmp` then
renames. The rename is genuinely atomic (good — this is the right shape),
but:

- **Permissions.** gpg creates the temp with the process umask (commonly
  `0644` → world-readable). It's *ciphertext*, so not a plaintext leak,
  but on a multi-user box the ciphertext (and its existence/size/mtime
  metadata) is exposed. Create the temp with `0600` and preserve the
  original file's mode/owner on replace.
- **Durability.** No `fsync` on the temp before `rename`, and no fsync of
  the directory after. On a crash/power-loss the rename can land while the
  data is still in the page cache — for a notes app that can silently
  eat an append. Fsync temp, then rename, then fsync dir.
- **Concurrency.** The temp name is fixed per target. Two rapid quick-note
  appends to the same file (easy with a global hotkey) race on the same
  temp and can corrupt each other; there's no lock and no `O_EXCL`.
  Serialize appends per target, or include a unique suffix and lock.
- **Symlink pre-seed.** A pre-existing `.foo.schl8-tmp` symlink would
  redirect gpg's write. Local-user threat model makes this low, but
  `O_EXCL`/unique names close it.

### 2.10 Config stores note *paths* in cleartext — [low]

`~/.config/schl8/config.toml` lists your quick-note target paths and
`last_target`. Filenames are metadata and can themselves be sensitive
(`~/vault/divorce-notes.md.gpg`). Written with default umask. *Fix:*
`0600` on the config; document that paths are stored; optionally allow an
opt-out of the recent-targets memory. Also note `render_blurb` only strips
`.gpg`/`.asc` (not `.pgp`) when choosing the template (`config.rs:132`) —
a minor inconsistency with the loader, which handles `.pgp`.

### 2.11 Screen capture, IME, and shoulder-surfing — [low, disclosed]

The window renders plaintext to the GPU; screenshots, screen recordings,
and screen-sharing capture it, and macOS input methods can observe typed
text. All are outside the model and disclosed. Worth building anyway (all
in [FEATURE-IDEAS](FEATURE-IDEAS.md)): `NSWindow.sharingType = .none`,
hide-on-blur, and a panic-hide hotkey are cheap, differentiating wins.

### 2.12 No auto-lock / idle zeroize — [low]

A decrypted document stays in memory indefinitely while the window is
open. Tray-hide does close the document (good), but an unattended open
window keeps secrets resident. Idle timeout + lock-on-sleep are already
roadmapped; worth doing.

### 2.13 Smaller notes

- **`mlock` failure is non-fatal and quiet** (`secure_buf.rs:31`) — it
  `eprintln`s and proceeds with swappable memory. For a security tool,
  consider making this a visible, in-UI warning (a "not fully protected"
  banner) rather than a stderr line no GUI user sees.
- **8 `eprintln!` sites** leak filenames/paths to stderr. Fine per the
  invariant (never content), but on a bundled app stderr may land in
  `Console.app`/unified logging where paths persist. Audit and downgrade.
- **`.DS_Store` is committed** to the repo — hygiene.

---

## 3. Architecture & code quality

**What's good.** The `State` enum + `Transition` pattern keeps the UI loop
readable; `document::` cleanly separates loading/append/archive/markdown;
crypto is isolated behind small functions; the immediate-mode borrow
discipline is consistently applied. Background threads use mpsc correctly
and never touch egui.

**Growing pains.**

- **`app.rs` is becoming a god-file** (well over 1,000 lines) and
  `update()` is a very long method mixing input handling, decrypt polling,
  tray polling, drag-drop, dialog rendering, and the encrypt/save state
  machine. It works, but it's getting hard to reason about ordering (e.g.
  which handler consumes an Enter/Escape first). Extract sub-controllers:
  an `InputRouter`, a `SaveController`, a `JotController`. This also makes
  the event-precedence bugs (below) testable.
- **Keyboard-shortcut precedence is implicit and fragile.** Cmd+S vs
  Cmd+Shift+S is distinguished by `!shift` checks scattered across a big
  `ctx.input` block; the jot window consumes Enter/Escape in its own
  render; menu actions merge with kb actions via `.or()`. There's no
  single source of truth for "who owns this key this frame," which is
  exactly the kind of thing that regresses silently. A small keymap
  abstraction with explicit priority would help.
- **Duplicated navigation logic.** The vim-scroll handling is copy-pasted
  between `render_viewing` and `render_viewing_archive`. Factor it.
- **33 `unwrap`/`expect`/`panic` in non-test code.** Most are on
  known-good invariants (embedded PNG decode, etc.), but each is a
  potential abort. Audit for any reachable from untrusted input.
- **Magic numbers / layout constants** live inline in places despite the
  `theme.rs` convention; the earlier statusbar width fix was a symptom.
- **No `#![forbid(unsafe_code)]` boundary.** 15 unsafe blocks across
  memory, secure_buf, and the ObjC handler. Consider `#![forbid(unsafe)]`
  crate-wide with explicit `#[allow]` only on the three modules that need
  it, so new unsafe can't sneak in.

---

## 4. Testing & verification gaps

Current: 42 unit tests over pure logic — genuinely good coverage of
parsers, templating, path decoding, secure-string relock, archive
extraction shapes.

Missing, roughly in priority order:

1. **Integration tests that actually run gpg.** Generate an ephemeral
   throwaway keypair in a temp `GNUPGHOME` in the test harness, then
   round-trip encrypt → decrypt → append → verify. This is the only way to
   catch regressions in the real crypto path (recipient parsing, atomic
   overwrite, armor handling) — none of which the unit tests touch.
2. **Fuzzing the untrusted parsers.** `cargo-fuzz` targets for the tar/gzip
   extractor and the markdown block parser. These consume attacker-shaped
   bytes and are exactly where a panic/OOM/UB would hide (see 2.7).
3. **Property tests** for `SecureString` reallocation (arbitrary edit
   sequences keep the mlock tracking the live allocation) and for
   `render_blurb`/template rendering.
4. **A "no plaintext on disk" regression test.** Under a temp HOME, run a
   full open/edit/save/append cycle and assert (via strace-equivalent, or
   by scanning for known plaintext in every file the process created) that
   no cleartext ever hit the filesystem. This directly guards the flagship
   invariant.
5. **Golden-image UI tests.** egui supports headless render + snapshot;
   the markdown renderer and archive tree are good candidates.

CI itself is solid (fmt/clippy `-D warnings`/test/build on macOS) but only
builds one target and has **no supply-chain checks** (see §5).

---

## 5. Dependencies & supply chain

517 transitive crates for a security tool is a large trusted base, driven
mostly by egui/winit/wgpu. That's a reasonable price for a native GUI, but
it deserves active management:

- **Add `cargo-audit` and `cargo-deny` to CI** — advisory scanning, license
  policy, duplicate/yanked detection, and a `[bans]` list. Today a `RUSTSEC`
  advisory in any of 517 crates ships silently.
- **`cargo-vet` or at least pinned, reviewed updates.** `Cargo.lock` is
  committed (good).
- **Minimize the GPU stack if feasible.** wgpu pulls a huge amount; egui
  can run on a glow/GL backend with far fewer deps. Worth measuring binary
  size (currently ~5.6 MB) and dep count against a glow build.
- **Reproducible builds + SBOM** before distributing binaries, so users
  can verify what they run.
- **Vendored/audited unsafe FFI.** The objc2 handler is hand-written
  `msg_send!`; a brief written rationale + the catch_unwind fix (2.8)
  would make it reviewable.

---

## 6. Distribution & operations

- **Ad-hoc signature only.** `bundle.sh` signs with `-` (ad-hoc), so
  Gatekeeper will block downloads and the global-hotkey/tray may hit TCC
  (Accessibility/Input Monitoring) prompts that behave differently for
  unsigned apps. Real distribution needs a Developer ID identity +
  **notarization + stapling**. Document the `codesign`/`notarytool` flow.
- **The PATH bug (2.3) makes the bundled app unreliable** until gpg is
  resolved to an absolute path — arguably the highest-impact single fix for
  "it just works after install."
- **No `LSUIElement`/agent story.** For a menu-bar-resident app that hides
  its window, decide whether it should show in the Dock at all; consider an
  `LSUIElement` mode or an explicit preference.
- **Global hotkey needs Input Monitoring / Accessibility permission** on
  modern macOS; there's no onboarding for granting it, so the hotkey will
  appear to "not work" with no explanation. Add a first-run check +
  guidance.
- **Distribution asks a lot of the user.** There is no Homebrew tap, and the
  published builds are not notarized, so installing means clearing a
  quarantine flag by hand — a step that trains people to disarm exactly the
  protection that would catch a malicious build. Signing and notarization
  would fix it properly; a cask without them would only move the problem.
- **No crash/telemetry policy** (appropriately — but say so explicitly for
  a privacy tool, and ensure no panic messages or paths escape to Apple's
  diagnostics).

---

## 7. Performance & scalability

- **Markdown re-parses every frame.** `ui::markdown::render` calls
  `parse_blocks` on each repaint. Fine for notes; for a large document at
  60fps it's wasteful and, more importantly, keeps re-deriving structures
  over plaintext. Cache the parsed blocks and invalidate on edit.
- **Whole-document `String`/`Vec` copies** on entering edit mode and on
  save (`buf.as_bytes().to_vec()`), each a fresh unlocked allocation.
- **Archive extraction is eager and fully in-memory.** Every text file is
  decrypted and mlock'd up front; a large vault could exhaust the mlock
  limit (see 2.7). Lazy per-file materialization would scale better.
- **No virtualization in the file tree or long documents** — everything is
  laid out each frame. egui handles a lot, but a 10k-file vault or a
  megabyte note will show it.

---

## 8. UX & accessibility

- **Accessibility vs. secrecy tension.** `selectable_labels = false` and
  the no-copy policy also degrade screen-reader/AccessKit behavior. There's
  a real, interesting design question here: how do you make a
  *deliberately* copy-hostile, capture-hostile app usable with assistive
  tech? Worth an explicit stance.
- **No in-app settings UI.** Templates, hotkey, and residency are
  config-file-only; a Preferences window would make them discoverable.
- **Error surfacing is uneven.** Some failures toast, some `eprintln`
  to a stderr no GUI user reads (mlock failure, config save failure). A
  consistent, non-alarming status channel would help — especially for the
  "you're not actually protected right now" cases.
- **First-run onboarding is absent.** gpg/pinentry setup, granting Input
  Monitoring, choosing a default editor — all currently tribal knowledge in
  the README. A guided first-run would dramatically cut the
  "bad PIN / nothing happens" confusion.
- **Quick-note discoverability.** Great that it has four entry points;
  consider a subtle menu-bar affordance showing the hotkey.

---

## 9. Feature ideas (creative, beyond the roadmap)

Grouped by the axis they serve. Items already in
[FEATURE-IDEAS.md](FEATURE-IDEAS.md) are not repeated except where this
review sharpens them.

**Security-forward**

- **Touch ID to gate decrypt/append.** Use `LocalAuthentication` so a jot
  append requires a fingerprint even while the gpg-agent PIN is cached —
  re-adds a presence check the agent cache removes.
- **Signed notes with authorship badges.** Sign-then-encrypt on save;
  verified/unverified/anonymous badge on open (pairs with 2.6).
- **Encrypted, tamper-evident history.** Each save also appends an
  encrypted, hash-chained journal entry, so you can see (and verify) a
  note's edit history without ever storing plaintext diffs.
- **"Panic paste" decoy.** A hotkey that instantly hides and shows a
  configurable decoy document.
- **Per-vault key policy.** A `.schl8.toml` in a folder that pins
  recipients/verified fingerprints for everything created there, so new
  notes can't accidentally be encrypted to the wrong key.

**Genuinely more useful**

- **Vault-wide encrypted search.** Decrypt-on-demand across a directory
  (with the YubiKey touch policy permitting), results as filenames + match
  counts, never cached — the killer feature for a notes tool.
- **Conflict-aware append.** Detect that a target changed since last read
  (mtime/hash) and merge rather than clobber — important once notes sync.
- **Templates & snippets for quick-note.** Named templates ("standup",
  "idea", "log") selectable in the jot window; you already have the
  rendering machinery.
- **Frontmatter + tags.** Parse YAML frontmatter, render a metadata card,
  and let tags drive filtering across a vault.
- **Multiple identities / keyring profiles.** Switch GNUPGHOME contexts
  (work/personal) from the menu.
- **`age` format support.** `age`/`rage` has clean Rust crates and YubiKey
  plugins; supporting `.age` alongside `.gpg` widens the audience and lets
  you drop the subprocess for that path (pure-Rust, no unlocked pipe).

**Visual / delightful**

- **Real font weights + syntax highlighting** (roadmapped) — the biggest
  visual upgrades; the current "brighten for bold" is a workaround.
- **Command palette** (Cmd+K) over files, actions, and vault search.
- **Reading-focused typography mode** — measure cap, generous margins, a
  serif option for long-form reading.

**Architecturally interesting**

- **Pure-Rust crypto backend option (`sequoia-openpgp`).** Removes the
  subprocess entirely for encrypt/decrypt — plaintext never leaves the
  process, closing 2.1 and 2.3 at the root. YubiKey via `openpgp-card`.
  Big dependency decision, but the single most security-improving change
  available.
- **A tiny broker process** that owns all gpg interaction and holds
  plaintext in a separately-hardened address space, with the GUI as a
  thin, capture-resistant renderer talking over a local socket.

---

## 10. Prioritized recommendations

If the goal is "trustworthy enough to open-source and rely on daily,"
roughly in order of value-per-effort:

1. **Resolve `gpg` to an absolute, verified path** (2.3). Fixes a real
   security exposure *and* the bundled-app reliability bug. Small.
2. **Bound archive extraction** (2.7) before shipping "open folders you
   were sent." Small-to-medium.
3. **Kill the edit-history and append plaintext leaks** (2.1, 2.2):
   disable/clear `TextEdit` undo for secure buffers; assemble appends in a
   secure buffer. Medium.
4. **`catch_unwind` the ObjC handler** (2.8). Tiny, removes a UB.
5. **Harden the atomic write**: `0600` temp, fsync, per-target
   serialization (2.9). Small.
6. **Add `cargo-audit` + `cargo-deny` to CI** (§5). Tiny, high ongoing
   value.
7. **Integration tests with an ephemeral gpg keyring** (§4). Medium,
   unlocks confident refactoring.
8. **Resolve recipient key IDs to fingerprints** on re-encrypt (2.4).
   Small-to-medium.
9. **`SECURITY.md` + a written threat model** that states plainly what is
   and isn't protected (and matches the code). Documentation, but
   table-stakes for a security project.
10. **Notarization + signed cask** for real distribution (§6). Medium,
    mostly process.

The through-line: Schl8's *design* is more secure than its *seams*. The
highest-leverage work is closing the boundaries where plaintext currently
escapes the model the app otherwise upholds well — and, eventually, asking
whether the subprocess boundary should exist at all.
