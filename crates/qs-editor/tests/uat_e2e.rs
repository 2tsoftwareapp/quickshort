//! End-to-End UAT for `qs-editor`: drives real `egui` UI frames across multiple
//! viewport widths and themes, and exercises the full Snagit workflow.

use eframe::egui;
use image::{Rgba, RgbaImage};
use qs_doc::{
    qsdoc, Anchor, ColorFilter, CutoutAxis, CutoutSeam, Document, PageCurlEffect,
    ReflectionEffect,
};
use qs_edit::{Editor, Tool};
use qs_editor::{
    image_panel,
    templates::{self, CombineLayout},
    theme, toolbar,
};

fn sample_ui_image(w: u32, h: u32) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(w, h, Rgba([242, 244, 248, 255]));
    for y in 16..34 {
        for x in (20..200).step_by(3) {
            img.put_pixel(x, y, Rgba([30, 70, 140, 255]));
        }
    }
    for y in 60..84 {
        for x in (24..260).step_by(3) {
            img.put_pixel(x, y, Rgba([40, 44, 52, 255]));
        }
    }
    img
}

#[test]
fn uat_responsive_toolbar_and_inspector_across_viewports_and_themes() {
    let doc = Document::new(800, 500, 1.0);
    let mut editor = Editor::new(doc);
    let path = std::path::PathBuf::from("/tmp/very_long_screenshot_filename_for_uat_test.qsdoc");

    for &theme_name in &["dark", "light"] {
        qs_theme::set_theme(theme_name);
        for &(w, h) in &[(720.0, 600.0), (1000.0, 700.0), (1280.0, 800.0), (1600.0, 1000.0)] {
            let ctx = egui::Context::default();
            theme::apply_macos_visuals(&ctx);
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::Vec2::new(w, h),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                egui::Panel::top("uat_top").show(ui, |ui| {
                    let _ = toolbar::render_top_bar(ui, &mut editor, 1.0, Some(&path));
                });
                egui::Panel::top("uat_props").show(ui, |ui| {
                    let _ = toolbar::render_properties_bar(ui, &mut editor);
                });
                egui::Panel::right("uat_right")
                    .exact_size(244.0)
                    .show(ui, |ui| {
                        let _ = image_panel::render(ui, &mut editor);
                    });
            });
            out.textures_delta.clear();
            assert!(
                !out.shapes.is_empty(),
                "expected rendered shapes for {theme_name} at {w}x{h}"
            );
        }
    }
}

#[test]
fn uat_full_snagit_workflow_simplify_cutout_effects_save_and_combine() {
    let dir = std::env::temp_dir().join(format!("qs_uat_e2e_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let base1 = sample_ui_image(320, 200);
    let base2 = sample_ui_image(280, 180);
    let p1 = dir.join("shot1.png");
    let p2 = dir.join("shot2.png");
    base1.save(&p1).unwrap();
    base2.save(&p2).unwrap();

    // 1. Simplify UI
    let mut ed = Editor::new(Document::new(320, 200, 1.0));
    let n_blocks = ed.simplify_from_image(&base1);
    assert_eq!(n_blocks, 2, "expected 2 simplified UI blocks");
    assert_eq!(ed.doc().objects.len(), 2);

    // 2. Cutout strip + Undo check
    ed.set_tool(Tool::Cutout);
    assert!(ed.apply_cutout(CutoutAxis::Horizontal, 40.0, 56.0, CutoutSeam::Jagged));
    assert_eq!(qs_render::output_size(ed.doc()), (320, 184));

    // 3. Snagit Effects: ColorFilter + PageCurl + Reflection
    let mut fx = ed.doc().effects.clone();
    fx.color_filter = Some(ColorFilter::Sepia);
    fx.page_curl = Some(PageCurlEffect {
        corner: Anchor::BottomRight,
        size: 24.0,
        shadow: 0.5,
    });
    fx.reflection = Some(ReflectionEffect {
        height: 20.0,
        opacity: 0.4,
        gap: 4.0,
    });
    assert!(ed.preview_effects(fx));
    ed.finish_effects();
    assert_eq!(qs_render::output_size(ed.doc()), (320, 208));

    // 4. Save .qsdoc & reload round-trip
    let qs_path = dir.join("shot1.qsdoc");
    let rendered = qs_render::render(ed.doc(), &base1);
    let thumb = qsdoc::make_thumb(&rendered);
    qsdoc::write(&qs_path, ed.doc(), &base1, Some(&thumb)).unwrap();
    let loaded = qsdoc::read(&qs_path).unwrap();
    assert_eq!(loaded.doc.cutouts.len(), 1);
    assert_eq!(loaded.doc.effects.color_filter, Some(ColorFilter::Sepia));

    // 5. Combine Templates
    let (comb_doc, comb_img) =
        templates::build_combined_document(&[p1, p2], CombineLayout::VerticalSteps, 0xFF1E_1E24)
            .unwrap();
    assert_eq!(comb_doc.objects.len(), 2);
    assert!(comb_img.width() >= 320 && comb_img.height() >= 380);

    let _ = std::fs::remove_dir_all(&dir);
}
