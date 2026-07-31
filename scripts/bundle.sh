#!/bin/zsh
# Build Schl8.app — a proper macOS bundle with file associations.
#
#   ./scripts/bundle.sh              build dist/Schl8.app
#   ./scripts/bundle.sh --install    also copy it to /Applications
#   ./scripts/bundle.sh --install-only
#                                    copy an already-built dist/Schl8.app
#                                    to /Applications without rebuilding
#
# Set SCHL8_BIN=/path/to/schl8 to bundle a prebuilt binary (e.g. a
# universal binary produced with lipo in CI) instead of running cargo.
#
# The bundle registers Schl8 as an "Open With" option for encrypted
# files (.gpg/.asc/.pgp/.age) and plain text/markdown (.txt/.md). To make it
# the DEFAULT app for a file type: Finder → select a file → Get Info →
# "Open with:" → choose Schl8 → "Change All…".
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d'"' -f2)
APP=dist/Schl8.app

# Copy the built bundle into /Applications and register it. A function
# because two entry points need it: --install (build then install) and
# --install-only (install what is already in dist/). The second exists so
# a caller can quit the running app *between* the two — replacing the
# bundle under a live process is what relaunch.sh is avoiding.
install_bundle() {
    rm -rf /Applications/Schl8.app
    cp -R "$APP" /Applications/
    # Register the bundle, its document types, and its exported UTIs with
    # LaunchServices. Unregistering first clears any stale record from a
    # previous build (otherwise old type claims can linger and Schl8
    # won't show up under "Open With").
    LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
    "$LSREGISTER" -u /Applications/Schl8.app 2>/dev/null || true
    "$LSREGISTER" -f -R /Applications/Schl8.app || true
    echo "Registered file associations (.gpg .pgp .asc .age .txt .md)."
    echo "To make Schl8 the default for them: open it and choose"
    echo "  Help → Install & Default Editor… → \"Make Schl8 the default\"."
    echo "Installed to /Applications/Schl8.app"
}

if [[ "${1:-}" == "--install-only" ]]; then
    if [[ ! -d "$APP" ]]; then
        echo "error: $APP does not exist — run ./scripts/bundle.sh first" >&2
        exit 1
    fi
    echo "Installing existing $APP (no rebuild)…"
    install_bundle
    exit 0
fi

if [[ -n "${SCHL8_BIN:-}" ]]; then
    echo "Bundling prebuilt binary $SCHL8_BIN (v$VERSION)…"
    BIN="$SCHL8_BIN"
