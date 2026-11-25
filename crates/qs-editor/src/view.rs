//! View layer — egui panels, menus, dialogs and canvas rendering for
//! `EditorApp`. Sibling of `app.rs` (state + actions) and `input.rs`
//! (keys); methods here stay `pub(crate)` for cross-module calls.

use crate::app::{load_document, spawn_app};
use crate::{
    canvas::{self},
    clipboard,
    icons::{self, ActionIcon},
    image_panel,
    recovery,
    state::ExportFormat,
    styles,
    templates::CombineLayout,
    theme::{self, accent_blue, success_green, text_primary},
    toolbar::{self, ZoomCmd},
    EditorApp,
};
use eframe::egui::{
    self, Color32, CursorIcon, FontId, Key, Pos2, Rect, Stroke, StrokeKind, Vec2,
};
use qs_doc::{PointF, RectF};
use qs_edit::{Alignment, Axis, DragHint, HandleId, Tool};
use std::path::{Path, PathBuf};

impl EditorApp {
    /// Canvas settings, drawn as a card at the top of the Inspector.
    pub(crate) fn canvas_section(&mut self, ui: &mut egui::Ui) {
        let mut apply = false;
        let mut fit_to_image = false;
        let mut fit_to_objects = false;
        let mut trim = false;
        let mut resample = false;
        {
            {
                ui.label("Canvas insets (px)");
                egui::Grid::new("canvas_insets")
                    .num_columns(2)
                    .spacing([
                        qs_theme::space::SM as f32,
                        qs_theme::space::XS as f32,
                    ])
                    .show(ui, |ui| {
                        for (label, index) in [("Top", 0), ("Right", 1), ("Bottom", 2), ("Left", 3)]
                        {
                            ui.label(label);
                            ui.add(
                                egui::DragValue::new(&mut self.tools.canvas_insets[index])
                                    .speed(1.0)
                                    .range(-100_000.0..=100_000.0),
                            );
                            ui.end_row();
                        }
                    });
                ui.label("Anchor");
                // Drawn grid: the arrow glyphs (↖ ↑ ↗ …) are absent from the
                // bundled fonts, so the picker rendered as one empty square.
                let cell = qs_theme::space::XL as f32;
                for row in 0..3usize {
                    ui.horizontal(|ui| {
                        for column in 0..3usize {
                            let (r, resp) =
                                ui.allocate_exact_size(Vec2::splat(cell), egui::Sense::click());
                            let active = self.tools.canvas_anchor == (column, row);
                            let painter = ui.painter();
                            painter.rect_filled(
                                r,
                                egui::CornerRadius::same(4),
                                if active {
                                    theme::accent_alpha(60)
                                } else {
                                    theme::bg_card()
                                },
                            );
                            painter.rect_stroke(
                                r,
                                egui::CornerRadius::same(4),
                                egui::Stroke::new(
                                    1.0,
                                    if active || resp.hovered() {
                                        theme::border_hot()
                                    } else {
                                        theme::border_subtle()
                                    },
                                ),
                                egui::StrokeKind::Inside,
                            );
                            painter.circle_filled(
                                r.center(),
                                2.6,
                                if active {
                                    theme::accent_blue()
                                } else {
                                    theme::text_disabled()
                                },
                            );
                            if resp.clicked() {
                                let vertical = self.tools.canvas_insets[0] + self.tools.canvas_insets[2];
                                let horizontal = self.tools.canvas_insets[1] + self.tools.canvas_insets[3];
                                self.tools.canvas_insets =
                                    anchored_insets(vertical, horizontal, (column, row));
                                self.tools.canvas_anchor = (column, row);
                            }
                        }
                    });
                }
                ui.label("Negative values crop that edge.");

                let [a, r, g, b] = self.tools.canvas_background.to_be_bytes();
                let mut color = Color32::from_rgba_unmultiplied(r, g, b, a);
                ui.horizontal(|ui| {
                    ui.label("Canvas color");
                    ui.color_edit_button_srgba(&mut color);
                });
                let [r, g, b, a] = color.to_srgba_unmultiplied();
                self.tools.canvas_background = u32::from_be_bytes([a, r, g, b]);
                ui.horizontal_wrapped(|ui| {
                    if crate::components::pill_button(ui, "Transparent", "") {
                        self.tools.canvas_background = 0;
                    }
                    if crate::components::pill_button(ui, "White", "") {
                        self.tools.canvas_background = u32::MAX;
                    }
                    if crate::components::pill_button(ui, "Black", "") {
                        self.tools.canvas_background = 255 << 24;
                    }
                });

                ui.separator();
                // Right-to-left inside a horizontal row let the left group run
                // under Apply at narrow widths.
                ui.horizontal_wrapped(|ui| {
                    if crate::components::pill_button(ui, "Trim edges", "") {
                        trim = true;
                    }
                    if crate::components::pill_button(ui, "Fit to image", "") {
                        fit_to_image = true;
                    }
                    if crate::components::pill_button(ui, "Fit to objects", "") {
                        fit_to_objects = true;
                    }
                });
                ui.add_space(qs_theme::space::SM as f32);
                crate::components::dialog_actions(ui, |ui| {
                    if crate::components::pill_button(ui, "Apply", "Apply the canvas insets") {
                        apply = true;
                    }
                });
                ui.add_space(qs_theme::space::SM as f32);
                ui.add_space(qs_theme::space::SM as f32);
                ui.separator();
                ui.label("Resample image (px)");
                ui.horizontal_wrapped(|ui| {
                    let (bw, bh) = (self.doc.base.width().max(1), self.doc.base.height().max(1));
                    let aspect = bw as f32 / bh as f32;
                    let w_resp = ui.add(
                        egui::DragValue::new(&mut self.tools.resize_w)
                            .range(1..=16384)
                            .prefix("W: "),
                    );
                    if w_resp.changed() && self.tools.resize_lock {
                        self.tools.resize_h = ((self.tools.resize_w as f32 / aspect).round() as u32).max(1);
                    }
                    let h_resp = ui.add(
                        egui::DragValue::new(&mut self.tools.resize_h)
                            .range(1..=16384)
                            .prefix("H: "),
                    );
                    if h_resp.changed() && self.tools.resize_lock {
                        self.tools.resize_w = ((self.tools.resize_h as f32 * aspect).round() as u32).max(1);
                    }
                    ui.checkbox(&mut self.tools.resize_lock, "Lock");
                    if crate::components::pill_button(ui, "Resize", "") {
                        resample = true;
                    }
                });
            }
        }

        if resample {
            let nw = self.tools.resize_w.clamp(1, 16384);
            let nh = self.tools.resize_h.clamp(1, 16384);
            if nw != self.doc.base.width() || nh != self.doc.base.height() {
                let resized =
                    image::imageops::resize(&self.doc.base, nw, nh, image::imageops::FilterType::Lanczos3);
                let sx = nw as f32 / self.doc.base.width().max(1) as f32;
                let sy = nh as f32 / self.doc.base.height().max(1) as f32;
                let mut doc = self.doc.editor.doc().clone();
                doc.base_size = qs_doc::SizeF::new(nw as f32 / doc.scale, nh as f32 / doc.scale);
                doc.view = RectF::new(
                    doc.view.x * sx,
                    doc.view.y * sy,
                    doc.view.w * sx,
                    doc.view.h * sy,
                );
                self.apply_document(doc, resized);
                self.doc.editor.mark_modified();
                self.touch();
                self.show_toast(format!("Resized image to {nw}×{nh}"));
            }
            self.dialogs.canvas_dialog_open = false;
        } else if trim {
            if self.doc.editor.trim_to_content(&self.doc.base, 6) {
                self.touch();
                self.dialogs.canvas_dialog_open = false;
            } else {
                self.show_toast("No safe uniform border to trim");
            }
        } else if fit_to_image {
            let view = self.doc.editor.doc().view;
            let base = self.doc.editor.doc().base_size;
            let top = view.y;
            let left = view.x;
            let right = base.w - view.w - left;
            let bottom = base.h - view.h - top;
            if self
                .doc.editor
                .update_canvas(top, right, bottom, left, self.tools.canvas_background)
            {
                self.touch();
            }
            self.tools.canvas_insets = [0.0; 4];
            self.dialogs.canvas_dialog_open = false;
        } else if fit_to_objects {
            if self
                .doc.editor
                .fit_canvas_to_objects_with_background(self.tools.canvas_background)
            {
                self.touch();
            }
            self.tools.canvas_insets = [0.0; 4];
            self.dialogs.canvas_dialog_open = false;
        } else if apply {
            // the dialog's edges are the displayed ones
            let insets = self
                .doc.editor
                .doc()
                .orientation
                .insets_to_input(self.tools.canvas_insets);
            let [top, right, bottom, left] = insets;
            if self
                .doc.editor
                .update_canvas(top, right, bottom, left, self.tools.canvas_background)
            {
                self.touch();
            }
            self.tools.canvas_insets = [0.0; 4];
            self.dialogs.canvas_dialog_open = false;
        }
    }

