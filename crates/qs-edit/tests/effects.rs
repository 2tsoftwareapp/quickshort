use qs_doc::{BorderEffect, Document, Effects, PointF};
use qs_edit::{Editor, Tool};

fn border() -> Effects {
    Effects {
        border: Some(BorderEffect::default()),
        ..Default::default()
    }
}

#[test]
fn slider_frames_commit_once_and_undo_restores_full_effects() {
    let mut e = Editor::new(Document::new(40, 30, 1.0));
    for width in [1.0, 2.0, 3.0, 8.0] {
        let mut effects = border();
        effects.border.as_mut().unwrap().width = width;
        assert!(e.preview_effects(effects));
    }
    e.finish_effects();
    e.finish_effects();
    assert!(e.undo());
    assert_eq!(e.doc().effects, Effects::default());
    assert!(!e.undo());
    assert!(e.redo());
    assert_eq!(e.doc().effects.border.as_ref().unwrap().width, 8.0);
    let rev = e.revision();
    assert!(!e.set_effects(e.doc().effects.clone()));
    assert_eq!(e.revision(), rev);
}

#[test]
fn pending_numeric_effect_and_canvas_drawing_have_separate_undo_steps() {
    let mut e = Editor::new(Document::new(40, 30, 1.0));
    assert!(e.preview_effects(border()));
    e.set_tool(Tool::Rect);
    e.pointer_down(PointF::new(5.0, 5.0));
    e.pointer_move(PointF::new(20.0, 20.0), false);
    e.pointer_up(PointF::new(20.0, 20.0));
    e.finish_effects();
    assert!(e.undo());
    assert!(e.doc().objects.is_empty());
    assert_eq!(e.doc().effects, border());
    assert!(e.undo());
    assert_eq!(e.doc().effects, Effects::default());
    assert!(!e.undo());
}

#[test]
fn invalid_effects_do_not_change_document_or_history() {
    let mut e = Editor::new(Document::new(40, 30, 1.0));
    let mut bad = border();
    bad.border.as_mut().unwrap().width = f32::NAN;
    assert!(!e.preview_effects(bad.clone()));
    assert!(!e.set_effects(bad));
    assert!(!e.undo());
    assert_eq!(e.doc().effects, Effects::default());
}
