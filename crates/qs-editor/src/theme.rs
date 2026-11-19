//! Apple Liquid Glass / Sonoma design tokens, palettes, and vector drawing helpers.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Stroke, StrokeKind, Vec2};
use eframe::epaint::{Mesh, Vertex, WHITE_UV};

// ---------------------------------------------------------------------------
// Design Tokens
// ---------------------------------------------------------------------------

pub const TOOLBAR_HEIGHT: f32 = 48.0;
pub const INSPECTOR_HEIGHT: f32 = 40.0;
pub const CHROME_PAD_X: f32 = 14.0;

pub const RADIUS_WINDOW: CornerRadius = CornerRadius::same(14);
pub const RADIUS_CARD: CornerRadius = CornerRadius::same(12);
pub const RADIUS_TRACK: CornerRadius = CornerRadius::same(10);
pub const RADIUS_PILL: CornerRadius = CornerRadius::same(8);
pub const RADIUS_BTN: CornerRadius = CornerRadius::same(8);
pub const RADIUS_CAPSULE: CornerRadius = CornerRadius::same(99);

// Colour tokens are functions, not consts: they read `qs_theme::pal()` at
// draw time so a theme switch applies on the next frame. Hex lives only in
// the `qs-theme` crate.

fn c(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn c_a(rgb: u32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, a)
}

/// Pick theme (`QS_THEME`) and accent (`QS_ACCENT`, hex `RRGGBB`) set by the
/// host that spawned us so the editor matches Settings 1:1.
pub fn init_theme_from_env() {
    if let Ok(t) = std::env::var("QS_THEME") {
        qs_theme::set_theme(&t);
    }
    if let Ok(a) = std::env::var("QS_ACCENT") {
        if let Ok(rgb) = u32::from_str_radix(a.trim().trim_start_matches('#'), 16) {
            qs_theme::set_accent(rgb);
        }
    }
}

// Backgrounds
pub fn bg_workspace_top() -> Color32 { c(qs_theme::pal().ws_top) }
pub fn bg_workspace_bot() -> Color32 { c(qs_theme::pal().ws_bottom) }
pub fn bg_toolbar_top() -> Color32 { c(qs_theme::pal().bar_top) }
pub fn bg_toolbar_bot() -> Color32 { c(qs_theme::pal().bar_bottom) }
pub fn bg_inspector_top() -> Color32 { c(qs_theme::pal().panel_top) }
pub fn bg_inspector_bot() -> Color32 { c(qs_theme::pal().panel_bottom) }
pub fn bg_card() -> Color32 { c(qs_theme::pal().card_bg) }
pub fn bg_status() -> Color32 { c(qs_theme::pal().status_bg) }
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

// Accents & Semantics (live runtime accent synced with qs-app Settings)
pub fn accent_blue() -> Color32 { c(qs_theme::accent()) }
pub fn accent_blue_top() -> Color32 { c(qs_theme::accent_top()) }
pub fn accent_blue_bot() -> Color32 { c(qs_theme::accent_bottom()) }
pub fn accent_blue_soft() -> Color32 { c(qs_theme::accent_soft()) }
pub fn accent_alpha(a: u8) -> Color32 { c_a(qs_theme::accent(), a) }
pub fn switch_off() -> Color32 { c(qs_theme::pal().switch_off) }
pub fn on_accent() -> Color32 { c(qs_theme::pal().on_accent) }
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
    if qs_theme::pal().dark { c(qs_theme::pal().hairline) } else { Color32::WHITE }
}

/// Selected tool segment: reads theme-specific active tool gradient tokens.
pub fn tool_active_gradient() -> (Color32, Color32) {
    (
        c(qs_theme::pal().tool_active_top),
        c(qs_theme::pal().tool_active_bot),
    )
}

// ---------------------------------------------------------------------------
// Global Egui Visuals
// ---------------------------------------------------------------------------

pub fn apply_macos_visuals(ctx: &egui::Context) {
    init_theme_from_env();
    sync_egui_visuals(ctx);
}

