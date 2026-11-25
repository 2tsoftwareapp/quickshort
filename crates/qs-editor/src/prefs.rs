//! Quickshort settings used by the standalone editor.

use serde::Deserialize;
use std::path::PathBuf;

pub const AFTER_CAPTURE_OPTIONS: &[(&str, &str)] = &[
    #[cfg(target_os = "macos")]
    ("shelf", "Show toast"),
    #[cfg(not(target_os = "macos"))]
    ("shelf", "Show Shelf"),
    ("editor", "Open Editor"),
    ("copy", "Copy to Clipboard"),
];

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(default)]
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
    /// Saved default watermark text so toggling Watermark on pre-fills it.
    pub watermark_text: String,
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
            watermark_text: String::new(),
        }
    }
}

impl AppPrefs {
    pub fn load() -> Self {
        match Self::read() {
            Ok(prefs) => prefs,
            Err(error) => {
                log::warn!("editor settings: {error}");
                Self::default()
            }
        }
    }

    pub fn read() -> std::io::Result<Self> {
        match qs_core::config::read(&qs_core::config::path()) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            result => result,
        }
    }

    /// The capture folder: what the app config says, else the documented
    /// default (`~/Pictures/quickshort`).
    pub fn capture_dir(&self) -> PathBuf {
        self.save_dir
            .clone()
            .filter(|path| !path.as_os_str().is_empty())
            .or_else(qs_library::default_dir)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn save_theme(theme: &str) -> std::io::Result<()> {
        qs_core::config::update(
            &qs_core::config::path(),
            &std::collections::BTreeMap::from([("theme", theme)]),
        )
    }

    pub fn save_watermark(text: &str) -> std::io::Result<()> {
        qs_core::config::update(
            &qs_core::config::path(),
            &std::collections::BTreeMap::from([("watermark_text", text)]),
        )
    }

    /// Apply live theme + accent to `qs-theme` immediately.
    pub fn apply_live(&self) {
        qs_theme::set_theme(&self.theme);
        qs_theme::set_accent(qs_theme::accent_by_name(&self.accent));
    }
}
