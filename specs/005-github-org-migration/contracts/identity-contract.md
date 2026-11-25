# Contract: Public Identity and Feedback Destinations

## Report relay

The report relay continues to accept the existing POST payloads for bug reports, feature requests, and supporter submissions. No field names, labels, length limits, or rate limits change.

Changed externally visible behavior:

- Browser requests originating from `https://2tsoftwareapp.github.io` MUST be allowed by CORS.
- Issues created by the relay MUST be created in `https://github.com/2tsoftwareapp/quickshort/issues`.
- The retired origin `https://truongnat.github.io` MUST NOT be required for the live form to work.

Compatibility requirement:

- Older installed app builds can keep posting the same payload to the same relay endpoint; they do not need to know the repository owner.

## User-facing URL contract

Active references owned by this repository use these canonical forms:

| Purpose | Canonical destination |
|---|---|
| Repository | `https://github.com/2tsoftwareapp/quickshort` |
| Issues | `https://github.com/2tsoftwareapp/quickshort/issues` |
| Pages site | `https://2tsoftwareapp.github.io/quickshort/` |
| Sponsors | `https://github.com/sponsors/2tsoftwareapp` |
| Homebrew tap command | `brew tap 2tsoftwareapp/tap` |
| Homebrew tap repository | `https://github.com/2tsoftwareapp/homebrew-tap` |
| Winget package identifier for future submissions | `2tsoftwareapp.quickshort` |
| Winget submission path for v1.0.0 | `manifests/2/2tsoftwareapp/quickshort/1.0.0/` |
| Debian package maintainer metadata | `2tsoftwareapp <noreply@github.com>` |

## Non-goals

- No change to the share worker contract.
- No change to update signature verification.
- No change to Flatpak application ID.
- No retroactive mutation of already-installed binaries or already-published external package metadata.
