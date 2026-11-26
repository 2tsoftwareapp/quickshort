# Tasks: macOS Developer ID Signing and Notarization

**Input**: Design documents from `/specs/006-macos-signing-notarization/`

**Prerequisites**: `spec.md`, `plan.md`, `research.md`, `data-model.md`, `contracts/signing-contract.md`, `quickstart.md`

**Organization**: Tasks are grouped by user story so user-facing artifact trust, CI repeatability, and documentation migration can be validated independently.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: User story this task belongs to (`US1`, `US2`, `US3`)
- Every implementation task names the exact file path it changes or validates

## Phase 1: Setup (Feature Guardrails)

**Purpose**: Establish the exact credential/artifact contract before editing packaging or workflow files.

- [x] T001 Create feature artifacts under `specs/006-macos-signing-notarization/` documenting the ZIP/DMG scope, strict production credential behavior, local development fallback, and exclusion of production `.pkg` signing.
- [x] T002 Record the required secret names in `specs/006-macos-signing-notarization/contracts/signing-contract.md`: `QS_DEV_ID_APPLICATION_P12`, `QS_DEV_ID_APPLICATION_P12_PASSWORD`, `QS_NOTARY_P8_B64`, `QS_NOTARY_KEY_ID`, and `QS_NOTARY_ISSUER`; no secret values may be written to the repository.

---

## Phase 2: Foundational (Release-contract constraints)

**Purpose**: Preserve existing artifact names and separate production strictness from local development fallback before story implementation.

- [x] T003 Review `packaging/macos/release.sh`, `.github/workflows/release.yml`, and `docs/installer-research.md`; confirm the existing artifact names `Quickshort-v<version>-macos.zip` and `Quickshort-v<version>-macos.dmg` must remain unchanged for updater/Homebrew compatibility.
- [x] T004 Confirm in `specs/006-macos-signing-notarization/plan.md` that `.pkg` production signing is out of scope because `.pkg` is not currently a published release asset.

**Checkpoint**: Credential names, artifact names, and scope boundaries are fixed.

---

## Phase 3: User Story 1 - macOS users receive a trusted production download (Priority: P1) 🎯 MVP

**Goal**: The packaging script can produce Developer ID-signed, notarized, stapled ZIP/DMG artifacts when valid credentials are supplied.

**Independent Test**: Inspect the script order, run shell syntax validation, and later validate a controlled workflow artifact with `codesign`/`stapler` after credentials are configured.

### Implementation for User Story 1

- [x] T005 [US1] Update the signing section of `packaging/macos/release.sh` so a supplied Developer ID identity is verified with hardened runtime before archiving, while preserving the existing self-signed/ad-hoc fallback when no notarization configuration is present.
- [x] T006 [US1] Add ZIP notarization to `packaging/macos/release.sh`: when `NOTARY_KEY`, `NOTARY_KEY_ID`, and `NOTARY_ISSUER` are present, create a temporary ZIP, submit it with `xcrun notarytool submit ... --wait`, staple `Quickshort.app`, validate the app staple, and only then create the final distributable ZIP.
- [x] T007 [US1] Add DMG notarization to `packaging/macos/release.sh`: submit the created DMG with the same notary credentials, staple and validate the DMG, and calculate checksums only after all stapling mutations.
- [x] T008 [US1] Validate User Story 1 statically with `sh -n packaging/macos/release.sh` and by inspecting the script order: sign → notarize temporary ZIP → staple app → final ZIP → DMG → notarize DMG → staple DMG → checksums.

**Checkpoint**: Packaging logic can produce trusted artifacts when credentials exist and fails rather than producing invalid artifacts when notarization is requested but incomplete.

---

## Phase 4: User Story 2 - Maintainers get a repeatable CI signing pipeline (Priority: P2)

**Goal**: The production release workflow imports the correct certificate, configures notarization securely, and fails fast when credentials are missing or invalid.

