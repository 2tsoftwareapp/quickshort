//! UAT for the 005-editor-ui-mockup-parity feature: flat grouped tool
//! palette (click-simulated), shortcut subscripts, tool-tag, and Polygon
//! shape rendering end-to-end through `qs_render`.

use eframe::egui::{self, Pos2, Rect, Vec2};
use qs_doc::{poly_points, Document, Fill, ObjectKind, PolyKind, PointF, RectF, Stroke};
use qs_edit::{Editor, Tool};
use qs_editor::{theme, toolbar};

const PALETTE_H: f32 = 40.0;

/// Mirror of `render_tool_palette`'s layout so a click lands on a real
/// button — if the layout formula changes, this helper must too.
fn tool_center(win_w: f32, group: usize, idx: usize) -> Pos2 {
    const GROUP_LEN: [usize; 6] = [3, 3, 9, 4, 4, 1];
    const CHROME_PAD_X: f32 = 12.0; // qs_theme::space::MD
    let n_tools: usize = GROUP_LEN.iter().sum();
    let div_w = 9.0f32;
    let n_divs = 5.0f32;
    let tool_space = win_w - 2.0 * CHROME_PAD_X - n_divs * div_w;
    let tool_w = ((tool_space - 100.0) / n_tools as f32)
        .clamp(24.0, 32.0)
        .min(tool_space / n_tools as f32 - 2.0);
    let before: usize = GROUP_LEN[..group].iter().sum();
    // Left-anchored flat row: stride = tool_w + 2px gap, 9px sep per group.
    let x = CHROME_PAD_X
        + before as f32 * (tool_w + 2.0)
        + group as f32 * div_w
        + idx as f32 * (tool_w + 2.0)
        + tool_w * 0.5;
    Pos2::new(x, PALETTE_H * 0.5)
}

fn texts(out: &egui::FullOutput) -> Vec<String> {
    out.shapes
        .iter()
        .filter_map(|cs| match &cs.shape {
            egui::Shape::Text(ts) => Some(ts.galley.job.text.clone()),
            _ => None,
        })
        .collect()
}

fn palette_frame(
    ctx: &egui::Context,
    editor: &mut Editor,
    w: f32,
    h: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))),
        events,
        ..Default::default()
    };
    let mut out = ctx.run_ui(raw, |ui| {
        egui::Panel::top("pal")
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                toolbar::render_tool_palette(ui, editor);
            });
    });
    out.textures_delta.clear();
    out
}

