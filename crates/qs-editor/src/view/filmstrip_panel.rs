//! Filmstrip — the collapsible recent-captures strip above the status bar.


use crate::app::load_document;
use crate::{
    clipboard,
    icons::{self, ActionIcon},
    theme::{self},
    EditorApp,
};
use eframe::egui::{
    self, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2,
};
use std::path::PathBuf;

impl EditorApp {
    /// Collapsible Recent Captures filmstrip above the status bar.
    pub(crate) fn filmstrip_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        // Collapsible Recent Captures Filmstrip (sits cleanly above Status Bar)
        let film_t = ctx.animate_bool_with_time(
            egui::Id::unique("filmstrip_panel_t"),
            self.viewport.show_filmstrip,
            0.18,
        );
        if film_t > 0.01 {
            let mut switch_target: Option<PathBuf> = None;
            let mut copy_target: Option<(PathBuf, String)> = None;
            let mut refresh_tray = false;
            // Tall enough for the RECENT column (label + three pill buttons);
            // at 94 the thumbnails were clipped by the panel edge.
            egui::Panel::bottom("filmstrip_panel")
                .exact_size(124.0 * film_t)
                .frame(
                    egui::Frame::NONE
                        .fill(theme::bg_inspector_bot())
                        .stroke(egui::Stroke::new(1.0, theme::border_dark_hairline()))
                        .inner_margin(egui::Margin::symmetric(10, 6)),
                )
                .show(ui, |ui| {
                    // Keyboard route: arrows move a cursor, Space marks for
                    // Combine, Enter opens the marked set. Without this the tray
                    // was mouse-only.
                    if ui.ctx().memory(|m| m.focused().is_none()) {
                        let n = self.filmstrip.items.len();
                        let (left, right, space, enter) = ui.input(|i| {
                            (
                                i.key_pressed(egui::Key::ArrowLeft),
                                i.key_pressed(egui::Key::ArrowRight),
                                i.key_pressed(egui::Key::Space),
                                i.key_pressed(egui::Key::Enter),
                            )
                        });
                        let (focus, action) = crate::filmstrip_keys::tray_keys(
                            self.viewport.filmstrip_focus,
                            n,
                            self.viewport.tray_selected.len(),
                            crate::filmstrip_keys::TrayInput {
                                left,
                                right,
                                space,
                                enter,
                            },
                        );
                        self.viewport.filmstrip_focus = focus;
                        match action {
                            crate::filmstrip_keys::TrayAction::None => {}
                            crate::filmstrip_keys::TrayAction::ToggleMark(index) => {
                                if let Some(item) = self.filmstrip.items.get(index) {
                                    let path = item.path.clone();
                                    if let Some(pos) =
                                        self.viewport.tray_selected.iter().position(|p| p == &path)
                                    {
                                        self.viewport.tray_selected.remove(pos);
                                    } else {
                                        self.viewport.tray_selected.push(path);
                                    }
                                }
                            }
                            crate::filmstrip_keys::TrayAction::Open(index) => {
                                if let Some(item) = self.filmstrip.items.get(index) {
                                    switch_target = Some(item.path.clone());
                                }
                            }
                            crate::filmstrip_keys::TrayAction::OpenCombine => {
                                self.dialogs.combine_dialog_open = true;
                            }
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            // Wide enough for the "Combine (N)" pill.
                            ui.set_width(104.0);
                            ui.label(
                                egui::RichText::new("RECENT")
                                    .size(qs_theme::font::CAPTION as f32)
                                    .strong()
                                    .color(theme::text_tertiary()),
                            );
                            ui.add_space(qs_theme::space::XS as f32);
                            if ui
                                .small_button("Refresh")
                                .on_hover_text("Rescan recent captures (Shift+Click to multi-select)")
                                .clicked()
                            {
                                refresh_tray = true;
                            }
                            if ui
                                .small_button("Library")
                                .on_hover_text("Search, tag and prune every capture")
                                .clicked()
                            {
                                self.library.toggle();
                            }
                            if self.viewport.tray_selected.len() >= 2
                                && ui
                                    .small_button(format!("Combine ({})", self.viewport.tray_selected.len()))
                                    .on_hover_text("Combine selected captures into a template")
                                    .clicked()
                            {
                                self.dialogs.combine_dialog_open = true;
                            }
                        });
                        status_divider(ui);
                        if self.filmstrip.items.is_empty() {
                            ui.centered_and_justified(|ui| {
                                let msg = if self.filmstrip.scanning {
                                    "Scanning recent captures…"
                                } else {
                                    "No recent captures found in ~/Pictures/quickshort"
                                };
                                ui.label(
                                    egui::RichText::new(msg)
                                        .size(qs_theme::font::SMALL as f32)
                                        .color(theme::text_tertiary()),
                                );
                            });
                        } else {
                            egui::ScrollArea::horizontal()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 8.0;
                                        for (item_idx, item) in
                                            self.filmstrip.items.iter().enumerate()
                                        {
                                            let focused = self.viewport.filmstrip_focus == Some(item_idx);
                                            let is_active =
                                                self.doc.path.as_deref() == Some(item.path.as_path());
                                            let is_tray_sel = self
                                                .viewport.tray_selected
                                                .iter()
                                                .any(|p| p == &item.path);
                                            let is_dirty = is_active && self.doc.unsaved
                                                || self
                                                    .doc.session_cache
                                                    .get(&item.path)
                                                    .is_some_and(|s| s.unsaved);
                                            let (rect, resp) = ui.allocate_exact_size(
                                                Vec2::new(108.0, 78.0),
                                                egui::Sense::click(),
                                            );
                                            let bg = if is_active {
                                                theme::bg_pill_active_bot()
                                            } else if resp.hovered() {
                                                theme::bg_btn_secondary_hover_bot()
                                            } else {
                                                theme::bg_card()
                                            };
                                            let border = if is_tray_sel {
                                                Stroke::new(1.5, theme::success_green())
                                            } else if is_active {
                                                Stroke::new(1.5, theme::accent_blue())
                                            } else if resp.hovered() {
                                                Stroke::new(1.0, theme::border_hot())
                                            } else {
                                                Stroke::new(1.0, theme::border_subtle())
                                            };
                                            ui.painter().rect(
                                                rect,
                                                theme::RADIUS_PILL,
                                                bg,
                                                border,
                                                StrokeKind::Inside,
                                            );
                                            if focused {
                                                ui.painter().rect_stroke(
                                                    rect,
                                                    theme::RADIUS_PILL,
                                                    egui::Stroke::new(2.0, theme::accent_blue()),
                                                    StrokeKind::Outside,
                                                );
                                            }
                                            if let Some(tex) = &item.texture {
                                                let avail = rect.shrink2(Vec2::new(6.0, 14.0));
                                                let img_rect = Rect::from_min_max(
                                                    Pos2::new(avail.left(), rect.top() + 5.0),
                                                    Pos2::new(avail.right(), rect.bottom() - 18.0),
                                                );
                                                let tsz = tex.size_vec2();
                                                let s = (img_rect.width() / tsz.x.max(1.0))
                                                    .min(img_rect.height() / tsz.y.max(1.0));
                                                let fit = Rect::from_center_size(
                                                    img_rect.center(),
                                                    tsz * s,
                                                );
                                                ui.painter().image(
                                                    tex.id(),
                                                    fit,
                                                    Rect::from_min_max(
                                                        Pos2::ZERO,
                                                        Pos2::new(1.0, 1.0),
                                                    ),
                                                    Color32::WHITE,
                                                );
                                            }
                                            let mut name = item.label.clone();
                                            if name.len() > 14 {
                                                name.truncate(11);
                                                name.push('…');
                                            }
                                            ui.painter().text(
                                                Pos2::new(rect.center().x, rect.bottom() - 9.0),
                                                egui::Align2::CENTER_CENTER,
                                                &name,
                                                FontId::proportional(qs_theme::font::CAPTION as f32),
                                                if is_active {
                                                    theme::text_primary()
                                                } else {
                                                    theme::text_secondary()
                                                },
                                            );
                                            if is_dirty {
                                                ui.painter().circle_filled(
                                                    Pos2::new(rect.right() - 7.0, rect.top() + 7.0),
                                                    3.5,
                                                    theme::amber_orange(),
                                                );
                                            }
                                            // Quick Copy button on hover (top-left of thumbnail)
                                            if resp.hovered() {
                                                let copy_r = Rect::from_min_size(
                                                    Pos2::new(rect.left() + 4.0, rect.top() + 4.0),
                                                    Vec2::new(20.0, 20.0),
                                                );
                                                let copy_resp = ui
                                                    .interact(
                                                        copy_r,
                                                        ui.auto_id_with(("tray_cp", item_idx)),
                                                        egui::Sense::click(),
                                                    )
                                                    .on_hover_text("Copy image to clipboard");
                                                ui.painter().rect_filled(
                                                    copy_r,
                                                    4.0,
                                                    if copy_resp.hovered() {
                                                        theme::accent_blue()
                                                    } else {
                                                        Color32::from_black_alpha(150)
                                                    },
                                                );
                                                icons::draw_action_icon(
                                                    ui.painter(),
                                                    copy_r.center(),
                                                    ActionIcon::Copy,
                                                    Color32::WHITE,
                                                );
                                                if copy_resp.clicked() {
                                                    copy_target = Some((item.path.clone(), item.label.clone()));
                                                }
                                            }
                                            if resp
                                                .on_hover_text(format!(
                                                    "{} (Shift+Click to select for Combine)",
                                                    item.label
                                                ))
                                                .clicked()
                                            {
                                                if ui.input(|i| i.modifiers.shift) {
                                                    if let Some(pos) = self
                                                        .viewport.tray_selected
                                                        .iter()
                                                        .position(|p| p == &item.path)
                                                    {
                                                        self.viewport.tray_selected.remove(pos);
                                                    } else {
                                                        self.viewport.tray_selected.push(item.path.clone());
                                                    }
                                                } else {
                                                    switch_target = Some(item.path.clone());
                                                }
                                            }
                                        }
                                    });
                                });
                        }
                    });
                });
            if refresh_tray {
                self.filmstrip.refresh(self.doc.path.as_deref());
            }
            if let Some((path, label)) = copy_target {
                if let Ok((doc, base, _)) = load_document(&path) {
                    let flat = qs_render::render(&doc, &base);
                    if clipboard::copy_image(&flat).is_ok() {
                        self.show_toast(format!("Copied {label}"));
                    }
                }
            }
            if let Some(target) = switch_target {
                if let Err(e) = self.switch_to_path(&target) {
                    self.show_toast(format!("Failed to open: {e}"));
                }
            }
        }

    }
}

/// Hairline group divider for the status bar.
///
/// `ui.separator()` inside a dense bar inherits the widget stroke and the
/// surrounding item spacing, which made the bar's dividers heavier than the
/// ones elsewhere. This draws a short token-coloured line with token gaps.
fn status_divider(ui: &mut egui::Ui) {
    ui.add_space(qs_theme::space::XS as f32);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(1.0, qs_theme::space::LG as f32),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, theme::divider_vertical());
    ui.add_space(qs_theme::space::XS as f32);
}

