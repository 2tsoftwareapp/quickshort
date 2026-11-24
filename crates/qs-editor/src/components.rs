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

/// Linear RGBA blend — feed it `animate_bool_with_time` output for
/// hover fade transitions (`motion::FAST`-ish ~120ms).
fn cmix(a: Color32, b: Color32, t: f32) -> Color32 {
    let (a, b) = (a.to_array(), b.to_array());
    Color32::from_rgba_unmultiplied(
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t) as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t) as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t) as u8,
        (a[3] as f32 + (b[3] as f32 - a[3] as f32) * t) as u8,
    )
}

/// Display the platform's primary modifier in editor shortcut hints.
pub(crate) fn platform_shortcut(text: &str) -> std::borrow::Cow<'_, str> {
    if cfg!(target_os = "macos") && text.contains("Ctrl+") {
        return text.replace("Ctrl+Shift+", "⌘⇧").replace("Ctrl+", "⌘").into();
    }
    text.into()
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

/// Elevated inspector card (`radius::SM` = 8px, `space::MD` = 12px padding).
/// Section cards sit one level below floating popovers/modals (`radius::LG`),
/// so they stay visibly squarer — DESIGN.md §3 L2 vs L4.
pub fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let resp = egui::Frame::NONE
        .fill(theme::bg_card())
        .stroke(Stroke::new(1.0, theme::border_subtle()))
        .corner_radius(rad(radius::SM))
        .inner_margin(egui::Margin::symmetric(sp(space::MD) as i8, 11))
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
            Pos2::new(r.left() + sp(radius::SM), r.top() + 0.75),
            Pos2::new(r.right() - sp(radius::SM), r.top() + 0.75),
        ],
        Stroke::new(1.0, spec),
    );
}

/// Uppercase section header inside a card — the mockup's `h4`: 11px/600 with
/// `.05em` tracking and a 9px bottom gap.
pub fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.label(
        egui::RichText::new(title.to_uppercase())
            .size(fs(font::CAPTION))
            .strong()
            .extra_letter_spacing(0.55)
            .color(theme::text_tertiary()),
    );
    ui.add_space(sp(space::XS));
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
pub fn button(ui: &mut egui::Ui, label: &str, variant: ButtonVariant, tip: &str) -> egui::Response {
    let h = sp(space::XL + space::XS); // 28px
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, h), Sense::click());
    let hovered = resp.hovered();
    let ht = ui.ctx().animate_bool_with_time(resp.id, hovered, 0.12);
    let painter = ui.painter();
    let cr = rad(radius::MD);

    match variant {
        ButtonVariant::Primary => {
            let fill = cmix(theme::accent_blue(), theme::accent_blue_top(), ht);
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
            let (rest, hot) = if qs_theme::pal().dark {
                (36, 56)
            } else {
                (20, 34)
            };
            let fill = cmix(theme::accent_alpha(rest), theme::accent_alpha(hot), ht);
            painter.rect_filled(rect, cr, fill);
            painter.rect_stroke(
                rect,
                cr,
                Stroke::new(
                    1.0,
                    cmix(theme::accent_alpha(72), theme::accent_alpha(130), ht),
                ),
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
            let fill = cmix(
                theme::bg_btn_secondary_bot(),
                theme::bg_btn_secondary_hover_bot(),
                ht,
            );
            let border = cmix(theme::border_subtle(), theme::border_hot(), ht);
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
            if ht > 0.0 {
                let hov = theme::bg_pill_hover();
                painter.rect_filled(
                    rect,
                    cr,
                    Color32::from_rgba_unmultiplied(
                        hov.r(),
                        hov.g(),
                        hov.b(),
                        (hov.a() as f32 * ht) as u8,
                    ),
                );
            }
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(fs(font::SMALL) + 0.5),
                cmix(theme::text_secondary(), theme::text_primary(), ht),
            );
        }
    }

    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if tip.is_empty() {
        resp
    } else {
        resp.on_hover_text(platform_shortcut(tip))
    }
}

