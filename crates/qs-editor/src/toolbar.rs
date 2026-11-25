//! Unified macOS-style header toolbar and contextual inspector bar.

use crate::{
    icons::{self, ActionIcon},
    theme::*,
};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use qs_doc::{
    Align, ArrowHead, BlurMode, CalloutShape, Fill, ObjectKind, PALETTE, SIZES, TEXT_SIZES,
};
use qs_edit::{Editor, Tool, ZOrder};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ZoomCmd {
    #[default]
    None,
    Out,
    In,
    Fit,
    /// Set zoom directly to a specific scale factor (e.g. 1.0 = 100%).
    Set(f32),
}

#[derive(Default)]
pub struct TopBarResult {
    pub mutated: bool,
    pub copy: bool,
    pub save: bool,
    pub save_as: bool,
    pub open: bool,
    pub export: bool,
    /// Where the Export button sits, so its dropdown can hang below it.
    pub export_rect: Option<Rect>,
    pub canvas: bool,
    /// The app menu (New capture, Library, Settings, theme) was clicked.
    pub menu: bool,
    pub menu_rect: Option<Rect>,
    pub close: bool,
    pub zoom: ZoomCmd,
}

pub fn rgb_to_color32(rgb: u32) -> Color32 {
    Color32::from_rgb(
        ((rgb >> 16) & 255) as u8,
        ((rgb >> 8) & 255) as u8,
        (rgb & 255) as u8,
    )
}

// ---------------------------------------------------------------------------
// Tier 1: Unified Header Toolbar (46px)
// ---------------------------------------------------------------------------

/// Shared control height so the width capsule matches chips and buttons.
const CONTROL_H: f32 = crate::components::CONTROL_H;
/// Colour swatch geometry: the swatch is a control, its step adds a hairline.
const SWATCH: f32 = 20.0;
const SPACE_XS: f32 = qs_theme::space::XS as f32;
const SPACE_SM: f32 = qs_theme::space::SM as f32;
/// Inner padding of a segmented track, matching the tool capsule.
const TRACK_PAD: f32 = 3.0;

