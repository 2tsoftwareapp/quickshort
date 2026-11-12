//! macOS Sonoma/Sequoia dark mode design tokens, palettes, and vector drawing helpers.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Stroke, StrokeKind, Vec2};
use eframe::epaint::{Mesh, Vertex, WHITE_UV};

// ---------------------------------------------------------------------------
// Design Tokens
// ---------------------------------------------------------------------------

pub const TOOLBAR_HEIGHT: f32 = 46.0;
pub const INSPECTOR_HEIGHT: f32 = 38.0;
pub const CHROME_PAD_X: f32 = 14.0;

pub const RADIUS_WINDOW: CornerRadius = CornerRadius::same(10);
pub const RADIUS_TRACK: CornerRadius = CornerRadius::same(8);
pub const RADIUS_PILL: CornerRadius = CornerRadius::same(6);
pub const RADIUS_BTN: CornerRadius = CornerRadius::same(6);

// Colour tokens are functions, not consts: they read `qs_theme::pal()` at
// draw time so a theme switch applies on the next frame. Hex lives only in
// the `qs-theme` crate.

fn c(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn c_a(rgb: u32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, a)
}

/// Pick the theme from `QS_THEME` (set by the host that spawned us) so the
/// editor opens in the colours the user chose in Settings.
pub fn init_theme_from_env() {
    if let Ok(t) = std::env::var("QS_THEME") {
        qs_theme::set_theme(&t);
    }
}

// Backgrounds
pub fn bg_workspace_top() -> Color32 { c(qs_theme::pal().ws_top) }
pub fn bg_workspace_bot() -> Color32 { c(qs_theme::pal().ws_bottom) }
pub fn bg_toolbar_top() -> Color32 { c(qs_theme::pal().bar_top) }
pub fn bg_toolbar_bot() -> Color32 { c(qs_theme::pal().bar_bottom) }
pub fn bg_inspector_top() -> Color32 { c(qs_theme::pal().panel_top) }
pub fn bg_inspector_bot() -> Color32 { c(qs_theme::pal().panel_bottom) }
pub fn bg_pill_track() -> Color32 { c(qs_theme::pal().track) }
pub fn bg_pill_hover() -> Color32 { c_a(qs_theme::pal().wash, 28) }
pub fn bg_pill_active_top() -> Color32 { c(qs_theme::pal().pill_top) }
pub fn bg_pill_active_bot() -> Color32 { c(qs_theme::pal().pill_bottom) }
pub fn bg_btn_secondary_top() -> Color32 { c(qs_theme::pal().btn_top) }
pub fn bg_btn_secondary_bot() -> Color32 { c(qs_theme::pal().btn_bottom) }
pub fn bg_btn_secondary_hover_top() -> Color32 { c(qs_theme::pal().btn_hot_top) }
pub fn bg_btn_secondary_hover_bot() -> Color32 { c(qs_theme::pal().btn_hot_bottom) }

// Borders & Dividers
pub fn border_dark_hairline() -> Color32 { c(qs_theme::pal().hairline) }
pub fn border_subtle() -> Color32 { c(qs_theme::pal().border_soft) }
pub fn border_hot() -> Color32 { c(qs_theme::pal().border_hot) }
/// Light catch along the top edge of a surface — white reads on both themes.
pub fn highlight_top() -> Color32 { Color32::from_white_alpha(22) }
pub fn divider_vertical() -> Color32 { c_a(qs_theme::pal().wash, 28) }

// Accents & Semantics (accent defaults; theme-independent)
pub fn accent_blue() -> Color32 { c(qs_theme::ACCENT_DEFAULT) }
pub fn accent_blue_top() -> Color32 { c(qs_theme::ACCENT_TOP) }
pub fn accent_blue_bot() -> Color32 { c(qs_theme::ACCENT_BOTTOM) }
pub fn accent_blue_soft() -> Color32 { c(qs_theme::ACCENT_SOFT) }
pub fn accent_alpha(a: u8) -> Color32 { c_a(qs_theme::ACCENT_DEFAULT, a) }
pub fn success_green() -> Color32 { c(qs_theme::pal().success) }
pub fn danger_red() -> Color32 { c(qs_theme::pal().danger) }
pub fn amber_orange() -> Color32 { c(qs_theme::pal().warning) }
pub fn amber_alpha(a: u8) -> Color32 { c_a(qs_theme::pal().warning, a) }

