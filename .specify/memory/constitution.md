# Quickshort Constitution

<!--
Sync Impact Report
- Version change: (unratified template) → 1.0.0 → 1.0.1
- Ratified: 2026-11-20
- 1.0.1 (PATCH): added `qs-library` to the layering diagram for feature 002. It sits
  beside `qs-doc` (reads documents for dimensions) and is consumed by `qs-editor` and
  `qs-app`; no dependency arrow is inverted, so no principle changed.
- Scope: the whole Rust workspace (`crates/*`), the `.qsdoc` format, and every spec under `specs/`.
- Source of truth for every principle below is existing code, not aspiration:
  qs-theme/tests/no_literal_colors.rs, crates/qs-doc/src/lib.rs (DOC_VERSION),
  crates/qs-edit/src/lib.rs (undo/redo), per-crate Cargo.toml dependency graphs.
-->

## Core Principles

### I. Design Token Single Source of Truth

Every colour, spacing, radius, and font value a user can see comes from `crates/qs-theme`
(`pal()`, `space`, `radius`, `font`). No hex literal, no `Color32::from_rgb(...)` with numeric
arguments, and no ad-hoc magic spacing in UI crates.

This is enforced, not advisory: `crates/qs-theme/tests/no_literal_colors.rs` holds a `MIGRATED`
allowlist of files that are already tokenised, and adding a surface to `MIGRATED` is part of
delivering it. A literal may stay only when it is not a UI colour (bit mask, document data) and
says so inline with `// allow-hex: <reason>`. Test modules are exempt.

**Rationale**: the app ships Dark and Light themes plus a Liquid Glass surface language; a stray
literal is a light-mode bug waiting to happen. Migrating file-by-file keeps the guard green while
the surface area grows.

### II. Layered Crate Boundaries

The dependency direction is fixed and one-way:

```
qs-core, qs-theme, qs-ocr            (leaves — no sibling deps)
      ↑
    qs-doc                           (document model, .qsdoc I/O)
      ↑                      ↑
   qs-render            qs-library   (capture index, search, provenance, trash)
      ↑                      ↑
   qs-edit                   │
      ↑                      │
   qs-editor  ───────────────┘       qs-app (host, tray, settings)
```

`qs-doc`, `qs-render`, and `qs-edit` MUST NOT depend on `egui`, `eframe`, or any windowing crate.
Rendering stays a pure function of document state so it can be tested headlessly and reused by the
overlay, the app, and export paths alike.

New capability lands in the lowest layer that can express it, and only surfaces in the UI layer
when a user needs to touch it. A change that would invert an arrow above needs a constitution
amendment, not a workaround.

### III. Document Format Backward Compatibility

`.qsdoc` is a user-owned artifact on disk. `DOC_VERSION` is currently `4` and only increases.

- Readers MUST accept every older version and migrate it in memory.
- A file written by a newer build MUST fail loudly with the existing "written by a newer
  quickshort" error. Never guess at an unknown field layout or silently drop data.
- A format change ships with a round-trip test at the new version and at least one legacy-version
  fixture, in the same change.

### IV. Reversible, Guarded User Actions

Every mutating user action produces exactly one undo entry — an effects draft applied as a whole
is one step, not one step per pixel touched (`qs-edit` history is the authority here). A tool that
cannot be expressed as one reversible step is not finished.

Operations that can destroy a large share of the user's content carry an explicit safety guard
(e.g. `Cutout` refuses `>= 90%` strip removal) and must report why they refused, rather than
degrading into a silent no-op.

### V. Verification Gate (NON-NEGOTIABLE)

A task is done only when all of the following hold for the change surface:

1. `cargo check --workspace --all-targets` — zero errors; new warnings are fixed, not suppressed.
2. `cargo test --workspace` — green, including the theme literal guard and `.qsdoc` round-trips.
3. For any UI change: verified at `720×600`, `1000×700`, `1280×800`, and `1600×1000` in both Dark
   and Light themes, with no overlap, clipping, or unreadable contrast.

Checks run from cheapest to most expensive (diff review → targeted test → workspace gate), and
the narrowest test that proves the fix is the one that must be added.

## Additional Constraints

**Technology stack**: Rust workspace (`resolver = "2"`), `eframe`/`egui` for UI, `tiny-skia` for
rendering, `image` for codecs. Platform targets are macOS (primary), Linux (Wayland and X11), and
Windows. A platform-only affordance must degrade visibly on the other platforms rather than
compile-time disappearing into a broken layout.

**Capture path latency**: the capture overlay is the hot path. No blocking I/O, OCR, thumbnail
decode, or directory scan may run on the frame that renders the overlay — background work reports
back through state the UI polls.

**Local-first**: captures, tags, and library metadata stay on the user's machine. Network use is
limited to explicit, user-initiated actions (relay share, update check). No telemetry.

**No speculative abstraction**: reuse the existing module before adding a sibling helper, trait, or
crate. A new crate needs a layer justification under Principle II.

## Development Workflow

Work is spec-driven under `/specs/NNN-feature/`, in this order, with a review gate between phases:

1. `spec.md` — prioritized user stories, each independently testable, plus functional requirements.
2. `plan.md` — technical context, constitution check, and the file-level change surface.
3. `tasks.md` — phased, atomic tasks with the verification gate per phase.
4. Implementation — tasks checked off only when Principle V passes.

Deviations from this constitution are recorded in the feature's `plan.md` "Constitution Check"
section with the tradeoff stated; unresolved ones block the plan gate.

## Governance

This constitution supersedes local habit and convenience. Amendments require a written rationale,
a version bump, and a migration note for any spec or code already relying on the old wording.

- **MAJOR**: a principle is removed or its direction reversed (e.g. changing the crate layering).
- **MINOR**: a principle or constraint is added or materially expanded.
- **PATCH**: clarification, wording, or a link fix with no behavioural change.

Every `/plan` and code review MUST verify compliance with Principles I–V. Complexity that violates
a principle is rejected, not grandfathered. Runtime guidance for day-to-day development lives in
`README.md`; this document governs when the two disagree.

**Version**: 1.0.1 | **Ratified**: 2026-11-20 | **Last Amended**: 2026-11-20
