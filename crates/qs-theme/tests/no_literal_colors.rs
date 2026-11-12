//! Guard: migrated surfaces must take colours from `qs_theme::pal()`.
//! Extend MIGRATED as more files move over. Test modules are exempt;
//! a non-colour literal opts out with an `// allow-hex: <reason>` comment.

use std::fs;
use std::path::Path;

const MIGRATED: &[&str] = &[
    "qs-app/src/settings.rs",
    "qs-app/src/toast.rs",
    "qs-app/src/quick_access.rs",
    "qs-app/src/recstop.rs",
    "qs-app/src/pin.rs",
    "qs-app/src/chrome.rs",
    "qs-app/src/ui.rs",
    "qs-overlay/src/toolbar.rs",
    "qs-overlay/src/sdf.rs",
    "qs-overlay/src/annotate.rs",
    "qs-overlay/src/lib.rs",
    "qs-editor/src/theme.rs",
    "qs-editor/src/app.rs",
    "qs-editor/src/canvas.rs",
    "qs-editor/src/icons.rs",
    "qs-editor/src/toolbar.rs",
];

/// A line may keep a literal when it says why: `// allow-hex: <reason>`
/// (bit masks, document data — never a UI colour).
const ALLOW: &str = "allow-hex:";

fn has_hex(line: &str) -> bool {
    let b = line.as_bytes();
    (0..b.len().saturating_sub(7)).any(|i| {
        b[i] == b'0'
            && b[i + 1] == b'x'
            && b[i + 2..i + 8].iter().all(|c| c.is_ascii_hexdigit())
            && !b.get(i + 8).is_some_and(|c| c.is_ascii_alphanumeric())
    })
}

#[test]
fn no_hex_colour_literals_in_migrated_files() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut bad = vec![];
    for f in MIGRATED {
        let src = fs::read_to_string(crates.join(f)).unwrap();
        let body = src.split("#[cfg(test)]").next().unwrap();
        for (n, l) in body.lines().enumerate() {
            if has_hex(l) && !l.contains(ALLOW) && !l.trim_start().starts_with("//") {
                bad.push(format!("{f}:{}: {}", n + 1, l.trim()));
            }
        }
    }
    assert!(bad.is_empty(), "use qs_theme::pal() instead:\n{}", bad.join("\n"));
}
