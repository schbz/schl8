# Agent integration — design

Status: **Phases 1, 2 and 4 implemented** (headless CLI; the onboarding
document, now generated live by `schl8 agent brief`). Phase 3 (MCP)
not yet green-lit.
See `docs/SPOOL-DESIGN.md` (offline append) and `docs/AGE-DESIGN.md`
(seed-phrase identity) — both are load-bearing here.

---

## 1. What "agent-friendly" means for a secrets app

Agentic platforms (Claude Code, Codex, and anything MCP-speaking) are
good at producing text: reports, summaries, research, logs. Schl8's job
is to hold text encrypted. The natural integration is:

> **Agents write into your encrypted store. They never read from it.**

This is not a limitation to apologize for — it is the security model.
Public-key encryption is asymmetric in exactly the right direction:
encrypting to your `age1…` recipient (or GPG fingerprint) requires **no
secret whatsoever**. The spool (SPOOL-DESIGN) already proves the whole
pipeline works key-free: encrypt to a recipient, drop a standalone file,
merge under human control later.

So a compromised or confused agent can, at worst, *add* ciphertext.
It can never exfiltrate a note, never learns the seed phrase, never even
holds a decrypted byte. That posture — **write-only crypto for
automation** — is the pitch, and every phase below preserves it.

### Invariants for the whole agent surface

1. **Headless = write-only.** No command or tool ever accepts a seed
   phrase, decrypts a file, or prints plaintext it did not just receive.
2. **Recipients come from config only.** Agents encrypt to keys the human
   registered in the GUI. Adding a recipient stays a human action.
3. **stdin plaintext is transient** — encrypted immediately, never
   logged, never written unencrypted.
4. **Provenance is marked.** Entries that arrive through the agent
   surface are labeled as such when merged (extending the spool's
   "written while locked" marking), so a human can always tell
   agent-written content from their own.
5. **No interactive prompts in headless paths.** Fail with a clear error
   and a stable exit code instead — agents parse, they don't answer.

## 2. Phase 1 — headless CLI (implemented)

The `schl8` binary already parses flags (clap). Add subcommands that
run headless and exit — no window, no prompts:

```
schl8 encrypt  --to <age1…|GPG-fpr> [--to …] [--out FILE] [< stdin]
schl8 append   --note <registry-name|path> [< stdin]
schl8 notes    list  --json
schl8 recipients list --json
schl8 pending  [--json]
```

- **`encrypt`** — stdin → ciphertext at `--out` (or stdout). The agent
  workhorse: `some-agent-report | schl8 encrypt --to age1… --out
  report.md.age`.
- **`append`** — stdin becomes one spool segment for the named quicknote,
  exactly as if jotted while locked. *Always* via the spool, even if the
  GUI happens to be unlocked: the CLI is a separate process and must
  never need key material. The GUI's badge picks it up within a scan
  tick; the human merges on next unlock.
- **`notes list` / `recipients list` / `pending`** — read-only *public*
  metadata (names, paths, backend, `age1…` strings, pending counts) so an
  agent can discover where it may write. `--json` for machines, table
  for humans.
- Deliberately absent: `decrypt`, `unlock`, anything touching the seed
  phrase or keyring secrets.

Concurrency with a running GUI is already safe: spool writes are atomic
with random names, and `encrypt --out` uses the same atomic-write path.

## 3. Phase 2 — onboarding (implemented)

Phase 2 was designed as a static file the user pastes a path to. It
shipped that way first (`assets/agent-guide.md`, written to
`~/.config/schl8/AGENT-GUIDE.md`) and the first real test against a
live agent showed two problems that the format itself caused:

1. **A static file cannot describe the machine it is on.** It named
   `~/Notes/`, a directory the user did not have, so the agent had to
   invent a location — a different one each session. It could not name
   the actual keys, so orientation cost extra round-trips.
2. **Pasted text is a snapshot.** It goes stale against the config the
   moment either changes, and nothing tells anyone it has.

So the onboarding surface is now a *command*:

```
schl8 agent brief          # the whole briefing, generated from config
schl8 agent init [DIR]     # write AGENTS.md so coding agents auto-read it
```

- **`agent brief`** substitutes the real notes folder, the real
  recipients, the real quicknotes and their pending counts into
  `assets/agent-brief.md`. What the human pastes shrinks to one line:
  "run `schl8 agent brief` and follow it". Re-running it is how an
  agent refreshes rather than trusting stale context.
