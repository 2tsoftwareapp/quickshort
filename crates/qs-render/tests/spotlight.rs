use image::{Rgba, RgbaImage};
use qs_doc::{Document, ObjectKind, RectF};
use qs_render::render;

fn doc(blur: f32, scale: f32) -> Document {
    let mut d = Document::new(80, 80, scale);
    d.push(ObjectKind::Spotlight { rect: RectF::new(20.0, 20.0, 40.0, 40.0), dim_opacity: 0.5, blur });
    d
}

#[test]
fn spotlight_dims_outside_and_keeps_focus_untouched() {
    let base = RgbaImage::from_pixel(80, 80, Rgba([200, 100, 50, 255]));
    let out = render(&doc(0.0, 1.0), &base);
    assert_eq!(out.get_pixel(40, 40), base.get_pixel(40, 40));
    assert_eq!(out.get_pixel(20, 20), base.get_pixel(20, 20), "edge pixel inside stays bright");
    assert_eq!(out.get_pixel(5, 5), &Rgba([100, 50, 25, 255]), "outside is halved");
    assert_eq!(out.get_pixel(70, 40), &Rgba([100, 50, 25, 255]));
}

#[test]
fn feather_blends_smoothly_outside_the_hole() {
    let base = RgbaImage::from_pixel(80, 80, Rgba([200, 200, 200, 255]));
    let out = render(&doc(10.0, 1.0), &base);
    let near = out.get_pixel(62, 40)[0];
    let far = out.get_pixel(75, 40)[0];
    assert!(near > far && near < 200, "near={near} far={far}");
    assert_eq!(far, 100);
}

#[test]
fn spotlight_is_deterministic_and_roundtrips_and_rejects_bad_dim() {
    let base = RgbaImage::from_pixel(80, 80, Rgba([10, 20, 30, 255]));
    let d = doc(4.0, 1.0);
    assert_eq!(render(&d, &base), render(&d, &base));
    let json = serde_json::to_string(&d).unwrap();
    assert_eq!(Document::from_json(&json).unwrap(), d);
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    v["objects"][0]["kind"]["dim_opacity"] = serde_json::json!(1.5);
    assert!(Document::from_json(&v.to_string()).is_err());
}