/// Standardized Apple toggle switch row (`28px` hit target >= 24px, aligned label + capsule switch).
pub fn switch_row(ui: &mut egui::Ui, on: &mut bool, label: &str) -> bool {
    // 26 px control plus a hair of breathing room, matching the pill scale.
    let row_h = CONTROL_H;
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

    let sw_w = 32.0;
    let sw_h = 19.0;
    let sw_rect = Rect::from_min_size(
        Pos2::new(
            row_rect.right() - sw_w - sp(space::XS),
            row_rect.center().y - sw_h * 0.5,
        ),
        Vec2::new(sw_w, sw_h),
    );
    let progress = ui.ctx().animate_bool_with_time(
        resp.id,
        *on,
        (qs_theme::motion::FAST / 1000.0) as f32,
    );
    let track = theme::switch_off().lerp_to_gamma(theme::accent_blue(), progress);
    painter.rect_filled(sw_rect, rad(radius::PILL), track);
    let knob_r = 7.5;
    let knob_x = sw_rect.left() + 9.5 + progress * (sw_w - 19.0);
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
    let paths = match icon {
        TransformIcon::RotateCcw => qs_icons::ROTATE_CCW,
        TransformIcon::RotateCw => qs_icons::ROTATE_CW,
        TransformIcon::FlipH => qs_icons::FLIP_HORIZONTAL_2,
        TransformIcon::FlipV => qs_icons::FLIP_VERTICAL_2,
    };
    crate::icons::draw_paths(painter, c, paths, color);
}

/// Single-row segmented vector-icon bar (`32px` height, concentric `radius::MD` / `radius::SM`).
/// `.seg` — shallow well track with gap-separated segments, no internal
/// dividers (mockup Transform bar).
pub fn segmented_icon_bar(ui: &mut egui::Ui, items: &[(TransformIcon, &str)]) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    let h = sp(space::XL); // 24px — the mockup's low four-button bar
    let total_w = ui.available_width();
    let (track_rect, _) = ui.allocate_exact_size(Vec2::new(total_w, h), Sense::hover());
    let painter = ui.painter();
    theme::paint_segmented_track(painter, track_rect, egui::CornerRadius::same(6));

    let pad = sp(space::HAIR); // 2px
    let inner = track_rect.shrink(pad);
    let seg_w = inner.width() / items.len() as f32;
    let mut clicked = None;

    for (i, (icon, tip)) in items.iter().enumerate() {
        let seg_rect = Rect::from_min_size(
            Pos2::new(inner.left() + i as f32 * seg_w, inner.top()),
            Vec2::new(seg_w - pad, inner.height()),
        );
        let resp = ui.interact(
            seg_rect,
            ui.scope_id().with(("seg_icon", i)),
            Sense::click(),
        );
        let hovered = resp.hovered();
        if hovered {
            painter.rect_filled(seg_rect, rad(radius::XS), theme::bg_card());
            painter.rect_stroke(
                seg_rect,
                rad(radius::XS),
                Stroke::new(0.75, theme::border_hot()),
                StrokeKind::Inside,
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
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

/// Status bar pill — 20px capsule, `radius::PILL`. Active state is the quiet
/// `--control` fill of the mockup, not accent.
/// Size of a status pill for `label`; the single place its height is decided.
pub fn status_pill_size(ui: &egui::Ui, label: &str) -> Vec2 {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(fs(font::SMALL)),
        theme::text_primary(),
    );
    Vec2::new(galley.size().x + 20.0, 20.0)
}

pub fn status_pill(ui: &mut egui::Ui, label: &str, active: bool, tip: &str) -> bool {
    let size = status_pill_size(ui, label);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered();
    let ht = theme::hot_t(&resp);
    let painter = ui.painter();
    let cr = rad(radius::PILL);

    if active || ht > 0.0 {
        let hov = theme::bg_btn_secondary_top();
        let a = if active { hov.a() as f32 } else { hov.a() as f32 * ht };
        painter.rect_filled(
            rect,
            cr,
            Color32::from_rgba_unmultiplied(hov.r(), hov.g(), hov.b(), a as u8),
        );
    }

    let fg = if active {
        theme::text_primary()
    } else {
        cmix(theme::text_secondary(), theme::text_primary(), ht)
    };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(fs(font::CAPTION) + 0.5),
        fg,
    );
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let resp = if tip.is_empty() {
        resp
    } else {
        resp.on_hover_text(platform_shortcut(tip))
    };
    resp.clicked()
}

/// Empty state following `baseline-ui` ("MUST give empty states one clear next action").
/// Returns `true` when the primary next-action button is clicked.
/// Right-aligned row of dialog actions with an explicit height.
///
/// A bare `with_layout(right_to_left)` claims all remaining height, which made
/// short dialogs stretch to a few hundred pixels of empty space.
pub fn dialog_actions(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let w = ui.available_width();
    ui.allocate_ui_with_layout(
        egui::vec2(w, CONTROL_H),
        egui::Layout::right_to_left(egui::Align::Center),
        add,
    );
}

/// Compact action button: filled pill with a hairline, hover lightens it.
///
/// `status_pill` is a *toggle* — inactive it draws text only, which is wrong
/// for an action. Actions use this instead.
/// One control height for row-level actions (chips, Library buttons, Open).
pub const CONTROL_H: f32 = 26.0;

/// Size of an action pill; the single place its height is decided.
pub fn pill_size(ui: &egui::Ui, label: &str) -> Vec2 {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(fs(font::SMALL)),
        theme::text_primary(),
    );
    Vec2::new(galley.size().x + sp(space::LG), CONTROL_H)
}

