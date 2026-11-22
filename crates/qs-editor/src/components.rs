//! Layer 2 — Standardized UI Component Primitives for `qs-editor`.
//!
//! Strictly token-driven from `qs_theme::{space, radius, font}` and `crate::theme`
//! colour roles. Follows:
//! - Vercel Labs Web Interface Guidelines (4px grid, >= 24px hit targets, 1 primary CTA)
//! - `baseline-ui` (flat crisp surfaces, no artificial gradients, empty state with 1 action)
//! - Apple HIG (inner-only 1px hairlines, concentric radii, SF-Symbol stroke weight)

use crate::theme;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use qs_theme::{font, radius, space};

#[inline]
fn sp(v: f64) -> f32 {
    v as f32
}

#[inline]
fn rad(v: f64) -> CornerRadius {
    CornerRadius::same(v.round().min(255.0) as u8)
}

#[inline]
fn fs(v: f64) -> f32 {
    v as f32
}

/// Inner-only panel hairline edge (prevents 4-sided double borders at panel junctions).
#[derive(Clone, Copy)]
pub enum DividerEdge {
    Top,
    Bottom,
    Left,
    Right,
}

/// Draw a single 1px hairline divider on the inner seam of a docked panel.
pub fn panel_divider(ui: &egui::Ui, rect: Rect, edge: DividerEdge) {
    let seg = match edge {
        DividerEdge::Top => [rect.left_top(), rect.right_top()],
        DividerEdge::Bottom => [rect.left_bottom(), rect.right_bottom()],
        DividerEdge::Left => [rect.left_top(), rect.left_bottom()],
        DividerEdge::Right => [rect.right_top(), rect.right_bottom()],
    };
    ui.painter()
        .line_segment(seg, Stroke::new(1.0, theme::border_dark_hairline()));
}

/// Elevated inspector card (`radius::LG` = 10px, `space::MD` = 12px padding).
pub fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let resp = egui::Frame::NONE
        .fill(theme::bg_card())
        .stroke(Stroke::new(1.0, theme::border_subtle()))
        .corner_radius(rad(radius::LG))
        .inner_margin(egui::Margin::same(sp(space::MD) as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
    let r = resp.response.rect;
    let spec = if qs_theme::pal().dark {
        Color32::from_white_alpha(16)
    } else {
        Color32::from_white_alpha(210)
    };
    ui.painter().line_segment(
        [
            Pos2::new(r.left() + sp(radius::LG), r.top() + 0.75),
            Pos2::new(r.right() - sp(radius::LG), r.top() + 0.75),
        ],
        Stroke::new(1.0, spec),
    );
}

/// Uppercase section header inside a card (`font::CAPTION`, `space::SM` bottom gap).
pub fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.label(
        egui::RichText::new(title.to_uppercase())
            .size(fs(font::CAPTION))
            .strong()
            .color(theme::text_tertiary()),
    );
    ui.add_space(sp(space::SM));
}

/// Button hierarchy variants (strictly 1 Primary CTA per view; Tonal/Secondary/Ghost elsewhere).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Tonal,
    Secondary,
    Ghost,
}

/// Standardized full-width button (`28px` height >= 24px hit target, `radius::MD` = 8px).
pub fn button(
    ui: &mut egui::Ui,
    label: &str,
    variant: ButtonVariant,
    tip: &str,
) -> egui::Response {
    let h = sp(space::XL + space::XS); // 28px
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, h), Sense::click());
    let hovered = resp.hovered();
    let painter = ui.painter();
    let cr = rad(radius::MD);

    match variant {
        ButtonVariant::Primary => {
            let fill = if hovered {
                theme::accent_blue_top()
            } else {
                theme::accent_blue()
            };
            painter.rect_filled(rect, cr, fill);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(fs(font::SMALL) + 0.5),
                theme::on_accent(),
            );
        }
        ButtonVariant::Tonal => {
            let fill = if hovered {
                theme::accent_alpha(if qs_theme::pal().dark { 56 } else { 34 })
            } else {
                theme::accent_alpha(if qs_theme::pal().dark { 36 } else { 20 })
            };
            painter.rect_filled(rect, cr, fill);
            painter.rect_stroke(
                rect,
                cr,
                Stroke::new(1.0, theme::accent_alpha(if hovered { 130 } else { 72 })),
                StrokeKind::Inside,
            );
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(fs(font::SMALL) + 0.5),
                if qs_theme::pal().dark {
                    theme::accent_blue_soft()
                } else {
                    theme::accent_blue()
                },
            );
        }
        ButtonVariant::Secondary => {
            let fill = if hovered {
                theme::bg_btn_secondary_hover_bot()
            } else {
                theme::bg_btn_secondary_bot()
            };
            let border = if hovered {
                theme::border_hot()
            } else {
                theme::border_subtle()
            };
            painter.rect_filled(rect, cr, fill);
            painter.rect_stroke(rect, cr, Stroke::new(1.0, border), StrokeKind::Inside);
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(fs(font::SMALL) + 0.5),
                theme::text_primary(),
            );
        }
        ButtonVariant::Ghost => {
            if hovered {
                painter.rect_filled(rect, cr, theme::bg_pill_hover());
            }
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(fs(font::SMALL) + 0.5),
                if hovered {
                    theme::text_primary()
                } else {
                    theme::text_secondary()
                },
            );
        }
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

