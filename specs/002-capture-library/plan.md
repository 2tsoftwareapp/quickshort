# Implementation Plan: Searchable Capture Library

**Branch**: `002-capture-library` | **Date**: 2026-11-20 | **Spec**: [spec.md](./spec.md)

## Summary

Add a persistent, searchable index of everything in the capture folder, plus a Library view in
`qs-editor` to browse it. Three pieces:

1. A new non-UI crate `qs-library` that owns the entry model, the on-disk index, directory
   scanning, filtering/sorting, and trash deletion.
2. `qs-app` records capture provenance (mode, source app) at the moment a capture is written.
3. `qs-editor` gains a Library view, reusing the existing `settings_open` panel pattern and the
   filmstrip's directory resolution, with tags, favourites, filters, and bulk delete.

Everything stays local: one JSON index inside the capture folder, atomic rewrite, no network.

## Technical Context

- **Language/Version**: Rust, workspace `resolver = "2"`, crates at edition 2021/2024
- **Primary Dependencies (new)**: `qs-library` (new crate, no UI deps); `trash = "5"` for the
  OS trash (macOS Finder trash, freedesktop on Linux, Recycle Bin on Windows). Reuses existing
  `serde`/`serde_json`, `image`, `qs-doc`, `qs-theme`.
- **Existing Dependencies Reused**: `qs_doc::qsdoc::read_thumb` / `read_doc` (already annotated
  "Document only, no pixels (library indexing)"), `qs_editor::filmstrip` directory resolution,
  `qs-editor`'s `settings_open` panel pattern, `qs_theme` tokens.
- **Storage**: `<capture dir>/.qs-library.json` — `{version, entries[]}`, written via temp file +
  rename. Image files are never modified.
- **Testing**: `cargo test --workspace`; new unit tests for index round-trip, legacy/forward index
  versions, filter/sort composition, stale-entry pruning, and trash-confirmation gating; the theme
  literal guard extended to the new UI files.
- **Target Platform**: macOS (primary, source-app available), Linux, Windows (source app unknown).
- **Constraints**:
  - Scanning, thumbnail decode, and index I/O off the UI thread (Constitution: no blocking work on
    an overlay/capture frame).
  - Zero colour/spacing literals in the new UI files (Constitution I).
  - `.qsdoc` format untouched (Constitution III).

## Constitution Check

| Principle | How this plan complies |
|---|---|
| **I. Design Token SSOT** | The Library view uses `qs_theme::pal()`, `space`, `radius`, `font` only. `crates/qs-editor/src/library.rs` is added to `MIGRATED` in `crates/qs-theme/tests/no_literal_colors.rs` in the same task that creates it. |
| **II. Layered Crates** | `qs-library` depends only on `qs-doc`, `image`, `serde`, `trash` — no `egui`/`eframe`, no `qs-edit`/`qs-editor`. Arrows added are `qs-editor → qs-library` and `qs-app → qs-library`, i.e. UI depends on a lower layer. **No arrow is inverted**, so no amendment is required; the crate is recorded in the constitution's diagram as a PATCH clarification. |
| **III. `.qsdoc` Compatibility** | The document format is not touched. The *index* follows the same discipline: it carries its own `version`, reads older versions, and refuses (with the existing wording pattern) to rewrite an index written by a newer build. |
| **IV. Reversible/Guarded Actions** | The Library never mutates a document, so undo semantics are untouched. Deletion is guarded: explicit confirmation, multi-select, OS trash only (never `remove_file`). A failed index write surfaces an error instead of silently dropping tags. |
| **V. Verification Gate** | `cargo check --workspace --all-targets` + `cargo test --workspace`, plus the Library view checked at 720×600 / 1000×700 / 1280×800 / 1600×1000 in Dark and Light. |

**Deviations**: none.

## Project Structure

### Documentation (this feature)

```text
specs/002-capture-library/
├── spec.md      # Prioritized user stories, FRs, success criteria
├── plan.md      # This file
└── tasks.md     # Phased task breakdown
```

### New crate

