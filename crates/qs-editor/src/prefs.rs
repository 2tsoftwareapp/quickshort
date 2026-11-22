//! Shared Quickshort `config.toml` preferences reader/writer for `qs-editor`.
//!
//! Reads and updates `theme`, `accent`, `after_capture`, and `copy_after_save`
//! in-place while preserving all other keys and `[[profiles]]` sections owned
//! by `qs-app`.

use std::path::{Path, PathBuf};

pub const AFTER_CAPTURE_OPTIONS: &[(&str, &str)] = &[
    ("shelf", "Show Shelf"),
    ("editor", "Open Editor"),
    ("copy", "Copy to Clipboard"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppPrefs {
    pub theme: String,
    pub accent: String,
    pub after_capture: String,
    pub copy_after_save: bool,
    /// `save_dir` from the shared config, when the user set one. The editor
    /// only reads it to point the Library at the folder captures land in.
    pub save_dir: Option<PathBuf>,
    /// `ocr_lang` from the shared config; the Smart Move hints recognize with
    /// it. `auto` lets the engine detect.
    pub ocr_lang: String,
}

impl Default for AppPrefs {
    fn default() -> Self {
        Self {
            theme: "light".into(),
            accent: "blue".into(),
            after_capture: "shelf".into(),
            copy_after_save: true,
            save_dir: None,
            ocr_lang: "auto".into(),
        }
    }
}

impl AppPrefs {
    /// Same config path as `qs-app::config::Config::path()`.
    pub fn config_path() -> PathBuf {
        let base = if cfg!(target_os = "macos") {
            std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join("Library").join("Application Support"))
        } else if cfg!(target_os = "windows") {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        };
        base.unwrap_or_else(|| PathBuf::from("."))
            .join("quickshort")
            .join("config.toml")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::config_path())
    }

    pub fn load_from(path: &Path) -> Self {
        let mut prefs = Self::default();
        let Ok(raw) = std::fs::read_to_string(path) else {
            return prefs;
        };
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                // Stop at first table/section header (`[[profiles]]` etc.)
                break;
            }
            let Some((k, v)) = trimmed.split_once('=') else {
                continue;
            };
            let key = k.trim();
            let val = v.trim().trim_matches('"');
            match key {
                "theme" => prefs.theme = val.to_owned(),
                "accent" => prefs.accent = val.to_owned(),
                "after_capture" => prefs.after_capture = val.to_owned(),
                "copy_after_save" => prefs.copy_after_save = val == "true",
                "ocr_lang" => prefs.ocr_lang = val.to_owned(),
                "save_dir" => {
                    if !val.is_empty() {
                        prefs.save_dir = Some(PathBuf::from(val));
                    }
                }
                _ => {}
            }
        }
        prefs
    }

    /// The capture folder: what the app config says, else the documented
    /// default (`~/Pictures/quickshort`).
    pub fn capture_dir(&self) -> PathBuf {
        self.save_dir
            .clone()
            .or_else(qs_library::default_dir)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn save(&self) {
        self.save_to(&Self::config_path());
    }

    pub fn save_to(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let existing = std::fs::read_to_string(path).unwrap_or_default();
        let updated = patch_toml(&existing, self);
        let _ = std::fs::write(path, updated);
    }

    /// Apply live theme + accent to `qs-theme` immediately.
    pub fn apply_live(&self) {
        qs_theme::set_theme(&self.theme);
        qs_theme::set_accent(qs_theme::accent_by_name(&self.accent));
    }
}

fn patch_toml(existing: &str, prefs: &AppPrefs) -> String {
    let mut out = Vec::new();
    let mut seen_theme = false;
    let mut seen_accent = false;
    let mut seen_after = false;
    let mut seen_copy = false;
    let mut in_section = false;
    let mut insert_at = None;

    for line in existing.lines() {
        let trimmed = line.trim();
        if !in_section && trimmed.starts_with('[') {
            in_section = true;
            insert_at = Some(out.len());
        }
        if !in_section {
            if let Some((k, _)) = trimmed.split_once('=') {
                match k.trim() {
                    "theme" => {
                        out.push(format!("theme = \"{}\"", prefs.theme));
                        seen_theme = true;
                        continue;
                    }
                    "accent" => {
                        out.push(format!("accent = \"{}\"", prefs.accent));
                        seen_accent = true;
                        continue;
                    }
                    "after_capture" => {
                        out.push(format!("after_capture = \"{}\"", prefs.after_capture));
                        seen_after = true;
                        continue;
                    }
                    "copy_after_save" => {
                        out.push(format!("copy_after_save = {}", prefs.copy_after_save));
                        seen_copy = true;
                        continue;
                    }
                    _ => {}
                }
            }
        }
        out.push(line.to_owned());
    }

    let mut missing = Vec::new();
    if !seen_theme {
        missing.push(format!("theme = \"{}\"", prefs.theme));
    }
    if !seen_accent {
        missing.push(format!("accent = \"{}\"", prefs.accent));
    }
    if !seen_after {
        missing.push(format!("after_capture = \"{}\"", prefs.after_capture));
    }
    if !seen_copy {
        missing.push(format!("copy_after_save = {}", prefs.copy_after_save));
    }

    if !missing.is_empty() {
        let idx = insert_at.unwrap_or(out.len());
        out.splice(idx..idx, missing);
    }
    let mut s = out.join("\n");
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_other_keys_and_sections() {
        let dir = std::env::temp_dir().join(format!("qs_prefs_test_{}", std::process::id()));
        let path = dir.join("config.toml");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            &path,
            "hotkey = \"cmd+shift+4\"\ntheme = \"dark\"\n\n[[profiles]]\nname = \"Default\"\n",
        )
        .unwrap();

        let mut prefs = AppPrefs::load_from(&path);
        assert_eq!(prefs.theme, "dark");
        prefs.theme = "light".into();
        prefs.accent = "purple".into();
        prefs.after_capture = "editor".into();
        prefs.copy_after_save = true;
        prefs.save_to(&path);

        let reloaded = AppPrefs::load_from(&path);
        assert_eq!(reloaded, prefs);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("hotkey = \"cmd+shift+4\""));
        assert!(raw.contains("[[profiles]]"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