// Typography
pub fn text_primary() -> Color32 { c(qs_theme::pal().fg) }
pub fn text_secondary() -> Color32 { c(qs_theme::pal().fg_dim) }
pub fn text_tertiary() -> Color32 { c(qs_theme::pal().fg_faint) }
pub fn text_disabled() -> Color32 { c_a(qs_theme::pal().fg_faint, 110) }

/// Ink for glyphs drawn on the success colour (check mark in its disc).
pub fn on_success() -> Color32 {
    if qs_theme::pal().dark { Color32::from_rgb(16, 20, 24) } else { Color32::WHITE }
}

/// Selected tool segment: a deep accent tint in dark mode, the full accent
/// gradient in light mode. The icon on it is white in both.
pub fn tool_active_gradient() -> (Color32, Color32) {
    if qs_theme::pal().dark {
        (Color32::from_rgb(34, 76, 128), Color32::from_rgb(22, 54, 96))
    } else {
        (accent_blue_top(), accent_blue_bot())
    }
}

// ---------------------------------------------------------------------------
// Global Egui Visuals
// ---------------------------------------------------------------------------

pub fn apply_macos_visuals(ctx: &egui::Context) {
    init_theme_from_env();
    let dark = qs_theme::pal().dark;
    let theme = if dark { egui::Theme::Dark } else { egui::Theme::Light };
    ctx.set_theme(theme);
    ctx.style_mut_of(theme, |s| {
        s.visuals.dark_mode = dark;
        s.visuals.panel_fill = bg_toolbar_bot();
        s.visuals.window_fill = bg_toolbar_bot();
        s.visuals.window_stroke = Stroke::new(1.0, border_dark_hairline());
        s.visuals.window_corner_radius = RADIUS_WINDOW;

        s.visuals.widgets.noninteractive.bg_fill = bg_toolbar_bot();
        s.visuals.widgets.noninteractive.weak_bg_fill = bg_toolbar_bot();
        s.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border_subtle());
        s.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text_primary());
        s.visuals.widgets.noninteractive.corner_radius = RADIUS_PILL;

        s.visuals.widgets.inactive.bg_fill = bg_pill_track();
        s.visuals.widgets.inactive.weak_bg_fill = bg_pill_track();
        s.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border_subtle());
        s.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, text_secondary());
        s.visuals.widgets.inactive.corner_radius = RADIUS_PILL;

        s.visuals.widgets.hovered.bg_fill = bg_btn_secondary_hover_bot();
        s.visuals.widgets.hovered.weak_bg_fill = bg_btn_secondary_hover_bot();
        s.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, border_hot());
        s.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, text_primary());
        s.visuals.widgets.hovered.corner_radius = RADIUS_PILL;

        s.visuals.widgets.active.bg_fill = bg_pill_active_bot();
        s.visuals.widgets.active.weak_bg_fill = bg_pill_active_bot();
        s.visuals.widgets.active.bg_stroke = Stroke::new(1.0, accent_blue());
        s.visuals.widgets.active.fg_stroke = Stroke::new(1.0, text_primary());
        s.visuals.widgets.active.corner_radius = RADIUS_PILL;

        s.visuals.selection.bg_fill = accent_blue();
        s.visuals.selection.stroke = Stroke::new(1.0, accent_blue());

        s.spacing.item_spacing = Vec2::new(6.0, 4.0);
        s.spacing.button_padding = Vec2::new(8.0, 4.0);
    });
}

// ---------------------------------------------------------------------------
// Mesh Gradient & Shape Drawing Helpers
// ---------------------------------------------------------------------------

fn add_mesh_vertex(mesh: &mut Mesh, pos: Pos2, color: Color32) -> u32 {
    let idx = mesh.vertices.len() as u32;
    mesh.vertices.push(Vertex {
        pos,
        uv: WHITE_UV,
        color,
    });
    idx
}

