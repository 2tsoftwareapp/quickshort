//! Foundations layer of the Quickshort design system.
//!
//! Three layers, one direction of dependency (after Astryx):
//!
//! 1. **Foundations** — this crate. Named tokens: colour [`Palette`]s per
//!    theme, plus theme-independent [`space`], [`radius`], [`font`] and
//!    [`motion`] scales. No toolkit types, no logic beyond contrast maths.
//! 2. **Components** — shared widgets (card, row, switch, dropdown …) that
//!    read tokens and never contain a literal colour or size.
//! 3. **Patterns** — screens (settings, toast, shelf, editor chrome) that
//!    compose components.
//!
//! A theme is just another [`Palette`] value: adding one means filling every
//! field (the compiler enforces it) — no component changes. Colours are
//! `0xRRGGBB`; user-chosen annotation/accent colours are data, not tokens.
//!
//! Rules (see `docs/ui/CONVENTIONS.md`): no `0xRRGGBB` literal outside this
//! crate; no magic spacing in components — pick the nearest scale step.

use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};

/// Colour tokens, named by role (not by hue) so a theme can remap them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    // — page —
    /// window background, top of the vertical gradient
    pub bg_top: u32,
    /// window background, bottom of the gradient
    pub bg_bottom: u32,
    /// sidebar / secondary pane
    pub sidebar: u32,
    // — surfaces —
    /// card / grouped-list surface
    pub surface: u32,
    /// row under the pointer
    pub surface_hover: u32,
    /// floating card (toast, capture card, REC pill)
    pub popover: u32,
    /// shadow-fringe tint under floating cards
    pub fringe: u32,
    /// button on a floating card
    pub control: u32,
    /// the same button under the pointer
    pub control_hot: u32,
    /// icon tile behind a row glyph
    pub tile: u32,
    /// glyph drawn on a tile
    pub tile_glyph: u32,
    /// raised keycap top
    pub keycap: u32,
    /// keycap bottom edge
    pub keycap_edge: u32,
    /// switch track when off
    pub switch_off: u32,
    // — lines —
    /// 1px hairline between and around surfaces
    pub border: u32,
    /// emphasised outline (thumbnails, focused fields)
    pub border_strong: u32,
    /// hairline of a floating card
    pub popover_border: u32,
    // — text —
    pub fg: u32,
    pub fg_dim: u32,
    pub fg_faint: u32,
    /// text/glyph drawn on the accent colour
    pub on_accent: u32,
    // — status —
    pub success: u32,
    pub danger: u32,
    pub warning: u32,
    // — editor chrome (vertical gradients run top → bottom) —
    /// true for themes with light text on dark surfaces
    pub dark: bool,
    /// editor workspace behind the document
    pub ws_top: u32,
    pub ws_bottom: u32,
    /// editor toolbar
    pub bar_top: u32,
    pub bar_bottom: u32,
    /// editor inspector strip
    pub panel_top: u32,
    pub panel_bottom: u32,
    /// segmented-control track / inset field
    pub track: u32,
    /// selected segment / pill
    pub pill_top: u32,
    pub pill_bottom: u32,
    /// active tool segment gradient in editor toolbar
    pub tool_active_top: u32,
    pub tool_active_bot: u32,
    /// inspector card & bottom status bar surfaces
    pub card_bg: u32,
    pub status_bg: u32,
    /// secondary button, rest and hover
    pub btn_top: u32,
    pub btn_bottom: u32,
    pub btn_hot_top: u32,
    pub btn_hot_bottom: u32,
    /// darkest line under chrome bars
    pub hairline: u32,
    /// quiet outline of controls; `border_hot` when hovered
    pub border_soft: u32,
    pub border_hot: u32,
    /// destructive button while hovered (gradient + edge)
    pub danger_top: u32,
    pub danger_bottom: u32,
    pub danger_edge: u32,
    /// colour of translucent hover/divider washes (white on dark, black on light)
    pub wash: u32,
    // — capture HUD: always dark, it floats over arbitrary screenshots —
    pub hud: u32,
    pub hud_hover: u32,
    pub on_hud: u32,
    /// drop-shadow ink
    pub shadow: u32,
}

