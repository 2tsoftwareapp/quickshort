# Feature Specification: OCR Line Geometry

**Feature Branch**: `004-ocr-line-boxes`

**Created**: 2026-11-20

**Status**: Draft — Phases 1–4 implemented (Linux/Windows geometry blocked on toolchain)

**Input**: The gap declared out of scope in `specs/003-annotation-parity/plan.md`: `qs-ocr` exposes
`extract_text` only, so Smart Move needs a user-dragged box and there is no way to copy the text
inside a region.

## Context

`crates/qs-ocr` answers one question: *what text is in this image?* It returns a `String` with lines
joined by `\n` and throws the geometry away, even though the macOS backend already walks Vision
observations that carry a normalized bounding box and a confidence score.

That missing geometry is why feature 003 shipped Smart Move as drag-a-box instead of propose-a-box,
and why "copy the text in this rectangle" is impossible today. This feature adds the geometry first
(Phase 1), then spends it on the editor (Phases 2–3).

Phase 1 is deliberately internal: it adds an API and tests, and changes nothing a user sees. Its
value is that Phases 2–3 become small and independently shippable.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Line rectangles from the OCR engine (Priority: P1)

A caller that already extracts text can instead ask for the same lines with the pixel rectangle each
one occupies, so it can act on *where* the words are.

**Why this priority**: Nothing else in this feature can be built without it, and it is the only part
that touches a platform API.

**Independent Test**: Call the new entry point on an image containing two known lines and assert the
returned lines carry text in reading order and rectangles that fall inside the image, with the first
line's rectangle above the second's.

**Acceptance Scenarios**:

1. **Given** an image with recognized text, **When** the caller requests line geometry, **Then** each
   line carries its text, a pixel rectangle with the origin at the top-left, and a confidence value.
2. **Given** a successful result, **Then** the texts joined by `\n` equal what `extract_text`
   returns for the same image and language — the two entry points cannot drift.
3. **Given** a platform whose engine cannot report geometry, **When** geometry is requested, **Then**
   the call fails with a clear message instead of returning an empty list that reads as "no text".
4. **Given** an image whose engine finds nothing, **When** geometry is requested, **Then** an empty
   list is returned *(distinct from the unsupported-platform case above)*.

---

### User Story 2 - Smart Move proposes the box (Priority: P2, later phase)

With the Smart Move tool active, hovering a recognized line highlights it, so a click takes that line
as the source rectangle instead of requiring a precise drag.

**Why this priority**: It is the payoff for Phase 1 on the tool I had to ship manual, but it is
useless without Phase 1 and can ship on its own afterwards.

**Independent Test**: Load a screenshot with a known text block, hover it with Smart Move active, and
confirm the proposed box matches the OCR line rectangle within a pixel.

**Acceptance Scenarios**:

1. **Given** text was recognized, **When** the pointer rests over a line, **Then** a proposal box is
   drawn around that line.
2. **Given** a proposal is shown, **When** the user clicks, **Then** that rectangle becomes the Smart
   Move source.
3. **Given** no text is recognized there, **Then** the manual drag still works unchanged.

---

### User Story 3 - Copy the text inside a region (Priority: P3, later phase)

A user drags a rectangle and gets the text inside it on the clipboard, in reading order.

**Why this priority**: The clearest user-visible use of geometry, but it needs Phase 1 only and is
otherwise independent.

**Independent Test**: Drag a rectangle covering one of two text lines and confirm the clipboard holds
exactly that line's text.

**Acceptance Scenarios**:

1. **Given** two recognized lines, **When** the user drags a box over one, **Then** only that line is
   copied.
2. **Given** a box covering both lines, **Then** both are copied in reading order, one per line.
3. **Given** a box over empty space, **Then** nothing is copied and the UI says so.

### User Story 4 - Find in capture (Priority: P4, this phase)

A user remembers the setting was called "Shortcuts" but not where it sits: they press ⌘F, type it,
and the canvas highlights every matching line and jumps to the first one.

