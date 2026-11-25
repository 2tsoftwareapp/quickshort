#!/bin/sh
# Release pipeline: build (universal when both rustup targets exist) -> bundle
# -> sign -> optionally notarize/staple -> zip + dmg + sha256 in dist/.
# usage: packaging/macos/release.sh [version]   (default: Cargo.toml version)
# env:   SIGNING_IDENTITY (default "quickshort"; "-" = ad-hoc)
#        NOTARY_KEY + NOTARY_KEY_ID + NOTARY_ISSUER enable notarization
set -e
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

VER="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' crates/qs-app/Cargo.toml | head -1)}"
IDENT="${SIGNING_IDENTITY:-quickshort}"
APP="Quickshort.app"
DIST="$ROOT/dist"
mkdir -p "$DIST"

STAGE=""
NOTARY_TMP=""
cleanup() {
  [ -z "$STAGE" ] || rm -rf "$STAGE"
  [ -z "$NOTARY_TMP" ] || rm -rf "$NOTARY_TMP"
}
trap cleanup EXIT

NOTARY_KEY="${NOTARY_KEY:-}"
NOTARY_KEY_ID="${NOTARY_KEY_ID:-}"
NOTARY_ISSUER="${NOTARY_ISSUER:-}"
NOTARIZE=0
if [ -n "$NOTARY_KEY$NOTARY_KEY_ID$NOTARY_ISSUER" ]; then
  if [ -z "$NOTARY_KEY" ] || [ -z "$NOTARY_KEY_ID" ] || [ -z "$NOTARY_ISSUER" ]; then
    echo "notarization needs NOTARY_KEY, NOTARY_KEY_ID and NOTARY_ISSUER together" >&2
    exit 1
  fi
  if [ ! -f "$NOTARY_KEY" ]; then
    echo "notary key file not found: $NOTARY_KEY" >&2
    exit 1
  fi
  NOTARIZE=1
fi

# --- build universal (arm64 + x86_64) when both targets are installed ------
ARCHES=""
for T in aarch64-apple-darwin x86_64-apple-darwin; do
  if rustup target list --installed 2>/dev/null | grep -qx "$T"; then
    ARCHES="$ARCHES $T"
  fi
done
if [ "$(echo "$ARCHES" | wc -w)" -ge 2 ]; then
  for T in $ARCHES; do
    cargo build --locked --release -p qs-app -p qs-editor --target "$T"
  done
  BIN_DIR=target/universal-release
  mkdir -p "$BIN_DIR"
  for BIN in quickshort qs-editor; do
    lipo -create "target/aarch64-apple-darwin/release/$BIN" \
      "target/x86_64-apple-darwin/release/$BIN" -output "$BIN_DIR/$BIN"
  done
else
  cargo build --locked --release -p qs-app -p qs-editor
  BIN_DIR=target/release
fi

# --- bundle ----------------------------------------------------------------
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN_DIR/quickshort" "$BIN_DIR/qs-editor" "$APP/Contents/MacOS/"
sed "s/__VERSION__/$VER/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
cp packaging/macos/Quickshort.icns "$APP/Contents/Resources/Quickshort.icns"

# --- sign (stable identity so TCC grants survive updates) -------------------
if [ "$NOTARIZE" -eq 1 ]; then
  # Production artifacts must not silently fall back when the Developer ID
  # identity is unavailable or mistyped. Sign inside out instead of relying
  # on --deep so the helper binary explicitly carries hardened runtime too.
  codesign --force --sign "$IDENT" --options runtime --timestamp \
    --identifier ai.quickshort.qs-editor "$APP/Contents/MacOS/qs-editor"
  codesign --force --sign "$IDENT" --options runtime --timestamp "$APP"
  AUTHORITY="$(codesign -dvv "$APP" 2>&1 | sed -n 's/^Authority=//p' | head -1)"
  case "$AUTHORITY" in
    "Developer ID Application:"*) ;;
    *)
      echo "production signing needs a Developer ID Application certificate, got: ${AUTHORITY:-none}" >&2
      exit 1
      ;;
  esac
  echo "signed with '$IDENT'"
elif codesign --force --deep --sign "$IDENT" --options runtime "$APP" 2>/dev/null; then
  echo "signed with '$IDENT'"
else
  echo "identity '$IDENT' unavailable — ad-hoc signing"
  codesign --force --deep --sign - "$APP"
fi
codesign --verify --deep --strict "$APP"
echo "signature verified"

# --- notarize + staple the app, then create the distributable zip -----------
ZIP="$DIST/Quickshort-v$VER-macos.zip"
if [ "$NOTARIZE" -eq 1 ]; then
  NOTARY_TMP="$(mktemp -d)"
  SUBMIT_ZIP="$NOTARY_TMP/Quickshort.zip"
  ditto -c -k --sequesterRsrc --keepParent "$APP" "$SUBMIT_ZIP"
  echo "submitting app zip for notarization"
  xcrun notarytool submit "$SUBMIT_ZIP" \
    --key "$NOTARY_KEY" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER" --wait
  rm -rf "$NOTARY_TMP"
  NOTARY_TMP=""
  xcrun stapler staple "$APP"
  xcrun stapler validate "$APP"
fi
rm -f "$ZIP"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$ZIP"

# --- dmg, then notarize + staple the distribution image ---------------------
DMG="$DIST/Quickshort-v$VER-macos.dmg"
rm -f "$DMG"
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
if command -v create-dmg >/dev/null 2>&1; then
  create-dmg --volname Quickshort --hide-extension "$APP" \
    --app-drop-link 380 205 --icon "$APP" 155 205 \
    --window-size 560 380 "$DMG" "$STAGE" || true
fi
[ -f "$DMG" ] || hdiutil create -volname Quickshort -srcfolder "$STAGE" \
  -ov -format UDZO "$DMG" >/dev/null

if [ "$NOTARIZE" -eq 1 ]; then
  echo "submitting dmg for notarization"
  xcrun notarytool submit "$DMG" \
    --key "$NOTARY_KEY" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER" --wait
  xcrun stapler staple "$DMG"
  xcrun stapler validate "$DMG"
fi

# --- checksums ---------------------------------------------------------------
( cd "$DIST" && shasum -a 256 "Quickshort-v$VER-macos.zip" > "Quickshort-v$VER-macos.zip.sha256" \
  && shasum -a 256 "Quickshort-v$VER-macos.dmg" > "Quickshort-v$VER-macos.dmg.sha256" )

echo "---"
ls -lh "$DIST" | tail -4
echo "release artifacts ready in $DIST"
