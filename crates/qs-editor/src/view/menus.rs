//! Menus — app menu, export menu and the canvas right-click menu.


use crate::app::spawn_app;
use crate::{
    state::ExportFormat,
    theme::{self},
    toolbar::ZoomCmd,
    EditorApp,
};
use eframe::egui::{
    self, Rect, Vec2,
};

impl EditorApp {
    /// App menu at the left of the top bar: the actions that used to crowd
    /// the status bar, which now only reports.
    pub(crate) fn app_menu(&mut self, ctx: &egui::Context) {
        if !self.dialogs.app_menu_open {
            return;
        }
        let anchor = self.dialogs.app_menu_anchor.unwrap_or_else(|| {
            let r = ctx.content_rect();
            Rect::from_min_size(r.min, Vec2::new(1.0, 36.0))
        });
        let is_dark = qs_theme::pal().dark;
        let has_region = self.doc.editor.region().is_some();
        let (mut capture, mut library, mut copy_text, mut canvas, mut settings, mut flip_theme, mut close) =
            (false, false, false, false, false, false, false);
        let area = egui::Area::new(egui::Id::unique("app_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor.left_bottom() + Vec2::new(0.0, qs_theme::space::XS as f32))
            .show(ctx, |ui| {
                theme::popover_frame().show(ui, |ui| {
                    ui.set_width(224.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    capture = crate::components::menu_item_with_icon(
                        ui,
                        qs_icons::PLUS,
                        "New capture",
                        Some("Ctrl+N"),
                        true,
                        false,
                    );
                    library = crate::components::menu_item_with_icon(
                        ui,
                        qs_icons::GRID_2X2,
                        "Library",
                        Some("Ctrl+L"),
                        true,
                        false,
                    );
                    copy_text = crate::components::menu_item_with_icon(
                        ui,
                        qs_icons::SCAN_LINE,
                        "Copy text in selection",
                        None,
                        has_region,
                        false,
                    );
                    canvas = crate::components::menu_item_with_icon(
                        ui,
                        qs_icons::PROPORTIONS,
                        "Canvas & Resize\u{2026}",
                        Some("Ctrl+Shift+C"),
                        true,
                        false,
                    );
                    ui.separator();
                    let theme_label = if is_dark {
                        "Switch to Light theme"
                    } else {
                        "Switch to Dark theme"
                    };
                    flip_theme = crate::components::menu_item_with_icon(
                        ui,
                        qs_icons::SUN,
                        theme_label,
                        None,
                        true,
                        false,
                    );
                    settings = crate::components::menu_item_with_icon(
                        ui,
                        qs_icons::SETTINGS,
                        "Settings\u{2026}",
                        Some("Ctrl+,"),
                        true,
                        false,
                    );
                    ui.separator();
                    close = crate::components::menu_item_with_icon(
                        ui,
                        qs_icons::X,
                        "Close",
                        Some("Ctrl+W"),
                        true,
                        true,
                    );
                });
            });
        let menu_rect = area.response.rect;
        let clicked_outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i
                    .pointer
                    .interact_pos()
                    .is_some_and(|p| !menu_rect.contains(p) && !anchor.contains(p))
        });
        if capture || library || copy_text || canvas || settings || flip_theme || close || clicked_outside {
            self.dialogs.app_menu_open = false;
        }
        if close {
            self.request_close(ctx);
        }
        if capture {
            spawn_app("--capture");
        }
        if library && !self.library.open {
            self.library.toggle();
        }
        if copy_text {
            self.copy_text_in_region();
        }
        if canvas {
            self.dialogs.canvas_dialog_open = true;
            self.show_inspector = true;
            self.tools.canvas_background = self.doc.editor.doc().background;
            self.tools.resize_w = self.doc.base.width().max(1);
            self.tools.resize_h = self.doc.base.height().max(1);
        }
        if settings {
            spawn_app("--settings");
        }
        if flip_theme {
            let next = if is_dark { "light" } else { "dark" };
            match crate::prefs::AppPrefs::save_theme(next) {
                Ok(()) => {
                    self.prefs.theme = next.into();
                    qs_theme::set_theme(next);
                    theme::sync_egui_visuals(ctx);
                }
                Err(error) => self.show_toast(format!("Could not save theme: {error}")),
            }
        }
    }


    /// Export is a dropdown under the Export button: one click picks the
    /// format and the OS save dialog is the only window that follows.
    pub(crate) fn export_menu(&mut self, ctx: &egui::Context) {
        if !self.dialogs.export_menu_open {
            return;
        }
        let anchor = self.dialogs.export_anchor.unwrap_or_else(|| {
            let r = ctx.content_rect();
            Rect::from_min_size(egui::pos2(r.right() - 220.0, r.top()), Vec2::new(1.0, 36.0))
        });
        let mut pick: Option<ExportFormat> = None;
        let (mut quick_png, mut copy, mut print) = (false, false, false);
        let area = egui::Area::new(egui::Id::unique("export_menu"))
            .order(egui::Order::Foreground)
            // right-aligned under Export, as macOS toolbar menus are, so it
            // keeps clear of the window edge
            .pivot(egui::Align2::RIGHT_TOP)
            .fixed_pos(anchor.right_bottom() + Vec2::new(0.0, qs_theme::space::XS as f32))
            .show(ctx, |ui| {
                theme::popover_frame().show(ui, |ui| {
                    ui.set_width(224.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if crate::components::menu_item(
                        ui,
                        "Quick export PNG",
                        Some("Ctrl+E"),
                        false,
                        true,
                        false,
                    ) {
                        quick_png = true;
                    }
                    ui.separator();
                    for fmt in [ExportFormat::Png, ExportFormat::Jpeg, ExportFormat::Webp] {
                        let label = format!("Save as {}\u{2026}", fmt.label());
                        if crate::components::menu_item(
                            ui,
                            &label,
                            None,
                            fmt == self.tools.export_format,
                            true,
                            false,
                        ) {
                            pick = Some(fmt);
                        }
                    }
                    ui.separator();
                    if crate::components::menu_item(
                        ui,
                        "Copy image",
                        Some("Ctrl+C"),
                        false,
                        true,
                        false,
                    ) {
                        copy = true;
                    }
                    if crate::components::menu_item(
                        ui,
                        "Print\u{2026}",
                        None,
                        false,
                        true,
                        false,
                    ) {
                        print = true;
                    }
                });
            });
        let menu_rect = area.response.rect;
        let clicked_outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i
                    .pointer
                    .interact_pos()
                    .is_some_and(|p| !menu_rect.contains(p) && !anchor.contains(p))
        });
        if quick_png || pick.is_some() || copy || print || clicked_outside {
            self.dialogs.export_menu_open = false;
        }
        if quick_png {
            if let Err(e) = self.export_png(None) {
                self.show_toast(format!("Export failed: {e}"));
            }
        } else if let Some(fmt) = pick {
            if let Err(e) = self.export_with_dialog(fmt, self.tools.export_quality) {
                self.show_toast(format!("Export failed: {e}"));
            }
        } else if copy {
            let _ = self.copy_to_clipboard();
        } else if print {
            if let Err(e) = self.print_image() {
                self.show_toast(format!("Print failed: {e}"));
            }
        }
    }