pub fn pill_button(ui: &mut egui::Ui, label: &str, tip: &str) -> bool {
    pill_button_colored(ui, label, tip, theme::text_primary())
}

/// Accent-filled pill for the one primary action of a dialog.
pub fn pill_button_primary(ui: &mut egui::Ui, label: &str, tip: &str) -> bool {
    let size = pill_size(ui, label);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    theme::paint_pill_button(
        ui.painter(),
        rect,
        rad(radius::SM),
        false,
        theme::hot_t(&resp),
        true,
        true,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(fs(font::SMALL)),
        theme::on_accent(),
    );
    if tip.is_empty() {
        resp.clicked()
    } else {
        resp.on_hover_text(platform_shortcut(tip)).clicked()
    }
}

/// Same pill with a custom label colour, for destructive actions.
pub fn pill_button_colored(ui: &mut egui::Ui, label: &str, tip: &str, color: Color32) -> bool {
    let size = pill_size(ui, label);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    let cr = rad(radius::SM);
    painter.rect_filled(
        rect,
        cr,
        cmix(
            theme::bg_btn_secondary_top(),
            theme::bg_btn_secondary_hover_top(),
            theme::hot_t(&resp),
        ),
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
        resp.on_hover_text(platform_shortcut(tip)).clicked()
    }
}

/// Subtle inset container for an expanded effect's sub-controls so nested
/// sliders/pickers visually belong to their parent switch rather than bleeding
/// into the next toggle row.
/// Sub-group separator inside a card: a hairline bled to the card's edge —
/// not a bordered well. Card-on-card depth was the flagged anti-pattern;
/// structure comes from tone and whitespace, never nested boxes.
pub fn effect_group(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(sp(space::XS));
    let hair = ui.available_rect_before_wrap();
    let bleed = sp(space::MD);
    ui.painter().line_segment(
        [
            Pos2::new(hair.left() - bleed, hair.top() + 0.5),
            Pos2::new(hair.right() + bleed, hair.top() + 0.5),
        ],
        Stroke::new(1.0, theme::border_subtle()),
    );
    ui.add_space(sp(space::XS));
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = sp(space::XS);
        add_contents(ui);
    });
    ui.add_space(sp(space::XS));
}

/// Consistent inspector row: left-aligned label (matching `switch_row` inset)
/// and right-aligned control.
pub fn prop_row<R>(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    add_right: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.horizontal(|ui| {
        ui.set_min_height(CONTROL_H);
        ui.add_space(sp(space::XS));
        ui.label(
            egui::RichText::new(label)
                .size(fs(font::SMALL) + 0.5)
                .color(if active {
                    theme::text_primary()
                } else {
                    theme::text_secondary()
                }),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(sp(space::XS));
            add_right(ui)
        })
        .inner
    })
    .inner
}

