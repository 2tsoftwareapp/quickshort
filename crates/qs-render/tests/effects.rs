use image::{Rgba, RgbaImage};
use qs_doc::Document;
use qs_render::{output_size, render};

fn effected(w: u32, h: u32, effects: serde_json::Value) -> Document {
    let mut value = serde_json::to_value(Document::new(w, h, 1.0)).unwrap();
    value["effects"] = effects;
    Document::from_json(&value.to_string()).unwrap()
}

#[test]
fn outset_border_extends_pixels_without_scaling_content() {
    let d = effected(
        8,
        6,
        serde_json::json!({"border":{"width":2.0,"radius":0.0,"inset":false,"color":4294901760u32}}),
    );
    let base = RgbaImage::from_fn(8, 6, |x, y| Rgba([x as u8 * 20, y as u8 * 20, 90, 255]));
    let out = render(&d, &base);
    assert_eq!(out.dimensions(), (12, 10));
    assert_eq!(output_size(&d), (12, 10));
    assert_eq!(out.get_pixel(0, 5).0, [255, 0, 0, 255]);
    for (x, y, p) in base.enumerate_pixels() {
        assert_eq!(out.get_pixel(x + 2, y + 2), p);
    }
}

#[test]
fn inset_round_border_clips_corners_and_keeps_dimensions() {
    let d = effected(
        30,
        20,
        serde_json::json!({"border":{"width":2.0,"radius":8.0,"inset":true,"color":4294901760u32}}),
    );
    let out = render(&d, &RgbaImage::from_pixel(30, 20, Rgba([0, 100, 200, 255])));
    assert_eq!(out.dimensions(), (30, 20));
    assert_eq!(out.get_pixel(0, 0)[3], 0);
    assert_eq!(out.get_pixel(15, 0).0, [255, 0, 0, 255]);
    assert_eq!(out.get_pixel(15, 10).0, [0, 100, 200, 255]);
}

#[test]
fn gaussian_shadow_uses_source_alpha_and_preserves_negative_offset_halo() {
    let mut d = effected(
        20,
        20,
        serde_json::json!({"shadow":{"blur_radius":2.0,"offset":{"x":-4.0,"y":3.0},"opacity":0.5,"color":4278190080u32}}),
    );
    d.background = 0;
    let base = RgbaImage::from_pixel(20, 20, Rgba([200, 100, 50, 128]));
    let out = render(&d, &base);
    assert_eq!(out.dimensions(), (32, 32));
    // content origin (10,3); shadow bounds include 3 sigma on every side.
    assert!(out.get_pixel(6, 15)[3] > 20 && out.get_pixel(6, 15)[3] < 70);
    assert!(out.get_pixel(3, 15)[3] < out.get_pixel(6, 15)[3]);
    assert!(out.get_pixel(20, 15)[0] > 120, "content above shadow");
    let empty = render(&d, &RgbaImage::new(20, 20));
    assert!(
        empty.pixels().all(|p| p[3] == 0),
        "transparent image casts no shadow"
    );
}

#[test]
fn rounded_border_has_no_translucent_seam_inside_opaque_frame() {
    let d = effected(
        30,
        20,
        serde_json::json!({"border":{"width":2.0,"radius":8.0,"inset":false,"color":4294901760u32}}),
    );
    let out = render(&d, &RgbaImage::from_pixel(30, 20, Rgba([0, 100, 200, 255])));
    assert_eq!(out.get_pixel(5, 2)[3], 255);
}

#[test]
fn fractional_crop_rounding_to_one_pixel_is_not_an_error() {
    let mut d = Document::new(4, 4, 1.0);
    d.view = qs_doc::RectF::new(0.0, 0.0, 1.2, 1.2);
    let out = render(&d, &RgbaImage::from_pixel(4, 4, Rgba([20, 40, 60, 255])));
    assert_eq!(out.dimensions(), (1, 1));
    assert_eq!(out.get_pixel(0, 0).0, [20, 40, 60, 255]);
}

#[test]
fn legacy_documents_keep_identical_pixels_and_reject_invalid_effects() {
    let d = Document::new(7, 5, 1.0);
    let mut legacy = serde_json::to_value(&d).unwrap();
    legacy.as_object_mut().unwrap().remove("effects");
    let old = Document::from_json(&legacy.to_string()).unwrap();
    let base = RgbaImage::from_pixel(7, 5, Rgba([123, 45, 67, 255]));
    assert_eq!(render(&old, &base), base);
    legacy["effects"] =
        serde_json::json!({"border":{"width":1e20,"radius":0.0,"inset":false,"color":0}});
    assert!(Document::from_json(&legacy.to_string()).is_err());
}
