# Codebase Audit: UI Quality & Snagit Feature Parity

**Date:** 2026-11-19  
**Workspace:** `/Users/truongdq/tx/GitHub/quickshort`  
**Baseline:** `main` @ `52e7614` (`cargo check --workspace` green with 6 `dead_code` warnings in `qs-app`; `cargo test --workspace` green: 220 passed, 0 failed, 5 ignored).

---

## 1. UI / UX & Design System Audit

### 1.1 Critical Layout & Rendering Bugs Found
1. **Top Toolbar Collision (`crates/qs-editor/src/toolbar.rs:394-403`)**
   - The center tool capsule contains **17 tools** (`2 + 6 + 9`), yielding `track_w = 517.0 px`, anchored dead-center at `bar_rect.center().x`.
   - Left zone consumes `~226 px` (`open 30 + save_as 30 + undo/redo 60 + canvas 72 + gaps + padding`) and right zone consumes `~332 px` (`zoom 94 + save 64 + export 72 + copy 68 + gaps + padding`).
   - Result: Any window narrower than `~1181 px` causes the center capsule to **overlap** the right-side Zoom pill or left-side Canvas button.
2. **Filename Text Overlaps Canvas Button (`crates/qs-editor/src/toolbar.rs:175-209`)**
   - `canvas_rect` is placed at `hist_rect.right() + 8.0` (width `72.0`), while the filename label is painted at `Pos2::new(hist_rect.right() + 14.0, cy)` — drawing directly on top of the `Canvas` button when window width `> 1260.0`.
3. **Hardcoded RGB Bypassing `qs-theme` (`crates/qs-editor/src/theme.rs:87-92`)**
   - `tool_active_gradient()` hardcodes `Color32::from_rgb(34, 76, 128)` and `Color32::from_rgb(22, 54, 96)` in dark mode despite the crate rule that hex/RGB colour literals live only in `qs-theme`.
4. **Compiler `dead_code` Warnings (`crates/qs-app/src/chrome.rs`, `settings.rs`)**
   - 6 unused items (`shelf_logical_size`, `ShotKind::Copied`, `Stack::modal_open`, `Hit::OcrInstall`, `InstallState::Ok`, `View::{installing, install_msg}`).

### 1.2 Visual Polish & Ergonomics Gaps (vs. Snagit / Modern Studio Apps)
1. **Chrome Density & Canvas Space (`crates/qs-editor/src/app.rs:1001-1255`)**
   - Up to 3 stacked top bars (`top_bar`, `properties_bar`, `quick_style_bar`) + non-collapsible `layers_panel` (`208 px` default) + fixed `image_panel` (`236 px`) consume `444 px` horizontally and `~120 px` vertically. On a 1280×800 laptop window, the editable canvas shrinks to ~836×680.
   - Side panels cannot be collapsed/toggled with a button or shortcut (`Tab` / `\` / panel toggle icons).
2. **Mixed Visual Language (Custom Chrome vs. Stock `egui` Widgets)**
   - Top bar uses custom-painted macOS-style gradients and vector icons (`icons.rs`), whereas `layers_panel`, `image_panel.rs`, and modal dialogs (`Canvas`, `Align & Distribute`, `Flatten all annotations`, `Unsaved changes`) use unstyled stock `egui::Button`, `egui::Slider`, and `egui::Window` with inconsistent spacing and no card/section grouping.
3. **Missing Bottom Status Bar**
   - No status bar showing canvas dimensions (`W × H px`), current selection bounds (`X, Y, W × H`), cursor coordinates, or clickable zoom presets (`Fit`, `50%`, `100%`, `200%`).
4. **No Bottom Recent Captures Filmstrip**
   - Snagit's defining UX is the bottom tray showing recent captures for instant switching, dragging into canvas, or batch actions. Currently `qs-editor` is strictly single-document per process.
5. **17 Tools in One Flat Row**
   - All 17 tools sit in a single horizontal pill without primary/secondary grouping or overflow/flyout menus (e.g., grouping Shapes `Arrow/Line/Rect/Ellipse`, Callout/Text/Step, Effects `Blur/Magnify/Spotlight`, Utility `Stamp/Eraser/Crop`).

---

## 2. Snagit Feature Parity Audit

| Category | Current Quickshort Status | Snagit Reference Capability | Gap / Priority |
|---|---|---|---|
| **Capture Core** | Region, Window, Fullscreen, Scrolling capture (`scroll.rs`), Screen Recording (`record.rs`), OCR (`qs-ocr`), Pin (`pin.rs`) | All-in-One HUD, Capture Profiles (Image/Video/Presets), Delay timer, Cursor toggle, Auto-apply effect | **Medium** — Capture Profiles & unified Capture HUD |
| **Editor Annotations** | 17 tools: Select, SelectRegion, Pen, Highlighter, Arrow, Line, Rect, Ellipse, Text, Callout, Step, Blur/Pixelate, Magnify, Spotlight, Stamp, Eraser, Crop | Same core tools + **Cutout** (strip splice), **Simplify** (auto UI block masking), **Smart Move** (OCR/object move), **Fill/Flood** | **High** — Cutout first; Simplify/Smart Move later |
| **Properties & Styles** | Quick Styles (`styles.rs`, 8 presets/tool), Advanced Typography (italic/underline/strike/outline/font family), Opacity, Align/Distribute, Group/Ungroup | Tool Properties drawer + Favorites + Theme palettes | **Low** — UI polish & Favorites/Search |
| **Image Operations** | Canvas Resize/Color/Anchors, Auto Trim, Rotate 90°/180°/270°, Flip H/V, Flatten | Same + **Image Resize** (resample W×H with aspect lock), **Combine Images** (step-by-step templates) | **High** — Image Resize & Templates |
| **Effects** | Border, Drop Shadow, Watermark, Torn Edge, Brightness/Contrast/Saturation | Same + **Page Curl**, **Reflection**, **Colorize / Color Replacement**, **Filters (Grayscale/Invert/Sepia)** | **Medium** — Page Curl, Reflection, Color Filters |
| **Library & Multi-Doc** | Single `.qsdoc`/image per `qs-editor` window; `qs-app` has basic album (`gtk_album.rs`) | **Recent Captures Tray (Filmstrip)** + **Library** (search by date, app, tag, file type) | **Critical** — Filmstrip tray + multi-doc tabs |
| **Share & Output** | Save `.qsdoc`, Export PNG, Copy to clipboard, Relay link (`share.rs`) | Multi-destination **Share** menu (Clipboard, File, Cloud/Webhook, Email, Print, Copy All) | **Medium** — Unified Share popover + Print |

---

## 3. Summary of Priorities
1. **P1 — Studio UI Overhaul:** Fix toolbar collision & filename overlap, eliminate hardcoded RGB in `theme.rs`, introduce collapsible side panels, grouped/overflow tool rail, styled inspector cards, and bottom status bar.
2. **P2 — Recent Captures Filmstrip & Multi-Doc Tabs:** Bottom collapsible tray + document switcher so users can jump between captures without reopening windows.
3. **P3 — High-Value Snagit Image Tools:** Cutout tool (horizontal/vertical strip splice with jagged/clean seam), Image Resize (resample), and Page Curl / Reflection / Color Filters.
4. **P4 — Templates (Combine Images) & Unified Share/Export:** Multi-image step templates + Share destination menu + Print.
5. **P5 — Capture Profiles & Smart Tools:** Saved capture presets in `qs-app` and Simplify (auto UI block redaction).
