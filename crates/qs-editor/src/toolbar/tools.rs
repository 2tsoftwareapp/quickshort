//! Tier 1.5: flat grouped tool palette — every tool visible, its bound
//! key drawn as a bottom-right subscript.

use crate::{
    icons,
    theme::*,
};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use qs_edit::{Editor, Tool};

/// Tools grouped exactly like the locked mockup: hairline dividers between
/// functional groups, every tool visible (no overflow). The four Polygon
/// shapes land in the third group via `Tool::key()` in Phase 5.
const TOOL_GROUPS: &[&[Tool]] = &[
    &[Tool::Select, Tool::SelectRegion, Tool::SmartMove],
    &[Tool::Pen, Tool::Highlighter, Tool::Eraser],
    &[
        Tool::Line,
        Tool::Arrow,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Triangle,
        Tool::Diamond,
        Tool::Star,
        Tool::Hexagon,
        Tool::Fill,
    ],
    &[Tool::Text, Tool::Callout, Tool::Step, Tool::Stamp],
    &[Tool::Blur, Tool::Spotlight, Tool::Magnify, Tool::Cutout],
    &[Tool::Crop],
];

/// Second chrome row: flat grouped tool palette. Every tool button shows its
/// bound key as a bottom-right subscript; the active tool's name sits at the
/// row's right edge. Returns true when a tool was clicked.
pub fn render_tool_palette(ui: &mut Ui, editor: &mut Editor) -> bool {
    let avail = ui.available_rect_before_wrap();
    let bar_rect = Rect::from_min_size(avail.min, Vec2::new(avail.width(), PALETTE_HEIGHT));
    ui.allocate_rect(bar_rect, Sense::hover());
    paint_chrome_bar(
        ui.painter(),
        bar_rect,
        bg_toolbar_top(),
        bg_toolbar_bot(),
        false,
    );

    let cur_tool = editor.tool();
    let cy = bar_rect.center().y;
    let n_tools: usize = TOOL_GROUPS.iter().map(|g| g.len()).sum();
    let n_divs = (TOOL_GROUPS.len() - 1) as f32;
    let div_w = 9.0;
    let tool_space = bar_rect.width() - 2.0 * CHROME_PAD_X - n_divs * div_w;
    let preferred_width = ((tool_space - 100.0) / n_tools as f32).clamp(24.0, 32.0);
    let tool_w = preferred_width.min(tool_space / n_tools as f32 - 2.0);

    let mut clicked = None;
    // Flat row anchored to the left edge — no enclosing capsule (mockup).
    let mut tx = bar_rect.left() + CHROME_PAD_X;
    for (gi, group) in TOOL_GROUPS.iter().enumerate() {
        if gi > 0 {
            tx += bar_sep(ui.painter(), tx, cy);
        }
        for &t in *group {
            let r = Rect::from_center_size(
                Pos2::new(tx + tool_w * 0.5, cy),
                Vec2::new(tool_w, 32.0),
            );
            let active = cur_tool == t;
            let resp = ui
                .interact(r, ui.auto_id_with(("pal_tool", t)), Sense::click())
                .on_hover_text(t.tooltip());
            paint_tool_segment(ui.painter(), r, active, hot_t(&resp));
            let icon_col = if active {
                accent_hi()
            } else if resp.hovered() {
                text_primary()
            } else {
                text_secondary()
            };
            let icon_size = if tool_w < 24.0 { 13.0 } else { 17.0 };
            icons::draw_tool_icon_sz(ui.painter(), r.center(), t, icon_col, icon_size);
            // Shortcut subscript — same source as the key dispatch chain.
            ui.painter().text(
                Pos2::new(r.right() - 2.5, r.bottom() - 1.0),
                egui::Align2::RIGHT_BOTTOM,
                t.key().to_ascii_uppercase().to_string(),
                FontId::monospace(8.5),
                if active {
                    accent_hi()
                } else {
                    text_tertiary()
                },
            );
            if resp.clicked() {
                clicked = Some(t);
            }
            tx += tool_w + 2.0;
        }
    }
    let tools_end = tx;

    // Active-tool name tag at the row's right edge, skipped when it would
    // collide with the buttons on narrow windows.
    let tag = cur_tool.label();
    let galley = ui.painter().layout_no_wrap(
        tag.to_owned(),
        FontId::proportional(qs_theme::font::SMALL as f32),
        text_tertiary(),
    );
    let tag_right = bar_rect.right() - CHROME_PAD_X;
    if tag_right - galley.size().x > tools_end + 8.0 {
        ui.painter().galley(
            Pos2::new(tag_right - galley.size().x, cy - galley.size().y * 0.5),
            galley,
            text_tertiary(),
        );
    }

    if let Some(t) = clicked {
        editor.set_tool(t);
        return true;
    }
    false
}

/// `.ibtn.on` — accent at 20 % over the bar, square-ish corner (mockup).
fn paint_tool_segment(painter: &egui::Painter, rect: Rect, active: bool, hot: f32) {
    if active {
        painter.rect_filled(rect, 6.0, accent_alpha(51));
    } else if hot > 0.0 {
        let hov = bg_btn_secondary_top();
        painter.rect_filled(
            rect,
            6.0,
            Color32::from_rgba_unmultiplied(
                hov.r(),
                hov.g(),
                hov.b(),
                (hov.a() as f32 * hot) as u8,
            ),
        );
    }
}

/// `.sep` — 1×18 hairline between chrome groups; returns the slot width.
fn bar_sep(painter: &egui::Painter, x: f32, cy: f32) -> f32 {
    let px = (x + 4.0).round() + 0.5;
    painter.line_segment(
        [Pos2::new(px, cy - 9.0), Pos2::new(px, cy + 9.0)],
        Stroke::new(1.0, divider_vertical()),
    );
    9.0
}
