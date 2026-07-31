# Contributing to Schl8

Thanks for looking. Issues and pull requests are welcome.

This is a one-developer hobby project, so there's no support commitment and no
guaranteed response time. What there *is* is a short list of properties the
code must keep. Read that part even if you skip the rest.

## The part that matters: security invariants

Schl8's whole reason to exist is that decrypted text stays in locked memory and
never lands anywhere it could be recovered. A change that quietly breaks one of
these rules is worse than no change, because the app keeps looking like it
works.

**A pull request must preserve all of these.** They are also documented, with
the code that implements them, in [CLAUDE.md](CLAUDE.md).

1. **Plaintext never touches disk.** Decrypted content flows from the
   decrypter's stdout straight into memory; saves always go back through
   encryption. Never add a plaintext export, a temp file, a cache, or any
   logging of document content.
2. **Plaintext lives only in `SecureBuffer` / `SecureString`.** Don't copy it
   into a plain `String` or `Vec<u8>`. UI code borrows `&str` per frame —
   that's what immediate mode buys us.
3. **After mutating a `SecureString`**, call `relock_if_moved()` so the mlock
   follows the buffer if it reallocated.
4. **Locking never destroys unsaved work, and unsaved work never blocks a
   lock.** Edits are encrypted into the stash and the plaintext dropped. If no
   key is available, the lock is *deferred* — never a silent discard.
5. **No clipboard for document content.** `Copy`/`Cut` events are stripped in
   `App::update` and `selectable_labels` is false. The single exception is
   `src/ui/agent_help.rs`, which copies fixed strings compiled into the binary.
   **Do not extend that exception.**
6. **Core dumps stay disabled.** `lock_down()` stays first in `main`.
7. **Error and toast messages may name files, never contents.**

If your change genuinely needs to bend one of these, say so explicitly in the
PR description and explain the reasoning. Don't bend one silently.

## Getting set up

Requires [Rust](https://rustup.rs/) 1.75+ and, for the full test suite, GnuPG.

```sh
git clone https://github.com/schbz/schl8.git
cd schl8
cargo build
cargo run                        # or: cargo run -- some-file.md.gpg
```

Building the real macOS app, with Finder file associations:

```sh
./scripts/bundle.sh --install
```

## The checks CI runs

Run these before opening a PR — they're the exact gate CI applies, and
`clippy` runs with `-D warnings`, so a warning is a failure.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`cargo test` includes real encrypt/decrypt/append round-trips against an
**ephemeral** keyring — they never touch your own keys. Without `gpg`
installed those tests skip themselves, which means a green run on a machine
with no GnuPG proves less than it looks like it does.

A separate CI job runs `cargo-deny` over advisories, licenses, bans and
sources. A new RUSTSEC advisory or an incompatible license fails the build; if
an item is genuinely acceptable, add it to `deny.toml` **with a rationale**.

### Don't test against your own config

Schl8 reads `~/.config/schl8/config.toml`, and a test run that writes there can
destroy a real setup — notes, keys, destinations, hotkeys. Point it somewhere
disposable:

```sh
XDG_CONFIG_HOME=/tmp/schl8-test cargo run
```

## Conventions

- Rust 2021. `anyhow` for application errors, `thiserror` for typed GPG errors.
- UI style constants live in `src/ui/theme.rs`. No hardcoded colors in new UI
  code — some legacy hardcoded colors remain in dialogs, but don't add more.
- **egui has no system-font fallback.** A codepoint missing from the bundled
  face renders as a tofu box. A test in `src/ui/theme.rs` scans the UI sources
  and fails on known-missing glyphs; if it flags one, pick a different
  character rather than suppressing the test. (`›` and `»` are safe; `→` is
  present in the monospace face only, and `✓` in neither.)
- Comments should explain *why*, not restate the code. Match the density and
  voice of the file you're editing.
- Keep the [CHANGELOG](CHANGELOG.md) `[Unreleased]` section current for
  user-visible changes.

## Reporting bugs

Use the [bug report template](.github/ISSUE_TEMPLATE/bug_report.md). Please
include the Schl8 version (Help → About Schl8), your macOS version, and
whether a hardware key was involved.

**Never paste decrypted content, key material, or a seed phrase into an
issue.** If reproducing a bug seems to require it, say so and we'll find
another way.

## Reporting vulnerabilities

**Don't open a public issue.** See [SECURITY.md](SECURITY.md) for how to report
privately.

## Pull requests

- One logical change per PR; it makes review and reverting possible.
- Explain the *why* in the description. The diff already shows the what.
- Say how you verified it. "Tests pass" is fine for a pure refactor; anything
  touching crypto, saving, or locking deserves a real round-trip.
- New behaviour should come with a test. A test that can't fail is worse than
  no test — where it's practical, break the thing on purpose once to confirm
  the test notices.
