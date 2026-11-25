#!/bin/sh
# Linux packaging: .deb (dpkg-deb, no extra tools) + .AppImage (linuxdeploy
# with the gtk plugin so tray-icon's libayatana dependency is bundled).
# usage: packaging/linux/package.sh [version]
set -e
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

VER="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' crates/qs-app/Cargo.toml | head -1)}"
BIN="target/release/quickshort"
EDITOR_BIN="target/release/qs-editor"
DIST="$ROOT/dist"
mkdir -p "$DIST"

cargo build --locked --release -p qs-app -p qs-editor

# --- .deb ------------------------------------------------------------------
PKGDIR="$(mktemp -d)/quickshort_${VER}_amd64"
mkdir -p "$PKGDIR/DEBIAN" "$PKGDIR/usr/bin" \
  "$PKGDIR/usr/share/applications" "$PKGDIR/usr/share/icons/hicolor/1024x1024/apps"
cp "$BIN" "$EDITOR_BIN" "$PKGDIR/usr/bin/"
cp packaging/linux/quickshort.desktop "$PKGDIR/usr/share/applications/quickshort.desktop"
cp docs/assets/icon.png "$PKGDIR/usr/share/icons/hicolor/1024x1024/apps/quickshort.png"
# ffmpeg is a Recommends not a Depends: recording (⌘R) needs it, everything
# else works without — apt installs it anyway unless --no-install-recommends
cat > "$PKGDIR/DEBIAN/control" <<EOF
Package: quickshort
Version: $VER
Section: utils
Priority: optional
Architecture: amd64
Maintainer: 2tsoftwareapp <noreply@github.com>
Depends: libgtk-3-0, libgtk-layer-shell0, libayatana-appindicator3-1, libpipewire-0.3-0, libxkbcommon-x11-0, libegl1, libgl1, tesseract-ocr, tesseract-ocr-eng, wl-clipboard, libnotify-bin, openssh-client
Recommends: tesseract-ocr-jpn, tesseract-ocr-vie, ffmpeg
Description: Region screenshot tool with annotations and native OCR
 A pure-Rust desktop utility: multi-monitor region capture, annotation
 layer, OS-native OCR, tray resident.
EOF
dpkg-deb --build "$PKGDIR" "$DIST/quickshort_amd64.deb"

# --- .AppImage ---------------------------------------------------------------
TOOLS="$(mktemp -d)"
curl -sL -o "$TOOLS/linuxdeploy.AppImage" \
  "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage"
# linuxdeploy discovers plugins as executables named linuxdeploy-plugin-<name>
# on PATH — the file must keep this exact name, not a .sh suffix.
GTK_PLUGIN_COMMIT="7a3fbc31a9e5075073ff8790f26effbac5f84453"
curl -sL -o "$TOOLS/linuxdeploy-plugin-gtk" \
  "https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/$GTK_PLUGIN_COMMIT/linuxdeploy-plugin-gtk.sh"
# gdk-pixbuf 2.44+ compiles core loaders into the library, so modern Ubuntu
# images may not create the external loader directory this plugin assumes.
# Mirror the upstream fix direction: skip loader bundling when it is absent.
awk '
$0 == "copy_lib_tree \"$gdk_pixbuf_binarydir\" \"$APPDIR/\"" {
  print "if [ -d \"$gdk_pixbuf_binarydir\" ]; then"
  print
  next
}
$0 == "sed -i \"s|$gdk_pixbuf_moduledir/||g\" \"$APPDIR/${gdk_pixbuf_cache_file/$LD_GTK_LIBRARY_PATH//usr/lib}\"" {
  print
  print "else"
  print "  echo \"WARNING: $gdk_pixbuf_binarydir not found, not bundling gdk-pixbuf loaders\""
  print "fi"
  next
}
{ print }
' "$TOOLS/linuxdeploy-plugin-gtk" > "$TOOLS/linuxdeploy-plugin-gtk.patched"
mv "$TOOLS/linuxdeploy-plugin-gtk.patched" "$TOOLS/linuxdeploy-plugin-gtk"
chmod +x "$TOOLS/linuxdeploy.AppImage" "$TOOLS/linuxdeploy-plugin-gtk"
export PATH="$TOOLS:$PATH"

APPDIR="$(mktemp -d)/AppDir"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" \
  "$APPDIR/usr/share/icons/hicolor/1024x1024/apps"
cp packaging/linux/quickshort.desktop "$APPDIR/usr/share/applications/quickshort.desktop"
cp docs/assets/icon.png "$APPDIR/usr/share/icons/hicolor/1024x1024/apps/quickshort.png"
# linuxdeploy wants the desktop file + icon at the appdir root too
cp packaging/linux/quickshort.desktop "$APPDIR/quickshort.desktop"
cp docs/assets/icon.png "$APPDIR/quickshort.png"

# xkbcommon-dl dlopens libxkbcommon-x11 at runtime for X11 keymaps —
# no DT_NEEDED for linuxdeploy to trace, so it must land by hand
mkdir -p "$APPDIR/usr/lib/x86_64-linux-gnu"
LIBXKB="$(ldconfig -p | awk '/libxkbcommon-x11\.so\.0/ {print $NF; exit}')"
[ -n "$LIBXKB" ] && cp "$LIBXKB" "$APPDIR/usr/lib/x86_64-linux-gnu/"

# AppImages can't self-exec on FUSE-less hosts without this env; CI has none
export APPIMAGE_EXTRACT_AND_RUN=1
# linuxdeploy continuous still carries an old binutils strip which rejects
# .relr.dyn sections used by libraries on current Ubuntu images.
export NO_STRIP=1
export LDAI_OUTPUT="$DIST/Quickshort-Linux.AppImage"
export OUTPUT="$DIST/Quickshort-Linux.AppImage"
rm -f "$OUTPUT" "$OUTPUT.zsync"
# embed gh-releases-zsync update info — AppImageUpdate can then self-update
# from GitHub Releases (the .zsync it generates lands in dist/ alongside)
export UPDATE_INFORMATION="gh-releases-zsync|2tsoftwareapp|quickshort|latest|Quickshort-Linux.AppImage.zsync"
# newer linuxdeploy builds dropped the -u flag — UPDATE_INFORMATION env var
# alone is what appimagetool reads for the embedded update info
if ! "$TOOLS/linuxdeploy.AppImage" \
  --executable "$BIN" --executable "$EDITOR_BIN" \
  --appdir "$APPDIR" --plugin gtk \
  --output appimage > "$TOOLS/linuxdeploy.log" 2>&1; then
    echo "linuxdeploy failed to build the AppImage" >&2
    tail -60 "$TOOLS/linuxdeploy.log" >&2 || true
    exit 1
fi
test -x "$APPDIR/usr/bin/quickshort"
test -x "$APPDIR/usr/bin/qs-editor"
test -f "$DIST/Quickshort-Linux.AppImage"
ls -lh "$DIST"/* 2>/dev/null || true
echo "linux artifacts ready in $DIST"