**Independent Test**: Parse the workflow YAML, inspect secret handling, and confirm no secrets are printed or committed. A controlled CI run happens only after maintainer-approved credential setup.

### Implementation for User Story 2

- [x] T009 [US2] Update the macOS credential environment in `.github/workflows/release.yml` to use `QS_DEV_ID_APPLICATION_P12`, `QS_DEV_ID_APPLICATION_P12_PASSWORD`, `QS_NOTARY_P8_B64`, `QS_NOTARY_KEY_ID`, and `QS_NOTARY_ISSUER`.
- [x] T010 [US2] Replace the hardcoded self-signed identity import in `.github/workflows/release.yml` with strict validation that imports the `.p12`, discovers the `Developer ID Application:` identity, fails before compilation if it is absent, and exports it as `SIGNING_IDENTITY`.
- [x] T011 [US2] Add notary credential setup in `.github/workflows/release.yml`: validate required values without printing them, decode `QS_NOTARY_P8_B64` into a runner temp `.p8` file with restrictive permissions, and export `NOTARY_KEY`, `NOTARY_KEY_ID`, and `NOTARY_ISSUER` for `packaging/macos/release.sh`.
- [x] T012 [US2] Validate User Story 2 by parsing `.github/workflows/release.yml` as YAML, running `git diff --check`, and inspecting the diff to confirm secret values are never echoed or written to repository files.

**Checkpoint**: Production CI is strict, uses the correct Apple identity, and does not silently fall back to unsigned artifacts.

---

## Phase 5: User Story 3 - Installation and update documentation matches the signed release (Priority: P3)

**Goal**: User-facing documentation and the Homebrew template stop teaching unsigned-build bypasses for future production artifacts while preserving accurate privacy-permission guidance.

**Independent Test**: Inspect changed files and run a grep audit for obsolete unsigned/quarantine instructions in active macOS install surfaces.

### Implementation for User Story 3

- [x] T013 [P] [US3] Update `packaging/homebrew/quickshort.rb` comments and remove the postflight quarantine-stripping block for future signed/notarized production casks.
- [x] T014 [P] [US3] Update `README.md` macOS installation, agent install instructions, development release description, and known-limits text so production builds are described as Developer ID signed/notarized and permission re-grant is mentioned only as a one-time signing-identity transition caveat where relevant.
- [x] T015 [P] [US3] Update `docs/index.html` and `docs/i18n.js` copy that currently says updates rely on a self-signed identity or that macOS builds are unsigned.
- [x] T016 [P] [US3] Update `LAUNCH.md` and `docs/installer-research.md` stale statements that say there is no Developer ID or that the workflow uses only self-signed/ad-hoc signing; keep `.pkg` signing clearly marked as future work.
- [x] T017 [P] [US3] Update `packaging/macos/gen_signing_cert.sh` comments to clarify that self-signed certificates are for local development/TCC experimentation, not production distribution.
- [x] T018 [US3] Validate User Story 3 with a grep audit confirming active macOS install surfaces no longer require `xattr` or right-click Open solely because production builds are unsigned.

**Checkpoint**: Documentation matches the signed production channel and does not remove legitimate permission guidance.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Verify the complete feature without expanding into release publication or Apple account mutation.

