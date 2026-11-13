use image::{Rgba, RgbaImage};
use qs_doc::Document;
use qs_render::render;

fn doc(adj: serde_json::Value) -> Document {
    let mut j = serde_json::to_value(Document::new(4, 4, 1.0)).unwrap();
    j["effects"] = serde_json::json!({ "color_adjust": adj });
    j["background"] = serde_json::json!(0);
    Document::from_json(&j.to_string()).unwrap()
}
fn px(d: &Document, c: [u8; 4]) -> [u8; 4] {
    render(d, &RgbaImage::from_pixel(4, 4, Rgba(c))).get_pixel(1, 1).0
}

#[test]
fn defaults_are_identity() {
    let d = doc(serde_json::json!({}));
    assert_eq!(px(&d, [200, 80, 30, 255]), [200, 80, 30, 255]);
}
#[test]
fn brightness_extremes_and_alpha_preserved() {
    assert_eq!(px(&doc(serde_json::json!({"brightness": 1.0})), [10, 20, 30, 128]), [255, 255, 255, 128]);
    assert_eq!(px(&doc(serde_json::json!({"brightness": -1.0})), [200, 100, 50, 77]), [0, 0, 0, 77]);
    let mid = px(&doc(serde_json::json!({"brightness": 0.2})), [100, 100, 100, 255]);
    assert_eq!(mid, [151, 151, 151, 255]);
}
#[test]
fn saturation_zero_is_gray_and_two_boosts() {
    let g = px(&doc(serde_json::json!({"saturation": 0.0})), [200, 80, 30, 255]);
    assert!(g[0] == g[1] && g[1] == g[2]);
    let src = [150u8, 100, 80, 255];
    let hi = px(&doc(serde_json::json!({"saturation": 2.0})), src);
    assert!(hi[0] as i32 - hi[2] as i32 > src[0] as i32 - src[2] as i32);
}
#[test]
fn contrast_pushes_away_from_midpoint() {
    let up = doc(serde_json::json!({"contrast": 0.5}));
    assert!(px(&up, [180, 180, 180, 255])[0] > 180);
    assert!(px(&up, [70, 70, 70, 255])[0] < 70);
    assert_eq!(px(&doc(serde_json::json!({"contrast": -1.0})), [10, 240, 90, 255])[0], 128);
}
#[test]
fn out_of_range_is_rejected() {
    let mut j = serde_json::to_value(Document::new(4, 4, 1.0)).unwrap();
    for bad in [serde_json::json!({"brightness": 1.5}), serde_json::json!({"contrast": -2.0}), serde_json::json!({"saturation": 3.0})] {
        j["effects"] = serde_json::json!({ "color_adjust": bad });
        assert!(Document::from_json(&j.to_string()).is_err());
    }
}
