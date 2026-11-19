# Tasks: Snagit-Grade Studio UI & Feature Parity

**Input**: Design documents in `/specs/001-snagit-ui-parity/` (`audit.md`, `spec.md`, `plan.md`)

## Phase 1: UI Foundation & Layout Fixes (Priority: P1)

- [x] **T001** [P] Clean up 6 `dead_code` warnings in `crates/qs-app/src/chrome.rs` and `crates/qs-app/src/settings.rs`.
- [x] **T002** [P] Add `tool_active_top`, `tool_active_bot`, `card_bg`, `status_bg` tokens to `Palette` in `crates/qs-theme/src/lib.rs` (both `DARK` and `LIGHT`) and remove hardcoded RGB in `crates/qs-editor/src/theme.rs:89`.
- [x] **T003** Fix filename text overlap with `Canvas` button in `crates/qs-editor/src/toolbar.rs:201` (anchor after `canvas_rect.right() + 12.0` and check remaining space before center capsule).
- [x] **T004** Re-architect `crates/qs-editor/src/toolbar.rs` center tool capsule for responsive widths (`>= 720 px`): group secondary tools into flyout/dropdowns (Shapes, Effects, Utility) or collapse smoothly when `bar_rect.width() < 1200.0` so zero overlap occurs.
- [x] **T005** Add collapsible Left (`Layers`) and Right (`Properties & Effects`) panel toggles + keyboard shortcuts in `crates/qs-editor/src/app.rs`.
- [x] **T006** Create shared styled UI components in `crates/qs-editor/src/theme.rs` (section cards, pill buttons, custom sliders, modal frames) and apply them to `crates/qs-editor/src/image_panel.rs` and `app.rs` dialogs (`Canvas`, `Align & Distribute`, `Flatten`, `Unsaved changes`).
- [x] **T007** Add bottom Status Bar in `crates/qs-editor/src/app.rs` showing canvas `W × H px`, cursor `(x, y)`, selection bounds, and clickable zoom presets (`Fit`, `50%`, `100%`, `200%`).
- [x] **T008** Add a test in `qs-theme` or `qs-editor` verifying zero hardcoded RGB/hex literals in `crates/qs-editor/src/`.

## Phase 2: Recent Captures Filmstrip & Multi-Doc (Priority: P2)

- [x] **T009** Add `crates/qs-editor/src/filmstrip.rs` to scan the capture directory for recent PNG/JPG/`.qsdoc` thumbnails asynchronously (using `qs_doc::qsdoc::read_thumb` where available).
- [x] **T010** Render collapsible bottom Filmstrip panel in `crates/qs-editor/src/app.rs` with click-to-switch (guarded by autosave/dirty check) and drag-to-insert onto canvas.
- [x] **T011** Add multi-document tab strip or quick-switcher state so switching between recent captures preserves zoom/pan/undo history per open tab.

## Phase 3: Snagit Image Tools & Effects (Priority: P3)

- [ ] **T012** Implement `Cutout` operation in `crates/qs-doc`, `crates/qs-edit`, and `crates/qs-render` (horizontal/vertical strip removal, clean/jagged/wave seam, 1-step Undo, safety guard against `>= 90%` removal).
- [ ] **T013** Add `Tool::Cutout` UI & icon in `crates/qs-editor` (`icons.rs`, `toolbar.rs`, `canvas.rs` drag preview).
- [ ] **T014** Implement `Image Resize` (Lanczos3 / bilinear resampling with aspect-ratio lock) in `crates/qs-edit` and add tab/mode in the `Canvas` dialog of `crates/qs-editor/src/app.rs`.
- [ ] **T015** [P] Add `Page Curl`, `Reflection`, and `Color Filters` (Grayscale, Sepia, Invert) to `Effects` in `crates/qs-doc`, `crates/qs-render`, and `crates/qs-editor/src/image_panel.rs` with pixel tests.

## Phase 4: Combine Templates & Unified Share/Export (Priority: P4)

- [ ] **T016** Implement unified Share/Export popover in `crates/qs-editor/src/toolbar.rs` (Copy, Save `.qsdoc`, Export PNG/JPEG/WebP with quality slider, Relay Share, Print).
- [ ] **T017** Implement multi-image Combine Templates (`2-up`, `3-up`, `Step-by-Step Guide`) triggered from Filmstrip multi-selection.

## Phase 5: Capture Profiles & Smart Simplify (Priority: P5)

- [ ] **T018** Add Capture Profiles (preset name, mode, delay timer, cursor toggle, auto-effect) to `qs-app/src/config.rs` and `settings.rs` / capture HUD.
- [ ] **T019** Prototype `Simplify` tool using `qs-ocr` + connected-component / edge bounding boxes to convert UI screenshots into clean editable vector blocks.

## Verification Gate (Run After Each Phase)

- `cargo check --workspace --all-targets`
- `cargo test --workspace`
- Visual verification of `qs-editor` at `720×600`, `1000×700`, `1280×800`, and `1600×1000` in Dark & Light themes.