    /// Right-click context menu on the canvas (object actions or viewport actions).
    pub(crate) fn canvas_context_menu(&mut self, ctx: &egui::Context) {
        let Some((pos, hit_id)) = self.dialogs.context_menu else {
            return;
        };
        let mut close = false;
        let has_sel = hit_id.is_some() || !self.doc.editor.selection().is_empty();
        let multi = self.doc.editor.selection().len() >= 2;
        let grouped = self.doc.editor.selection_grouped();
        let area = egui::Area::new(egui::Id::unique("canvas_ctx_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                theme::popover_frame().show(ui, |ui| {
                    ui.set_width(210.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if has_sel {
                        if crate::components::menu_item(ui, "Duplicate", Some("Ctrl+D"), false, true, false) {
                            if self.doc.editor.duplicate_selected().is_some() {
                                self.touch();
                            }
                            close = true;
                        }
                        if crate::components::menu_item(ui, "Bring to Front", None, false, true, false) {
                            if self.doc.editor.reorder(qs_edit::ZOrder::Front) {
                                self.touch();
                            }
                            close = true;
                        }
                        if crate::components::menu_item(ui, "Send to Back", None, false, true, false) {
                            if self.doc.editor.reorder(qs_edit::ZOrder::Back) {
                                self.touch();
                            }
                            close = true;
                        }
                        if multi {
                            ui.separator();
                            if crate::components::menu_item(ui, "Group", Some("Ctrl+G"), false, true, false) {
                                self.doc.editor.group_selected();
                                close = true;
                            }
                        }
                        if grouped
                            && crate::components::menu_item(ui, "Ungroup", Some("Ctrl+Shift+G"), false, true, false) {
                                self.doc.editor.ungroup_selected();
                                close = true;
                            }
                        ui.separator();
                        if crate::components::menu_item(ui, "Delete", Some("Del"), false, true, true) {
                            if self.doc.editor.delete_selected() {
                                self.show_toast("Deleted");
                                self.touch();
                            }
                            close = true;
                        }
                    } else {
                        if crate::components::menu_item(ui, "Copy image", Some("Ctrl+C"), false, true, false) {
                            let _ = self.copy_to_clipboard();
                            close = true;
                        }
                        if crate::components::menu_item(ui, "Fit to window", Some("Ctrl+0"), false, true, false) {
                            self.apply_zoom_cmd(ZoomCmd::Fit);
                            close = true;
                        }
                        if crate::components::menu_item(ui, "Zoom 100%", Some("Ctrl+1"), false, true, false) {
                            self.apply_zoom_cmd(ZoomCmd::Set(1.0));
                            close = true;
                        }
                        ui.separator();
                        if crate::components::menu_item(ui, "Canvas & Resize…", Some("Ctrl+Shift+C"), false, true, false) {
                            self.dialogs.canvas_dialog_open = true;
                            self.show_inspector = true;
                            self.tools.canvas_background = self.doc.editor.doc().background;
                            self.tools.resize_w = self.doc.base.width().max(1);
                            self.tools.resize_h = self.doc.base.height().max(1);
                            close = true;
                        }
                        if crate::components::menu_item(ui, "Find text (OCR)…", Some("Ctrl+F"), false, true, false) {
                            self.find.open = true;
                            self.find.focus_pending = true;
                            close = true;
                        }
                    }
                });
            });
        let r = area.response.rect;
        let outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer.interact_pos().is_some_and(|p| !r.contains(p))
        });
        if close || outside {
            self.dialogs.context_menu = None;
        }
    }

}
