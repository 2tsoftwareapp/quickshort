# Tasks: GitHub Identity Migration Consistency

**Input**: Design documents from `/specs/005-github-org-migration/`

**Prerequisites**: `spec.md`, `plan.md`, `research.md`, `data-model.md`, `contracts/identity-contract.md`, `quickstart.md`

**Organization**: Tasks are grouped by user story so runtime feedback, release metadata, and public documentation can be validated independently.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: User story this task belongs to (`US1`, `US2`, `US3`)
- Every implementation task names the exact file path it changes or validates

## Phase 1: Setup (Audit Harness)

**Purpose**: Create the narrow repeatable check that proves the migration defect and later proves the fix.

- [x] T001 Create executable `scripts/check-github-identity.sh` that runs `git grep -n 'truongnat'` over `crates`, `relay`, `packaging`, `.github`, `docs`, `README.md`, `LAUNCH.md`, `demo`, and `appshot-video`, prints migration guidance on matches, and exits nonzero when any match is found.
- [x] T002 Run `scripts/check-github-identity.sh` before editing product files and confirm it fails by detecting the current legacy references; record the failure output as the baseline in the implementation notes for `specs/005-github-org-migration/tasks.md`.


**Baseline audit output (2026-11-25, exit status 1, 36 matched lines):**

```text
legacy GitHub identity 'truongnat' found in active surfaces:
.github/FUNDING.yml:1:github: truongnat
.github/FUNDING.yml:2:custom: "https://truongnat.github.io/quickshort/#donate"
.github/workflows/publish-release.yml:129:          GH_TOKEN="$TAP_TOKEN" git clone https://github.com/truongnat/homebrew-tap "$RUNNER_TEMP/tap"
LAUNCH.md:47:curl -sL https://github.com/truongnat/quickshort/releases/download/v1.0.0/Quickshort-Windows-Setup.exe | sha256sum
LAUNCH.md:49:curl -sL https://github.com/truongnat/quickshort/releases/download/v1.0.0/quickshort_amd64.deb | sha256sum
LAUNCH.md:54:      `manifests/t/truongnat/quickshort/1.0.0/`
LAUNCH.md:66:4. end card: `github.com/truongnat/quickshort`
LAUNCH.md:76:- `gh api repos/truongnat/quickshort/releases` → `download_count` per asset
README.md:85:brew tap truongnat/tap
README.md:161:  `brew tap truongnat/tap && brew install --cask quickshort`.
README.md:327:- **[GitHub Sponsors](https://github.com/sponsors/truongnat)** — international, zero fees on personal sponsorships
appshot-video/src/app-config.ts:9:    url: "github.com/truongnat/quickshort",
crates/qs-app/src/actions.rs:349:                    "https://github.com/truongnat/quickshort/issues/new?labels=bug&body={}",
crates/qs-app/src/host/menus.rs:34:                "https://truongnat.github.io/quickshort/#donate",
crates/qs-app/src/settings/mod.rs:144:    ("GitHub", "https://github.com/truongnat/quickshort"),
crates/qs-app/src/settings/mod.rs:147:        "https://github.com/truongnat/quickshort/issues",
crates/qs-app/src/settings/mod.rs:151:        "https://truongnat.github.io/quickshort/#request",
crates/qs-app/src/settings/mod.rs:153:    ("Support", "https://truongnat.github.io/quickshort/#donate"),
demo/desktop-sim.html:275:    <div class="br-url"><div class="url-pill">github.com/truongnat/<b>quickshort</b>/releases</div></div>
demo/index.html:338:  <div class="repo">github.com/truongnat/quickshort</div>
demo/landing-v2.html:254:      <a href="https://github.com/truongnat/quickshort" target="_blank" rel="noopener">GitHub</a>
demo/landing-v2.html:286:      <a class="btn btn-ghost" href="https://github.com/truongnat/quickshort" target="_blank" rel="noopener">View on GitHub</a>
demo/landing-v2.html:474:      <div class="code-block"><code>brew tap truongnat/tap
demo/ui-2026.html:267:      <div class="btn btn-ghost mono">github.com/truongnat/quickshort</div>
docs/index.html:229:        <code id="brewcmd">brew tap truongnat/tap
docs/index.html:231:        <button class="copy-btn" data-copy="brew tap truongnat/tap && brew install --cask quickshort" aria-label="Copy install command">Copy</button>
docs/index.html:318:      <a class="btn btn-primary" href="https://github.com/sponsors/truongnat" target="_blank" rel="noopener" data-i18n="donate.gh.b">Become a sponsor</a>
packaging/homebrew/quickshort.rb:2:# Publish in a custom tap repo `truongnat/homebrew-tap` at Casks/quickshort.rb;
packaging/homebrew/quickshort.rb:5:#   brew tap truongnat/tap
packaging/linux/package.sh:32:Maintainer: truongnat <noreply@github.com>
packaging/winget/quickshort.installer.yaml:2:PackageIdentifier: truongnat.quickshort
packaging/winget/quickshort.locale.en-US.yaml:2:PackageIdentifier: truongnat.quickshort
packaging/winget/quickshort.locale.en-US.yaml:5:Publisher: truongnat
packaging/winget/quickshort.version.yaml:4:PackageIdentifier: truongnat.quickshort
relay/report/worker.js:14:const REPO = "truongnat/quickshort";
relay/report/worker.js:16:const ALLOW_ORIGIN = "https://truongnat.github.io";
migrate active links/metadata to '2tsoftwareapp' or move historical notes under specs/
```

