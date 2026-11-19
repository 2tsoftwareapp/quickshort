//! Right-hand "Image" panel: whole-image operations (rotate / flip).

use crate::theme;
use eframe::egui::{self, RichText};
use qs_edit::Editor;

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
    theme::section_header(ui, title);
}

/// Draw the panel contents; returns true when the document changed.
pub fn render(ui: &mut egui::Ui, editor: &mut Editor) -> bool {
    let mut changed = false;
    if !editor.selection().is_empty() {
        theme::inspector_card(ui, |ui| {
            changed |= object_ui(ui, editor);
        });
        ui.add_space(8.0);
    }
    if matches!(
        editor.selected_object().map(|o| &o.kind),
        Some(qs_doc::ObjectKind::Text { .. } | qs_doc::ObjectKind::Callout { .. })
    ) {
        theme::inspector_card(ui, |ui| {
            changed |= text_ui(ui, editor);
        });
        ui.add_space(8.0);
    }
    theme::inspector_card(ui, |ui| {
        let mut cmd = None;
        section(ui, "Rotate & flip");
        ui.horizontal_wrapped(|ui| {
            for (c, label, tip) in [
                (OrientCmd::RotateCcw, "Rotate left", "Rotate 90° counter-clockwise (Ctrl+Shift+R)"),
                (OrientCmd::RotateCw, "Rotate right", "Rotate 90° clockwise (Ctrl+R)"),
                (OrientCmd::FlipH, "Flip H", "Mirror left-right"),
                (OrientCmd::FlipV, "Flip V", "Mirror top-bottom"),
            ] {
                if ui.button(label).on_hover_text(tip).clicked() {
                    cmd = Some(c);
                }
            }
        });
        if let Some(c) = cmd {
            apply_orient(editor, c);
            changed = true;
        }
    });
    ui.add_space(8.0);
    theme::inspector_card(ui, |ui| {
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
            changed |= editor.group_selected();
        }
        if ui
            .add_enabled(editor.selection_grouped(), egui::Button::new("Ungroup"))
            .on_hover_text("Ctrl+Shift+G")
            .clicked()
        {
            changed |= editor.ungroup_selected();
        }
    });

    let mut pct = (editor.selected_opacity() * 100.0).round();
    let resp = ui
        .add(egui::Slider::new(&mut pct, 0.0..=100.0).suffix("%").text("Opacity"))
        .on_hover_text("Fades the selected objects; the image below shows through");
    if resp.changed() && editor.preview_opacity(pct / 100.0) {
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
    if ui.checkbox(&mut on, "Outline").changed() {
        current.outline = on.then(|| qs_doc::TextOutline {
            width: (style.px * 0.14).clamp(1.0, 6.0),
            rgb: qs_edit::contrast(style.rgb),
        });
        editor.set_text_outline(current.outline);
        changed = true;
    }
    if let Some(mut outline) = current.outline {
        let mut dirty = false;
        if ui
            .add(egui::Slider::new(&mut outline.width, 0.5..=12.0).text("Outline width"))
            .changed()
        {
            dirty = true;
        }
        let mut rgb = outline.rgb;
        let mut swatch = egui::Color32::from_rgb(
            (rgb >> 16) as u8,
            (rgb >> 8) as u8,
            rgb as u8,
        );
        if ui
            .horizontal(|ui| {
                ui.label("Outline colour");
                ui.color_edit_button_srgba(&mut swatch).changed()
            })
            .inner
        {
            let [r, g, b, _] = swatch.to_srgba_unmultiplied();
            rgb = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
            dirty = true;
        }
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
    let mut c = egui::Color32::from_rgba_unmultiplied((*value >> 16) as u8, (*value >> 8) as u8, *value as u8, (*value >> 24) as u8);
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.color_edit_button_srgba(&mut c).changed() {
            let [r,g,b,a] = c.to_srgba_unmultiplied();
            *value = ((a as u32)<<24)|((r as u32)<<16)|((g as u32)<<8)|b as u32;
        }
    });
}

fn toggle<T: Default>(ui: &mut egui::Ui, label: &str, effect: &mut Option<T>) {
    let mut on = effect.is_some();
    if ui.checkbox(&mut on, label).changed() { *effect = on.then(T::default); }
}

fn effects_ui(ui: &mut egui::Ui, editor: &mut Editor) -> bool {
    section(ui, "Effects");
    let mut effects = editor.doc().effects.clone();
    toggle(ui, "Border", &mut effects.border);
    if let Some(b) = &mut effects.border {
        ui.add(egui::Slider::new(&mut b.width, 0.0..=256.0).text("Width"));
        ui.add(egui::Slider::new(&mut b.radius, 0.0..=1024.0).text("Corner radius"));
        ui.checkbox(&mut b.inset, "Inside image");
        color(ui, "Border color", &mut b.color);
    }
    toggle(ui, "Drop shadow", &mut effects.shadow);
    if let Some(s) = &mut effects.shadow {
        ui.add(egui::Slider::new(&mut s.blur_radius, 0.0..=128.0).text("Blur"));
        ui.add(egui::Slider::new(&mut s.offset.x, -2048.0..=2048.0).text("Offset X"));
        ui.add(egui::Slider::new(&mut s.offset.y, -2048.0..=2048.0).text("Offset Y"));
        ui.add(egui::Slider::new(&mut s.opacity, 0.0..=1.0).text("Opacity"));
        color(ui, "Shadow color", &mut s.color);
    }
    toggle(ui, "Color adjustments", &mut effects.color_adjust);
    if let Some(c) = &mut effects.color_adjust {
        ui.add(egui::Slider::new(&mut c.brightness, -1.0..=1.0).text("Brightness"));
        ui.add(egui::Slider::new(&mut c.contrast, -1.0..=1.0).text("Contrast"));
        ui.add(egui::Slider::new(&mut c.saturation, 0.0..=2.0).text("Saturation"));
    }
    toggle(ui, "Watermark", &mut effects.watermark);
    if let Some(w) = &mut effects.watermark {
        ui.text_edit_singleline(&mut w.text);
        egui::ComboBox::from_label("Position")
            .selected_text(format!("{:?}", w.anchor))
            .show_ui(ui, |ui| {
                use qs_doc::Anchor::*;
                for a in [TopLeft, Top, TopRight, Left, Center, Right, BottomLeft, Bottom, BottomRight] {
                    ui.selectable_value(&mut w.anchor, a, format!("{a:?}"));
                }
            });
        ui.add(egui::Slider::new(&mut w.font_scale, 0.05..=32.0).logarithmic(true).text("Size"));
        ui.add(egui::Slider::new(&mut w.opacity, 0.0..=1.0).text("Watermark opacity"));
        color(ui, "Watermark color", &mut w.color);
    }
    toggle(ui, "Torn edge", &mut effects.edge);
    if let Some(e) = &mut effects.edge {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut e.top, "Top");
            ui.checkbox(&mut e.bottom, "Bottom");
            ui.checkbox(&mut e.left, "Left");
            ui.checkbox(&mut e.right, "Right");
        });
        ui.add(egui::Slider::new(&mut e.amplitude, 0.0..=256.0).text("Depth"));
        let mut seed = e.seed;
        if ui.add(egui::DragValue::new(&mut seed).prefix("Seed ")).changed() {
            e.seed = seed;
        }
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
