#!/bin/zsh
# One-time: create a stable, self-signed code-signing certificate so
# macOS remembers Schl8's Desktop/Documents/Downloads folder-access
# grants across rebuilds.
#
# Why this is needed: macOS ties a folder-access "Allow" to the app's
# code-signing identity. An ad-hoc signed app (Schl8's default) is
# keyed to its exact binary hash, which changes on every rebuild — so
# macOS treats each new build as a new app and re-prompts. Signing with a
# stable identity keeps the grant permanently.
#
# Run this ONCE:   ./scripts/setup-signing.sh
# Then rebuild:    ./scripts/bundle.sh --install   (auto-uses the cert)
#
# Undo anytime in Keychain Access: delete the "Schl8 Code Signing"
# certificate from your login keychain. This touches only your own login
# keychain — no admin rights, no system settings.
set -euo pipefail

CERT_NAME="Schl8 Code Signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-identity -v -p codesigning 2>/dev/null | grep -q "$CERT_NAME"; then
    echo "\"$CERT_NAME\" already exists — nothing to do."
    echo "Rebuild with ./scripts/bundle.sh --install to sign with it."
    exit 0
fi

echo "Creating a self-signed \"$CERT_NAME\" certificate in your login keychain…"
echo "(macOS may ask for your login password to authorize this.)"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# A code-signing cert needs the codeSigning extended key usage.
cat > "$TMP/cert.conf" <<CONF
[ req ]
distinguished_name = dn
x509_extensions    = v3
prompt             = no
[ dn ]
CN = $CERT_NAME
[ v3 ]
basicConstraints   = critical, CA:false
keyUsage           = critical, digitalSignature
extendedKeyUsage   = critical, codeSigning
CONF

openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
    -keyout "$TMP/key.pem" -out "$TMP/cert.pem" -config "$TMP/cert.conf"

openssl pkcs12 -export -name "$CERT_NAME" \
    -inkey "$TMP/key.pem" -in "$TMP/cert.pem" \
    -out "$TMP/cert.p12" -passout pass:schl8

# Import the key+cert, allowing codesign to use it without further prompts.
security import "$TMP/cert.p12" -k "$KEYCHAIN" -P schl8 \
    -T /usr/bin/codesign -T /usr/bin/security

# Trust the self-signed cert for code signing in the login keychain only.
security add-trusted-cert -d -r trustAsRoot \
    -p codeSign -k "$KEYCHAIN" "$TMP/cert.pem" 2>/dev/null || \
    echo "note: could not set trust automatically; codesign will still use it."

echo
if security find-identity -v -p codesigning | grep -q "$CERT_NAME"; then
    echo "✓ Created \"$CERT_NAME\"."
    echo "Now run:  ./scripts/bundle.sh --install"
    echo "The first Desktop save after that will prompt once, then never again."
else
    echo "The certificate was imported but isn't showing as a valid signing"
    echo "identity yet. Open Keychain Access → login → \"$CERT_NAME\" → Get"
    echo "Info → Trust → Code Signing: Always Trust, then rerun bundle.sh."
fi
