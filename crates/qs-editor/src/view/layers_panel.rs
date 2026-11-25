//! Layers panel — the animated left rail listing document objects.


use crate::{
    theme::{self},
    EditorApp,
};
use eframe::egui::{
    self, Color32, Rect, Stroke, Vec2,
};
use qs_edit::Tool;

impl EditorApp {
    /// Left layers panel — object list with reorder/lock/visibility.
    pub(crate) fn layers_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
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

    }
}
