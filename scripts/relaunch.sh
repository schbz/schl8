#!/bin/zsh
# Rebuild, reinstall, and restart Schl8 — the inner development loop.
#
#   ./scripts/relaunch.sh
#
# Order of operations, and why it is not the obvious one:
#
#   1. build          the slow part; the running app is untouched, so you
#                     can keep using it, and a build failure aborts here
#                     leaving your current Schl8 alive and installed
#   2. quit           gracefully, while /Applications/Schl8.app still
#                     matches the running process
#   3. install        replace the bundle now that nothing is using it
#   4. launch         the new build
#
# The tempting order is build+install first and quit afterwards. That was
# tried and the graceful quit did not survive it: after `--install` has
# done its rm -rf / cp -R / lsregister dance, `tell application "Schl8"
# to quit` stopped reaching the running process and every rebuild fell
# through to a forced kill five seconds later. Quitting before the
# bundle moves sidesteps the whole interaction — and a forced kill is
# not cosmetic here, because Schl8 encrypts unsaved edits into its
# stash on a clean shutdown and a signal skips that entirely.
#
# "Any running Schl8" is deliberate: `pgrep -x schl8` matches on
# process name, so it catches both the installed build and a `cargo run`
# debug build. Both are usually the point — a stale debug instance still
# holding the global hotkeys is exactly what makes you think the rebuild
# did not work.
set -euo pipefail
cd "$(dirname "$0")/.."

APP=/Applications/Schl8.app

echo "==> Building…"
./scripts/bundle.sh

echo "==> Stopping any running Schl8…"
if pgrep -x schl8 >/dev/null 2>&1; then
    # `quit` rather than a signal, so the app can stash unsaved edits.
    # The timeout keeps an unsaved-changes dialog from wedging the task.
    osascript -e 'with timeout of 5 seconds
        tell application "Schl8" to quit
    end timeout' >/dev/null 2>&1 || true

    for _ in {1..20}; do
        pgrep -x schl8 >/dev/null 2>&1 || break
        sleep 0.25
    done

    if pgrep -x schl8 >/dev/null 2>&1; then
        # Reached only if the app ignored the quit or is showing a modal.
        # This discards unsaved edits, so say so rather than hiding it in
        # a spinner.
        echo "    did not quit in 5s — forcing (unsaved edits will be lost)"
        pkill -x schl8 || true
        sleep 0.5
    fi
    echo "    stopped"
else
    echo "    nothing running"
fi

echo "==> Installing…"
./scripts/bundle.sh --install-only

# Launch Services re-registers the bundle asynchronously; give it a beat
# before asking it to open the thing it is still indexing.
sleep 0.5

echo "==> Launching $APP"
# Plain `open`, not `open -n`. The two differ only if something survived
# the step above, and there the difference matters: `-n` would start a
# second instance, and two menu-bar residents fighting over the same
# global hotkey registrations is worse than reactivating the survivor.
open "$APP"

# Report what actually happened rather than assuming it worked.
for _ in {1..20}; do
    pgrep -x schl8 >/dev/null 2>&1 && break
    sleep 0.25
done
VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
if PID=$(pgrep -x schl8); then
    echo "==> Done — Schl8 $VERSION running (pid ${PID//$'\n'/, })"
else
    echo "==> Installed Schl8 $VERSION, but it is not running — check Console" >&2
    exit 1
fi
