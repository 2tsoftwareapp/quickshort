use image::{Rgba, RgbaImage};
use qs_doc::Document;
use qs_render::render;

fn doc(effects: serde_json::Value) -> Document {
    let mut json = serde_json::to_value(Document::new(240, 150, 1.0)).unwrap();
    json["effects"] = effects;
    Document::from_json(&json.to_string()).unwrap()
}
fn ink_bounds(img: &RgbaImage) -> (u32,u32,u32,u32) {
    let mut b = (img.width(),img.height(),0,0);
    for (x,y,p) in img.enumerate_pixels() {
        if p[0] < 245 { b.0=b.0.min(x); b.1=b.1.min(y); b.2=b.2.max(x); b.3=b.3.max(y); }
    }
    assert!(b.0 < b.2 && b.1 < b.3,"watermark must have visible glyphs");
    b
}
#[test]
fn watermark_renders_all_nine_anchors_and_zero_opacity_is_identity() {
    let base = RgbaImage::from_pixel(240,150,Rgba([255,255,255,255]));
    for (row, names) in [["top_left","top","top_right"],["left","center","right"],["bottom_left","bottom","bottom_right"]].iter().enumerate() {
        for (col,anchor) in names.iter().enumerate() {
            let d = doc(serde_json::json!({"watermark":{"text":"QS","anchor":anchor,"opacity":0.5,"font_scale":1.0,"color":4278190080u32}}));
            let out = render(&d,&base);
            let (x0,y0,x1,y1) = ink_bounds(&out);
            assert_eq!(out.dimensions(),base.dimensions());
            assert!((x0+x1)/2 >= [0,90,170][col] && (x0+x1)/2 <= [70,150,240][col],"{anchor} wrong x");
            assert!((y0+y1)/2 >= [0,45,100][row] && (y0+y1)/2 <= [50,105,150][row],"{anchor} wrong y");
            assert!(out.pixels().all(|p|p[0]>=125),"watermark ignores opacity");
        }
    }
    let d=doc(serde_json::json!({"watermark":{"text":"QS","opacity":0.0}}));
    assert_eq!(render(&d,&base),base);
}
#[test]
fn watermark_font_scale_changes_ink_extent() {
    let base=RgbaImage::from_pixel(240,150,Rgba([255,255,255,255]));
    let small=ink_bounds(&render(&doc(serde_json::json!({"watermark":{"text":"QS","font_scale":0.5}})),&base));
    let large=ink_bounds(&render(&doc(serde_json::json!({"watermark":{"text":"QS","font_scale":2.0}})),&base));
    assert!(large.2-large.0 > 2*(small.2-small.0));
}
#[test]
fn torn_edge_is_repeatable_and_only_changes_selected_edges() {
    let base=RgbaImage::from_pixel(240,150,Rgba([30,60,90,255]));
    let mut d=doc(serde_json::json!({"edge":{"top":true,"bottom":false,"left":false,"right":false,"amplitude":12.0,"seed":7}}));
    let out=render(&d,&base);
    assert_eq!(out,render(&d,&base));
    assert!(out.rows().next().unwrap().all(|p|p[3]==0));
    assert_eq!(out.get_pixel(120,25),base.get_pixel(120,25));
    assert_eq!(out.get_pixel(0,75),base.get_pixel(0,75));
    assert_eq!(out.get_pixel(239,149),base.get_pixel(239,149));
    let mut json=serde_json::to_value(&d).unwrap();json["effects"]["edge"]["seed"]=serde_json::json!(8);
    d=Document::from_json(&json.to_string()).unwrap();
    assert_ne!(out,render(&d,&base),"seed must control edge pattern");
}
#[test]
fn invalid_watermark_and_edge_inputs_are_rejected() {
    let mut j=serde_json::to_value(Document::new(20,20,1.0)).unwrap();
    for bad in [serde_json::json!({"watermark":{"opacity":2.0}}),serde_json::json!({"watermark":{"font_scale":-1.0}}),serde_json::json!({"edge":{"amplitude":1e20}})] {
        j["effects"]=bad;assert!(Document::from_json(&j.to_string()).is_err());
    }
}