/// Sleek Apple/Linear-style slider row: label on left, click-to-edit formatted
/// numeric value on right, thin 4px rounded track with accent fill and clean
/// circular knob. Hold Shift while dragging for fine precision.
pub fn slider_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    decimals: usize,
) -> bool {
    let min = *range.start();
    let max = *range.end();
    let span = (max - min).max(f32::EPSILON);
    let formatted = if decimals == 0 {
        format!("{:.0}{suffix}", *value)
    } else {
        format!("{:.prec$}{suffix}", *value, prec = decimals)
    };
    let edit_id = ui.auto_id_with(("slider_edit", label));
    let mut editing: Option<String> = ui.data_mut(|d| d.get_temp(edit_id));
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.spacing_mut().interact_size.y = 18.0;
        ui.allocate_ui(Vec2::new(58.0, 18.0), |ui| {
            ui.set_min_width(58.0);
            ui.label(egui::RichText::new(label)
                .size(fs(font::SMALL))
                .color(theme::text_secondary()));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(mut buf) = editing.take() {
                let te = ui.add(
                    egui::TextEdit::singleline(&mut buf)
                        .desired_width(54.0)
                        .font(egui::FontId::monospace(fs(font::SMALL))),
                );
                let enter = te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if enter || (te.lost_focus() && !esc) {
                    let clean = buf.trim().trim_end_matches(suffix.trim()).trim();
                    if let Some(parsed) = clean.parse::<f32>().ok().filter(|value| value.is_finite()) {
                        let clamped = parsed.clamp(min, max);
                        if (clamped - *value).abs() > f32::EPSILON {
                            *value = clamped;
                            changed = true;
                        }
                    }
                    ui.data_mut(|d| d.remove::<String>(edit_id));
                } else if esc {
                    ui.data_mut(|d| d.remove::<String>(edit_id));
                } else {
                    if !te.has_focus() {
                        te.request_focus();
                    }
                    ui.data_mut(|d| d.insert_temp(edit_id, buf));
                }
            } else {
                let val_resp = ui
                    .add_sized(
                        Vec2::new(42.0, 18.0),
                        egui::Label::new(
                            egui::RichText::new(&formatted)
                                .size(fs(font::SMALL))
                                .monospace()
                                .color(theme::text_primary()),
                        )
                        .halign(egui::Align::Max)
                        .sense(Sense::click()),
                    )
                    .on_hover_text("Click to type a value");
                if val_resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                }
                if val_resp.clicked() {
                    let raw = if decimals == 0 {
                        format!("{:.0}", *value)
                    } else {
                        format!("{:.prec$}", *value, prec = decimals)
                    };
                    ui.data_mut(|d| d.insert_temp(edit_id, raw));
                }
            }

            let width = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::click_and_drag());
            let knob_r: f32 = if resp.dragged() {
                7.0
            } else if resp.hovered() {
                6.5
            } else {
                6.0
            };
            let pad = 7.0;
            let track_left = rect.left() + pad;
            let track_right = (rect.right() - pad).max(track_left + 1.0);
            let track_w = track_right - track_left;

            if resp.dragged() {
                let fine = ui.input(|i| i.modifiers.shift);
                if fine {
                    let dx = resp.drag_delta().x;
                    if dx.abs() > 0.0 {
                        let next = (*value + (dx / track_w) * span * 0.25).clamp(min, max);
                        let next = if decimals == 0 { next.round() } else { next };
                        if (next - *value).abs() > f32::EPSILON {
                            *value = next;
                            changed = true;
                        }
                    }
                } else if let Some(pos) = resp.interact_pointer_pos() {
                    let t = ((pos.x - track_left) / track_w).clamp(0.0, 1.0);
                    let next = min + t * span;
                    let next = if decimals == 0 { next.round() } else { next };
                    if (next - *value).abs() > f32::EPSILON {
                        *value = next;
                        changed = true;
                    }
                }
            } else if resp.clicked() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let t = ((pos.x - track_left) / track_w).clamp(0.0, 1.0);
                    let next = min + t * span;
                    let next = if decimals == 0 { next.round() } else { next };
                    if (next - *value).abs() > f32::EPSILON {
                        *value = next;
                        changed = true;
                    }
                }
            }

            let t = ((*value - min) / span).clamp(0.0, 1.0);
            let cy = rect.center().y;
            let track_h = 4.0;
            let track_rect = Rect::from_min_max(
                Pos2::new(track_left, cy - track_h * 0.5),
                Pos2::new(track_right, cy + track_h * 0.5),
            );
            let painter = ui.painter();
            let track_cr = rad(radius::PILL);
            painter.rect_filled(track_rect, track_cr, theme::bg_pill_track());
            if t > 0.0 {
                let fill_rect = Rect::from_min_max(
                    track_rect.min,
                    Pos2::new(track_left + track_w * t, track_rect.max.y),
                );
                painter.rect_filled(fill_rect, track_cr, theme::accent_blue());
            }

            let knob_center = Pos2::new(track_left + track_w * t, cy);
            // Soft drop shadow + crisp white knob (no harsh dark ring).
            painter.circle_filled(
                knob_center + Vec2::new(0.0, 1.0),
                knob_r + 0.5,
                Color32::from_black_alpha(if qs_theme::pal().dark { 96 } else { 36 }),
            );
            painter.circle_filled(knob_center, knob_r, Color32::WHITE);
            painter.circle_stroke(
                knob_center,
                knob_r,
                Stroke::new(
                    0.75,
                    if resp.hovered() || resp.dragged() {
                        theme::accent_blue()
                    } else {
                        Color32::from_black_alpha(if qs_theme::pal().dark { 60 } else { 32 })
                    },
                ),
            );

            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        });
    });
    changed
}

