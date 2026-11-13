use eframe::egui::{Pos2, Rect, Vec2};
use image::{Rgba, RgbaImage};
use qs_doc::{
    qsdoc, BorderEffect, Document, Effects, Fill, ObjectKind, Orientation, PointF, RectF,
    ShadowEffect, Stroke,
};
use qs_edit::Editor;
use qs_editor::canvas::CanvasView;

fn specimen(orientation: Orientation) -> (Document, RgbaImage) {
    let mut d = Document::new(100, 80, 2.0);
    d.view = RectF::new(5.0, 6.0, 30.0, 25.0);
    d.orientation = orientation;
    d.effects = Effects {
        border: Some(BorderEffect {
            width: 3.0,
            radius: 2.0,
            inset: false,
            ..Default::default()
        }),
        shadow: Some(ShadowEffect {
            blur_radius: 1.0,
            offset: PointF::new(-2.0, 3.0),
            ..Default::default()
        }),
        ..Default::default()
    };
    d.push(ObjectKind::Rect {
        r: RectF::new(10.0, 10.0, 10.0, 8.0),
        stroke: Stroke::new(0xff0000, 2.0),
        fill: Fill::None,
        radius: 0.0,
    });
    (d, RgbaImage::from_pixel(100, 80, Rgba([60, 70, 80, 255])))
}

#[test]
fn rotated_effects_round_trip_without_mutating_base_or_export_pixels() {
    let (d, base) = specimen(Orientation::Rotate90);
    let original = base.clone();
    let flat = qs_render::render(&d, &base);
    assert_eq!(flat.dimensions(), qs_render::output_size(&d));
    let root = std::env::temp_dir().join(format!(
        "qs_effects_probe_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("effects.qsdoc");
    qsdoc::write(&file, &d, &base, None).unwrap();
    let loaded = qsdoc::read(&file).unwrap();
    assert_eq!(loaded.doc.effects, d.effects);
    assert_eq!(loaded.doc.orientation, d.orientation);
    assert_eq!(loaded.base, original, "source pixels changed");
    let after = qs_render::render(&loaded.doc, &loaded.base);
    assert_eq!(after, flat, "save/open changed render");
    let png = root.join("effects.png");
    after.save(&png).unwrap();
    assert_eq!(image::open(&png).unwrap().to_rgba8(), flat);
    assert_eq!(base, original);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn hit_testing_matches_real_pixels_with_rotation_crop_scale_and_shadow_padding() {
    for o in [
        Orientation::Normal,
        Orientation::Rotate90,
        Orientation::Transverse,
    ] {
        let (d, base) = specimen(o);
        let id = d.objects[0].id;
        let p = PointF::new(15.0, 10.0); // middle of top red stroke in document coords
        let mut v = CanvasView::default();
        v.sync(&d);
        let screen = Rect::from_min_size(Pos2::new(25.0, 40.0), Vec2::new(800.0, 500.0));
        v.fit_to(v.display_rect(), screen, 40.0);
        let clicked = v.screen_to_doc(v.doc_to_screen(p, screen), screen);
        assert_eq!(
            Editor::new(d.clone()).pick(clicked),
            Some(id),
            "{o:?} hit mismatch"
        );
        let q = v.map.to_display(p);
        let out = qs_render::render(&d, &base);
        let px = out.get_pixel(
            (q.x * d.scale).floor() as u32,
            (q.y * d.scale).floor() as u32,
        );
        assert!(
            px[0] > 200 && px[1] < 30,
            "{o:?}: mapped point is not on the visible red stroke: {px:?}"
        );
        assert_eq!(
            out.dimensions(),
            (
                (v.display_rect().w * d.scale).round() as u32,
                (v.display_rect().h * d.scale).round() as u32
            )
        );
    }
}
