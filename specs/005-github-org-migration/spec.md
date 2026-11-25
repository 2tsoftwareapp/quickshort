# Feature Specification: GitHub Identity Migration Consistency

**Feature Branch**: `005-github-org-migration`

**Created**: 2026-11-25

**Status**: Draft — ready for plan review

**Input**: User description: "Fix inconsistent GitHub identity migration from truongnat to 2tsoftwareapp across app links, feedback relay, packaging, release automation, documentation, demos, and funding surfaces."

## Context

Quickshort moved its public GitHub presence from `truongnat` to `2tsoftwareapp`, but part of the product still sends users and automation to the old identity. The breakage is no longer theoretical: the old GitHub Pages site and old Sponsors page return 404, while the new repo, new Pages site, new Sponsors page, and `2tsoftwareapp/homebrew-tap` are live.

The most user-visible failure is feedback: the report worker still files issues against the old repository and only accepts browser requests from the old Pages origin, so the live feature-request form can be blocked by CORS and reports can land in the wrong tracker. The most user-trust-visible failure is installation/support: copyable Homebrew commands, sponsor links, and several in-app links still point at the old identity.

Two migration edits already exist in the working tree and are treated as part of the baseline to preserve, not redo: the AppImage update metadata in `packaging/linux/package.sh` already points at `2tsoftwareapp`, and `relay/report/README.md` already documents the new repository.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Feedback and in-app destinations reach the live project (Priority: P1)

A user opens Settings, the tray menu, or the public feedback form and every bug-report, feature-request, GitHub, and support destination reaches the current Quickshort project rather than a dead or legacy destination.

**Why this priority**: This path affects active users when they need help or want to send feedback. A failure here loses bug reports, creates issues in the wrong place, or sends users to a 404.

**Independent Test**: From a build containing only this story, inspect every in-app external destination and submit one controlled feature-request test through the report relay; the request is accepted from the live site origin and the resulting issue is created under the current repository.

**Acceptance Scenarios**:

1. **Given** the app is running, **When** the user opens GitHub, report-issue, feature-request, or support links from Settings and the tray Donate action, **Then** each destination uses the current `2tsoftwareapp` GitHub repo or Pages site.
2. **Given** the report relay cannot be reached, **When** the app falls back to opening a prefilled GitHub issue, **Then** the issue URL targets `2tsoftwareapp/quickshort`.
3. **Given** the live Quickshort Pages origin, **When** the browser sends a CORS preflight or feature-request submission to the report relay, **Then** the relay allows that origin.
4. **Given** a valid report or feature request reaches the relay, **When** the worker files the issue, **Then** it files it in `2tsoftwareapp/quickshort` with the existing labels and payload shape unchanged.

---

### User Story 2 - Installation and release automation use the current publisher identity (Priority: P2)

A new user copying an install command, or a maintainer publishing a release, consistently uses the current `2tsoftwareapp` identity for Homebrew, Winget metadata, Linux package metadata, and AppImage updates.

**Why this priority**: Broken install instructions stop adoption, while wrong release automation can publish updates into a legacy tap or leave AppImage users unable to update. It is slightly lower than feedback only because part of the release path has already been repaired locally.

**Independent Test**: Inspect the generated install instructions and package metadata for each channel; validate that every copyable command names an existing current-identity repository or tap, and run the release workflow static checks without needing to publish a real release.

**Acceptance Scenarios**:

1. **Given** a macOS user copies the documented Homebrew command, **When** they run or inspect it, **Then** it uses the existing `2tsoftwareapp/tap` tap rather than the retired `truongnat/tap` tap.
2. **Given** the release workflow updates the Homebrew cask, **When** it clones the tap repository, **Then** it clones `2tsoftwareapp/homebrew-tap` and expects `TAP_TOKEN` to have access there.
3. **Given** Winget manifest templates are prepared for submission, **When** a maintainer inspects them, **Then** package identifier and publisher metadata consistently use the current publisher identity.
4. **Given** Linux packages are built, **When** the Debian metadata and AppImage update metadata are inspected, **Then** they do not present or target the legacy GitHub identity.
5. **Given** a distribution identifier is stable rather than a link, **When** it is reviewed, **Then** it is intentionally preserved rather than blindly renamed.

---

### User Story 3 - Public documentation and demos present one coherent project home (Priority: P3)

A visitor reading the README, launch notes, product site, demos, or generated video configuration sees one coherent project home and one working sponsorship destination.

**Why this priority**: These surfaces shape first impressions and are easy to validate, but they do not directly break runtime feedback or release automation once P1/P2 are fixed.

**Independent Test**: Run a repository audit over user-facing documentation and demo assets; no active user-facing reference to the legacy GitHub identity remains outside explicitly historical notes, and all changed HTTP destinations resolve successfully.

**Acceptance Scenarios**:

1. **Given** a visitor opens the product site or README, **When** they follow GitHub, Homebrew, or Sponsors links, **Then** they reach live `2tsoftwareapp` destinations rather than old pages or a 404 sponsor page.
2. **Given** a maintainer follows the launch checklist, **When** they compute checksums, submit Winget metadata, render the demo, or measure release downloads, **Then** every command and example targets the current repository.
3. **Given** demo and video mockups display a repository URL, **When** they are rendered or exported, **Then** they show `github.com/2tsoftwareapp/quickshort`.
4. **Given** payment or banking details are personal rather than GitHub-hosted, **When** they are reviewed, **Then** they are not changed merely because they contain a personal name.

---

### Edge Cases