/// Re-apply egui visuals for the currently active `qs_theme::pal()` (used on
/// startup and whenever the user toggles Light/Dark live).
pub fn sync_egui_visuals(ctx: &egui::Context) {
    let dark = qs_theme::pal().dark;
    let theme = if dark { egui::Theme::Dark } else { egui::Theme::Light };
    ctx.set_theme(theme);
    ctx.style_mut_of(theme, |s| {
        s.visuals.dark_mode = dark;
        s.visuals.panel_fill = bg_inspector_top();
        s.visuals.window_fill = bg_card();
        s.visuals.window_stroke = Stroke::new(1.0, border_subtle());
        s.visuals.window_corner_radius = RADIUS_WINDOW;

        s.visuals.widgets.noninteractive.bg_fill = bg_inspector_top();
        s.visuals.widgets.noninteractive.weak_bg_fill = bg_inspector_top();
        s.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border_subtle());
        s.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text_primary());
        s.visuals.widgets.noninteractive.corner_radius = RADIUS_PILL;

        s.visuals.widgets.inactive.bg_fill = bg_btn_secondary_bot();
        s.visuals.widgets.inactive.weak_bg_fill = bg_btn_secondary_bot();
        s.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border_subtle());
        s.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, text_primary());
        s.visuals.widgets.inactive.corner_radius = RADIUS_PILL;

        s.visuals.widgets.hovered.bg_fill = bg_btn_secondary_hover_bot();
        s.visuals.widgets.hovered.weak_bg_fill = bg_btn_secondary_hover_bot();
        s.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, border_hot());
        s.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, text_primary());
        s.visuals.widgets.hovered.corner_radius = RADIUS_PILL;

        s.visuals.widgets.active.bg_fill = accent_alpha(38);
        s.visuals.widgets.active.weak_bg_fill = accent_alpha(38);
        s.visuals.widgets.active.bg_stroke = Stroke::new(1.0, accent_blue());
        s.visuals.widgets.active.fg_stroke = Stroke::new(1.0, text_primary());
        s.visuals.widgets.active.corner_radius = RADIUS_PILL;

        s.visuals.selection.bg_fill = accent_alpha(if dark { 70 } else { 42 });
        s.visuals.selection.stroke = Stroke::new(1.0, accent_blue());

        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
        s.spacing.slider_width = 132.0;
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

// ---------------------------------------------------------------------------
// Liquid Glass Inspector Cards, Switches, Segmented Controls & Modal Chrome
// ---------------------------------------------------------------------------

/// Elevated Liquid Glass card container for sidebar inspector sections.
pub fn inspector_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let resp = egui::Frame::NONE
        .fill(bg_card())
        .stroke(Stroke::new(1.0, border_subtle()))
        .corner_radius(RADIUS_CARD)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
    // Specular top-edge highlight for Liquid Glass depth
    let r = resp.response.rect;
    let spec = if qs_theme::pal().dark {
        Color32::from_white_alpha(18)
    } else {
        Color32::from_white_alpha(200)
    };
    ui.painter().line_segment(
        [
            Pos2::new(r.left() + 12.0, r.top() + 0.75),
            Pos2::new(r.right() - 12.0, r.top() + 0.75),
        ],
        Stroke::new(1.0, spec),
    );
}

/// Consistent uppercase-tracked section header inside inspector cards.
pub fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.label(
        egui::RichText::new(title.to_uppercase())
            .size(10.0)
            .strong()
            .color(text_tertiary()),
    );
    ui.add_space(6.0);
}