**Why this priority**: It is the cheapest way to spend geometry — no new engine work, no new
platform surface — and it turns the recognized text into navigation.

**Independent Test**: Open a capture with known text, search for a word that appears twice, and
confirm two highlights appear, the first one is framed and centered, and Next moves to the second.

**Acceptance Scenarios**:

1. **Given** recognized text, **When** the user searches a word, **Then** every line containing it is
   highlighted and the current match is framed.
2. **Given** several matches, **When** the user presses Next, **Then** the next match becomes current
   and the canvas centres on it, wrapping at the end.
3. **Given** a search with no matches, **Then** the dialog says so and nothing is highlighted.
4. **Given** the recognition is still running, **Then** the dialog says it is reading rather than
   claiming there are no matches.
5. **Given** the dialog is open, **When** the user presses Escape, **Then** it closes and the
   highlights disappear.

### Edge Cases

- **Rect at the image edge**: clamping keeps it inside `0..=width` / `0..=height`.
- **Zero-size observation**: kept as a degenerate rect rather than dropped, so text is never silently
  lost.
- **Rotated or upside-down text**: reported as the engine sees it; no attempt to un-rotate.
- **Very many lines**: order preserved exactly as the engine returns it.
- **Engine unavailable on this machine**: the existing "not installed" path still applies; geometry
  must not turn that into a panic.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `qs-ocr` MUST expose line-level geometry: text, pixel rectangle (top-left origin), and
  confidence for every recognized line.
- **FR-002**: The rectangle MUST be expressed in the coordinates of the image passed in, so a caller
  can draw it over that image without knowing the engine's normalization.
- **FR-003**: The macOS backend MUST fill geometry from Vision's observation bounding boxes; Vision
  reports a normalized rect with a bottom-left origin, so the conversion MUST be a single tested
  helper rather than inline arithmetic at the call site.
- **FR-004**: `extract_text` MUST keep its current signature and output, and MUST be expressed in
  terms of the new entry point on platforms that support geometry so the two cannot drift.
- **FR-005**: A platform without geometry support MUST fail explicitly, never return an empty list.
- **FR-006**: `qs-ocr` MUST NOT gain a dependency on `qs-doc`, `qs-edit`, or any UI crate; the type
  stays self-contained (Constitution II keeps it a leaf).
- **FR-007**: Existing OCR callers (clipboard extraction, the Settings language badges) MUST be
  unaffected.
- **FR-008**: Searching MUST match recognized line text case-insensitively, highlight every match,
  and treat an empty query as "match nothing" rather than highlighting everything.
- **FR-009**: Stepping through matches MUST wrap and MUST pan the canvas so the current match is
  visible; a search with no matches MUST say so instead of leaving the previous highlights up.

### Key Entities

- **OcrLine**: recognized text for one line, its pixel rectangle in the source image, and a 0–1
  confidence where the engine provides one.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The normalized→pixel conversion is covered by unit tests for a top-left line, a
  bottom-left line, a full-image rect, and a rect that overruns the edge.
- **SC-002**: `cargo test --workspace` stays green and `cargo check --workspace --all-targets` adds no
  warnings.
- **SC-003**: `extract_text` output is unchanged for every existing caller (no call site edited).
- **SC-004**: Phase 2 can place a box over a line using only the public API, with no arithmetic at
  the call site.
- **SC-005**: Searching a word that appears in a real capture highlights exactly the lines that
  contain it, verified against the repo screenshot.

## Assumptions

- macOS is the first platform: Vision reports geometry natively and the crate already links it.
- Linux (tesseract) and Windows (WinRT OCR) also expose geometry, but neither target can be compiled
  or exercised on this machine, so shipping untested bindings there is worse than an explicit error.
  They get their own task in Phase 3.
- Coordinates are physical pixels of the image handed to the engine; callers scale to logical
  document units themselves, exactly as the overlay already does for captures.
