# Feature Specification: Annotation Parity — Fill, Smart Move, Colorize, Style Favourites

**Feature Branch**: `003-annotation-parity`

**Created**: 2026-11-20

**Status**: Draft

**Input**: Remaining non-Critical gaps from `specs/001-snagit-ui-parity/audit.md` after feature 002:
Fill/Flood, Smart Move, Colorize/Color Replacement, and Quick Style favourites/search.

## Context

Feature 002 closed the audit's last **Critical** gap. What is left in the parity matrix is tool
depth: the annotation set still lacks a paint bucket and a way to relocate existing content, the
effects set has no colour replacement (only whole-image Grayscale/Sepia/Invert), and Quick Styles
offers eight presets per tool with no way to mark the ones a user actually reaches for.

Each of the four is independent and individually shippable, which is how they are prioritised.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Paint Bucket (Priority: P1)

A user clicks inside a flat area of a screenshot and expects that enclosed area to fill with the
current colour, without disturbing the rest of the image and without losing editability.

**Why this priority**: It is the most-requested gap in the tool row and the cheapest to verify —
one click, one pixel assertion. It also unblocks redacting blank form fields in one action.

**Independent Test**: Draw a filled rectangle on a white canvas, click inside it with Fill active,
and confirm the rectangle's interior takes the chosen colour while the border and the area outside
are untouched; then undo once and confirm the image returns exactly.

**Acceptance Scenarios**:

1. **Given** a capture with a white page and dark text, **When** the user clicks a blank area with
   Fill, **Then** the connected blank region fills with the current colour and the text pixels stay
   dark.
2. **Given** a fill just applied, **When** the user presses undo once, **Then** the image is
   pixel-identical to before the fill.
3. **Given** a fill object exists, **When** the user hides it in the layers panel, **Then** the
   original pixels show again (the fill is non-destructive, not baked into the base).
4. **Given** a fill is applied over a gradient, **When** the tolerance is low, **Then** only the
   near-identical band fills; raising the tolerance widens it predictably.

---

### User Story 2 - Smart Move (Priority: P2)

A user drags a rectangle around a UI element and drops it elsewhere on the canvas; the element moves
and the hole it left is covered with the surrounding background.

**Why this priority**: This is the one editing operation Quickshort cannot do at all today, and it
is what makes "rearrange this dialog before sharing it" possible. It depends on nothing built in
P1, so it ships on its own.

**Independent Test**: On a synthetic image with a solid 40×20 block on a uniform background, drag a
box exactly around the block and drop it 60 px to the right; assert the block pixels appear at the
destination and the source region now matches the background colour.

**Acceptance Scenarios**:

1. **Given** a uniform background with one distinctive block, **When** the user moves a box around the
   block, **Then** the block appears at the drop point and the vacated area shows the background.
2. **Given** a moved region, **When** the user undoes once, **Then** the canvas is pixel-identical to
   before the move.
3. **Given** a source region at the canvas edge, **When** it is moved, **Then** the pasted patch is
   clipped to the canvas instead of panicking or wrapping.
4. **Given** a source rectangle with zero area, **When** the drag ends, **Then** no object is
   created.

---

### User Story 3 - Colorize / Colour Replacement (Priority: P3)

A user wants the blue call-to-action button in a screenshot to be green, or wants to recolour a
brand accent throughout the capture, without editing pixel by pixel.

**Why this priority**: Extends the existing effects pipeline, so it is small and self-contained, but
it is polish rather than a blocker.

**Independent Test**: Render a document containing a solid target-colour patch, enable Colorize with
that colour as the target and green as the replacement at full strength, and assert the patch is
green and a differently coloured patch is untouched.

**Acceptance Scenarios**:

1. **Given** an image with two distinct colours, **When** Colorize targets one of them, **Then** only
   that colour is replaced and the other is unchanged.
2. **Given** a partial strength, **When** the effect renders, **Then** the result is a blend between
   the original and the replacement rather than a hard swap.
3. **Given** Colorize is disabled, **When** the document is exported, **Then** the output matches a
   document that never had the effect.
4. **Given** a document saved with Colorize, **When** it is reopened, **Then** the effect settings are
   restored and the render is identical.

---

### User Story 4 - Quick Style Favourites and Search (Priority: P4)

A user who only ever uses two of the twenty available presets can star them and find them first, and
can type to filter the list instead of scanning chips.

