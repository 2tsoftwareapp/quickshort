# Quickstart Validation: GitHub Identity Migration Consistency

Use this guide to validate the implementation without publishing a release.

## 1. Baseline audit

After creating the audit script but before editing product files:

```bash
scripts/check-github-identity.sh
```

Expected at baseline: nonzero exit with a list of active `truongnat` references.

Expected after the fix: zero output and exit status 0.

## 2. Static checks

```bash
node --check relay/report/worker.js
bash -n packaging/linux/package.sh
cargo check --workspace --all-targets
cargo test --workspace
```

Expected: all pass with no new warnings. If Node is unavailable, record that the Worker syntax check was skipped rather than treating it as passed.

## 3. Canonical destination probes

```bash
gh api repos/2tsoftwareapp/quickshort --jq .full_name
gh api repos/2tsoftwareapp/homebrew-tap/contents/Casks/quickshort.rb --jq .name
curl -s -o /dev/null -w '%{http_code}\n' -L https://2tsoftwareapp.github.io/quickshort/
curl -s -o /dev/null -w '%{http_code}\n' -L https://github.com/sponsors/2tsoftwareapp
```

Expected:

- repo API prints `2tsoftwareapp/quickshort`
- cask API prints `quickshort.rb`
- Pages and Sponsors probes print a successful final HTTP status such as `200`

## 4. App destination inspection

Inspect these files after implementation:

- `crates/qs-app/src/settings/mod.rs`
- `crates/qs-app/src/actions.rs`
- `crates/qs-app/src/host/menus.rs`

Expected: every GitHub/Pages destination uses the canonical identity. Because only URL constants change and no layout changes, the constitution's four-viewport visual pass is not required unless implementation causes URLs to be visibly rendered in UI.

## 5. Release/channel inspection

Inspect:

- `packaging/homebrew/quickshort.rb`
- `.github/workflows/publish-release.yml`
- `packaging/winget/*.yaml`
- `packaging/linux/package.sh`
- `.github/FUNDING.yml`

Expected: future publish/copy operations target `2tsoftwareapp` consistently. `packaging/linux/package.sh` must still contain the already-fixed AppImage update owner `2tsoftwareapp`.

## 6. Relay post-deploy validation

After deploying `relay/report/worker.js` with Wrangler:

```bash
curl -i -X OPTIONS \
  -H 'Origin: https://2tsoftwareapp.github.io' \
  -H 'Access-Control-Request-Method: POST' \
  -H 'Access-Control-Request-Headers: Content-Type' \
  https://report.2tsoftware.app/
```

Expected: preflight succeeds and includes `Access-Control-Allow-Origin: https://2tsoftwareapp.github.io`.

Then, if acceptable to the maintainer, submit one controlled feature request titled clearly as a migration test. Expected: it creates an issue in `2tsoftwareapp/quickshort`; close that test issue immediately afterward. Do not run this step against production without maintainer approval because it creates a public issue.

## 7. Final audit

```bash
scripts/check-github-identity.sh
git diff --check
```

Expected: no active legacy identity references and no whitespace errors.
