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

/// Dark — neutral greys after the macOS 26 system palette.
pub const DARK: Palette = Palette {
    bg_top: 0x171717,
    bg_bottom: 0x121212,
    sidebar: 0x161616,
    surface: 0x1F1F1F,
    surface_hover: 0x292929,
    popover: 0x1E1E1E,
    fringe: 0x0A0A0A,
    control: 0x282828,
    control_hot: 0x353535,
    tile: 0x272727,
    tile_glyph: 0xAAAAAA,
    keycap: 0x2D2D2D,
    keycap_edge: 0x161616,
    switch_off: 0x383838,
    border: 0x2E2E2E,
    border_strong: 0x434343,
    popover_border: 0x373737,
    fg: 0xF6F6F6,
    fg_dim: 0xBABABA,
    fg_faint: 0x949494,
    on_accent: 0xFFFFFF,
    success: 0x30D158,
    danger: 0xFF453A,
    warning: 0xFF9F0A,
    dark: true,
    ws_top: 0x131313,
    ws_bottom: 0x0F0F0F,
    bar_top: 0x1E1E1E,
    bar_bottom: 0x1A1A1A,
    panel_top: 0x181818,
    panel_bottom: 0x151515,
    track: 0x141414,
    pill_top: 0x323232,
    pill_bottom: 0x292929,
    tool_active_top: 0x1A8CFF,
    tool_active_bot: 0x006AE6,
    card_bg: 0x202020,
    status_bg: 0x161616,
    btn_top: 0x2A2A2A,
    btn_bottom: 0x242424,
    btn_hot_top: 0x363636,
    btn_hot_bottom: 0x2E2E2E,
    hairline: 0x2A2A2A,
    border_soft: 0x323232,
    border_hot: 0x484848,
    danger_top: 0x5A1C1F,
    danger_bottom: 0x461518,
    danger_edge: 0xC83A35,
    wash: 0xFFFFFF,
    hud: 0x202020,
    hud_hover: 0x383838,
    on_hud: 0xFFFFFF,
    shadow: 0x000000,
};

/// Light Glass — the default: airy Apple Liquid Glass / Sonoma light surfaces.
pub const LIGHT: Palette = Palette {
    bg_top: 0xF8F8F8,
    bg_bottom: 0xF0F0F0,
    sidebar: 0xF3F3F3,
    surface: 0xFFFFFF,
    surface_hover: 0xF5F5F5,
    popover: 0xFFFFFF,
    fringe: 0xA2A2A2,
    control: 0xF1F1F1,
    control_hot: 0xE6E6E6,
    tile: 0xF1F1F1,
    tile_glyph: 0x5E5E5E,
    keycap: 0xFFFFFF,
    keycap_edge: 0xD1D1D1,
    switch_off: 0xD2D2D2,
    border: 0xE2E2E2,
    border_strong: 0xC2C2C2,
    popover_border: 0xDCDCDC,
    fg: 0x181818,
    fg_dim: 0x4F4F4F,
    fg_faint: 0x6D6D6D,
    on_accent: 0xFFFFFF,
    success: 0x1E7E38,
    danger: 0xD70015,
    warning: 0xB25000,
    dark: false,
    ws_top: 0xEEEEEE,
    ws_bottom: 0xE8E8E8,
    bar_top: 0xFFFFFF,
    bar_bottom: 0xF8F8F8,
    panel_top: 0xF8F8F8,
    panel_bottom: 0xF4F4F4,
    track: 0xE9E9E9,
    pill_top: 0xFFFFFF,
    pill_bottom: 0xF8F8F8,
    tool_active_top: 0x1A8CFF,
    tool_active_bot: 0x006CE6,
    card_bg: 0xFFFFFF,
    status_bg: 0xF6F6F6,
    btn_top: 0xFFFFFF,
    btn_bottom: 0xF6F6F6,
    btn_hot_top: 0xFAFAFA,
    btn_hot_bottom: 0xEEEEEE,
    hairline: 0xE0E0E0,
    border_soft: 0xE3E3E3,
    border_hot: 0xC5C5C5,
    danger_top: 0xFFE8E6,
    danger_bottom: 0xFFD8D4,
    danger_edge: 0xE05A50,
    wash: 0x000000,
    hud: 0x202020,
    hud_hover: 0x383838,
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
    let rgb = CURRENT_ACCENT.load(Ordering::Relaxed);
    if pal().dark {
        dark_variant(rgb)
    } else {
        rgb
    }
}

/// Apple's dark-appearance variant of a system colour (systemBlue
/// `#007AFF` → `#0A84FF` …); custom colours pass through unchanged.
pub fn dark_variant(rgb: u32) -> u32 {
    match rgb {
        0x007AFF => 0x0A84FF,
        0xAF52DE => 0xBF5AF2,
        0xFF2D55 => 0xFF375F,
        0xFF3B30 => 0xFF453A,
        0xFF9500 => 0xFF9F0A,
        0xFFCC00 => 0xFFD60A,
        0x34C759 => 0x30D158,
        0x8E8E93 => 0x98989D,
        other => other,
    }
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

/// Accent presets shared by Settings in `qs-app` and `qs-editor` — (key, rgb).
pub const ACCENTS: &[(&str, u32)] = &[
    ("blue", 0x007AFF),
    ("purple", 0xAF52DE),
    ("pink", 0xFF2D55),
    ("orange", 0xFF9500),
    ("green", 0x34C759),
    ("gray", 0x8E8E93),
];

/// Look up an accent preset RGB by config key (falls back to `"blue"`).
pub fn accent_by_name(name: &str) -> u32 {
    ACCENTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, rgb)| *rgb)
        .unwrap_or(ACCENTS[0].1)
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

/// Corner radii, logical px — concentric steps after macOS 26: a control
/// inside a card inside a panel each step up by the padding between them.
pub mod radius {
    pub const XS: f64 = 4.0;
    pub const SM: f64 = 8.0;
    pub const MD: f64 = 10.0;
    pub const LG: f64 = 12.0;
    pub const XL: f64 = 18.0;
    pub const PILL: f64 = 999.0;
}

/// Type scale, logical px — macOS HIG text styles: Subheadline 11,
/// Callout 12, Body 13, Title 3 15, Title 1 22. Nothing below 11.
pub mod font {
    pub const CAPTION: f64 = 11.0;
    pub const SMALL: f64 = 12.0;
    pub const BODY: f64 = 13.0;
    pub const TITLE: f64 = 15.0;
    pub const HEADING: f64 = 22.0;
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