/// Standardized Apple toggle switch row (`28px` hit target >= 24px, aligned label + capsule switch).
pub fn switch_row(ui: &mut egui::Ui, on: &mut bool, label: &str) -> bool {
    // 26 px control plus a hair of breathing room, matching the pill scale.
    let row_h = CONTROL_H + sp(space::XS);
    let width = ui.available_width().max(120.0);
    let (row_rect, resp) = ui.allocate_exact_size(Vec2::new(width, row_h), Sense::click());
    let mut changed = false;
    if resp.clicked() {
        *on = !*on;
        changed = true;
    }

    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(row_rect, rad(radius::SM), theme::bg_pill_hover());
    }

    painter.text(
        Pos2::new(row_rect.left() + sp(space::XS), row_rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(fs(font::BODY) - 0.5),
        if *on {
            theme::text_primary()
        } else {
            theme::text_secondary()
        },
    );

    let sw_w = sp(space::XXL); // 32px
    let sw_h = 18.0;
    let sw_rect = Rect::from_min_size(
        Pos2::new(
            row_rect.right() - sw_w - sp(space::XS),
            row_rect.center().y - sw_h * 0.5,
        ),
        Vec2::new(sw_w, sw_h),
    );
    let capsule = rad(radius::PILL);
    painter.rect_filled(
        sw_rect,
        capsule,
        if *on {
            theme::accent_blue()
        } else {
            theme::switch_off()
        },
    );

    let knob_r = 7.0;
    let knob_x = if *on {
        sw_rect.right() - 9.0
    } else {
        sw_rect.left() + 9.0
    };
    let knob_center = Pos2::new(knob_x, sw_rect.center().y);
    painter.circle_filled(
        knob_center + Vec2::new(0.0, 0.75),
        knob_r,
        Color32::from_rgba_unmultiplied(0, 0, 0, 32),
    );
    painter.circle_filled(knob_center, knob_r, theme::on_accent());

    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    changed
}

/// Vector icon kinds for Transform segmented control (1.5px SF-Symbol stroke weight).
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
            let tip = Pos2::new(c.x + if cw { 1.8 } else { -1.8 }, c.y - r);
            let dx = if cw { -3.2 } else { 3.2 };
            painter.line_segment([tip, Pos2::new(tip.x + dx, tip.y - 2.2)], st);
            painter.line_segment([tip, Pos2::new(tip.x + dx, tip.y + 2.5)], st);
        }
        TransformIcon::FlipH => {
            painter.line_segment(
                [Pos2::new(c.x, c.y - 6.0), Pos2::new(c.x, c.y + 6.0)],
                Stroke::new(1.1, theme::divider_vertical()),
            );
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
            painter.line_segment(
                [Pos2::new(c.x - 6.0, c.y), Pos2::new(c.x + 6.0, c.y)],
                Stroke::new(1.1, theme::divider_vertical()),
            );
            let top = vec![
                Pos2::new(c.x - 4.5, c.y - 2.0),
                Pos2::new(c.x + 4.5, c.y - 2.0),
                Pos2::new(c.x, c.y - 6.5),
            ];
            painter.add(egui::Shape::convex_polygon(top, color, Stroke::NONE));
            let bot = vec![
                Pos2::new(c.x - 4.5, c.y + 2.0),
                Pos2::new(c.x + 4.5, c.y + 2.0),
                Pos2::new(c.x, c.y + 6.5),
            ];
            painter.add(egui::Shape::closed_line(bot, Stroke::new(1.3, color)));
        }
    }
}

