# Feature Specification: macOS Developer ID Signing and Notarization

**Feature Branch**: `006-macos-signing-notarization`

**Created**: 2026-11-25

**Status**: Draft — implementation approved by maintainer

**Input**: User description: "I have an Apple Developer account; use it if needed. Proceed, using browser automation if needed."

## Context

Quickshort's production macOS ZIP and DMG artifacts are currently self-signed or ad-hoc signed and are not notarized. Users must right-click Open or rely on the Homebrew cask stripping quarantine. The maintainer now has an Apple Developer account, so production releases can use a Developer ID Application certificate and Apple notarization.

The repository already contains useful pieces: `packaging/macos/release.sh` builds a universal app bundle and creates the ZIP/DMG artifacts, `packaging/macos/installer.sh` has partial local `.pkg` signing support, and the release workflow already imports a self-signed certificate when `QS_SIGN_P12` is configured. What is missing is a production-grade path that imports a Developer ID Application certificate in CI, applies a stable hardened-runtime signature, submits the distributed ZIP/DMG artifacts to Apple's notary service, staples tickets where possible, and updates user-facing installation documentation.

This feature covers the existing production macOS ZIP and DMG assets only. The `.pkg` installer remains a separate installer feature; it is not currently a published release asset. Windows SignPath signing, Linux packaging, Ed25519 update signatures, and the auto-update payload format are out of scope.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - macOS users receive a trusted production download (Priority: P1)

A user downloads the production Quickshort DMG or ZIP, copies the app to Applications, and launches it without the unsigned-app workaround while still being asked for the privacy permissions the app legitimately needs.

**Why this priority**: This is the visible user benefit of the Apple Developer account. It removes first-launch friction and improves trust in the download.

**Independent Test**: On a clean macOS user environment, download the notarized DMG/ZIP artifact, copy `Quickshort.app` to Applications, launch it, inspect the signature and notarization ticket, and confirm Gatekeeper does not present the unsigned-build workflow. Screen Recording and Accessibility permission prompts still appear only when the corresponding features are used.

**Acceptance Scenarios**:

1. **Given** a production DMG artifact, **When** its signature and notarization state are inspected, **Then** the app inside is signed by a Developer ID Application certificate with hardened runtime enabled and the DMG carries a stapled notarization ticket.
2. **Given** a production ZIP artifact, **When** it is expanded, **Then** the app inside is stapled with its notarization ticket before the ZIP is created.
3. **Given** a user opens the production app for the first time, **When** macOS evaluates it, **Then** the user does not need right-click Open, System Settings approval, or `xattr -dr com.apple.quarantine` solely because the app is unsigned.
4. **Given** the app uses Screen Recording or Accessibility features, **When** macOS prompts for those privacy permissions, **Then** the prompts still occur because notarization does not bypass user privacy consent.

---

### User Story 2 - Maintainers get a repeatable CI signing pipeline (Priority: P2)

A maintainer runs the existing release workflow and the macOS job either produces signed, notarized artifacts or fails before packaging with a clear missing/invalid credential error.

**Why this priority**: A signing pipeline that silently falls back to unsigned production artifacts would recreate the current problem. CI must be deterministic and must not expose secrets in logs or the repository.

**Independent Test**: Inspect the workflow and run it in a controlled release-build invocation after credentials are configured. Confirm the job imports the Developer ID Application certificate, configures App Store Connect API credentials for `notarytool`, signs and notarizes the ZIP/DMG, and uploads the final artifacts. A missing-credential dry inspection fails fast without building or publishing.

**Acceptance Scenarios**:

1. **Given** the required GitHub secrets are absent or malformed in the production release workflow, **When** the macOS job starts, **Then** it fails before compilation with a clear message naming the missing configuration class rather than silently producing unsigned artifacts.
2. **Given** a valid Developer ID Application certificate secret, **When** the certificate is imported, **Then** the workflow selects the actual `Developer ID Application:` identity rather than hardcoding the legacy self-signed `quickshort` identity.
3. **Given** valid App Store Connect API key configuration, **When** the packaging script runs, **Then** it uses `xcrun notarytool submit ... --wait` without putting Apple ID passwords in the workflow or repository.
4. **Given** notarization fails or Apple rejects an artifact, **When** the packaging script exits, **Then** checksums and workflow artifacts are not produced as if the release were valid.

---

### User Story 3 - Installation and update documentation matches the signed release (Priority: P3)

A user reading the README, product site, Homebrew cask, or launch documentation sees instructions for a signed and notarized macOS build rather than instructions to strip quarantine or bypass Gatekeeper.

**Why this priority**: Keeping unsigned-build instructions after notarization would train users to bypass the exact protections the feature adds.

**Independent Test**: Inspect the Homebrew cask template and user-facing docs. The cask no longer strips quarantine for production builds, copy/paste install instructions no longer include `xattr` as a required step, and permission guidance is limited to Screen Recording, Accessibility, and optional ffmpeg needs.

**Acceptance Scenarios**:

1. **Given** the Homebrew cask template used for future releases, **When** it is inspected, **Then** it does not contain a postflight step that strips quarantine from production artifacts.
2. **Given** the README install instructions, **When** a macOS user follows them, **Then** they are not told that the build is unsigned or that right-click Open is required because of missing notarization.
3. **Given** the first update that changes signing identities from self-signed to Developer ID, **When** documentation or operational notes discuss permissions, **Then** they acknowledge that macOS privacy grants may need to be approved once for the new signing identity and then remain stable across subsequent Developer ID updates.