- **Older installed builds**: old binaries may still contain old URLs. They cannot be rewritten retroactively; the fixed relay must remain compatible with their existing POST payload so their reports still file correctly.
- **GitHub repository redirects**: GitHub currently redirects the transferred repository to the new namespace, but the migration must not rely on redirects because Pages and Sponsors do not redirect and redirects can disappear if the old name is reused.
- **Custom marketing domain**: `2tsoftware.app` is live but does not currently host the feedback form that posts to the report relay. This feature only guarantees the live form's GitHub Pages origin; multi-origin CORS is future work if that form is deployed on the custom domain.
- **Stable package/application IDs**: Flatpak's `com.truongsoftware.quickshort` app ID is a compatibility identifier, not a user-facing GitHub link; changing it could break updates and user data, so it is out of scope unless a separate distribution migration is approved.
- **External channel state**: Winget, Homebrew tap contents, AUR, and Flathub may have already-published metadata outside this repository. This feature fixes the source templates and automation here; external submissions are validation/follow-up steps, not silently assumed complete.
- **Secrets and permissions**: the release workflow can only update the new tap if `TAP_TOKEN` is valid for `2tsoftwareapp/homebrew-tap`; missing or insufficient credentials must fail visibly or skip explicitly, never fall back to the legacy tap.
- **Historical text**: migration notes may mention the old identity when explaining the migration, but active links, copyable commands, package metadata, and automation targets must not use it.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Every user-facing GitHub repository link owned by the app, product site, documentation, demos, or video configuration MUST target `github.com/2tsoftwareapp/quickshort`.
- **FR-002**: Every user-facing Quickshort Pages link owned by the app or repository metadata MUST target `2tsoftwareapp.github.io/quickshort` when it refers to the documentation site's feedback, donate, or privacy sections; broader migration of the separate custom marketing domain is out of scope.
- **FR-003**: The report relay MUST file bug reports, feature requests, and supporter submissions in `2tsoftwareapp/quickshort` without changing their existing labels, size limits, payload format, or rate-limit behavior.
- **FR-004**: The report relay MUST accept browser requests from the live Pages origin `https://2tsoftwareapp.github.io` and MUST NOT continue to treat the retired Pages origin as the allowed origin.
- **FR-005**: The app's fallback prefilled issue URL MUST target `2tsoftwareapp/quickshort` and preserve the existing diagnostics payload encoding.
- **FR-006**: Homebrew instructions, the example cask comments, and release automation MUST use `2tsoftwareapp/homebrew-tap` / `brew tap 2tsoftwareapp/tap` as the canonical tap.
- **FR-007**: Winget manifest templates MUST use a package identifier and publisher metadata consistent with the `2tsoftwareapp` publisher identity for future submissions.
- **FR-008**: Linux packaging MUST not present `truongnat` as package maintainer or embed AppImage update information for the legacy owner.
- **FR-009**: GitHub Sponsors references MUST target the live `2tsoftwareapp` Sponsors page; non-GitHub payment details such as VietQR MUST remain unchanged.
- **FR-010**: The repository MUST include a repeatable audit procedure that distinguishes active stale references from intentional historical or compatibility references.
- **FR-011**: No migration change MAY alter capture, OCR, document, rendering, update-signing, or local-first privacy behavior.
- **FR-012**: The fix MUST preserve compatibility identifiers that are not GitHub-owner links, including the Flatpak application ID, unless separately approved.

### Key Entities

- **Canonical identity**: `2tsoftwareapp`, the live owner namespace for the GitHub repository, Pages site, Sponsors page, and Homebrew tap.
- **Legacy identity**: `truongnat`, the retired owner namespace that may still appear in historical explanations or compatibility identifiers.
- **Feedback relay**: the unauthenticated-but-rate-limited submission bridge that creates public GitHub issues without requiring users to have GitHub accounts.
- **Distribution channel metadata**: copyable install commands and package-manager metadata that determine where users install from and how release automation publishes updates.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% of active user-facing links and copyable commands in app code, README, product docs, launch notes, packaging templates, demos, and video configuration use the canonical identity or an intentionally preserved non-GitHub identifier.
- **SC-002**: A controlled browser-originated feature-request test from the live Pages origin is accepted by the relay and produces an issue in the current repository; legacy-origin browser requests are no longer required for the form to work.
- **SC-003**: 100% of changed HTTP destinations return a successful response or redirect chain ending at the canonical destination; no changed destination lands on the retired Pages or Sponsors 404.
- **SC-004**: The migration audit reports zero unallowlisted `truongnat` occurrences in runtime code, packaging, release automation, user-facing docs, demos, and video configuration.
- **SC-005**: 100% of pre-existing automated verification that passed before the migration remains green afterward, with no new warnings and no functional behavior change outside identity metadata.
- **SC-006**: A maintainer can verify Homebrew tap targeting without publishing a release by confirming the workflow clone URL and the live `2tsoftwareapp/homebrew-tap` repository agree.

## Assumptions

- `2tsoftwareapp` is the canonical public identity even though the GitHub API currently classifies the account as a user rather than an organization; product behavior only depends on the namespace being canonical.
- `2tsoftwareapp/homebrew-tap` is the intended Homebrew tap because the repository exists publicly.
- `https://github.com/sponsors/2tsoftwareapp` is the intended sponsorship destination because it is live, while `https://github.com/sponsors/truongnat` is not.
- The two existing uncommitted edits in `packaging/linux/package.sh` and `relay/report/README.md` are intentional user work and must be preserved.
- Out of scope for this feature: the broader audit recommendations about `rfd` version alignment, refactoring files over 1,000 lines, `.expect()` in stitch dispatch, Clippy CI expansion, video capture architecture, or `df` portability.
