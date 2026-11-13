//! Unified macOS-style header toolbar and contextual inspector bar.

use crate::{
    icons::{self, ActionIcon},
    theme::*,
};
use eframe::egui::{
    self, Color32, FontFamily, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2,
};
use qs_doc::{
    Align, ArrowHead, BlurMode, CalloutShape, Fill, ObjectKind, PALETTE, SIZES, TEXT_SIZES,
};
use qs_edit::{Editor, Tool, ZOrder};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoomCmd {
    None,
    Out,
    In,
    Fit,
}

#[derive(Default)]
pub struct TopBarResult {
    pub mutated: bool,
    pub copy: bool,
    pub save: bool,
    pub save_as: bool,
    pub open: bool,
    pub export: bool,
    pub canvas: bool,
    pub zoom: ZoomCmd,
}

impl Default for ZoomCmd {
    fn default() -> Self {
        Self::None
    }
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

    let inner = bar_rect.shrink2(Vec2::new(CHROME_PAD_X, 0.0));
    let cy = inner.center().y;

    // --- Left Zone: Open / Save As + Undo / Redo Capsule + optional filename ---
    let hist_w = 60.0;
    let hist_h = 30.0;
    let io_w = 30.0;
    let io_gap = 4.0;
    // the capsule stays dead-center only while the right zone keeps its width,
    // so the file buttons appear once there is room for them
    let show_file_buttons = bar_rect.width() > 980.0;
    let mut left_x = inner.left();
    if show_file_buttons {
        let open_rect = Rect::from_min_size(
            Pos2::new(left_x, cy - hist_h * 0.5),
            Vec2::new(io_w, hist_h),
        );
        if icon_segment(
            ui,
            "tb_open",
            open_rect,
            ActionIcon::Open,
            false,
            true,
            "Open… (Ctrl+O)",
        ) {
            res.open = true;
        }
        left_x = open_rect.right() + io_gap;

        let save_as_rect = Rect::from_min_size(
            Pos2::new(left_x, cy - hist_h * 0.5),
            Vec2::new(io_w, hist_h),
        );
        if icon_segment(
            ui,
            "tb_save_as",
            save_as_rect,
            ActionIcon::SaveAs,
            false,
            true,
            "Save As… (Ctrl+Shift+S)",
        ) {
            res.save_as = true;
        }
        left_x = save_as_rect.right() + 8.0;
    }

    let hist_rect = Rect::from_min_size(
        Pos2::new(left_x, cy - hist_h * 0.5),
        Vec2::new(hist_w, hist_h),
    );
    paint_segmented_track(ui.painter(), hist_rect, RADIUS_TRACK);

    let seg_w = (hist_w - 6.0) * 0.5;
    let undo_rect = Rect::from_min_size(
        Pos2::new(hist_rect.left() + 3.0, hist_rect.top() + 3.0),
        Vec2::new(seg_w, hist_h - 6.0),
    );
    let redo_rect = Rect::from_min_size(
        Pos2::new(undo_rect.right(), hist_rect.top() + 3.0),
        Vec2::new(seg_w, hist_h - 6.0),
    );

    let can_undo = editor.can_undo();
    let can_redo = editor.can_redo();

    if icon_segment(
        ui,
        "tb_undo",
        undo_rect,
        ActionIcon::Undo,
        false,
        can_undo,
        "Undo (Ctrl+Z)",
    ) && editor.undo()
    {
        res.mutated = true;
    }

    ui.painter().line_segment(
        [
            Pos2::new(undo_rect.right() + 0.5, hist_rect.top() + 6.0),
            Pos2::new(undo_rect.right() + 0.5, hist_rect.bottom() - 6.0),
        ],
        Stroke::new(1.0, divider_vertical()),
    );

    if icon_segment(
        ui,
        "tb_redo",
        redo_rect,
        ActionIcon::Redo,
        false,
        can_redo,
        "Redo (Ctrl+Y)",
    ) && editor.redo()
    {
        res.mutated = true;
    }

    // Canvas controls live beside history, away from the centered tool capsule.
    let canvas_rect = Rect::from_min_size(
        Pos2::new(hist_rect.right() + 8.0, cy - hist_h * 0.5),
        Vec2::new(72.0, hist_h),
    );
    if chrome_button(
        ui,
        "tb_canvas",
        canvas_rect,
        ActionIcon::Canvas,
        "Canvas",
        false,
        "Resize canvas and change its background",
    ) {
        res.canvas = true;
    }