pub fn render_top_bar(
    ui: &mut Ui,
    editor: &mut Editor,
    zoom: f32,
    doc_path: Option<&Path>,
) -> TopBarResult {
    let mut res = TopBarResult::default();
    let avail = ui.available_rect_before_wrap();
    let bar_rect = Rect::from_min_size(avail.min, Vec2::new(avail.width(), TOOLBAR_HEIGHT));
    ui.allocate_rect(bar_rect, Sense::hover());

    paint_chrome_bar(
        ui.painter(),
        bar_rect,
        bg_toolbar_top(),
        bg_toolbar_bot(),
        true,
    );

    // macOS: the docbar spans the transparent titlebar — leave room for the
    // floating traffic lights exactly like the mockup's `padding-left: 78px`.
    let left_inset = if cfg!(target_os = "macos") {
        78.0
    } else {
        CHROME_PAD_X
    };
    let inner = Rect::from_min_max(
        Pos2::new(bar_rect.left() + left_inset, bar_rect.top()),
        Pos2::new(bar_rect.right() - CHROME_PAD_X, bar_rect.bottom()),
    );
    let cy = inner.center().y;
    /// `.ibtn` — square transparent chrome button.
    const BTN: f32 = 28.0;
    /// `.docbar` is a flex row with `gap:10px` — every child (button or sep)
    /// is followed by this trailing gap.
    const GAP: f32 = 10.0;

    // --- Left: menu · open · save-as │ undo · redo │ doctab ---
    let mut x = inner.left();
    let take = |w: f32, x: &mut f32| {
        let r = Rect::from_min_size(Pos2::new(*x, cy - w * 0.5), Vec2::new(w, w));
        *x += w + GAP;
        r
    };
    // `.sep` — 1px hairline occupying its own flex slot.
    let sep = |painter: &egui::Painter, x: &mut f32| {
        let px = (*x + 0.5).round() + 0.5;
        painter.line_segment(
            [Pos2::new(px, cy - 9.0), Pos2::new(px, cy + 9.0)],
            Stroke::new(1.0, divider_vertical()),
        );
        *x += 1.0 + GAP;
    };
    let menu_rect = take(BTN, &mut x);
    res.menu_rect = Some(menu_rect);
    if bar_ibtn(
        ui,
        "tb_menu",
        menu_rect,
        ActionIcon::Menu,
        true,
        "New capture, Library, Settings, theme",
    ) {
        res.menu = true;
    }
    if bar_ibtn(
        ui,
        "tb_open",
        take(BTN, &mut x),
        ActionIcon::Open,
        true,
        "Open… (Ctrl+O)",
    ) {
        res.open = true;
    }
    if bar_ibtn(
        ui,
        "tb_save_as",
        take(BTN, &mut x),
        ActionIcon::SaveAs,
        true,
        "Save As… (Ctrl+Shift+S)",
    ) {
        res.save_as = true;
    }
    sep(ui.painter(), &mut x);

    if bar_ibtn(
        ui,
        "tb_undo",
        take(BTN, &mut x),
        ActionIcon::Undo,
        editor.can_undo(),
        "Undo (Ctrl+Z)",
    ) && editor.undo()
    {
        res.mutated = true;
    }
    if bar_ibtn(
        ui,
        "tb_redo",
        take(BTN, &mut x),
        ActionIcon::Redo,
        editor.can_redo(),
        "Redo (Ctrl+Shift+Z)",
    ) && editor.redo()
    {
        res.mutated = true;
    }
    sep(ui.painter(), &mut x);

    // doctab: document icon + file name, quiet — the mockup's identity chip.
    // Painted only after the right cluster's extent is known so it never
    // runs under the buttons on narrow windows.
    let doc_name = doc_path
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_owned();
    let left_end = x;

    // --- Right: zoom − pct + │ Save · Export · Copy · × ---
    // Widths are measured off the right edge so the cluster hugs the margin.
    let mut rx = inner.right();

    let close_rect = {
        rx -= BTN;
        Rect::from_min_size(Pos2::new(rx, cy - BTN * 0.5), Vec2::splat(BTN))
    };
    rx -= GAP;

    // `.tbtn` widths: 11px padding + 13px icon + 6px gap + label.
    let label_font = FontId::proportional(qs_theme::font::SMALL as f32);
    let tbtn_w = |label: &str| -> f32 {
        let g = ui
            .painter()
            .layout_no_wrap(label.to_owned(), label_font.clone(), text_primary());
        11.0 + 13.0 + 6.0 + g.size().x + 11.0
    };
    let compact_actions = bar_rect.width() < 1080.0;
    let copy_w = if compact_actions { BTN } else { tbtn_w("Copy") };
    let export_w = if compact_actions { BTN } else { tbtn_w("Export") };
    let save_w = if compact_actions { BTN } else { tbtn_w("Save") };

    let copy_rect = {
        rx -= copy_w;
        Rect::from_min_size(Pos2::new(rx, cy - BTN * 0.5), Vec2::new(copy_w, BTN))
    };
    rx -= GAP;
    let export_rect = {
        rx -= export_w;
        Rect::from_min_size(Pos2::new(rx, cy - BTN * 0.5), Vec2::new(export_w, BTN))
    };
    rx -= GAP;
    let save_rect = {
        rx -= save_w;
        Rect::from_min_size(Pos2::new(rx, cy - BTN * 0.5), Vec2::new(save_w, BTN))
    };

    res.export_rect = Some(export_rect);

    // Zoom cluster: flat `− % +`, pct click-to-type / double-click Fit.
    let zoom_pct = format!("{:.0}%", zoom * 100.0);
    let pct_w = 44.0;
    let show_zoom = bar_rect.width() >= 620.0;
    let zoom_right = if show_zoom {
        // `.sep` child sits between zoom and Save in the mockup — in
        // right-to-left layout it comes before the zoom cluster.
        rx -= GAP;
        let sep_x = rx;
        rx -= 1.0 + GAP;
        let mut zx = rx;
        let z_in_rect = {
            zx -= BTN;
            Rect::from_min_size(Pos2::new(zx, cy - BTN * 0.5), Vec2::splat(BTN))
        };
        let z_mid_rect = {
            zx -= pct_w;
            Rect::from_min_size(Pos2::new(zx, cy - BTN * 0.5), Vec2::new(pct_w, BTN))
        };
        let z_out_rect = {
            zx -= BTN;
            Rect::from_min_size(Pos2::new(zx, cy - BTN * 0.5), Vec2::splat(BTN))
        };
        rx = zx;

        let px = (sep_x - 0.5).round() + 0.5;
        ui.painter().line_segment(
            [Pos2::new(px, cy - 9.0), Pos2::new(px, cy + 9.0)],
            Stroke::new(1.0, divider_vertical()),
        );

        if bar_text_btn(ui, "tb_zout", z_out_rect, "−", "Zoom Out") {
            res.zoom = ZoomCmd::Out;
        }

        let z_edit_id = ui.auto_id_with("tb_zedit");
        let z_editing: Option<String> = ui.data_mut(|d| d.get_temp(z_edit_id));
        if let Some(mut buf) = z_editing {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(z_mid_rect));
            let te = child.add(
                egui::TextEdit::singleline(&mut buf)
                    .desired_width(z_mid_rect.width())
                    .align(egui::Align2::CENTER_CENTER)
                    .font(FontId::monospace(qs_theme::font::SMALL as f32)),
            );
            let enter = te.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if enter || (te.lost_focus() && !esc) {
                let clean = buf.trim().trim_end_matches('%').trim();
                if let Ok(pct) = clean.parse::<f32>() {
                    if pct.is_finite() && pct > 0.0 {
                        res.zoom = ZoomCmd::Set((pct / 100.0).clamp(0.1, 16.0));
                    }
                }
                ui.data_mut(|d| d.remove::<String>(z_edit_id));
            } else if esc {
                ui.data_mut(|d| d.remove::<String>(z_edit_id));
            } else {
                if !te.has_focus() {
                    te.request_focus();
                }
                ui.data_mut(|d| d.insert_temp(z_edit_id, buf));
            }
        } else {
            let z_mid_resp = ui
                .interact(z_mid_rect, ui.auto_id_with("tb_zfit"), Sense::click())
                .on_hover_text("Click to type zoom % · Double-click to Fit (Ctrl+0)");
            if z_mid_resp.hovered() {
                ui.painter().rect_filled(z_mid_rect, 4.0, bg_pill_hover());
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            }
            ui.painter().text(
                z_mid_rect.center(),
                egui::Align2::CENTER_CENTER,
                &zoom_pct,
                FontId::proportional(qs_theme::font::SMALL as f32),
                if z_mid_resp.hovered() {
                    text_primary()
                } else {
                    text_secondary()
                },
            );
            if z_mid_resp.double_clicked() {
                res.zoom = ZoomCmd::Fit;
            } else if z_mid_resp.clicked() {
                ui.data_mut(|d| d.insert_temp(z_edit_id, format!("{:.0}", zoom * 100.0)));
            }
        }

        if bar_text_btn(ui, "tb_zin", z_in_rect, "+", "Zoom In") {
            res.zoom = ZoomCmd::In;
        }
        rx
    } else {
        save_rect.left()
    };
    let right_start = zoom_right;

    if compact_actions {
        if bar_ibtn(ui, "tb_save", save_rect, ActionIcon::Save, true, "Save .qsdoc (Ctrl+S)") {
            res.save = true;
        }
        if bar_ibtn(
            ui,
            "tb_export",
            export_rect,
            ActionIcon::Export,
            true,
            "Export image (Ctrl+Shift+E)",
        ) {
            res.export = true;
        }
        if bar_ibtn(
            ui,
            "tb_copy",
            copy_rect,
            ActionIcon::Copy,
            true,
            "Copy Flattened Image (Ctrl+C)",
        ) {
            res.copy = true;
        }
    } else {
        if tbtn(
            ui,
            "tb_save",
            save_rect,
            ActionIcon::Save,
            "Save",
            false,
            "Save .qsdoc (Ctrl+S)",
        ) {
            res.save = true;
        }
        if tbtn(
            ui,
            "tb_export",
            export_rect,
            ActionIcon::Export,
            "Export",
            false,
            "Export image (Ctrl+Shift+E)",
        ) {
            res.export = true;
        }
        if tbtn(
            ui,
            "tb_copy",
            copy_rect,
            ActionIcon::Copy,
            "Copy",
            true,
            "Copy Flattened Image (Ctrl+C)",
        ) {
            res.copy = true;
        }
    }

    if bar_ibtn(
        ui,
        "tb_close",
        close_rect,
        ActionIcon::Close,
        true,
        "Close (Ctrl+W)",
    ) {
        res.close = true;
    }

    // doctab in the leftover gap, truncated to what actually fits.
    let gap = right_start - left_end;
    if gap >= 60.0 {
        icons::draw_action_icon(
            ui.painter(),
            Pos2::new(left_end + 6.5, cy),
            ActionIcon::DocTab,
            text_secondary(),
        );
        let max_w = gap - 24.0;
        let font = FontId::proportional(qs_theme::font::SMALL as f32);
        let mut disp = doc_name;
        let mut g = ui
            .painter()
            .layout_no_wrap(disp.clone(), font.clone(), text_secondary());
        if g.size().x > max_w {
            let max_chars = (max_w / 6.5).floor().max(6.0) as usize;
            disp.truncate(max_chars.saturating_sub(1));
            disp.push('…');
            g = ui
                .painter()
                .layout_no_wrap(disp.clone(), font.clone(), text_secondary());
        }
        ui.painter().galley(
            Pos2::new(left_end + 20.0, cy - g.size().y * 0.5),
            g,
            text_secondary(),
        );
    }

    res
}

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

/// Segmented-track button used by the properties bar swatch/stroke segments.
fn icon_segment(
    ui: &mut Ui,
    id_salt: &str,
    rect: Rect,
    icon: ActionIcon,
    active: bool,
    enabled: bool,
    tooltip: &str,
) -> bool {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let id = ui.auto_id_with(id_salt);
    let resp = ui.interact(rect, id, sense).on_hover_text(crate::components::platform_shortcut(tooltip));
    paint_pill_button(
        ui.painter(),
        rect,
        5.5,
        active,
        hot_t(&resp),
        enabled,
        false,
    );
    let color = if !enabled {
        text_disabled()
    } else if active || resp.hovered() {
        text_primary()
    } else {
        text_secondary()
    };
    icons::draw_action_icon(ui.painter(), rect.center(), icon, color);
    enabled && resp.clicked()
}

