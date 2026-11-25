# Quickstart Validation: macOS Signing and Notarization

## Static validation

```sh
sh -n packaging/macos/release.sh
python3 - <<'PY'
import yaml
with open('.github/workflows/release.yml') as f:
    yaml.safe_load(f)
print('release workflow yaml ok')
PY
git diff --check
```

## Credential setup contract

Required GitHub secrets:

- `QS_DEV_ID_APPLICATION_P12`
- `QS_DEV_ID_APPLICATION_P12_PASSWORD`
- `QS_NOTARY_P8_B64`
- `QS_NOTARY_KEY_ID`
- `QS_NOTARY_ISSUER`

The `.p12` and `.p8` values are stored base64-encoded. Never paste private key material into source files, logs, or chat.


## Apple credential creation outline

Do not create, download, revoke, or replace these Apple assets without explicit maintainer approval.

1. **Developer ID Application certificate**
   - Create a certificate signing request locally with Keychain Access.
   - In the Apple Developer Certificates UI, create/download a **Developer ID Application** certificate.
   - Import it into the login keychain, export the certificate plus private key as `.p12`, then encode it:

     ```sh
     base64 -i developer-id-application.p12 | pbcopy
     ```

   - Store the copied value in GitHub secret `QS_DEV_ID_APPLICATION_P12`; store the export password in `QS_DEV_ID_APPLICATION_P12_PASSWORD`.

2. **App Store Connect API team key**
   - In App Store Connect → Users and Access → Integrations → App Store Connect API, create a team key with sufficient access for notarization (Developer access is normally enough).
   - Download the `.p8` immediately; Apple only offers the private key once.
   - Encode it:

     ```sh
     base64 -i AuthKey_KEYID.p8 | pbcopy
     ```

   - Store the copied value in `QS_NOTARY_P8_B64`, the 10-character key ID in `QS_NOTARY_KEY_ID`, and the issuer UUID in `QS_NOTARY_ISSUER`.

3. Verify secret presence without printing values:

   ```sh
   gh secret list --repo 2tsoftwareapp/quickshort
   ```

## Controlled release-build validation

After secrets are configured and the maintainer approves a CI artifact build:

1. Run the `release` workflow with `installer_preview=false`.
2. Download `dist-macos` artifacts from that workflow run.
3. Verify locally:

```sh
unzip -q Quickshort-v<version>-macos.zip -d qs-verify
codesign --verify --deep --strict qs-verify/Quickshort.app
codesign -dvv qs-verify/Quickshort.app 2>&1 | grep 'Authority=Developer ID Application:'
xcrun stapler validate qs-verify/Quickshort.app
xcrun stapler validate Quickshort-v<version>-macos.dmg
shasum -a 256 -c Quickshort-v<version>-macos.zip.sha256
shasum -a 256 -c Quickshort-v<version>-macos.dmg.sha256
```

4. Copy the app to a clean test location and launch it. Gatekeeper should not require the unsigned-build right-click/Open or `xattr` workaround. Screen Recording and Accessibility prompts remain expected when features need them.

## Operational note

Do not publish a release or push a tag as part of validation without explicit maintainer approval.
