# Implementation Plan: Annotation Parity

**Branch**: `003-annotation-parity` | **Date**: 2026-11-20 | **Spec**: [spec.md](./spec.md)

## Summary

Four independent additions, each landing in the lowest layer that can express it:

1. **Fill** — new `ObjectKind::Fill`, rendered by flood-filling the composited pixmap at draw time.
2. **Smart Move** — new `ObjectKind::SmartMove`, rendered by copying, covering, then pasting pixels.
3. **Colorize** — new document effect applied in the existing effects pass.
4. **Style favourites/search** — `QuickStyle.favorite` plus filtering in the quick-style bar.

## Technical Context

- **Stack**: Rust workspace; `eframe`/`egui` 0.37, `tiny-skia` for render, `serde` for the document.
- **New dependencies**: none. Flood fill, patch copy and colour replacement are written directly
  against `tiny_skia::Pixmap` and `image::RgbaImage`.
- **Schema impact**: two new `ObjectKind` variants and one new optional `Effects` field, all
  additive; older files keep loading (missing field → `None`) and `DOC_VERSION` stays `4` because
  no existing field changes meaning.
- **Testing**: `qs-render` pixel tests for each render path, `qs-doc` round-trip for the new
  variants/effect, `qs-editor::styles` serialisation tests for the flag, plus the workspace gate.
- **Constraints**: one undo entry per user action; no blocking work added to a capture frame.

## Constitution Check

| Principle | How this plan complies |
|---|---|
| **I. Design Token SSOT** | The Colour card reuses `image_panel`'s existing swatch helpers; `styles.rs` gains no literals and joins the `MIGRATED` list so the guard covers it. |
| **II. Layered Crates** | Objects and the effect live in `qs-doc`; rendering in `qs-render`; mutation/undo in `qs-edit`; only UI in `qs-editor`. No new arrows, no UI types below `qs-editor`. |
| **III. `.qsdoc` Compatibility** | Two new `ObjectKind` variants and one `#[serde(default)]` effect field; a document written before this change loads and renders unchanged. `DOC_VERSION` is not bumped because nothing existing changed shape. |
| **IV. Reversible Actions** | Each tool creates exactly one object inside one `Editor` mutation, so undo is one step. Fill and Smart Move are recomputed at render time, so they are non-destructive and can be hidden in the layers panel. |
| **V. Verification Gate** | `cargo check --workspace --all-targets`, `cargo test --workspace`, plus the quick-style bar and the Colorize card checked at the four viewports in Dark and Light. |

**Deviations**: none. Note that OCR-proposed Smart Move boxes are **out of scope**: `qs-ocr`
exposes `extract_text` only (no word geometry), so the source rectangle is user-dragged. The
mechanic — move the content, heal the hole — is unchanged; proposing the box automatically would
need a new platform OCR binding and is left to a later feature.

## Project Structure

| File | Change |
|---|---|
| `crates/qs-doc/src/lib.rs` | `ObjectKind::Fill { seed, rgb, tolerance }`, `ObjectKind::SmartMove { src, dst }`. |
| `crates/qs-doc/src/effects.rs` | `ColorizeEffect { target, replacement, tolerance, strength }`, `Effects::colorize`. |
| `crates/qs-render/src/draw.rs` | Render branches: `flood_fill`, `smart_move`; both excluded from `is_paint_only`. |
| `crates/qs-render/src/bounds.rs` | Bounds for the two new kinds (union of the touched rect). |
| `crates/qs-render/src/effects.rs` | Apply Colorize after `color_adjust`, before `color_filter`. |
| `crates/qs-edit/src/lib.rs` | `Tool::Fill`, `Tool::SmartMove`; object creation methods. |
| `crates/qs-edit/src/hit.rs` | Bounds mapping so the new objects are selectable/nudgeable. |
| `crates/qs-editor/src/icons.rs` | Icons for the two tools. |
| `crates/qs-editor/src/toolbar.rs` | Tools in the utility group. |
| `crates/qs-editor/src/canvas.rs` | Click-to-fill, and drag-rect for Smart Move. |
| `crates/qs-editor/src/image_panel.rs` | Colorize card (target/replacement swatches, tolerance, strength). |
| `crates/qs-editor/src/styles.rs` | `favorite` flag, favourites-first ordering, name filter. |
| `crates/qs-theme/tests/no_literal_colors.rs` | Add `qs-editor/src/styles.rs` to `MIGRATED`. |

## Design Decisions

**Fill is an object, not a pixel edit.** The seed, colour and tolerance are stored; the region is
recomputed per render. That keeps undo free, keeps the base image untouched, and means the fill
follows the layers underneath it instead of freezing a stale outline.

**Tolerance is per-channel Chebyshev distance.** Simple, predictable, and matches what the user sees
when they raise a slider: "how far from this colour counts as the same colour".

**Smart Move captures before it covers.** One pass reads the source rect out of the composited
pixmap, then paints the cover, then blits the captured patch — so overlapping source/destination
works without special cases.

**Colorize blends, it does not mask-replace.** `strength` interpolates between the original pixel and
the replacement, so the default 1.0 is a clean swap while lower values tint.

## Phased Delivery

1. **Phase 1 — Fill:** object, render branch, edit API, toolbar/canvas wiring, pixel tests.
2. **Phase 2 — Smart Move:** object, render branch, drag interaction, pixel tests.
3. **Phase 3 — Colorize:** effect struct, render pass, inspector card, round-trip test.
4. **Phase 4 — Style favourites/search:** flag with serde default, ordering, filter field, tests.

Each phase ends with the Constitution V gate before the next begins.
