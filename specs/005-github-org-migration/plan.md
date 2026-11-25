# Implementation Plan: GitHub Identity Migration Consistency

**Branch**: `005-github-org-migration` (artifact branch name; implementation may happen on `main` or a short-lived fix branch) | **Date**: 2026-11-25 | **Spec**: [spec.md](./spec.md)

## Summary

Make `2tsoftwareapp` the single canonical public GitHub identity for Quickshort by replacing the remaining active `truongnat` references in runtime links, the report relay, packaging metadata, release automation, funding metadata, documentation, demos, and the video configuration. Preserve behavior and payload shapes everywhere; this is an identity/metadata migration, not a product refactor.

The implementation is deliberately direct: update the constants and copy at the existing call sites, add a small repeatable audit script that fails on unallowlisted legacy identity references in active surfaces, and validate with the normal Rust verification gate plus targeted static checks and live destination probes. No new runtime dependency, crate, network behavior, persistent format, or package-application ID is introduced.

## Technical Context

**Language/Version**: Rust stable workspace; JavaScript for Cloudflare Workers; shell for packaging/audit; YAML for GitHub workflows/package manifests; static HTML/JS and TypeScript configuration for public docs/demos.

**Primary Dependencies**: Existing dependencies only. No new runtime or build dependency.

**Storage**: N/A — no persistent user data or document format changes.

**Testing**: `cargo check --workspace --all-targets`; `cargo test --workspace`; `scripts/check-github-identity.sh`; `node --check relay/report/worker.js` when Node is available; `bash -n packaging/linux/package.sh`; live HTTP/API probes for canonical public destinations; one controlled relay validation after deployment.

**Target Platform**: macOS, Linux, Windows for app metadata; Cloudflare Workers/Pages and GitHub for hosted surfaces.

**Project Type**: Rust desktop application workspace plus static documentation site, packaging templates, and serverless relay workers.

**Performance Goals**: No runtime performance change. Validation commands should complete within the existing normal local verification time.

**Constraints**: Preserve local-first/no-telemetry posture; do not commit secrets; do not change report payload shape, labels, rate limits, update signing, capture/OCR behavior, or Flatpak's stable application ID; preserve the user's existing uncommitted edits to `packaging/linux/package.sh` and `relay/report/README.md`.

**Scale/Scope**: 20 existing files plus one new audit script. Broader audit findings (`rfd` alignment, large-file refactoring, `.expect()` in stitch dispatch, CI expansion, recording architecture, `df` portability) are explicitly out of scope; the only extra file is the minimal `qs-app` dev-dependency unblocker required for the pre-existing test target to compile.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | How this plan complies |
|---|---|
| **I. Design Token SSOT** | No visual design, colour, spacing, radius, or font changes. URL constants do not affect theme tokens. |
| **II. Layered Crates** | No crate dependency or layer changes. The only Rust edits are string literals in `qs-app`, which already owns host/settings actions. |
| **III. `.qsdoc` Compatibility** | `.qsdoc` is untouched; no document schema, migration, or persistence behavior changes. |
| **IV. Reversible Actions** | No user-content mutation is added. Undo/redo behavior is untouched. |
| **V. Verification Gate** | Rust changes are covered by `cargo check --workspace --all-targets` and `cargo test --workspace`. The migration audit script is the narrowest test proving the fix. There is no layout-affecting UI change, so the four-viewport UI check is not applicable; if implementation reveals URL text is visibly rendered, run the Settings window check at the constitution viewports. |

**Deviations**: none.

## Project Structure

### Documentation (this feature)

```text
specs/005-github-org-migration/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── identity-contract.md
├── checklists/
│   └── requirements.md
└── tasks.md
```

### Source Code (repository root)