/// Apple macOS / Liquid Glass pill toggle switch row (label on left, 32×18
/// capsule switch on right). Matches the custom SDF switch in `qs-app`.
pub fn toggle_switch(ui: &mut egui::Ui, on: &mut bool, label: &str) -> bool {
    let row_h = 24.0;
    let width = ui.available_width().max(120.0);
    let (row_rect, resp) = ui.allocate_exact_size(Vec2::new(width, row_h), egui::Sense::click());
    let mut changed = false;
    if resp.clicked() {
        *on = !*on;
        changed = true;
    }

    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(
            row_rect.expand2(Vec2::new(4.0, 1.0)),
            RADIUS_PILL,
            bg_pill_hover(),
        );
    }

    // Label on the left
    painter.text(
        Pos2::new(row_rect.left(), row_rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.5),
        if *on { text_primary() } else { text_secondary() },
    );

    // 32×18 capsule switch on the right
    let sw_w = 32.0;
    let sw_h = 18.0;
    let sw_rect = Rect::from_min_size(
        Pos2::new(row_rect.right() - sw_w, row_rect.center().y - sw_h * 0.5),
        Vec2::new(sw_w, sw_h),
    );
    let track_fill = if *on { accent_blue() } else { switch_off() };
    painter.rect_filled(sw_rect, RADIUS_CAPSULE, track_fill);
    if !*on {
        painter.rect_stroke(
            sw_rect,
            RADIUS_CAPSULE,
            Stroke::new(0.75, border_subtle()),
            StrokeKind::Inside,
        );
    }

    // Crisp white sliding knob with subtle drop shadow
    let knob_r = 7.0;
    let knob_x = if *on {
        sw_rect.right() - 9.0
    } else {
        sw_rect.left() + 9.0
    };
    let knob_center = Pos2::new(knob_x, sw_rect.center().y);
    painter.circle_filled(
        knob_center + Vec2::new(0.0, 0.8),
        knob_r,
        Color32::from_rgba_unmultiplied(0, 0, 0, 36),
    );
    painter.circle_filled(knob_center, knob_r, on_accent());

    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    changed
}

/// Sleek Liquid Glass pill button (accent-tinted tonal or subtle secondary).
pub fn tonal_button(ui: &mut egui::Ui, label: &str, accent_tint: bool, tip: &str) -> egui::Response {
    let h = 28.0;
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, h), egui::Sense::click());
    let hovered = resp.hovered();
    let painter = ui.painter();

    if accent_tint {
        let fill = if hovered {
            accent_alpha(if qs_theme::pal().dark { 58 } else { 36 })
        } else {
            accent_alpha(if qs_theme::pal().dark { 36 } else { 22 })
        };
        painter.rect_filled(rect, RADIUS_BTN, fill);
        painter.rect_stroke(
            rect,
            RADIUS_BTN,
            Stroke::new(1.0, accent_alpha(if hovered { 140 } else { 80 })),
            StrokeKind::Inside,
        );
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(12.0),
            if qs_theme::pal().dark { accent_blue_soft() } else { accent_blue() },
        );
    } else {
        paint_secondary_button(painter, rect, RADIUS_BTN, hovered, true, false);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(12.0),
            text_primary(),
        );
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if tip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tip)
    }
}

/// Vector icon kinds for the Transform segmented bar (drawn with `egui::Painter`
/// so they render razor-sharp like SF Symbols on every OS without font fallback).
#[derive(Clone, Copy)]
pub enum TransformIcon {
    RotateCcw,
    RotateCw,
    FlipH,
    FlipV,
}

