use image::RgbaImage;
use qs_doc::{qsdoc, Align, ArrowHead, CalloutShape, Document, Fill, ObjectKind, PointF, RectF, Stroke, TextStyle};
use std::path::Path;

fn main() {
    let mut doc = Document::new(700, 480, 1.0);
    doc.push(ObjectKind::Callout {
        shape: CalloutShape::RoundRect,
        body: RectF::new(60.0, 40.0, 260.0, 75.0),
        tail: PointF::new(140.0, 170.0),
        stroke: Stroke::new(0x0A84FF, 2.5),
        fill: Fill::Solid { rgb: 0xFFFFFF, alpha: 1.0 },
        text: "Snagit Style Quickshort\nBe Vietnam Pro hiển thị cực nét!".to_string(),
        style: TextStyle { px: 15.0, bold: false, rgb: 0x1C1C1E, align: Align::Left },
    });
    doc.push(ObjectKind::Arrow {
        a: PointF::new(140.0, 220.0),
        b: PointF::new(320.0, 220.0),
        stroke: Stroke::new(0xFF453A, 3.6),
        head: ArrowHead::Filled,
    });
    doc.push(ObjectKind::Step {
        center: PointF::new(360.0, 220.0),
        radius: 14.0,
        rgb: 0x30D158,
        text_rgb: 0xFFFFFF,
    });
    doc.push(ObjectKind::Rect {
        r: RectF::new(420.0, 160.0, 200.0, 120.0),
        stroke: Stroke::new(0xFF9F0A, 2.5),
        fill: Fill::Solid { rgb: 0xFF9F0A, alpha: 0.15 },
        radius: 6.0,
    });

    let base = RgbaImage::from_pixel(700, 480, image::Rgba([245, 247, 250, 255]));
    let thumb = qsdoc::make_thumb(&base);
    let sample = "/home/vietis/.agents/outputs/quickshort/reports/sample_annotated.qsdoc";
    qsdoc::write(Path::new(sample), &doc, &base, Some(&thumb)).unwrap();
    println!("Wrote sample to {sample}");
}
