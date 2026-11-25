//! Config plumbing shared by the pages — key-slot read/write,
//! dropdown value mapping, recent-shot thumbs and small utils.

use super::*;

pub(super) fn drop_state(cfg: &Config, d: Drop) -> (Vec<&'static str>, usize) {
    match d {
        Drop::RecentN => (
            vec!["4 items", "8 items", "12 items"],
            match cfg.recent_max {
                4 => 0,
                12 => 2,
                _ => 1,
            },
        ),
        Drop::KeepDays => (
            vec!["Forever", "7 days", "30 days"],
            match cfg.keep_days {
                7 => 1,
                30 => 2,
                _ => 0,
            },
        ),
        Drop::PopupPos => (
            POPUP_POS.iter().map(|(_, l)| *l).collect(),
            POPUP_POS
                .iter()
                .position(|(k, _)| *k == cfg.popup_pos)
                .unwrap_or(0),
        ),
        Drop::Theme => (
            THEMES.iter().map(|(_, l)| *l).collect(),
            THEMES
                .iter()
                .position(|(k, _)| *k == cfg.theme)
                .unwrap_or(0),
        ),
        Drop::AfterCapture => (
            AFTER_CAPTURE.iter().map(|(_, l)| *l).collect(),
            AFTER_CAPTURE
                .iter()
                .position(|(k, _)| *k == cfg.after_capture)
                .unwrap_or(0),
        ),
    }
}

pub(super) fn section_title(p: &Painter, cv: &mut Canvas, y: f64, title: &str, sub: &str) {
    p.text(cv, CX, y, 13.5, title, pal().fg);
    p.text(cv, CX, y + 19.0, 11.5, sub, pal().fg_dim);
}

pub(super) fn switch_row(
    p: &Painter,
    cv: &mut Canvas,
    v: &View,
    hits: &mut Vec<(LRect, Hit)>,
    r: LRect,
    (g, label, desc): (Glyph, &str, &str),
    sw: Sw,
) {
    if v.hover == Some(Hit::Sw(sw)) {
        p.fill(cv, r, 7.0, pal().surface_hover, 1.0);
    }
    row_label(p, cv, r, g, label, desc);
    p.switch(
        cv,
        (r.0 + r.2 - 14.0 - 44.0, r.1 + 13.0, 44.0, 26.0),
        v.sw_t[sw.idx()],
    );
    hits.push((r, Hit::Sw(sw)));
}

// config plumbing -----------------------------------------------------------

pub(super) fn key_str(cfg: &Config, r: Rec) -> &str {
    match r {
        Rec::Save => &cfg.key_save,
        Rec::Copy => &cfg.key_copy,
        Rec::Ocr => &cfg.key_ocr,
        Rec::Share => &cfg.key_share,
        Rec::Scroll => &cfg.key_scroll,
        Rec::Record => &cfg.key_record,
        Rec::Undo => &cfg.key_undo,
        Rec::Global | Rec::GlobalFull => "",
    }
}

/// Assign `ch` to a key slot; if another slot already uses it, swap so the
/// binding stays unique — silent duplicates would make one action dead.
pub(super) fn set_key(cfg: &mut Config, slot: Rec, ch: char) {
    let old = key_str(cfg, slot).chars().next().unwrap_or(' ');
    for other in [
        Rec::Save,
        Rec::Copy,
        Rec::Ocr,
        Rec::Share,
        Rec::Scroll,
        Rec::Record,
        Rec::Undo,
    ] {
        if other != slot && key_str(cfg, other).starts_with(ch) {
            *key_mut(cfg, other) = old.to_string();
        }
    }
    *key_mut(cfg, slot) = ch.to_string();
}

pub(super) fn key_mut(cfg: &mut Config, r: Rec) -> &mut String {
    match r {
        Rec::Save => &mut cfg.key_save,
        Rec::Copy => &mut cfg.key_copy,
        Rec::Ocr => &mut cfg.key_ocr,
        Rec::Share => &mut cfg.key_share,
        Rec::Scroll => &mut cfg.key_scroll,
        Rec::Record => &mut cfg.key_record,
        Rec::Undo => &mut cfg.key_undo,
        Rec::Global | Rec::GlobalFull => unreachable!(),
    }
}

pub(super) fn drop_apply(cfg: &mut Config, d: Drop, i: usize) {
    match d {
        Drop::RecentN => cfg.recent_max = [4u8, 8, 12][i.min(2)],
        Drop::KeepDays => {
            cfg.keep_days = [0u32, 7, 30][i.min(2)];
            // apply the new policy immediately, not just on next launch
            actions::cleanup_captures(&cfg.save_dir(), cfg.keep_days);
        }
        Drop::PopupPos => {
            cfg.popup_pos = POPUP_POS
                .get(i)
                .map(|(k, _)| *k)
                .unwrap_or(POPUP_POS[0].0)
                .into();
        }
        Drop::Theme => {
            cfg.theme = THEMES.get(i).map(|(k, _)| *k).unwrap_or(THEMES[0].0).into();
            // live: the next frame of every window reads the new palette
            qs_theme::set_theme(&cfg.theme);
        }
        Drop::AfterCapture => {
            cfg.after_capture = AFTER_CAPTURE
                .get(i)
                .map(|(k, _)| *k)
                .unwrap_or(AFTER_CAPTURE[0].0)
                .into();
        }
    }
}