pub fn paint_rect_v_gradient(painter: &egui::Painter, rect: Rect, top: Color32, bot: Color32) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let mut mesh = Mesh::default();
    let tl = add_mesh_vertex(&mut mesh, rect.left_top(), top);
    let tr = add_mesh_vertex(&mut mesh, rect.right_top(), top);
    let br = add_mesh_vertex(&mut mesh, rect.right_bottom(), bot);
    let bl = add_mesh_vertex(&mut mesh, rect.left_bottom(), bot);

    mesh.add_triangle(tl, tr, br);
    mesh.add_triangle(tl, br, bl);
    painter.add(mesh);
}

pub fn paint_rounded_v_gradient(
    painter: &egui::Painter,
    rect: Rect,
    corner_radius: CornerRadius,
    top: Color32,
    bot: Color32,
) {
    let _mid_y = rect.center().y;
    let mid = Color32::from_rgb(
        ((top.r() as u16 + bot.r() as u16) / 2) as u8,
        ((top.g() as u16 + bot.g() as u16) / 2) as u8,
        ((top.b() as u16 + bot.b() as u16) / 2) as u8,
    );
    painter.rect_filled(rect, corner_radius, mid);

    let n_slices = 5;
    let h = rect.height() / n_slices as f32;
    for i in 0..n_slices {
        let t0 = i as f32 / n_slices as f32;
        let t1 = (i + 1) as f32 / n_slices as f32;
        let c0 = lerp_color(top, bot, t0);
        let c1 = lerp_color(top, bot, t1);
        let slice = Rect::from_min_max(
            Pos2::new(rect.left(), rect.top() + i as f32 * h),
            Pos2::new(rect.right(), rect.top() + (i + 1) as f32 * h),
        );
        let mut mesh = Mesh::default();
        let tl = add_mesh_vertex(&mut mesh, slice.left_top(), c0);
        let tr = add_mesh_vertex(&mut mesh, slice.right_top(), c0);
        let br = add_mesh_vertex(&mut mesh, slice.right_bottom(), c1);
        let bl = add_mesh_vertex(&mut mesh, slice.left_bottom(), c1);
        mesh.add_triangle(tl, tr, br);
        mesh.add_triangle(tl, br, bl);
        painter.with_clip_rect(rect).add(mesh);
    }
}

pub fn paint_soft_shadow(
    painter: &egui::Painter,
    rect: Rect,
    corner_radius: f32,
    blur_inner: f32,
    blur_outer: f32,
    max_alpha: u8,
) {
    let steps = 4;
    for s in 0..steps {
        let t = (s + 1) as f32 / steps as f32;
        let spread = blur_inner + (blur_outer - blur_inner) * t;
        let shadow_rect = rect
            .translate(Vec2::new(0.0, 1.5 + spread * 0.4))
            .expand(spread * 0.5);
        let alpha = ((max_alpha as f32) * (1.0 - t * 0.75) / (steps as f32)).round() as u8;
        painter.rect_filled(
            shadow_rect,
            CornerRadius::same((corner_radius + spread * 0.35).round() as u8),
            Color32::from_rgba_unmultiplied(0, 0, 0, alpha),
        );
    }
}

// ---------------------------------------------------------------------------
// Chrome & Control Painting Functions
// ---------------------------------------------------------------------------

pub fn paint_chrome_bar(
    painter: &egui::Painter,
    rect: Rect,
    top: Color32,
    bot: Color32,
    is_top_level: bool,
) {
    paint_rect_v_gradient(painter, rect, top, bot);

    if is_top_level {
        painter.line_segment(
            [rect.left_top(), rect.right_top()],
            Stroke::new(1.0, highlight_top()),
        );
    }

    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, border_dark_hairline()),
    );
}

pub fn paint_segmented_track(painter: &egui::Painter, rect: Rect, corner_radius: CornerRadius) {
    painter.rect_filled(rect, corner_radius, bg_pill_track());
    painter.rect_stroke(
        rect,
        corner_radius,
        Stroke::new(1.0, border_subtle()),
        StrokeKind::Inside,
    );
}