/// Dark Glass — refined obsidian-zinc dark mode matching macOS Liquid Glass.
pub const DARK: Palette = Palette {
    bg_top: 0x16171C,
    bg_bottom: 0x111216,
    sidebar: 0x15161B,
    surface: 0x1D1F26,
    surface_hover: 0x262933,
    popover: 0x1C1E25,
    fringe: 0x090A0D,
    control: 0x252832,
    control_hot: 0x313543,
    tile: 0x242731,
    tile_glyph: 0xA6AAB8,
    keycap: 0x2A2D38,
    keycap_edge: 0x14161C,
    switch_off: 0x343846,
    border: 0x2B2E3A,
    border_strong: 0x3E4354,
    popover_border: 0x333746,
    fg: 0xF5F6FA,
    fg_dim: 0xB6BAC8,
    fg_faint: 0x8F94A6,
    on_accent: 0xFFFFFF,
    success: 0x30D158,
    danger: 0xFF453A,
    warning: 0xFF9F0A,
    dark: true,
    ws_top: 0x121318,
    ws_bottom: 0x0E0F13,
    bar_top: 0x1C1E24,
    bar_bottom: 0x181A20,
    panel_top: 0x17181E,
    panel_bottom: 0x14151A,
    track: 0x131419,
    pill_top: 0x2E323E,
    pill_bottom: 0x262934,
    tool_active_top: 0x1A8CFF,
    tool_active_bot: 0x006AE6,
    card_bg: 0x1E2028,
    status_bg: 0x15161B,
    btn_top: 0x272A34,
    btn_bottom: 0x21242D,
    btn_hot_top: 0x323644,
    btn_hot_bottom: 0x2B2E3B,
    hairline: 0x272A34,
    border_soft: 0x2E323F,
    border_hot: 0x43485A,
    danger_top: 0x5A1C1F,
    danger_bottom: 0x461518,
    danger_edge: 0xC83A35,
    wash: 0xFFFFFF,
    hud: 0x1E2028,
    hud_hover: 0x343846,
    on_hud: 0xFFFFFF,
    shadow: 0x000000,
};

/// Light Glass — the default: airy Apple Liquid Glass / Sonoma light surfaces.
pub const LIGHT: Palette = Palette {
    bg_top: 0xF7F8FB,
    bg_bottom: 0xEEF0F5,
    sidebar: 0xF2F3F7,
    surface: 0xFFFFFF,
    surface_hover: 0xF3F5F9,
    popover: 0xFFFFFF,
    fringe: 0x9CA2B0,
    control: 0xEFF1F6,
    control_hot: 0xE3E6EE,
    tile: 0xEFF1F6,
    tile_glyph: 0x595E6D,
    keycap: 0xFFFFFF,
    keycap_edge: 0xCDD1DC,
    switch_off: 0xCED2DC,
    border: 0xDFE2EA,
    border_strong: 0xBDC2CF,
    popover_border: 0xD8DCE6,
    fg: 0x16181D,
    fg_dim: 0x4A4F5C,
    fg_faint: 0x676D7E,
    on_accent: 0xFFFFFF,
    success: 0x1E7E38,
    danger: 0xD70015,
    warning: 0xB25000,
    dark: false,
    ws_top: 0xECEEF3,
    ws_bottom: 0xE5E8EF,
    bar_top: 0xFFFFFF,
    bar_bottom: 0xF7F8FB,
    panel_top: 0xF7F8FB,
    panel_bottom: 0xF2F4F8,
    track: 0xE6E9F0,
    pill_top: 0xFFFFFF,
    pill_bottom: 0xF7F8FB,
    tool_active_top: 0x1A8CFF,
    tool_active_bot: 0x006CE6,
    card_bg: 0xFFFFFF,
    status_bg: 0xF5F6FA,
    btn_top: 0xFFFFFF,
    btn_bottom: 0xF5F6FA,
    btn_hot_top: 0xF9FAFD,
    btn_hot_bottom: 0xECEEF4,
    hairline: 0xDCE0E9,
    border_soft: 0xDFE3EC,
    border_hot: 0xBFC5D4,
    danger_top: 0xFFE8E6,
    danger_bottom: 0xFFD8D4,
    danger_edge: 0xE05A50,
    wash: 0x000000,
    hud: 0x1E2028,
    hud_hover: 0x343846,
    on_hud: 0xFFFFFF,
    shadow: 0x000000,
};