/// Thumbnails sized in *physical* px for the window's current scale.
pub(super) fn load_thumbs(recents: &[PathBuf], scale: f64) -> Vec<Option<image::RgbaImage>> {
    let (tw, th) = ((THUMB_W * scale) as u32, (THUMB_H * scale) as u32);
    recents
        .iter()
        .map(|p| {
            image::open(p).ok().map(|i| {
                i.resize_to_fill(tw, th, image::imageops::FilterType::Triangle)
                    .to_rgba8()
            })
        })
        .collect()
}

/// Restore app activation + window focus after a modal panel closes —
/// without this an accessory app stays deactivated and the settings
/// window stops receiving keyboard input.
pub(super) fn refocus(win: &Window) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        if let Some(mtm) = MainThreadMarker::new() {
            NSApplication::sharedApplication(mtm).activate();
        }
    }
    win.focus_window();
    win.request_redraw();
}

/// Match the macOS window titlebar appearance to the active Light / Dark Glass theme.
#[cfg(target_os = "macos")]
pub(super) fn sync_window_appearance(win: &Window) {
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua,
        NSView,
    };
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = win.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(h) = handle.as_raw() else {
        return;
    };
    let ns_view: &NSView = unsafe { &*(h.ns_view.as_ptr() as *const NSView) };
    let Some(ns_win) = ns_view.window() else {
        return;
    };
    let name = if pal().dark {
        unsafe { NSAppearanceNameDarkAqua }
    } else {
        unsafe { NSAppearanceNameAqua }
    };
    ns_win.setAppearance(NSAppearance::appearanceNamed(name).as_deref());
}

#[cfg(not(target_os = "macos"))]
pub(super) fn sync_window_appearance(_win: &Window) {}

/// right-trimmed tail for display
pub(super) fn tail(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let keep: String = s.chars().rev().take(n).collect::<String>();
        format!("…{}", keep.chars().rev().collect::<String>())
    }
}

/// left-preserved prefix with ellipsis for filenames
pub(super) fn truncate_head(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let keep: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{keep}…")
    }
}

/// "/Users/x/…" -> "~/…" for display
pub(super) fn short_path(s: &str) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let h = home.to_string_lossy();
        if s.starts_with(h.as_ref()) {
            return format!("~{}", &s[h.len()..]);
        }
    }
    s.to_string()
}

/// "1.4 MB" from a byte count.
pub(super) fn age_label(p: &PathBuf) -> String {
    let Ok(m) = std::fs::metadata(p).and_then(|m| m.modified()) else {
        return String::new();
    };
    let Ok(d) = m.elapsed() else {
        return String::new();
    };
    let s = d.as_secs();
    if s < 60 {
        "just now".into()
    } else if s < 3600 {
        format!("{}m ago", s / 60)
    } else if s < 86_400 {
        format!("{}h ago", s / 3600)
    } else if s < 7 * 86_400 {
        format!("{}d ago", s / 86_400)
    } else {
        format!("{}w ago", s / (7 * 86_400))
    }
}

/// winit `Key` -> global-hotkey key token.
pub(super) fn key_token(key: &Key) -> Option<String> {
    match key {
        Key::Character(c) => {
            let u = c.to_uppercase();
            (u.chars().count() == 1).then_some(u)
        }
        Key::Named(n) => Some(match n {
            NamedKey::Space => "Space".into(),
            NamedKey::Enter => "Enter".into(),
            NamedKey::Tab => "Tab".into(),
            NamedKey::Backspace => "Backspace".into(),
            NamedKey::Delete => "Delete".into(),
            NamedKey::Insert => "Insert".into(),
            NamedKey::Home => "Home".into(),
            NamedKey::End => "End".into(),
            NamedKey::PageUp => "PageUp".into(),
            NamedKey::PageDown => "PageDown".into(),
            NamedKey::ArrowUp => "Up".into(),
            NamedKey::ArrowDown => "Down".into(),
            NamedKey::ArrowLeft => "Left".into(),
            NamedKey::ArrowRight => "Right".into(),
            NamedKey::PrintScreen => "PrintScreen".into(),
            other => {
                let d = format!("{other:?}");
                if d.starts_with('F') && d[1..].chars().all(|c| c.is_ascii_digit()) {
                    d
                } else {
                    return None;
                }
            }
        }),
        _ => None,
    }
}

/// First usable system font (fontdue reads TTF/OTF) — bundled Be Vietnam Pro first,
/// then fallback to platform system fonts. Shared with the toast and quick access.
pub(crate) fn system_font() -> Result<Font> {
    const BUNDLED: &[u8] = include_bytes!("../../../qs-render/fonts/BeVietnamPro-Regular.ttf");
    if let Ok(f) = Font::from_bytes(BUNDLED, FontSettings::default()) {
        return Ok(f);
    }
    const CANDIDATES: &[&str] = &[
        #[cfg(target_os = "macos")]
        "/System/Library/Fonts/SFNS.ttf",
        #[cfg(target_os = "macos")]
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        #[cfg(target_os = "macos")]
        "/System/Library/Fonts/Supplemental/Helvetica.ttf",
        #[cfg(target_os = "windows")]
        "C:\\Windows\\Fonts\\segoeui.ttf",
        #[cfg(target_os = "windows")]
        "C:\\Windows\\Fonts\\arial.ttf",
        #[cfg(target_os = "linux")]
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        #[cfg(target_os = "linux")]
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ];
    for p in CANDIDATES {
        if let Ok(bytes) = std::fs::read(p) {
            if let Ok(f) = Font::from_bytes(bytes.as_slice(), FontSettings::default()) {
                return Ok(f);
            }
        }
    }
    anyhow::bail!("no usable system font found");
}
