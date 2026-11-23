//! Right-hand "Image" panel: whole-image operations (rotate / flip).

use crate::{
    components::{self, TransformIcon},
    theme,
};
use eframe::egui::{self, RichText};
use qs_edit::Editor;
use qs_theme::space;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrientCmd {
    RotateCw,
    RotateCcw,
    FlipH,
    FlipV,
}

/// One undo step per command.
pub fn apply_orient(editor: &mut Editor, cmd: OrientCmd) {
    match cmd {
        OrientCmd::RotateCw => editor.rotate_cw(),
        OrientCmd::RotateCcw => editor.rotate_ccw(),
        OrientCmd::FlipH => editor.flip_h(),
        OrientCmd::FlipV => editor.flip_v(),
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    components::section_header(ui, title);
}

/// Draw the panel contents; returns true when the document changed.
pub fn render(ui: &mut egui::Ui, editor: &mut Editor) -> bool {
    let mut changed = false;
    // Paint bucket tolerance only makes sense while that tool (or one of its
    // fills) is in play, and it is a style knob, not a document change.
    let fill_active = editor.tool() == qs_edit::Tool::Fill
        || matches!(
            editor.selected_object().map(|o| &o.kind),
            Some(qs_doc::ObjectKind::Fill { .. })
        );
    if fill_active {
        components::card(ui, |ui| {
            section(ui, "Paint bucket");
            // A full-width slider overflows this 236 px panel and pushed the
            // whole inspector out of the window, so the knob is a compact row.
            let mut tolerance = editor.style.fill_tolerance as i32;
            ui.horizontal(|ui| {
                ui.label("Tolerance");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let resp = ui
                        .add(egui::DragValue::new(&mut tolerance).range(0..=255))
                        .on_hover_text(
                            "How far a colour may differ from the pixel you clicked and still fill",
                        );
                    if resp.changed() {
                        editor.style.fill_tolerance = tolerance.clamp(0, 255) as u8;
                    }
                });
            });
        });
        ui.add_space(space::MD as f32);
    }
    if !editor.selection().is_empty() {
        components::card(ui, |ui| {
            changed |= object_ui(ui, editor);
        });
        ui.add_space(space::MD as f32);
    }
    if matches!(
        editor.selected_object().map(|o| &o.kind),
        Some(qs_doc::ObjectKind::Text { .. } | qs_doc::ObjectKind::Callout { .. })
    ) {
        components::card(ui, |ui| {
            changed |= text_ui(ui, editor);
        });
        ui.add_space(space::MD as f32);
    }
    components::card(ui, |ui| {
        section(ui, "Transform");
        let cmds = [
            OrientCmd::RotateCcw,
            OrientCmd::RotateCw,
            OrientCmd::FlipH,
            OrientCmd::FlipV,
        ];
        if let Some(idx) = components::segmented_icon_bar(
            ui,
            &[
                (TransformIcon::RotateCcw, "Rotate 90° counter-clockwise (Ctrl+Shift+R)"),
                (TransformIcon::RotateCw, "Rotate 90° clockwise (Ctrl+R)"),
                (TransformIcon::FlipH, "Flip horizontal"),
                (TransformIcon::FlipV, "Flip vertical"),
            ],
        ) {
            apply_orient(editor, cmds[idx]);
            changed = true;
        }
    });
    ui.add_space(space::MD as f32);
    components::card(ui, |ui| {
        changed |= effects_ui(ui, editor);
    });
    changed
}

/// Selection-level object controls: grouping and opacity.
fn object_ui(ui: &mut egui::Ui, editor: &mut Editor) -> bool {
    if editor.selection().is_empty() {
        return false;
    }
    section(ui, "Object");
    let mut changed = false;

    let count = editor.selection().len();
    ui.label(
        RichText::new(if count == 1 {
            "1 object selected".to_owned()
        } else {
            format!("{count} objects selected")
        })
        .color(theme::text_tertiary()),
    );
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(count >= 2, egui::Button::new("Group"))
            .on_hover_text("Ctrl+G")
            .clicked()
        {
            editor.group_selected();
        }
        if ui
            .add_enabled(editor.selection_grouped(), egui::Button::new("Ungroup"))
            .on_hover_text("Ctrl+Shift+G")
            .clicked()
        {
            editor.ungroup_selected();
        }
    });

    let mut pct = (editor.selected_opacity() * 100.0).round();
    if components::slider_row(ui, "Opacity", &mut pct, 0.0..=100.0, "%", 0)
        && editor.preview_opacity(pct / 100.0)
    {
        changed = true;
    }
    if !ui.input(|i| i.pointer.any_down()) {
        editor.finish_opacity();
    }
    changed
}

