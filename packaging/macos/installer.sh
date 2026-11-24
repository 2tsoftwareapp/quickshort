#!/bin/sh
# Package the built Quickshort.app. Set PKG_SIGNING_IDENTITY and
# NOTARY_KEYCHAIN_PROFILE together to produce a notarized distribution.
set -eu
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
APP="$ROOT/Quickshort.app"
VER="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$APP/Contents/Info.plist")"
MIN_OS="$(/usr/libexec/PlistBuddy -c 'Print LSMinimumSystemVersion' "$APP/Contents/Info.plist")"
printf '%s\n' "$VER" | grep -Eq '^[0-9]+(\.[0-9]+)*$' || {
  echo "Invalid macOS installer version: $VER" >&2
  exit 1
}
DIST="$ROOT/dist"
mkdir -p "$DIST"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/payload" "$STAGE/resources"
for BIN in quickshort qs-editor; do
  test -x "$APP/Contents/MacOS/$BIN"
done
ARCHS="$(lipo -archs "$APP/Contents/MacOS/quickshort")"
if [ "$ARCHS" != "$(lipo -archs "$APP/Contents/MacOS/qs-editor")" ]; then
  echo "Quickshort and qs-editor have different architectures." >&2
  exit 1
fi
HOST_ARCHS="$(printf '%s' "$ARCHS" | tr ' ' ',')"
sed -e "s/@HOST_ARCHS@/$HOST_ARCHS/g" -e "s/@MIN_OS@/$MIN_OS/g" \
  "$ROOT/packaging/macos/installer.xml" > "$STAGE/Distribution.xml"
codesign --verify --deep --strict "$APP"
ditto "$APP" "$STAGE/payload/Quickshort.app"
sed "s/@VERSION@/$VER/g" "$ROOT/packaging/macos/welcome.rtf" > "$STAGE/resources/welcome.rtf"
cp "$ROOT/packaging/macos/conclusion.rtf" "$STAGE/resources/"
sips -Z 72 "$ROOT/assets/icon.png" --out "$STAGE/resources/quickshort.png" >/dev/null

pkgbuild --root "$STAGE/payload" --install-location /Applications \
  --component-plist "$ROOT/packaging/macos/installer-components.plist" \
  --identifier ai.quickshort.installer --version "$VER" "$STAGE/QuickshortPayload.pkg"

IDENT="${PKG_SIGNING_IDENTITY:-}"
NOTARY_PROFILE="${NOTARY_KEYCHAIN_PROFILE:-}"
if [ -z "$IDENT" ] && [ -z "$NOTARY_PROFILE" ]; then
  NAME="Quickshort-v$VER-macos-preview.pkg"
  set --
elif [ -n "$IDENT" ] && [ -n "$NOTARY_PROFILE" ]; then
  codesign -dvv "$APP" 2>&1 | grep -q '^Authority=Developer ID Application:'
  NAME="Quickshort-v$VER-macos.pkg"
  set -- --sign "$IDENT"
else
  echo "Set both PKG_SIGNING_IDENTITY and NOTARY_KEYCHAIN_PROFILE for distribution." >&2
  exit 1
fi
productbuild --distribution "$STAGE/Distribution.xml" \
  --package-path "$STAGE" --resources "$STAGE/resources" "$@" "$STAGE/$NAME"
if [ -n "$IDENT" ]; then
  xcrun notarytool submit "$STAGE/$NAME" --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple "$STAGE/$NAME"
  xcrun stapler validate "$STAGE/$NAME"
  pkgutil --check-signature "$STAGE/$NAME"
fi
mv "$STAGE/$NAME" "$DIST/$NAME"
(cd "$DIST" && shasum -a 256 "$NAME" > "$NAME.sha256")
echo "$DIST/$NAME"