---

## Phase 2: Foundational (Migration Guardrails)

**Purpose**: Establish constraints that prevent accidental regressions or destruction of existing user work before story implementation.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T003 Review the existing uncommitted changes in `packaging/linux/package.sh` and `relay/report/README.md` with `git diff -- packaging/linux/package.sh relay/report/README.md`; preserve both edits throughout implementation.
- [x] T004 Confirm the canonical destinations documented in `specs/005-github-org-migration/contracts/identity-contract.md` against the live checks already recorded in `specs/005-github-org-migration/research.md`: repo exists, tap cask exists, Pages is live, and Sponsors is live.
- [x] T005 Confirm explicitly that `packaging/flatpak/com.truongsoftware.quickshort.yaml` and `packaging/flatpak/com.truongsoftware.quickshort.metainfo.xml` are not renamed because the Flatpak application ID is a compatibility identifier, not a legacy GitHub owner reference.

**Checkpoint**: Audit harness exists and fails on the baseline; canonical targets and compatibility exclusions are confirmed.

---

## Phase 3: User Story 1 - Feedback and in-app destinations reach the live project (Priority: P1) 🎯 MVP

**Goal**: Users can reach the current project for help, feedback, and support, and the relay files issues in the current repository with the live Pages origin allowed.

**Independent Test**: Inspect the four changed runtime files, run Worker syntax validation, run the Rust verification gate, and verify no legacy identity remains in runtime paths.

### Implementation for User Story 1

- [x] T006 [P] [US1] Update `REPO` to `2tsoftwareapp/quickshort` and `ALLOW_ORIGIN` to `https://2tsoftwareapp.github.io` in `relay/report/worker.js` without changing payload parsing, labels, size limits, rate limiting, or responses.
- [x] T007 [P] [US1] Update all four `LINKS` entries in `crates/qs-app/src/settings/mod.rs` so GitHub and issue links use `github.com/2tsoftwareapp/quickshort` and Pages links use `2tsoftwareapp.github.io/quickshort`.
- [x] T008 [P] [US1] Update the fallback prefilled issue URL in `crates/qs-app/src/actions.rs` to `https://github.com/2tsoftwareapp/quickshort/issues/new?labels=bug&body=` while preserving the existing diagnostics encoding.
- [x] T009 [P] [US1] Update the tray Donate destination in `crates/qs-app/src/host/menus.rs` to `https://2tsoftwareapp.github.io/quickshort/#donate`.
- [x] T010 [US1] Validate the User Story 1 surface by running `node --check relay/report/worker.js` when Node is available and by running `cargo check --workspace --all-targets` plus `cargo test --workspace` from `Cargo.toml`; record whether the Node check ran or was unavailable.

