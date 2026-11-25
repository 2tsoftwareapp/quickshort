//! Canvas panel — the central document surface (texture, overlays, gestures).


use crate::{
    canvas::{self},
    icons::{self, ActionIcon},
    theme::{self, accent_blue, success_green, text_primary},
    EditorApp,
};
use eframe::egui::{
    self, Color32, CursorIcon, FontId, Key, Pos2, Rect, Stroke, StrokeKind, Vec2,
};
use qs_doc::{PointF, RectF};
use qs_edit::{DragHint, HandleId, Tool};
use std::path::Path;

impl EditorApp {
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


    /// Central canvas — document surface, overlays and gestures.
    pub(crate) fn canvas_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
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

    }
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
