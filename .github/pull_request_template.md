<!--
Thanks for the pull request. CONTRIBUTING.md has the full detail; this is the
short version. Delete anything that doesn't apply.
-->

## What this changes

A sentence or two on the behaviour that differs afterwards.

## Why

The problem it solves. The diff already shows the *what* — this is the part
review can't reconstruct.

Closes #

## How it was verified

<!-- "Tests pass" is enough for a pure refactor. Anything touching crypto,
     saving, or locking deserves a real round-trip: encrypt it, close it,
     reopen it, confirm the bytes. -->

## Checks

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test` (with `gpg` installed, so the round-trip tests actually run)
- [ ] `CHANGELOG.md` `[Unreleased]` updated, if the change is user-visible

## Security invariants

These are the properties Schl8 exists to keep. Confirm the change preserves
them, or say explicitly which one it bends and why.

- [ ] No plaintext written to disk — no export, temp file, cache, or log of
      document content
- [ ] Plaintext stays in `SecureBuffer` / `SecureString`, not plain
      `String` / `Vec<u8>`
- [ ] `relock_if_moved()` called after mutating a `SecureString`
- [ ] Locking still can't destroy unsaved work, and unsaved work still can't
      block a lock
- [ ] No new path puts document content on the clipboard
- [ ] Error and toast messages name files, never contents