**Checkpoint**: Runtime feedback and support destinations are canonical, the relay contract is unchanged except destination/origin, and the Rust workspace gate is green.

---

## Phase 4: User Story 2 - Installation and release automation use the current publisher identity (Priority: P2)

**Goal**: Install instructions, package metadata, and publishing automation consistently use the canonical publisher identity.

**Independent Test**: Inspect packaging and workflow files, run shell/YAML-oriented static validation where available, and verify the canonical tap repository path exists.

### Implementation for User Story 2

- [x] T011 [P] [US2] Update Debian maintainer metadata in `packaging/linux/package.sh` to `Maintainer: 2tsoftwareapp <noreply@github.com>`, while preserving the existing uncommitted `UPDATE_INFORMATION` value `gh-releases-zsync|2tsoftwareapp|quickshort|latest|Quickshort-Linux.AppImage.zsync`.
- [x] T012 [P] [US2] Update `PackageIdentifier` to `2tsoftwareapp.quickshort` in `packaging/winget/quickshort.version.yaml`.
- [x] T013 [P] [US2] Update `PackageIdentifier` to `2tsoftwareapp.quickshort` and `Publisher` to `2tsoftwareapp` in `packaging/winget/quickshort.locale.en-US.yaml`.
- [x] T014 [P] [US2] Update `PackageIdentifier` to `2tsoftwareapp.quickshort` in `packaging/winget/quickshort.installer.yaml`.
- [x] T015 [P] [US2] Update the publish instructions/comments in `packaging/homebrew/quickshort.rb` to use `2tsoftwareapp/homebrew-tap` and `brew tap 2tsoftwareapp/tap` while leaving the already-canonical release URL and homepage unchanged.
- [x] T016 [P] [US2] Update the Homebrew release automation in `.github/workflows/publish-release.yml` to clone `https://github.com/2tsoftwareapp/homebrew-tap`, copy the canonical cask template from `packaging/homebrew/quickshort.rb` before applying version/sha256 updates, and keep the explicit no-`TAP_TOKEN` skip behavior.
- [x] T017 [P] [US2] Update `.github/FUNDING.yml` so the GitHub Sponsors target is `2tsoftwareapp` and the custom funding URL is `https://2tsoftwareapp.github.io/quickshort/#donate`.
- [x] T018 [US2] Validate the User Story 2 surface by running `bash -n packaging/linux/package.sh`, inspecting `.github/workflows/publish-release.yml`, and confirming `gh api repos/2tsoftwareapp/homebrew-tap/contents/Casks/quickshort.rb --jq .name` prints `quickshort.rb`.

**Checkpoint**: All release/channel source metadata points at canonical publishing targets, with no fallback to the legacy tap.

---

## Phase 5: User Story 3 - Public documentation and demos present one coherent project home (Priority: P3)

**Goal**: Public-facing written and rendered surfaces consistently show the canonical project home and sponsorship destination.

**Independent Test**: Inspect all public documentation, demo, and video configuration files; run the migration audit over those paths; spot-check canonical live destinations.

### Implementation for User Story 3

- [x] T019 [P] [US3] Update Homebrew copy commands in `README.md` to `brew tap 2tsoftwareapp/tap` and update the GitHub Sponsors link in `README.md` to `https://github.com/sponsors/2tsoftwareapp`.
- [x] T020 [P] [US3] Update every release asset URL, demo end-card URL, and release metrics API example in `LAUNCH.md` from the legacy owner to `2tsoftwareapp`, and update the Winget submission path to `manifests/2/2tsoftwareapp/quickshort/1.0.0/`.
- [x] T021 [P] [US3] Update the copyable Homebrew command and GitHub Sponsors link in `docs/index.html` to the canonical tap and `https://github.com/sponsors/2tsoftwareapp`.
- [x] T022 [P] [US3] Update the displayed repository URL in `demo/ui-2026.html` to `github.com/2tsoftwareapp/quickshort`.
- [x] T023 [P] [US3] Update all repository links and the copyable Homebrew command in `demo/landing-v2.html` to the canonical identity.
- [x] T024 [P] [US3] Update the displayed repository URL in `demo/index.html` to `github.com/2tsoftwareapp/quickshort`.
- [x] T025 [P] [US3] Update the displayed releases URL in `demo/desktop-sim.html` to `github.com/2tsoftwareapp/quickshort/releases`.
- [x] T026 [P] [US3] Update `url` in `appshot-video/src/app-config.ts` to `github.com/2tsoftwareapp/quickshort`.
- [x] T027 [US3] Validate the User Story 3 surface by running `git grep -n 'truongnat' -- README.md LAUNCH.md docs demo appshot-video` and confirming it returns no matches.

