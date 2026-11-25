# Data Model: macOS Signing and Notarization

No persistent user data or document format changes are introduced.

## Credential Entities

| Entity | Storage | Required in production | Notes |
|---|---|---:|---|
| Developer ID Application `.p12` | GitHub secret `QS_DEV_ID_APPLICATION_P12` as base64 | yes | Contains certificate + private key; never committed. |
| `.p12` password | GitHub secret `QS_DEV_ID_APPLICATION_P12_PASSWORD` | yes | Used only during keychain import in CI. |
| App Store Connect `.p8` | GitHub secret `QS_NOTARY_P8_B64` as base64 | yes | Decoded to a runner temp file with restrictive permissions. |
| API key ID | GitHub secret `QS_NOTARY_KEY_ID` | yes | Public identifier, still stored as secret for operational simplicity. |
| API issuer ID | GitHub secret `QS_NOTARY_ISSUER` | yes | Required for team keys. |
| Legacy self-signed `.p12` | GitHub secret `QS_SIGN_P12` / local keychain | no | Development fallback only; not production after this feature. |

## Artifact State Transitions

```text
unsigned build products
  -> Quickshort.app signed with Developer ID Application
  -> temporary ZIP submitted to Apple notary service
  -> accepted ticket stapled into Quickshort.app
  -> final distributable ZIP created and checksummed
  -> DMG created from stapled app
  -> DMG submitted to Apple notary service
  -> accepted ticket stapled into DMG
  -> final DMG checksummed
```

## Validation State

- `codesign --verify --deep --strict Quickshort.app` must pass before archiving.
- `xcrun stapler validate Quickshort.app` must pass after app stapling.
- `xcrun stapler validate Quickshort-v<version>-macos.dmg` must pass after DMG stapling.
- Checksums are derived only from final post-staple bytes.
