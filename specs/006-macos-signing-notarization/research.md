# Research: macOS Developer ID Signing and Notarization

## Decisions

### Use an App Store Connect API key for CI notarization

`notarytool` supports App Store Connect API keys via `--key`, `--key-id`, and `--issuer`. This is preferable to Apple ID + app-specific password in CI because it avoids interactive 2FA and keeps authentication scoped to API credentials that can be revoked independently.

Required GitHub secrets:

- `QS_DEV_ID_APPLICATION_P12` — base64-encoded Developer ID Application certificate + private key export.
- `QS_DEV_ID_APPLICATION_P12_PASSWORD` — password for that `.p12`.
- `QS_NOTARY_P8_B64` — base64-encoded App Store Connect API private key (`.p8`).
- `QS_NOTARY_KEY_ID` — App Store Connect API key ID.
- `QS_NOTARY_ISSUER` — App Store Connect issuer ID for a team key.

The distinct `QS_DEV_ID_APPLICATION_*` names prevent confusion with the legacy self-signed `QS_SIGN_P12` secret.

### Production signing is strict; local development remains flexible

The production release workflow should fail fast if credentials are missing or if the certificate is not a Developer ID Application certificate. Silently producing unsigned release artifacts would preserve the defect this feature removes.

`packaging/macos/release.sh` remains useful locally without Apple credentials: when notarization variables are absent it keeps the existing self-signed/ad-hoc behavior. The strictness lives in the production workflow credential validation plus the script's validation whenever notarization variables are present.

### Staple the app before zipping

Apple's notary service accepts ZIP files, but a ZIP file itself cannot carry a stapled ticket. The correct sequence is:

1. Sign `Quickshort.app`.
2. Create a temporary ZIP solely for notary submission.
3. Submit that ZIP with `notarytool --wait`.
4. Staple the ticket to `Quickshort.app`.
5. Create the final distributable ZIP containing the stapled app.
6. Calculate the ZIP checksum after the final ZIP is created.

This matters because Homebrew and the updater consume the ZIP artifact.

### Notarize and staple the DMG separately

The DMG is itself a distribution container and can be stapled. The correct sequence is:

1. Create the DMG from the stapled app.
2. Submit the DMG with `notarytool --wait`.
3. Staple the DMG.
4. Validate the DMG staple.
5. Calculate the DMG checksum after stapling, because stapling mutates the file.

### Keep `.pkg` out of this feature

`packaging/macos/installer.sh` already has a local path for `.pkg` signing and notarization using a keychain profile, but `.pkg` is currently only a preview artifact and is not part of the published release asset contract. Production `.pkg` signing requires a separate Developer ID Installer certificate and release-contract decision, so it remains future work.

### Permission continuity caveat

The existing self-signed identity and a Developer ID identity are different TCC signing identities. Users upgrading across that transition may need to grant Screen Recording/Accessibility once again. Subsequent releases signed with the same Developer ID certificate and bundle identifier should keep grants stable.

## Alternatives Considered

- **Apple ID + app-specific password**: works, but couples CI to a personal Apple ID and 2FA account. Rejected in favor of App Store Connect API key.
- **Notarize only the DMG**: insufficient because Homebrew/updater consume the ZIP. The app inside the ZIP should be stapled before zipping.
- **Keep the Homebrew quarantine stripping step**: rejected for production because it bypasses Gatekeeper even when artifacts are properly signed and notarized.
- **Require a separate signing mode variable**: unnecessary complexity. Presence of the explicit production secrets plus workflow validation is enough for this repository's maintainer-driven release workflow.
