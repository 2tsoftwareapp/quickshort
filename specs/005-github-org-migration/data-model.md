# Data Model: GitHub Identity Migration Consistency

This feature introduces no persistent user-data model, no `.qsdoc` schema change, and no runtime state transition stored by the app.

## Conceptual entities

| Entity | Fields / attributes | Validation rule |
|---|---|---|
| Canonical identity | namespace `2tsoftwareapp`; repo `quickshort`; Pages host `2tsoftwareapp.github.io`; tap `2tsoftwareapp/homebrew-tap` | Must be the destination for active GitHub-owned user-facing links, copyable install commands, issue creation, and release automation. |
| Legacy identity | namespace `truongnat` | May appear only in excluded historical/migration artifacts; must not appear in active runtime, packaging, automation, documentation, demo, or video surfaces. |
| Compatibility identifier | examples such as Flatpak `com.truongsoftware.quickshort` | Preserved when it is a package-manager compatibility key rather than a GitHub owner reference. |
| Feedback submission | existing report/feature/supporter payload and labels | Payload shape, labels, size limits, and rate limits remain unchanged; only the destination repository and browser origin change. |

## State transition

```text
legacy public identity (truongnat)
        │
        │ metadata-only migration
        ▼
canonical public identity (2tsoftwareapp)
```

There is no rollback state stored in user data. Rolling back source control changes only affects future builds/deployments; already-installed older builds continue to use whatever metadata they were built with.
