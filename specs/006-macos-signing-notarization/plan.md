# Implementation Plan: macOS Developer ID Signing and Notarization

**Branch**: `006-macos-signing-notarization` (artifact branch name; implementation may happen on the current working branch) | **Date**: 2026-11-25 | **Spec**: [spec.md](./spec.md)

## Summary

Upgrade the existing macOS release packaging path so production ZIP/DMG artifacts are signed with a Developer ID Application certificate, notarized through Apple's notary service using an App Store Connect API key, stapled where possible, and documented as trusted production artifacts.

The implementation reuses `packaging/macos/release.sh` and the existing `release.yml` macOS job rather than adding a parallel pipeline. The production workflow becomes strict: missing or invalid Apple credentials fail before compilation. Local development retains self-signed/ad-hoc fallback when notarization variables are absent. Documentation and the Homebrew cask template are updated so users are no longer instructed to bypass Gatekeeper for future production builds.

## Technical Context

**Language/Version**: POSIX shell for macOS packaging; YAML for GitHub Actions; Markdown/HTML/JavaScript strings for documentation.

**Primary Dependencies**: macOS/Xcode command-line tools already present on GitHub `macos-latest` runners: `codesign`, `xcrun notarytool`, `xcrun stapler`, `security`, `hdiutil`, `ditto`. No new runtime or Rust dependency.

**Storage**: N/A — no persistent user data or `.qsdoc` format changes.

**Testing**: `sh -n packaging/macos/release.sh`; YAML parsing of `.github/workflows/release.yml`; `git diff --check`; repository audit for obsolete unsigned-build instructions; normal Rust workspace verification because the repository constitution requires it even though no Rust code changes; controlled workflow/artifact validation only after credentials and maintainer approval.

**Target Platform**: macOS production release artifacts. No Windows/Linux behavior change.

**Project Type**: Rust desktop application workspace with shell packaging and GitHub Actions release automation.

**Performance Goals**: No runtime performance change. CI adds Apple notary service wait time to the macOS release job; artifact generation remains otherwise unchanged.

**Constraints**: Do not commit or print secrets; do not create/revoke Apple certificates or API keys without explicit approval; do not publish releases/tags as validation; preserve artifact names for updater/Homebrew compatibility; keep `.pkg` production signing out of scope; preserve local development fallback.

**Scale/Scope**: Expected source changes in `packaging/macos/release.sh`, `.github/workflows/release.yml`, `packaging/homebrew/quickshort.rb`, README/product docs/launch notes, plus spec artifacts. No Rust runtime behavior changes; the only Rust edit is a deterministic test unblocker in `qs-theme` for a pre-existing parallel-test race exposed by the verification gate.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | How this plan complies |
|---|---|
| **I. Design Token SSOT** | No visual UI, color, spacing, radius, or font changes. Documentation text changes only. |
| **II. Layered Crates** | No crate dependency or Rust layer changes. Packaging/workflow files are outside the crate graph. |
| **III. `.qsdoc` Compatibility** | `.qsdoc` untouched. |
| **IV. Reversible Actions** | No user-content mutation is added. Release artifacts are build outputs, not user content. |
| **V. Verification Gate** | No Rust code changes, but the normal `cargo check --workspace --all-targets` and `cargo test --workspace` gate will still run. Narrow validation is shell syntax, workflow YAML parse, diff checks, and post-credential artifact validation. No UI layout change, so viewport testing is not applicable. |

**Deviations**: none.

## Project Structure

### Documentation (this feature)

```text
specs/006-macos-signing-notarization/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── signing-contract.md
├── checklists/
│   └── requirements.md
└── tasks.md
```

### Source Code (repository root)

| File | Change |
|---|---|
| `packaging/macos/release.sh` | Add notarization-aware signing flow: validate notary config when present, notarize temporary ZIP, staple app before final ZIP, notarize/staple DMG before checksums. Keep local unsigned/self-signed fallback when notary variables are absent. |
| `.github/workflows/release.yml` | Replace hardcoded self-signed identity logic with strict production credential validation, import Developer ID Application certificate, decode App Store Connect key into a runner temp file, and pass notarization environment into packaging. |
| `packaging/homebrew/quickshort.rb` | Remove production quarantine-stripping postflight and update comments for signed/notarized artifacts. |
| `README.md` | Replace unsigned macOS installation/development/known-limit text with signed/notarized production behavior and permission-transition note. |
| `docs/index.html` | Update user-facing permission/update copy that currently describes self-signed or unsigned macOS builds. |
| `docs/i18n.js` | Update corresponding English/Vietnamese copy. |
| `LAUNCH.md` | Update launch constraint text that says builds are unsigned/no Dev ID yet. |
| `docs/installer-research.md` | Update stale research statements so they reflect the production ZIP/DMG signing pipeline and remaining `.pkg` future work. |
| `packaging/macos/gen_signing_cert.sh` | Clarify that the self-signed certificate path is local development fallback, not production distribution. |
| `crates/qs-theme/src/lib.rs` | Verification unblocker only: make `set_accent_updates_runtime_accent` deterministic by using a custom color unaffected by dark-theme accent mapping; no production code change. |

**Structure Decision**: Keep signing logic in the existing packaging script and credential bootstrap in the workflow. Do not introduce a new signing abstraction or CI action; Apple/Xcode CLIs are already the narrowest correct tools.

## Design Decisions

**Strict production credentials.** The release workflow exists to produce production artifacts. If Apple credentials are absent or invalid, failing before compilation is safer than uploading unsigned artifacts that look official. Local development fallback remains in the shell script, not the production workflow.

**Distinct production secret names.** `QS_DEV_ID_APPLICATION_P12` and `QS_DEV_ID_APPLICATION_P12_PASSWORD` avoid confusion with legacy self-signed `QS_SIGN_P12`. This prevents accidentally selecting the self-signed cert in production and makes secret rotation clearer.

**App Store Connect API key instead of Apple ID password.** The `.p8` key can be scoped/revoked and avoids interactive 2FA. It is decoded only into a runner temporary file and never echoed.

**Staple before final archive/checksum.** The final ZIP must contain the stapled app; the final DMG must be stapled itself. Checksums are generated only after those mutations. This preserves updater/Homebrew checksum correctness.

**`.pkg` remains future work.** A production `.pkg` requires a Developer ID Installer certificate and an explicit release asset contract. Since `.pkg` is not currently published, mixing it into this feature would expand credential scope without immediate user benefit.

## Phase 0/1 Research Outcome

Research is complete in [research.md](./research.md). No unresolved clarification remains. The signing contract is documented in [contracts/signing-contract.md](./contracts/signing-contract.md).

## Validation Strategy

1. Static validation before credentials:
   - `sh -n packaging/macos/release.sh`
   - YAML parse of `.github/workflows/release.yml`
   - `git diff --check`
   - grep audit for stale unsigned/quarantine instructions in active user-facing docs
2. Repository verification gate:
   - `cargo check --workspace --all-targets`
   - `cargo test --workspace`
3. Controlled validation after credential setup, only with maintainer approval:
   - run the `release` workflow without publishing;
   - download artifacts;
   - verify `codesign`, stapler, checksums, and Gatekeeper behavior as documented in `quickstart.md`.
