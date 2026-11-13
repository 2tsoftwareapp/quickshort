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
    false
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
