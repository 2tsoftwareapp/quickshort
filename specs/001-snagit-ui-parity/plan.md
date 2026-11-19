# Implementation Plan: Snagit-Grade Studio UI & Feature Parity

**Branch**: `001-snagit-ui-parity` | **Date**: 2026-11-19 | **Spec**: [spec.md](./spec.md)

## Summary

Upgrade Quickshort from a functional annotation editor into a cohesive, Snagit-grade visual studio:
1. Fix immediate layout collisions, hardcoded colours, and stock-widget visual mismatches in `qs-editor` & `qs-theme`.
2. Re-architect the editor workspace around a responsive tool rail, collapsible sidebars, styled inspector cards, bottom status bar, and a Recent Captures filmstrip.
3. Implement remaining high-value Snagit capabilities: Cutout, Image Resize, Page Curl / Reflection / Color Filters, Combine Templates, Unified Share/Export, and Capture Profiles.

## Technical Context

- **Language/Version**: Rust (workspace edition 2021/2024, `resolver = "2"`)
- **Primary Dependencies**: `eframe` / `egui` `0.37.0`, `tiny-skia` `0.11.4`, `image` `0.25.10`, `qs-theme`, `qs-doc`, `qs-edit`, `qs-render`, `qs-capture`, `qs-ocr`
- **Storage**: `.qsdoc` binary/zip archive (`crates/qs-doc/src/qsdoc.rs`), JSON config & `~/.config/quickshort/styles.json`, local capture directory
- **Testing**: `cargo test --workspace` (unit, serialization round-trip, pixel render parity, theme contrast/no-literal-colors tests)
- **Target Platform**: macOS (primary), Linux (Wayland/X11), Windows
- **Constraints**:
  - Colour tokens MUST live exclusively in `crates/qs-theme/src/lib.rs` (zero literals in `qs-editor`).
  - `.qsdoc` format changes MUST remain backward-compatible with v1–v4 files.
  - Every `qs-edit` mutation MUST preserve 1-step Undo/Redo invariants.

## Constitution Check

- **Design System Discipline**: Single source of truth in `qs-theme` (`Palette` + `space` + `radius` + `font`).
- **Layered Crate Boundaries**: `qs-doc` (data/schema) -> `qs-render` (pure pixel rendering) -> `qs-edit` (state/undo) -> `qs-editor` (egui UI). No UI dependencies in `qs-doc`/`qs-edit`/`qs-render`.
- **Verification Gate**: `cargo check --workspace`, `cargo test --workspace`, and visual/responsive checks at `720px`, `1000px`, `1280px`, `1600px`.

## Project Structure

### Documentation (this feature)
```text
specs/001-snagit-ui-parity/
├── audit.md     # Codebase audit (UI bugs + Snagit parity matrix)
├── spec.md      # Prioritized user stories & functional requirements
├── plan.md      # This implementation plan
└── tasks.md     # Phased, actionable task breakdown
```

### Primary Source Targets
- `crates/qs-theme/src/lib.rs` — Add missing tokens (e.g., `tool_active_top`, `tool_active_bot`, card/status-bar tokens).
- `crates/qs-editor/src/theme.rs` — Remove hardcoded RGB; add shared styled card, section header, slider, and modal frame helpers.
- `crates/qs-editor/src/toolbar.rs` — Responsive layout (fix `517px` capsule collision & filename overlap), grouped tool flyouts, Share popover trigger.
- `crates/qs-editor/src/app.rs` — Collapsible Left/Right panels, unified Properties/Effects inspector, bottom Status Bar, bottom Filmstrip tray.
- `crates/qs-editor/src/image_panel.rs` — Refactor into styled Snagit-style Properties & Effects cards.
- `crates/qs-doc/src/` & `crates/qs-render/src/` & `crates/qs-edit/src/` — Cutout, Image Resize, Page Curl, Reflection, Color Filters, Templates.
- `crates/qs-app/src/` — Clean up 6 `dead_code` warnings; add Capture Profiles & HUD polish.

## Phased Delivery Strategy

1. **Phase 1 — UI Foundation & Bug Fixes (P1):**
   - Fix `toolbar.rs` filename overlap and responsive tool capsule grouping/overflow.
   - Move `tool_active_gradient` colours into `qs-theme`; extend `no_hex_colour_literals` check to `qs-editor`.
   - Add collapsible sidebars (`Layers`, `Properties & Effects`), bottom Status Bar, and styled cards/modals.
2. **Phase 2 — Recent Captures Filmstrip & Multi-Doc (P2):**
   - Add `filmstrip.rs` in `qs-editor` scanning capture directory thumbnails asynchronously, click-to-switch, drag-to-insert.
3. **Phase 3 — Snagit Image Tools & Effects (P3):**
   - Implement `Cutout` (doc + render + edit + UI tool), `Image Resize` dialog, and `Page Curl` / `Reflection` / `Color Filters`.
4. **Phase 4 — Templates & Unified Share/Export (P4):**
   - Multi-image Combine Templates + Share/Export popover (PNG/JPEG/WebP, Relay, Print).
5. **Phase 5 — Capture Profiles & Simplify (P5):**
   - Saved Capture Profiles in `qs-app` + auto UI-block Simplify tool.