/// `.ibtn` — transparent chrome button resting on the bar; hover paints the
/// `--control` surface, disabled dims to 35 % like the mockup.
fn bar_ibtn(
    ui: &mut Ui,
    id_salt: &str,
    rect: Rect,
    icon: ActionIcon,
    enabled: bool,
    tooltip: &str,
) -> bool {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let resp = ui
        .interact(rect, ui.auto_id_with(id_salt), sense)
        .on_hover_text(crate::components::platform_shortcut(tooltip));
    let ht = hot_t(&resp);
    if ht > 0.0 && enabled {
        let hov = bg_btn_secondary_top();
        ui.painter().rect_filled(
            rect,
            6.0,
            Color32::from_rgba_unmultiplied(hov.r(), hov.g(), hov.b(), (hov.a() as f32 * ht) as u8),
        );
    }
    let color = if !enabled {
        text_disabled()
    } else if resp.hovered() {
        text_primary()
    } else {
        text_secondary()
    };
    icons::draw_action_icon_sz(ui.painter(), rect.center(), icon, color, 15.0);
    enabled && resp.clicked()
}

/// Same flat chrome as `bar_ibtn` but paints a text glyph (`−`, `+`).
fn bar_text_btn(ui: &mut Ui, id_salt: &str, rect: Rect, glyph: &str, tooltip: &str) -> bool {
    let resp = ui
        .interact(rect, ui.auto_id_with(id_salt), Sense::click())
        .on_hover_text(crate::components::platform_shortcut(tooltip));
    let ht = hot_t(&resp);
    if ht > 0.0 {
        let hov = bg_btn_secondary_top();
        ui.painter().rect_filled(
            rect,
            6.0,
            Color32::from_rgba_unmultiplied(
                hov.r(),
                hov.g(),
                hov.b(),
                (hov.a() as f32 * ht) as u8,
            ),
        );
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        FontId::proportional(qs_theme::font::BODY as f32),
        if resp.hovered() {
            text_primary()
        } else {
            text_secondary()
        },
    );
    resp.clicked()
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

/// `.tbtn` — bordered text+icon button (`--control`); `primary` is the one
/// accent CTA per row (Copy).
fn tbtn(
    ui: &mut Ui,
    id_salt: &str,
    rect: Rect,
    icon: ActionIcon,
    label: &str,
    primary_accent: bool,
    tooltip: &str,
) -> bool {
    let resp = ui
        .interact(rect, ui.auto_id_with(id_salt), Sense::click())
        .on_hover_text(crate::components::platform_shortcut(tooltip));
    let ht = hot_t(&resp);
    if primary_accent {
        paint_pill_button(ui.painter(), rect, 6.0, false, ht, true, true);
    } else {
        paint_secondary_button(ui.painter(), rect, 6.0, ht, true, false);
    }
    let fg = if primary_accent {
        Color32::WHITE
    } else {
        text_primary()
    };
    icons::draw_action_icon(
        ui.painter(),
        Pos2::new(rect.left() + 11.0 + 6.5, rect.center().y),
        icon,
        fg,
    );
    ui.painter().text(
        Pos2::new(rect.left() + 11.0 + 13.0 + 6.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(qs_theme::font::SMALL as f32),
        fg,
    );
    resp.clicked()
}

// ---------------------------------------------------------------------------
// Tier 2: Contextual Inspector Bar (38px)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContextKind {
    Generic,
    StrokeOnly,
    Shape,
    Arrow,
    Text,
    Callout,
    Step,
    Blur,
    Magnify,
    Spotlight,
    Stamp,
    Eraser,
}

pub fn render_properties_bar(ui: &mut Ui, editor: &mut Editor) -> bool {
    let mut mutated = false;
    let avail = ui.available_rect_before_wrap();
    let bar_rect = Rect::from_min_size(avail.min, Vec2::new(avail.width(), INSPECTOR_HEIGHT));
    ui.allocate_rect(bar_rect, Sense::hover());

    // One step darker than the toolbar above (`--bg`) — keeps the rows
    // visually separated like the mockup's `.proprow`.
    paint_chrome_bar(
        ui.painter(),
        bar_rect,
        bg_props(),
        bg_props(),
        false,
    );

    let inner = bar_rect.shrink2(Vec2::new(CHROME_PAD_X, 0.0));
    let cy = inner.center().y;

    // Resolve effective properties from selected object OR active creation tool.
    let sel_kind = editor.selected_object().map(|o| o.kind.clone());
    let has_selection = sel_kind.is_some();

    let (
        ctx_kind,
        eff_rgb,
        eff_width,
        eff_fill,
        eff_callout,
        eff_arrow,
        eff_blur_mode,
        eff_blur_str,
        eff_text,
    ) = match &sel_kind {
        Some(ObjectKind::Pen { stroke, .. }) | Some(ObjectKind::Highlighter { stroke, .. }) => {
            let w = if matches!(sel_kind, Some(ObjectKind::Highlighter { .. })) {
                (stroke.width / 2.6).round()
            } else {
                stroke.width
            };
            (
                ContextKind::StrokeOnly,
                stroke.rgb,
                w,
                editor.style.fill,
                editor.style.callout_shape,
                editor.style.arrow_head,
                editor.style.blur_mode,
                editor.style.blur_strength,
                editor.style.text,
            )
        }
        Some(ObjectKind::Line { stroke, .. }) => (
            ContextKind::StrokeOnly,
            stroke.rgb,
            stroke.width,
            editor.style.fill,
            editor.style.callout_shape,
            editor.style.arrow_head,
            editor.style.blur_mode,
            editor.style.blur_strength,
            editor.style.text,
        ),
        Some(ObjectKind::Arrow { stroke, head, .. }) => (
            ContextKind::Arrow,
            stroke.rgb,
            stroke.width,
            editor.style.fill,
            editor.style.callout_shape,
            *head,
            editor.style.blur_mode,
            editor.style.blur_strength,
            editor.style.text,
        ),
        Some(ObjectKind::Rect { stroke, fill, .. })
        | Some(ObjectKind::Ellipse { stroke, fill, .. })
        | Some(ObjectKind::Polygon { stroke, fill, .. }) => (
            ContextKind::Shape,
            stroke.rgb,
            stroke.width,
            *fill,
            editor.style.callout_shape,
            editor.style.arrow_head,
            editor.style.blur_mode,
            editor.style.blur_strength,
            editor.style.text,
        ),
        Some(ObjectKind::Text { style, fill, .. }) => (
            ContextKind::Text,
            style.rgb,
            editor.style.width,
            *fill,
            editor.style.callout_shape,
            editor.style.arrow_head,
            editor.style.blur_mode,
            editor.style.blur_strength,
            *style,
        ),
        Some(ObjectKind::Callout {
            shape,
            stroke,
            fill,
            style,
            ..
        }) => (
            ContextKind::Callout,
            stroke.rgb,
            stroke.width,
            *fill,
            *shape,
            editor.style.arrow_head,
            editor.style.blur_mode,
            editor.style.blur_strength,
            *style,
        ),
        Some(ObjectKind::Step { rgb, .. }) => (
            ContextKind::Step,
            *rgb,
            editor.style.width,
            editor.style.fill,
            editor.style.callout_shape,
            editor.style.arrow_head,
            editor.style.blur_mode,
            editor.style.blur_strength,
            editor.style.text,
        ),
        Some(ObjectKind::Blur { mode, strength, .. }) => (
            ContextKind::Blur,
            editor.style.rgb,
            editor.style.width,
            editor.style.fill,
            editor.style.callout_shape,
            editor.style.arrow_head,
            *mode,
            *strength,
            editor.style.text,
        ),
        Some(ObjectKind::Magnify { stroke, .. }) => (
            ContextKind::Magnify,
            stroke.rgb,
            stroke.width,
            editor.style.fill,
            editor.style.callout_shape,
            editor.style.arrow_head,
            editor.style.blur_mode,
            editor.style.blur_strength,
            editor.style.text,
        ),
        Some(ObjectKind::Spotlight { .. }) => (
            ContextKind::Spotlight,
            editor.style.rgb,
            editor.style.width,
            editor.style.fill,
            editor.style.callout_shape,
            editor.style.arrow_head,
            editor.style.blur_mode,
            editor.style.blur_strength,
            editor.style.text,
        ),
        Some(ObjectKind::Stamp { .. }) => (
            ContextKind::Stamp,
            editor.style.rgb,
            editor.style.width,
            editor.style.fill,
            editor.style.callout_shape,
            editor.style.arrow_head,
            editor.style.blur_mode,
            editor.style.blur_strength,
            editor.style.text,
        ),
        Some(ObjectKind::Eraser { .. }) => (
            ContextKind::Eraser,
            editor.style.rgb,
            editor.style.width,
            editor.style.fill,
            editor.style.callout_shape,
            editor.style.arrow_head,
            editor.style.blur_mode,
            editor.style.blur_strength,
            editor.style.text,
        ),
        Some(ObjectKind::Fill { rgb, .. }) => (
            ContextKind::Shape,
            *rgb,
            editor.style.width,
            editor.style.fill,
            editor.style.callout_shape,
            editor.style.arrow_head,
            editor.style.blur_mode,
            editor.style.blur_strength,
            editor.style.text,
        ),
        Some(ObjectKind::SmartMove { .. }) => (
            ContextKind::Generic,
            editor.style.rgb,
            editor.style.width,
            editor.style.fill,
            editor.style.callout_shape,
            editor.style.arrow_head,
            editor.style.blur_mode,
            editor.style.blur_strength,
            editor.style.text,
        ),
        None => {
            let kind = match editor.tool() {
                Tool::Select => ContextKind::Generic,
                Tool::Pen | Tool::Highlighter | Tool::Line => ContextKind::StrokeOnly,
                Tool::Arrow => ContextKind::Arrow,
                Tool::Rect | Tool::Ellipse => ContextKind::Shape,
                Tool::Triangle | Tool::Diamond | Tool::Star | Tool::Hexagon => {
                    ContextKind::Shape
                }
                Tool::Text => ContextKind::Text,
                Tool::Callout => ContextKind::Callout,
                Tool::Step => ContextKind::Step,
                Tool::Blur => ContextKind::Blur,
                Tool::Magnify => ContextKind::Magnify,
                Tool::Spotlight => ContextKind::Spotlight,
                Tool::Stamp => ContextKind::Stamp,
                Tool::Eraser => ContextKind::Eraser,
                Tool::Crop => ContextKind::Generic,
                Tool::Cutout => ContextKind::Generic,
                Tool::SelectRegion => ContextKind::Generic,
                Tool::Fill => ContextKind::Shape,
                Tool::SmartMove => ContextKind::Generic,
            };
            (
                kind,
                editor.style.rgb,
                editor.style.width,
                editor.style.fill,
                editor.style.callout_shape,
                editor.style.arrow_head,
                editor.style.blur_mode,
                editor.style.blur_strength,
                editor.style.text,
            )
        }
    };

    // --- Right Cluster first (so we know boundary) ---
    let right_reserved = if has_selection {
        let mut rx = inner.right();
        let btn_s = 26.0;
        let del_rect =
            Rect::from_min_size(Pos2::new(rx - btn_s, cy - btn_s * 0.5), Vec2::splat(btn_s));
        rx = del_rect.left() - 5.0;
        let dup_rect =
            Rect::from_min_size(Pos2::new(rx - btn_s, cy - btn_s * 0.5), Vec2::splat(btn_s));
        rx = dup_rect.left() - 8.0;

        let z_track_w = 56.0;
        let z_rect = Rect::from_min_size(
            Pos2::new(rx - z_track_w, cy - 13.0),
            Vec2::new(z_track_w, 26.0),
        );

        // Z-order capsule
        paint_segmented_track(ui.painter(), z_rect, RADIUS_TRACK);
        let front_r = Rect::from_min_size(
            Pos2::new(z_rect.left() + 2.0, z_rect.top() + 2.0),
            Vec2::new(25.0, 22.0),
        );
        let back_r = Rect::from_min_size(
            Pos2::new(front_r.right() + 2.0, z_rect.top() + 2.0),
            Vec2::new(25.0, 22.0),
        );
        if icon_segment(
            ui,
            "ins_front",
            front_r,
            ActionIcon::LayerFront,
            false,
            true,
            "Bring to Front",
        )
            && editor.reorder(ZOrder::Front) {
                mutated = true;
            }
        if icon_segment(
            ui,
            "ins_back",
            back_r,
            ActionIcon::LayerBack,
            false,
            true,
            "Send to Back",
        )
            && editor.reorder(ZOrder::Back) {
                mutated = true;
            }

        // Duplicate
        let dup_id = ui.auto_id_with("ins_dup");
        let dup_resp = ui
            .interact(dup_rect, dup_id, Sense::click())
            .on_hover_text("Duplicate (Ctrl+D)");
        paint_secondary_button(
            ui.painter(),
            dup_rect,
            RADIUS_PILL,
            hot_t(&dup_resp),
            true,
            false,
        );
        icons::draw_action_icon(
            ui.painter(),
            dup_rect.center(),
            ActionIcon::Duplicate,
            if dup_resp.hovered() {
                text_primary()
            } else {
                text_secondary()
            },
        );
        if dup_resp.clicked() && editor.duplicate_selected().is_some() {
            mutated = true;
        }

        // Delete
        let del_id = ui.auto_id_with("ins_del");
        let del_resp = ui
            .interact(del_rect, del_id, Sense::click())
            .on_hover_text("Delete Selected (Del)");
        paint_secondary_button(
            ui.painter(),
            del_rect,
            RADIUS_PILL,
            hot_t(&del_resp),
            true,
            true,
        );
        icons::draw_action_icon(
            ui.painter(),
            del_rect.center(),
            ActionIcon::Trash,
            if del_resp.hovered() {
                danger_red()
            } else {
                text_secondary()
            },
        );
        if del_resp.clicked() && editor.delete_selected() {
            mutated = true;
        }

        inner.right() - z_rect.left() + 12.0
    } else {
        0.0
    };

    // --- Left/Main Contextual Controls ---
    let mut x = inner.left();
    let max_x = inner.right() - right_reserved;

    if ctx_kind == ContextKind::Generic && !has_selection {
        ui.painter().text(
            Pos2::new(x, cy),
            egui::Align2::LEFT_CENTER,
            "No options for this tool",
            FontId::proportional(qs_theme::font::SMALL as f32),
            text_tertiary(),
        );
        return mutated;
    }

    // 1. Color Swatches (for everything except Blur)
    if ctx_kind != ContextKind::Blur {
        for (idx, &rgb) in PALETTE.iter().enumerate() {
            let sw_r = Rect::from_center_size(Pos2::new(x + SWATCH * 0.5, cy), Vec2::splat(SWATCH));
            if sw_r.right() > max_x {
                break;
            }
            let id = ui.auto_id_with(("ins_col", idx));
            let resp = ui
                .interact(sw_r, id, Sense::click())
                .on_hover_text(format!("#{:06X}", rgb));
            paint_color_swatch(
                ui.painter(),
                sw_r.center(),
                SWATCH * 0.34,
                rgb_to_color32(rgb),
                eff_rgb == rgb,
                hot_t(&resp),
            );
            if resp.clicked() {
                editor.set_color(rgb);
                mutated = true;
            }
            x += SWATCH + SPACE_XS;
        }
    }

    // 2. Stroke Width Capsule (with rounded line-weight preview pills)
    if matches!(
        ctx_kind,
        ContextKind::Generic
            | ContextKind::StrokeOnly
            | ContextKind::Shape
            | ContextKind::Arrow
            | ContextKind::Callout
    ) {
        if x > inner.left() {
            x = draw_ins_divider(ui.painter(), x + SPACE_XS, cy) + SPACE_SM;
        }
        let seg_w = CONTROL_H;
        let track_w = 2.0 * TRACK_PAD + SIZES.len() as f32 * seg_w;
        if x + track_w <= max_x {
            let track_r = Rect::from_min_size(
                Pos2::new(x, cy - CONTROL_H * 0.5),
                Vec2::new(track_w, CONTROL_H),
            );
            paint_segmented_track(ui.painter(), track_r, RADIUS_TRACK);
            for (i, &w) in SIZES.iter().enumerate() {
                let sr = Rect::from_min_size(
                    Pos2::new(
                        track_r.left() + TRACK_PAD + i as f32 * seg_w,
                        track_r.top() + TRACK_PAD,
                    ),
                    Vec2::new(seg_w, CONTROL_H - 2.0 * TRACK_PAD),
                );
                let active = (eff_width - w).abs() < 0.35;
                let tip = match i {
                    0 => "Thin Stroke (2px)",
                    1 => "Medium Stroke (4px)",
                    _ => "Thick Stroke (6px)",
                };
                let id = ui.auto_id_with(("ins_w", i));
                let resp = ui.interact(sr, id, Sense::click()).on_hover_text(tip);
                paint_pill_button(
                    ui.painter(),
                    sr,
                    egui::CornerRadius::same(6),
                    active,
                    hot_t(&resp),
                    true,
                    false,
                );
                let bar_h = match i {
                    0 => 1.5,
                    1 => 2.8,
                    _ => 4.4,
                };
                let col = if active || resp.hovered() {
                    text_primary()
                } else {
                    text_secondary()
                };
                let bar_rect =
                    Rect::from_center_size(sr.center(), Vec2::new(sr.width() - 12.0, bar_h));
                ui.painter().rect_filled(bar_rect, RADIUS_CAPSULE, col);
                if resp.clicked() {
                    editor.set_stroke_width(w);
                    mutated = true;
                }
            }
            x += track_w;
        }
    }

    // 3. Fill Toggle & Quick Fill Preset (for Shape / Callout / Text)
    if matches!(
        ctx_kind,
        ContextKind::Shape | ContextKind::Callout | ContextKind::Text
    ) {
        x = draw_ins_divider(ui.painter(), x + 6.0, cy) + 6.0;
        let is_filled = !eff_fill.is_none();
        let fill_w = 56.0;
        if x + fill_w <= max_x {
            let fr = Rect::from_min_size(Pos2::new(x, cy - 13.0), Vec2::new(fill_w, 26.0));
            let id = ui.auto_id_with("ins_fill");
            let resp = ui
                .interact(fr, id, Sense::click())
                .on_hover_text("Toggle Background Fill");
            if is_filled {
                paint_pill_button(
                    ui.painter(),
                    fr,
                    RADIUS_PILL,
                    true,
                    hot_t(&resp),
                    true,
                    false,
                );
            } else {
                paint_secondary_button(ui.painter(), fr, RADIUS_PILL, hot_t(&resp), true, false);
            }
            let fg = if is_filled || resp.hovered() {
                text_primary()
            } else {
                text_secondary()
            };
            icons::draw_action_icon(
                ui.painter(),
                Pos2::new(fr.left() + 13.0, cy),
                ActionIcon::Fill,
                fg,
            );
            ui.painter().text(
                Pos2::new(fr.left() + 24.0, cy),
                egui::Align2::LEFT_CENTER,
                "Fill",
                FontId::proportional(qs_theme::font::SMALL as f32),
                fg,
            );
            if resp.clicked() {
                let next_fill = if is_filled {
                    Fill::None
                } else if ctx_kind == ContextKind::Callout {
                    Fill::Solid {
                        rgb: 0xFFFFFF, // allow-hex: document fill data, not UI chrome
                        alpha: 1.0,
                    }
                } else if ctx_kind == ContextKind::Text {
                    // Solid pill badge with automatic high-contrast text color.
                    Fill::Solid {
                        rgb: eff_rgb,
                        alpha: 0.94,
                    }
                } else {
                    Fill::Solid {
                        rgb: eff_rgb,
                        alpha: 0.25,
                    }
                };
                if ctx_kind == ContextKind::Callout {
                    editor.set_callout_fill_and_contrast(next_fill);
                } else if ctx_kind == ContextKind::Text {
                    editor.set_fill(next_fill);
                    if let Fill::Solid { rgb, .. } = next_fill {
                        editor.set_color(qs_edit::contrast(rgb));
                    }
                } else {
                    editor.set_fill(next_fill);
                }
                mutated = true;
            }
            x += fill_w;
        }
    }

    // 4. Contextual Tool / Selection Controls
    match ctx_kind {
        ContextKind::Callout => {
            x = draw_ins_divider(ui.painter(), x + 6.0, cy) + 6.0;
            let shapes = [
                (
                    CalloutShape::RoundRect,
                    ActionIcon::CalloutRoundRect,
                    "Rounded Bubble",
                ),
                (
                    CalloutShape::Ellipse,
                    ActionIcon::CalloutEllipse,
                    "Ellipse Bubble",
                ),
                (CalloutShape::Pill, ActionIcon::CalloutPill, "Pill Bubble"),
            ];
            let seg_w = 26.0;
            let tw = 6.0 + shapes.len() as f32 * seg_w;
            if x + tw <= max_x {
                let tr = Rect::from_min_size(Pos2::new(x, cy - 13.0), Vec2::new(tw, 26.0));
                paint_segmented_track(ui.painter(), tr, RADIUS_TRACK);
                for (i, (sh, ic, tip)) in shapes.into_iter().enumerate() {
                    let sr = Rect::from_min_size(
                        Pos2::new(tr.left() + 3.0 + i as f32 * seg_w, tr.top() + 3.0),
                        Vec2::new(seg_w, 20.0),
                    );
                    if icon_segment(
                        ui,
                        match i {
                            0 => "ins_cs0",
                            1 => "ins_cs1",
                            _ => "ins_cs2",
                        },
                        sr,
                        ic,
                        eff_callout == sh,
                        true,
                        tip,
                    ) {
                        editor.set_callout_shape(sh);
                        mutated = true;
                    }
                }
                x += tw;
            }
            draw_text_controls(ui, editor, &eff_text, x, cy, max_x, &mut mutated);
        }
        ContextKind::Text => {
            draw_text_controls(ui, editor, &eff_text, x, cy, max_x, &mut mutated);
        }
        ContextKind::Arrow => {
            x = draw_ins_divider(ui.painter(), x + 6.0, cy) + 6.0;
            let heads = [
                (ArrowHead::Open, ActionIcon::ArrowOpen, "Open Arrowhead"),
                (
                    ArrowHead::Filled,
                    ActionIcon::ArrowFilled,
                    "Filled Arrowhead",
                ),
                (ArrowHead::Curved, ActionIcon::Redo, "Curved Arrow (Bezier)"),
            ];
            let seg_w = 28.0;
            let tw = 6.0 + heads.len() as f32 * seg_w;
            if x + tw <= max_x {
                let tr = Rect::from_min_size(Pos2::new(x, cy - 13.0), Vec2::new(tw, 26.0));
                paint_segmented_track(ui.painter(), tr, RADIUS_TRACK);
                for (i, (hd, ic, tip)) in heads.into_iter().enumerate() {
                    let sr = Rect::from_min_size(
                        Pos2::new(tr.left() + 3.0 + i as f32 * seg_w, tr.top() + 3.0),
                        Vec2::new(seg_w, 20.0),
                    );
                    let id = match i {
                        0 => "ins_ah0",
                        1 => "ins_ah1",
                        _ => "ins_ah2",
                    };
                    if icon_segment(ui, id, sr, ic, eff_arrow == hd, true, tip) {
                        editor.set_arrow_head(hd);
                        mutated = true;
                    }
                }
            }
        }
        ContextKind::Blur => {
            let modes = [
                (
                    BlurMode::Pixelate,
                    ActionIcon::BlurPixelate,
                    "Pixelate Mosaic",
                ),
                (
                    BlurMode::Gaussian,
                    ActionIcon::BlurSmooth,
                    "Smooth Gaussian Blur",
                ),
                (
                    BlurMode::Solid,
                    ActionIcon::Fill,
                    "Solid Blackout (Redaction)",
                ),
            ];
            let seg_w = 28.0;
            let tw = 6.0 + modes.len() as f32 * seg_w;
            if x + tw <= max_x {
                let tr = Rect::from_min_size(Pos2::new(x, cy - 13.0), Vec2::new(tw, 26.0));
                paint_segmented_track(ui.painter(), tr, RADIUS_TRACK);
                for (i, (md, ic, tip)) in modes.into_iter().enumerate() {
                    let sr = Rect::from_min_size(
                        Pos2::new(tr.left() + 3.0 + i as f32 * seg_w, tr.top() + 3.0),
                        Vec2::new(seg_w, 20.0),
                    );
                    let id = match i {
                        0 => "ins_bm0",
                        1 => "ins_bm1",
                        _ => "ins_bm2",
                    };
                    if icon_segment(ui, id, sr, ic, eff_blur_mode == md, true, tip) {
                        editor.set_blur_mode(md);
                        mutated = true;
                    }
                }
                x += tw;
            }

            x = draw_ins_divider(ui.painter(), x + 6.0, cy) + 6.0;
            let levels = [(8.0, "Low"), (14.0, "Med"), (24.0, "High")];
            let lseg_w = 38.0;
            let ltw = 6.0 + levels.len() as f32 * lseg_w;
            if x + ltw <= max_x {
                let tr = Rect::from_min_size(Pos2::new(x, cy - 13.0), Vec2::new(ltw, 26.0));
                paint_segmented_track(ui.painter(), tr, RADIUS_TRACK);
                for (i, (val, lbl)) in levels.into_iter().enumerate() {
                    let sr = Rect::from_min_size(
                        Pos2::new(tr.left() + 3.0 + i as f32 * lseg_w, tr.top() + 3.0),
                        Vec2::new(lseg_w, 20.0),
                    );
                    let active = (eff_blur_str - val).abs() < 3.5;
                    let id = ui.auto_id_with(("ins_bs", i));
                    let resp = ui
                        .interact(sr, id, Sense::click())
                        .on_hover_text(format!("Blur Intensity: {lbl}"));
                    paint_pill_button(
                        ui.painter(),
                        sr,
                        RADIUS_PILL,
                        active,
                        hot_t(&resp),
                        true,
                        false,
                    );
                    ui.painter().text(
                        sr.center(),
                        egui::Align2::CENTER_CENTER,
                        lbl,
                        FontId::proportional(qs_theme::font::SMALL as f32),
                        if active || resp.hovered() {
                            text_primary()
                        } else {
                            text_secondary()
                        },
                    );
                    if resp.clicked() {
                        editor.set_blur_strength(val);
                        mutated = true;
                    }
                }
            }
        }
        ContextKind::Magnify => {
            x = draw_ins_divider(ui.painter(), x + 6.0, cy) + 6.0;
            for (i, zoom) in [1.5_f32, 2.0, 3.0, 4.0].into_iter().enumerate() {
                let r = Rect::from_min_size(Pos2::new(x, cy - 11.0), Vec2::new(32.0, 22.0));
                let resp = ui
                    .interact(r, ui.auto_id_with(("magnify_zoom", i)), Sense::click())
                    .on_hover_text(format!("Magnification: {zoom}×"));
                let active = (editor.style.magnify_zoom - zoom).abs() < 0.01;
                paint_pill_button(
                    ui.painter(),
                    r,
                    RADIUS_PILL,
                    active,
                    hot_t(&resp),
                    true,
                    false,
                );
                ui.painter().text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{zoom}×"),
                    FontId::proportional(qs_theme::font::SMALL as f32),
                    text_primary(),
                );
                if resp.clicked() {
                    editor.set_magnify_zoom(zoom);
                    mutated = true;
                }
                x += 34.0;
            }
            x = draw_ins_divider(ui.painter(), x + 4.0, cy) + 4.0;
            for (i, shape) in [qs_doc::MagnifyShape::Ellipse, qs_doc::MagnifyShape::Rect]
                .into_iter()
                .enumerate()
            {
                let r = Rect::from_min_size(Pos2::new(x, cy - 11.0), Vec2::new(38.0, 22.0));
                let resp = ui
                    .interact(r, ui.auto_id_with(("magnify_shape", i)), Sense::click())
                    .on_hover_text(match shape {
                        qs_doc::MagnifyShape::Ellipse => "Circular lens",
                        qs_doc::MagnifyShape::Rect => "Rectangular lens",
                    });
                let active = editor.style.magnify_shape == shape;
                paint_pill_button(
                    ui.painter(),
                    r,
                    RADIUS_PILL,
                    active,
                    hot_t(&resp),
                    true,
                    false,
                );
                ui.painter().text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    if shape == qs_doc::MagnifyShape::Ellipse {
                        "Circle"
                    } else {
                        "Rect"
                    },
                    FontId::proportional(qs_theme::font::SMALL as f32),
                    text_primary(),
                );
                if resp.clicked() {
                    editor.set_magnify_shape(shape);
                    mutated = true;
                }
                x += 40.0;
            }
        }
        ContextKind::Spotlight => {
            x = draw_ins_divider(ui.painter(), x + 6.0, cy) + 6.0;
            for (i, dim) in [0.4_f32, 0.6, 0.8].into_iter().enumerate() {
                let r = Rect::from_min_size(Pos2::new(x, cy - 11.0), Vec2::new(40.0, 22.0));
                let resp = ui
                    .interact(r, ui.auto_id_with(("spot_dim", i)), Sense::click())
                    .on_hover_text(format!("Dim outside: {}%", (dim * 100.0) as u32));
                let active = (editor.style.spot_dim - dim).abs() < 0.01;
                paint_pill_button(
                    ui.painter(),
                    r,
                    RADIUS_PILL,
                    active,
                    hot_t(&resp),
                    true,
                    false,
                );
                ui.painter().text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{}%", (dim * 100.0) as u32),
                    FontId::proportional(qs_theme::font::SMALL as f32),
                    text_primary(),
                );
                if resp.clicked() {
                    editor.set_spotlight(Some(dim), None);
                    mutated = true;
                }
                x += 42.0;
            }
            x = draw_ins_divider(ui.painter(), x + 4.0, cy) + 4.0;
            for (i, (blur, label)) in [(0.0_f32, "Sharp"), (8.0, "Soft"), (20.0, "Feather")]
                .into_iter()
                .enumerate()
            {
                let r = Rect::from_min_size(Pos2::new(x, cy - 11.0), Vec2::new(52.0, 22.0));
                let resp = ui
                    .interact(r, ui.auto_id_with(("spot_blur", i)), Sense::click())
                    .on_hover_text(format!("Edge softness: {blur}px"));
                let active = (editor.style.spot_blur - blur).abs() < 0.01;
                paint_pill_button(
                    ui.painter(),
                    r,
                    RADIUS_PILL,
                    active,
                    hot_t(&resp),
                    true,
                    false,
                );
                ui.painter().text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(qs_theme::font::SMALL as f32),
                    text_primary(),
                );
                if resp.clicked() {
                    editor.set_spotlight(None, Some(blur));
                    mutated = true;
                }
                x += 54.0;
            }
        }
        ContextKind::Stamp => {
            x = draw_ins_divider(ui.painter(), x + 6.0, cy) + 6.0;
            for (i, kind) in qs_doc::StampKind::ALL.into_iter().enumerate() {
                let r = Rect::from_min_size(Pos2::new(x, cy - 11.0), Vec2::new(58.0, 22.0));
                let resp = ui
                    .interact(r, ui.auto_id_with(("stamp_kind", i)), Sense::click())
                    .on_hover_text(format!("{} stamp", kind.label()));
                let active = editor.style.stamp_kind == kind;
                paint_pill_button(
                    ui.painter(),
                    r,
                    RADIUS_PILL,
                    active,
                    hot_t(&resp),
                    true,
                    false,
                );
                ui.painter().text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    kind.label(),
                    FontId::proportional(qs_theme::font::CAPTION as f32),
                    text_primary(),
                );
                if resp.clicked() {
                    editor.set_stamp_kind(kind);
                    mutated = true;
                }
                x += 60.0;
            }
        }
        ContextKind::Eraser => {
            x = draw_ins_divider(ui.painter(), x + 6.0, cy) + 6.0;
            for (i, w) in [4.0_f32, 10.0, 24.0].into_iter().enumerate() {
                let r = Rect::from_min_size(Pos2::new(x, cy - 11.0), Vec2::new(30.0, 22.0));
                let resp = ui
                    .interact(r, ui.auto_id_with(("eraser_w", i)), Sense::click())
                    .on_hover_text(format!("Eraser size: {w}px"));
                let active = (editor.style.eraser_width - w).abs() < 0.01;
                paint_pill_button(
                    ui.painter(),
                    r,
                    RADIUS_PILL,
                    active,
                    hot_t(&resp),
                    true,
                    false,
                );
                ui.painter().text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{w}"),
                    FontId::proportional(qs_theme::font::SMALL as f32),
                    text_primary(),
                );
                if resp.clicked() {
                    editor.set_eraser_width(w);
                    mutated = true;
                }
                x += 32.0;
            }
        }
        ContextKind::Step => {
            x = draw_ins_divider(ui.painter(), x + 6.0, cy) + 6.0;
            let cur_start = editor.doc().step_start;
            let track_w = 88.0;
            if x + track_w <= max_x {
                ui.painter().text(
                    Pos2::new(x, cy),
                    egui::Align2::LEFT_CENTER,
                    "Start #",
                    FontId::proportional(qs_theme::font::SMALL as f32),
                    text_secondary(),
                );
                let sx = x + 46.0;
                let tr = Rect::from_min_size(Pos2::new(sx, cy - 13.0), Vec2::new(72.0, 26.0));
                paint_segmented_track(ui.painter(), tr, RADIUS_TRACK);
                let minus_r = Rect::from_min_size(
                    Pos2::new(tr.left() + 2.0, tr.top() + 2.0),
                    Vec2::splat(22.0),
                );
                let plus_r = Rect::from_min_size(
                    Pos2::new(tr.right() - 24.0, tr.top() + 2.0),
                    Vec2::splat(22.0),
                );
                if icon_segment(
                    ui,
                    "ins_step_dec",
                    minus_r,
                    ActionIcon::ZoomOut,
                    false,
                    cur_start > 1,
                    "Decrement Start Number",
                ) {
                    editor.set_step_start(cur_start.saturating_sub(1).max(1));
                    mutated = true;
                }
                ui.painter().text(
                    tr.center(),
                    egui::Align2::CENTER_CENTER,
                    cur_start.to_string(),
                    FontId::proportional(12.0),
                    text_primary(),
                );
                if icon_segment(
                    ui,
                    "ins_step_inc",
                    plus_r,
                    ActionIcon::ZoomIn,
                    false,
                    true,
                    "Increment Start Number",
                ) {
                    editor.set_step_start(cur_start + 1);
                    mutated = true;
                }
            }
        }
        _ => {}
    }

    mutated
}

