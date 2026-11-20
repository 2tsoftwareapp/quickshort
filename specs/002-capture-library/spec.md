# Feature Specification: Searchable Capture Library

**Feature Branch**: `002-capture-library`

**Created**: 2026-11-20

**Status**: Draft

**Input**: User description: "Pull latest code and load specs-kit to init and continue project — next gap from `specs/001-snagit-ui-parity/audit.md` (Library: search by date, app, tag, file type — the one remaining Critical item)."

## Context

`specs/001-snagit-ui-parity/audit.md` scored **Library & Multi-Doc** as the only remaining
**Critical** gap. Spec 001 delivered the bottom filmstrip (`crates/qs-editor/src/filmstrip.rs`, a
24-item newest-first thumbnail tray) and multi-document tabs, but a *library* is a different
object: a browsable, searchable index of everything the user has captured, carrying metadata that
survives restarts.

Today the capture folder (`~/Pictures/quickshort` by default, holding `quickshort-<unix_ts>.png`
plus `.qsdoc` files) is only ever read as a flat, undifferentiated list. Nothing records how a
capture was taken or what it was called, so "find that screenshot of the billing page from last
Tuesday" is a manual file-manager hunt.

This feature adds that index and the browser on top of it.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Browse and Search Captures (Priority: P1)

A user who has been capturing for weeks opens the Library and can find a specific capture by
typing part of its name, narrowing to a date, or filtering to one file type — then open it in the
editor.

**Why this priority**: This is the entire point of the feature and delivers value with no metadata
authoring at all — it works on the captures that already exist on disk today. Without it, the index
in P2 has nothing to be browsed through.

**Independent Test**: Populate the capture folder with a mix of `.png`, `.jpg`, `.webp`, and
`.qsdoc` files with varied timestamps and names. Open the Library, type a substring, and confirm
only matching rows remain; apply a type filter and a date range and confirm the intersection is
shown; open a row and confirm it loads in the editor.

**Acceptance Scenarios**:

1. **Given** the capture folder contains 3+ captures, **When** the user opens the Library, **Then**
   every supported file is listed newest-first with a thumbnail, name, type, and capture date.
2. **Given** the Library is open, **When** the user types `billing` in the search field, **Then**
   the list narrows to files whose name or tags contain `billing`, case-insensitively, updating as
   they type.
3. **Given** the Library is open, **When** the user filters to `.qsdoc` and a date range, **Then**
   only documents matching both filters are listed and the result count is shown.
4. **Given** a listed capture with unsaved edits in the editor, **When** the user opens it from the
   Library, **Then** the current document's unsaved edits and undo history survive in the session
   cache and are restored when it is switched back to.
5. **Given** the capture folder is empty or missing, **When** the Library opens, **Then** an empty
   state explains where captures will appear, with no error dialog.

---

### User Story 2 - Tags, Favourites, and Capture Provenance (Priority: P2)

A user annotates captures with their own tags, marks the ones they keep returning to, and can see
how each capture was taken (region, fullscreen, scrolling, recording) and which app was in front.

**Why this priority**: Turns the browser into a library. It is separately shippable: P1 is fully
useful without tags, and tags are additive metadata that never alters the image file itself.

**Independent Test**: Tag two captures, favourite one, restart the application, and confirm the
tags and favourite state are still attached to the same files. Capture a new screenshot and confirm
its mode and source app are recorded without any manual step.

**Acceptance Scenarios**:

1. **Given** a listed capture, **When** the user adds the tag `invoice`, **Then** the tag appears on
   that entry immediately and the entry is findable by searching `invoice`.
2. **Given** tagged captures, **When** the application is restarted, **Then** all tags and
   favourites are still present and attached to the same file paths.
3. **Given** a capture taken while the Library is closed, **When** the Library is next opened,
   **Then** the new capture is present with its capture mode recorded, and its source app when the
   platform can report it.
4. **Given** a capture whose file was deleted outside Quickshort, **When** the Library refreshes,
   **Then** the entry disappears and no stale row remains.
5. **Given** a tag is removed from every capture, **When** the tag filter list is shown, **Then**
   the orphaned tag no longer appears as a filter option.

---

### User Story 3 - Library Housekeeping (Priority: P3)

A user prunes a batch of unwanted captures and can recover from a damaged or missing index without
losing their tags.

**Why this priority**: Keeps the library from degrading over time. Valuable, but the product is
usable and correct without it.

**Independent Test**: Multi-select five captures, delete them, and confirm they are moved to the OS
trash (not permanently erased) after an explicit confirmation. Corrupt the index file, reopen the
Library, and confirm the list rebuilds from disk with a visible notice.

**Acceptance Scenarios**:

1. **Given** several selected captures, **When** the user confirms deletion, **Then** the files are
   moved to the operating system's trash and the rows disappear.
2. **Given** a selection and a cancelled confirmation, **Then** nothing is deleted.
3. **Given** a corrupt or unreadable index file, **When** the Library opens, **Then** the index is
   rebuilt from a directory scan and the user is told that annotations (tags/favourites) could not
   be recovered.
4. **Given** an entry is selected, **When** the user reveals it in the file manager, **Then** the
   containing folder opens with the file selected.

---

### Edge Cases

- **Capture folder unreadable** (permissions): the Library shows the reason and offers the path,
  rather than an empty list that implies there are no captures.
- **Large folder** (10,000+ files): the scan runs off the UI thread; the list renders progressively
  and the window stays responsive.
- **Unsupported files** in the folder (`.txt`, `.DS_Store`, partial downloads): ignored silently and
  never listed or indexed.