**Checkpoint**: Public docs and demo surfaces have no active legacy references.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Verify the complete migration across all stories without expanding scope.

- [x] T028 Run `scripts/check-github-identity.sh` and confirm it exits successfully with no active legacy identity matches.
- [x] T029 Run static validation from `specs/005-github-org-migration/quickstart.md`: `node --check relay/report/worker.js` when available, `bash -n packaging/linux/package.sh`, and `git diff --check`.
- [x] T030 Run the Rust verification gate from `Cargo.toml`: `cargo check --workspace --all-targets` and `cargo test --workspace`; fix new warnings rather than suppressing them.
- [x] T031 Run the canonical destination probes from `specs/005-github-org-migration/quickstart.md` for `https://github.com/2tsoftwareapp/quickshort`, `https://github.com/2tsoftwareapp/homebrew-tap`, `https://2tsoftwareapp.github.io/quickshort/`, and `https://github.com/sponsors/2tsoftwareapp`.
- [x] T032 Review the final diff with `git diff -- crates relay packaging .github docs README.md LAUNCH.md demo appshot-video scripts` and confirm the two pre-existing edits in `packaging/linux/package.sh` and `relay/report/README.md` remain intact and no unrelated audit recommendations were implemented.
- [x] T033 Request explicit maintainer approval for the real-world relay validation in `specs/005-github-org-migration/quickstart.md`; only after approval, redeploy `relay/report/worker.js` with `relay/report/wrangler.toml`, verify the CORS preflight, submit one clearly marked controlled feature request, confirm it lands in `2tsoftwareapp/quickshort`, and close that test issue.
- [x] T034 Record completion notes in `specs/005-github-org-migration/tasks.md`, including whether the Worker syntax check ran, exact cargo test totals, whether relay deployment/controlled-issue validation was approved and completed, and any remaining operational follow-up.
- [x] T035 Request explicit maintainer approval before updating the already-published `2tsoftwareapp/homebrew-tap` repository; the live `Casks/quickshort.rb` currently still references `truongnat` in comments, `url`, and `homepage`, so either push the corrected cask after approval or explicitly defer that remote update until the next automated release.

## Completion Notes (2026-11-25)

