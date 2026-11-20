# Tasks: Annotation Parity

**Input**: Design documents in `/specs/003-annotation-parity/` (`spec.md`, `plan.md`)

## Phase 1: Fill / Paint Bucket (Priority: P1)

- [x] **T001** Add `ObjectKind::Fill { seed: PointF, rgb: u32, tolerance: u8 }` to `crates/qs-doc/src/lib.rs` with `#[serde(default)]` on the optional knobs so the variant is additive.
- [x] **T002** Render it in `crates/qs-render/src/draw.rs`: read the seed pixel from the composited `Pixmap`, flood-fill (4-connected, iterative, channel-Chebyshev `tolerance`) bounded by the pixmap, writing `rgb` with the object's alpha; add to the non-paint-only list.
- [x] **T003** [P] Extend `crates/qs-render/src/bounds.rs` so Fill reports a usable bounding box (its seed, expanded to the canvas) for selection and nudge.
- [x] **T004** [P] Add `Tool::Fill` and `Editor::add_fill(seed, rgb, tolerance)` to `crates/qs-edit`, wired through the existing single-history-entry mutation path.
- [x] **T005** Add the tool to `icons.rs`, the toolbar utility group, and `canvas.rs` (click creates the fill at the click point).
- [x] **T006** Pixel tests: solid area fills and stops at a border; tolerance 0 vs high; seed outside the canvas is clamped; hiding the object restores the original pixels.

## Phase 2: Smart Move (Priority: P2)

- [x] **T007** Add `ObjectKind::SmartMove { src: RectF, dst: PointF }` to `qs-doc`.
- [x] **T008** Render it in `draw.rs`: capture `src` pixels from the composited pixmap, cover `src` with the dominant ring colour sampled around it, blit the captured patch at `dst`; clip both to the pixmap; add to the non-paint-only list.
- [x] **T009** [P] Bounds for Smart Move (union of source and destination).
- [x] **T010** [P] `Tool::SmartMove` + `Editor::add_smart_move(src, dst)` in `qs-edit`.
- [x] **T011** Canvas drag interaction: drag defines the source rect, drop defines the destination; zero-area drags create nothing.
- [x] **T012** Pixel tests: block moves and the hole is healed; overlapping move keeps original content; edge clipping never panics; undo restores exactly.

## Phase 3: Colorize (Priority: P3)

- [x] **T013** Add `ColorizeEffect { target: u32, replacement: u32, tolerance: u8, strength: f32 }` and `Effects::colorize: Option<ColorizeEffect>` with `#[serde(default)]`.
- [x] **T014** Apply it in `crates/qs-render/src/effects.rs` after `color_adjust` and before `color_filter`, blending by `strength`, with target == replacement treated as a no-op.
- [x] **T015** [P] Inspector card in `image_panel.rs`: enable toggle, target/replacement swatches, tolerance and strength sliders, token colours only.
- [x] **T016** Tests: only the targeted colour changes; strength blends; disabled matches a document without the effect; `.qsdoc` round-trip renders identically.

## Phase 4: Quick Style Favourites & Search (Priority: P4)

- [x] **T017** Add `QuickStyle.favorite: bool` with `#[serde(default)]`; sort favourites first in `presets_for`.
- [x] **T018** Wire the star into the chip context menu and draw a marker on favourited chips; persist through the existing `styles.json` save.
- [x] **T019** Add a compact filter field to the quick-style bar; case-insensitive substring match on the preset name; empty state when nothing matches.
- [x] **T020** Add `qs-editor/src/styles.rs` to `MIGRATED` in `crates/qs-theme/tests/no_literal_colors.rs`.
- [x] **T021** Tests: old `styles.json` loads with the flag off; favourite ordering; filter matching.

## Verification Gate (Run After Each Phase)

- `cargo check --workspace --all-targets`
- `cargo test --workspace`
- Quick-style bar and Colorize card checked at `720×600`, `1000×700`, `1280×800`, `1600×1000` in Dark & Light.

## Completion Notes

All 21 tasks landed in one change set. Verification actually run:

- `cargo check --workspace --all-targets` — clean, zero warnings.
- `cargo test --workspace` — **268 passed / 0 failed** (19 new).
- `qs-theme` literal guard now also covers `qs-editor/src/styles.rs`.
- `cargo test -p qs-editor --test uat_e2e` — the toolbar/inspector UAT renders the enlarged
  capsule and the Effects panel (now carrying the Colorize card) at 720×600, 1000×700, 1280×800 and
  1600×1000 in Dark and Light.

Deliberate scope notes:

- **Smart Move is two gestures, by design**: drag a box around the content, then click where it
  lands. A stray click that produces a degenerate box arms nothing.
- **OCR-proposed boxes are out of scope.** `qs-ocr` exposes `extract_text` only — no word geometry —
  so the source rectangle is user-dragged. The mechanic (move the content, heal the hole) is what
  shipped; auto-proposing the box needs a new platform OCR binding.
- **Fill's selection anchor is its seed.** The filled region only exists once the canvas is
  composited, which `bounds()` deliberately does not have, so click-select targets the seed and the
  layers panel selects the object directly.
- **Favourites apply to saved presets only.** Built-in presets are regenerated on every launch, so a
  star on one could not persist; the context menu says so and points at "Save style".