/// Synthetic click at `p` across two frames on the same context.
fn click(ctx: &egui::Context, editor: &mut Editor, p: Pos2, w: f32, h: f32) {
    // Warm-up frame: let the interest map register every widget before the
    // press resolves ownership (a first-frame press owns nothing).
    palette_frame(ctx, editor, w, h, vec![]);
    palette_frame(
        ctx,
        editor,
        w,
        h,
        vec![
            egui::Event::PointerMoved(p),
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    palette_frame(
        ctx,
        editor,
        w,
        h,
        vec![egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
}

#[test]
fn uat_palette_shows_all_tools_with_subscripts_and_tag() {
    for &theme_name in &["dark", "light"] {
        qs_theme::set_theme(theme_name);
        for &w in &[900.0f32, 1280.0, 1600.0] {
            let ctx = egui::Context::default();
            theme::sync_egui_visuals(&ctx);
            let mut editor = Editor::new(Document::new(800, 500, 1.0));
            let out = palette_frame(&ctx, &mut editor, w, 600.0, vec![]);
            assert!(!out.shapes.is_empty(), "palette rendered nothing at {w}");
            let texts = texts(&out);
            // Tool-tag: the active tool's name sits at the row's right edge.
            assert!(
                texts.iter().any(|t| t == "Select"),
                "no tool-tag at {w}/{theme_name}: {texts:?}"
            );
            // Shortcut subscripts — the four new shape keys are unique to the
            // palette, so their presence proves subscripts render.
            for k in ["N", "D", "J", "W"] {
                assert!(
                    texts.iter().any(|t| t.trim() == k),
                    "missing subscript {k} at {w}/{theme_name}"
                );
            }
        }
    }
}

#[test]
fn uat_palette_click_arms_each_group_representative() {
    qs_theme::set_theme("dark");
    let ctx = egui::Context::default();
    theme::sync_egui_visuals(&ctx);
    let mut editor = Editor::new(Document::new(800, 500, 1.0));
    let w = 1280.0f32;
    // (group, idx, expected) — one tool per mockup group.
    for (g, i, want) in [
        (0, 2, Tool::SmartMove),
        (1, 0, Tool::Pen),
        (2, 6, Tool::Star),
        (2, 4, Tool::Triangle),
        (3, 2, Tool::Step),
        (4, 2, Tool::Magnify),
        (5, 0, Tool::Crop),
        (0, 0, Tool::Select),
    ] {
        click(&ctx, &mut editor, tool_center(w, g, i), w, 600.0);
        assert_eq!(editor.tool(), want, "click on group {g} idx {i}");
    }
}

#[test]
fn uat_crop_button_is_inside_the_minimum_width_viewport() {
    let ctx = egui::Context::default();
    let mut editor = Editor::new(Document::new(800, 500, 1.0));
    let width = 640.0;
    let crop = tool_center(width, 5, 0);
    assert!(crop.x < width, "Crop is outside the window");
    click(&ctx, &mut editor, crop, width, 480.0);
    assert_eq!(editor.tool(), Tool::Crop);
}

#[test]
fn uat_tool_keys_are_unique_and_dispatchable() {
    let mut seen = std::collections::HashSet::new();
    for &t in Tool::ALL {
        let c = t.key();
        assert!(seen.insert(c), "duplicate shortcut {c}");
        let name = c.to_ascii_uppercase().to_string();
        assert!(
            egui::Key::from_name(&name).is_some(),
            "{name} is not a valid egui key for {t:?}"
        );
    }
}

#[test]
fn uat_polygon_renders_pixels_for_every_kind() {
    let base = image::RgbaImage::from_pixel(120, 120, image::Rgba([255, 255, 255, 255]));
    for kind in PolyKind::ALL {
        let mut doc = Document::new(120, 120, 1.0);
        doc.push(ObjectKind::Polygon {
            kind,
            r: RectF::new(20.0, 20.0, 60.0, 60.0),
            stroke: Stroke::new(0xFF0000, 4.0),
            fill: Fill::Solid {
                rgb: 0x0000FF,
                alpha: 1.0,
            },
        });
        let img = qs_render::render(&doc, &base);
        // Fill must have painted pixels inside r that differ from the base.
        let centre = img.get_pixel(50, 50);
        assert_ne!(
            centre.0,
            [255, 255, 255, 255],
            "{kind:?} centre pixel untouched — polygon did not render"
        );
        // And the bounding box outside r stays white (no stray geometry).
        let corner = img.get_pixel(5, 5);
        assert_eq!(corner.0, [255, 255, 255, 255], "{kind:?} painted outside r");
    }
}

#[test]
fn uat_polygon_geometry_matches_hit_and_bounds() {
    for kind in PolyKind::ALL {
        let r = RectF::new(10.0, 10.0, 80.0, 60.0);
        let pts = poly_points(kind, r);
        let k = ObjectKind::Polygon {
            kind,
            r,
            stroke: Stroke::new(0xFF0000, 2.0),
            fill: Fill::Solid {
                rgb: 0x0000FF,
                alpha: 1.0,
            },
        };
        // Centre of a filled polygon must be a hit; far corner must not.
        let c = PointF::new(50.0, 40.0);
        assert!(
            qs_edit::hit::hit(&k, c, 0.0),
            "{kind:?} centre not hittable"
        );
        assert!(!qs_edit::hit::hit(&k, PointF::new(200.0, 200.0), 0.0));
        // Rendered vertices must lie inside the declared bounds.
        let b = qs_render::bounds(&k);
        for p in &pts {
            assert!(b.inflate(1.0).contains(*p), "{kind:?} vertex outside bounds");
        }
    }
}