| File | Change |
|---|---|
| `relay/report/worker.js` | Point `REPO` at `2tsoftwareapp/quickshort` and `ALLOW_ORIGIN` at `https://2tsoftwareapp.github.io`; keep payload, labels, limits, and rate limiting unchanged. |
| `crates/qs-app/src/settings/mod.rs` | Update GitHub, issue, feature-request, and support links in `LINKS`. |
| `crates/qs-app/src/actions.rs` | Update fallback prefilled issue URL. |
| `crates/qs-app/src/host/menus.rs` | Update tray Donate destination. |
| `packaging/linux/package.sh` | Update Debian maintainer identity to `2tsoftwareapp <noreply@github.com>`; preserve the already-modified AppImage `UPDATE_INFORMATION` value pointing at `2tsoftwareapp`. |
| `packaging/winget/quickshort.version.yaml` | Update future package identifier to the canonical publisher namespace. |
| `packaging/winget/quickshort.locale.en-US.yaml` | Update package identifier and publisher display metadata. |
| `packaging/winget/quickshort.installer.yaml` | Update package identifier. |
| `packaging/homebrew/quickshort.rb` | Update comments and copyable tap instructions to `2tsoftwareapp/homebrew-tap` / `brew tap 2tsoftwareapp/tap`. |
| `.github/workflows/publish-release.yml` | Clone `2tsoftwareapp/homebrew-tap` in the Homebrew update job, copy the canonical cask template before version/sha256 updates, and never fall back to the legacy tap. |
| `.github/FUNDING.yml` | Update sponsor/custom funding destinations to live canonical URLs. |
| `README.md` | Update Homebrew commands and Sponsors link. |
| `LAUNCH.md` | Update release asset commands, Winget path, demo end card, and metrics API examples. |
| `docs/index.html` | Update copyable Homebrew command and GitHub Sponsors link. |
| `demo/ui-2026.html` | Update displayed repository URL. |
| `demo/landing-v2.html` | Update repository links and Homebrew command. |
| `demo/index.html` | Update displayed repository URL. |
| `demo/desktop-sim.html` | Update displayed releases URL. |
| `appshot-video/src/app-config.ts` | Update rendered project URL. |
| `crates/qs-app/Cargo.toml` | Verification unblocker only: add `tempfile` as a dev-dependency because existing update tests use it on non-Linux hosts; no runtime dependency or behavior change. |
| `scripts/check-github-identity.sh` | New executable audit script using `git grep` over active surfaces; exits nonzero when `truongnat` appears there. |

**Structure Decision**: Keep all changes in the files that already own those surfaces. Do not introduce a shared URL constants module: the URLs are few, context-specific, and the audit script is cheaper and clearer than a new abstraction.

## Design Decisions

**Direct replacement at existing call sites.** The defect is inconsistent ownership metadata, not missing abstraction. A central URL module would touch more modules and still would not cover shell, YAML, HTML, or Worker constants. Direct edits plus a repo-wide active-surface audit solve the actual regression mode.

**Hardcoded worker constants stay hardcoded.** The Worker already deploys code through Wrangler; moving `REPO` or `ALLOW_ORIGIN` into environment variables would add deployment configuration without removing the need to redeploy. The payload contract remains stable for older app builds.

**Canonical Homebrew tap is `2tsoftwareapp/homebrew-tap`.** The repository exists and already contains `Casks/quickshort.rb`, so the copyable command can be `brew tap 2tsoftwareapp/tap`. Falling back to the personal tap would preserve the fragmentation this feature removes.

**GitHub Sponsors moves to `2tsoftwareapp`.** The old sponsor URL currently returns 404 while the new sponsor URL returns 200. VietQR and personal banking details are not GitHub identity metadata and remain unchanged.

**Future Winget metadata uses `2tsoftwareapp.quickshort`.** The launch checklist presents Winget submission as pending. If an old package identifier was nevertheless published externally, that is an external migration follow-up; this repository should not keep preparing new submissions under the retired publisher.

**Flatpak application ID remains unchanged.** `com.truongsoftware.quickshort` is a compatibility key, not a `truongnat` GitHub owner reference. Renaming it would risk update continuity and user data for no user-visible gain.

## Phased Delivery

1. **Phase 1 — audit baseline**: add the audit script first and confirm it fails on the current tree, proving it detects the defect.
2. **Phase 2 — runtime/feedback migration**: update the Worker and `qs-app` links; validate Rust compilation/tests and Worker syntax.
3. **Phase 3 — release/channel metadata migration**: update packaging, Homebrew automation, Winget templates, Linux metadata, and funding metadata while preserving existing user edits.
4. **Phase 4 — public docs/demo migration**: update README, launch notes, product docs, demos, and video configuration.
5. **Phase 5 — final verification**: audit passes with zero active legacy references, static checks pass, canonical live destinations respond successfully, and post-deploy relay validation is documented as an operational checklist item.
