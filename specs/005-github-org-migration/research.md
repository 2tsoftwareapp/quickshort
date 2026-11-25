# Research: GitHub Identity Migration Consistency

## Canonical destination verification

**Decision**: Use `2tsoftwareapp` as the canonical namespace for repo, Pages, Sponsors, and Homebrew tap.

**Evidence gathered on 2026-11-25**:

- `gh api repos/2tsoftwareapp/quickshort` resolved successfully.
- `gh api users/2tsoftwareapp` reports the account type as `User`; this does not change the namespace's role as the canonical public identity.
- `gh api repos/truongnat/quickshort` resolves through GitHub's repository transfer redirect to `2tsoftwareapp/quickshort`, which explains why stale repo links can appear to work even though they are still wrong.
- `gh api repos/2tsoftwareapp/homebrew-tap` resolved successfully.
- `gh api repos/2tsoftwareapp/homebrew-tap/contents/Casks/quickshort.rb` resolved successfully, so the tap already has the expected cask path.
- `https://2tsoftwareapp.github.io/quickshort/` returned HTTP 200.
- `https://github.com/sponsors/2tsoftwareapp` returned HTTP 200.
- `https://truongnat.github.io/quickshort/` returned HTTP 404.
- `https://github.com/sponsors/truongnat` returned HTTP 404.

**Rationale**: The new namespace is live and the old public Pages/Sponsors destinations are dead. Continuing to point users at the old namespace is a functional bug, not merely inconsistent branding.

**Alternatives considered**: Keep sponsor/tap personal because they may belong to an individual. Rejected because the canonical sponsor page and tap both exist under `2tsoftwareapp`, while the old sponsor page is unavailable.

## Public site origins

**Decision**: Keep the required browser origin for the feedback form as `https://2tsoftwareapp.github.io`.

**Evidence gathered on 2026-11-25**: `https://2tsoftware.app/quickshort/` and `https://www.2tsoftware.app/quickshort/` are live, but the fetched marketing page does not contain the repository's feedback form or the `report.2tsoftware.app` endpoint. The form that posts to the report worker is the GitHub Pages site in `docs/`.

**Rationale**: Supporting only the live form's actual origin is the smallest safe fix. If the same feedback form is later deployed on `2tsoftware.app`, the Worker can then switch to an explicit allowlist that reflects the request `Origin`; adding that mechanism now would be speculative.

**Alternatives considered**: Immediately allow both the GitHub Pages and custom-domain origins. Rejected for this phase because the custom domain does not currently host the form and multi-origin CORS requires more logic than the reported bug needs.

## Worker configuration style

**Decision**: Keep `REPO` and `ALLOW_ORIGIN` as hardcoded Worker constants, changing only their values.

**Rationale**: The existing deployment model treats the Worker source as configuration. Environment variables would introduce new deployment state and secret/config management without improving safety or removing the need to redeploy.

**Alternatives considered**: `env.REPO` and `env.ALLOW_ORIGIN`. Rejected as unnecessary operational complexity for two values that change only with project ownership.

## Shared URL constants in Rust

**Decision**: Do not introduce a shared URL constants module.

**Rationale**: Only three `qs-app` files contain user-facing legacy URLs, each in a different context. The regression is better prevented by the audit script across all file types than by a Rust-only abstraction that cannot cover YAML, shell, HTML, or JavaScript.

**Alternatives considered**: Add `urls.rs` or similar. Rejected under the constitution's no-speculative-abstraction constraint.

## Winget identity

**Decision**: Use `2tsoftwareapp.quickshort` in future Winget manifest templates.

**Rationale**: `LAUNCH.md` presents Winget submission as pending. Public checks on 2026-11-25 found neither `manifests/t/truongnat/quickshort` nor `manifests/2/2tsoftwareapp/quickshort` in `microsoft/winget-pkgs`, so there is no published identifier continuity to preserve. The correct future submission path uses `2` as the first character of `2tsoftwareapp`, not `t`.

**Alternatives considered**: Keep `truongnat.quickshort` for upgrade continuity. Rejected for source templates because continuity cannot be confirmed and the launch checklist says submission is still pending.

## Stable identifiers

**Decision**: Preserve `com.truongsoftware.quickshort` for Flatpak.

**Rationale**: Application IDs are compatibility identifiers consumed by package managers and user data. This string does not contain the legacy `truongnat` namespace and is not a GitHub ownership link.

**Alternatives considered**: Rename all branding-like identifiers. Rejected because it creates upgrade/data migration risk with no evidence of user benefit.

## Audit strategy

**Decision**: Add `scripts/check-github-identity.sh`, using `git grep` over active surfaces and exiting nonzero if `truongnat` remains.

**Rationale**: This is the narrowest repeatable proof of migration completeness. `git grep` is available wherever the repository is cloned and naturally ignores untracked build output.

**Alternatives considered**: A one-time manual audit, or a larger CI workflow expansion. A one-time audit is not repeatable enough; CI expansion is unnecessary for this fix because the script can be added to CI later without changing product behavior.

## Homebrew tap contents discovered during implementation

**Decision**: Make the release workflow copy the canonical cask template from this repository into `2tsoftwareapp/homebrew-tap` before applying version/sha256 updates, and track the current remote tap content as an operational follow-up requiring approval.

**Evidence gathered during implementation**: `2tsoftwareapp/homebrew-tap` exists and contains `Casks/quickshort.rb`, but the currently published cask still comments `truongnat/homebrew-tap`, shows `brew tap truongnat/tap`, and points `url`/`homepage` at `github.com/truongnat/quickshort` (which currently relies on GitHub's repository redirect).

**Rationale**: Simply cloning the new tap at the next release would not repair those stale fields because the old workflow only edited version and sha256 in-place. Copying the canonical template makes this repository the source of truth for future cask metadata. The already-published remote tap still needs a separate approved update if the current install path must stop relying on redirects before the next release.

## Existing working-tree edits

**Decision**: Treat the existing edits in `packaging/linux/package.sh` and `relay/report/README.md` as intentional baseline work.

**Rationale**: They already move the AppImage update metadata and report README toward the canonical target. Implementation must preserve them and complete the surrounding migration rather than reverting or duplicating them.
