use image::RgbaImage;
use qs_doc::{qsdoc, Document, Fill, ObjectKind, PointF, RectF, Stroke, TextStyle};
use std::fs;

#[test]
fn test_roundtrip_qsdoc_in_editor_pipeline() {
    let tmp_dir = std::env::temp_dir().join("qs_test_editor");
    fs::create_dir_all(&tmp_dir).unwrap();
    let qsdoc_path = tmp_dir.join("sample.qsdoc");
    let export_path = tmp_dir.join("exported.png");

    let mut doc = Document::new(400, 300, 1.0);
    doc.push(ObjectKind::Rect {
        r: RectF::new(20.0, 20.0, 100.0, 60.0),
        stroke: Stroke::new(0xFF453A, 2.0),
        fill: Fill::Solid { rgb: 0x30D158, alpha: 0.3 },
        radius: 4.0,
    });
    doc.push(ObjectKind::Callout {
        shape: qs_doc::CalloutShape::RoundRect,
        body: RectF::new(150.0, 50.0, 160.0, 60.0),
        tail: PointF::new(120.0, 130.0),
        stroke: Stroke::new(0x0A84FF, 2.0),
        fill: Fill::Solid { rgb: 0xFFFFFF, alpha: 1.0 },
        text: "Xin chào tiếng Việt!\nBe Vietnam Pro font".to_string(),
        style: TextStyle { px: 14.0, bold: false, rgb: 0x000000, align: qs_doc::Align::Left },
    });
    doc.push(ObjectKind::Step {
        center: PointF::new(350.0, 50.0),
        radius: 14.0,
        rgb: 0xBF5AF2,
        text_rgb: 0xFFFFFF,
    });

    let base = RgbaImage::from_pixel(400, 300, image::Rgba([250, 250, 250, 255]));
    let thumb = qsdoc::make_thumb(&base);

    qsdoc::write(&qsdoc_path, &doc, &base, Some(&thumb)).expect("write qsdoc");
    assert!(qsdoc_path.exists());

    let loaded = qsdoc::read(&qsdoc_path).expect("read qsdoc");
    assert_eq!(loaded.doc.objects.len(), 3);
    assert_eq!(loaded.base.width(), 400);
    assert_eq!(loaded.base.height(), 300);

    let flat = qs_render::render(&loaded.doc, &loaded.base);
    flat.save(&export_path).expect("save exported image");
    assert!(export_path.exists());
    assert_eq!(flat.width(), 400);
    assert_eq!(flat.height(), 300);

    let _ = fs::remove_file(&qsdoc_path);
    let _ = fs::remove_file(&export_path);
    let _ = fs::remove_dir(&tmp_dir);
}
