//! View layer — egui panels, menus, dialogs and canvas rendering for
//! `EditorApp`. Sibling of `app.rs` (state + actions) and `input.rs`
//! (keys); methods here stay `pub(crate)` for cross-module calls.

use crate::{
    recovery,
    styles,
    theme::{self},
    toolbar::{self, ZoomCmd},
    EditorApp,
};
use eframe::egui::{
    self,
};

mod canvas_panel;
mod dialogs;
mod filmstrip_panel;
mod inspector_panel;
mod layers_panel;
mod menus;
mod status_bar;

impl EditorApp {    pub(crate) fn render_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.refresh_preferences(&ctx);

        self.recovery_guard(&ctx);
        self.close_guard(&ctx);
        self.filmstrip.poll(&ctx);
        self.handle_dialog_keys(&ctx);
        self.handle_text_events(&ctx);
        self.handle_shortcuts(&ctx);

        let top_res = egui::Panel::top("top_bar")
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                toolbar::render_top_bar(
                    ui,
                    &mut self.doc.editor,
                    self.viewport.view.zoom,
                    self.doc.path.as_deref(),
                )
            })
            .inner;

        egui::Panel::top("tool_palette")
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                toolbar::render_tool_palette(ui, &mut self.doc.editor)
            });

        let prop_mutated = egui::Panel::top("properties_bar")
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                toolbar::render_properties_bar(ui, &mut self.doc.editor)
            })
            .inner;

        if top_res.mutated || prop_mutated {
            self.touch();
        }
        // Only show Quick Styles bar when the user has saved custom presets for
        // the active tool — hides the redundant 8-color bar that duplicated the
        // swatches already shown in the Properties bar right above.
        let styles_mutated = if !self.styles.for_tool(self.doc.editor.tool()).is_empty() {
            egui::Panel::top("quick_style_bar")
                // Without an explicit fill the panel rendered as a black strip
                // in the light theme.
                .frame(
                    egui::Frame::NONE
                        .fill(theme::bg_inspector_bot())
                        .stroke(egui::Stroke::new(1.0, theme::border_subtle()))
                        .inner_margin(egui::Margin::symmetric(
                            qs_theme::space::MD as i8,
                            qs_theme::space::XS as i8,
                        )),
                )
                .show(ui, |ui| {
                    styles::render_bar(
                        ui,
                        &mut self.doc.editor,
                        &mut self.styles,
                        &mut self.tools.style_filter,
                    )
                })
                .inner
        } else {
            false
        };
        if styles_mutated {
            self.touch();
        }
        if top_res.copy {
            let _ = self.copy_to_clipboard();
        }
        if top_res.save {
            let _ = self.save();
        }
        if top_res.save_as {
            let _ = self.save_as();
        }
        if top_res.open {
            let _ = self.open_dialog();
        }
        if top_res.export_rect.is_some() {
            self.dialogs.export_anchor = top_res.export_rect;
        }
        if top_res.export {
            self.dialogs.export_menu_open = !self.dialogs.export_menu_open;
            self.dialogs.app_menu_open = false;
        }
        if top_res.menu_rect.is_some() {
            self.dialogs.app_menu_anchor = top_res.menu_rect;
        }
        if top_res.menu {
            self.dialogs.app_menu_open = !self.dialogs.app_menu_open;
            self.dialogs.export_menu_open = false;
        }
        if top_res.close {
            self.request_close(&ctx);
        }
        if top_res.canvas {
            self.dialogs.canvas_dialog_open = !self.dialogs.canvas_dialog_open;
            if self.dialogs.canvas_dialog_open {
                self.show_inspector = true;
            }
            self.tools.canvas_background = self.doc.editor.doc().background;
            self.tools.resize_w = self.doc.base.width().max(1);
            self.tools.resize_h = self.doc.base.height().max(1);
        }
        if top_res.zoom != ZoomCmd::None {
            self.apply_zoom_cmd(top_res.zoom);
        }
        self.export_menu(&ctx);
        self.app_menu(&ctx);
        self.combine_dialog(&ctx);
        self.find_dialog(&ctx);
        self.canvas_context_menu(&ctx);
        self.shortcuts_dialog(&ctx);

        self.status_bar(ui);

        self.filmstrip_panel(&ctx, ui);

        self.layers_panel(&ctx, ui);

        self.inspector_panel(&ctx, ui);

        self.canvas_panel(&ctx, ui);
        self.sync_unsaved();
        self.autosave_tick();
        if self.doc.unsaved {
            // A second edit inside the throttle interval must be recovered
            // even when no more input events wake this window.
            ctx.request_repaint_after(recovery::AUTOSAVE_INTERVAL);
        }
        self.sync_window_title(&ctx);
    }
}

pub(crate) fn anchored_insets(vertical: f32, horizontal: f32, anchor: (usize, usize)) -> [f32; 4] {
    let distribute = |total: f32, position: usize| match position {
        0 => (0.0, total),
        1 => (total * 0.5, total - total * 0.5),
        _ => (total, 0.0),
    };
    let (top, bottom) = distribute(vertical, anchor.1);
    let (left, right) = distribute(horizontal, anchor.0);
    [top, right, bottom, left]
}

