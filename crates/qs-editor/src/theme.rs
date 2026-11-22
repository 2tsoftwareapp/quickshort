//! Apple Liquid Glass / Sonoma design tokens, palettes, and vector drawing helpers.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Stroke, StrokeKind, Vec2};
use eframe::epaint::{Mesh, Vertex, WHITE_UV};

// ---------------------------------------------------------------------------
// Design Tokens (derived strictly from `qs_theme::{space, radius, font}`)
// ---------------------------------------------------------------------------

pub const TOOLBAR_HEIGHT: f32 = 48.0;
pub const INSPECTOR_HEIGHT: f32 = 40.0;
pub const CHROME_PAD_X: f32 = qs_theme::space::MD as f32;

pub const RADIUS_WINDOW: CornerRadius = CornerRadius::same(qs_theme::radius::XL as u8);
pub const RADIUS_CARD: CornerRadius = CornerRadius::same(qs_theme::radius::LG as u8);
/// macOS 26 toolbar groups are capsules; egui clamps to half the height.
pub const RADIUS_TRACK: CornerRadius = CornerRadius::same(255);
pub const RADIUS_PILL: CornerRadius = CornerRadius::same(qs_theme::radius::SM as u8);
pub const RADIUS_BTN: CornerRadius = CornerRadius::same(qs_theme::radius::MD as u8);
pub const RADIUS_CAPSULE: CornerRadius = CornerRadius::same(255);

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
        s.visuals.menu_corner_radius = RADIUS_CARD;
        s.visuals.popup_shadow = soft_shadow();
        s.visuals.window_shadow = soft_shadow();

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
        // The inspector is 236 px wide; a wider rail pushed its labels and
        // value fields outside the panel.
        s.spacing.slider_width = 84.0;
        // Stock sliders/combos follow the same control height and token
        // colours as the pill language: 26 px tall, token rail, round knob,
        // accent trailing fill.
        s.spacing.interact_size.y = crate::components::CONTROL_H;
        s.visuals.slider_trailing_fill = true;
        s.visuals.handle_shape = egui::style::HandleShape::Circle;
        s.visuals.widgets.inactive.bg_fill = bg_pill_track();
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
    let steps = 8;
    for s in 0..steps {
        let t = (s + 1) as f32 / steps as f32;
        let spread = blur_inner + (blur_outer - blur_inner) * t;
        let shadow_rect = rect
            .translate(Vec2::new(0.0, 2.0 + spread * 0.32))
            .expand(spread * 0.45);
        let falloff = (1.0 - t * 0.85).powi(2);
        let alpha = ((max_alpha as f32) * falloff / (steps as f32) * 2.2).round().clamp(1.0, 255.0) as u8;
        painter.rect_filled(
            shadow_rect,
            CornerRadius::same((corner_radius + spread * 0.4).round() as u8),
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
    _top: Color32,
    bot: Color32,
    _is_top_level: bool,
) {
    // macOS 26: no bar slab; control groups float over the workspace.
    let _ = bot;
    painter.rect_filled(rect, CornerRadius::ZERO, bg_workspace_top());
}

fn soft_shadow() -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, 2],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(if qs_theme::pal().dark { 70 } else { 22 }),
    }
}

/// Floating glass panel (Layers, Inspector): inset from the window edge,
/// large concentric corners, hairline edge and a soft drop shadow.
pub fn glass_panel_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(bg_inspector_top())
        .corner_radius(CornerRadius::same(qs_theme::radius::XL as u8))
        .stroke(Stroke::new(1.0, border_subtle()))
        .shadow(soft_shadow())
        .inner_margin(egui::Margin::same(qs_theme::space::MD as i8))
}