- Migration implementation completed across runtime, packaging/release automation, documentation, demos, and video configuration. The audit harness now exits successfully with no active `truongnat` matches (`audit_status=0`).
- Static validation completed: `node --check relay/report/worker.js` passed, `bash -n packaging/linux/package.sh` passed, edited YAML files parsed successfully, and `git diff --check` passed.
- Rust verification gate passed: `cargo check --workspace --all-targets` exited 0; `cargo test --workspace` exited 0 across 43 test-result suites with 352 passed, 0 failed, and 5 ignored. The build still emits the pre-existing `unused import: stack::shelf_logical_size` warning in `crates/qs-app/src/chrome.rs`; no new warning was introduced or suppressed.
- Verification unblocker: `tempfile = "3"` was added to `crates/qs-app/Cargo.toml` `[dev-dependencies]` because existing `update.rs` tests already used `tempfile` on non-Linux hosts while the dependency was Linux-only. This was required to make the pre-existing test target compile; it does not add a runtime dependency.
- Canonical destination probes all returned HTTP 200 for `https://github.com/2tsoftwareapp/quickshort`, `https://github.com/2tsoftwareapp/homebrew-tap`, `https://2tsoftwareapp.github.io/quickshort/`, and `https://github.com/sponsors/2tsoftwareapp`.
- Maintainer explicitly approved real-world relay validation. `npx wrangler deploy` succeeded for `quickshort-report` version `a157ffa5-1906-4c83-b13a-c44044b14a4c`; both `https://report.2tsoftware.app` and `https://quickshort-report.truongnat.workers.dev` now answer CORS preflight with `access-control-allow-origin: https://2tsoftwareapp.github.io`.
- Controlled feature-request validation succeeded: the relay created `https://github.com/2tsoftwareapp/quickshort/issues/30` with labels `enhancement` + `user-request`; the issue was confirmed in the canonical repo and closed with the planned validation comment.
- Maintainer explicitly approved the live Homebrew tap correction. `2tsoftwareapp/homebrew-tap` commit `e3c4bb11fbdce180e134539b382335c1f4c11b30` replaced four legacy references in `Casks/quickshort.rb` while preserving the live cask version `0.4.0` and sha256 `1ca5afd244bc8df277bf24d79685352624abb9e033581a4d4f216ef53be014e7`; post-update inspection found zero `truongnat` references in the live cask.
- The two pre-existing uncommitted edits in `packaging/linux/package.sh` and `relay/report/README.md` remain represented in the working tree; no unrelated audit recommendations (`rfd` alignment, monolithic-file refactors, stitch `.expect()`, CI expansion, recording architecture, or `df` portability) were implemented.
- Remaining operational follow-up: none for this migration. Future releases should use the corrected workflow to keep the Homebrew cask synchronized automatically.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies; creates the audit harness first.
- **Foundational (Phase 2)**: Depends on Setup; blocks all story implementation because it establishes preserved edits and canonical targets.
- **User Story 1 (Phase 3)**: Depends on Foundational; highest priority and independently testable.
- **User Story 2 (Phase 4)**: Depends on Foundational; can run in parallel with US1 if desired because files do not overlap.
- **User Story 3 (Phase 5)**: Depends on Foundational; can run in parallel with US1/US2 because files do not overlap.
- **Polish (Phase 6)**: Depends on all selected user stories being complete.

### User Story Dependencies

- **US1**: No dependency on US2/US3.
- **US2**: No dependency on US1/US3, though final verification is shared.
- **US3**: No dependency on US1/US2, though final verification is shared.

### Parallel Opportunities

- T006, T007, T008, and T009 touch different files and can run in parallel.
- T011 through T017 touch different packaging/workflow/funding files and can run in parallel.
- T019 through T026 touch different documentation/demo/video files and can run in parallel.
- Validation tasks T010, T018, and T027 must run after their respective story implementation tasks.

## Parallel Example: User Story 1

```bash
# These four file edits are independent:
Task: "Update relay/report/worker.js constants"
Task: "Update crates/qs-app/src/settings/mod.rs links"
Task: "Update crates/qs-app/src/actions.rs fallback URL"
Task: "Update crates/qs-app/src/host/menus.rs donate URL"
```

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1 and Phase 2.
2. Implement T006–T010.
3. Stop and validate runtime feedback surfaces independently.
4. Deploy the report Worker only after maintainer approval; deployment and controlled public issue creation are operational side effects, not silent implementation steps.

### Incremental Delivery

1. US1 restores feedback and active support paths first.
2. US2 aligns installation and release automation second.
3. US3 cleans public documentation and demos third.
4. Phase 6 proves zero active legacy references and verifies no scope creep into unrelated audit recommendations.

## Notes

- Do not implement unrelated audit recommendations (`rfd` alignment, monolithic-file refactors, stitch `.expect()`, clippy CI expansion, recording push architecture, or `df` portability) in this feature.
- Do not deploy the Cloudflare Worker, create test GitHub issues, publish package-manager metadata, or push tags as part of code implementation unless the user explicitly approves those real-world side effects.
- `scripts/check-github-identity.sh` intentionally scans active surfaces only; historical migration artifacts under `specs/` may mention the legacy identity.