else
    echo "Building release binary (v$VERSION)…"
    cargo build --release
    BIN=target/release/schl8
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/schl8"
cp assets/schl8.icns "$APP/Contents/Resources/schl8.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>              <string>Schl8</string>
    <key>CFBundleDisplayName</key>       <string>Schl8</string>
    <key>CFBundleIdentifier</key>        <string>com.functiondesk.schl8</string>
    <key>CFBundleVersion</key>           <string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundlePackageType</key>       <string>APPL</string>
    <key>CFBundleExecutable</key>        <string>schl8</string>
    <key>CFBundleIconFile</key>          <string>schl8</string>
    <key>LSMinimumSystemVersion</key>    <string>11.0</string>
    <key>NSHighResolutionCapable</key>   <true/>
    <key>NSPrincipalClass</key>          <string>NSApplication</string>
    <key>CFBundleDocumentTypes</key>
    <array>
        <!-- Encrypted files and encrypted folder archives. Declared by
             both UTI and extension: LaunchServices prefers UTIs, but the
             extension list keeps older/edge cases working. Rank Owner
             because we define the .gpg/.pgp type below. -->
        <dict>
            <key>CFBundleTypeName</key>      <string>GPG Encrypted File</string>
            <key>CFBundleTypeRole</key>      <string>Editor</string>
            <key>LSHandlerRank</key>         <string>Owner</string>
            <key>LSItemContentTypes</key>
            <array>
                <string>com.functiondesk.schl8.gpg</string>
                <string>com.functiondesk.schl8.asc</string>
                <string>com.functiondesk.schl8.age</string>
            </array>
            <key>CFBundleTypeExtensions</key>
            <array>
                <string>gpg</string>
                <string>asc</string>
                <string>pgp</string>
                <string>age</string>
            </array>
            <key>CFBundleTypeIconFile</key>  <string>schl8</string>
        </dict>
        <!-- Plain text and markdown. Rank Alternate: Schl8 offers itself
             in "Open With" and can be set as the default via Get Info,
             without hijacking every .txt on install. -->
        <dict>
            <key>CFBundleTypeName</key>      <string>Text Document</string>
            <key>CFBundleTypeRole</key>      <string>Editor</string>
            <key>LSHandlerRank</key>         <string>Alternate</string>
            <key>LSItemContentTypes</key>
            <array>
                <string>public.plain-text</string>
                <string>public.utf8-plain-text</string>
                <string>public.text</string>
                <string>net.daringfireball.markdown</string>
            </array>
            <key>CFBundleTypeExtensions</key>
            <array>
                <string>txt</string>
                <string>text</string>
                <string>md</string>
                <string>markdown</string>
            </array>
            <key>CFBundleTypeIconFile</key>  <string>schl8</string>
        </dict>
    </array>

    <!-- macOS ships no UTI for OpenPGP files, so Schl8 declares them.
         Both conform to public.data so generic handlers still work. -->
    <key>UTExportedTypeDeclarations</key>
    <array>
        <dict>
            <key>UTTypeIdentifier</key>  <string>com.functiondesk.schl8.gpg</string>
            <key>UTTypeDescription</key> <string>GPG Encrypted File</string>
            <key>UTTypeIconFile</key>    <string>schl8</string>
            <key>UTTypeConformsTo</key>
            <array><string>public.data</string></array>
            <key>UTTypeTagSpecification</key>
            <dict>
                <key>public.filename-extension</key>
                <array>
                    <string>gpg</string>
                    <string>pgp</string>
                </array>
                <key>public.mime-type</key>
                <array><string>application/pgp-encrypted</string></array>
            </dict>
        </dict>
        <dict>
            <key>UTTypeIdentifier</key>  <string>com.functiondesk.schl8.asc</string>
            <key>UTTypeDescription</key> <string>PGP Armored File</string>
            <key>UTTypeIconFile</key>    <string>schl8</string>
            <key>UTTypeConformsTo</key>
            <array><string>public.text</string></array>
            <key>UTTypeTagSpecification</key>
            <dict>
                <key>public.filename-extension</key>
                <array><string>asc</string></array>
                <key>public.mime-type</key>
                <array><string>application/pgp</string></array>
            </dict>
        </dict>
        <!-- age files (.age). The last extension is what LaunchServices
             matches, so "note.md.age" is claimed by this .age type. -->
        <dict>
            <key>UTTypeIdentifier</key>  <string>com.functiondesk.schl8.age</string>
            <key>UTTypeDescription</key> <string>age Encrypted File</string>
            <key>UTTypeIconFile</key>    <string>schl8</string>
            <key>UTTypeConformsTo</key>
            <array><string>public.data</string></array>
            <key>UTTypeTagSpecification</key>
            <dict>
                <key>public.filename-extension</key>
                <array><string>age</string></array>
                <key>public.mime-type</key>
                <array><string>application/age</string></array>
            </dict>
        </dict>
    </array>

    <!-- Markdown has no system UTI on older macOS; import the de-facto one
         so .md files match by type as well as by extension. -->
    <key>UTImportedTypeDeclarations</key>
    <array>
        <dict>
            <key>UTTypeIdentifier</key>  <string>net.daringfireball.markdown</string>
            <key>UTTypeDescription</key> <string>Markdown Document</string>
            <key>UTTypeConformsTo</key>
            <array><string>public.plain-text</string></array>
            <key>UTTypeTagSpecification</key>
            <dict>
                <key>public.filename-extension</key>
                <array>
                    <string>md</string>
                    <string>markdown</string>
                </array>
                <key>public.mime-type</key>
                <array><string>text/markdown</string></array>
            </dict>
        </dict>
    </array>
</dict>
</plist>
PLIST

# Code signature. A STABLE identity lets macOS remember Desktop/Documents
# folder-access grants across rebuilds; ad-hoc signing changes the binary
# hash every build, so macOS forgets the grant and re-prompts. Preference:
#   1. $SCHL8_SIGN_ID (explicit override — e.g. a Developer ID)
#   2. a self-signed "Schl8 Code Signing" cert (./scripts/setup-signing.sh)
#   3. ad-hoc fallback (works, but folder prompts recur on each rebuild)
SIGN_ID="${SCHL8_SIGN_ID:-}"
if [[ -z "$SIGN_ID" ]] && \
   security find-identity -v -p codesigning 2>/dev/null | grep -q "Schl8 Code Signing"; then
    SIGN_ID="Schl8 Code Signing"
fi

if [[ -n "$SIGN_ID" ]]; then
    codesign --force --deep --sign "$SIGN_ID" "$APP"
    echo "Signed with stable identity: $SIGN_ID"
    echo "(macOS will remember folder-access grants across rebuilds.)"
else
    codesign --force --deep --sign - "$APP"
    echo "Signed ad-hoc. macOS re-prompts for Desktop/Documents access on"
    echo "each rebuild because the binary hash changes. To make grants stick,"
    echo "run ./scripts/setup-signing.sh once, then rebuild."
fi

echo "Built $APP"

if [[ "${1:-}" == "--install" ]]; then
    install_bundle
fi
