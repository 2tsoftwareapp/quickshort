//! Regression scenarios found while reviewing the P0–P2 editor phases.
use image::{Rgba, RgbaImage};
use qs_doc::{Document, Fill, ObjectKind, PointF, RectF, Stroke};
use qs_edit::{Editor, Motion, Tool};

fn pt(x: f32, y: f32) -> PointF {
    PointF::new(x, y)
}

#[test]
fn live_text_changes_revision_without_splitting_session_undo() {
    let mut e = Editor::new(Document::new(400, 300, 1.0));
    e.set_tool(Tool::Text);
    e.pointer_down(pt(20.0, 20.0));
    e.pointer_up(pt(20.0, 20.0));
    let mut saved_rev = e.revision();
    e.text_insert("Xin chào");
    assert_ne!(
        e.revision(),
        saved_rev,
        "typing must trigger unsaved/recovery"
    );
    saved_rev = e.revision();
    e.text_backspace();
    assert_ne!(
        e.revision(),
        saved_rev,
        "backspace after autosave must be detected"
    );
    e.text_move(Motion::DocStart, false);
    saved_rev = e.revision();
    e.text_delete();
    assert_ne!(e.revision(), saved_rev);
    saved_rev = e.revision();
    e.text_backspace();
    assert_eq!(e.revision(), saved_rev, "a no-op must not change revision");
    e.set_text_px(28.0);
    assert_ne!(
        e.revision(),
        saved_rev,
        "style edits in a session must be dirty"
    );
    e.end_text_edit();
    assert!(e.undo());
    assert!(e.doc().objects.is_empty(), "the whole session is one undo");
    assert!(!e.can_undo());
    assert!(e.redo());
    assert_eq!(e.doc().objects.len(), 1);
}

#[test]
fn new_callout_marks_document_dirty_before_text_edit_ends() {
    let mut e = Editor::new(Document::new(400, 300, 1.0));
    let rev = e.revision();
    e.set_tool(Tool::Callout);
    e.pointer_down(pt(20.0, 20.0));
    e.pointer_move(pt(160.0, 80.0), false);
    e.pointer_up(pt(160.0, 80.0));
    assert!(e.edit().is_some());
    assert_ne!(e.revision(), rev);
    e.end_text_edit();
    assert!(e.undo());
    assert!(e.doc().objects.is_empty());
    assert!(!e.can_undo());
}

#[test]
fn trim_preserves_visible_annotation_inside_the_current_view_only() {
    let mut doc = Document::new(100, 100, 1.0);
    let mut base = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
    for y in 15..85 {
        for x in 15..85 {
            base.put_pixel(x, y, Rgba([40, 40, 40, 255]));
        }
    }
    doc.push(ObjectKind::Rect {
        r: RectF::new(2.0, 2.0, 12.0, 12.0),
        stroke: Stroke::new(0xFF0000, 3.0),
        fill: Fill::None,
        radius: 1.5,
    });
    let bounds = qs_render::bounds(&doc.objects[0].kind);
    let trim = qs_edit::trim::calculate_trim_rect(&doc, &base, 6).unwrap();
    assert!(
        trim.x <= bounds.x && trim.y <= bounds.y,
        "trim clips visible annotation"
    );
    doc.objects[0].visible = false;
    assert_eq!(
        qs_edit::trim::calculate_trim_rect(&doc, &base, 6),
        Some(RectF::new(15.0, 15.0, 70.0, 70.0))
    );
    doc.objects[0].visible = true;
    doc.view = RectF::new(10.0, 10.0, 80.0, 80.0);
    let trim = qs_edit::trim::calculate_trim_rect(&doc, &base, 6).unwrap();
    assert!(
        trim.x >= 10.0 && trim.y >= 10.0,
        "trim must not undo a prior crop"
    );
    assert!(
        trim.x <= 10.0 && trim.y <= 10.0,
        "retain the visible part of the clipped annotation"
    );
}

#[test]
fn cancelling_crop_or_region_drag_preserves_committed_region() {
    for tool in [Tool::Crop, Tool::SelectRegion] {
        let mut e = Editor::new(Document::new(400, 300, 1.0));
        e.set_region(Some(RectF::new(10.0, 10.0, 100.0, 70.0)));
        let region = e.doc().region;
        let rev = e.revision();
        e.set_tool(tool);
        e.pointer_down(pt(5.0, 5.0));
        e.pointer_move(pt(200.0, 200.0), false);
        e.escape();
        assert_eq!(
            e.doc().region,
            region,
            "cancelled {tool:?} drag removed committed region"
        );
        assert_eq!(e.revision(), rev);
        assert!(!e.is_dragging());
        assert_eq!(e.tool(), Tool::Select);
    }
}
