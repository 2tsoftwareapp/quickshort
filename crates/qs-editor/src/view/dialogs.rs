//! Dialogs — find bar, shortcuts cheat-sheet and the combine layout dialog.


use crate::{
    templates::CombineLayout,
    theme::{self},
    EditorApp,
};
use eframe::egui::{
    self,
};
use qs_doc::{PointF, RectF};

impl EditorApp {
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

}
