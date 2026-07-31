# Schl8 Feature Ideas

An expansive brainstorm of possible directions — deliberately broader and more
speculative than [ROADMAP.md](ROADMAP.md), which tracks what's actually
scheduled. Items graduate from here to the roadmap when they're wanted and
scoped. Every idea must respect the security invariants in `CLAUDE.md`
(plaintext never touches disk, secure buffers only, no clipboard by default).

Legend: 🟢 small effort · 🟡 medium · 🔴 large / research project

---

## 🔒 More secure

### Hardening the window of exposure

- 🟢 **Auto-lock on idle** — after N minutes without input, drop all secure
  buffers and show a locked screen; reopening re-prompts the YubiKey.
- 🟢 **Lock on screen sleep / lid close / fast user switch** — subscribe to
  `NSWorkspace` notifications and zeroize immediately.
- 🟢 **Lock on YubiKey removal** — poll `gpg --card-status` or use a USB
  watcher; the key leaving the machine means the session should end.
- 🟡 **Panic key** — a single keystroke (e.g. double-Esc) that instantly
  zeroizes buffers, closes the document, and clears the window title.
- 🟡 **Screen-capture resistance** — set `NSWindow.sharingType = .none` so the
  window renders black in screenshots, screen recordings, and screen shares.
  (Genuinely valuable for reviewing sensitive docs on calls.)
- 🟡 **Hide-on-blur mode** — optionally blur or blank the document whenever
  the window loses focus (shoulder-surfing defense).
- 🟢 **Redacted boot** — start with content visually redacted (block glyphs);
  hold a key to reveal, paragraph by paragraph under the cursor.
- 🔴 **Direct pipe decryption** — replace `Command::output()` with a
  hand-rolled pipe reader that reads gpg stdout straight into an mlock'd,
  pre-reserved buffer, closing the "plaintext transits an unlocked Vec"
  window documented in the threat model.
- 🔴 **GPGME or sequoia-openpgp backend option** — sequoia (pure Rust) would
  remove the subprocess entirely; plaintext never leaves the process. Big
  dependency decision; YubiKey support via sequoia's `openpgp-card` crate.

### Trust & integrity

- 🟢 **Signature verification** — if a file is signed as well as encrypted,
  show a verified/unverified badge in the status bar with the signer's UID.
- 🟡 **Recipient inspection** — before decrypting, show which key IDs the
  file is encrypted to (`gpg --list-packets` on ciphertext is safe — it never
  needs the plaintext); warn if your key isn't among them instead of failing.
- 🟡 **Fingerprint verification UX** — when importing a key, display the full
  fingerprint in chunked, easily-comparable groups with a "verified out of
  band" checkbox stored per key; drop `--trust-model always` for keys that
  haven't been marked verified.
- 🟡 **Encrypt-to-self guard** — warn before saving a file you won't be able
  to decrypt (none of the selected recipients is your own key).
- 🔴 **Tamper-evident audit trail** — optional signed, encrypted log of
  open/save events for compliance-minded users (must itself never leak names
  or content — filenames hashed).

### Process hygiene

- 🟢 **Ptrace/debugger denial** — `PT_DENY_ATTACH` on macOS at startup.
- 🟢 **Environment scrubbing** — clear suspicious env vars before spawning
  gpg; pin the gpg binary path to a config-declared absolute path instead of
  trusting `$PATH`.
- 🟡 **Pinentry health check** — detect a missing/misconfigured pinentry-mac
  up front and show setup instructions instead of a cryptic gpg error.
- 🟡 **Opt-in clipboard with countdown** — if a user really wants copy, allow
  it per-selection via an explicit menu action that auto-clears the clipboard
  after 30 seconds and shows a countdown toast. Default stays off.

## 🛠 More useful

### Files & workflow

- 🟡 **Save back to source (Cmd+S)** — re-encrypt to the original recipients
  and overwrite the source file; the single biggest editing-workflow gap.
- 🟡 **New encrypted file** — create a document from scratch inside the app;
  pick recipients on first save.
- 🔴 **Directory browser** — open a folder; sidebar tree of `.gpg`/`.asc`
  files with type badges; navigate with j/k; remembers last folder. The
  "secure notes vault" experience.
- 🟡 **Recent files** — a File > Recent menu. Privacy note: store only
  salted hashes → display basenames, or make it strictly opt-in since even
  filenames can be sensitive.
- 🟢 **Finder integration** — proper `Schl8.app` bundle with file
  associations so double-clicking a `.gpg` file opens Schl8.
- 🟡 **Quick Look-style speed** — open-file → pinentry → rendered in under a
  second; measure and optimize the cold path.
- 🟡 **Multi-document tabs** — a few documents open at once, each in its own
  secure buffer, with per-tab lock state.
- 🟡 **Re-encrypt tool** — batch re-encrypt selected files to a new/rotated
  key (reads each, re-encrypts, writes; never plaintext on disk).

### Reading & editing

- 🟡 **In-document search** — `/` opens a find bar (view mode), n/N to jump
  between highlighted matches; search state lives in secure memory.
- 🟢 **Go-to-line** (`:42` vim style) and **percentage indicator**.
- 🟡 **Outline/TOC panel** — headings extracted from the parsed markdown; jump
  by clicking or with `[[`/`]]`.
