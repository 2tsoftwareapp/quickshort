# Tasks: Searchable Capture Library

**Input**: Design documents in `/specs/002-capture-library/` (`spec.md`, `plan.md`)

## Phase 1: `qs-library` Core (Priority: P1, backend)

- [x] **T001** Add `crates/qs-library` to the workspace (`Cargo.toml` members + workspace deps) with `entry.rs` defining `LibraryEntry`, `EntryKind`, `CaptureMode` (Region/Fullscreen/Scrolling/Recording/Unknown), all `Serialize`/`Deserialize`.
- [x] **T002** Implement `index.rs`: `INDEX_VERSION = 1`, `LibraryIndex { version, entries: BTreeMap<String, LibraryEntry> }`, `load()` tolerating a missing file, rejecting a newer `version` with the document-format wording pattern, and `save()` via temp file + rename.
- [x] **T003** [P] Implement `scan.rs`: `is_supported()` for `png/jpg/jpeg/webp/qsdoc`, directory walk producing fresh entries with size, mtime, kind, and dimensions (`image::image_dimensions` for images, `qs_doc::qsdoc::read_doc` for documents).
- [x] **T004** Implement index/scan merge: prune entries whose file vanished; preserve tags, favourite, mode, and source app only when `(size, mtime)` are unchanged, dropping them otherwise (FR-010).
- [x] **T005** [P] Implement `query.rs`: `FilterState { search, kind, date, tag, sort }` and pure `apply(&[LibraryEntry]) -> Vec<&LibraryEntry>`; case-insensitive name/tag search, composable filters, sorts (Newest/Oldest/Name). Include a dependency-free civil-date helper for day bucketing.
- [x] **T006** Implement `trash.rs`: `delete_all(paths)` moving files to the OS trash via the `trash` crate, returning per-path results; never falls back to `remove_file`.
- [x] **T007** Unit tests: index round-trip, forward-version rejection, missing-file prune, tag preservation vs. drop on content change, filter composition, sort stability, `is_supported`.

## Phase 2: Library View (Priority: P1, UI)

- [x] **T008** Create `crates/qs-editor/src/library.rs` rendering the list with thumbnails, name, kind, date, tag chips, and favourite marker; empty state names the capture folder; unreadable folder shows the reason.
- [x] **T009** Add the search field, type filter, date-range presets, sort control, and live result count; re-filter without rescanning or re-decoding thumbnails.
- [x] **T010** Run scan and thumbnail decode off the UI thread over an `mpsc` channel (filmstrip pattern); render progressively and show a scanning indicator.
- [x] **T011** Wire `library_open` state, a keyboard shortcut, a toolbar button, and a filmstrip affordance in `app.rs` / `toolbar.rs` / `filmstrip.rs`; reuse `qs_library::is_supported` in `filmstrip.rs` and delete its local copy.
- [x] **T012** Route "open" through the existing unsaved-changes guard before replacing the active document.
- [x] **T013** Add `qs-editor/src/library.rs` to `MIGRATED` in `crates/qs-theme/tests/no_literal_colors.rs`; verify zero literals.

## Phase 3: Capture Provenance (Priority: P2)

- [x] **T014** Add a `record_capture(dir, path, mode, source_app)` entry point in `qs-library` that upserts one entry without a full rescan (atomic read-modify-write).
- [x] **T015** Call it from `qs-app` after every successful save (`actions::save`, `save_to_dir`), passing the mode known at that call site; unknown flows record `Unknown`, never a guess.
- [x] **T016** macOS: frontmost application name helper in `crates/qs-app/src/chrome/macos.rs` via the already-linked `NSWorkspace`; other platforms return `None`.

## Phase 4: Tags & Favourites (Priority: P2)

- [x] **T017** Inline tag editor and favourite toggle in `library.rs`, writing through the index with an error surfaced on failure (read-only folder case).
- [x] **T018** Tag filter chips built from tags actually present; orphaned tags disappear when the last entry drops them.
- [x] **T019** Restart-persistence test: write tags/favourites to a temp index, reload, assert they reattach to the same file names.

## Phase 5: Housekeeping (Priority: P3)

- [x] **T020** Multi-select with shift/cmd-click, batch actions bar, and a confirmation dialog stating the count and that files go to the trash.
- [x] **T021** "Reveal in file manager" action (macOS `open -R`, Linux `xdg-open` on the parent, Windows `explorer /select`).
- [x] **T022** Corrupt/missing index → rebuild by scanning and show a visible notice that annotations could not be recovered; forward-version index → refuse to rewrite and say so.

## Verification Gate (Run After Each Phase)

- `cargo check --workspace --all-targets`
- `cargo test --workspace`
- Library view verified at `720×600`, `1000×700`, `1280×800`, `1600×1000` in Dark & Light themes.

## Completion Notes

All 22 tasks landed on `main` in one change set. Verification actually run:

- `cargo check --workspace --all-targets` — clean, zero warnings added.
- `cargo test --workspace` — green, including 22 new `qs-library` tests and the extended
  `crates/qs-theme/tests/no_literal_colors.rs` guard (which now covers `qs-editor/src/library.rs`).
- `cargo test -p qs-editor --test uat_e2e` — 3 passing, the new one driving the real Library view
  over a temp folder of 6 captures plus junk files at `720×600`, `1000×700`, `1280×800`, and
  `1600×1000` in Dark and Light.

Deliberate scope notes:

- **T012** resolves to the existing lossless `switch_to_path` session-cache path rather than a
  modal guard; the modal stays for window close and file replacement, where edits are actually
  destroyed (see spec FR-005).
- **T019** is proven at the index layer (`index_round_trips_through_disk` writes, reloads, and
  reattaches tags/favourite/mode/source app), which is exactly the restart path.
- Scrolling and recording captures carry their mode through the shelf item (`ShotKind`); shelf saves
  of a `Saved` shot and quick-access saves record `Unknown` rather than guessing region vs.
  fullscreen.