/// Compact selectable chip for multi-toggle options (e.g. Torn edge sides).
pub fn toggle_chip(ui: &mut egui::Ui, on: &mut bool, label: &str) -> bool {
    let size = Vec2::new(
        ui.painter()
            .layout_no_wrap(
                label.to_owned(),
                egui::FontId::proportional(fs(font::CAPTION) + 0.5),
                theme::text_primary(),
            )
            .size()
            .x
            + 14.0,
        22.0,
    );
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let mut changed = false;
    if resp.clicked() {
        *on = !*on;
        changed = true;
    }
    let painter = ui.painter();
    let cr = rad(radius::SM);
    let (fill, border, fg) = if *on {
        (
            theme::accent_alpha(if qs_theme::pal().dark { 56 } else { 28 }),
            theme::accent_alpha(if qs_theme::pal().dark { 140 } else { 90 }),
            if qs_theme::pal().dark {
                theme::accent_blue_soft()
            } else {
                theme::accent_blue()
            },
        )
    } else if resp.hovered() {
        (
            theme::bg_pill_hover(),
            theme::border_hot(),
            theme::text_primary(),
        )
    } else {
        (
            theme::bg_card(),
            theme::border_subtle(),
            theme::text_secondary(),
        )
    };
    painter.rect_filled(rect, cr, fill);
    painter.rect_stroke(rect, cr, Stroke::new(1.0, border), StrokeKind::Inside);
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(fs(font::CAPTION) + 0.5),
        fg,
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    changed
}

/// Translucent full-viewport scrim painted behind modal alerts/dialogs.
pub fn modal_backdrop(ctx: &egui::Context, id: &str) {
    let rect = ctx.content_rect();
    egui::Area::new(egui::Id::unique(id))
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .show(ctx, |ui| {
            let (_r, _resp) = ui.allocate_exact_size(rect.size(), Sense::click());
            ui.painter().rect_filled(
                rect,
                CornerRadius::ZERO,
                Color32::from_black_alpha(if qs_theme::pal().dark { 120 } else { 52 }),
            );
        });
}

/// Borderless, centered macOS-style alert/modal container with custom title,
/// body copy, and bottom action row.
pub fn modal_window(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    min_width: f32,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    modal_backdrop(ctx, &format!("{id}_scrim"));
    egui::Window::new(title)
        .id(egui::Id::unique(id))
        .title_bar(false)
        .order(egui::Order::Foreground)
        .frame(theme::modal_frame())
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_min_width(min_width);
            ui.label(
                egui::RichText::new(title)
                    .size(fs(font::HEADING) - 1.0)
                    .strong()
                    .color(theme::text_primary()),
            );
            ui.add_space(sp(space::SM));
            add_contents(ui);
        });
}

/// Consistent popover menu row (`28px` height, rounded hover pill, left label,
/// optional active accent dot, right-aligned shortcut hint).
pub fn menu_item(
    ui: &mut egui::Ui,
    label: &str,
    shortcut: Option<&str>,
    active: bool,
    enabled: bool,
    danger: bool,
) -> bool {
    menu_row(ui, None, label, shortcut, active, enabled, danger)
}