- [x] T019 Run static validation from `specs/006-macos-signing-notarization/quickstart.md`: `sh -n packaging/macos/release.sh`, YAML parse of `.github/workflows/release.yml`, and `git diff --check`.
- [x] T020 Run the repository verification gate: `cargo check --workspace --all-targets` and `cargo test --workspace`; record exact results and warnings.
- [x] T021 Review the final diff and confirm no Rust runtime behavior, updater payload format, Ed25519 signing, Windows SignPath, Linux packaging, or production `.pkg` contract changed.
- [x] T022 Request explicit maintainer approval before using the browser or Apple Developer/App Store Connect UI to create/download/revoke any certificate or API key; if approved, assist with credential creation and GitHub secret setup without echoing secret values.
- [x] T023 Request explicit maintainer approval before running a controlled `release` workflow build with real Apple credentials; only after approval, validate artifacts according to `specs/006-macos-signing-notarization/quickstart.md` without publishing a release or pushing a tag.
- [x] T024 Record completion notes in `specs/006-macos-signing-notarization/tasks.md`, including static/cargo results, whether Apple credential setup was approved/completed, whether controlled artifact validation ran, and remaining operational follow-up.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies; fixes credential/artifact contract.
- **Foundational (Phase 2)**: Depends on Setup; blocks implementation because artifact compatibility is fixed first.
- **User Story 1 (Phase 3)**: Depends on Foundational; implements packaging behavior.
- **User Story 2 (Phase 4)**: Depends on Foundational; can run parallel with US1 because files differ.
- **User Story 3 (Phase 5)**: Depends on Foundational; can run parallel with US1/US2 because files differ.
- **Polish (Phase 6)**: Depends on all implementation stories.

### Parallel Opportunities

- T005–T007 intentionally stay sequential because they edit the same shell script in dependent order.
- T009–T011 intentionally stay sequential because they edit the same workflow credential block in dependent order.
- T013, T014, T015, T016, and T017 touch different documentation files and can run in parallel.
- T022 and T023 are operational side-effect gates and must not be silently executed.

## Implementation Strategy

### MVP First (User Story 1 + 2)

1. Implement the packaging and workflow credential path.
2. Validate statically.
3. Stop before Apple account mutation or CI artifact validation until separately approved.

### Incremental Delivery

1. US1 makes artifacts technically capable of being trusted.
2. US2 makes the production CI path strict and repeatable.
3. US3 updates user-facing guidance after the production path is defined.
4. Controlled artifact validation happens only after credentials are configured and approved.


## Completion Notes

Implementation, Apple/GitHub credential setup, controlled CI artifact validation, and production release `v0.4.1` completed. Tag `v0.4.1` and the GitHub release were published after explicit maintainer approval.

### Verification results

- `sh -n packaging/macos/release.sh`: pass (`release_sh_syntax_ok`).
- `.github/workflows/release.yml` YAML parse: pass (`release_yml_yaml_ok`).
- `.github/workflows/publish-release.yml` YAML parse: pass.
- `git diff --check`: pass.
- `node --check relay/report/worker.js`: pass.
- `bash -n packaging/linux/package.sh`: pass.
- Failure-path probes: complete-but-missing notary key exits 1 with `notary key file not found`; partial notary configuration exits 1 with `notarization needs NOTARY_KEY, NOTARY_KEY_ID and NOTARY_ISSUER together`.
- `cargo check --workspace --all-targets`: pass.
- `cargo test --workspace`: pass — 49 test-result suites, 352 passed, 0 failed, 5 ignored.
- `qs-theme` targeted regression check passed 5 consecutive runs after fixing the pre-existing parallel-test race.
- Pre-existing warning remains: `unused import: stack::shelf_logical_size` in `crates/qs-app/src/chrome.rs`; it is unrelated to signing/notarization and was not suppressed.

### Operational gates

- T022 completed: maintainer explicitly approved full Apple/GitHub secret setup. A Developer ID Application certificate was created in Apple team `3279CRW8KQ` (expiration `2027/12/26`), converted locally to a macOS-keychain-compatible `.p12`, and validated with `security import`; an App Store Connect Team key named `quickshort-notarization` was created with Developer role (Key ID `7K8H246YN4`) and `notarytool history` authenticated successfully. The five required GitHub secrets were set in `2tsoftwareapp/quickshort`; no secret values were printed or committed. Browser automation session was closed.
- T023 completed: maintainer explicitly approved committing/pushing the validation branch and running controlled CI artifact builds. Validation branch `validation/macos-signing-notarization` was pushed; workflow run `45676352445` completed successfully at commit `2fa0cb98e60acc9ff184657407c422c67c2dd6e1` with all four jobs green (`metadata`, `windows`, `linux`, `macos`). No tag or release publication was performed.