    // Only show filename if window is very wide (> 1120px) so center tool capsule remains dead-center
    if bar_rect.width() > 1260.0 {
        if let Some(p) = doc_path {
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                let mut disp = name.to_string();
                if disp.len() > 24 {
                    disp.truncate(21);
                    disp.push_str("...");
                }
                ui.painter().text(
                    Pos2::new(hist_rect.right() + 14.0, cy),
                    egui::Align2::LEFT_CENTER,
                    &disp,
                    FontId::proportional(11.5),
                    text_tertiary(),
                );
            }
        }
    }

    // --- Right Zone: Zoom Pill + Save / Export / Copy ---
    let compact_actions = bar_rect.width() < 1060.0;
    let zoom_w = 94.0;
    let save_w = if compact_actions { 30.0 } else { 64.0 };
    let export_w = if compact_actions { 30.0 } else { 72.0 };
    let copy_w = 68.0;
    let btn_h = 30.0;
    let right_gap = 6.0;

    let mut x_cursor = inner.right();

    let copy_rect = Rect::from_min_size(
        Pos2::new(x_cursor - copy_w, cy - btn_h * 0.5),
        Vec2::new(copy_w, btn_h),
    );
    x_cursor = copy_rect.left() - right_gap;

    let export_rect = Rect::from_min_size(
        Pos2::new(x_cursor - export_w, cy - btn_h * 0.5),
        Vec2::new(export_w, btn_h),
    );
    x_cursor = export_rect.left() - 4.0;

    let save_rect = Rect::from_min_size(
        Pos2::new(x_cursor - save_w, cy - btn_h * 0.5),
        Vec2::new(save_w, btn_h),
    );
    x_cursor = save_rect.left() - 10.0;

    let zoom_rect = Rect::from_min_size(
        Pos2::new(x_cursor - zoom_w, cy - btn_h * 0.5),
        Vec2::new(zoom_w, btn_h),
    );

    // Zoom pill: [-] [100%] [+]
    paint_segmented_track(ui.painter(), zoom_rect, RADIUS_TRACK);
    let z_btn_w = 23.0;
    let z_out_rect = Rect::from_min_size(
        Pos2::new(zoom_rect.left() + 3.0, zoom_rect.top() + 3.0),
        Vec2::new(z_btn_w, btn_h - 6.0),
    );
    let z_in_rect = Rect::from_min_size(
        Pos2::new(zoom_rect.right() - 3.0 - z_btn_w, zoom_rect.top() + 3.0),
        Vec2::new(z_btn_w, btn_h - 6.0),
    );
    let z_mid_rect = Rect::from_min_max(
        Pos2::new(z_out_rect.right(), zoom_rect.top() + 3.0),
        Pos2::new(z_in_rect.left(), zoom_rect.bottom() - 3.0),
    );

    if icon_segment(
        ui,
        "tb_zout",
        z_out_rect,
        ActionIcon::ZoomOut,
        false,
        true,
        "Zoom Out",
    ) {
        res.zoom = ZoomCmd::Out;
    }

    let zoom_pct = format!("{:.0}%", zoom * 100.0);
    let z_mid_id = ui.auto_id_with("tb_zfit");
    let z_mid_resp = ui
        .interact(z_mid_rect, z_mid_id, Sense::click())
        .on_hover_text("Fit Document to Window");
    if z_mid_resp.hovered() {
        ui.painter().rect_filled(z_mid_rect, 4.0, bg_pill_hover());
    }
    ui.painter().text(
        z_mid_rect.center(),
        egui::Align2::CENTER_CENTER,
        &zoom_pct,
        FontId::proportional(11.5),
        if z_mid_resp.hovered() {
            text_primary()
        } else {
            text_secondary()
        },
    );
    if z_mid_resp.clicked() {
        res.zoom = ZoomCmd::Fit;
    }

    if icon_segment(
        ui,
        "tb_zin",
        z_in_rect,
        ActionIcon::ZoomIn,
        false,
        true,
        "Zoom In",
    ) {
        res.zoom = ZoomCmd::In;
    }

    // Secondary action buttons (Save / Export) + Primary CTA (Copy)
    if compact_actions {
        if icon_segment(
            ui,
            "tb_save",
            save_rect,
            ActionIcon::Save,
            false,
            true,
            "Save .qsdoc (Ctrl+S)",
        ) {
            res.save = true;
        }
        if icon_segment(
            ui,
            "tb_export",
            export_rect,
            ActionIcon::Export,
            false,
            true,
            "Export PNG",
        ) {
            res.export = true;
        }
    } else {
        if chrome_button(
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
        if chrome_button(
            ui,
            "tb_export",
            export_rect,
            ActionIcon::Export,
            "Export",
            false,
            "Export PNG",
        ) {
            res.export = true;
        }
    }

    if chrome_button(
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

    // --- Center Zone: Tool Segmented Capsule ---
    let tools_group1 = [
        (Tool::Select, "Select (V)"),
        (Tool::SelectRegion, "Select Region (M)"),
    ];
    let tools_group2 = [
        (Tool::Pen, "Pen (P)"),
        (Tool::Highlighter, "Highlighter (H)"),
        (Tool::Arrow, "Arrow (A)"),
        (Tool::Line, "Line (L)"),
        (Tool::Rect, "Rectangle (R)"),
        (Tool::Ellipse, "Ellipse (O)"),
    ];
    let tools_group3 = [
        (Tool::Text, "Text (T)"),
        (Tool::Callout, "Callout Bubble (C)"),
        (Tool::Step, "Step Counter (S)"),
        (Tool::Blur, "Blur / Pixelate (B)"),
        (Tool::Crop, "Crop / Trim (K)"),
    ];

    let tool_w = 29.0;
    let tool_h = 26.0;
    let div_w = 9.0;
    let n_tools = (tools_group1.len() + tools_group2.len() + tools_group3.len()) as f32;
    let track_w = 6.0 + n_tools * tool_w + 2.0 * div_w;
    let track_h = 32.0;

    let center_x = bar_rect.center().x;
    let track_rect = Rect::from_center_size(Pos2::new(center_x, cy), Vec2::new(track_w, track_h));
    paint_segmented_track(ui.painter(), track_rect, RADIUS_TRACK);

    let cur_tool = editor.tool();
    let mut tx = track_rect.left() + 3.0;
    let ty = track_rect.top() + 3.0;

    let mut clicked_tool = None;

    {
        let mut check_group = |ui: &mut Ui, group: &[(Tool, &str)], x: &mut f32| {
            for &(t, tip) in group {
                let r = Rect::from_min_size(Pos2::new(*x, ty), Vec2::new(tool_w, tool_h));
                let active = cur_tool == t;
                let id = ui.auto_id_with(("tb_tool", t));
                let resp = ui.interact(r, id, Sense::click()).on_hover_text(tip);
                paint_tool_segment(ui.painter(), r, active, resp.hovered());
                let icon_col = if active {
                    Color32::WHITE
                } else if resp.hovered() {
                    text_primary()
                } else {
                    text_secondary()
                };
                icons::draw_tool_icon(ui.painter(), r.center(), t, icon_col);
                if resp.clicked() {
                    clicked_tool = Some(t);
                }
                *x += tool_w;
            }
        };

        check_group(ui, &tools_group1, &mut tx);
        paint_bar_divider(ui.painter(), tx + div_w * 0.5, track_rect);
        tx += div_w;
        check_group(ui, &tools_group2, &mut tx);
        paint_bar_divider(ui.painter(), tx + div_w * 0.5, track_rect);
        tx += div_w;
        check_group(ui, &tools_group3, &mut tx);
    }

    if let Some(t) = clicked_tool {
        editor.set_tool(t);
    }

    res
}

fn paint_bar_divider(painter: &egui::Painter, x: f32, track: Rect) {
    let px = x.round() + 0.5;
    painter.line_segment(
        [
            Pos2::new(px, track.top() + 7.0),
            Pos2::new(px, track.bottom() - 7.0),
        ],
        Stroke::new(1.0, divider_vertical()),
    );
}

fn paint_tool_segment(painter: &egui::Painter, rect: Rect, active: bool, hovered: bool) {
    if active {
        let (top, bot) = crate::theme::tool_active_gradient();
        paint_rounded_v_gradient(painter, rect, RADIUS_PILL, top, bot);
        painter.rect_stroke(
            rect,
            RADIUS_PILL,
            Stroke::new(1.5, accent_blue()),
            StrokeKind::Inside,
        );
    } else if hovered {
        painter.rect_filled(rect, RADIUS_PILL, bg_pill_hover());
    }
}

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
    let resp = ui.interact(rect, id, sense).on_hover_text(tooltip);
    paint_pill_button(
        ui.painter(),
        rect,
        5.5,
        active,
        resp.hovered(),
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

fn chrome_button(
    ui: &mut Ui,
    id_salt: &str,
    rect: Rect,
    icon: ActionIcon,
    label: &str,
    primary_accent: bool,
    tooltip: &str,
) -> bool {
    let id = ui.auto_id_with(id_salt);
    let resp = ui.interact(rect, id, Sense::click()).on_hover_text(tooltip);
    if primary_accent {
        paint_pill_button(
            ui.painter(),
            rect,
            RADIUS_BTN,
            false,
            resp.hovered(),
            true,
            true,
        );
    } else {
        paint_secondary_button(ui.painter(), rect, RADIUS_BTN, resp.hovered(), true, false);
    }
    let fg = if primary_accent {
        Color32::WHITE
    } else {
        text_primary()
    };
    let icon_pos = Pos2::new(rect.left() + 14.0, rect.center().y);
    icons::draw_action_icon(ui.painter(), icon_pos, icon, fg);
    ui.painter().text(
        Pos2::new(rect.left() + 24.5, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.0),
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
}

pub fn render_properties_bar(ui: &mut Ui, editor: &mut Editor) -> bool {
    let mut mutated = false;
    let avail = ui.available_rect_before_wrap();
    let bar_rect = Rect::from_min_size(avail.min, Vec2::new(avail.width(), INSPECTOR_HEIGHT));
    ui.allocate_rect(bar_rect, Sense::hover());

    paint_chrome_bar(
        ui.painter(),
        bar_rect,
        bg_inspector_top(),
        bg_inspector_bot(),
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
                editor.style.text.clone(),
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
            editor.style.text.clone(),
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
            editor.style.text.clone(),
        ),
        Some(ObjectKind::Rect { stroke, fill, .. })
        | Some(ObjectKind::Ellipse { stroke, fill, .. }) => (
            ContextKind::Shape,
            stroke.rgb,
            stroke.width,
            *fill,
            editor.style.callout_shape,
            editor.style.arrow_head,
            editor.style.blur_mode,
            editor.style.blur_strength,
            editor.style.text.clone(),
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
            style.clone(),
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
            style.clone(),
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
            editor.style.text.clone(),
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
            editor.style.text.clone(),
        ),
        None => {
            let kind = match editor.tool() {
                Tool::Select => ContextKind::Generic,
                Tool::Pen | Tool::Highlighter | Tool::Line => ContextKind::StrokeOnly,
                Tool::Arrow => ContextKind::Arrow,
                Tool::Rect | Tool::Ellipse => ContextKind::Shape,
                Tool::Text => ContextKind::Text,
                Tool::Callout => ContextKind::Callout,
                Tool::Step => ContextKind::Step,
                Tool::Blur => ContextKind::Blur,
                Tool::Crop => ContextKind::Generic,
                Tool::SelectRegion => ContextKind::Generic,
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
                editor.style.text.clone(),
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
        ) {
            if editor.reorder(ZOrder::Front) {
                mutated = true;
            }
        }
        if icon_segment(
            ui,
            "ins_back",
            back_r,
            ActionIcon::LayerBack,
            false,
            true,
            "Send to Back",
        ) {
            if editor.reorder(ZOrder::Back) {
                mutated = true;
            }
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
            dup_resp.hovered(),
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
            del_resp.hovered(),
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
        let doc = editor.doc();
        let meta = format!(
            "{:.0} × {:.0} px   ·   {} {}",
            doc.view.w,
            doc.view.h,
            doc.objects.len(),
            if doc.objects.len() == 1 {
                "layer"
            } else {
                "layers"
            }
        );
        ui.painter().text(
            Pos2::new(inner.right(), cy),
            egui::Align2::RIGHT_CENTER,
            &meta,
            FontId::proportional(11.5),
            text_tertiary(),
        );
        165.0
    };

    // --- Left/Main Contextual Controls ---
    let mut x = inner.left();
    let max_x = inner.right() - right_reserved;

    // 1. Color Swatches (for everything except Blur)
    if ctx_kind != ContextKind::Blur {
        for (idx, &rgb) in PALETTE.iter().enumerate() {
            let sw_r = Rect::from_center_size(Pos2::new(x + 9.0, cy), Vec2::splat(20.0));
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
                7.8,
                rgb_to_color32(rgb),
                eff_rgb == rgb,
                resp.hovered(),
            );
            if resp.clicked() {
                editor.set_color(rgb);
                mutated = true;
            }
            x += 23.0;
        }
    }

    // 2. Stroke Width Capsule (with actual line-weight preview bars!)
    if matches!(
        ctx_kind,
        ContextKind::Generic
            | ContextKind::StrokeOnly
            | ContextKind::Shape
            | ContextKind::Arrow
            | ContextKind::Callout
    ) {
        if x > inner.left() {
            x = draw_ins_divider(ui.painter(), x + 4.0, cy) + 6.0;
        }
        let seg_w = 26.0;
        let track_w = 6.0 + SIZES.len() as f32 * seg_w;
        if x + track_w <= max_x {
            let track_r = Rect::from_min_size(Pos2::new(x, cy - 13.0), Vec2::new(track_w, 26.0));
            paint_segmented_track(ui.painter(), track_r, RADIUS_TRACK);
            for (i, &w) in SIZES.iter().enumerate() {
                let sr = Rect::from_min_size(
                    Pos2::new(track_r.left() + 3.0 + i as f32 * seg_w, track_r.top() + 3.0),
                    Vec2::new(seg_w, 20.0),
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
                    RADIUS_PILL,
                    active,
                    resp.hovered(),
                    true,
                    false,
                );
                let bar_h = match i {
                    0 => 1.6,
                    1 => 3.0,
                    _ => 4.6,
                };
                let col = if active || resp.hovered() {
                    text_primary()
                } else {
                    text_secondary()
                };
                ui.painter().line_segment(
                    [
                        Pos2::new(sr.left() + 6.0, sr.center().y),
                        Pos2::new(sr.right() - 6.0, sr.center().y),
                    ],
                    Stroke::new(bar_h, col),
                );
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
                    resp.hovered(),
                    true,
                    false,
                );
            } else {
                paint_secondary_button(ui.painter(), fr, RADIUS_PILL, resp.hovered(), true, false);
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
                FontId::proportional(11.5),
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
                } else {
                    Fill::Solid {
                        rgb: eff_rgb,
                        alpha: 0.25,
                    }
                };
                if ctx_kind == ContextKind::Callout {
                    editor.set_callout_fill_and_contrast(next_fill);
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
                    if icon_segment(
                        ui,
                        if i == 0 { "ins_ah0" } else { "ins_ah1" },
                        sr,
                        ic,
                        eff_arrow == hd,
                        true,
                        tip,
                    ) {
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
                    if icon_segment(
                        ui,
                        if i == 0 { "ins_bm0" } else { "ins_bm1" },
                        sr,
                        ic,
                        eff_blur_mode == md,
                        true,
                        tip,
                    ) {
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
                        resp.hovered(),
                        true,
                        false,
                    );
                    ui.painter().text(
                        sr.center(),
                        egui::Align2::CENTER_CENTER,
                        lbl,
                        FontId::proportional(11.0),
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
        ContextKind::Step => {
            x = draw_ins_divider(ui.painter(), x + 6.0, cy) + 6.0;
            let cur_start = editor.doc().step_start;
            let track_w = 88.0;
            if x + track_w <= max_x {
                ui.painter().text(
                    Pos2::new(x, cy),
                    egui::Align2::LEFT_CENTER,
                    "Start #",
                    FontId::proportional(11.5),
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
                    &cur_start.to_string(),
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
                resp.hovered(),
                true,
                false,
            );
            ui.painter().text(
                sr.center(),
                egui::Align2::CENTER_CENTER,
                format!("{:.0}", sz),
                FontId::proportional(11.0),
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

    // Bold toggle
    let bold_r = Rect::from_min_size(Pos2::new(x, cy - 13.0), Vec2::splat(26.0));
    if bold_r.right() <= max_x {
        let bold_id = ui.auto_id_with("ins_bold");
        let resp = ui
            .interact(bold_r, bold_id, Sense::click())
            .on_hover_text("Toggle Bold");
        if eff_text.bold {
            paint_pill_button(
                ui.painter(),
                bold_r,
                RADIUS_PILL,
                true,
                resp.hovered(),
                true,
                false,
            );
        } else {
            paint_secondary_button(
                ui.painter(),
                bold_r,
                RADIUS_PILL,
                resp.hovered(),
                true,
                false,
            );
        }
        ui.painter().text(
            bold_r.center(),
            egui::Align2::CENTER_CENTER,
            "B",
            FontId::new(13.0, FontFamily::Name("Bold".into())),
            if eff_text.bold || resp.hovered() {
                text_primary()
            } else {
                text_secondary()
            },
        );
        if resp.clicked() {
            editor.set_text_bold(!eff_text.bold);
            *mutated = true;
        }
        x += 26.0;
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