- 🟢 **Word count / reading time** in the status bar (computed per frame from
  the borrowed &str — no copies).
- 🟡 **Basic editor niceties** — line numbers in edit mode, current-line
  highlight, markdown-aware continuation (auto-insert `- ` on Enter inside a
  list).
- 🔴 **Split view** — raw markdown on the left, rendered preview on the right,
  live while editing.
- 🟢 **Undo depth guarantee** — verify egui TextEdit undo works well with the
  secure buffer and doesn't retain plaintext in its undo stack (audit —
  possibly a security item too).

## 🎨 More visually appealing

- 🟡 **Real font weights** — load system fonts (SF Pro / SF Mono or bundled
  open-licensed fonts like Inter + JetBrains Mono) so bold is actually bold
  and headings gain hierarchy. Single highest-impact visual upgrade.
- 🟡 **Syntax highlighting in code blocks** — `syntect` with a lazy theme;
  highlights borrowed line-by-line per frame.
- 🟢 **Readable measure** — cap content width (~68 characters), centered,
  with comfortable margins; long-line documents stop feeling like a terminal.
- 🟢 **Typographic polish** — proper paragraph spacing, first-line heading
  spacing, hanging list indents, smart link styling on hover.
- 🟡 **Themes** — light theme, high-contrast theme, and a couple of tasteful
  dark variants (the current one, warm "paper", cool "slate"); theme picker
  in a settings dialog, persisted to config.
- 🟢 **Smooth scrolling** — animate j/k and page jumps with a short ease-out
  instead of hard offset jumps.
- 🟢 **Reading progress bar** — a 2px accent line under the menu bar showing
  position in document.
- 🟡 **Polished chrome** — unified title bar (content under a transparent
  titlebar like modern macOS apps), traffic-light-aware layout, subtle window
  shadow on dialogs, consistent 4/8pt spacing grid.
- 🟡 **Zoom** — Cmd+plus/minus/0 scaling the whole typographic scale, not
  just one font size.
- 🟢 **Image placeholders with dignity** — markdown images render as a small
  framed card with the alt text and a note that images are not loaded
  (loading them would be a network/disk leak).
- 🟡 **App icon set refresh** — current icon is good; consider a matching
  document icon for `.gpg`/`.asc` associations and a menu-bar template icon
  if a status item is ever added.

## 💪 More powerful

- 🔴 **Multi-format decryption targets** — beyond text/markdown: view
  encrypted CSVs as tables, JSON with folding, maybe encrypted images
  (rendered from memory only). Each format needs its own "no temp files"
  audit.
- 🟡 **Frontmatter awareness** — parse YAML frontmatter in notes and render
  it as a neat metadata card (title, tags, dates).
- 🔴 **Vault search** — search across all files in a directory *by
  decrypting each in sequence into a reusable secure buffer* (explicit user
  action, YubiKey touch policy permitting; results show filenames + match
  counts, never cached).
- 🟡 **Templates** — new-file templates (meeting note, journal entry,
  credential record) stored as plaintext templates in config (they're not
  secret) and instantiated into secure buffers.
- 🟡 **CLI mode** — `schl8 cat file.gpg | less`-style subcommands are an
  anti-goal (plaintext to stdout), but `schl8 encrypt`, `schl8
  recipients`, `schl8 verify` are safe and useful.
- 🔴 **Agent/automation-safe API** — an XPC or socket interface that lets
  scripts *request* the GUI to open files (never extract content), so
  Schl8 can be the trusted display endpoint of other tools.
- 🟡 **Multiple keyring profiles** — switch between GNUPGHOME contexts
  (work / personal) from a menu; each with its own key list.
- 🔴 **Age format support** — `age`/`rage` encryption is increasingly popular
  and has clean Rust crates; supporting both `.gpg` and `.age` would widen
  the audience considerably (age has YubiKey plugins too).
- 🟡 **Printing… deliberately absent** — document the decision: printing
  spools plaintext to disk. If ever added, it must render to the printer
  context directly and be loudly opt-in.

## 🌍 Open-source & community power-ups

- 🟢 **Screenshots/demo GIF in README** using the `--sample` mode (no real
  secrets on screen).
- 🟢 **SECURITY.md** with the threat model and a disclosure contact.
- 🟢 **CONTRIBUTING.md** stating the security invariants as PR requirements.
- 🟡 **Reproducible release builds** + checksums and (eventually) notarized,
  stapled `.app` downloads.
- 🟡 **Security review issue template** — invite cryptographers to poke holes
  in specific areas (secure_buf, gpg invocation) with pointers to the code.
- 🔴 **External audit** — once the surface stabilizes, a community or paid
  audit of the memory-handling claims; publish the findings.

---

### Suggested next picks (opinionated)

1. **Save back to source** — completes the core edit loop (usefulness).
2. **Auto-lock (idle + sleep + YubiKey removal)** — biggest security win per
   line of code.
3. **Real font weights + readable measure** — biggest visual win.
4. **Directory browser** — unlocks the "vault" identity of the app.
5. **Screen-capture resistance** — rare, differentiating, genuinely useful.
