# Offline append (spool) — design

Status: **spec frozen for v1**, implemented.
See also `docs/AGE-DESIGN.md` for the AGE identity itself.

---

## 1. The problem

Appending to an encrypted note is `decrypt → concatenate → re-encrypt`.
The **decrypt** step needs the private key, so a quick note can't be saved
while the AGE identity is locked. With `age_lock.forget_idle_minutes`
defaulting to 15, "locked when the hotkey fires" is the common case, not
a rare one.

Public-key crypto is asymmetric in exactly the useful direction:
**encrypting to a recipient never needs their private key.** So the fix is
to stop reading the note in order to write to it.

## 2. Why a spool, and not the alternatives

Appending a second AGE message onto an existing `.age` file was the
tidiest idea, so it was measured first:

- decrypting the concatenated blob with a stock reader **fails** — age
  does not tolerate trailing bytes after a message;
- the `age-encryption.org/v1` header magic *is* a clean boundary
  (offsets `[0, 275]` for a 275-byte first message);
- split there and each half decrypts fine.

So concatenation works only if Schl8 owns the reader, and the result
stops being a valid `.age` file to `age`/`rage`. Rejected: interop with
standard tooling is worth more than saving a directory.

The spool keeps **every file a standard, standalone age file** — the
canonical note *and* each pending segment. No custom framing, no
heuristic scanning, no interop loss. The unusual state (a note plus
pending segments) is temporary and self-healing.

## 3. Layout

For a note at `/notes/journal.md.age`:

```
/notes/journal.md.age              ← canonical note, ordinary age file
/notes/.journal.md.age.spool/      ← hidden sibling, created on demand
    3f9a…c1.age                    ← one pending entry, ordinary age file
    a17b…04.age
```

- The spool directory is **dot-prefixed** so it stays out of the way in
  Finder; its presence is surfaced in the menu bar instead.
- It sits beside the note so it travels with the note (copy, sync, back
  up) rather than living in a central cache keyed by path.
- Segment filenames are **random hex**, carrying no ordering or timing
  information — see §5.

## 4. Segment format (v1 — frozen)

Each segment file is a normal age (or GPG) encryption, to the same
recipient(s) the note's save rule already names.

The backend follows the note's own save plan: age when the plan has an
age recipient, GPG otherwise. Writing needs only a public key either way,
so a GPG note spools without a PIN prompt or a hardware-key touch — the
same benefit age notes get from a locked identity. The segment's
**extension** (`.age` / `.gpg`) records which backend wrote it, so a
merge knows how to open it with no manifest and no other state; a plan
carrying both prefers age, since merging an age note needs the identity
anyway and an age segment therefore costs no extra unlock.

The **plaintext inside** is:

```
schl8-spool/v1
written: 2026-07-22T09:15:03.123Z

<body bytes, verbatim>
```

- Line 1 is the exact magic `schl8-spool/v1`.
- Then `key: value` headers until the first blank line.
- Everything after that blank line is the body, byte-for-byte — it is
  never re-wrapped or re-encoded.
- Unknown headers are **ignored** by v1 readers, so fields can be added
  without breaking old segments.

`written` is an RFC 3339 UTC timestamp. It is the ordering key.

## 5. Why the timestamp is inside the ciphertext

Ordering has to come from somewhere, and there are only two places:
the filename or the plaintext.

Putting it in the filename would let Schl8 sort without decrypting —
but a directory listing would then leak *when you wrote each note*, which
is exactly the activity metadata an encrypted notes app should not
publish. It would also let anyone with write access reorder your entries
by renaming files.

Putting it inside the ciphertext costs nothing real: merging only happens
when the identity is unlocked, so the segments are being decrypted
anyway. Filenames are therefore random, and a listing reveals only *how
many* entries are pending, never when.

Clock skew across machines can misorder entries. Accepted: these are
notes, not a ledger.

## 6. Flows

**Write (identity locked).** The recipients come from the quicknote's
save rule (`age_recipient` / `key_fingerprint`) — available in config
with no private key. Render the blurb, wrap it in the §4 envelope,
encrypt to those recipients, write one new segment into the spool. No
decryption anywhere in this path.

**Write (identity unlocked).** Unchanged — decrypt, append, re-encrypt.
The spool is not used.

**Merge (on unlock / on open / on demand).** Decrypt the note; decrypt
each segment; sort by `(written, filename)`; append the bodies in order;
re-encrypt the note through the existing save rules; then delete the
merged segments.

Order matters: the note is written and fsync'd **before** any segment is
removed. A crash in that window re-merges segments on the next attempt,
producing at most a duplicated entry — strictly better than losing one.

## 7. Authentication — the real trade-off

Today, needing the private key to append *incidentally proves the
appender is you*. The spool removes that: anyone who knows your public
recipient can drop a convincing entry into your notes, and you could not
tell it from your own.

This is already technically true (encryption is public) — forging a whole
file has always been possible. The spool makes it a supported, invisible
operation, which is a real change in posture.

v1 ships **unsigned**, with the mitigation that entries merged from the
spool are marked in the UI as having been written while locked, so
provenance is visible rather than assumed.

The envelope is designed so signing can be added later: a v2 would add a
`sig:` header and **require** it, rather than adding it as an optional
field a v1 reader would ignore (an ignored-but-present signature is worse
than none, since it invites false confidence). GPG can sign directly; AGE
has no signature primitive, so it would need ssh-sig or minisign.

## 8. Surfacing it

Counting pending entries needs no key (it is a directory listing), so the
menu bar can show the state while locked:

- each note reads `Journal — 3 pending` in the Quick Note submenu;
- `Merge N Pending Entries…` and `Discard N Pending Entries…` sit below,
  disabled when there is nothing to act on;
- the menu-bar glyph gains a badge dot whenever anything is pending, so
  unmerged work is visible without opening the menu.

The scan is throttled (a few seconds) and cached — a quiet app does no
filesystem work per frame.

Merging happens automatically on unlock, and on demand from the menu.
Discarding is confirmed first: those entries cannot be read back without
the key, so the dialog states the count, says the loss is permanent, and
defaults to keeping them.

## 9. Limits

- ~200 bytes of age overhead per segment, so many one-line jots are
  inefficient until merged.
- A note's spool is capped at `quick_note.max_pending` segments (default
  500; 0 disables). Past four fifths of the cap Schl8 says so on every
  spooled entry, and at the cap it refuses to add more.

  The cap is enforced in `write_segment`, the one place segments are
  created, so no caller can grow a spool past it. Refusing is the safe
  side of the trade: in the app a refused spool falls back to the
  seed-phrase prompt, so the entry is still saved — while an unbounded
  spool has no recovery at all, and the write-only CLI would otherwise
  give a looping agent a way to fill the disk.

  500 is far past ordinary use (500 jots without ever unlocking) and
  about 100 KB of overhead, so it bounds pathology rather than budgeting
  storage. The cap counts *pending* entries, not lifetime writes:
  merging frees the room again.
- Editing an existing note still requires the key — this is a
  **write-only** capability by construction.
