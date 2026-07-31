#!/usr/bin/env python3
"""Count the lines of code that actually ship.

Tests are excluded, because "how big is this application" and "how much
of this repository is Rust" are different questions and only the first
one is interesting. Two shapes of test are stripped:

  * `#[cfg(test)] mod tests { … }` blocks inside a source file — every
    one in this repo starts at column 0 and closes with a bare `}` at
    column 0, which is what the block scanner relies on;
  * whole files declared as `#[cfg(test)] mod name;` — currently just
    `src/crypto/integration_tests.rs`, which is 100% test code and would
    otherwise be counted in full.

The parse is naive by design (no Rust grammar, no string-literal
awareness), so two checks keep it honest and both exit non-zero:

  * reconciliation — every line of every file must land in exactly one
    bucket, catching anything the scanner drops on the floor;
  * an unclosed-block check — reconciliation cannot see this one, since
    a test module that never closes just swallows the rest of the file
    into the test bucket and the totals still balance.

Between them, a wrong number fails the task rather than being printed.
"""

import re
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src"

CFG_TEST = re.compile(r"^#\[cfg\(test\)\]\s*$")
MOD_BLOCK = re.compile(r"^\s*(?:pub\s+)?mod\s+[\w:]+\s*\{")
MOD_DECL = re.compile(r"^\s*(?:pub\s+)?mod\s+([\w]+)\s*;")


def test_only_files() -> set[Path]:
    """Files that exist solely to hold tests, found by their declaration.

    A `#[cfg(test)] mod foo;` names `foo.rs` (or `foo/mod.rs`) beside the
    declaring file. Discovering them this way rather than by filename
    means a future test module gets excluded automatically instead of
    silently inflating the count.
    """
    found: set[Path] = set()
    for path in SRC.rglob("*.rs"):
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        for i, line in enumerate(lines[:-1]):
            if not CFG_TEST.match(line):
                continue
            m = MOD_DECL.match(lines[i + 1])
            if not m:
                continue
            for cand in (
                path.parent / f"{m.group(1)}.rs",
                path.parent / m.group(1) / "mod.rs",
            ):
                if cand.exists():
                    found.add(cand.resolve())
    return found


class Counts:
    __slots__ = ("code", "comment", "blank", "test")

    def __init__(self) -> None:
        self.code = self.comment = self.blank = self.test = 0

    def total(self) -> int:
        return self.code + self.comment + self.blank + self.test

    def add(self, other: "Counts") -> None:
        self.code += other.code
        self.comment += other.comment
        self.blank += other.blank
        self.test += other.test


def classify(path: Path, whole_file_is_test: bool, problems: list[str]) -> Counts:
    c = Counts()
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()

    if whole_file_is_test:
        c.test = len(lines)
        return c

    in_test_block = False
    i = 0
    while i < len(lines):
        line = lines[i]

        if in_test_block:
            c.test += 1
            # Every `#[cfg(test)] mod` in this repo sits at column 0, so
            # its closing brace does too. Anything else is caught by the
            # reconciliation check rather than assumed away.
            if line == "}":
                in_test_block = False
            i += 1
            continue

        if CFG_TEST.match(line) and i + 1 < len(lines) and MOD_BLOCK.match(lines[i + 1]):
            in_test_block = True
            c.test += 1
            i += 1
            continue

        stripped = line.strip()
        if not stripped:
            c.blank += 1
        elif stripped.startswith("//"):
            c.comment += 1
        else:
            c.code += 1
        i += 1

    if in_test_block:
        # The reconciliation at the end cannot catch this: an unclosed
        # block swallows the rest of the file into the test bucket, so
        # the line totals still balance while the split is nonsense.
        # This is the only signal, so it has to reach the exit code —
        # a warning that scrolls past in a task terminal is not a check.
        problems.append(
            f"{path.relative_to(ROOT)}: test module never closes at column 0"
        )
    return c


def commas(n: int) -> str:
    return f"{n:,}"


def main() -> int:
    if not SRC.is_dir():
        print("error: no src/ directory — run this from the repo", file=sys.stderr)
        return 1

    test_files = test_only_files()
    per_file: dict[Path, Counts] = {}
    per_dir: dict[str, Counts] = defaultdict(Counts)
    problems: list[str] = []

    for path in sorted(SRC.rglob("*.rs")):
        resolved = path.resolve()
        c = classify(path, resolved in test_files, problems)
        per_file[path] = c
        # Group by top-level area: `src/ui/theme.rs` → `ui`, `src/app.rs` → `(root)`.
        rel = path.relative_to(SRC)
        per_dir[rel.parts[0] if len(rel.parts) > 1 else "(root)"].add(c)

    grand = Counts()
    for c in per_file.values():
        grand.add(c)

    shipping = grand.code + grand.comment + grand.blank

    print()
    print("  Schl8 — lines of code")
    print(f"  {len(per_file)} Rust files under src/")
    print()
    print("  Shipping code (tests excluded)")
    print(f"    code        {commas(grand.code):>7}")
    print(f"    comments    {commas(grand.comment):>7}")
    print(f"    blank       {commas(grand.blank):>7}")
    print(f"    {'-' * 20}")
    print(f"    total       {commas(shipping):>7}")
    print()
    print(f"  Excluded: {commas(grand.test)} lines of tests", end="")
    if test_files:
        names = ", ".join(sorted(p.relative_to(ROOT).as_posix() for p in test_files))
        print(f" (including all of {names})")
    else:
        print()
    print()

    print("  By area (code lines only)")
    for name, c in sorted(per_dir.items(), key=lambda kv: -kv[1].code):
        print(f"    {name:<14} {commas(c.code):>7}")
    print()

    print("  Largest files (code lines)")
    top = sorted(per_file.items(), key=lambda kv: -kv[1].code)[:8]
    for path, c in top:
        print(f"    {path.relative_to(ROOT).as_posix():<34} {commas(c.code):>6}")
    print()

    # Reconciliation: the parse is naive, so prove it lost nothing.
    on_disk = sum(
        len(p.read_text(encoding="utf-8", errors="replace").splitlines())
        for p in per_file
    )
    if on_disk != grand.total():
        print(
            f"  WARNING: counted {commas(grand.total())} lines but the files "
            f"hold {commas(on_disk)} — the parse dropped something.",
            file=sys.stderr,
        )
        return 1

    # Non-Rust sources, mentioned rather than mixed in: they are part of
    # the project but not part of "the application".
    shell = sorted((ROOT / "scripts").glob("*.sh")) + sorted(
        (ROOT / "scripts").glob("*.py")
    )
    if shell:
        n = sum(len(p.read_text(errors="replace").splitlines()) for p in shell)
        print(f"  Also: {commas(n)} lines of build/dev scripts ({len(shell)} files)")
        print()

    if problems:
        print("  WARNING: the test/code split is unreliable —", file=sys.stderr)
        for p in problems:
            print(f"    {p}", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
