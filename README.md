<div align="center">

# Quickshort

**Region screenshots with annotations and OCR — native Rust, all the way down.**

[![Release](https://img.shields.io/github/v/release/2tsoftwareapp/quickshort?include_prereleases&label=release)](https://github.com/2tsoftwareapp/quickshort/releases)
[![Build](https://github.com/2tsoftwareapp/quickshort/actions/workflows/release.yml/badge.svg)](https://github.com/2tsoftwareapp/quickshort/actions/workflows/release.yml)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
![Platform](https://img.shields.io/badge/platform-macOS%20%C2%B7%20Linux%20%C2%B7%20Windows-blue)

[Features](#features) · [Install](#install) · [Usage](#usage) · [Architecture](#architecture) · [Privacy](#privacy)

</div>

## Why Quickshort

Most screenshot tools force a trade: quick but dumb (print-screen into an
editor you didn't ask for), or capable but modal (capture → open → annotate →
export → find the file). Quickshort collapses it to one flow:
**capture → quick look → optional edit**. Press the hotkey, drag a region —
even across monitors — mark it up right on the overlay, press **Done**, and
the PNG is saved *and* on your clipboard. If the shot needs real work, **Edit**
reopens it in the editor with every annotation still a live layer.

No Electron, no runtime, no model downloads.

## Features

### Capture

| | |
|---|---|
| **Region capture** | One flow, no mode picker. `Enter` saves and copies; `E` continues in the editor; `Esc` cancels |
| **Multi-monitor selection** | Drag across monitor boundaries — per-monitor crops stitched through a virtual coordinate space |
| **Full physical resolution** | Retina and mixed-DPI captured and stitched at native framebuffer pixels, never downsampled |
| **Scrolling capture** | `⌘G` re-grabs while you scroll, stitching frames into one tall image — docs, threads, chat logs |
| **Screen recording** | `⌘R` records the region to mp4 at 15fps — PipeWire push on GNOME/portal Wayland, re-capture polling elsewhere; `ffmpeg` encodes on a worker, dropping frames rather than shortening the clip |
| **Native Wayland** | `zwlr_screencopy` in-process on Hyprland/Sway/Niri/River/labwc — instant, no consent dialog; mutter ScreenCast on GNOME; the portal is the last resort |

### Markup

| | |
|---|---|
| **Annotations** | Rect, arrow, numbered step badges (①②③), text, blur on the main row; select, pen, ellipse, highlight behind **⋯** |
| **Text tool** | Real font sizes (12–48), bold toggle, a proper editing box (selection, caret, `⌘A`), Vietnamese IME input |
| **Editable blocks** | Shapes and text stay live objects — Select picks, drags, deletes; the overlay re-arms your last tool |
| **Output-quality rendering** | Undo/redo, stroke sizes, color palette — rendered at output resolution, confined to the captured region |

### Workflow

| | |
|---|---|
| **Editor** | `.qsdoc` documents with layers, Inspector (style, Canvas, Align), in-canvas Library, pinned Find bar with word-level OCR highlights, Export (PNG/JPEG/WebP, Copy, Print), undoable Flatten/Trash |
| **Evidence sessions** | For manual testers: each Done saves `TC-001_01.png`, `TC-001_02.png`…; *Next Test Case* stitches the case into one tall `TC-001_all.png` |
| **Link sharing** | One click uploads WebP (~half a PNG) to a Cloudflare Worker and copies an unguessable URL — kept 30 days after its last view, no account |
| **Google Drive** | Opt-in alternative: images land in a `Quickshort` folder in *your* Drive (`drive.file` scope only), share again to revoke |
| **OCR** | Vision on macOS, `Windows.Media.Ocr` on Windows, Tesseract on Linux with script auto-detection — nothing ships in the binary |
| **Pin & beautify** | Float a capture as an always-on-top reference, or mount it on a soft gradient backdrop with rounded corners and a drop shadow |

### Desktop integration

| | |
|---|---|
| **Resident tray app** | Menu-bar icon, global hotkey, launch at login, recent captures, auto-update |
| **Per-desktop hotkeys** | GNOME keybinding, swaymsg, hyprctl, GlobalShortcuts portal (KDE, probed on other Wayland), `XGrabKey` on X11 |
| **Notifications** | Native `org.freedesktop.Notifications` with an Edit action — honors the desktop's position, timeout and Do Not Disturb |
| **Settings** | Capture, General, Shortcuts, Appearance, Storage, About — rebindable keys, accent color, retention policy, one-click update install |
| **One-shot CLI** | `quickshort --capture` fits scripts and WM keybinds; relays to the resident app over a locked-down unix socket |
| **Signed updates** | Release assets carry `<asset>.sig` ed25519 sidecars, verified against a public key embedded in the binary |

The look: neutral greys, dark-mode accent colors, concentric corner radii,
a capsule capture HUD, floating Editor toolbar groups and inset glass
panels — SF Pro on macOS, Be Vietnam Pro elsewhere.

## Install

### macOS

Grab `Quickshort-v*-macos.dmg` from
[Releases](https://github.com/2tsoftwareapp/quickshort/releases), open it,
drag **Quickshort** into Applications. Or via Homebrew:

```bash
brew tap 2tsoftwareapp/tap
brew install --cask quickshort
```

> Production macOS builds are Developer ID signed and notarized. macOS still
> shows the normal first-launch confirmation for a downloaded app, but no
> right-click Open override or manual quarantine removal is needed.

Required permissions (System Settings → Privacy & Security): **Screen
Recording** (capturing), **Accessibility** (global hotkey).

Updates check GitHub Releases daily — tray menu → **Check for Updates…**, or
`brew upgrade --cask quickshort`. Optional `brew install ffmpeg` enables
recording; without it everything else works, Record fails with a toast.

### Linux

**Flatpak:**

```bash
flatpak install flathub com.truongsoftware.quickshort
```

Everything sandboxed except what the app needs — tesseract, ffmpeg and
wl-copy are bundled. Global hotkey on Wayland via the KDE GlobalShortcuts
portal.

**Arch / Manjaro (AUR):**

```bash
yay -S quickshort-bin
```

**AppImage** — `Quickshort-Linux.AppImage` from
[Releases](https://github.com/2tsoftwareapp/quickshort/releases), `chmod +x`,
run. Tray dependencies bundled.

**Debian / Ubuntu:**

```bash
sudo apt install ./quickshort_amd64.deb
```

Installs to `/usr/bin` with a `.desktop` entry and icon, and pulls `ffmpeg`
via Recommends. Upgrading from the v0.4.0 package: install the new `.deb`
the same way so APT picks up the editor's dependencies.

**From source** — `./install.sh` handles Debian/Ubuntu system deps
(pipewire/spa/clang dev packages, `tesseract-ocr` + language packs,
`wl-clipboard`, `libnotify-bin`; extra OCR languages via `QS_OCR_LANGS`),
builds release → `~/.local/bin`, registers a GNOME shortcut
(`Ctrl+Alt+Q`). Elsewhere the resident app registers the hotkey itself on
KDE, sway, Hyprland and portal-capable desktops — or bind
`quickshort --capture` yourself.

### Windows

`Quickshort-Windows-Setup.exe` from
[Releases](https://github.com/2tsoftwareapp/quickshort/releases) — NSIS:
Program Files install, Start Menu shortcut, uninstaller. Unsigned, so
SmartScreen may warn (**More info → Run anyway**). Runs as a tray app, no
console window. `winget install ffmpeg` for recording.

### Install with an agent

Paste this into your AI agent (Claude Code, Cursor, Devin…) and let it do
the install:

```text
Install Quickshort (github.com/2tsoftwareapp/quickshort) on this machine.
Pick the right channel for my OS and install it:

- macOS: download the latest Quickshort-v*-macos.dmg from GitHub Releases,
  mount it, copy Quickshort.app to /Applications. Or
  `brew tap 2tsoftwareapp/tap && brew install --cask quickshort`.
  Production builds are signed and notarized — remind me to grant Screen
  Recording and Accessibility permissions in System Settings.
- Debian/Ubuntu: download quickshort_amd64.deb and `sudo apt install ./…`.
- Other Linux: `flatpak install flathub com.truongsoftware.quickshort`,
  or the Quickshort-Linux.AppImage (chmod +x). Make sure tesseract-ocr
  (+ my language packs), wl-clipboard, ffmpeg and libnotify are installed.
- Arch: `yay -S quickshort-bin`.
- Windows: download Quickshort-Windows-Setup.exe and run it. Unsigned —
  SmartScreen → More info → Run anyway.

When done, verify the install (`quickshort --version` or launch the tray
app) and tell me the capture hotkey (Cmd/Ctrl+Shift+S, or PrintScreen on
Windows).
```

## Usage

```bash
quickshort                              # resident tray app (default)
quickshort --capture                    # one-shot: select -> action -> exit
quickshort --capture --delay 2          # wait first (menus, tooltips)
quickshort --capture --out ~/shots/a.png
quickshort --capture-full               # save the whole screen, no selection
```

### Shortcuts

| Context | Shortcut | Action |
|---|---|---|
| Anywhere | `⌘⇧S` macOS/Linux · `PrintScreen` Windows | Start a selection |
| Anywhere | `⌘⇧F` (`hotkey_full`) | Save the whole screen — no overlay |
| Selection made | `Enter` or `⌘C` | Done — copy to clipboard and save |
| Selection made | `R` `A` `N` `T` `B` · `V` `P` `O` `H` | Rect, Arrow, Step, Text, Blur · Select, Pen, Ellipse, Highlight |
| Selection made | `E` or `⌘E` | Open in the editor, annotations still editable |
| Selection made | `⌘S` / `⌘T` / `⌘K` | Save only / OCR / share link → clipboard |
| Selection made | `⌘G` / `⌘R` | Scrolling capture / record region to mp4 |
| Selection made | `⌘Z` / `⌘⇧Z` | Undo / redo an edit |
| Block selected | `Delete` / `⌫` / `Esc` | Remove block / deselect |
| Text draft | `Enter` / `Esc` | Commit / discard |
| Text draft | `⌘A`, `⇧`+arrows, `←→`, `⌫`/`⌦` | Select, move caret, delete |
| Selection made | `Esc` | Cancel |

All action keys are rebindable. The selection toolbar docks to whichever
edge of the region has room; **⋯** opens Select, Pen, Ellipse, Highlight,
Scroll, Record, OCR and Share. Saved files go to
`~/Pictures/quickshort/quickshort-<ts>.png`.

### Configuration

`~/.config/quickshort/config.toml`:

```toml
hotkey = "CmdOrCtrl+Shift+S"   # "PrintScreen" is the Windows default —
                               # falls back to Ctrl+Shift+S if the shell owns it
hotkey_full = "CmdOrCtrl+Shift+F" # whole-screen chord; "" disables
# save_dir = "~/Pictures/quickshort"
launch_at_login = false
auto_update = true
recent_max = 8                 # entries in the tray menu
accent = "blue"                # blue/purple/pink/orange/green/gray
copy_after_save = true         # also put the PNG on the clipboard
share_to_drive = false         # Share uploads to your Google Drive
                               # (needs a build with QS_GDRIVE_CLIENT_ID)
theme = "light"                # light/dark
ocr_lang = "auto"              # Linux tesseract: detect the script,
                               # or pin e.g. "eng+jpn"
keep_days = 0                  # auto-delete captures older than N days
key_save = "s"                 # overlay ⌘+key actions
key_copy = "c"                 # Done: copy + save (Enter does the same)
key_ocr = "t"
key_share = "k"
key_scroll = "g"               # scrolling capture
key_record = "r"               # screen recording (needs ffmpeg)
key_undo = "z"                 # ⌘z undo, ⌘⇧z redo
key_edit = "e"                 # open the capture in the editor
popup_pos = "top-right"        # toast/card/REC dock corner
```

## Architecture

```
capture every monitor into RAM
  → dimmed overlay windows (one per monitor)
  → selection in global virtual coordinates
  → map through each monitor's origin + DPI scale
  → stitch per-monitor crops on a worker thread
```

### Crates

| Crate | Role |
|---|---|
| `qs-core` | geometry + stitching, pure |
| `qs-capture` | per-OS backends behind `ScreenCapture` |
| `qs-overlay` | winit + softbuffer, SDF-drawn toolbar, annotation layers, embedded Inter text |
| `qs-ocr` | native text extraction, line + word geometry |
| `qs-doc` | `.qsdoc` vector document schema |
| `qs-edit` | headless editing engine, undo history |
| `qs-library` | capture album storage + provenance |
| `qs-icons` | embedded icon assets |
| `qs-render` | rasterization and effects (`tiny-skia`) |
| `qs-editor` | annotation editor (`eframe`/`egui`) |
| `qs-theme` | shared palette, accent, radius, type and spacing tokens |
| `qs-app` | CLI + resident tray app, toasts, evidence sessions, share/Drive, updates |

### Capture backends

| Platform | Backend chain (best first) |
|---|---|
| macOS 14+ | `SCScreenshotManager` per-display one-shot at framebuffer pixels |
| Windows | GDI BitBlt per monitor, `PER_MONITOR_AWARE_V2` |
| GNOME Wayland | `org.gnome.Mutter.ScreenCast` → PipeWire shm frames, in-process |
| Hyprland | `zwlr_screencopy_manager_v1` in-process → `grim`+`hyprctl` |
| Other wlroots (Sway, Niri, River, labwc…) | `zwlr_screencopy_manager_v1` in-process — no dialog |
| Other Wayland (KDE…) | `org.freedesktop.portal.ScreenCast` → PipeWire |
| X11 | RandR layout + `GetImage` |

Each Wayland session gets an ordered chain; cheap protocol probes drop
backends that can't run, and a grab failure swaps in the next one. On GNOME
there is no portal dialog and no subprocess — mutter's private ScreenCast
API feeds PipeWire shared-memory frames, one connection multiplexing all
monitors. wlroots compositors expose `zwlr_screencopy` themselves, so
capture is likewise instant and dialog-free. The portal is the last
resort: one consent dialog, and on `persist_mode` portals (KDE 5.24+) a
stored restore token makes later captures dialog-free.
`QS_CAPTURE=portal|mutter|hyprland|wlr|x11` forces a chain.

OCR on Linux runs the system `tesseract` binary: `auto` mode runs an OSD
pass to pick the script's language packs (missing packs fail loud with the
exact `apt` package to install), and CJK output gets its artificial word
spacing collapsed. Wayland capture pokes a 1x1 XWayland window mid-screen
because mutter only produces frames on damage; MemFd/DmaBuf buffers are
mmap'ed directly.

## Development

```bash
cargo test                                       # unit tests
cargo test -p qs-capture -- --ignored            # real capture (needs Wayland/macOS)
cargo run -p qs-app                              # resident app
cargo run -p qs-app -- --capture                 # one-shot
QUICKSHORT_LOG=debug cargo run -p qs-app         # trace routing, IPC, etc.
```

`packaging/macos/release.sh` runs the full pipeline — release build
(universal when both rustup targets exist), `Quickshort.app` bundle,
code signing, zip + dmg + sha256 into `dist/`. Production workflow builds
use a Developer ID Application certificate and notarize/staple both macOS
artifacts; local builds can still use a self-signed or ad-hoc identity when
notary credentials are absent. Tagging `v*` triggers the workflow, which
does the same and drafts a GitHub Release; the publish job signs every
asset with ed25519 when `QS_ED25519_KEY` is provisioned.

## Privacy

Everything runs locally — captures, OCR and settings never leave your
machine. The only outbound traffic: a share upload you trigger (kept 30
days after its last view on a Cloudflare Worker, or your own Drive), an
opt-in bug report (a public
GitHub issue), and a daily release check. No analytics, no accounts.
Full policy: [docs/privacy.html](docs/privacy.html) ·
[site](https://2tsoftwareapp.github.io/quickshort/privacy.html).

## Support

Free and open source, built in spare time. If Quickshort saves you time:

- **[GitHub Sponsors](https://github.com/sponsors/2tsoftwareapp)** — international, zero fees on personal sponsorships
- **VietQR** — MB Bank `0969069035` · ĐÀO QUANG TRƯỞNG (scan with any VN banking app)

## Known limits

- Mutter inhibits screencast while the screen is locked
- Wayland needs one overlay window per monitor (no global window
  positioning); pointer grab carries the selection across monitors
- No global-hotkey API on Wayland — the app registers through the
  compositor (gsettings, swaymsg, hyprctl, probed GlobalShortcuts portal);
  elsewhere bind `quickshort --capture` yourself. The tray needs an
  AppIndicator extension on GNOME
- Upgrading from a pre-Developer-ID self-signed macOS build can require
  re-granting Screen Recording and Accessibility once because macOS treats
  the new signing identity differently; later Developer ID updates keep the
  same stable identity
