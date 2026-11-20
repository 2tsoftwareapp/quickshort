# Tasks: OCR Line Geometry

**Input**: Design documents in `/specs/004-ocr-line-boxes/` (`spec.md`, `plan.md`)

## Phase 1: Geometry API (this change)

- [x] **T001** Add `pub struct OcrLine { text, x, y, w, h, confidence }` to `crates/qs-ocr/src/lib.rs`, documented as image pixels with a top-left origin.
- [x] **T002** Add `pub(crate) fn px_rect_from_normalized(x, y, w, h, img_w, img_h)` converting Vision's normalized bottom-left rect to those pixels, clamping to the image.
- [x] **T003** [P] Unit tests for the conversion: top-left line, bottom-left line, full-image rect, zero-size rect, overrunning rect.
- [x] **T004** Implement `extract_lines` in `crates/qs-ocr/src/macos.rs` from `VNRecognizedTextObservation` bounding boxes and candidate confidence.
- [x] **T005** Make macOS `extract` delegate to `extract_lines` and join with `\n`, so text and geometry cannot drift.
- [x] **T006** `extract_lines` on Linux, Windows and unknown targets returns an explicit "geometry not available on this platform" error; `extract_text` keeps working everywhere.
- [x] **T007** Guard test: joining `extract_lines` texts reproduces `extract_text` on this machine (skipped when the engine reports no text), and no existing call site changed.

## Phase 2: Smart Move proposal (later)

- [ ] **T008** Cache `extract_lines` for the active document in `qs-editor`, invalidated on document revision.
- [ ] **T009** With `Tool::SmartMove` armed and no pending source, hover picks the line under the cursor and draws the proposal box.
- [ ] **T010** Click on a proposal arms it as the source rect; clicking elsewhere keeps today's manual drag.
- [ ] **T011** UAT: proposal box matches the OCR rect within a pixel at the four viewports.

## Phase 3: Copy text by region, and the other engines (later)

- [ ] **T012** Drag a region with the text tool to copy the lines inside it, in reading order.
- [ ] **T013** Empty-region feedback distinct from "engine unavailable".
- [ ] **T014** Linux: tesseract `tsv` output parsed into `OcrLine`s (pure parser, fixture-tested).
- [ ] **T015** Windows: WinRT `OcrLine.Words[].BoundingRect` mapped into `OcrLine`s (compile-verified on the Windows target).

## Verification Gate (Run After Each Phase)

- `cargo check --workspace --all-targets`
- `cargo test --workspace`

## Phase 1 Completion Notes

Landed and verified on this machine:

- `cargo check --workspace --all-targets` — clean, no new warnings.
- `cargo test --workspace` — **275 passed / 0 failed** (7 new).
- The Vision path is not just compiled but *executed*: `text_and_geometry_agree_on_the_same_image`
  runs a real `VNRecognizeTextRequest` (≈1.2 s) and asserts the two entry points agree.
- Six conversion tests pin the bottom-left→top-left flip, the `0..=1` clamp, a zero-size
  observation, an overrunning rect, and a degenerate image size.

Honest gaps, on purpose:

- **T007** is covered by the text↔geometry agreement test on a blank frame rather than a
  text-fixture test: generating a bitmap with real glyphs would need a rasterizer, and `qs-ocr` must
  not depend on `qs-render` (Constitution II). A real-text assertion belongs in Phase 2's UAT, where
  the editor already has the bundled font and a real screenshot.
- **T006** adds `extract_lines` stubs for the Linux and Windows backends. Those targets cannot be
  compiled here, so the stubs are deliberately the simplest possible code — an explicit `bail!` —
  rather than untested FFI. Real geometry for those engines is Phase 3 (T014, T015).
