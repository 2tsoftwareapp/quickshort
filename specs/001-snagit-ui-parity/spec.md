# Feature Specification: Snagit-Grade Studio UI & Feature Parity

**Feature Branch**: `001-snagit-ui-parity`  
**Created**: 2026-11-19  
**Status**: Draft  
**Input**: Audit & plan to upgrade Quickshort UI to a polished studio-grade experience and achieve Snagit feature parity.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Responsive, Studio-Grade Editor UI (Priority: P1)

As a Quickshort user editing screenshots on both laptop (1280×800) and desktop (1920×1080+) displays, I want a clean, non-overlapping toolbar, collapsible sidebars, unified custom-styled controls, and a bottom status bar so the canvas stays spacious and every control feels cohesive.

**Why this priority**: Currently the 17-tool top capsule (`517 px`) collides with action buttons below `1181 px` width, the filename overlaps the `Canvas` button, side panels are permanently pinned (`444 px` total), and stock `egui` widgets clash with the custom macOS-style header. Fixing layout & visual craft immediately elevates every workflow.

**Independent Test**: Open `qs-editor` at `1000×700`, `1280×800`, and `1600×1000` in both Dark and Light themes; verify zero overlapping widgets, toggle left/right panels, inspect styled inspector cards, and verify the status bar updates cursor/selection/zoom info.

**Acceptance Scenarios**:

1. **Given** a `1000×700` editor window with a loaded `.qsdoc`, **When** the top toolbar renders, **Then** no tool icon, filename label, zoom pill, or action button overlaps another control.
2. **Given** the editor workspace, **When** the user toggles the Layers or Properties/Image panel (via button or shortcut), **Then** the panel collapses smoothly and the canvas expands to reclaim the width.
3. **Given** Dark Studio or Light Studio theme, **When** inspecting any panel, dialog, or slider, **Then** all colours resolve through `qs-theme` tokens with zero hardcoded RGB literals.
4. **Given** an active document and selection, **When** the user moves the cursor or selects an object, **Then** the bottom status bar displays canvas dimensions, cursor coordinates, selection bounds, and a clickable zoom menu.

---

### User Story 2 - Recent Captures Filmstrip & Multi-Document Switching (Priority: P2)

As a user taking multiple screenshots in a session, I want a collapsible Recent Captures filmstrip at the bottom of the editor (plus open document tabs) so I can switch between captures, drag an earlier capture onto the current canvas, or delete old items without leaving the editor.

**Why this priority**: The Recent Captures tray is Snagit's signature productivity feature and the biggest workflow gap between Quickshort and Snagit.

**Independent Test**: Capture or open 3+ images in the configured capture directory, launch `qs-editor`, click thumbnails in the bottom filmstrip to switch documents, and drag a thumbnail onto the active canvas to insert it as an image/stamp layer.

**Acceptance Scenarios**:

1. **Given** recent captures exist in the capture directory, **When** `qs-editor` opens, **Then** the bottom filmstrip displays thumbnails sorted newest-first with quick-switch on click.
2. **Given** unsaved edits on the current document, **When** switching via the filmstrip, **Then** Quickshort preserves/autosaves state cleanly without data loss.
3. **Given** the filmstrip is open, **When** the user drags a thumbnail onto the canvas, **Then** the image is inserted or combined onto the canvas as an editable layer (or new step).

---

### User Story 3 - Snagit Image Tools: Cutout, Image Resize & Effects (Priority: P3)

As a documentation author, I want Snagit's **Cutout** tool (slice out a horizontal or vertical strip with a clean or jagged seam), **Image Resize** (resample image dimensions with aspect lock), and classic Snagit effects (**Page Curl**, **Reflection**, **Color Filters**) so I can prepare polished screenshots without external editors.

**Why this priority**: Cutout and Image Resize are daily-driver Snagit features not yet covered by Quickshort's existing Crop/Canvas Resize tools.

**Independent Test**: Run `qs-doc` / `qs-edit` / `qs-render` unit and pixel tests for Cutout, Image Resize, Page Curl, Reflection, and Color Filters; verify 1-step Undo/Redo and deterministic rendering.

**Acceptance Scenarios**:

1. **Given** a tall screenshot, **When** the user drags a horizontal Cutout strip and commits, **Then** the middle strip is removed, the top and bottom sections join with the chosen seam style (clean/jagged/wave), and Undo restores the original image in 1 step.
2. **Given** an image of `1920×1080`, **When** the user opens Image Resize and sets width to `1280` with aspect lock, **Then** height updates to `720` and export/canvas dimensions reflect the resampled resolution.
3. **Given** Page Curl or Reflection enabled in Effects, **When** rendering or exporting PNG, **Then** the effect composites deterministically without clipping existing annotations.