fn draw_ins_divider(painter: &egui::Painter, x: f32, cy: f32) -> f32 {
    let px = x.round() + 0.5;
    painter.line_segment(
        [Pos2::new(px, cy - 9.0), Pos2::new(px, cy + 9.0)],
        Stroke::new(1.0, divider_vertical()),
    );
    x + 4.0
}

fn draw_text_controls(
    ui: &mut Ui,
    editor: &mut Editor,
    eff_text: &qs_doc::TextStyle,
    mut x: f32,
    cy: f32,
    max_x: f32,
    mutated: &mut bool,
) -> f32 {
    x = draw_ins_divider(ui.painter(), x + 6.0, cy) + 6.0;

    // Font sizes capsule
    let seg_w = 27.0;
    let tw = 6.0 + TEXT_SIZES.len() as f32 * seg_w;
    if x + tw <= max_x {
        let tr = Rect::from_min_size(Pos2::new(x, cy - 13.0), Vec2::new(tw, 26.0));
        paint_segmented_track(ui.painter(), tr, RADIUS_TRACK);
        for (i, &sz) in TEXT_SIZES.iter().enumerate() {
            let sr = Rect::from_min_size(
                Pos2::new(tr.left() + 3.0 + i as f32 * seg_w, tr.top() + 3.0),
                Vec2::new(seg_w, 20.0),
            );
            let active = (eff_text.px - sz).abs() < 0.5;
            let id = ui.auto_id_with(("ins_tsz", i));
            let resp = ui
                .interact(sr, id, Sense::click())
                .on_hover_text(format!("Font Size {:.0}px", sz));
            paint_pill_button(
                ui.painter(),
                sr,
                RADIUS_PILL,
                active,
                hot_t(&resp),
                true,
                false,
            );
            ui.painter().text(
                sr.center(),
                egui::Align2::CENTER_CENTER,
                format!("{:.0}", sz),
                FontId::proportional(qs_theme::font::SMALL as f32),
                if active || resp.hovered() {
                    text_primary()
                } else {
                    text_secondary()
                },
            );
            if resp.clicked() {
                editor.set_text_px(sz);
                *mutated = true;
            }
        }
        x += tw + 6.0;
    }

    // Bold / Italic / Underline / Strike / Outline toggles
    let outline_on = eff_text.outline.is_some();
    for (key, label, active, tip) in [
        ("ins_bold", "B", eff_text.bold, "Bold"),
        ("ins_italic", "I", eff_text.italic, "Italic"),
        ("ins_under", "U", eff_text.underline, "Underline"),
        ("ins_strike", "S", eff_text.strike, "Strikethrough"),
        ("ins_outline", "O", outline_on, "Text outline"),
    ] {
        if text_toggle(ui, (key, label, tip), active, &mut x, cy, max_x) {
            match key {
                "ins_bold" => editor.set_text_bold(!eff_text.bold),
                "ins_italic" => editor.set_text_italic(!eff_text.italic),
                "ins_under" => editor.set_text_underline(!eff_text.underline),
                "ins_strike" => editor.set_text_strike(!eff_text.strike),
                _ => {
                    if outline_on {
                        editor.set_text_outline(None);
                    } else {
                        editor.set_text_outline(Some(qs_doc::TextOutline {
                            width: (eff_text.px * 0.14).clamp(1.0, 6.0),
                            rgb: qs_edit::contrast(eff_text.rgb),
                        }));
                    }
                }
            }
            *mutated = true;
        }
    }

    // Font family capsule
    x = draw_ins_divider(ui.painter(), x + 6.0, cy) + 6.0;
    let fw = 40.0;
    let ftw = 6.0 + fw * 3.0;
    if x + ftw <= max_x {
        let tr = Rect::from_min_size(Pos2::new(x, cy - 13.0), Vec2::new(ftw, 26.0));
        paint_segmented_track(ui.painter(), tr, RADIUS_TRACK);
        for (i, family) in qs_doc::FontFamily::ALL.iter().enumerate() {
            let sr = Rect::from_min_size(
                Pos2::new(tr.left() + 3.0 + i as f32 * fw, tr.top() + 3.0),
                Vec2::new(fw, 20.0),
            );
            let active = eff_text.family == *family;
            let resp = ui
                .interact(sr, ui.auto_id_with(("ins_font", i)), Sense::click())
                .on_hover_text(format!("{} typeface", family.label()));
            paint_pill_button(
                ui.painter(),
                sr,
                RADIUS_PILL,
                active,
                hot_t(&resp),
                true,
                false,
            );
            ui.painter().text(
                sr.center(),
                egui::Align2::CENTER_CENTER,
                family.label(),
                FontId::proportional(qs_theme::font::CAPTION as f32),
                if active || resp.hovered() {
                    text_primary()
                } else {
                    text_secondary()
                },
            );
            if resp.clicked() {
                editor.set_text_family(*family);
                *mutated = true;
            }
        }
        x += ftw;
    }

    // Align capsule
    x = draw_ins_divider(ui.painter(), x + 6.0, cy) + 6.0;
    let aligns = [
        (Align::Left, ActionIcon::AlignLeft, "Align Left"),
        (Align::Center, ActionIcon::AlignCenter, "Align Center"),
        (Align::Right, ActionIcon::AlignRight, "Align Right"),
    ];
    let aw = 24.0;
    let atw = 6.0 + aligns.len() as f32 * aw;
    if x + atw <= max_x {
        let tr = Rect::from_min_size(Pos2::new(x, cy - 13.0), Vec2::new(atw, 26.0));
        paint_segmented_track(ui.painter(), tr, RADIUS_TRACK);
        for (i, (al, ic, tip)) in aligns.into_iter().enumerate() {
            let sr = Rect::from_min_size(
                Pos2::new(tr.left() + 3.0 + i as f32 * aw, tr.top() + 3.0),
                Vec2::new(aw, 20.0),
            );
            if icon_segment(
                ui,
                match i {
                    0 => "ins_al0",
                    1 => "ins_al1",
                    _ => "ins_al2",
                },
                sr,
                ic,
                eff_text.align == al,
                true,
                tip,
            ) {
                editor.set_text_align(al);
                *mutated = true;
            }
        }
        x += atw;
    }

    x
}

/// One small square text-format toggle (B / I / U / S / O). Advances `x`.
/// `spec` is (id key, button label, hover tooltip).
fn text_toggle(
    ui: &mut Ui,
    (key, label, tip): (&str, &str, &str),
    active: bool,
    x: &mut f32,
    cy: f32,
    max_x: f32,
) -> bool {
    let r = Rect::from_min_size(Pos2::new(*x, cy - 13.0), Vec2::splat(26.0));
    if r.right() > max_x {
        return false;
    }
    let resp = ui
        .interact(r, ui.auto_id_with(key), Sense::click())
        .on_hover_text(tip);
    if active {
        paint_pill_button(
            ui.painter(),
            r,
            RADIUS_PILL,
            true,
            hot_t(&resp),
            true,
            false,
        );
    } else {
        paint_secondary_button(ui.painter(), r, RADIUS_PILL, hot_t(&resp), true, false);
    }
    ui.painter().text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::new(12.5, egui::FontFamily::Name("Bold".into())),
        if active || resp.hovered() {
            text_primary()
        } else {
            text_secondary()
        },
    );
    *x += 26.0;
    resp.clicked()
}
