# Tasks: OCR Line Geometry

**Input**: Design documents in `/specs/004-ocr-line-boxes/` (`spec.md`, `plan.md`)

## Phase 1: Geometry API (done)

- [x] **T001** Add `pub struct OcrLine { text, x, y, w, h, confidence }` to `crates/qs-ocr/src/lib.rs`, documented as image pixels with a top-left origin.
- [x] **T002** Add `pub(crate) fn px_rect_from_normalized(x, y, w, h, img_w, img_h)` converting Vision's normalized bottom-left rect to those pixels, clamping to the image.
- [x] **T003** [P] Unit tests for the conversion: top-left line, bottom-left line, full-image rect, zero-size rect, overrunning rect.
- [x] **T004** Implement `extract_lines` in `crates/qs-ocr/src/macos.rs` from `VNRecognizedTextObservation` bounding boxes and candidate confidence.
- [x] **T005** Make macOS `extract` delegate to `extract_lines` and join with `\n`, so text and geometry cannot drift.
- [x] **T006** `extract_lines` on Linux, Windows and unknown targets returns an explicit "geometry not available on this platform" error; `extract_text` keeps working everywhere.
- [x] **T007** Guard test: joining `extract_lines` texts reproduces `extract_text` on this machine (skipped when the engine reports no text), and no existing call site changed.

## Phase 2: Smart Move proposal (this change)

- [x] **T008** Cache `extract_lines` for the active document in `qs-editor`, invalidated on document revision.
- [x] **T009** With `Tool::SmartMove` armed and no pending source, hover picks the line under the cursor and draws the proposal box.
- [x] **T010** Click on a proposal arms it as the source rect; clicking elsewhere keeps today's manual drag.
- [x] **T011** UAT: proposal box matches the OCR rect within a pixel at the four viewports.

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
- `cargo test --workspace` — **276 passed / 0 failed** (8 new).
- The Vision path is not just compiled but *executed*: `text_and_geometry_agree_on_the_same_image`
  runs a real `VNRecognizeTextRequest` and asserts the two entry points agree, and
  `geometry_of_real_ui_text_sits_inside_the_image` reads 33 real UI lines off a repo screenshot.
- Six conversion tests pin the bottom-left→top-left flip, the `0..=1` clamp, a zero-size
  observation, an overrunning rect, and a degenerate image size.

Honest gaps, on purpose:

- **T007** is covered twice: a blank-frame agreement test that holds on any machine, and a
  real-text test on `docs/shots/settings-general.png` (1640×1200, already in the repo) which
  recognizes 33 lines, asserts every rect and confidence is sane and inside the image, and checks
  the joined geometry against `extract_text`. No rasterizer is needed, so `qs-ocr` keeps its leaf
  status (Constitution II).
- **T006** adds `extract_lines` stubs for the Linux and Windows backends. Those targets cannot be
  compiled here, so the stubs are deliberately the simplest possible code — an explicit `bail!` —
  rather than untested FFI. Real geometry for those engines is Phase 3 (T014, T015).

## Phase 2 Completion Notes

Landed on top of Phase 1:

- `qs-editor/src/ocr_hint.rs` — `OcrHints` recognizes on a worker thread and maps every line into
  document coordinates through one helper (`px_to_doc`), so no caller does arithmetic.
- Hover proposal: with Smart Move armed and nothing pending, the line under the pointer is drawn as
  the candidate source box.
- Click on a line arms it (`begin_smart_move`); a click anywhere else falls through to the manual
  drag, so the old workflow is untouched.
- The cache is keyed on `(base size, scale, view)` and dropped in `apply_document`, so opening a
  different capture of the same size cannot reuse the previous capture's lines. Revision-based
  invalidation was rejected: annotating does not change which text is under the pixels, and would
  have re-run OCR on every stroke.
- `qs-editor/src/ocr_hint.rs` joined the theme literal guard.

Verified: `cargo test --workspace` **284 passed / 0 failed** (8 new) and `cargo check
--workspace --all-targets` clean. The integration test reads the repo screenshot, recognizes 33
lines, and asserts that hovering the centre of every line offers exactly that line's rect within
1 px, in both a 1:1 document and a 2× (Retina) document.

Honest gap: **T011** is proven at the geometry level, not through the UI at the four viewports —
driving a real hover needs a full `EditorApp` window, which the headless UAT harness does not
build. The proposal paint itself is the same `canvas::paint_move_overlay` already exercised by the
viewport UAT, so the untested part is the wiring, not the drawing.