    /// Align & distribute, drawn as a card in the Inspector while two or
    /// more objects are selected.
    pub(crate) fn align_section(&mut self, ui: &mut egui::Ui) {
                    // One row per axis: wrapping all six mixed them up at 720.
                    ui.horizontal_wrapped(|ui| {
                        for (label, a) in [
                            ("Left", Alignment::Left),
                            ("Center X", Alignment::HCenter),
                            ("Right", Alignment::Right),
                        ] {
                            if crate::components::pill_button(ui, label, "")
                                && self.doc.editor.align_selected(a) {
                                    self.touch();
                                }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (label, a) in [
                            ("Top", Alignment::Top),
                            ("Center Y", Alignment::VCenter),
                            ("Bottom", Alignment::Bottom),
                        ] {
                            if crate::components::pill_button(ui, label, "")
                                && self.doc.editor.align_selected(a) {
                                    self.touch();
                                }
                        }
                    });
                    ui.separator();
                    ui.horizontal_wrapped(|ui| {
                        let enabled = self.doc.editor.selection().len() >= 3;
                        ui.add_enabled_ui(enabled, |ui| {
                            if crate::components::pill_button(ui, "Distribute horizontally", "")
                                && self.doc.editor.distribute_selected(Axis::Horizontal)
                            {
                                self.touch();
                            }
                            if crate::components::pill_button(ui, "Distribute vertically", "")
                                && self.doc.editor.distribute_selected(Axis::Vertical)
                            {
                                self.touch();
                            }
                        });
                    });
    }

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

    /// Find in capture: the window paints, this owns the matching and panning.
    pub(crate) fn find_dialog(&mut self, ctx: &egui::Context) {
        if !self.find.open {
            return;
        }
        let hits = self.ocr_hints.find(&self.find.query);
        match crate::find::render(
            ctx,
            &mut self.find,
            hits.len(),
            self.ocr_hints.is_scanning(),
        ) {
            crate::find::FindAction::None => {}
            crate::find::FindAction::Close => {
                self.find.open = false;
                self.last_find_focus = None;
            }
            crate::find::FindAction::Step(step) => {
                if !hits.is_empty() {
                    let count = hits.len() as i32;
                    let current = self.find.index.min(hits.len() - 1) as i32;
                    self.find.index = (current + step).rem_euclid(count) as usize;
                }
            }
            crate::find::FindAction::RedactAll => {
                // Word-level where the engine reports it — only the matched
                // text is covered, not the whole line around it.
                let rects: Vec<RectF> = self
                    .ocr_hints
                    .find_rects(&self.find.query)
                    .into_iter()
                    .flatten()
                    .collect();
                let n = self.doc.editor.add_redactions(&rects, qs_doc::BlurMode::Solid);
                if n > 0 {
                    self.touch();
                    self.show_toast(format!("Redacted {n} match(es)"));
                }
            }
        }

        // Centre the current match once per change. Re-centring every frame
        // would make the canvas impossible to pan while the dialog is open.
        let hits = self.ocr_hints.find(&self.find.query);
        if hits.is_empty() {
            return;
        }
        let idx = self.find.index.min(hits.len() - 1);
        let focus = (self.find.query.clone(), idx);
        if self.last_find_focus.as_ref() == Some(&focus) {
            return;
        }
        self.last_find_focus = Some(focus);
        let line = &self.ocr_hints.lines()[hits[idx]];
        let centre = PointF::new(
            line.rect.x + line.rect.w / 2.0,
            line.rect.y + line.rect.h / 2.0,
        );
        self.viewport.view.center = self.viewport.view.map.to_display(centre);
    }

    /// Library view: keep it pointed at the capture folder and apply the one
    /// action it can request (open, or reveal in the file manager).
    pub(crate) fn library_view(&mut self, ui: &mut egui::Ui) {
        self.library.set_dir(self.prefs.capture_dir());
        match self.library.show(ui) {
            Some(crate::library::LibraryAction::Open(path)) => {
                if let Err(e) = self.switch_to_path(&path) {
                    self.show_toast(format!("Failed to open: {e}"));
                } else {
                    self.library.open = false;
                }
            }
            Some(crate::library::LibraryAction::Reveal(path)) => {
                if let Err(e) = reveal_in_file_manager(&path) {
                    self.show_toast(format!("Could not reveal file: {e}"));
                }
            }
            None => {}
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

    /// Keyboard shortcuts cheat sheet modal (`?` or Status Bar `?` button).
    pub(crate) fn shortcuts_dialog(&mut self, ctx: &egui::Context) {
        if !self.dialogs.shortcuts_open {
            return;
        }
        let mut close = false;
        crate::components::modal_window(
            ctx,
            "shortcuts_modal",
            "Keyboard Shortcuts",
            420.0,
            |ui| {
                let groups: &[(&str, &[(&str, &str)])] = &[
                    (
                        "Editing",
                        &[
                            ("Undo / Redo", "Ctrl+Z / Ctrl+Shift+Z"),
                            ("Select all", "Ctrl+A"),
                            ("Duplicate / Delete", "Ctrl+D / Del"),
                            ("Group / Ungroup", "Ctrl+G / Ctrl+Shift+G"),
                            ("Stroke width - / +", "[ / ]"),
                            ("Nudge 1px / 10px", "Arrows / Shift+Arrows"),
                        ],
                    ),
                    (
                        "Navigation & Color",
                        &[
                            ("Pan canvas", "Space+Drag / Middle-Drag"),
                            ("Zoom In / Out / Fit / 100%", "Ctrl+± / Ctrl+0 / Ctrl+1"),
                            ("Sample color (Eyedropper)", "Alt+Click on canvas"),
                            ("Right-click menu", "Right-click canvas"),
                        ],
                    ),
                    (
                        "Files & Panels",
                        &[
                            ("Save / Save As / Quick PNG", "Ctrl+S / Ctrl+Shift+S / Ctrl+E"),
                            ("Copy image / Find OCR", "Ctrl+C / Ctrl+F"),
                            ("Library / Recent strip", "Ctrl+L / Ctrl+Shift+T"),
                            ("Close window", "Ctrl+W"),
                        ],
                    ),
                ];
                for (title, items) in groups {
                    crate::components::section_header(ui, title);
                    for &(action, keys) in *items {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(action)
                                    .size(qs_theme::font::SMALL as f32)
                                    .color(theme::text_secondary()),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    egui::RichText::new(crate::components::platform_shortcut(keys))
                                        .size(qs_theme::font::CAPTION as f32 + 0.5)
                                        .monospace()
                                        .color(theme::text_primary()),
                                );
                            });
                        });
                    }
                    ui.add_space(qs_theme::space::SM as f32);
                }
                crate::components::dialog_actions(ui, |ui| {
                    if crate::components::pill_button_primary(ui, "Done", "Close (Esc)") {
                        close = true;
                    }
                });
            },
        );
        if close {
            self.dialogs.shortcuts_open = false;
        }
    }

    pub(crate) fn combine_dialog(&mut self, ctx: &egui::Context) {
        if !self.dialogs.combine_dialog_open {
            return;
        }
        let mut open = self.dialogs.combine_dialog_open;
        let mut create = false;
        let count = self.viewport.tray_selected.len();
        crate::components::modal_window(
            ctx,
            "combine_template_modal",
            "Combine in Template",
            320.0,
            |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Combining {count} captures with numbered Step badges:"
                    ))
                    .size(qs_theme::font::SMALL as f32 + 0.5)
                    .color(theme::text_secondary()),
                );
                ui.add_space(qs_theme::space::MD as f32);
                for layout in [
                    CombineLayout::SideBySide,
                    CombineLayout::BeforeAfter,
                    CombineLayout::VerticalSteps,
                    CombineLayout::Grid2x2,
                ] {
                    if crate::components::radio_card(
                        ui,
                        self.tools.combine_layout == layout,
                        layout.label(),
                    ) {
                        self.tools.combine_layout = layout;
                    }
                    ui.add_space(qs_theme::space::XS as f32);
                }
                if self.viewport.tray_selected.len() >= 2 {
                    ui.add_space(qs_theme::space::XS as f32);
                    if crate::components::pill_button(
                        ui,
                        "Reverse order",
                        "Flip the order of selected captures (e.g. swap Before / After)",
                    ) {
                        self.viewport.tray_selected.reverse();
                    }
                }
                ui.add_space(qs_theme::space::SM as f32);
                crate::components::dialog_actions(ui, |ui| {
                    if crate::components::pill_button_primary(
                        ui,
                        "Create Combined Sheet",
                        "Build the sheet",
                    ) {
                        create = true;
                    }
                    ui.add_space(qs_theme::space::XS as f32);
                    if crate::components::pill_button(ui, "Cancel", "Close dialog") {
                        open = false;
                    }
                });
            },
        );
        if !create
            && open
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        {
            create = true;
        }
        self.dialogs.combine_dialog_open = open;

        if create {
            match self.create_combined_sheet() {
                Ok(()) => self.show_toast("Created combined template"),
                Err(e) => self.show_toast(format!("Combine failed: {e}")),
            }
        }
    }

    pub(crate) fn update_canvas_cursor(
        &self,
        ctx: &egui::Context,
        screen_rect: Rect,
        panning: bool,
        space_held: bool,
    ) {
        let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        if !screen_rect.contains(pos) {
            return;
        }
        // Pan overrides tool cursors in every mode (middle-drag or Space+drag).
        if panning {
            ctx.set_cursor_icon(CursorIcon::Grabbing);
            return;
        }
        if space_held {
            ctx.set_cursor_icon(CursorIcon::Grab);
            return;
        }
        let icon = match self.doc.editor.tool() {
            Tool::Text => CursorIcon::Text,
            Tool::Select => match self.doc.editor.drag_hint() {
                DragHint::Move => CursorIcon::Grabbing,
                DragHint::Handle(h) => handle_cursor(h),
                DragHint::Marquee => CursorIcon::Default,
                DragHint::TextSelect => CursorIcon::Text,
                DragHint::Create
                | DragHint::Crop
                | DragHint::Cutout
                | DragHint::SmartMove
                | DragHint::Region => CursorIcon::Crosshair,
                DragHint::None => {
                    let doc_p = self.viewport.view.screen_to_doc(pos, screen_rect);
                    let slack = self.doc.editor.hit_tol * 1.8;
                    let handle = self
                        .doc.editor
                        .selection_handles()
                        .iter()
                        .find(|h| h.pos.dist(doc_p) <= slack)
                        .map(|h| h.id);
                    if let Some(h) = handle {
                        handle_cursor(h)
                    } else if let Some(id) = self.doc.editor.pick(doc_p) {
                        if self.doc.editor.edit().is_some_and(|e| e.id == id) {
                            CursorIcon::Text
                        } else {
                            CursorIcon::Grab
                        }
                    } else {
                        CursorIcon::Default
                    }
                }
            },
            _ => CursorIcon::Crosshair,
        };
        ctx.set_cursor_icon(icon);
    }

    pub(crate) fn render_ui(&mut self, ui: &mut egui::Ui) {
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

        let layers_open = self.show_layers && !self.library.open;
        let layers_t = ctx.animate_bool_with_time(
            egui::Id::unique("layers_panel_t"),
            layers_open,
            0.18,
        );
        let layers_mutated = if layers_t > 0.01 {
            let mut panel = egui::Panel::left("layers_panel").frame(theme::panel_gutter(true));
            panel = if layers_t >= 0.999 {
                panel
                    .resizable(true)
                    .default_size(236.0)
                    .size_range(192.0..=316.0)
            } else {
                panel.resizable(false).exact_size(self.layers_w * layers_t)
            };
            let layers_resp = panel.show(ui, |ui| {
                theme::glass_panel(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 7.0;
                    let (icon_r, _) = ui.allocate_exact_size(
                        egui::Vec2::splat(13.0),
                        egui::Sense::hover(),
                    );
                    crate::icons::draw_paths(
                        ui.painter(),
                        icon_r.center(),
                        qs_icons::LAYERS,
                        theme::text_tertiary(),
                    );
                    ui.label(
                        egui::RichText::new("LAYERS")
                            .size(qs_theme::font::CAPTION as f32)
                            .strong()
                            .extra_letter_spacing(0.55)
                            .color(theme::text_tertiary()),
                    );
                    // `.count` — flat horizontal capsule (`padding:0 7px`,
                    // mono numerals) drawn manually so its height can't
                    // inflate the header row.
                    let count_g = ui.painter().layout_no_wrap(
                        format!("{}", self.doc.editor.doc().objects.len()),
                        egui::FontId::monospace(qs_theme::font::CAPTION as f32 - 1.0),
                        theme::text_tertiary(),
                    );
                    let badge_size = egui::Vec2::new(
                        (count_g.size().x + 14.0).ceil(),
                        (count_g.size().y + 4.0).ceil(),
                    );
                    let (badge_r, _) = ui.allocate_exact_size(badge_size, egui::Sense::hover());
                    ui.painter().rect_filled(
                        badge_r,
                        badge_size.y * 0.5,
                        theme::bg_btn_secondary_top(),
                    );
                    ui.painter().galley(badge_r.center() - count_g.size() * 0.5, count_g, theme::text_tertiary());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.doc.editor.selection().len() >= 2
                            && crate::components::pill_button(
                                ui,
                                "Align",
                                "Align & distribute in the Inspector",
                            ) {
                                self.dialogs.alignment_open = true;
                                self.show_inspector = true;
                            }
                        if self.doc.editor.selection_grouped()
                            && crate::components::pill_button(
                                ui,
                                "Ungroup",
                                "Ungroup (Ctrl+Shift+G)",
                            )
                        {
                            self.doc.editor.ungroup_selected();
                        }
                        if self.doc.editor.selection().len() >= 2
                            && crate::components::pill_button(ui, "Group", "Group (Ctrl+G)")
                        {
                            self.doc.editor.group_selected();
                        }
                    });
                });
                // `.side-h` hairline bleeds to both panel edges (mockup).
                let bleed = ui.max_rect().expand(qs_theme::space::SM as f32);
                ui.painter().hline(
                    bleed.left()..=bleed.right(),
                    ui.cursor().top().round() + 0.5,
                    Stroke::new(1.0, theme::border_dark_hairline()),
                );
                ui.add_space(4.0);
                let mut changed = false;
                let rows: Vec<_> = self
                    .doc.editor
                    .doc()
                    .objects
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(index, o)| {
                        let preview = match &o.kind {
                            qs_doc::ObjectKind::Text { text, .. }
                            | qs_doc::ObjectKind::Callout { text, .. } => {
                                let t = text.trim().lines().next().unwrap_or("");
                                if t.is_empty() {
                                    format!("{} #{}", o.kind.label(), o.id.0)
                                } else {
                                    let short: String = t.chars().take(16).collect();
                                    let ell = if t.chars().count() > 16 { "…" } else { "" };
                                    format!("{}: “{short}{ell}”", o.kind.label())
                                }
                            }
                            _ => format!("{} #{}", o.kind.label(), o.id.0),
                        };
                        (
                            index,
                            o.id,
                            crate::icons::kind_tool(&o.kind),
                            preview,
                            o.visible,
                            o.locked,
                            o.group.is_some(),
                            self.doc.editor.selection().contains(&o.id),
                        )
                    })
                    .collect();
                if rows.is_empty() {
                    if crate::components::empty_state(
                        ui,
                        "No annotations yet",
                        "Pick a tool above or start\nwith an arrow callout",
                        Some("Draw Arrow"),
                    ) {
                        self.doc.editor.set_tool(Tool::Arrow);
                    }
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    let mut released_layer = None;
                    let mut drop_target = None;
                    for (index, id, icon_tool, label, visible, locked, grouped, selected) in rows {
                        // Mockup anatomy: 28px row, type icon + label left,
                        // eye/lock trailing — revealed on hover, persisted only
                        // while active (hidden keeps eye-off, locked keeps lock).
                        const ROW_H: f32 = 28.0;
                        let avail = ui.available_rect_before_wrap();
                        let row_rect =
                            Rect::from_min_size(avail.min, Vec2::new(avail.width(), ROW_H));
                        let row_hovered = ui.rect_contains_pointer(row_rect);
                        if row_hovered {
                            drop_target = Some((id, index));
                        }
                        let row_fill = if selected {
                            theme::accent_alpha(if qs_theme::pal().dark { 72 } else { 36 })
                        } else if row_hovered {
                            theme::bg_pill_hover()
                        } else {
                            Color32::TRANSPARENT
                        };
                        let response = ui.interact(
                            row_rect,
                            ui.scope_id().with(("layer_row", id)),
                            egui::Sense::click_and_drag(),
                        );
                        egui::Frame::NONE
                            .inner_margin(egui::Margin::symmetric(
                                qs_theme::space::SM as i8,
                                1,
                            ))
                            .corner_radius(egui::CornerRadius::same(
                                qs_theme::radius::SM as u8
                            ))
                            .fill(row_fill)
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.x = qs_theme::space::XS as f32;
                                ui.set_min_height(ROW_H - 2.0);
                                ui.horizontal(|ui| {
                                    let dim = if visible { 1.0 } else { 0.45 };
                                    let (ic_rect, _) = ui.allocate_exact_size(
                                        egui::Vec2::splat(16.0),
                                        egui::Sense::hover(),
                                    );
                                    crate::icons::draw_tool_icon(
                                        ui.painter(),
                                        ic_rect.center(),
                                        icon_tool,
                                        theme::text_tertiary().gamma_multiply(dim),
                                    );
                                    let text = if grouped {
                                        format!("▦ {label}")
                                    } else {
                                        label
                                    };
                                    ui
                                        .add(
                                            egui::Label::new(
                                                egui::RichText::new(text)
                                                    .size(qs_theme::font::SMALL as f32)
                                                    .color(
                                                        if selected {
                                                            theme::text_primary()
                                                        } else {
                                                            theme::text_secondary()
                                                        }
                                                        .gamma_multiply(dim),
                                                    ),
                                            )
                                            .truncate()
                                            .selectable(false),
                                        )
                                        .on_hover_text(format!(
                                            "Drag to reorder · index {}",
                                            index + 1
                                        ));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if locked || row_hovered {
                                                let (lock_rect, lock_resp) = ui
                                                    .allocate_exact_size(
                                                        egui::Vec2::splat(18.0),
                                                        egui::Sense::click(),
                                                    );
                                                crate::icons::draw_action_icon(
                                                    ui.painter(),
                                                    lock_rect.center(),
                                                    if locked {
                                                        crate::icons::ActionIcon::Lock
                                                    } else {
                                                        crate::icons::ActionIcon::Unlock
                                                    },
                                                    if lock_resp.hovered() {
                                                        theme::text_primary()
                                                    } else {
                                                        theme::text_tertiary()
                                                    },
                                                );
                                                if lock_resp
                                                    .on_hover_text(if locked {
                                                        "Unlock layer"
                                                    } else {
                                                        "Lock layer"
                                                    })
                                                    .clicked()
                                                {
                                                    changed |= self.doc.editor.set_locked(id, !locked);
                                                }
                                            }
                                            if !visible || row_hovered {
                                                let (eye_rect, eye_resp) = ui
                                                    .allocate_exact_size(
                                                        egui::Vec2::splat(18.0),
                                                        egui::Sense::click(),
                                                    );
                                                crate::icons::draw_action_icon(
                                                    ui.painter(),
                                                    eye_rect.center(),
                                                    if visible {
                                                        crate::icons::ActionIcon::Eye
                                                    } else {
                                                        crate::icons::ActionIcon::EyeOff
                                                    },
                                                    if eye_resp.hovered() {
                                                        theme::text_primary()
                                                    } else {
                                                        theme::text_tertiary()
                                                    },
                                                );
                                                if eye_resp
                                                    .on_hover_text(if visible {
                                                        "Hide layer"
                                                    } else {
                                                        "Show layer"
                                                    })
                                                    .clicked()
                                                {
                                                    changed |=
                                                        self.doc.editor.set_visible(id, !visible);
                                                }
                                            }
                                        },
                                    );
                                });
                            });
                        if response.clicked() && !locked {
                            self.doc.editor.set_tool(Tool::Select);
                            self.doc.editor.select(Some(id));
                        }
                        if response.dragged() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                        }
                        if response.drag_stopped() {
                            released_layer = Some(id);
                        }
                    }
                    if let (Some(source), Some((target, index))) = (released_layer, drop_target) {
                        if source != target {
                            changed |= self.doc.editor.move_layer(source, index);
                        }
                    }
                    });
                }
                changed
            })
            });
            if layers_t >= 0.999 {
                self.layers_w = layers_resp.response.rect.width();
            }
            layers_resp.inner
        } else {
            false
        };
        if layers_mutated {
            self.touch();
        }

        let insp_open = self.show_inspector && !self.library.open;
        let insp_t = ctx.animate_bool_with_time(
            egui::Id::unique("image_panel_t"),
            insp_open,
            0.18,
        );
        let panel_mutated = if insp_t > 0.01 {
            let panel_resp = egui::Panel::right("image_panel")
                .resizable(false)
                .exact_size(272.0 * insp_t)
                .frame(theme::panel_gutter(false))
                .show(ui, |ui| {
                theme::glass_panel(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = qs_theme::space::XS as f32;
                            if self.dialogs.canvas_dialog_open {
                                crate::components::card(ui, |ui| {
                                    if inspector_card_header(ui, "CANVAS", true) {
                                        self.dialogs.canvas_dialog_open = false;
                                    }
                                    self.canvas_section(ui);
                                });
                                ui.add_space(qs_theme::space::SM as f32);
                            }
                            let selected = self.doc.editor.selection().len();
                            if selected >= 2 || self.dialogs.alignment_open {
                                crate::components::card(ui, |ui| {
                                    if inspector_card_header(ui, "ALIGN", self.dialogs.alignment_open) {
                                        self.dialogs.alignment_open = false;
                                    }
                                    if selected >= 2 {
                                        self.align_section(ui);
                                    } else {
                                        ui.label(
                                            egui::RichText::new("Select two or more objects to align them.")
                                                .size(qs_theme::font::SMALL as f32)
                                                .color(theme::text_tertiary()),
                                        );
                                    }
                                });
                                ui.add_space(qs_theme::space::SM as f32);
                            }
                            let changed = image_panel::render(ui, &mut self.doc.editor, &mut self.prefs);
                            ui.add_space(qs_theme::space::SM as f32);
                            crate::components::card(ui, |ui| {
                                crate::components::section_header(ui, "Quick Actions");
                                // `.tbtn` with a leading icon — icon+label
                                // group stays centred like the mockup.
                                let qa_button = |ui: &mut egui::Ui,
                                                 icon: crate::icons::ActionIcon,
                                                 label: &str,
                                                 accent_text: bool,
                                                 tip: &str|
                                 -> bool {
                                    let w = ui.available_width();
                                    let (rect, resp) = ui.allocate_exact_size(
                                        Vec2::new(w, 28.0),
                                        egui::Sense::click(),
                                    );
                                    let ht = theme::hot_t(&resp);
                                    theme::paint_secondary_button(
                                        ui.painter(),
                                        rect,
                                        6.0,
                                        ht,
                                        true,
                                        false,
                                    );
                                    let fg = if accent_text {
                                        theme::accent_hi()
                                    } else {
                                        theme::text_primary()
                                    };
                                    let font = egui::FontId::proportional(
                                        qs_theme::font::SMALL as f32,
                                    );
                                    let g = ui.painter().layout_no_wrap(
                                        label.to_owned(),
                                        font,
                                        fg,
                                    );
                                    let total = 13.0 + 6.0 + g.size().x;
                                    let ix = rect.center().x - total * 0.5;
                                    crate::icons::draw_action_icon(
                                        ui.painter(),
                                        Pos2::new(ix + 6.5, rect.center().y),
                                        icon,
                                        fg,
                                    );
                                    ui.painter().galley(
                                        Pos2::new(
                                            ix + 19.0,
                                            rect.center().y - g.size().y * 0.5,
                                        ),
                                        g,
                                        fg,
                                    );
                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(
                                            egui::CursorIcon::PointingHand,
                                        );
                                    }
                                    resp.on_hover_text(crate::components::platform_shortcut(tip)).clicked()
                                };
                                if qa_button(
                                    ui,
                                    crate::icons::ActionIcon::ScanLine,
                                    "Auto-Redact (OCR)",
                                    true,
                                    "Scan text with OCR and blackout emails, IPs, tokens, and phone numbers",
                                )
                                {
                                    self.ocr_hints.ensure(
                                        self.doc.editor.doc(),
                                        &self.doc.base,
                                        &self.prefs.ocr_lang,
                                    );
                                    self.ocr_hints.poll();
                                    if self.ocr_hints.is_scanning() {
                                        self.show_toast("Scanning text with OCR… click again in a moment");
                                    } else {
                                        let rects = self.ocr_hints.sensitive_rects();
                                        let n = self.doc.editor.add_redactions(&rects, qs_doc::BlurMode::Solid);
                                        if n > 0 {
                                            self.touch();
                                            self.show_toast(format!("Auto-redacted {n} sensitive item(s)"));
                                        } else {
                                            self.show_toast("No sensitive text detected");
                                        }
                                    }
                                }
                                ui.add_space(6.0);
                                if qa_button(
                                    ui,
                                    crate::icons::ActionIcon::Sparkles,
                                    "Simplify UI",
                                    false,
                                    "Convert UI text/controls into clean editable wireframe blocks (uses OCR if scanned)",
                                )
                                {
                                    let ocr_rects: Vec<RectF> = self
                                        .ocr_hints
                                        .lines()
                                        .iter()
                                        .map(|h| h.rect)
                                        .collect();
                                    let n = if !ocr_rects.is_empty() {
                                        self.doc.editor.simplify_from_rects(&ocr_rects, self.doc.editor.region())
                                    } else {
                                        self.doc.editor.simplify_from_image(&self.doc.base)
                                    };
                                    if n > 0 {
                                        self.touch();
                                        self.show_toast(format!("Simplified {n} UI block(s)"));
                                    } else {
                                        self.show_toast("No UI blocks detected");
                                    }
                                }
                                ui.add_space(6.0);
                                if qa_button(
                                    ui,
                                    crate::icons::ActionIcon::LayerFront,
                                    "Flatten all",
                                    false,
                                    "Bake every annotation into the image (undo with Ctrl+Z)",
                                )
                                {
                                    if let Err(e) = self.flatten_all() {
                                        self.show_toast(format!("Flatten failed: {e}"));
                                    }
                                }
                            });
                            changed
                        })
                        .inner
                })
            });
            panel_resp.inner
        } else {
            false
        };
        if panel_mutated {
            self.touch();
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if self.library.open {
                    self.library_view(ui);
                    return;
                }
                let (screen_rect, response) =
                    ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
                self.viewport.last_canvas_rect = Some(screen_rect);
                self.viewport.view.sync(self.doc.editor.doc());
                self.viewport.last_cursor_doc = ui
                    .input(|i| i.pointer.latest_pos())
                    .filter(|p| screen_rect.contains(*p))
                    .map(|p| self.viewport.view.screen_to_doc(p, screen_rect));

                // Smart Move: keep line hints in step with the base image, and
                // keep repainting while the worker runs so the proposal shows
                // up without the user moving the mouse again.
                // Hints feed two things: the Smart Move proposal, and copying
                // the text inside the selection region.
                let wants_hints = self.doc.editor.tool() == Tool::SmartMove
                    || self.doc.editor.region().is_some()
                    || self.find.open;
                if wants_hints {
                    self.ocr_hints.ensure(
                        self.doc.editor.doc(),
                        &self.doc.base,
                        &self.prefs.ocr_lang,
                    );
                    if self.ocr_hints.is_scanning() {
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_millis(120));
                    }
                    self.ocr_hints.poll();
                }
                let hint_rect = if self.doc.editor.tool() == Tool::SmartMove
                    && self.doc.editor.pending_smart_move().is_none()
                    && self.doc.editor.move_src_rect().is_none()
                {
                    self.viewport.last_cursor_doc
                        .and_then(|p| self.ocr_hints.hit(p).map(|h| h.rect))
                } else {
                    None
                };

                let fit_cap = if self.viewport.needs_initial_fit {
                    Some(1.0)
                } else {
                    self.viewport.fit_memo
                        .filter(|m| m.wants_refit(&self.viewport.view, screen_rect.size()))
                        .map(|m| m.max_zoom)
                };
                if let Some(cap) = fit_cap {
                    if screen_rect.width() > 32.0 && screen_rect.height() > 32.0 {
                        self.viewport.view
                            .fit_to(self.viewport.view.display_rect(), screen_rect, 40.0);
                        self.viewport.view.zoom = self.viewport.view.zoom.min(cap);
                        self.viewport.needs_initial_fit = false;
                        self.viewport.fit_memo = Some(crate::canvas::FitMemo::of(
                            &self.viewport.view,
                            screen_rect.size(),
                            cap,
                        ));
                    }
                }

                let mut painter = ui.painter_at(screen_rect);

                // Zoom & Pan via mouse wheel
                ui.input(|i| {
                    if screen_rect.contains(i.pointer.latest_pos().unwrap_or_default()) {
                        let zoom_delta = i.zoom_delta();
                        if (zoom_delta - 1.0).abs() > 0.001 {
                            if let Some(pos) = i.pointer.latest_pos() {
                                self.viewport.view.zoom_at(zoom_delta, pos, screen_rect);
                            }
                        } else if i.smooth_scroll_delta != Vec2::ZERO {
                            if i.modifiers.ctrl || i.modifiers.command {
                                let factor = (1.0 + i.smooth_scroll_delta.y * 0.01).clamp(0.5, 2.0);
                                if let Some(pos) = i.pointer.latest_pos() {
                                    self.viewport.view.zoom_at(factor, pos, screen_rect);
                                }
                            } else {
                                self.viewport.view.pan(i.smooth_scroll_delta);
                            }
                        }
                    }
                });

                // Pointer interactions on canvas:
                // - Middle-mouse drag or Space+drag pans canvas in any tool
                // - Alt+click samples pixel color from screenshot (Eyedropper)
                // - Right-click opens contextual canvas menu
                let (shift_held, alt_held, space_held, mid_drag) = ui.input(|i| {
                    (
                        i.modifiers.shift,
                        i.modifiers.alt,
                        i.key_down(Key::Space)
                            && self.doc.editor.edit().is_none()
                            && ctx.memory(|m| m.focused().is_none()),
                        i.pointer.button_down(egui::PointerButton::Middle),
                    )
                });
                let panning = mid_drag || (space_held && response.dragged());
                if panning {
                    let delta = ui.input(|i| i.pointer.delta());
                    if delta != Vec2::ZERO {
                        self.viewport.view.pan(delta);
                    }
                    ctx.set_cursor_icon(CursorIcon::Grabbing);
                } else if space_held && screen_rect.contains(ui.input(|i| i.pointer.hover_pos().unwrap_or_default())) {
                    ctx.set_cursor_icon(CursorIcon::Grab);
                }

                if response.secondary_clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let doc_p = self.viewport.view.screen_to_doc(pos, screen_rect);
                        let picked = self.doc.editor.pick(doc_p);
                        if let Some(id) = picked {
                            if self.doc.editor.tool() != Tool::Select {
                                self.doc.editor.set_tool(Tool::Select);
                            }
                            if !self.doc.editor.selection().contains(&id) {
                                self.doc.editor.select(Some(id));
                            }
                        }
                        self.dialogs.context_menu = Some((pos, picked));
                    }
                }

                self.doc.editor.hit_tol = 6.0 / self.viewport.view.zoom.max(0.1);

                if panning || space_held {
                    // Handled by pan mode above.
                } else if alt_held && response.clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let doc_p = self.viewport.view.screen_to_doc(pos, screen_rect);
                        let s = self.doc.editor.doc().scale;
                        let px = (doc_p.x * s).round() as i32;
                        let py = (doc_p.y * s).round() as i32;
                        if px >= 0
                            && py >= 0
                            && (px as u32) < self.doc.base.width()
                            && (py as u32) < self.doc.base.height()
                        {
                            let p = self.doc.base.get_pixel(px as u32, py as u32).0;
                            let rgb = ((p[0] as u32) << 16) | ((p[1] as u32) << 8) | p[2] as u32;
                            self.doc.editor.set_color(rgb);
                            self.touch();
                            self.show_toast(format!("Sampled color #{rgb:06X}"));
                        }
                    }
                } else if response.double_clicked() && self.doc.editor.tool() == Tool::Select {
                    self.tools.saw_drag_press = false;
                    if let Some(pos) = response.interact_pointer_pos() {
                        let doc_p = self.viewport.view.screen_to_doc(pos, screen_rect);
                        self.doc.editor.double_click(doc_p);
                        self.touch();
                    }
                } else if response.drag_started() {
                    self.tools.saw_drag_press = true;
                    if let Some(pos) = response.interact_pointer_pos() {
                        // egui recognizes a drag only after crossing the click threshold.
                        let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(pos);
                        let start = self.viewport.view.screen_to_doc(origin, screen_rect);
                        let rev_before = self.doc.editor.revision();
                        self.doc.editor.pointer_down_mod(start, shift_held);
                        if pos != origin {
                            let point = self.viewport.view.screen_to_doc(pos, screen_rect);
                            self.doc.editor.pointer_move(point, shift_held);
                        }
                        if self.doc.editor.revision() != rev_before || pos != origin {
                            self.touch();
                        }
                    }
                } else if response.dragged() {
                    if response.drag_delta() != Vec2::ZERO {
                        if let Some(pos) = response.interact_pointer_pos() {
                            let doc_p = self.viewport.view.screen_to_doc(pos, screen_rect);
                            self.doc.editor.pointer_move(doc_p, shift_held);
                            // Marquee, Crop, Cutout, SelectRegion and SmartMove source
                            // drags only draw transient egui overlays — skip full bitmap
                            // re-rendering until release.
                            let overlay_only = self.doc.editor.marquee_rect().is_some()
                                || self.doc.editor.crop_rect().is_some()
                                || self.doc.editor.cutout_rect().is_some()
                                || self.doc.editor.tool() == Tool::SelectRegion
                                || (self.doc.editor.tool() == Tool::SmartMove
                                    && self.doc.editor.move_src_rect().is_some());
                            if !overlay_only {
                                self.touch();
                            }
                        }
                    }
                } else if response.drag_stopped() {
                    self.tools.saw_drag_press = false;
                    if let Some(pos) = response.interact_pointer_pos() {
                        let doc_p = self.viewport.view.screen_to_doc(pos, screen_rect);
                        let was_marquee = self.doc.editor.marquee_rect().is_some();
                        self.doc.editor.pointer_move(doc_p, shift_held);
                        self.doc.editor.pointer_up(doc_p);
                        if !was_marquee {
                            self.touch();
                        }
                    }
                } else if response.clicked() && !self.tools.saw_drag_press {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let doc_p = self.viewport.view.screen_to_doc(pos, screen_rect);
                        // Smart Move: a click on a recognized line arms that
                        // line; anywhere else keeps the manual drag flow.
                        let arm = if self.doc.editor.tool() == Tool::SmartMove
                            && self.doc.editor.pending_smart_move().is_none()
                        {
                            self.ocr_hints.hit(doc_p).map(|h| h.rect)
                        } else {
                            None
                        };
                        let rev_before = self.doc.editor.revision();
                        let armed = arm.is_some_and(|r| self.doc.editor.begin_smart_move(r));
                        if !armed {
                            self.doc.editor.pointer_down_mod(doc_p, shift_held);
                            self.doc.editor.pointer_up(doc_p);
                        }
                        if self.doc.editor.revision() != rev_before {
                            self.touch();
                        }
                    }
                }

                self.update_canvas_cursor(&ctx, screen_rect, panning, space_held);

                if self.render_state.needs_render {
                    self.update_texture(&ctx);
                }

                // 1. Calm matte macOS workspace backdrop
                canvas::paint_workspace(&painter, screen_rect);

                // 2. Document screen bounds + soft multi-layer drop shadow
                self.viewport.view.sync(self.doc.editor.doc());
                let img_screen_rect = self
                    .viewport.view
                    .display_rect_to_screen(self.viewport.view.display_rect(), screen_rect);

                canvas::paint_document_shadow(&painter, img_screen_rect);

                // 3. Checkerboard strictly inside document bounds (visible only through alpha)
                let checker_id = self.checker_texture_id(&ctx);
                canvas::paint_checkerboard(&painter, img_screen_rect, checker_id);

                // 4. Rendered flattened document
                if let Some(tex) = &self.render_state.texture {
                    painter.image(
                        tex.id(),
                        img_screen_rect,
                        Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }

                // 5. Crisp hairline rim around document
                canvas::paint_document_frame(&painter, img_screen_rect);

                // 5a. Find in capture: every match dimly, the current one framed
                if self.find.open {
                    let hit_groups = self.ocr_hints.find_rects(&self.find.query);
                    if !hit_groups.is_empty() {
                        let groups: Vec<Vec<Rect>> = hit_groups
                            .iter()
                            .map(|words| {
                                words
                                    .iter()
                                    .map(|r| {
                                        self.viewport.view.doc_rect_to_screen(*r, screen_rect)
                                    })
                                    .collect()
                            })
                            .collect();
                        let current = self.find.index.min(groups.len() - 1);
                        canvas::paint_find_hits(&painter, &groups, current);
                    }
                }

                // 5b. Crop / Cutout preview + Crop aspect ratio presets bar
                if self.doc.editor.tool() == Tool::Crop {
                    if let Some(crop) = self.doc.editor.crop_rect() {
                        let crop_screen = self.viewport.view.doc_rect_to_screen(crop, screen_rect);
                        canvas::paint_crop_overlay(&mut painter, crop_screen, screen_rect);
                    }
                    let mut ratio_pick: Option<f32> = None;
                    let mut auto_trim = false;
                    let mut reset_view = false;
                    egui::Area::new(egui::Id::unique("crop_presets_bar"))
                        .order(egui::Order::Foreground)
                        .pivot(egui::Align2::CENTER_TOP)
                        .fixed_pos(Pos2::new(screen_rect.center().x, screen_rect.top() + 10.0))
                        .show(&ctx, |ui| {
                            theme::popover_frame().show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    for (lbl, ratio) in [
                                        ("16:9", 16.0 / 9.0),
                                        ("4:3", 4.0 / 3.0),
                                        ("1:1", 1.0),
                                    ] {
                                        if crate::components::pill_button(
                                            ui,
                                            lbl,
                                            &format!("Center-crop to {lbl} aspect ratio"),
                                        ) {
                                            ratio_pick = Some(ratio);
                                        }
                                    }
                                    if crate::components::pill_button(
                                        ui,
                                        "Auto-trim",
                                        "Trim uniform outer borders",
                                    ) {
                                        auto_trim = true;
                                    }
                                    if crate::components::pill_button(
                                        ui,
                                        "Reset",
                                        "Restore full original frame",
                                    ) {
                                        reset_view = true;
                                    }
                                });
                            });
                        });
                    if let Some(ratio) = ratio_pick {
                        let v = self.doc.editor.doc().view.normalized();
                        let cur = v.w / v.h.max(1.0);
                        let (nw, nh) = if cur > ratio {
                            (v.h * ratio, v.h)
                        } else {
                            (v.w, v.w / ratio)
                        };
                        let nx = v.x + (v.w - nw) * 0.5;
                        let ny = v.y + (v.h - nh) * 0.5;
                        self.doc.editor.set_view(RectF::new(nx, ny, nw, nh));
                        self.touch();
                        self.show_toast("Cropped to aspect ratio");
                    } else if auto_trim {
                        if self.doc.editor.trim_to_content(&self.doc.base, 6) {
                            self.touch();
                            self.show_toast("Trimmed uniform edges");
                        } else {
                            self.show_toast("No uniform edges to trim");
                        }
                    } else if reset_view {
                        let sz = self.doc.editor.doc().base_size;
                        self.doc.editor.set_view(RectF::new(0.0, 0.0, sz.w, sz.h));
                        self.touch();
                        self.show_toast("Reset crop to full image");
                    }
                }
                if self.doc.editor.tool() == Tool::Cutout {
                    if let Some(strip) = self.doc.editor.cutout_rect() {
                        let mut s_rect = self.viewport.view.doc_rect_to_screen(strip, screen_rect);
                        match self.doc.editor.style.cutout_axis {
                            qs_doc::CutoutAxis::Horizontal => {
                                s_rect.min.x = img_screen_rect.min.x;
                                s_rect.max.x = img_screen_rect.max.x;
                            }
                            qs_doc::CutoutAxis::Vertical => {
                                s_rect.min.y = img_screen_rect.min.y;
                                s_rect.max.y = img_screen_rect.max.y;
                            }
                        }
                        canvas::paint_cutout_overlay(&mut painter, s_rect, screen_rect);
                    }
                }
                if self.doc.editor.tool() == Tool::SmartMove {
                    let armed = self.doc.editor.pending_smart_move().is_some();
                    let rect = if armed {
                        self.doc.editor.pending_smart_move()
                    } else {
                        self.doc.editor.move_src_rect().or(hint_rect)
                    };
                    if let Some(r) = rect {
                        let s_rect = self.viewport.view.doc_rect_to_screen(r, screen_rect);
                        canvas::paint_move_overlay(&painter, s_rect, armed);
                    }
                }

                // 5c. Committed selection region outline + floating action bar
                if self.doc.editor.tool() != Tool::Crop
                    && self.doc.editor.tool() != Tool::SelectRegion
                    && self.doc.editor.region().is_some()
                {
                    let region = self.doc.editor.region().unwrap();
                    let region_screen = self.viewport.view.doc_rect_to_screen(region, screen_rect);
                    canvas::paint_region(&painter, region_screen);

                    // Floating quick-action pill directly under the selection region
                    let bar_pos = Pos2::new(
                        region_screen.center().x,
                        (region_screen.bottom() + 8.0).min(screen_rect.bottom() - 44.0),
                    );
                    let mut act_copy_txt = false;
                    let mut act_redact = false;
                    let mut act_crop = false;
                    let mut act_clear = false;
                    egui::Area::new(egui::Id::unique("region_action_bar"))
                        .order(egui::Order::Foreground)
                        .pivot(egui::Align2::CENTER_TOP)
                        .fixed_pos(bar_pos)
                        .show(&ctx, |ui| {
                            theme::popover_frame().show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    if crate::components::pill_button(
                                        ui,
                                        "Copy Text",
                                        "Copy OCR text inside this selection",
                                    ) {
                                        act_copy_txt = true;
                                    }
                                    if crate::components::pill_button_colored(
                                        ui,
                                        "Redact",
                                        "Cover this region with a blackout bar",
                                        theme::danger_red(),
                                    ) {
                                        act_redact = true;
                                    }
                                    if crate::components::pill_button(
                                        ui,
                                        "Crop",
                                        "Crop the image to this region",
                                    ) {
                                        act_crop = true;
                                    }
                                    if crate::components::pill_button(
                                        ui,
                                        "Clear",
                                        "Clear selection region",
                                    ) {
                                        act_clear = true;
                                    }
                                });
                            });
                        });
                    if act_copy_txt {
                        self.copy_text_in_region();
                    }
                    if act_redact {
                        let n = self.doc.editor.add_redactions(&[region], qs_doc::BlurMode::Solid);
                        self.doc.editor.set_region(None);
                        if n > 0 {
                            self.touch();
                            self.show_toast("Redacted selection");
                        }
                    }
                    if act_crop {
                        self.doc.editor.set_view(region.normalized());
                        self.doc.editor.set_region(None);
                        self.touch();
                        self.show_toast("Cropped to selection");
                    }
                    if act_clear {
                        self.doc.editor.set_region(None);
                        self.touch();
                    }
                }

                // 5d. Marquee preview while dragging on empty canvas
                if self.doc.editor.tool() == Tool::Select {
                    if let Some(m) = self.doc.editor.marquee_rect() {
                        if m.w >= 1.0 || m.h >= 1.0 {
                            let m_screen =
                                self.viewport.view.doc_rect_to_screen(m.normalized(), screen_rect);
                            canvas::paint_region(&painter, m_screen);
                        }
                    }
                }

                // 5e. Multi-selection bounding outline (no handles)
                if self.doc.editor.selection().len() > 1 {
                    if let Some(b) = self.doc.editor.selection_bounds() {
                        let b_screen = self.viewport.view.doc_rect_to_screen(b, screen_rect);
                        painter.rect_stroke(
                            b_screen,
                            0.0,
                            Stroke::new(1.0, accent_blue()),
                            StrokeKind::Outside,
                        );
                    }
                }

                // 6. Selection outline & handles
                let handles = self.doc.editor.selection_handles();
                if !handles.is_empty() {
                    canvas::paint_handles(
                        &painter,
                        &handles,
                        self.doc.editor.selected_object().map(|o| &o.kind),
                        &self.viewport.view,
                        screen_rect,
                    );
                }

                // 7. Active text-edit frame + blinking caret
                if let Some(te) = self.doc.editor.edit() {
                    if let Some(obj) = self.doc.editor.doc().get(te.id) {
                        if let Some((origin, lay)) = qs_render::text_block(&obj.kind) {
                            let box_tl = self.viewport.view.doc_to_screen(origin, screen_rect);
                            let box_br = self.viewport.view.doc_to_screen(
                                PointF::new(
                                    origin.x + lay.width.max(24.0),
                                    origin.y + lay.height.max(20.0),
                                ),
                                screen_rect,
                            );
                            let edit_rect = Rect::from_min_max(box_tl, box_br).expand(5.0);
                            painter.rect_stroke(
                                edit_rect,
                                4.0,
                                Stroke::new(1.25, theme::accent_alpha(180)),
                                StrokeKind::Outside,
                            );
                            let (cx, cy) = lay.caret(te.caret);
                            let p1 = self.viewport.view.doc_to_screen(
                                PointF::new(origin.x + cx, origin.y + cy),
                                screen_rect,
                            );
                            let p2 = self.viewport.view.doc_to_screen(
                                PointF::new(origin.x + cx, origin.y + cy + 18.0),
                                screen_rect,
                            );
                            // Telling eframe a text field is active is what makes
                            // winit call set_ime_allowed(true) — without it the OS
                            // never engages Telex/VNI and only raw ASCII arrives.
                            ctx.output_mut(|o| {
                                o.ime = Some(egui::output::IMEOutput {
                                    purpose: egui::IMEPurpose::Normal,
                                    rect: edit_rect,
                                    cursor_rect: Rect::from_min_max(
                                        Pos2::new(p1.x, p1.y.min(p2.y)),
                                        Pos2::new(p1.x + 2.0, p1.y.max(p2.y)),
                                    ),
                                    should_interrupt_composition: false,
                                });
                            });
                            let t = ctx.input(|i| i.time);
                            let show_caret = ((t * 2.0) as u64).is_multiple_of(2);
                            if show_caret {
                                painter.line_segment([p1, p2], Stroke::new(2.0, accent_blue()));
                            }
                            // In-flight IME composition (Telex "đan" before it
                            // commits to "đan"/"dans") so the user sees typing.
                            if !self.tools.ime_preedit.is_empty() {
                                painter.text(
                                    p2 + egui::vec2(6.0, 0.0),
                                    egui::Align2::LEFT_BOTTOM,
                                    &self.tools.ime_preedit,
                                    FontId::proportional(14.0),
                                    theme::text_primary(),
                                );
                            }
                            ctx.request_repaint_after(std::time::Duration::from_millis(500));
                        }
                    }
                }

                // 8. Floating macOS Toast HUD (semantic success/error icon, crisp rim, ease-out entrance)
                if let Some((msg, time)) = &self.toast {
                    let elapsed = time.elapsed().as_secs_f32();
                    if elapsed < 2.6 {
                        let enter_t = (elapsed / 0.16).clamp(0.0, 1.0);
                        let enter_ease = 1.0 - (1.0 - enter_t).powi(3);
                        let fade_out = if elapsed > 2.2 {
                            ((2.6 - elapsed) / 0.4).clamp(0.0, 1.0)
                        } else {
                            1.0
                        };
                        let fade = enter_ease * fade_out;
                        let lift = (1.0 - enter_ease) * 6.0 + (1.0 - fade_out) * 4.0;
                        let is_err = msg.contains("failed")
                            || msg.contains("Failed")
                            || msg.contains("No ");
                        let text_galley = painter.layout_no_wrap(
                            msg.clone(),
                            FontId::proportional(12.5),
                            text_primary(),
                        );
                        let w = (text_galley.size().x + 56.0).clamp(164.0, 440.0);
                        let hud_rect = Rect::from_center_size(
                            Pos2::new(screen_rect.center().x, screen_rect.bottom() - 34.0 + lift),
                            Vec2::new(w, 36.0),
                        );
                        let bg_col = theme::bg_card();
                        let rim_col = theme::border_hot();
                        let alpha_bg = (244.0 * fade) as u8;
                        let alpha_fg = (255.0 * fade) as u8;
                        theme::paint_soft_shadow(
                            &painter,
                            hud_rect,
                            18.0,
                            6.0,
                            16.0,
                            (if qs_theme::pal().dark { 130.0 } else { 52.0 } * fade) as u8,
                        );
                        painter.rect_filled(
                            hud_rect,
                            18.0,
                            Color32::from_rgba_unmultiplied(
                                bg_col.r(),
                                bg_col.g(),
                                bg_col.b(),
                                alpha_bg,
                            ),
                        );
                        painter.rect_stroke(
                            hud_rect,
                            18.0,
                            Stroke::new(
                                1.0,
                                Color32::from_rgba_unmultiplied(
                                    rim_col.r(),
                                    rim_col.g(),
                                    rim_col.b(),
                                    (200.0 * fade) as u8,
                                ),
                            ),
                            StrokeKind::Inside,
                        );
                        let icon_c = Pos2::new(hud_rect.left() + 19.0, hud_rect.center().y);
                        if is_err {
                            let dc = theme::danger_red();
                            painter.circle_filled(
                                icon_c,
                                8.0,
                                Color32::from_rgba_unmultiplied(dc.r(), dc.g(), dc.b(), alpha_fg),
                            );
                            icons::draw_action_icon(
                                &painter,
                                icon_c,
                                ActionIcon::Close,
                                Color32::from_rgba_unmultiplied(255, 255, 255, alpha_fg),
                            );
                        } else {
                            let sc = success_green();
                            icons::draw_action_icon(
                                &painter,
                                icon_c,
                                ActionIcon::CheckCircle,
                                Color32::from_rgba_unmultiplied(sc.r(), sc.g(), sc.b(), alpha_fg),
                            );
                        }
                        painter.text(
                            Pos2::new(hud_rect.left() + 36.0, hud_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            msg,
                            FontId::proportional(12.5),
                            Color32::from_rgba_unmultiplied(
                                text_primary().r(),
                                text_primary().g(),
                                text_primary().b(),
                                alpha_fg,
                            ),
                        );
                        if elapsed < 0.16 || elapsed >= 2.2 {
                            ctx.request_repaint();
                        } else {
                            ctx.request_repaint_after(std::time::Duration::from_secs_f32(
                                (2.2 - elapsed).max(0.016),
                            ));
                        }
                    }
                }
            });

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

