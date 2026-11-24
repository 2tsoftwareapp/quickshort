//! `~/.config/quickshort/config.toml` — user-tweakable settings.
//! Missing file / unknown keys fall back to defaults; a corrupt file never
//! kills the app.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    #[default]
    Image,
    Video,
    Ocr,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoEffect {
    #[default]
    None,
    BorderShadow,
    RoundedShadow,
}

/// Snagit-style saved capture preset.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptureProfile {
    pub name: String,
    pub mode: CaptureMode,
    pub delay_secs: u8,
    pub include_cursor: bool,
    pub auto_effect: AutoEffect,
}

impl Default for CaptureProfile {
    fn default() -> Self {
        Self {
            name: "Quick Annotation".into(),
            mode: CaptureMode::Image,
            delay_secs: 0,
            include_cursor: false,
            auto_effect: AutoEffect::None,
        }
    }
}

pub fn default_profiles() -> Vec<CaptureProfile> {
    vec![
        CaptureProfile::default(),
        CaptureProfile {
            name: "Delayed Menu (3s)".into(),
            mode: CaptureMode::Image,
            delay_secs: 3,
            include_cursor: true,
            auto_effect: AutoEffect::None,
        },
        CaptureProfile {
            name: "Clean Border & Shadow".into(),
            mode: CaptureMode::Image,
            delay_secs: 0,
            include_cursor: false,
            auto_effect: AutoEffect::BorderShadow,
        },
        CaptureProfile {
            name: "OCR Text Grab".into(),
            mode: CaptureMode::Ocr,
            delay_secs: 0,
            include_cursor: false,
            auto_effect: AutoEffect::None,
        },
    ]
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Global hotkey in `global-hotkey` syntax, e.g. "CmdOrCtrl+Shift+S".
    pub hotkey: String,
    /// Global hotkey that captures the whole virtual screen without the
    /// region overlay — "CmdOrCtrl+Shift+F" by default. Empty string
    /// disables the second grab.
    pub hotkey_full: String,
    /// Screenshot folder override; `None` = `~/Pictures/quickshort`.
    pub save_dir: Option<PathBuf>,
    pub launch_at_login: bool,
    /// Once-a-day release check from GitHub.
    pub auto_update: bool,
    /// How many recent captures the tray menu lists.
    pub recent_max: u8,
    /// Selection/highlight accent preset name (see `accent_rgb`).
    pub accent: String,
    /// Also put the PNG on the clipboard after a Save.
    pub copy_after_save: bool,
    /// Share uploads to the user's own Google Drive instead of the
    /// 3-day quick-link worker (needs a build with a Drive OAuth client).
    pub share_to_drive: bool,
    /// Auto-delete captures older than this many days; 0 = keep forever.
    pub keep_days: u32,
    /// Overlay ⌘+key triggers — single chars (see `overlay_prefs`).
    pub key_save: String,
    pub key_copy: String,
    pub key_ocr: String,
    pub key_share: String,
    pub key_scroll: String,
    pub key_record: String,
    /// ⌘+key = undo, ⌘⇧+key = redo — one field drives both.
    pub key_undo: String,
    /// Overlay key that hands the selection to the editor.
    pub key_edit: String,
    /// What the overlay's Save (and the fullscreen grab) leads to — a key
    /// from `AFTER_CAPTURE`: "shelf" (platform capture feedback), "editor" (open
    /// Quickshort Editor, Snagit-style) or "copy" (clipboard + toast).
    pub after_capture: String,
    /// Colour theme — a key from `qs_theme::THEMES`.
    pub theme: String,
    /// tesseract language list, e.g. "eng", "eng+jpn", "eng+vie".
    pub ocr_lang: String,
    /// Where floating popups (toast, capture card, REC pill) dock —
    /// a key from `POPUP_POS`, e.g. "top-right".
    pub popup_pos: String,
    /// Unix ts of the last update check (auto-checks once a day).
    #[serde(default)]
    pub last_update_check: u64,
    /// Saved Snagit-style capture profiles.
    #[serde(default = "default_profiles")]
    pub profiles: Vec<CaptureProfile>,
    /// Index of the active capture profile.
    #[serde(default)]
    pub active_profile: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // Windows reserves Win+Shift+S for Snipping Tool — the dedicated
            // PrintScreen key is the next best thing (registration still
            // falls back to Ctrl+Shift+S if the shell owns it)
            hotkey: if cfg!(windows) {
                "PrintScreen"
            } else {
                "CmdOrCtrl+Shift+S"
            }
            .into(),
            // same chord, F for "full screen" — free on all three desktops
            hotkey_full: "CmdOrCtrl+Shift+F".into(),
            save_dir: None,
            launch_at_login: false,
            auto_update: true,
            recent_max: 8,
            accent: "blue".into(),
            copy_after_save: true,
            share_to_drive: false,
            keep_days: 0,
            key_save: "s".into(),
            key_copy: "c".into(),
            key_ocr: "t".into(),
            // ⌘L is the editor's Library — one key, one meaning
            key_share: "k".into(),
            key_scroll: "g".into(),
            key_record: "r".into(),
            key_undo: "z".into(),
            key_edit: "e".into(),
            after_capture: "shelf".into(),
            theme: "light".into(),
            ocr_lang: "auto".into(),
            popup_pos: "top-right".into(),
            last_update_check: 0,
            profiles: default_profiles(),
            active_profile: 0,
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        qs_core::config::path()
    }

    pub fn read() -> std::io::Result<Self> {
        match qs_core::config::read(&Self::path()) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            result => result,
        }
    }

    pub fn load() -> Self {
        let cfg = Self::read().unwrap_or_else(|error| {
            eprintln!("config read: {error}");
            Self::default()
        });
        cfg.apply_appearance();
        cfg
    }

    pub fn apply_appearance(&self) {
        qs_theme::set_theme(&self.theme);
        qs_theme::set_accent(self.accent_rgb());
    }

    /// Apply appearance changes made by another Quickshort process.
    pub fn refresh_appearance(&mut self) -> bool {
        let Ok(current) = Self::read() else {
            return false;
        };
        if current.theme == self.theme && current.accent == self.accent {
            return false;
        }
        current.apply_appearance();
        self.theme = current.theme;
        self.accent = current.accent;
        true
    }

    pub fn save_changes(&self, before: &Self) {
        if let Err(error) = qs_core::config::save_changes(&Self::path(), before, self) {
            eprintln!("config write: {error}");
        }
    }

    /// Effective screenshot folder (default `~/Pictures/quickshort`).
    pub fn save_dir(&self) -> PathBuf {
        self.save_dir.clone().unwrap_or_else(|| {
            dirs::picture_dir()
                .or_else(dirs::home_dir)
                .unwrap_or_else(|| PathBuf::from("."))
                .join("quickshort")
        })
    }

    /// Accent preset name -> 0xRRGGBB. Unknown names fall back to blue.
    pub fn accent_rgb(&self) -> u32 {
        accent_rgb(&self.accent)
    }

    /// Overlay prefs mapped from config (accent + ⌘+key triggers).
    pub fn overlay_prefs(&self) -> qs_overlay::Prefs {
        let key = |s: &str, d: char| s.chars().next().unwrap_or(d).to_ascii_lowercase();
        qs_overlay::Prefs {
            accent: self.accent_rgb(),
            keys: qs_overlay::Keys {
                save: key(&self.key_save, 's'),
                copy: key(&self.key_copy, 'c'),
                ocr: key(&self.key_ocr, 't'),
                share: key(&self.key_share, 'k'),
                scroll: key(&self.key_scroll, 'g'),
                record: key(&self.key_record, 'r'),
                undo: key(&self.key_undo, 'z'),
                edit: key(&self.key_edit, 'e'),
            },
        }
    }
}