/// Typography for the selected text / callout: outline width and colour.
fn text_ui(ui: &mut egui::Ui, editor: &mut Editor) -> bool {
    let style = match editor.selected_object().map(|o| &o.kind) {
        Some(qs_doc::ObjectKind::Text { style, .. })
        | Some(qs_doc::ObjectKind::Callout { style, .. }) => *style,
        _ => return false,
    };
    section(ui, "Text");
    let mut changed = false;
    let mut current = style;
    let mut on = current.outline.is_some();
    if components::switch_row(ui, &mut on, "Outline") {
        current.outline = on.then(|| qs_doc::TextOutline {
            width: (style.px * 0.14).clamp(1.0, 6.0),
            rgb: qs_edit::contrast(style.rgb),
        });
        editor.set_text_outline(current.outline);
        changed = true;
    }
    if let Some(mut outline) = current.outline {
        let mut dirty = false;
        let mut rgb = outline.rgb;
        components::effect_group(ui, |ui| {
            if components::slider_row(ui, "Width", &mut outline.width, 0.5..=12.0, " px", 1) {
                dirty = true;
            }
            let mut swatch = egui::Color32::from_rgb(
                (rgb >> 16) as u8,
                (rgb >> 8) as u8,
                rgb as u8,
            );
            if components::prop_row(ui, "Colour", true, |ui| {
                ui.color_edit_button_srgba(&mut swatch).changed()
            }) {
                let [r, g, b, _] = swatch.to_srgba_unmultiplied();
                rgb = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
                dirty = true;
            }
        });
        if dirty {
            current.outline = Some(qs_doc::TextOutline {
                width: outline.width,
                rgb,
            });
            changed |= editor.preview_text_style(current);
        }
        if !ui.input(|i| i.pointer.any_down()) {
            editor.finish_text_style();
        }
    }
    changed
}

fn color(ui: &mut egui::Ui, label: &str, value: &mut u32) {
    let mut c = egui::Color32::from_rgba_unmultiplied(
        (*value >> 16) as u8,
        (*value >> 8) as u8,
        *value as u8,
        (*value >> 24) as u8,
    );
    components::prop_row(ui, label, true, |ui| {
        if ui.color_edit_button_srgba(&mut c).changed() {
            let [r, g, b, a] = c.to_srgba_unmultiplied();
            *value = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
        }
    });
}

/// `color()` edits an ARGB value; document effects store plain RGB. The alpha
/// byte is padded on the way in (so the picker opens opaque) and dropped on the
/// way out.
fn rgb_color(ui: &mut egui::Ui, label: &str, value: &mut u32) {
    let mut argb = *value | ((u8::MAX as u32) << 24);
    color(ui, label, &mut argb);
    *value = argb & 0x00FF_FFFF; // allow-hex: bit mask, not a colour
}

fn toggle<T: Default>(ui: &mut egui::Ui, label: &str, effect: &mut Option<T>) {
    let mut on = effect.is_some();
    if components::switch_row(ui, &mut on, label) {
        *effect = on.then(T::default);
    }
}

fn anchor_label(a: qs_doc::Anchor) -> &'static str {
    use qs_doc::Anchor::*;
    match a {
        TopLeft => "Top Left",
        Top => "Top",
        TopRight => "Top Right",
        Left => "Left",
        Center => "Center",
        Right => "Right",
        BottomLeft => "Bottom Left",
        Bottom => "Bottom",
        BottomRight => "Bottom Right",
    }
}

