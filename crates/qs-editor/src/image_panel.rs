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
    ui.add_space(6.0);
    ui.label(RichText::new(title).strong().color(theme::text_secondary()));
    ui.add_space(2.0);
}

/// Draw the panel contents; returns true when the document changed.
pub fn render(ui: &mut egui::Ui, editor: &mut Editor) -> bool {
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
        return true;
    }
    effects_ui(ui, editor)
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
