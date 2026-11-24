#!/bin/sh
# Build the offline setup wizard from the existing AppImage.
# usage: packaging/linux/build-installer.sh [version] [appimage]
set -eu
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIST="$ROOT/dist"
VER="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/crates/qs-app/Cargo.toml" | head -1)}"
APPIMAGE="${2:-$DIST/Quickshort-Linux.AppImage}"
IFW="${QT_IFW_BIN:-}"
if [ -z "$IFW" ]; then
  CREATOR="$(command -v binarycreator)" || {
    echo "Install Qt Installer Framework or set QT_IFW_BIN to its bin directory." >&2
    exit 1
  }
  IFW="$(dirname "$CREATOR")"
fi
test -f "$APPIMAGE"
printf '%s\n' "$VER" | grep -Eq '^[0-9]+([.-][0-9]+)*$' || {
  echo "Invalid Qt installer version: $VER" >&2
  exit 1
}

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
COMPONENT="$STAGE/packages/ai.quickshort.app"
mkdir -p "$DIST" "$STAGE/config" "$COMPONENT/meta" "$COMPONENT/data"
install -m 755 "$APPIMAGE" "$COMPONENT/data/Quickshort.AppImage"
cp "$ROOT/assets/icon.png" "$COMPONENT/data/quickshort.png"
cp "$ROOT/packaging/linux/installer/logo.png" "$STAGE/config/"
cp "$STAGE/config/logo.png" "$STAGE/config/window-icon.png"
sed "s/@VERSION@/$VER/g" "$ROOT/packaging/linux/installer/config.xml" > "$STAGE/config/config.xml"
sed -e "s/@VERSION@/$VER/g" -e "s/@RELEASE_DATE@/$(date -u +%F)/g" \
  "$ROOT/packaging/linux/installer/package.xml" > "$COMPONENT/meta/package.xml"
cp "$ROOT/packaging/linux/installer/installscript.qs" "$COMPONENT/meta/"

NAME="Quickshort-Linux-Setup.run"
QT_QPA_PLATFORM=offscreen "$IFW/binarycreator" --offline-only \
  -t "$IFW/installerbase" -c "$STAGE/config/config.xml" -p "$STAGE/packages" "$STAGE/$NAME"
test -x "$STAGE/$NAME"
mv "$STAGE/$NAME" "$DIST/$NAME"
(cd "$DIST" && sha256sum "$NAME" > "$NAME.sha256")
echo "$DIST/$NAME"
