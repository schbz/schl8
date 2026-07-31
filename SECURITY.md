# Security Policy

## Status: experimental, unaudited

Schl8 is a personal, open-source hobby project. It has **never had an
independent security audit**. The security measures described in the README
(mlock'd buffers, zeroization, no plaintext on disk, clipboard suppression,
compile-time trait locks) reflect design intent and are covered by tests
where practical — but no third party has verified them.

**Do not use Schl8 as the primary safeguard for data whose exposure or
loss would seriously harm you.** Keep independent backups of anything it
touches. If your threat model is serious, read the source and make your own
judgment — or use it as a reference for building a tool you fully control.

## Threat model

What Schl8 tries to defend against, and what it explicitly does not, is
documented in the README's [Security model](README.md#security-model)
section. Highlights of what is **out of scope**:

- Attackers with root or kernel-level access
- A compromised `gpg`, `gpg-agent`, or `pinentry` binary
- Hardware keyloggers and screen capture
- Transient plaintext in the gpg subprocess pipe, in freed memory after a
  buffer reallocation, or in egui's per-frame text-change bookkeeping

Reports about limitations already listed there are welcome as ideas, but
are not treated as vulnerabilities.

## Supported versions

Only the latest release is supported. There are no backported fixes.

The rolling [`nightly`](https://github.com/schbz/schl8/releases/tag/nightly)
build is rebuilt from every commit that passes the test suite. It has had no
release review, and a fix reported against it may be a bug that never reached
a tagged version — that is still worth reporting, just say which build you
were on.

## Reporting a vulnerability

If you find a way to make Schl8 leak plaintext or key material *within
its documented threat model* (e.g. plaintext written to disk, retained
after close, exposed via clipboard without opt-in, or a save that silently
produces unencrypted output):

- Preferred: open a **private security advisory** on GitHub
  (Security → Advisories → "Report a vulnerability")
- Or open a regular issue if the problem is not sensitive

Please include reproduction steps and your macOS + GnuPG versions. This is
a solo project with no security team and no bounty program; I read reports
as time permits and will credit reporters in the changelog unless they ask
otherwise.