- **Read-only capture folder**: browsing works; a write attempt (tag/favourite/delete) reports that
  the folder is read-only instead of silently discarding the change.
- **Two captures with the same timestamp** (`quickshort-<ts>.png` and `-1.png` suffix): both listed
  as independent entries.
- **A capture tagged, then renamed outside Quickshort**: the entry is treated as a deletion plus a
  new file, so tags do not silently attach to the wrong image.
- **Unicode and very long tags**: accepted and matched; display truncates with a tooltip rather than
  breaking layout.
- **`.qsdoc` with unreadable thumbnail**: the row still lists with a type placeholder instead of
  vanishing or blocking the list.
- **Index newer than the build understands**: refuse to rewrite it silently; surface the same
  "written by a newer version" class of error used for documents.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The Library MUST list every supported file (`.png`, `.jpg`, `.jpeg`, `.webp`,
  `.qsdoc`) in the configured capture folder, newest-first by default, with thumbnail, name, type,
  and capture date.
- **FR-002**: Users MUST be able to search by free text, matching file name and tags
  case-insensitively, updating incrementally as they type.
- **FR-003**: Users MUST be able to filter by file type and by capture date (day or range), and
  filters MUST compose with search.
- **FR-004**: Users MUST be able to change sort order (newest, oldest, name) without rescanning the
  folder or re-decoding thumbnails.
- **FR-005**: Opening an entry MUST NOT discard unsaved edits. It routes through
  `switch_to_path`, which parks the active document's undo/zoom/pan/unsaved state in the session
  cache before loading the target — the same lossless path the filmstrip uses. The Save/Discard/
  Cancel dialog stays where edits are actually destroyed: window close and file replacement.
- **FR-006**: Tags and favourite state MUST persist across restarts, keyed to the capture file, in
  a versioned index stored inside the capture folder — never by modifying the image files.
- **FR-007**: The index MUST be written atomically (temporary file plus rename) so an interrupted
  write cannot leave a truncated index.
- **FR-008**: New captures MUST record capture mode (region, fullscreen, scrolling, recording) and
  source application name at capture time when available; unavailable fields are recorded as
  unknown rather than guessed. The four modes above are the modes the capture path can actually
  emit today — window capture does not exist in this codebase and is not introduced here.
- **FR-009**: A missing, corrupt, or version-incompatible index MUST NOT crash or silently
  overwrite: the library rebuilds by scanning and reports which metadata was unrecoverable.
- **FR-010**: Entries whose files no longer exist MUST be pruned from the index on refresh; tags for
  a path that reappears with different content MUST NOT be resurrected blindly.
- **FR-011**: Directory scanning, thumbnail decoding, and index I/O MUST NOT run on the UI thread or
  on a capture-overlay frame.
- **FR-012**: Deletion MUST move files to the operating system trash, require explicit confirmation,
  and support multi-selection. Permanent deletion is out of scope.
- **FR-013**: Every colour and spacing value in the Library UI MUST come from `qs-theme`, and the
  new files MUST be added to the `MIGRATED` allowlist in
  `crates/qs-theme/tests/no_literal_colors.rs`.
- **FR-014**: The feature MUST be local-only: no network access, no telemetry, no cloud index.
- **FR-015**: The Library MUST be reachable from the editor's existing filmstrip region and via its
  own keyboard shortcut, without regressing filmstrip behaviour.

### Key Entities

- **Library Entry**: one capture file — path, display name, kind (image vs. document), byte size,
  modification time, image dimensions when known, capture mode, source app, tags, favourite flag,
  thumbnail availability.
- **Library Index**: the persisted collection of entries plus a format version, stored in the
  capture folder, atomically rewritten.
- **Tag**: a user-defined label, unique case-insensitively, referenced by any number of entries.
- **Filter State**: current search text, type filter, date filter, sort order, and selected tag —
  session state, not persisted.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With 2,000 captures in the folder, the Library shows its first results within 300 ms
  and thumbnails fill in progressively; no single UI frame is blocked longer than 16 ms.
- **SC-002**: Typing in the search field re-filters a 10,000-entry library in under 50 ms per
  keystroke.
- **SC-003**: After a restart, 100% of tags and favourites reappear on the same files, verified on a
  folder of at least 50 tagged captures.
- **SC-004**: A capture taken with the Library closed is listed, with its mode, on the next open —
  100% of the time for the four capture modes; source app is populated on platforms that report it
  and shows nothing elsewhere.
- **SC-005**: `cargo test --workspace` stays green, including the theme literal guard covering the
  new UI files, and the index round-trip test covers a legacy-version fixture.
- **SC-006**: Deleting N selected captures moves all N to the trash and leaves zero stale rows.

## Assumptions

- Captures live in one flat folder configured by `Config::save_dir` (default
  `~/Pictures/quickshort`); recursive subfolder support is out of scope.
- The existing `.qsdoc` reader and `read_thumb` remain the source of document thumbnails and
  dimensions; this feature does not change the document format.
- Source-application detection is new work and platform-dependent: macOS can report the frontmost
  application through the already-linked `NSWorkspace`; Linux and Windows record `unknown` in this
  feature. Where the platform cannot report it, the field is unknown and the UI shows nothing
  rather than a misleading value.
- Window capture is not a supported mode in this codebase today, so the mode vocabulary is region,
  fullscreen, scrolling, and recording.
- The Library is a view inside `qs-editor`, not a separate process or window, so it reuses the
  existing theme, fonts, and document-switching guards.
- Thumbnails are derived on demand and cached in memory for the session; a persistent thumbnail
  cache is out of scope for this feature.
- Trash integration uses the platform's own trash mechanism; a "recently deleted" in-app bin is out
  of scope.