fn effects_ui(ui: &mut egui::Ui, editor: &mut Editor) -> bool {
    section(ui, "Effects");
    let mut effects = editor.doc().effects.clone();
    toggle(ui, "Border", &mut effects.border);
    if let Some(b) = &mut effects.border {
        components::effect_group(ui, |ui| {
            components::slider_row(ui, "Width", &mut b.width, 0.0..=256.0, " px", 0);
            components::slider_row(ui, "Radius", &mut b.radius, 0.0..=1024.0, " px", 0);
            components::switch_row(ui, &mut b.inset, "Inside image");
            color(ui, "Color", &mut b.color);
        });
    }
    toggle(ui, "Drop shadow", &mut effects.shadow);
    if let Some(s) = &mut effects.shadow {
        components::effect_group(ui, |ui| {
            components::slider_row(ui, "Blur", &mut s.blur_radius, 0.0..=128.0, " px", 0);
            components::slider_row(ui, "Offset X", &mut s.offset.x, -256.0..=256.0, " px", 0);
            components::slider_row(ui, "Offset Y", &mut s.offset.y, -256.0..=256.0, " px", 0);
            let mut op_pct = (s.opacity * 100.0).round();
            if components::slider_row(ui, "Opacity", &mut op_pct, 0.0..=100.0, "%", 0) {
                s.opacity = (op_pct / 100.0).clamp(0.0, 1.0);
            }
            color(ui, "Color", &mut s.color);
        });
    }
    toggle(ui, "Color adjustments", &mut effects.color_adjust);
    if let Some(c) = &mut effects.color_adjust {
        components::effect_group(ui, |ui| {
            let mut br = (c.brightness * 100.0).round();
            if components::slider_row(ui, "Brightness", &mut br, -100.0..=100.0, "%", 0) {
                c.brightness = (br / 100.0).clamp(-1.0, 1.0);
            }
            let mut ct = (c.contrast * 100.0).round();
            if components::slider_row(ui, "Contrast", &mut ct, -100.0..=100.0, "%", 0) {
                c.contrast = (ct / 100.0).clamp(-1.0, 1.0);
            }
            let mut sat = (c.saturation * 100.0).round();
            if components::slider_row(ui, "Saturation", &mut sat, 0.0..=200.0, "%", 0) {
                c.saturation = (sat / 100.0).clamp(0.0, 2.0);
            }
        });
    }
    toggle(ui, "Colorize", &mut effects.colorize);
    if let Some(cz) = &mut effects.colorize {
        components::effect_group(ui, |ui| {
            rgb_color(ui, "Replace", &mut cz.target);
            rgb_color(ui, "With", &mut cz.replacement);
            let mut tol = cz.tolerance as f32;
            if components::slider_row(ui, "Tolerance", &mut tol, 0.0..=128.0, "", 0) {
                cz.tolerance = tol.round().clamp(0.0, 128.0) as u8;
            }
            let mut st_pct = (cz.strength * 100.0).round();
            if components::slider_row(ui, "Strength", &mut st_pct, 0.0..=100.0, "%", 0) {
                cz.strength = (st_pct / 100.0).clamp(0.0, 1.0);
            }
        });
    }
    let mut filter = effects.color_filter.unwrap_or(qs_doc::ColorFilter::None);
    components::prop_row(ui, "Filter", filter != qs_doc::ColorFilter::None, |ui| {
        egui::ComboBox::from_id_salt("effect_color_filter")
            .width(104.0)
            .selected_text(format!("{filter:?}"))
            .show_ui(ui, |ui| {
                use qs_doc::ColorFilter::*;
                for f in [None, Grayscale, Sepia, Invert] {
                    ui.selectable_value(&mut filter, f, format!("{f:?}"));
                }
            });
    });
    effects.color_filter = (filter != qs_doc::ColorFilter::None).then_some(filter);
    toggle(ui, "Page curl", &mut effects.page_curl);
    if let Some(pc) = &mut effects.page_curl {
        components::effect_group(ui, |ui| {
            components::prop_row(ui, "Corner", true, |ui| {
                egui::ComboBox::from_id_salt("effect_page_curl_corner")
                    .width(108.0)
                    .selected_text(anchor_label(pc.corner))
                    .show_ui(ui, |ui| {
                        use qs_doc::Anchor::*;
                        for a in [TopLeft, TopRight, BottomLeft, BottomRight] {
                            ui.selectable_value(&mut pc.corner, a, anchor_label(a));
                        }
                    });
            });
            components::slider_row(ui, "Size", &mut pc.size, 8.0..=160.0, " px", 0);
            let mut sh_pct = (pc.shadow * 100.0).round();
            if components::slider_row(ui, "Shadow", &mut sh_pct, 0.0..=100.0, "%", 0) {
                pc.shadow = (sh_pct / 100.0).clamp(0.0, 1.0);
            }
        });
    }
    toggle(ui, "Reflection", &mut effects.reflection);
    if let Some(rf) = &mut effects.reflection {
        components::effect_group(ui, |ui| {
            components::slider_row(ui, "Height", &mut rf.height, 8.0..=256.0, " px", 0);
            let mut op_pct = (rf.opacity * 100.0).round();
            if components::slider_row(ui, "Opacity", &mut op_pct, 0.0..=100.0, "%", 0) {
                rf.opacity = (op_pct / 100.0).clamp(0.0, 1.0);
            }
            components::slider_row(ui, "Gap", &mut rf.gap, 0.0..=32.0, " px", 0);
        });
    }
    toggle(ui, "Watermark", &mut effects.watermark);
    if let Some(w) = &mut effects.watermark {
        components::effect_group(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut w.text)
                    .hint_text("Watermark text…")
                    .desired_width(f32::INFINITY),
            );
            components::prop_row(ui, "Position", true, |ui| {
                egui::ComboBox::from_id_salt("effect_watermark_pos")
                    .width(108.0)
                    .selected_text(anchor_label(w.anchor))
                    .show_ui(ui, |ui| {
                        use qs_doc::Anchor::*;
                        for a in [
                            TopLeft, Top, TopRight, Left, Center, Right, BottomLeft, Bottom,
                            BottomRight,
                        ] {
                            ui.selectable_value(&mut w.anchor, a, anchor_label(a));
                        }
                    });
            });
            components::slider_row(ui, "Size", &mut w.font_scale, 0.25..=6.0, "×", 2);
            let mut op_pct = (w.opacity * 100.0).round();
            if components::slider_row(ui, "Opacity", &mut op_pct, 0.0..=100.0, "%", 0) {
                w.opacity = (op_pct / 100.0).clamp(0.0, 1.0);
            }
            color(ui, "Color", &mut w.color);
        });
    }
    toggle(ui, "Torn edge", &mut effects.edge);
    if let Some(e) = &mut effects.edge {
        components::effect_group(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = space::XS as f32;
                components::toggle_chip(ui, &mut e.top, "Top");
                components::toggle_chip(ui, &mut e.bottom, "Bottom");
                components::toggle_chip(ui, &mut e.left, "Left");
                components::toggle_chip(ui, &mut e.right, "Right");
            });
            components::slider_row(ui, "Depth", &mut e.amplitude, 0.0..=128.0, " px", 0);
            let mut seed = e.seed;
            components::prop_row(ui, "Seed", true, |ui| {
                if ui.add(egui::DragValue::new(&mut seed)).changed() {
                    e.seed = seed;
                }
            });
        });
    }
    let changed = editor.preview_effects(effects);
    if !ui.input(|i| i.pointer.any_down()) && !ui.ctx().egui_wants_keyboard_input() { editor.finish_effects(); }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use qs_doc::{Document, Orientation};

    #[test]
    fn each_command_is_one_undo_step_in_output_space() {
        let mut e = Editor::new(Document::new(40, 20, 1.0));
        apply_orient(&mut e, OrientCmd::RotateCw);
        apply_orient(&mut e, OrientCmd::FlipV);
        assert_eq!(e.doc().orientation, Orientation::Transverse);
        apply_orient(&mut e, OrientCmd::FlipH);
        apply_orient(&mut e, OrientCmd::RotateCcw);
        assert!(e.undo());
        assert!(e.undo());
        assert!(e.undo());
        assert_eq!(e.doc().orientation, Orientation::Rotate90);
    }
}