### Controlled CI artifact validation

- Workflow run `45671094638` validated the macOS production path but exposed unrelated Linux/Windows packaging failures: the Windows checksum step used bash syntax under default PowerShell, and `linuxdeploy-plugin-gtk` assumed a legacy external gdk-pixbuf loader directory that Ubuntu 26.04 no longer creates.
- Fixes committed in `2fa0cb9`: force the Windows checksum step to `shell: bash`, pin and patch the GTK plugin to skip absent gdk-pixbuf loader directories, and set `NO_STRIP=1` because linuxdeploy's bundled strip rejects modern `.relr.dyn` ELF sections.
- Workflow run `45676352445` then completed green for metadata, Windows, Linux, and macOS.
- Downloaded artifacts validated locally:
  - Linux: `Quickshort-Linux.AppImage` and `quickshort_amd64.deb` both passed `linux.sha256` verification.
  - Windows: `Quickshort-Windows-Setup.exe` passed its `.sha256` verification.
  - macOS: ZIP and DMG passed `.sha256` verification; extracted `Quickshort.app` passed `codesign --verify --deep --strict`; both app and DMG passed `xcrun stapler validate`; Gatekeeper reported `accepted`, `source=Notarized Developer ID`, origin `Developer ID Application: Cuong Nguyen Van (3279CRW8KQ)`.
- Apple notarization history showed both controlled-run submissions accepted: `Quickshort.zip` and `Quickshort-v0.4.1-macos.dmg`.

### Production release outcome

- Release workflow run `45730157514` was dispatched on `main` at `987eb8c11a627ffec1b562d50557bf7d31c9052b` with version input `0.4.1`; metadata, Windows, Linux, and macOS jobs all completed successfully.
- Downloaded build artifacts passed checksum validation. The macOS app passed strict codesign verification, app/DMG stapler validation, and Gatekeeper assessment as `Notarized Developer ID`.
- Lightweight tag `v0.4.1` was pushed at the same tested commit `987eb8c11a627ffec1b562d50557bf7d31c9052b`.
- Publish workflow run `45732600587` created the draft release successfully. All five primary payload `.sig` files were locally verified against the embedded Ed25519 public key before publication.
- Release `v0.4.1` was published at `2026-11-26T01:00:54Z` with 14 assets, including payload signatures and checksums. Release-event workflow run `45732794687` completed successfully and updated the live Homebrew cask to version `0.4.1` with the published ZIP SHA256 `14389a6eeeed1a2aa0bc05b82858ba20a73977b1df085cbb1ac48e02bbb054ac`.
- Windows artifacts remain unsigned because SignPath variables/secrets are not configured; the pipeline's optional SignPath steps were skipped. macOS artifacts are Developer ID signed and notarized.

### Diff review result

- macOS ZIP/DMG artifact names are unchanged: `Quickshort-v<version>-macos.zip` and `Quickshort-v<version>-macos.dmg`.
- No updater payload format, Ed25519 asset-signing logic, Windows SignPath flow, Linux artifact/update-info contract, or production `.pkg` contract was changed. Linux packaging internals changed only to tolerate modern Ubuntu/gdk-pixbuf layouts and skip linuxdeploy's incompatible strip binary; Windows packaging changed only to run the existing checksum commands under bash.
- No Rust runtime behavior was changed for this feature. The only Rust test change is the deterministic `qs-theme` test unblocker noted in `plan.md`; `crates/qs-app` Rust changes in the same working tree belong to feature `005-github-org-migration`.
- Active macOS install documentation no longer instructs users to remove quarantine or use right-click Open solely because production builds are unsigned. Remaining `unsigned` references are legitimate Windows/self-signed local-development/upgrade-transition caveats.