/// Default accent (Apple Liquid Blue) and its gradient stops. The user's choice in
/// Settings overrides this at runtime; these are the fallback.
pub const ACCENT_DEFAULT: u32 = 0x007AFF;
pub const ACCENT_TOP: u32 = 0x1A8CFF;
pub const ACCENT_BOTTOM: u32 = 0x0066E0;
pub const ACCENT_SOFT: u32 = 0x4DA6FF;
/// Gradient stops of an accent button under the pointer.
pub const ACCENT_TOP_HOT: u32 = 0x3399FF;
pub const ACCENT_BOTTOM_HOT: u32 = 0x0A78F0;

static CURRENT_ACCENT: AtomicU32 = AtomicU32::new(ACCENT_DEFAULT);

fn shift_rgb(rgb: u32, delta: i16) -> u32 {
    let r = (((rgb >> 16) & 0xFF) as i16 + delta).clamp(0, 255) as u32;
    let g = (((rgb >> 8) & 0xFF) as i16 + delta).clamp(0, 255) as u32;
    let b = ((rgb & 0xFF) as i16 + delta).clamp(0, 255) as u32;
    (r << 16) | (g << 8) | b
}

/// Active runtime accent colour (`0xRRGGBB`), shared by `qs-app` and `qs-editor`.
pub fn accent() -> u32 {
    CURRENT_ACCENT.load(Ordering::Relaxed)
}

pub fn accent_top() -> u32 {
    shift_rgb(accent(), 20)
}

pub fn accent_bottom() -> u32 {
    shift_rgb(accent(), -20)
}

pub fn accent_soft() -> u32 {
    shift_rgb(accent(), 52)
}

/// Set the process-wide accent colour (`0xRRGGBB`).
pub fn set_accent(rgb: u32) {
    CURRENT_ACCENT.store(rgb & 0x00FF_FFFF, Ordering::Relaxed);
}

/// Curated modern Apple HIG / Sonoma annotation swatches — harmonious, crisp
/// on both Light Glass and Dark Glass: coral red, amber, gold, emerald,
/// liquid blue, iris purple, pure white, graphite ink.
pub const ANNOTATION: [u32; 8] = [
    0xFF3B30, 0xFF9500, 0xFFCC00, 0x34C759, 0x007AFF, 0xAF52DE, 0xFFFFFF, 0x1C1E24,
];

/// Selectable themes — (config key, label). Light-first default; unknown keys
/// fall back to the first entry (`light`).
pub const THEMES: &[(&str, &str)] = &[("light", "Light"), ("dark", "Dark")];

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// The active palette. Cheap; call it per draw rather than caching, so a
/// theme switch shows up on the next frame.
pub fn pal() -> &'static Palette {
    match CURRENT.load(Ordering::Relaxed) {
        1 => &DARK,
        _ => &LIGHT,
    }
}

/// Current active theme key (`"light"` or `"dark"`).
pub fn theme_key() -> &'static str {
    THEMES
        .get(CURRENT.load(Ordering::Relaxed) as usize)
        .map(|(k, _)| *k)
        .unwrap_or("light")
}

/// Switch theme by config key.
pub fn set_theme(key: &str) {
    let i = THEMES.iter().position(|(k, _)| *k == key).unwrap_or(0);
    CURRENT.store(i as u8, Ordering::Relaxed);
}

/// Spacing scale, logical px — multiples of 4 (plus the 2px hairline step).
pub mod space {
    pub const HAIR: f64 = 2.0;
    pub const XS: f64 = 4.0;
    pub const SM: f64 = 8.0;
    pub const MD: f64 = 12.0;
    pub const LG: f64 = 16.0;
    pub const XL: f64 = 24.0;
    pub const XXL: f64 = 32.0;
}

/// Corner radii, logical px.
pub mod radius {
    pub const XS: f64 = 4.0;
    pub const SM: f64 = 6.0;
    pub const MD: f64 = 8.0;
    pub const LG: f64 = 10.0;
    pub const XL: f64 = 14.0;
    pub const PILL: f64 = 999.0;
}

/// Type scale, logical px.
pub mod font {
    pub const CAPTION: f64 = 10.5;
    pub const SMALL: f64 = 11.5;
    pub const BODY: f64 = 13.0;
    pub const TITLE: f64 = 13.5;
    pub const HEADING: f64 = 24.0;
}