/// Single-row segmented vector-icon bar (`32px` height, concentric `radius::MD` / `radius::SM`).
pub fn segmented_icon_bar(ui: &mut egui::Ui, items: &[(TransformIcon, &str)]) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    let h = sp(space::XXL); // 32px
    let total_w = ui.available_width();
    let (track_rect, _) = ui.allocate_exact_size(Vec2::new(total_w, h), Sense::hover());
    let painter = ui.painter();
    theme::paint_segmented_track(painter, track_rect, rad(radius::MD));

    let pad = sp(space::HAIR); // 2px
    let inner = track_rect.shrink(pad);
    let seg_w = inner.width() / items.len() as f32;
    let mut clicked = None;

    for (i, (icon, tip)) in items.iter().enumerate() {
        let seg_rect = Rect::from_min_size(
            Pos2::new(inner.left() + i as f32 * seg_w, inner.top()),
            Vec2::new(seg_w, inner.height()),
        );
        let resp = ui.interact(seg_rect, ui.scope_id().with(("seg_icon", i)), Sense::click());
        let hovered = resp.hovered();
        if hovered {
            painter.rect_filled(seg_rect, rad(radius::SM), theme::bg_card());
            painter.rect_stroke(
                seg_rect,
                rad(radius::SM),
                Stroke::new(0.75, theme::border_hot()),
                StrokeKind::Inside,
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if i > 0 {
            painter.line_segment(
                [
                    Pos2::new(seg_rect.left(), inner.top() + sp(space::XS)),
                    Pos2::new(seg_rect.left(), inner.bottom() - sp(space::XS)),
                ],
                Stroke::new(1.0, theme::divider_vertical()),
            );
        }
        draw_transform_icon(
            painter,
            seg_rect.center(),
            *icon,
            if hovered {
                theme::text_primary()
            } else {
                theme::text_secondary()
            },
        );
        if resp.on_hover_text(*tip).clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

/// Status bar pill (`24px` height >= 24px Vercel Labs hit-target minimum, `radius::SM`).
pub fn status_pill(ui: &mut egui::Ui, label: &str, active: bool, tip: &str) -> bool {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(fs(font::SMALL)),
        theme::text_primary(),
    );
    let size = Vec2::new(galley.size().x + sp(space::LG), CONTROL_H);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    let painter = ui.painter();
    let cr = rad(radius::SM);

    if active {
        let fill = theme::accent_alpha(if qs_theme::pal().dark { 48 } else { 26 });
        painter.rect_filled(rect, cr, fill);
        painter.rect_stroke(
            rect,
            cr,
            Stroke::new(0.75, theme::accent_alpha(if qs_theme::pal().dark { 110 } else { 70 })),
            StrokeKind::Inside,
        );
    } else if hovered {
        painter.rect_filled(rect, cr, theme::bg_pill_hover());
    }

    let fg = if active {
        if qs_theme::pal().dark {
            theme::accent_blue_soft()
        } else {
            theme::accent_blue()
        }
    } else if hovered {
        theme::text_primary()
    } else {
        theme::text_secondary()
    };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(fs(font::SMALL)),
        fg,
    );
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let resp = if tip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tip)
    };
    resp.clicked()
}

/// Empty state following `baseline-ui` ("MUST give empty states one clear next action").
/// Returns `true` when the primary next-action button is clicked.
/// Compact action button: filled pill with a hairline, hover lightens it.
///
/// `status_pill` is a *toggle* — inactive it draws text only, which is wrong
/// for an action. Actions use this instead.
/// One control height for row-level actions (chips, Library buttons, Open).
pub const CONTROL_H: f32 = 26.0;

pub fn pill_button(ui: &mut egui::Ui, label: &str, tip: &str) -> bool {
    pill_button_colored(ui, label, tip, theme::text_primary())
}

/// Same pill with a custom label colour, for destructive actions.
pub fn pill_button_colored(
    ui: &mut egui::Ui,
    label: &str,
    tip: &str,
    color: Color32,
) -> bool {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(fs(font::SMALL)),
        color,
    );
    let size = Vec2::new(galley.size().x + sp(space::LG), CONTROL_H);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    let painter = ui.painter();
    let cr = rad(radius::SM);
    painter.rect_filled(
        rect,
        cr,
        if hovered {
            theme::bg_btn_secondary_hover_top()
        } else {
            theme::bg_btn_secondary_top()
        },
    );
    painter.rect_stroke(
        rect,
        cr,
        Stroke::new(1.0, theme::border_subtle()),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(fs(font::SMALL)),
        color,
    );
    if tip.is_empty() {
        resp.clicked()
    } else {
        resp.on_hover_text(tip).clicked()
    }
}

pub fn empty_state(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    cta_label: Option<&str>,
) -> bool {
    let mut clicked = false;
    ui.add_space(sp(space::XL));
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(fs(font::BODY))
                .strong()
                .color(theme::text_secondary()),
        );
        ui.add_space(sp(space::XS));
        ui.label(
            egui::RichText::new(subtitle)
                .size(fs(font::SMALL))
                .color(theme::text_tertiary()),
        );
        if let Some(cta) = cta_label {
            ui.add_space(sp(space::MD));
            if button(ui, cta, ButtonVariant::Tonal, "").clicked() {
                clicked = true;
            }
        }
    });
    clicked
}