---

### User Story 4 - Combine Templates & Unified Share Destinations (Priority: P4)

As a technical writer, I want to combine multiple captures into a numbered step-by-step template and share/export via a unified Share menu (Copy, Save, Export PNG/JPEG/WebP, Relay Link, Print) so I can publish guides in one click.

**Why this priority**: Builds directly on User Story 2 (Filmstrip) and existing `qs-app/src/share.rs` relay infrastructure.

**Independent Test**: Select 2+ captures in the filmstrip, choose "Combine in Template", verify a composed multi-step document opens; trigger Share menu destinations and verify output.

**Acceptance Scenarios**:

1. **Given** 2 or more captures selected in the filmstrip, **When** "Combine in Template" is triggered, **Then** a new document is created with structured layout, step headers, and editable annotations.
2. **Given** the Share button in the top-right action bar, **When** clicked, **Then** a popover lists Copy, Save `.qsdoc`, Export (PNG/JPEG/WebP), Share Link (if configured), and Print.

---

### User Story 5 - Capture Profiles & Smart Simplify (Priority: P5)

As a power user, I want saved Capture Profiles (Image/Video/OCR/Timed presets with auto-effects) in `qs-app` and a **Simplify** tool that auto-detects UI regions to replace cluttered text/chrome with clean blocks.

**Why this priority**: Completes the end-to-end Snagit workflow from capture automation to simplified documentation graphics.

**Independent Test**: Create a Capture Profile with 3s delay + auto-border, trigger capture, verify the resulting document has the border applied; run Simplify on a UI screenshot and verify simplified vector blocks are generated as editable objects.

### Edge Cases

- Window resized below `800 px` width: secondary tools collapse into overflow/grouped popovers; toolbar never clips or overlaps.
- Switching documents in filmstrip while text editing is active: active text edit commits cleanly before switching.
- Cutout strip covering `>= 90%` of image height/width: rejected safely with a toast message instead of producing a zero-dimension canvas.
- Theme switched while dialogs (`Canvas`, `Flatten`, `Share`) are open: next frame reflects new `qs_theme::pal()` tokens immediately.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `qs-editor` top toolbar MUST prevent overlap at all supported window widths (`>= 720 px`) by grouping tools into a compact primary rail + flyout/overflow groups and positioning filename/breadcrumb without colliding with `Canvas`.
- **FR-002**: `qs-editor` MUST provide collapsible left (Layers) and right (Properties / Effects) sidebars with persisted state and keyboard shortcuts.
- **FR-003**: `qs-editor` MUST replace hardcoded RGB values (e.g., `theme.rs:89`) with `qs-theme` palette tokens and style all inspector controls, sliders, and modal dialogs using shared `qs-editor` UI primitives.
- **FR-004**: `qs-editor` MUST render a bottom status bar showing document dimensions, cursor `(x, y)`, selection bounds, and a zoom preset selector (`Fit`, `50%`, `100%`, `200%`).
- **FR-005**: `qs-editor` MUST provide a collapsible bottom Recent Captures filmstrip reading from the configured capture directory, supporting click-to-open and drag-to-insert.
- **FR-006**: `qs-doc`, `qs-edit`, and `qs-render` MUST support `Cutout` (horizontal/vertical strip removal with seam style), `Image Resize` (Lanczos/bilinear resample), and effects `Page Curl`, `Reflection`, and `Color Filters` with full 1-step Undo/Redo and backward-compatible `.qsdoc` serialization.
- **FR-007**: `qs-editor` MUST provide a unified Share/Export popover (PNG/JPEG/WebP quality options, Copy, Save, Relay Share, Print) and multi-image Combine Templates.
- **FR-008**: `qs-app` MUST resolve existing `dead_code` warnings and support Capture Profiles in the capture HUD/settings.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Zero widget overlap in `qs-editor` top bar across widths `720 px` to `2560 px`.
- **SC-002**: Zero hex or `Color32::from_rgb` palette literals in `qs-editor` (enforced by test, extending `qs-theme`'s `no_hex_colour_literals_in_migrated_files` check).
- **SC-003**: `cargo check --workspace --all-targets` and `cargo clippy --workspace` pass with 0 warnings in `qs-editor`, `qs-theme`, and `qs-app`.
- **SC-004**: All new `qs-doc` / `qs-edit` / `qs-render` features (Cutout, Image Resize, Page Curl, Reflection, Filters) have deterministic unit + pixel tests passing in `cargo test --workspace`.
