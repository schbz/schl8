# Schl8 age backend — design & derivation spec

Status: **proposal for review.** As of this commit only the reference
harness (`src/bin/age_vectors.rs`) exists — the app's crypto and UI paths
are untouched. Nothing here ships until the derivation below is approved,
because **the derivation can never change** without locking existing
users out of their files.

This document specifies a second encryption backend for Schl8:
[**age**](https://age-encryption.org) with a private key derived from a
12-word **BIP-39** seed phrase, as an alternative to GPG + YubiKey.

---

## 1. Goals & decisions

- A parallel encryption backend, offered everywhere GPG is (encrypt
  dialog, Save Targets, quicknote registry, new-file flow, loader).
- The user's **private key is derived from a 12-word BIP-39 phrase**,
  entered at unlock, **held only in mlock'd memory**, and **never written
  to disk**. Removed on quit or an explicit "Forget key"; **not** dropped
  by idle auto-lock (per product decision — see the tradeoff in §6).
- **age recipient public keys** (`age1…`) can be imported, listed, and
  deleted like GPG public keys, and used as encryption identities.
- **GPG becomes optional at runtime.** If `gpg`/`pinentry` aren't
  installed, Schl8 still runs; only the age backend is offered.
- **Save (re-encrypt in place) defaults to your own identity.** age
  ciphertext does not record its recipients (§7), so multi-recipient age
  files use an explicit Save Targets plan, which Schl8 remembers.
- The optional BIP-39 **passphrase (the "25th word")** is supported.
- A dedicated YubiKey age flow is deferred; until then, a hardware key
  that types a preset string is just text entered into the phrase field.

---

## 2. Cryptographic primitives

| Layer | Choice | Crate |
|-------|--------|-------|
| Mnemonic | BIP-39, 12 words (128-bit entropy) | `bip39` 2.x |
| Phrase → seed | BIP-39 PBKDF2-HMAC-SHA512, 2048 iters | `bip39` |
| Seed → key | HKDF-SHA256 | `hkdf` 0.12 + `sha2` 0.10 |
| Identity encoding | Bech32 → age X25519 identity | `bech32` 0.12, `age` 0.12 |
| File encryption | age (X25519 + ChaCha20-Poly1305) | `age` 0.12 |

Why age over GPG or a custom format: a real spec, multiple
interoperable implementations (`age`, `rage`, the Go `age` CLI), modern
primitives, and a recipient model that maps cleanly onto Schl8's
existing "encrypt to recipient(s)" architecture.

Why require valid BIP-39: BIP-39's phrase→seed step uses only 2048 PBKDF2
iterations — far too weak to protect a *human-chosen* passphrase. It is
only safe because a valid BIP-39 phrase carries a full 128 bits of
entropy. **Schl8 must generate the phrase with a CSPRNG and reject any
input that fails the BIP-39 checksum** — this is the brute-force
guarantee. A memorable phrase is not equivalent and must never be
accepted.

### 2.1 New-key generation

`generate_mnemonic_with_entropy(user)` produces the 128-bit BIP-39
entropy as `HKDF-SHA256(salt = "schl8.age.mnemonic.v1",
ikm = 32 getrandom bytes ‖ user)`. Because a full 32 bytes of OS CSPRNG
output are always mixed in, the result is never weaker than the CSPRNG
alone — optional user "mash the keyboard" entropy is pure
defense-in-depth and cannot lower the strength. This salt is *not*
compatibility-critical (unlike the v1 identity derivation): the output
is a random phrase the user writes down, so changing the generation
process never invalidates an existing key. The UI shows a conservative
Shannon estimate of the user's typed entropy as a guide only.

---

## 3. The derivation (FROZEN — v1)

Given a BIP-39 mnemonic `M` and an optional passphrase `P` (default `""`):

```
1. Validate: M must be a valid 12-word BIP-39 mnemonic (checksum OK),
   else reject.
2. seed  = BIP39-seed(M, P)                       # 64 bytes
           = PBKDF2-HMAC-SHA512(password = UTF8-NFKD(M words),
                                salt     = "mnemonic" || UTF8-NFKD(P),
                                iters    = 2048, dkLen = 64)
3. okm   = HKDF-SHA256(ikm  = seed,
                       salt = "schl8.age.identity.v1",
                       info = "x25519-secret-key")   # 32 bytes
4. secret = uppercase( bech32(hrp = "age-secret-key-", data = okm) )
          # → "AGE-SECRET-KEY-1…"
5. identity  = age::x25519::Identity::from_str(secret)
6. recipient = identity.to_public()                # "age1…"
```

Frozen constants (changing any is a breaking v2):

- HKDF salt: the exact ASCII bytes `schl8.age.identity.v1`
- HKDF info: the exact ASCII bytes `x25519-secret-key`
- Bech32 HRP: `age-secret-key-` (note the trailing hyphen; age's format)

**Not interchangeable with Schlate.** Schl8 began as a rename of an
earlier project whose salt was `schlate.age.identity.v1`. The salt is an
input to the derivation, so the same twelve words produce a *different*
identity in each — files encrypted by one cannot be opened by the other,
and there is no migration short of decrypting with the original tool and
re-encrypting here. This was a deliberate clean break rather than an
oversight: do not "fix" it by restoring the old salt, which would
silently change every key this version derives.

Steps 2–3 keep the raw seed transient; only `okm` (32 bytes) is retained
in a `SecureBuffer`, and the transient `age::Identity` is rebuilt from it
per operation (§6).

### 3.1 Reference test vectors

Computed and round-trip-verified by `cargo run --bin age_vectors`
(encrypt to the recipient, decrypt with the identity, compare). The
mnemonics are public BIP-39 test vectors, so the secrets are safe to
publish here; **real phrases must never be printed or logged.**

**Vector A** — mnemonic `abandon abandon abandon abandon abandon abandon
abandon abandon abandon abandon abandon about`, no passphrase:

```
secret:    AGE-SECRET-KEY-1C0EZE5DAQKF76P7NMEJD8LH65TJSQLFGHP7NE0M9HRLU8U0PAFZQF2VS6P
recipient: age14zee60n7ltlay00sn00h5z93wk7c8d90eu42v7hydmp90acl69wqj909lk
```

**Vector B** — same mnemonic, passphrase `TREZOR`:

```
secret:    AGE-SECRET-KEY-1UG6VT0N0WLJ5D07QFCTJK200LDWUVZ70Z6LD26KWCDGZL6G63DLSTZ0UCT
recipient: age1rz7d8m40mr8va447gumdu5k0rdsu7q4rpwp7gawqayzzs5j4lelshll4ys
```

**Vector C** — mnemonic `legal winner thank year wave sausage worth
useful legal winner thank yellow`, no passphrase:

```
secret:    AGE-SECRET-KEY-14VFJLHM503Y76SYNJEY6PVVTP4TZP9ZX2H69C00J8GNPWD30K48QLMK2YX
recipient: age1qtuv8xp8k75vqjf9z7cgs0tg6ulm2hhte2f3h438mkmw8jkkmexsae4n3k
```

Vectors A and B differ, confirming the 25th-word passphrase changes the
identity. These become locked-in unit tests when the backend lands.

---

## 4. Architecture: a `Backend` trait

The app touches crypto through ~10 functions. Refactor them behind a
trait so age and GPG are peers (Phase 1, pure refactor):

```rust
trait Backend {
    fn kind(&self) -> BackendKind;            // Gpg | Age
    fn available(&self) -> bool;              // gpg: binary present; age: true
    fn decrypt(&self, path: &Path, id: &Identity) -> Result<Decrypted>;
    fn encrypt(&self, plaintext: &[u8], to: &[Recipient], armor: bool) -> Result<Vec<u8>>;
    fn recipients_of(&self, path: &Path) -> Option<Vec<Recipient>>; // age: None (§7)
}
```

A unified recipient replaces the GPG-shaped `PublicKey` in shared UI:

```rust
struct Recipient { backend: BackendKind, id: String, label: String }
// gpg:  id = primary-key fingerprint
// age:  id = "age1…" recipient string
```

Format detection in the loader (parallels the existing `.md.gpg`
double-extension logic):

- GPG: OpenPGP packets / ASCII-armor header; `.gpg` `.asc` `.pgp`
- age: binary `age-encryption.org/v1` header or the
  `-----BEGIN AGE ENCRYPTED FILE-----` armor; extension `.age`
  (e.g. `notes.md.age`)

---

## 5. Recipient store (age public keys)

age has no keyring daemon, so Schl8 keeps its own list of imported
recipients in `~/.config/schl8/age-recipients.toml` (public keys only —
non-secret):

```toml
[[recipient]]
label = "My laptop"
recipient = "age14zee60n7ltlay00sn00h5z93wk7c8d90eu42v7hydmp90acl69wqj909lk"
```

Import by pasting an `age1…` string or from a file (validated on entry);
list/delete parallels the GPG key manager. The unlocked identity's own
recipient is auto-added to the available list, labeled
"This device (seed phrase)".

---

## 6. In-memory key handling

Reuses the existing secure-memory machinery:

- On unlock: derive `okm` (32 bytes) into a **`SecureBuffer`** (mlock'd,
  zeroize-on-drop). Zeroize the mnemonic string the instant derivation
  finishes.
- Per decrypt: rebuild the transient `age::x25519::Identity` from `okm`,
  use it, drop it immediately (its lifetime is a single call).
- The `okm` buffer is held until **quit** or **"Forget key"**; it is
  **not** cleared by idle auto-lock (product decision).

**Tradeoff to surface in-app:** unlike a YubiKey, this key lives in the
Mac's RAM for the whole session. Malware running as the user, a memory
scraper, or a tampered Schl8 build could capture it while it is
resident — and keeping it across auto-locks widens that window versus the
document buffers, which auto-lock still zeroizes. This is a
convenience-vs-hardware-isolation choice, not a security upgrade, and the
unlock UI should say so.

---

## 7. Consequences to accept

1. **age ciphertext does not reveal its recipients** (X25519 stanzas
   carry ephemeral shares, not recipient keys — by design, for recipient
   privacy). So "Save re-encrypts to the same recipients" cannot
   auto-discover them the way GPG does. Resolution: Save defaults to your
   own identity; multi-recipient files carry a Save Targets plan that
   Schl8 remembers (public recipients, in config).
2. **age has no signatures.** age files always show "Unsigned"; the
   signature badge is GPG-only.
3. **The phrase is one irreplaceable secret** — no revocation, no expiry,
   no subkeys. Whoever holds the 12 words (plus the 25th word, if set)
   has everything, forever.
4. **The derivation is a Schl8 convention**, not an age standard.
   Frozen here and pinned by the vectors in §3.1; a v2 would keep v1 as a
   legacy read path.
5. **New crypto glue on an unaudited app.** The primitives (age, BIP-39,
   HKDF) are solid and widely used; the composition is new. This ships
   behind the same "experimental, unaudited" framing as the rest of
   Schl8.

---

## 8. Implementation phases & status

Branch: **`ageadd`** (not merged to master).

- [x] **0. Design doc + reference vectors.**
- [x] **2. Age core:** `crypto::age_backend` — derivation into
  `SecureBuffer`, encrypt/decrypt, generate/validate mnemonic,
  `recipient_from_mnemonic`, `looks_like_age`. §3.1 vectors + round-trip
  are unit tests (8).
- [x] **3. Identity session:** unlock dialog (BIP-39 validated, optional
  passphrase), hold `Option<AgeIdentity>` in memory, "Forget", export
  public key with Save-to-file. Keys menu wired.
- [x] **open/decrypt age files:** `loader::is_age_file` / `load_age`;
  main-thread age open; unlock-then-open flow. End-to-end tested.
- [x] **create/save age notes:** the encrypt method + key are chosen at
  save time (Encrypt & Save As has a GPG/age backend selector); Save
  encrypts to the unlocked identity (default-to-self) + atomic write.
  Re-encrypting an age file no longer stacks `.age.age` (fixed in
  `suggest_encrypted_name`).
- [x] **4. Age recipient store:** import / list / delete `age1…`
  recipients under Manage Public Keys (name + date, in `config`);
  encrypt-to-others via the encrypt dialog; "Add to my keys" in export.
- [x] **generate a new key:** Manage Public Keys → "Generate new age
  key…" mixes OS CSPRNG with optional user entropy (§2.1), shows a live
  strength meter, displays the phrase once for write-down, and offers to
  store its recipient. Module-level `dead_code` allow removed.
- [x] **1. GPG optional at runtime:** `gpg::gpg_available()` is checked
  once at startup. In age-only mode the Encrypt & Save dialog is fixed to
  age (the backend selector is hidden), the GPG import buttons (menu +
  key manager) are disabled with an explanatory tooltip, the key manager
  shows an age-only note, and a one-time startup toast points the user at
  Unlock Age Identity. `SCHL8_FORCE_NO_GPG=1` forces the mode for
  testing on a machine that has gpg.
- [x] **age quicknotes:** `SaveRule` gained an `age_recipient` field
  (mutually exclusive with `key_fingerprint`); `multisave::execute` and
  the append flow encrypt/decrypt via age when it is set. The quicknote
  manager's key dropdown lists stored age recipients + the unlocked
  identity. Appending needs the identity unlocked (to decrypt-then-
  re-append); a locked identity is refused with a clear message. The
  append path detects age by the header magic, so it works even if the
  file is misnamed `.gpg`. End-to-end test (no gpg) covers create +
  two appends + locked-refusal.
- [x] **Finder association for `.age`:** the bundle declares a
  `com.functiondesk.schl8.age` UTI + `.age` document type, so
  "note.md.age" shows Schl8 under Open With. (`.txt`/`.md` remain
  declared at Alternate rank.) Requires a rebuild + reinstall
  (`scripts/bundle.sh --install`) to register.
- [ ] **5. Remaining unification:** age recipients in the Save Targets
  dialog; per-rule backend there; age in the on-disk save-plan UI.
  (Encrypt dialog + quicknotes already offer age.)
- [ ] **6. Polish:** threat-model update (README + SECURITY.md),
  YubiKey-for-age long-press path, homepage note.

What works today on `ageadd`: unlock/forget a seed-phrase identity,
export its public key, and a full personal-notes round trip — create an
age note, save it encrypted to yourself, reopen it. Encrypting to *other
people's* age keys needs the recipient store (Phase 4).