**Why this priority**: Pure ergonomics on an existing surface; valuable daily, but nothing else
depends on it.

**Independent Test**: Favourite a custom preset, restart, and confirm it is still favourited and
sorted ahead of the others; type part of a preset name and confirm the bar narrows to matches.

**Acceptance Scenarios**:

1. **Given** presets for the current tool, **When** the user marks one as a favourite, **Then** it is
   drawn first and keeps a visible marker.
2. **Given** a favourited preset, **When** the editor restarts, **Then** the favourite flag is still
   set (it lives with the preset in `styles.json`).
3. **Given** the search field has text, **When** presets are shown, **Then** only those whose name
   contains the text remain, case-insensitively.
4. **Given** an old `styles.json` without the favourite field, **When** it is loaded, **Then** it
   loads without error and every preset is simply not favourited.

### Edge Cases

- **Fill seeded outside the canvas**: clamp to the nearest pixel rather than panicking.
- **Fill on a fully uniform canvas**: fills the whole visible area; must stay bounded by the canvas.
- **Fill tolerance at 0 vs 255**: 0 fills only exact matches, 255 fills everything reachable.
- **Smart Move overlapping its own destination**: source is captured before the source area is
  covered, so the paste shows the original content, not the cover colour.
- **Smart Move larger than the canvas**: the destination is clipped; the object stays valid.
- **Colorize with target == replacement**: a no-op, not a crash or an infinite loop.
- **Colorize tolerance 0**: exact match only; must not match "close" colours.
- **Very long preset search text**: no matches, empty state rather than a broken layout.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The editor MUST offer a Fill tool that creates a non-destructive object recording the
  seed point, colour, and tolerance; the base image and `.qsdoc` schema stay backward compatible.
- **FR-002**: Fill MUST flood the region reachable from the seed whose pixels are within `tolerance`
  (per-channel Chebyshev distance) of the seed pixel, on the already-composited canvas, and MUST NOT
  cross the canvas bounds.
- **FR-003**: Creating a Fill, a Smart Move, or a Colorize change MUST each be exactly one undo step.
- **FR-004**: The editor MUST offer a Smart Move tool: a dragged rectangle captures the pixels under
  it, covers that area with the surrounding background colour, and paints the captured pixels at the
  drop point.
- **FR-005**: Smart Move MUST read the source pixels before covering the source, and MUST clip both
  the cover and the paste to the canvas.
- **FR-006**: The document effects MUST gain a Colorize effect with target colour, replacement colour,
  tolerance, and strength; it MUST apply only to pixels within tolerance of the target and blend by
  strength.
- **FR-007**: Colorize MUST serialise with the document, default to absent (`None`) so existing files
  load unchanged, and render identically after a save/reload round-trip.
- **FR-008**: Quick Style presets MUST carry a persisted `favorite` flag; loading a `styles.json`
  written before the field existed MUST succeed with the flag off.
- **FR-009**: The quick-style bar MUST order favourites first and MUST offer a text filter over preset
  names, case-insensitive.
- **FR-010**: Every new surface MUST take its colours and spacing from `qs-theme`, and any new
  `qs-editor` file MUST be added to `MIGRATED` in `crates/qs-theme/tests/no_literal_colors.rs`.
- **FR-011**: Fill, Smart Move and Colorize MUST be covered by pixel tests in `qs-render`, and the
  preset changes by serialisation tests in `qs-editor`.

### Key Entities

- **Fill object**: seed point, ARGB colour, tolerance (0–255) — the reachable region is recomputed at
  render time so it stays correct if the layers below change.
- **Smart Move object**: source rectangle in document coordinates and the destination point.
- **Colorize effect**: target colour, replacement colour, tolerance, strength.
- **Quick Style**: existing preset plus a `favorite` flag.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Clicking a blank area of a real screenshot fills only that region; a pixel probe 2 px
  outside it is unchanged.
- **SC-002**: One undo removes each Fill, Smart Move, and Colorize edit completely, verified by pixel
  comparison against the pre-edit render.
- **SC-003**: A document with each new feature round-trips through `.qsdoc` (save → load → render)
  with byte-identical output.
- **SC-004**: `cargo test --workspace` stays green, and the theme literal guard covers every new UI
  file.
- **SC-005**: A `styles.json` written by the previous release loads without warnings and shows no
  preset as favourited.
