# Implementation Plan: OCR Line Geometry

**Branch**: `004-ocr-line-boxes` | **Date**: 2026-11-20 | **Spec**: [spec.md](./spec.md)

## Summary

Phase 1 exposes the geometry the macOS backend already receives:

- `OcrLine { text, x, y, w, h, confidence }` in `qs-ocr`, in image pixels, top-left origin.
- `extract_lines(&img, lang) -> Result<Vec<OcrLine>>` alongside the existing `extract_text`.
- One tested helper converts Vision's normalized bottom-left rect into those pixels.
- `extract_text` delegates to `extract_lines` on macOS so the two cannot disagree.
- Unsupported platforms fail with a clear message instead of returning an empty list.

Phases 2 and 3 (Smart Move proposals, copy-text-in-region) consume it later and are **not** part of
this phase.

## Technical Context

- **Stack**: Rust; `objc2-vision` 0.3 (already linked) for `VNRecognizeTextRequest`.
- **New dependencies**: none.
- **Coordinates**: Vision returns `CGRect` normalized to `0..=1` with the origin at the **bottom-left**
  of the image; every other surface in this codebase is top-left pixels. The conversion lives in one
  `pub(crate) fn px_rect_from_normalized(...)` next to the type, with unit tests.
- **Testing**: unit tests for the conversion (all four corners + overflow), plus a test that
  `extract_text` and `extract_lines` agree when an engine is available (skipped when it is not).
- **Constraints**: `qs-ocr` stays a leaf crate — no `qs-doc` type in the public API.

## Constitution Check

| Principle | How this plan complies |
|---|---|
| **I. Design Token SSOT** | No UI in this phase, so no colour or spacing is introduced. |
| **II. Layered Crates** | The new type is plain data inside `qs-ocr`; no dependency is added, so the crate stays a leaf. `qs-editor` will consume it in Phase 2. |
| **III. `.qsdoc` Compatibility** | The document format is untouched; OCR geometry is never persisted. |
| **IV. Reversible Actions** | Phase 1 mutates nothing. Phase 2 will create Smart Move objects through the existing one-undo-step path. |
| **V. Verification Gate** | `cargo check --workspace --all-targets` and `cargo test --workspace`. The Vision path is compiled by every local build (this is the primary platform), and the conversion is proven by tests rather than eyesight. |

**Deviations**: Linux and Windows keep the explicit "geometry not available on this platform" error
until they can be compiled on their own targets. Shipping untestable FFI would violate Principle V.

## Project Structure

| File | Change |
|---|---|
| `crates/qs-ocr/src/lib.rs` | `OcrLine`, `extract_lines`, `px_rect_from_normalized`, unit tests. |
| `crates/qs-ocr/src/macos.rs` | `extract_lines` from Vision observations; `extract` delegates to it. |
| `crates/qs-ocr/src/linux.rs`, `windows.rs`, and the stub arm | `extract_lines` returns an explicit unsupported error. |
| `crates/qs-editor/src/ocr_hint.rs` | `find()` alongside `hit`/`text_in`; keyed recognition cache. |
| `crates/qs-editor/src/canvas.rs`, `app.rs` | Find highlights, the ⌘F dialog, panning to matches. |
| `crates/qs-ocr/Cargo.toml` | Only if a Vision feature flag turns out to be missing for `boundingBox`. |

## Design Decisions

**Flat `f32` fields, not `RectF`.** `RectF` lives in `qs-doc`; importing it would add an arrow from a
leaf crate to the document model and force `qs-doc` on every OCR consumer. Four numbers plus a doc
comment cost less than the layering violation.

**`extract_text` delegates instead of duplicating.** Two independent Vision walks would drift the
moment one gains a filter. Delegating means "the text" is always exactly the texts of the lines.

**Failure is loud, empty is honest.** An engine that found nothing returns `Ok(vec![])`; a platform
that cannot report geometry returns `Err`. Collapsing those two into an empty list would make the UI
say "no text here" when the truth is "this build cannot tell you".

## Phased Delivery

1. **Phase 1 (this change)** — type, conversion helper + tests, macOS implementation, explicit errors
   elsewhere.
2. **Phase 2** — Smart Move hover proposal in `qs-editor`, falling back to the manual drag.
3. **Phase 3** — copy-text-in-region.
4. **Phase 4 (this change)** — find-in-capture: `OcrHints::find`, a small dialog (⌘F), highlights and
   panning, plus integration tests on the repo screenshot.
5. **Phase 5 (blocked)** — geometry for the Linux and Windows backends, on a machine that can build
   those targets.
