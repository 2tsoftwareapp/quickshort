use image::{Rgba, RgbaImage};
use qs_doc::{Document, ObjectKind, RectF, StampKind};
use qs_render::render;

fn doc(kind: StampKind) -> Document {
    let mut d = Document::new(100, 100, 1.0);
    d.push(ObjectKind::Stamp { rect: RectF::new(20.0, 20.0, 60.0, 60.0), kind, rgb: 0x00AA00 });
    d
}

#[test]
fn every_stamp_kind_paints_and_keeps_its_tint() {
    let base = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
    for kind in StampKind::ALL {
        let out = render(&doc(kind), &base);
        assert_ne!(out, base, "{kind:?} should paint");
        assert!(out.pixels().any(|p| p.0[1] > 150 && p.0[0] < 80), "{kind:?} keeps green tint");
    }
}

#[test]
fn stamp_roundtrips_and_rejects_degenerate_rect() {
    let d = doc(StampKind::Star);
    let json = serde_json::to_string(&d).unwrap();
    assert_eq!(Document::from_json(&json).unwrap(), d);
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    v["objects"][0]["kind"]["rect"]["w"] = serde_json::json!(0.0);
    assert!(Document::from_json(&v.to_string()).is_err());
}
