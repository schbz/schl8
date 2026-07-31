#!/bin/zsh
# Cut a release: check everything, tag, push, and hand off to CI.
#
#   ./scripts/release.sh --check     what would happen; changes nothing
#   ./scripts/release.sh             tag and push, then watch the run
#
# The version comes from Cargo.toml — this script never edits it. Bump
# the manifest and write the CHANGELOG entry first, then run this.
#
# Why a script rather than a handful of git commands: a tag is permanent
# and a published release's assets cannot be swapped. Everything below
# is a check that is cheap now and expensive to discover afterwards.
set -euo pipefail
cd "$(dirname "$0")/.."

DRY=0
[[ "${1:-}" == "--check" || "${1:-}" == "--dry-run" ]] && DRY=1

VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
TAG="v$VERSION"
fail() { print -u2 "error: $*"; exit 1; }
step() { print "\n==> $*" }

step "Releasing $TAG"

# ── Preconditions ────────────────────────────────────────────────────
step "Checking the repository"
[[ -d .git ]] || fail "no git repository here"
git rev-parse --verify HEAD >/dev/null 2>&1 || fail "no commits yet"

if [[ -n "$(git status --porcelain)" ]]; then
    git status --short
    fail "working tree is dirty — commit or stash first, so the tag \
points at exactly what you tested"
fi

if git rev-parse "$TAG" >/dev/null 2>&1; then
    fail "$TAG already exists locally. Releases are immutable: bump the \
version in Cargo.toml rather than moving a tag people may already have."
fi

BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [[ "$BRANCH" != "master" && "$BRANCH" != "main" ]]; then
    print "note: releasing from branch '$BRANCH', not master/main."
fi

# The lockfile carries the version too; a stale one means the built
# binary and the tag disagree.
LOCK_VERSION=$(awk '/^name = "schl8"$/{getline; gsub(/[^0-9.]/,""); print; exit}' Cargo.lock)
[[ "$LOCK_VERSION" == "$VERSION" ]] || \
    fail "Cargo.lock says $LOCK_VERSION but Cargo.toml says $VERSION — run \`cargo build\`"

# A release with no changelog entry is one nobody can read later.
if ! grep -q "^## \[$VERSION\]" CHANGELOG.md; then
    fail "CHANGELOG.md has no '## [$VERSION]' section"
fi

# ── The same gates CI runs, before burning a tag on a red build ───────
step "Format"; cargo fmt --check
step "Clippy";  cargo clippy --all-targets -- -D warnings
step "Tests";   cargo test --quiet
step "Bundle";  ./scripts/bundle.sh >/dev/null && print "Schl8.app builds"

BUILT=$(dist/Schl8.app/Contents/MacOS/schl8 --version | awk '{print $2}')
[[ "$BUILT" == "$VERSION" ]] || fail "built binary reports $BUILT, expected $VERSION"

if [[ $DRY -eq 1 ]]; then
    print "\n==> All checks passed. Would tag $TAG and push it."
    print "    Re-run without --check to release."
    exit 0
fi

# ── Tag and hand off ─────────────────────────────────────────────────
step "Tagging $TAG"
# Unsigned: this repo's configured signing key is not loaded.
git tag -a "$TAG" -m "Schl8 $VERSION" --no-sign 2>/dev/null || git tag -a "$TAG" -m "Schl8 $VERSION"
git push origin "$TAG"

step "CI is building the release"
if command -v gh >/dev/null 2>&1; then
    sleep 3
    gh run watch --exit-status "$(gh run list --workflow=release.yml --limit 1 --json databaseId --jq '.[0].databaseId')" || \
        print "note: could not attach to the run; check the Actions tab."
    print "\nThe release is a DRAFT. Review the assets, then publish it:"
    print "    gh release edit $TAG --draft=false"
    print "Until then the rolling 'nightly' build stays the newest download."
else
    print "Pushed $TAG. Watch progress in the Actions tab; the release"
    print "lands as a draft for you to review and publish."
fi
