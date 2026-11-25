# Quickshort

**Region screenshots with annotations and OCR — native Rust, all the way down.**

[![Release](https://img.shields.io/github/v/release/2tsoftwareapp/quickshort?include_prereleases&label=release)](https://github.com/2tsoftwareapp/quickshort/releases)
[![Build](https://github.com/2tsoftwareapp/quickshort/actions/workflows/release.yml/badge.svg)](https://github.com/2tsoftwareapp/quickshort/actions/workflows/release.yml)
![Platform](https://img.shields.io/badge/platform-macOS%20%C2%B7%20Linux%20%C2%B7%20Windows-blue)

One flow: **capture → quick look → (optional) edit**. Press the global hotkey,
drag a region — even across monitors — mark it up right on the overlay, then
press **Done**: the PNG is saved *and* on your clipboard. Need more? **Edit**
opens the same capture in the editor with every annotation still an editable
layer. No Electron, no runtime, no model downloads.

## Features

### Capture

| Capability | Details |
|---|---|
| Region capture | One flow, no mode picker. Done (`Enter`) saves + copies; Edit (`E`) continues in the editor; `Esc` cancels |
| Multi-monitor selection | Drag across monitor boundaries; per-monitor crops stitched through a virtual coordinate space |
| Full physical resolution | Retina and mixed-DPI displays captured and stitched at native framebuffer pixels — never downsampled |
| Scrolling capture | `⌘G` re-grabs the region while you scroll and stitches frames into one tall image — docs, threads, chat logs |
| Screen recording | `⌘R` records the region to mp4 at 15fps via PipeWire push (GNOME/portal Wayland) or re-capture polling; encodes through `ffmpeg`, slow encodes drop frames instead of shortening the clip |
| Native Wayland capture | `zwlr_screencopy` in-process on Hyprland/Sway/Niri/River/labwc — instant, no consent dialog; mutter ScreenCast on GNOME; portal only as last resort |

### Markup

| Capability | Details |
|---|---|
| Annotations | Rect, arrow, numbered step badges (①②③), text, blur on the main row; select, pen, ellipse, highlight behind **⋯** |
| Text tool | Real font sizes (12–48), bold toggle, proper editing box (arrows, selection, `⌘A`), Vietnamese IME input |
| Editable blocks | Shapes and text stay live objects — the Select tool picks, drags, deletes; the overlay re-arms your last tool |
| Rendering | Undo/redo, stroke sizes, color palette — rendered at output resolution, confined to the captured region |

### Workflow

| Capability | Details |
|---|---|
| Editor | `.qsdoc` documents with layers, Inspector (style, Canvas, Align), in-canvas Library, pinned Find bar with word-level OCR highlights, Export dropdown (PNG/JPEG/WebP, Copy, Print), undoable Flatten/Trash |
| Evidence sessions | For manual testers: tray → *Start Evidence Session* — each Done saves `TC-001_01.png`, `TC-001_02.png`…; *Next Test Case* stitches the case into one tall `TC-001_all.png` |
| Link sharing | One click uploads WebP (q95, ~half a PNG) to a Cloudflare Worker and copies an unguessable URL that expires in 3 days — no account, no server to keep alive |
| Google Drive | Opt-in alternative: images land in a `Quickshort` folder in *your* Drive (`drive.file` scope only), share again to revoke |
| OCR | Vision on macOS, `Windows.Media.Ocr` on Windows, Tesseract on Linux with script auto-detection; nothing ships in the binary |
| Pin & beautify | Pin floats a capture as an always-on-top reference; Beautify mounts it on a soft gradient backdrop with rounded corners and a drop shadow |

### Desktop integration

| Capability | Details |
|---|---|
| Resident tray app | Menu-bar icon, global hotkey, launch at login, recent captures, auto-update |
| Global hotkeys, per desktop | GNOME keybinding, swaymsg, hyprctl, GlobalShortcuts portal (KDE + probed on other Wayland), `XGrabKey` on X11 |
| Notifications | Native `org.freedesktop.Notifications` with an Edit action — follows the desktop's position/timeout/DND settings |
| Settings window | Paged preferences: Capture, General, Shortcuts, Appearance, Storage, About — rebindable keys, accent color, retention policy, one-click update install |
| One-shot CLI | `quickshort --capture` fits scripts and window-manager keybinds; relays to the resident app over a locked-down unix socket |
| Signed updates | Release assets carry `<asset>.sig` ed25519 sidecars verified against a public key embedded in the binary |

### Look

Neutral greys, dark-mode accent colors, concentric corner radii, a capsule
capture HUD, floating Editor toolbar groups and inset glass panels — SF Pro on
macOS, Be Vietnam Pro elsewhere.

## Install

### macOS

**DMG** — grab `Quickshort-v*-macos.dmg` from
[Releases](https://github.com/2tsoftwareapp/quickshort/releases), open it, drag
**Quickshort** into Applications.

**Homebrew** (custom tap):

```bash
brew tap truongnat/tap
brew install --cask quickshort
```

> **Unsigned build** — no Developer ID signature or notarization. On first
> launch, right-click the app → **Open** → **Open** once. The cask strips
> quarantine automatically. The app is signed with a stable self-signed
> identity purely so macOS keeps your permission grants across updates.

Required permissions (System Settings → Privacy & Security):

- **Screen Recording** — needed to capture anything
- **Accessibility** — needed for the global hotkey

**Updates** — the app checks GitHub Releases once a day in the background;
the tray item becomes *Update to vX — Restart to Install*. Or: tray menu →
**Check for Updates…**, or `brew upgrade --cask quickshort`. Downloads are
verified against the embedded ed25519 key (see *Signed updates* above).

Optional: `brew install ffmpeg` enables screen recording (`⌘R`); without
it everything else still works — Record fails with a visible toast.

### Linux

**AppImage** — grab `Quickshort-Linux.AppImage` from
[Releases](https://github.com/2tsoftwareapp/quickshort/releases), `chmod +x`,
run. Bundles the tray dependencies (gtk plugin).

**Debian / Ubuntu**:

```bash
sudo apt install ./quickshort_amd64.deb
```

Installs to `/usr/bin` with a `.desktop` entry and icon. When upgrading from
the v0.4.0 Debian package, install the new `.deb` with that command so APT
pulls the editor and its runtime dependencies. The deb pulls `ffmpeg` via
Recommends (screen recording); AppImage users need it on PATH themselves.

**From source** — `./install.sh` installs missing system deps on
Debian/Ubuntu (pipewire/spa/clang dev packages for the build, plus
`tesseract-ocr` + language packs, `wl-clipboard`, `libnotify-bin` at
runtime; extra OCR languages via `QS_OCR_LANGS`), builds release →
`~/.local/bin` and registers a GNOME custom shortcut via `gsettings`
(default `Ctrl+Alt+Q`). On other desktops the installer just drops the
binary — the resident app registers the hotkey itself on KDE Plasma,
sway, Hyprland and portal-capable Wayland desktops; elsewhere, bind
`quickshort --capture` in your compositor settings.

**Notifications** — status messages go through the desktop's notification
service (`org.freedesktop.Notifications` over D-Bus) instead of a
Quickshort-drawn toast window, so they follow the desktop's notification
settings (position, timeout, Do Not Disturb).

### Windows

`Quickshort-Windows-Setup.exe` under
[Releases](https://github.com/2tsoftwareapp/quickshort/releases) — NSIS
installer: Program Files install, Start Menu shortcut, uninstaller entry.
Unsigned, so SmartScreen may warn (**More info → Run anyway**). Runs as a
tray app, no console window. Or build the app and editor from source
(`cargo build --release -p qs-app -p qs-editor`). For screen recording,
install FFmpeg with `winget install ffmpeg`.

## Usage

```bash
quickshort                              # resident tray app (default)
quickshort --capture                    # one-shot: select -> action -> exit
quickshort --capture --delay 2          # wait first (menus, tooltips)
quickshort --capture --out ~/shots/a.png
quickshort --capture-full               # save the whole screen, no selection
```

| Context | Shortcut | Action |
|---|---|---|
| Anywhere | `⌘⇧S` macOS/Linux, `PrintScreen` Windows (configurable) | Start a selection |
| Anywhere | `⌘⇧F` (configurable, `hotkey_full`) | Save the whole screen — no overlay |
| Selection made | `Enter` or `⌘C` (rebindable) | Done — copy to clipboard and save |
| Selection made | `R` `A` `N` `T` `B` · `V` `P` `O` `H` | Rect, Arrow, Step, Text, Blur · Select, Pen, Ellipse, Highlight (same letters as the editor) |
| Selection made | `E` or `⌘E` (rebindable, `key_edit`) | Open in the editor, annotations still editable |
| Selection made | `⌘S` / `⌘T` / `⌘K` (rebindable) | Save only / OCR / share link → clipboard |
| Selection made | `⌘G` / `⌘R` (rebindable) | Scrolling capture / record region to mp4 |
| Selection made | `⌘Z` / `⌘⇧Z` (rebindable key) | Undo / redo an edit |
| Block selected | `Delete` / `⌫` / `Esc` | Remove block / deselect |
| Text draft | `Enter` / `Esc` | Commit / discard |
| Text draft | `⌘A`, `⇧`+arrows, `←→`, `⌫`/`⌦` | Select, move caret, delete |
| Selection made | `Esc` | Cancel |

The tray menu offers Capture Region, Capture Full Screen, Recent Captures,
Open Screenshots Folder, Start Evidence Session (then Next Test Case / End
Evidence Session), Share Links via Google Drive, Disconnect Google Drive,
Check for Updates, Launch at Login, Settings and Quit. Settings also live in `~/.config/quickshort/config.toml`:

```toml
hotkey = "CmdOrCtrl+Shift+S"   # global-hotkey syntax; "PrintScreen" is
                               # the Windows default — falls back to
                               # Ctrl+Shift+S if the shell owns it
hotkey_full = "CmdOrCtrl+Shift+F" # second chord: saves the whole screen;
                               # empty string disables it
# save_dir = "~/Pictures/quickshort"
launch_at_login = false
auto_update = true
recent_max = 8                 # entries in the tray menu
accent = "blue"                # blue/purple/pink/orange/green/gray
copy_after_save = true         # also put the PNG on the clipboard
share_to_drive = false         # Share uploads to your Google Drive
                               # (needs a build with QS_GDRIVE_CLIENT_ID)
theme = "light"                # light/dark
ocr_lang = "auto"              # Linux tesseract: "auto" detects the
                               # script, or pin e.g. "eng+jpn"
keep_days = 0                  # auto-delete captures older than N days
key_save = "s"                 # overlay ⌘+key actions
key_copy = "c"                 # Done: copy + save (Enter does the same)
key_ocr = "t"
key_share = "k"
key_scroll = "g"               # scrolling capture
key_record = "r"               # screen recording (needs ffmpeg on PATH)
key_undo = "z"                 # ⌘z undo, ⌘⇧z redo
key_edit = "e"                 # open the capture in the editor
popup_pos = "top-right"        # toast/card/REC dock corner: top-left,
                               # top-center, top-right, bottom-* too
```

The selection toolbar docks to whichever edge of the region has room.
Its main row is Rect, Arrow, Step, Text, Blur · Undo, Redo · **⋯**,
**Edit** and **Done** (highlighted): Enter or a click copies the capture
and saves it, so nothing exists only in memory. **⋯** opens Select, Pen,
Ellipse, Highlight, Scroll, Record, OCR and Share; **Edit** hands the
capture, annotations still editable, to the editor. Before you drag, a
hint reads *Drag to select an area · Esc to cancel*. Clicking away
starts a new selection; Esc or a click without dragging cancels. Saved
files go to `~/Pictures/quickshort/quickshort-<ts>.png`.

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
| `qs-overlay` | winit + softbuffer, SDF-drawn toolbar, annotation layers, text via embedded Inter (ab_glyph) |
| `qs-ocr` | native text extraction, line + word geometry |
| `qs-doc` | `.qsdoc` vector document schema |
| `qs-edit` | headless editing engine, undo history |
| `qs-library` | capture album storage + provenance |
| `qs-icons` | embedded icon assets |
| `qs-render` | rasterization and effects (`tiny-skia`) |
| `qs-editor` | annotation editor (`eframe`/`egui`) |
| `qs-theme` | shared palette, accent, radius, type and spacing tokens |
| `qs-app` | CLI + resident tray app (`tray-icon`/`muda`/`global-hotkey`), toasts, evidence sessions, share/Drive, updates |

### Capture backends

| Platform | Backend chain (best first) |
|---|---|
| macOS 14+ | `SCScreenshotManager` per-display one-shot at framebuffer pixels |
| Windows | GDI BitBlt per monitor, `PER_MONITOR_AWARE_V2` |
| GNOME Wayland | `org.gnome.Mutter.ScreenCast` → PipeWire shm frames, in-process |
| Hyprland | `zwlr_screencopy_manager_v1` in-process → `grim`+`hyprctl` fallback |
| Other wlroots (Sway, Niri, River, labwc…) | `zwlr_screencopy_manager_v1` in-process — no dialog |
| Other Wayland (KDE…) | `org.freedesktop.portal.ScreenCast` → PipeWire |
| X11 | RandR layout + `GetImage` |

Each Wayland session gets an ordered chain of candidate backends; cheap
protocol probes drop ones that can't run, and a grab failure swaps in the
next candidate. On GNOME Wayland there is no portal dialog and no
subprocess — the backend talks to mutter's private ScreenCast API (the
same one the GNOME portal uses internally), then reads shared-memory
buffers from PipeWire, one connection multiplexing all monitor streams.
wlroots compositors expose `zwlr_screencopy` themselves, so capture there
is likewise instant and dialog-free. The portal is the last resort:
the compositor shows a consent dialog once, and on portals supporting
`persist_mode` (KDE 5.24+) a stored restore token makes later captures
dialog-free. `QS_CAPTURE=portal|mutter|hyprland|wlr|x11` forces a chain.

OCR on Linux runs the system `tesseract` binary: in `auto` mode an OSD
pass detects the script and maps it to the installed language packs
(missing packs fail loud with the exact `apt` package to install), and
CJK output gets its artificial word spacing collapsed. Wayland capture
pokes a 1x1 XWayland window mid-screen because mutter only produces
frames on damage, and MemFd/DmaBuf buffers are mmap'ed directly.

## Development

```bash
cargo test                                       # unit tests
cargo test -p qs-capture -- --ignored            # real capture (needs Wayland/macOS)
cargo run -p qs-app                              # resident app
cargo run -p qs-app -- --capture                 # one-shot
QUICKSHORT_LOG=debug cargo run -p qs-app         # trace capture routing, IPC, etc.
```

Release packaging: `packaging/macos/release.sh` runs the full pipeline —
release build (universal when both rustup targets exist), `Quickshort.app`
bundle, self-signed identity (`packaging/macos/gen_signing_cert.sh`), zip +
dmg + sha256 into `dist/`. Tagging `v*` triggers the GitHub workflow, which
does the same and publishes a GitHub Release; the publish job signs every
asset with ed25519 when the `QS_ED25519_KEY` secret is provisioned.

## Privacy

Everything runs locally — captures, OCR, settings never leave your
machine. The only outbound traffic is a share upload you trigger (image
lives 3 days on a Cloudflare Worker — or, if you opted in, goes to your
own Google Drive), an opt-in bug report (posted as a
public GitHub issue), and a daily GitHub release check. No analytics,
no accounts. Full policy: [docs/privacy.html](docs/privacy.html) ·
[quickshort site](https://2tsoftwareapp.github.io/quickshort/privacy.html).

## Support

Free and open source, built in spare time. If Quickshort saves you time:

- **[GitHub Sponsors](https://github.com/sponsors/truongnat)** — international, zero fees on personal sponsorships
- **VietQR** — MB Bank `0969069035` · ĐÀO QUANG TRƯỞNG (scan with any VN banking app)

## Known limits

- Mutter inhibits screencast while the screen is locked
- Wayland needs one overlay window per monitor (no global window
  positioning); pointer grab carries the selection across monitors
- Wayland has no global hotkey API — the resident app registers through
  the compositor instead (GNOME custom keybinding, swaymsg, hyprctl, and
  the GlobalShortcuts portal on KDE and other portal-capable Wayland
  desktops — probed at runtime); remaining compositors: bind
  `quickshort --capture` as a custom shortcut. The tray needs an
  AppIndicator extension on GNOME
- Unsigned macOS builds: first launch needs one right-click → Open; the
  self-signed identity keeps TCC grants across updates but is not a
  Developer ID