```text
crates/qs-library/
├── Cargo.toml
└── src/
    ├── lib.rs      # re-exports, `is_supported`
    ├── entry.rs    # LibraryEntry, EntryKind, CaptureMode
    ├── index.rs    # LibraryIndex: load/save (atomic), version guard, upsert/prune
    ├── scan.rs     # directory scan -> entries (sync; callers run it off-thread)
    ├── query.rs    # FilterState + search/filter/sort (pure)
    └── trash.rs    # move_to_trash(...) wrapper + confirmation contract
```

### Modified files

| File | Change |
|---|---|
| `Cargo.toml` | Add `crates/qs-library` member + workspace dependency. |
| `crates/qs-editor/Cargo.toml` | Add `qs-library`. |
| `crates/qs-app/Cargo.toml` | Add `qs-library`. |
| `crates/qs-app/src/actions.rs` | After a successful `save()`, record the file in the index with mode. |
| `crates/qs-app/src/host.rs` | Pass capture mode + (macOS) frontmost app through to the record call. |
| `crates/qs-app/src/chrome/macos.rs` | Small helper: frontmost application name via the already-linked `NSWorkspace`. |
| `crates/qs-editor/src/library.rs` | **New**: the Library view (search, filters, grid/list, tag editor, delete). |
| `crates/qs-editor/src/app.rs` | `library_open` state, toggle + shortcut, draw the view, route open/delete. |
| `crates/qs-editor/src/filmstrip.rs` | Reuse `qs_library::is_supported`; add the "Open Library" affordance. |
| `crates/qs-editor/src/toolbar.rs` | Library toggle button (styled, tokens only). |
| `crates/qs-theme/tests/no_literal_colors.rs` | Add `qs-editor/src/library.rs` to `MIGRATED`. |
| `.specify/memory/constitution.md` | PATCH 1.0.1: record `qs-library` in the layering diagram. |

## Design Decisions

**Why a new crate, not `qs-doc`.** Both `qs-app` (which does not depend on `qs-editor`) and
`qs-editor` need the index. `qs-doc` is the document *format* crate; a collection index, a
directory walker, and trash handling do not belong in it. A shared lower-layer crate is the
smallest structure that satisfies both consumers without inverting Principle II.

**Index keyed by file name, not an opaque id.** Tags must survive a restart and follow the file that
is actually in the folder. Keying by file name inside the capture dir means a renamed file is
treated as a new entry (spec edge case) rather than silently inheriting another image's tags.

**Metadata is additive and lossy-tolerant.** Missing fields are `None`/`Unknown`. The UI renders
nothing for unknown values — it never fabricates a source app or mode.

**Thumbnails stay session-cached.** `image` decode and `qs_doc::read_thumb` results live in an
in-memory map keyed by (path, mtime). A persistent thumbnail cache is explicitly out of scope, so
the feature adds no cache-invalidation surface.

**Trash, not delete.** `trash::delete` is the only removal path. The confirmation dialog states how
many files move and that they go to the trash.

**Provenance is snapshotted before the work is spawned.** Captures are stitched and encoded on a
worker thread, and the frontmost application can only be read from the UI thread. So the caller
assembles `Provenance { mode, source_app }` while it still owns the trigger and hands it to
`dispatch`; a worker that cannot see the window server records no source app rather than a guess.

## Phased Delivery

1. **Phase 1 — `qs-library` core (P1 backend):** entry model, index load/save with version guard,
   scan, `is_supported`, query (search/type/date/sort). Unit tests including a legacy fixture and a
   forward-version rejection.
2. **Phase 2 — Library view (P1 UI):** `library.rs` with search field, type filter, date filter,
   sort, list/grid with thumbnails, empty/permission states, open-in-editor through the existing
   dirty guard, shortcut + toolbar/filmstrip entry points. Add to `MIGRATED`.
3. **Phase 3 — Provenance (P2):** `qs-app` builds a `Provenance { mode, source_app }` on the UI
   thread *before* the capture work is spawned (AppKit cannot be asked from a worker), passes it into
   `dispatch`, and records the resulting file; new captures appear in the Library with their mode.
4. **Phase 4 — Tags & favourites (P2):** inline tag editor, favourite toggle, tag filter chips,
   persistence through the index, restart test.
5. **Phase 5 — Housekeeping (P3):** multi-select, trash delete with confirmation, reveal in file
   manager, index rebuild on corrupt/missing file with a visible notice, stale pruning.

Each phase ends with the Constitution V gate before the next begins.