pub fn paint_pill_button(
    painter: &egui::Painter,
    rect: Rect,
    corner_radius: impl Into<CornerRadius>,
    active: bool,
    hovered: bool,
    enabled: bool,
    is_accent: bool,
) {
    let cr = corner_radius.into();
    if !enabled {
        return;
    }

    if is_accent {
        let top = if hovered {
            c(qs_theme::ACCENT_TOP_HOT)
        } else {
            accent_blue_top()
        };
        let bot = if hovered {
            c(qs_theme::ACCENT_BOTTOM_HOT)
        } else {
            accent_blue_bot()
        };
        paint_rounded_v_gradient(painter, rect, cr, top, bot);
        painter.rect_stroke(
            rect,
            cr,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 55)),
            StrokeKind::Inside,
        );
        return;
    }

    if active {
        paint_rounded_v_gradient(painter, rect, cr, bg_pill_active_top(), bg_pill_active_bot());
        painter.line_segment(
            [
                Pos2::new(rect.left() + 2.0, rect.top() + 0.5),
                Pos2::new(rect.right() - 2.0, rect.top() + 0.5),
            ],
            Stroke::new(1.0, highlight_top()),
        );
        painter.rect_stroke(
            rect,
            cr,
            Stroke::new(1.0, border_hot()),
            StrokeKind::Inside,
        );
    } else if hovered {
        painter.rect_filled(rect, cr, bg_pill_hover());
    }
}

pub fn paint_secondary_button(
    painter: &egui::Painter,
    rect: Rect,
    corner_radius: impl Into<CornerRadius>,
    hovered: bool,
    enabled: bool,
    danger_hover: bool,
) {
    let cr = corner_radius.into();
    if !enabled {
        return;
    }

    let (top, bot, border) = if hovered && danger_hover {
        (
            c(qs_theme::pal().danger_top),
            c(qs_theme::pal().danger_bottom),
            c(qs_theme::pal().danger_edge),
        )
    } else if hovered {
        (
            bg_btn_secondary_hover_top(),
            bg_btn_secondary_hover_bot(),
            border_hot(),
        )
    } else {
        (
            bg_btn_secondary_top(),
            bg_btn_secondary_bot(),
            border_subtle(),
        )
    };

    paint_rounded_v_gradient(painter, rect, cr, top, bot);
    painter.line_segment(
        [
            Pos2::new(rect.left() + 2.0, rect.top() + 0.5),
            Pos2::new(rect.right() - 2.0, rect.top() + 0.5),
        ],
        Stroke::new(1.0, highlight_top()),
    );
    painter.rect_stroke(rect, cr, Stroke::new(1.0, border), StrokeKind::Inside);
}

pub fn paint_color_swatch(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    color: Color32,
    selected: bool,
    hovered: bool,
) {
    painter.circle_filled(
        center + Vec2::new(0.0, 1.0),
        radius,
        Color32::from_rgba_unmultiplied(0, 0, 0, 110),
    );
    painter.circle_filled(center, radius, color);

    // Subtle dark hairline inner ring on light colors so white stands out
    let lum = 0.299 * color.r() as f32 + 0.587 * color.g() as f32 + 0.114 * color.b() as f32;
    if lum > 190.0 {
        painter.circle_stroke(
            center,
            radius - 0.5,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 60)),
        );
    }

    if selected {
        painter.circle_stroke(
            center,
            radius + 3.2,
            Stroke::new(2.0, accent_blue()),
        );
        painter.circle_stroke(
            center,
            radius + 1.2,
            Stroke::new(1.2, bg_inspector_bot()),
        );
    } else if hovered {
        painter.circle_stroke(
            center,
            radius + 2.4,
            Stroke::new(1.5, c_a(qs_theme::pal().wash, 160)),
        );
    } else {
        painter.circle_stroke(
            center,
            radius,
            Stroke::new(1.0, c_a(qs_theme::pal().wash, 45)),
        );
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t).round() as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t).round() as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t).round() as u8,
        (a.a() as f32 + (b.a() as f32 - a.a() as f32) * t).round() as u8,
    )
}
