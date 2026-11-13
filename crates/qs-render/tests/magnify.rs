use image::{Rgba, RgbaImage};
use qs_doc::{Document, MagnifyShape, ObjectKind, RectF, Stroke};
use qs_render::render;

#[test]
fn magnifier_samples_enlarged_pixels_clips_to_ellipse_and_draws_border() {
    let base = RgbaImage::from_fn(80, 80, |x, y| Rgba([x as u8 * 3, y as u8 * 3, 0, 255]));
    let mut d = Document::new(80, 80, 1.0);
    d.push(ObjectKind::Magnify {
        rect: RectF::new(20.0, 20.0, 40.0, 40.0), zoom: 2.0,
        shape: MagnifyShape::Ellipse, stroke: Stroke::new(0xFF0000, 2.0),
    });
    let out = render(&d, &base);
    assert_eq!(out.get_pixel(20, 20), base.get_pixel(20, 20), "ellipse clips its corners");
    assert_eq!(out.get_pixel(40, 40), base.get_pixel(40, 40), "lens center samples its center");
    assert!(out.get_pixel(25, 40)[0] > base.get_pixel(25, 40)[0], "left side samples closer to lens center");
    assert_eq!(out.get_pixel(40, 20).0[0], 255, "lens border is rendered on top");
}

#[test]
fn rectangular_magnifier_respects_document_scale_and_is_deterministic() {
    let base = RgbaImage::from_fn(100, 60, |x, y| Rgba([x as u8, y as u8, 10, 255]));
    let mut d = Document::new(100, 60, 2.0);
    d.push(ObjectKind::Magnify {
        rect: RectF::new(10.0, 10.0, 20.0, 20.0), zoom: 4.0,
        shape: MagnifyShape::Rect, stroke: Stroke::new(0xFFFFFF, 1.0),
    });
    let a = render(&d, &base);
    assert_eq!(a.dimensions(), base.dimensions());
    assert_eq!(a, render(&d, &base));
    assert!(a.get_pixel(20, 20)[0] > base.get_pixel(20, 20)[0]);
}

#[test]
fn magnifier_parameters_roundtrip_and_invalid_zoom_is_rejected() {
    let mut d = Document::new(40, 30, 1.0);
    d.push(ObjectKind::Magnify {
        rect: RectF::new(2.0, 3.0, 10.0, 8.0), zoom: 3.0,
        shape: MagnifyShape::Rect, stroke: Stroke::new(0x123456, 2.0),
    });
    let json = serde_json::to_string(&d).unwrap();
    let loaded = Document::from_json(&json).unwrap();
    assert_eq!(loaded, d);
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["objects"][0]["kind"]["zoom"] = serde_json::json!(9.0);
    assert!(Document::from_json(&value.to_string()).is_err());
}

#[test]
fn lens_reads_the_composited_image_below_it() {
    let base = RgbaImage::from_pixel(80, 80, Rgba([180, 180, 180, 255]));
    let mut d = Document::new(80, 80, 1.0);
    d.push(ObjectKind::Rect {
        r: RectF::new(25.0, 25.0, 10.0, 30.0), stroke: Stroke::new(0x0000FF, 0.0),
        fill: qs_doc::Fill::Solid { rgb: 0x0000FF, alpha: 1.0 }, radius: 0.0,
    });
    d.push(ObjectKind::Magnify {
        rect: RectF::new(20.0, 20.0, 40.0, 40.0), zoom: 2.0,
        shape: MagnifyShape::Rect, stroke: Stroke::new(0xFFFFFF, 0.0),
    });
    let out = render(&d, &base);
    assert_eq!(out.get_pixel(28, 40).0[2], 255, "underlying blue rectangle is enlarged into the lens");
    assert_eq!(out.get_pixel(32, 40).0[0], 180, "outside the source feature remains gray");
}
