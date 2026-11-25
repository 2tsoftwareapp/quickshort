//! Tier 1: unified header toolbar (46px) — app actions, file ops, zoom.

use crate::{
    icons::{self, ActionIcon},
    theme::*,
};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use qs_edit::Editor;
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