/// Paints the glass card over the whole space the panel gives it and lays
/// `add` out inside, clipped — content wider than the panel can no longer
/// stretch the card past the window edge.
pub fn glass_panel<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let rect = ui.available_rect_before_wrap();
    let cr = CornerRadius::same(qs_theme::radius::XL as u8);
    ui.painter().add(soft_shadow().as_shape(rect, cr));
    ui.painter().rect(
        rect,
        cr,
        bg_inspector_top(),
        Stroke::new(1.0, border_subtle()),
        StrokeKind::Inside,
    );
    let inner = rect.shrink(qs_theme::space::MD as f32);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.set_clip_rect(rect.shrink(1.0).intersect(ui.clip_rect()));
    // stock sliders size their rail from a fixed width; fit it to the card
    child.spacing_mut().slider_width = (inner.width() - 150.0).clamp(48.0, 84.0);
    let out = add(&mut child);
    ui.allocate_rect(rect, egui::Sense::hover());
    out
}

/// Transparent gutter a side panel reserves around its `glass_panel_frame`,
/// so the panel floats inset from the window edge.
pub fn panel_gutter() -> egui::Frame {
    let gap = qs_theme::space::SM as i8;
    egui::Frame::NONE.inner_margin(egui::Margin { left: gap, right: gap, top: 0, bottom: gap })
}

pub fn paint_segmented_track(painter: &egui::Painter, rect: Rect, corner_radius: CornerRadius) {
    painter.add(soft_shadow().as_shape(rect, corner_radius));
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
        let fill = if hovered { accent_blue_top() } else { accent_blue() };
        painter.rect_filled(rect, cr, fill);
        return;
    }

    if active {
        painter.rect_filled(rect, cr, bg_card());
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

    let (fill, border) = if hovered && danger_hover {
        (c(qs_theme::pal().danger_top), c(qs_theme::pal().danger_edge))
    } else if hovered {
        (bg_btn_secondary_hover_bot(), border_hot())
    } else {
        (bg_btn_secondary_bot(), border_subtle())
    };

    painter.rect_filled(rect, cr, fill);
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
    let shadow_a = if qs_theme::pal().dark { 72 } else { 26 };
    painter.circle_filled(
        center + Vec2::new(0.0, 0.8),
        radius,
        Color32::from_rgba_unmultiplied(0, 0, 0, shadow_a),
    );
    painter.circle_filled(center, radius, color);

    // Subtle dark hairline inner ring on light colors so white stands out cleanly
    let lum = 0.299 * color.r() as f32 + 0.587 * color.g() as f32 + 0.114 * color.b() as f32;
    if lum > 190.0 {
        painter.circle_stroke(
            center,
            radius - 0.5,
            Stroke::new(0.85, Color32::from_rgba_unmultiplied(0, 0, 0, 52)),
        );
    }

    if selected {
        painter.circle_stroke(
            center,
            radius + 2.5,
            Stroke::new(1.75, accent_blue()),
        );
        painter.circle_stroke(
            center,
            radius + 0.9,
            Stroke::new(1.0, bg_inspector_top()),
        );
    } else if hovered {
        painter.circle_stroke(
            center,
            radius + 2.0,
            Stroke::new(1.25, c_a(qs_theme::pal().wash, 120)),
        );
    } else {
        painter.circle_stroke(
            center,
            radius,
            Stroke::new(0.75, c_a(qs_theme::pal().wash, 35)),
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
// Modal Chrome & Component Bridges (components live in `crate::components`)
// ---------------------------------------------------------------------------

/// Rounded card container for sidebar inspector sections (delegates to `components::card`).
pub fn inspector_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    crate::components::card(ui, add_contents);
}

/// Consistent uppercase section header (delegates to `components::section_header`).
pub fn section_header(ui: &mut egui::Ui, title: &str) {
    crate::components::section_header(ui, title);
}

/// Consistent modal window frame for editor dialogs (`radius::XL`, `space::LG` padding).
pub fn modal_frame() -> egui::Frame {
    egui::Frame::window(&egui::Style::default())
        .fill(bg_card())
        .stroke(Stroke::new(1.0, border_hot()))
        .corner_radius(RADIUS_WINDOW)
        .inner_margin(egui::Margin::same(qs_theme::space::LG as i8))
}
