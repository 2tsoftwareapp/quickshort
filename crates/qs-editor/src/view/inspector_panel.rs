//! Inspector — the right panel hosting the canvas and align cards.


use crate::{
    image_panel,
    theme::{self},
    EditorApp,
};
use eframe::egui::{
    self, Color32, Pos2, Vec2,
};
use qs_doc::RectF;
use qs_edit::{Alignment, Axis};

use super::anchored_insets;

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


    /// Right Inspector panel — style, canvas and align cards.
    pub(crate) fn inspector_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
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

    }
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