---

## Edge Cases

- **Credential transition**: The existing `QS_SIGN_P12` secret may contain the legacy self-signed certificate. Production Developer ID secrets use distinct names so the old local-development path is not confused with production signing.
- **Permission continuity**: Changing from the self-signed identity to Developer ID can make macOS treat the app as a new TCC identity. Users may need to grant Screen Recording/Accessibility once after upgrading; subsequent Developer ID-signed updates should keep the grants.
- **ZIP stapling**: A ZIP file itself cannot be stapled. The app bundle must be stapled after notarization and then zipped again for distribution.
- **DMG stapling**: The DMG can be stapled after its own notarization submission. Its checksum must be calculated only after stapling because stapling changes the file.
- **Local development**: Developers without Apple credentials must still be able to make local ad-hoc/self-signed test artifacts; only the production workflow is strict.
- **Forks and missing secrets**: A fork or maintainer running the production release workflow without credentials should receive an explicit credential failure, not unsigned production artifacts.
- **Apple service failure**: Notary service rejection or timeout must fail the macOS job. No fallback unsigned artifact may be uploaded as the production artifact.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Production macOS app bundles MUST be signed with a valid `Developer ID Application:` identity, stable bundle identifier `ai.quickshort`, hardened runtime, and secure timestamp.
- **FR-002**: Nested executables inside `Quickshort.app` MUST be covered by a valid signature strategy accepted by `codesign --verify --deep --strict` and Apple notarization.
- **FR-003**: The production ZIP workflow MUST submit a ZIP representation of the signed app for notarization, staple the resulting ticket to `Quickshort.app`, and only then create the final distributable ZIP and checksum.
- **FR-004**: The production DMG workflow MUST submit the final DMG for notarization, staple the DMG after acceptance, validate the staple, and only then calculate its checksum.
- **FR-005**: CI MUST authenticate to the notary service using an App Store Connect API key file, key ID, and issuer ID stored in GitHub secrets; Apple ID passwords and private key material MUST NOT be committed or printed.
- **FR-006**: The production release workflow MUST fail before compilation when Developer ID or notarization secrets are missing or when the imported certificate is not a Developer ID Application certificate.
- **FR-007**: Local development builds MAY continue to use the existing self-signed/ad-hoc fallback when notarization variables are absent.
- **FR-008**: The Homebrew cask template for future production releases MUST NOT strip quarantine as a required workaround for unsigned builds.
- **FR-009**: User-facing installation documentation MUST remove claims that current production macOS builds are unsigned once the signed pipeline is in place, while preserving accurate privacy-permission guidance.
- **FR-010**: The feature MUST NOT change capture, OCR, document, update payload, Ed25519 release signing, Windows SignPath, Linux packaging, or local-first privacy behavior.
- **FR-011**: The existing `.pkg` preview/installer path MAY remain unchanged; production `.pkg` signing is explicitly out of scope because `.pkg` is not a published release asset.
- **FR-012**: Secrets setup documentation MUST name the exact GitHub secrets and Apple artifacts needed, without storing secret values in the repository.

### Key Entities

- **Developer ID Application certificate**: Apple-issued certificate used to sign the application bundle and executables for distribution outside the App Store.
- **App Store Connect API key**: `.p8` private key plus key ID and issuer ID used by `notarytool` in CI without Apple ID password/2FA prompts.
- **Notarization ticket**: Apple-issued proof attached to the app or DMG with `stapler` and also available online to Gatekeeper.
- **Production artifact**: `Quickshort-v<version>-macos.zip` or `Quickshort-v<version>-macos.dmg` uploaded by the release workflow.
- **Development signature**: self-signed or ad-hoc local signing used only for non-production testing.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of production macOS ZIP/DMG artifacts produced by the release workflow after credentials are configured are signed by a Developer ID Application identity and accepted by Apple notarization.
- **SC-002**: `xcrun stapler validate` succeeds for the stapled app bundle and DMG in the controlled validation build.
- **SC-003**: `codesign --verify --deep --strict Quickshort.app` succeeds in the packaging script before artifacts are archived.
- **SC-004**: The final ZIP and DMG checksums are generated after all stapling mutations, so published checksums match the exact distributed bytes.
- **SC-005**: A controlled release workflow run either completes with signed/notarized artifacts or fails before compilation with a clear credential error; it never silently uploads unsigned production artifacts.
- **SC-006**: User-facing macOS install instructions contain zero required `xattr`/right-click-Open unsigned-build steps for the notarized production channel.
- **SC-007**: Existing automated verification remains green; no Rust runtime behavior changes are introduced.

## Assumptions

- The maintainer's Apple Developer account is eligible to create Developer ID certificates and App Store Connect API keys.
- GitHub-hosted `macos-latest` runners include Xcode tools sufficient for `codesign`, `notarytool`, and `stapler`.
- The Apple notary service is available during release builds; transient Apple failures fail the job and can be retried.
- Homebrew continues to consume the ZIP asset, so the ZIP must contain the stapled app even though ZIP files themselves cannot be stapled.
- Users upgrading from self-signed builds may need to approve privacy permissions once because the signing identity changes; this is acceptable and preferable to recurring prompts on every update.

## Out of Scope

- Creating or revoking Apple certificates/API keys without explicit maintainer approval.
- Publishing a new release or pushing tags.
- Production `.pkg` signing/notarization as a release asset.
- Windows installer signing changes and Linux packaging changes.
- Redesigning the updater or its Ed25519 signature mechanism.
