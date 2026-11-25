//! Status bar — the 30px bottom strip (zoom, cursor position, dirty state).


use crate::{
    theme::{self},
    toolbar::ZoomCmd,
    EditorApp,
};
use eframe::egui::{
    self,
};

impl EditorApp {
    /// Bottom status bar — zoom, position, dirty state.
    pub(crate) fn status_bar(&mut self, ui: &mut egui::Ui) {
        // Bottom status bar (30px, `--panel` surface, single top hairline).
        let status_resp = egui::Panel::bottom("status_bar")
            .exact_size(30.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::bg_status())
                    .inner_margin(egui::Margin::symmetric(
                        qs_theme::space::MD as i8,
                        5,
                    )),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if crate::components::status_pill(
                        ui,
                        "Layers",
                        self.show_layers,
                        "Toggle Layers panel",
                    ) {
                        self.show_layers = !self.show_layers;
                    }
                    if crate::components::status_pill(
                        ui,
                        "Inspector",
                        self.show_inspector,
                        "Toggle Properties & Effects panel",
                    ) {
                        self.show_inspector = !self.show_inspector;
                    }
                    if crate::components::status_pill(
                        ui,
                        "Recent",
                        self.viewport.show_filmstrip,
                        "Toggle the recent captures strip (Ctrl+Shift+T)",
                    ) {
                        self.viewport.show_filmstrip = !self.viewport.show_filmstrip;
                    }
                    // `.dims` — extra margin-left like the mockup, no divider.
                    ui.add_space(8.0);
                    let view_r = self.viewport.view.display_rect();
                    let dim_resp = ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "{} × {} px",
                                    view_r.w.round() as i32,
                                    view_r.h.round() as i32
                                ))
                                .size(qs_theme::font::SMALL as f32)
                                .color(theme::text_secondary()),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_text(crate::components::platform_shortcut("Click to open Canvas & Resize (Ctrl+Shift+C)"));
                    if dim_resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if dim_resp.clicked() {
                        self.dialogs.canvas_dialog_open = !self.dialogs.canvas_dialog_open;
                        if self.dialogs.canvas_dialog_open {
                            self.show_inspector = true;
                        }
                        self.tools.canvas_background = self.doc.editor.doc().background;
                        self.tools.resize_w = self.doc.base.width().max(1);
                        self.tools.resize_h = self.doc.base.height().max(1);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Mockup `.right`: selInfo at the edge, cursorPos before it.
                        if let Some(b) = self.doc.editor.selection_bounds() {
                            let count = self.doc.editor.selection().len();
                            let label = if count > 1 {
                                format!(
                                    "Sel ({count}): {}×{}",
                                    b.w.round() as i32,
                                    b.h.round() as i32
                                )
                            } else {
                                format!("Sel: {}×{}", b.w.round() as i32, b.h.round() as i32)
                            };
                            ui.label(
                                egui::RichText::new(label)
                                    .size(qs_theme::font::SMALL as f32)
                                    .color(theme::text_tertiary()),
                            );
                            ui.add_space(14.0 - 6.0);
                        }
                        if let Some(p) = self.viewport.last_cursor_doc {
                            ui.label(
                                egui::RichText::new(format!(
                                    "X: {}  Y: {}",
                                    p.x.round() as i32,
                                    p.y.round() as i32
                                ))
                                .size(qs_theme::font::SMALL as f32)
                                .color(theme::text_tertiary()),
                            );
                            ui.add_space(14.0 - 6.0);
                        }
                        if crate::components::status_pill(
                            ui,
                            "?",
                            self.dialogs.shortcuts_open,
                            "Keyboard shortcuts (?)",
                        ) {
                            self.dialogs.shortcuts_open = !self.dialogs.shortcuts_open;
                        }
                        for (label, target_zoom) in [("200%", 2.0f32), ("100%", 1.0), ("50%", 0.5)]
                        {
                            let active = (self.viewport.view.zoom - target_zoom).abs() < 0.01;
                            if crate::components::status_pill(ui, label, active, "") {
                                if let Some(rect) = self.viewport.last_canvas_rect {
                                    self.viewport.view.zoom_at(
                                        target_zoom / self.viewport.view.zoom.max(0.01),
                                        rect.center(),
                                        rect,
                                    );
                                }
                            }
                        }
                        if crate::components::status_pill(ui, "Fit", false, "Fit to window") {
                            self.apply_zoom_cmd(ZoomCmd::Fit);
                        }
                    });
                });
            });
        crate::components::panel_divider(
            ui,
            status_resp.response.rect,
            crate::components::DividerEdge::Top,
        );

    }
}
