//! Round-trip guard for a document with several objects, and the fixture used
//! for manual UAT of the Layers panel (clicks cannot be synthesised on this
//! machine, so a file is the only way to populate the list).

use image::RgbaImage;
use qs_doc::{qsdoc, Document, Fill, ObjectKind, RectF, Stroke, TextStyle};

#[test]
fn a_multi_object_document_round_trips_and_leaves_a_uat_fixture() {
    let mut doc = Document::new(640, 400, 1.0);
    doc.push(ObjectKind::Rect {
        r: RectF::new(40.0, 40.0, 220.0, 120.0),
        stroke: Stroke::new(0x2F6FEB, 3.0),
        fill: Fill::Solid {
            rgb: 0x2F6FEB,
            alpha: 0.18,
        },
        radius: 8.0,
    });
    doc.push(ObjectKind::Text {
        pos: qs_doc::PointF::new(60.0, 200.0),
        text: "Layers row".into(),
        style: TextStyle::default(),
        wrap_w: None,
        fill: Fill::None,
    });
    doc.push(ObjectKind::Ellipse {
        r: RectF::new(360.0, 200.0, 160.0, 100.0),
        stroke: Stroke::new(0xD93025, 3.0),
        fill: Fill::None,
    });

    // Effects on, so the inspector renders its sliders for UAT capture.
    doc.effects.border = Some(qs_doc::BorderEffect::default());
    doc.effects.color_adjust = Some(qs_doc::ColorAdjustEffect::default());
    doc.effects.shadow = Some(qs_doc::ShadowEffect::default());

    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.zwork/uat");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("layers-sample.qsdoc");
    let base = RgbaImage::from_pixel(640, 400, image::Rgba([248, 249, 252, 255]));
    qsdoc::write(&path, &doc, &base, None).unwrap();

    let back = qsdoc::read(&path).unwrap();
    assert_eq!(back.doc.objects.len(), 3, "objects survive the round trip");
    assert_eq!(back.doc.objects.len(), doc.objects.len());
}
