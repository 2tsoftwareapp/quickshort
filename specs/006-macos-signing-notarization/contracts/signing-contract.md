# Contract: Production macOS Signing Artifacts

## Inputs

The production workflow requires these secrets before compilation starts:

- `QS_DEV_ID_APPLICATION_P12`
- `QS_DEV_ID_APPLICATION_P12_PASSWORD`
- `QS_NOTARY_P8_B64`
- `QS_NOTARY_KEY_ID`
- `QS_NOTARY_ISSUER`

Missing values are a pre-build workflow failure. No partial/unsigned production artifact is a valid substitute.

## Script Contract

`packaging/macos/release.sh` keeps its existing interface:

```sh
packaging/macos/release.sh [version]
```

Additional optional environment inputs:

- `SIGNING_IDENTITY` — signing identity; production receives the full `Developer ID Application: ...` identity.
- `NOTARY_KEY` — filesystem path to App Store Connect `.p8` key.
- `NOTARY_KEY_ID` — App Store Connect API key ID.
- `NOTARY_ISSUER` — App Store Connect issuer ID.

When the three notarization inputs are present, the script must notarize and staple or fail. When they are absent, the script may produce local self-signed/ad-hoc artifacts for development only.

## Outputs

Production outputs keep their existing names:

- `dist/Quickshort-v<version>-macos.zip`
- `dist/Quickshort-v<version>-macos.zip.sha256`
- `dist/Quickshort-v<version>-macos.dmg`
- `dist/Quickshort-v<version>-macos.dmg.sha256`

The ZIP contains a stapled `Quickshort.app`. The DMG is itself stapled. Checksums describe final post-staple bytes.

## Failure Contract

The following must stop the build with nonzero exit status:

- missing or malformed production secrets;
- imported certificate not being a Developer ID Application certificate;
- `codesign` or `codesign --verify --deep --strict` failure;
- notary submission rejection/failure;
- stapler failure or validation failure.

## Non-goals

- No Apple ID password in CI.
- No secret values in logs, git, or documentation.
- No change to Ed25519 update signatures or artifact names.
- No production `.pkg` contract in this feature.