pub(crate) fn menu_item_with_icon(
    ui: &mut egui::Ui,
    icon: &[qs_icons::StrokePath],
    label: &str,
    shortcut: Option<&str>,
    enabled: bool,
    danger: bool,
) -> bool {
    menu_row(ui, Some(icon), label, shortcut, false, enabled, danger)
}

fn menu_row(
    ui: &mut egui::Ui,
    icon: Option<&[qs_icons::StrokePath]>,
    label: &str,
    shortcut: Option<&str>,
    active: bool,
    enabled: bool,
    danger: bool,
) -> bool {
    let width = ui.available_width().max(160.0);
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(width, 28.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hovered = enabled && resp.hovered();
    let ht = ui.ctx().animate_bool_with_time(resp.id, hovered, 0.12);
    let painter = ui.painter();
    let cr = rad(radius::SM);

    if ht > 0.0 {
        let fill = if danger {
            Color32::from_rgba_unmultiplied(
                theme::danger_red().r(),
                theme::danger_red().g(),
                theme::danger_red().b(),
                (if qs_theme::pal().dark { 48.0 } else { 28.0 } * ht) as u8,
            )
        } else {
            let hov = theme::bg_pill_hover();
            Color32::from_rgba_unmultiplied(hov.r(), hov.g(), hov.b(), (hov.a() as f32 * ht) as u8)
        };
        painter.rect_filled(rect, cr, fill);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let text_x = if let Some(icon) = icon {
        crate::icons::draw_paths(
            painter,
            Pos2::new(rect.left() + 16.5, rect.center().y),
            icon,
            if enabled { theme::text_secondary() } else { theme::text_disabled() },
        );
        rect.left() + 32.0
    } else {
        rect.left() + 10.0
    };
    if active {
        painter.circle_filled(
            Pos2::new(rect.left() + 5.0, rect.center().y),
            2.5,
            theme::accent_blue(),
        );
    }

    let fg = if !enabled {
        theme::text_disabled()
    } else if danger {
        theme::danger_red()
    } else if active {
        theme::accent_blue()
    } else {
        theme::text_primary()
    };

    painter.text(
        Pos2::new(if active { text_x + 4.0 } else { text_x }, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(fs(font::SMALL) + 0.5),
        fg,
    );

    if let Some(sc) = shortcut {
        painter.text(
            Pos2::new(rect.right() - 10.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            platform_shortcut(sc),
            egui::FontId::proportional(fs(font::CAPTION) + 0.5),
            if enabled {
                theme::text_tertiary()
            } else {
                theme::text_disabled()
            },
        );
    }

    enabled && resp.clicked()
}

/// Selectable option card used inside dialogs (replaces raw egui radio buttons).
pub fn radio_card(ui: &mut egui::Ui, selected: bool, label: &str) -> bool {
    let width = ui.available_width().max(180.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click());
    let hovered = resp.hovered();
    let painter = ui.painter();
    let cr = rad(radius::MD);

    let (fill, border) = if selected {
        (
            theme::accent_alpha(if qs_theme::pal().dark { 44 } else { 24 }),
            theme::accent_blue(),
        )
    } else if hovered {
        (theme::bg_pill_hover(), theme::border_hot())
    } else {
        (theme::bg_inspector_top(), theme::border_subtle())
    };
    painter.rect_filled(rect, cr, fill);
    painter.rect_stroke(rect, cr, Stroke::new(1.0, border), StrokeKind::Inside);

    let dot_c = Pos2::new(rect.left() + 15.0, rect.center().y);
    if selected {
        painter.circle_filled(dot_c, 5.5, theme::accent_blue());
        painter.circle_filled(dot_c, 2.2, Color32::WHITE);
    } else {
        painter.circle_stroke(dot_c, 5.0, Stroke::new(1.25, theme::border_hot()));
    }

    painter.text(
        Pos2::new(rect.left() + 30.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(fs(font::SMALL) + 0.5),
        if selected {
            theme::text_primary()
        } else {
            theme::text_secondary()
        },
    );
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
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
