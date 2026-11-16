use image::{Rgba, RgbaImage};
use qs_doc::{Document, Fill, ObjectKind, PointF, RectF, Stroke};
use qs_render::render;

/// White background, one blue rect, one horizontal eraser stroke at y=50.
fn stroke_doc() -> Document {
    let mut d = Document::new(100, 100, 1.0);
    d.background = 0xFFFFFF;
    d.push(ObjectKind::Rect { r: RectF::new(10.0, 10.0, 80.0, 80.0), stroke: Stroke::new(0xFF0000, 0.0), fill: Fill::Solid { rgb: 0x0000FF, alpha: 1.0 }, radius: 0.0 });
    d.push(ObjectKind::Eraser { pts: vec![PointF::new(20.0, 50.0), PointF::new(80.0, 50.0)], width: 10.0 });
    d
}

#[test]
fn eraser_reveals_background_and_keeps_strokes_outside() {
    // Base is mid-gray over a white doc background: erasing must reveal the
    // pre-annotation composite (the base pixel), not the annotation blue.
    let base = RgbaImage::from_pixel(100, 100, Rgba([220, 220, 220, 255]));
    let out = render(&stroke_doc(), &base);
    assert_eq!(out.get_pixel(50, 50).0, [220, 220, 220, 255], "erased centre reveals base pixel");
    let side = out.get_pixel(50, 20).0;
    assert!(side[2] > side[0], "stroke outside eraser is untouched");
}

#[test]
fn eraser_follows_document_scale() {
    // 100x100 logical document at scale 2 renders onto a 200x200 base.
    let mut d = Document::new(200, 200, 2.0);
    d.background = 0xFFFFFF;
    d.push(ObjectKind::Rect { r: RectF::new(20.0, 20.0, 160.0, 160.0), stroke: Stroke::new(0xFF0000, 0.0), fill: Fill::Solid { rgb: 0x0000FF, alpha: 1.0 }, radius: 0.0 });
    d.push(ObjectKind::Eraser { pts: vec![PointF::new(40.0, 50.0), PointF::new(160.0, 50.0)], width: 20.0 });
    let base = RgbaImage::from_pixel(200, 200, Rgba([0, 0, 0, 255]));
    let out = render(&d, &base);
    assert_eq!(out.dimensions(), (200, 200));
    assert_eq!(out.get_pixel(100, 100).0, [0, 0, 0, 255], "logical (50,50) is physical (100,100), reveals black base");
}

#[test]
fn eraser_roundtrips_and_rejects_bad_width() {
    let d = stroke_doc();
    let json = serde_json::to_string(&d).unwrap();
    assert_eq!(Document::from_json(&json).unwrap(), d);
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    v["objects"][1]["kind"]["width"] = serde_json::json!(0.0);
    assert!(Document::from_json(&v.to_string()).is_err());
}