fn draw_transform_icon(painter: &egui::Painter, c: Pos2, icon: TransformIcon, color: Color32) {
    let st = Stroke::new(1.5, color);
    match icon {
        TransformIcon::RotateCcw | TransformIcon::RotateCw => {
            let cw = matches!(icon, TransformIcon::RotateCw);
            let r = 5.2;
            let steps = 14;
            let (a0, a1) = if cw {
                (-std::f32::consts::FRAC_PI_2 - 0.3, std::f32::consts::PI * 1.15)
            } else {
                (-std::f32::consts::FRAC_PI_2 + 0.3, -std::f32::consts::PI * 1.85)
            };
            let pts: Vec<Pos2> = (0..=steps)
                .map(|i| {
                    let a = a0 + (a1 - a0) * (i as f32 / steps as f32);
                    Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
                })
                .collect();
            painter.add(egui::Shape::line(pts, st));
            // Arrowhead at top of arc
            let tip = Pos2::new(c.x + if cw { 1.8 } else { -1.8 }, c.y - r);
            let dx = if cw { -3.2 } else { 3.2 };
            painter.line_segment([tip, Pos2::new(tip.x + dx, tip.y - 2.2)], st);
            painter.line_segment([tip, Pos2::new(tip.x + dx, tip.y + 2.5)], st);
        }
        TransformIcon::FlipH => {
            // Vertical center axis + two mirrored triangles
            painter.line_segment([Pos2::new(c.x, c.y - 6.0), Pos2::new(c.x, c.y + 6.0)], Stroke::new(1.1, divider_vertical()));
            let left = vec![
                Pos2::new(c.x - 2.0, c.y - 4.5),
                Pos2::new(c.x - 6.5, c.y + 4.5),
                Pos2::new(c.x - 2.0, c.y + 4.5),
            ];
            painter.add(egui::Shape::convex_polygon(left, color, Stroke::NONE));
            let right = vec![
                Pos2::new(c.x + 2.0, c.y - 4.5),
                Pos2::new(c.x + 6.5, c.y + 4.5),
                Pos2::new(c.x + 2.0, c.y + 4.5),
            ];
            painter.add(egui::Shape::closed_line(right, Stroke::new(1.3, color)));
        }
        TransformIcon::FlipV => {
            // Horizontal center axis + two mirrored triangles
            painter.line_segment([Pos2::new(c.x - 6.0, c.y), Pos2::new(c.x + 6.0, c.y)], Stroke::new(1.1, divider_vertical()));
            let top = vec![
                Pos2::new(c.x - 4.5, c.y - 2.0),
                Pos2::new(c.x + 4.5, c.y - 6.5),
                Pos2::new(c.x + 4.5, c.y - 2.0),
            ];
            painter.add(egui::Shape::convex_polygon(top, color, Stroke::NONE));
            let bot = vec![
                Pos2::new(c.x - 4.5, c.y + 2.0),
                Pos2::new(c.x + 4.5, c.y + 6.5),
                Pos2::new(c.x + 4.5, c.y + 2.0),
            ];
            painter.add(egui::Shape::closed_line(bot, Stroke::new(1.3, color)));
        }
    }
}

/// Single-row Liquid Glass segmented vector-icon bar (used for Transform).
pub fn segmented_icon_bar(ui: &mut egui::Ui, items: &[(TransformIcon, &str)]) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    let h = 30.0;
    let total_w = ui.available_width();
    let (track_rect, _) = ui.allocate_exact_size(Vec2::new(total_w, h), egui::Sense::hover());
    let painter = ui.painter();
    paint_segmented_track(painter, track_rect, RADIUS_PILL);

    let inner = track_rect.shrink(2.0);
    let seg_w = inner.width() / items.len() as f32;
    let mut clicked = None;

    for (i, (icon, tip)) in items.iter().enumerate() {
        let seg_rect = Rect::from_min_size(
            Pos2::new(inner.left() + i as f32 * seg_w, inner.top()),
            Vec2::new(seg_w, inner.height()),
        );
        let resp = ui.interact(seg_rect, ui.scope_id().with(("seg_icon", i)), egui::Sense::click());
        let hovered = resp.hovered();
        if hovered {
            painter.rect_filled(seg_rect, CornerRadius::same(6), bg_btn_secondary_top());
            painter.rect_stroke(
                seg_rect,
                CornerRadius::same(6),
                Stroke::new(0.75, border_hot()),
                StrokeKind::Inside,
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if i > 0 {
            painter.line_segment(
                [
                    Pos2::new(seg_rect.left(), inner.top() + 5.0),
                    Pos2::new(seg_rect.left(), inner.bottom() - 5.0),
                ],
                Stroke::new(1.0, divider_vertical()),
            );
        }
        draw_transform_icon(
            painter,
            seg_rect.center(),
            *icon,
            if hovered { text_primary() } else { text_secondary() },
        );
        if resp.on_hover_text(*tip).clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// Centered subtle empty state for panels (Layers, Recent).
pub fn empty_state(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(12.5)
                .strong()
                .color(text_secondary()),
        );
        ui.add_space(3.0);
        ui.label(
            egui::RichText::new(subtitle)
                .size(11.0)
                .color(text_tertiary()),
        );
    });
}

/// Consistent modal window frame for editor dialogs.
pub fn modal_frame() -> egui::Frame {
    egui::Frame::window(&egui::Style::default())
        .fill(bg_card())
        .stroke(Stroke::new(1.0, border_hot()))
        .corner_radius(RADIUS_WINDOW)
        .inner_margin(egui::Margin::same(16))
}
