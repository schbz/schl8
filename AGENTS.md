# Schl8 — agent instructions

The architecture map, the commands, and the **security invariants every change
must preserve** live in **[CLAUDE.md](CLAUDE.md)**. Read that file.

This one used to be a byte-for-byte copy of it, which is a guarantee that the
two eventually disagree without anyone noticing. A pointer instead.

- Working on the code? → [CLAUDE.md](CLAUDE.md), then
  [CONTRIBUTING.md](CONTRIBUTING.md).
- Want Schl8 to *store things for you* — appending to encrypted notes,
  encrypting output — rather than to edit its source? That is a different,
  write-only surface, and it describes itself:

  ```sh
  schl8 agent brief
  ```

  It prints instructions generated from the live configuration: the notes
  folder, the recipients it may encrypt to, the notes it may append to. It can
  write; it cannot decrypt, unlock, or read anything back, and there is no
  command that does. See [docs/AGENT-DESIGN.md](docs/AGENT-DESIGN.md).