/// Post-capture behaviours offered by the Appearance page — (key, label).
/// Unknown keys fall back to the first entry.
pub const AFTER_CAPTURE: &[(&str, &str)] = &[
    #[cfg(target_os = "macos")]
    ("shelf", "Show toast"),
    #[cfg(not(target_os = "macos"))]
    ("shelf", "Show thumbnail"),
    ("editor", "Open in editor"),
    ("copy", "Copy to clipboard"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AfterCapture {
    Shelf,
    Editor,
    Copy,
}

impl Config {
    pub fn after_capture(&self) -> AfterCapture {
        match self.after_capture.as_str() {
            "editor" => AfterCapture::Editor,
            "copy" => AfterCapture::Copy,
            _ => AfterCapture::Shelf,
        }
    }
}

/// Popup dock positions offered by the Appearance page — (key, label).
/// Unknown keys fall back to the first entry.
pub const POPUP_POS: &[(&str, &str)] = &[
    ("top-right", "Top right"),
    ("top-left", "Top left"),
    ("top-center", "Top center"),
    ("bottom-right", "Bottom right"),
    ("bottom-left", "Bottom left"),
    ("bottom-center", "Bottom center"),
];

/// Accent presets offered by the Appearance page — (name, rgb), shared from `qs-theme`.
pub const ACCENTS: &[(&str, u32)] = qs_theme::ACCENTS;

pub fn accent_rgb(name: &str) -> u32 {
    qs_theme::accent_by_name(name)
}

/// Accent variant that can carry white text or glyphs — the raw presets
/// are tuned for vibrancy, not legibility (white on orange is 2.0:1).
/// Darkens the hue until white reaches 4.5:1.
pub fn accent_deep(name: &str) -> u32 {
    deepen(accent_rgb(name))
}

/// Same darkening for a raw accent value (chrome passes RGB, not names).
pub fn deepen(rgb: u32) -> u32 {
    let (mut r, mut g, mut b) = (
        ((rgb >> 16) & 0xFF) as f64,
        ((rgb >> 8) & 0xFF) as f64,
        (rgb & 0xFF) as f64,
    );
    for _ in 0..32 {
        if 1.05 / (luminance(r, g, b) + 0.05) >= 4.5 {
            break;
        }
        r *= 0.92;
        g *= 0.92;
        b *= 0.92;
    }
    ((r.round() as u32) << 16) | ((g.round() as u32) << 8) | b.round() as u32
}

fn luminance(r: f64, g: f64, b: f64) -> f64 {
    fn lin(c: f64) -> f64 {
        let c = c / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_accent_carries_white_content() {
        for (name, _) in ACCENTS {
            let d = accent_deep(name);
            let (r, g, b) = (
                ((d >> 16) & 0xFF) as f64,
                ((d >> 8) & 0xFF) as f64,
                (d & 0xFF) as f64,
            );
            let ratio = 1.05 / (luminance(r, g, b) + 0.05);
            assert!(ratio >= 4.5, "{name}: {ratio:.2}:1");
        }
    }
}