- **`agent init`** writes an `AGENTS.md` that *points at* `agent brief`
  rather than copying it, so the file cannot rot either. In a project
  directory this removes the paste entirely — the agent reads it on its
  own. `--claude` also writes `CLAUDE.md`; `--force` to overwrite.

**Recipient labels are omitted from `brief` on purpose.** GPG uids and
age nicknames carry real names and email addresses, and this output is
expected to be read by a third-party service. Bare fingerprints and
`age1…` strings are everything needed to encrypt and nothing needed to
identify anyone. `schl8 recipients list` still shows labels for the
human.

**`Help → Install Command Line Tool…`** is part of this phase, not a
convenience. Inside an app bundle the binary is at
`/Applications/Schl8.app/Contents/MacOS/schl8`, which no shell
finds; without the symlink the first command an agent runs fails and
every path above dead-ends. See `src/cli_install.rs` for why it prefers
an already-writable directory on PATH over prompting for an
administrator.

The static guide survives as the paste-only fallback for assistants that
cannot run a command, and gets the same notes-folder substitution.

## 4. Phase 4 — persistence, without a platform table

Phases 1–2 make Schl8 usable *in a conversation*. Phase 4 makes it
usable in *every* conversation: `schl8 agent toolkit` prints a
specification an assistant turns into a standing skill, command, rules
entry or memory — whatever its own platform provides.

The tempting design was a table: Claude Code skills live here, Codex
instructions there, Cursor rules somewhere else. That table would be
wrong within months and silently wrong the whole time. So the spec
describes *capabilities* — save encrypted, append, log machine output,
discover, build a vault, verify backups, refresh — with exact commands
and the invariants that must survive into whatever gets generated, and
says outright that it does not know what the reader is. `--json` gives
the same content as a manifest for agents that would rather generate
than parse prose. A unit test asserts the prose never names a
platform-specific path.

Per-quicknote entries are emitted alongside a generic one, never instead
of it: the shortcuts are the nice shape and also the one that rots when
a note is renamed, so there is always a path that works for a note
created after the toolkit was built. Notes with no key are listed but
get no shortcut — an entry that always fails is worse than none.

`schl8 agent skills install|uninstall` is the single exception, writing
Claude Code's layout directly because it can be verified here. Generated
files carry an ownership marker and uninstall removes only marked ones,
because putting files in `~/.claude/` means injecting instructions into
every future session on the machine and that has to be cleanly
reversible — without ever deleting a file the user wrote under the same
name.

## 5. Phase 3 — MCP server

MCP is the native tool protocol for Claude (and increasingly others).
`schl8 mcp` runs a stdio JSON-RPC server exposing the same verbs as
the CLI, same invariants:

- `append_note {note, text}` → spool segment
- `encrypt_text {recipients, text, out_path}` → encrypted file
- `list_notes {}` / `list_recipients {}` / `pending_status {}`

Registration is one command (`claude mcp add schl8 -- schl8 mcp`),
after which "save this to my encrypted journal" works as a first-class
tool call instead of shelling out. stdio only — no network listener,
nothing to firewall. The protocol surface is small enough to hand-roll
over serde_json; no new heavyweight dependency.

## 6. Explicitly rejected / deferred

- **Headless decrypt** (even "gated"): a request/approve flow where the
  agent asks and the human approves in the GUI is *coherent*, but it
  breaks the one-sentence security story and invites approval fatigue.
  Deferred until a concrete need exists; likely never.
- **Watch-folder ingestion** (agent drops plaintext in a directory,
  Schl8 encrypts and deletes): plaintext would touch disk — violates
  the core invariant. Rejected outright; stdin piping covers the need.
- **Agent-held identities** (giving an agent its own seed phrase so it
  can read a dedicated notebook): nothing stops a user doing this
  manually today (generate a second key, register its recipient), and
  documenting it is fine — but Schl8 won't store or manage agent
  secrets itself.

## 7. Signing agent entries (ties into SPOOL-DESIGN §7)

The spool's v2 signing plan gains a second motivation here: with
multiple writers (you, agent A, agent B), a `sig:` header per segment
would distinguish them cryptographically instead of by honor system.
Until then, provenance marking is textual ("via CLI/MCP") and advisory.