/// Directional resize cursor for a box handle; point handles (line ends,
/// callout tail, step radius) get the repositioning cursor.
fn handle_cursor(h: HandleId) -> CursorIcon {
    match h {
        HandleId::N | HandleId::S => CursorIcon::ResizeVertical,
        HandleId::E | HandleId::W => CursorIcon::ResizeHorizontal,
        HandleId::NE | HandleId::SW => CursorIcon::ResizeNeSw,
        HandleId::NW | HandleId::SE => CursorIcon::ResizeNwSe,
        _ => CursorIcon::Grab,
    }
}

/// Reads `.qsdoc`, an image beside its `.qsdoc`, or a bare image.
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

/// Reveal a capture in the platform's file manager, selecting it where the
/// platform supports it.
fn reveal_in_file_manager(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = path.parent().unwrap_or(path);
        std::process::Command::new("xdg-open").arg(target).spawn()?;
    }
    Ok(())
}

/// Caption header of an Inspector card; `closable` adds a Done pill and the
/// return value reports a click on it.
fn inspector_card_header(ui: &mut egui::Ui, title: &str, closable: bool) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(qs_theme::font::CAPTION as f32)
                .strong()
                .color(theme::text_tertiary()),
        );
        if closable {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close = crate::components::pill_button(ui, "Done", "Close (Esc)");
            });
        }
    });
    ui.add_space(qs_theme::space::XS as f32);
    close
}