/// Motion durations, milliseconds.
pub mod motion {
    pub const FAST: f64 = 120.0;
    pub const BASE: f64 = 200.0;
    pub const SLOW: f64 = 280.0;
}

/// WCAG 2.x relative luminance of `0xRRGGBB`.
pub fn luminance(rgb: u32) -> f64 {
    let ch = |s: u32| {
        let c = ((rgb >> s) & 0xFF) as f64 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch(16) + 0.7152 * ch(8) + 0.0722 * ch(0)
}

/// WCAG contrast ratio between two colours (1.0 – 21.0).
pub fn contrast(a: u32, b: u32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [(&str, Palette); 2] = [("dark", DARK), ("light", LIGHT)];

    #[test]
    fn body_text_meets_aa_on_every_surface() {
        for (name, p) in ALL {
            for (surf_name, surf) in [
                ("bg_top", p.bg_top),
                ("bg_bottom", p.bg_bottom),
                ("sidebar", p.sidebar),
                ("surface", p.surface),
                ("popover", p.popover),
            ] {
                for (fg_name, fg) in [("fg", p.fg), ("fg_dim", p.fg_dim)] {
                    let c = contrast(fg, surf);
                    assert!(c >= 4.5, "{name}: {fg_name} on {surf_name} = {c:.2}");
                }
            }
        }
    }

    #[test]
    fn faint_text_meets_large_text_aa() {
        for (name, p) in ALL {
            for (surf_name, surf) in [("surface", p.surface), ("bg_bottom", p.bg_bottom)] {
                let c = contrast(p.fg_faint, surf);
                assert!(c >= 3.0, "{name}: fg_faint on {surf_name} = {c:.2}");
            }
        }
    }

    #[test]
    fn status_colours_are_legible_on_surface() {
        for (name, p) in ALL {
            for (n, c) in [("success", p.success), ("danger", p.danger)] {
                let r = contrast(c, p.surface);
                assert!(r >= 3.0, "{name}: {n} on surface = {r:.2}");
            }
        }
    }

    #[test]
    fn editor_chrome_text_is_legible() {
        for (name, p) in ALL {
            for (n, s) in [
                ("ws_top", p.ws_top),
                ("ws_bottom", p.ws_bottom),
                ("bar_top", p.bar_top),
                ("bar_bottom", p.bar_bottom),
                ("panel_top", p.panel_top),
                ("panel_bottom", p.panel_bottom),
                ("track", p.track),
                ("pill_bottom", p.pill_bottom),
                ("btn_top", p.btn_top),
                ("btn_bottom", p.btn_bottom),
                ("btn_hot_bottom", p.btn_hot_bottom),
            ] {
                let c = contrast(p.fg, s);
                assert!(c >= 4.5, "{name}: fg on {n} = {c:.2}");
                let c = contrast(p.fg_faint, s);
                assert!(c >= 3.0, "{name}: fg_faint on {n} = {c:.2}");
            }
            for (n, s) in [("danger_top", p.danger_top), ("danger_bottom", p.danger_bottom)] {
                let c = contrast(p.fg, s);
                assert!(c >= 4.5, "{name}: fg on {n} = {c:.2}");
            }
        }
    }

    #[test]
    fn hud_is_dark_in_every_theme() {
        for (name, p) in ALL {
            assert!(contrast(p.on_hud, p.hud) >= 7.0, "{name}");
            assert!(luminance(p.hud) < 0.1, "{name}: hud must stay dark");
        }
    }

    #[test]
    fn dark_flag_matches_luminance() {
        for (name, p) in ALL {
            assert_eq!(p.dark, luminance(p.bg_top) < 0.5, "{name}");
        }
    }

    #[test]
    fn set_theme_switches_and_falls_back() {
        set_theme("dark");
        assert_eq!(*pal(), DARK);
        set_theme("nonsense");
        assert_eq!(*pal(), LIGHT);
    }

    #[test]
    fn set_accent_updates_runtime_accent() {
        set_accent(0xAF52DE);
        assert_eq!(accent(), 0xAF52DE);
        set_accent(ACCENT_DEFAULT);
        assert_eq!(accent(), ACCENT_DEFAULT);
    }

    #[test]
    fn spacing_scale_is_monotonic() {
        let s = [space::XS, space::SM, space::MD, space::LG, space::XL, space::XXL];
        assert!(s.windows(2).all(|w| w[0] < w[1]));
    }
}
