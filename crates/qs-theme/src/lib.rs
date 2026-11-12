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

use std::sync::atomic::{AtomicU8, Ordering};

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

/// Dark Studio — the default.
pub const DARK: Palette = Palette {
    bg_top: 0x1A1A1F,
    bg_bottom: 0x141418,
    sidebar: 0x18181D,
    surface: 0x22222A,
    surface_hover: 0x2C2C37,
    popover: 0x1E1E24,
    fringe: 0x0C0C10,
    control: 0x2A2A34,
    control_hot: 0x3A3A48,
    tile: 0x282832,
    tile_glyph: 0x9EA0AC,
    keycap: 0x2E2E39,
    keycap_edge: 0x181820,
    switch_off: 0x383844,
    border: 0x2F2F3B,
    border_strong: 0x424252,
    popover_border: 0x363644,
    fg: 0xF4F4F7,
    fg_dim: 0xB4B7C4,
    fg_faint: 0x8E91A0,
    on_accent: 0xFFFFFF,
    success: 0x30D158,
    danger: 0xFF453A,
    warning: 0xFF9F0A,
    dark: true,
    ws_top: 0x26262B,
    ws_bottom: 0x1E1E23,
    bar_top: 0x323237,
    bar_bottom: 0x26262B,
    panel_top: 0x242428,
    panel_bottom: 0x1D1D21,
    track: 0x18181C,
    pill_top: 0x4C4C52,
    pill_bottom: 0x3C3C42,
    btn_top: 0x343439,
    btn_bottom: 0x2A2A2E,
    btn_hot_top: 0x404046,
    btn_hot_bottom: 0x323238,
    hairline: 0x141418,
    border_soft: 0x38383E,
    border_hot: 0x46464C,
    danger_top: 0x5A1818,
    danger_bottom: 0x461212,
    danger_edge: 0xB43228,
    wash: 0xFFFFFF,
    hud: 0x2C2C2E,
    hud_hover: 0x48484D,
    on_hud: 0xFFFFFF,
    shadow: 0x000000,
};

/// Light — same roles, macOS-style light surfaces.
pub const LIGHT: Palette = Palette {
    bg_top: 0xF5F5F7,
    bg_bottom: 0xECECEF,
    sidebar: 0xE6E6EB,
    surface: 0xFFFFFF,
    surface_hover: 0xF0F0F5,
    popover: 0xFFFFFF,
    fringe: 0x9A9AA6,
    control: 0xEDEDF2,
    control_hot: 0xDDDDE6,
    tile: 0xEDEDF2,
    tile_glyph: 0x5F6270,
    keycap: 0xFFFFFF,
    keycap_edge: 0xC4C4CE,
    switch_off: 0xC8C8D0,
    border: 0xDADAE1,
    border_strong: 0xB4B4C0,
    popover_border: 0xD0D0D8,
    fg: 0x1C1C1F,
    fg_dim: 0x464955,
    fg_faint: 0x6E7180,
    on_accent: 0xFFFFFF,
    success: 0x1F7A36,
    danger: 0xD70015,
    warning: 0xB25000,
    dark: false,
    ws_top: 0xE9E9EE,
    ws_bottom: 0xDFDFE5,
    bar_top: 0xFAFAFC,
    bar_bottom: 0xEDEDF1,
    panel_top: 0xF3F3F6,
    panel_bottom: 0xE8E8ED,
    track: 0xDCDCE3,
    pill_top: 0xFFFFFF,
    pill_bottom: 0xF1F1F5,
    btn_top: 0xFFFFFF,
    btn_bottom: 0xF0F0F4,
    btn_hot_top: 0xF6F6F9,
    btn_hot_bottom: 0xE4E4EA,
    hairline: 0xC8C8D0,
    border_soft: 0xD2D2DA,
    border_hot: 0xB4B4C0,
    danger_top: 0xFFE3E0,
    danger_bottom: 0xFFD0CC,
    danger_edge: 0xE5675D,
    wash: 0x000000,
    hud: 0x2C2C2E,
    hud_hover: 0x48484D,
    on_hud: 0xFFFFFF,
    shadow: 0x000000,
};

/// Default accent (macOS blue) and its gradient stops. The user's choice in
/// Settings overrides this at runtime; these are the fallback.
pub const ACCENT_DEFAULT: u32 = 0x0A84FF;
pub const ACCENT_TOP: u32 = 0x2092FF;
pub const ACCENT_BOTTOM: u32 = 0x0070E0;
pub const ACCENT_SOFT: u32 = 0x4DA6FF;
/// Gradient stops of an accent button under the pointer.
pub const ACCENT_TOP_HOT: u32 = 0x2C9CFF;
pub const ACCENT_BOTTOM_HOT: u32 = 0x0A7AEB;

/// Colours offered by the annotation tools — user data, identical in every
/// theme: red, orange, yellow, green, blue, purple, white, black.
pub const ANNOTATION: [u32; 8] = [
    0xFF453A, 0xFF9F0A, 0xFFD60A, 0x32D74B, 0x0A84FF, 0xBF5AF2, 0xFFFFFF, 0x1C1C1E,
];

/// Selectable themes — (config key, label). Unknown keys fall back to the
/// first entry.
pub const THEMES: &[(&str, &str)] = &[("dark", "Dark"), ("light", "Light")];

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// The active palette. Cheap; call it per draw rather than caching, so a
/// theme switch shows up on the next frame.
pub fn pal() -> &'static Palette {
    match CURRENT.load(Ordering::Relaxed) {
        1 => &LIGHT,
        _ => &DARK,
    }
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
        set_theme("light");
        assert_eq!(*pal(), LIGHT);
        set_theme("nonsense");
        assert_eq!(*pal(), DARK);
    }

    #[test]
    fn spacing_scale_is_monotonic() {
        let s = [space::XS, space::SM, space::MD, space::LG, space::XL, space::XXL];
        assert!(s.windows(2).all(|w| w[0] < w[1]));
    }
}
