//! qs-render vs the overlay's own rasterizer (`annotate_image`): same
//! annotation, same base, compare pixels. Differences allowed only at
//! anti-aliased edges (tiny-skia coverage vs SDF coverage).

use image::RgbaImage;
use qs_core::{Point, Rect};
use qs_doc::{BlurMode, Document, Fill, ObjectKind, PointF, RectF, Stroke, TextStyle};
use qs_overlay::{annotate_image, Annotation};

const RGB: u32 = 0xFF453A;

fn base(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let v = 190 + ((x / 3 + y / 3) % 2) as u8 * 30;
        image::Rgba([v, v, v, 255])
    })
}

struct Cmp {
    ink: usize,
    bad: usize,
    max: u8,
}

fn compare(a: &RgbaImage, b: &RgbaImage, orig: &RgbaImage, tol: u8) -> Cmp {
    assert_eq!(a.dimensions(), b.dimensions());
    let (mut ink, mut bad, mut max) = (0, 0, 0u8);
    for ((pa, pb), po) in a.pixels().zip(b.pixels()).zip(orig.pixels()) {
        let d = (0..3).map(|k| pa.0[k].abs_diff(pb.0[k])).max().unwrap();
        let changed = (0..3).any(|k| pa.0[k] != po.0[k] || pb.0[k] != po.0[k]);
        if changed {
            ink += 1;
        }
        if d > tol {
            bad += 1;
        }
        max = max.max(d);
    }
    Cmp { ink, bad, max }
}

fn run(scale: f64, ov: Annotation, kind: ObjectKind, tol: u8, name: &str) -> Cmp {
    let (lw, lh) = (160u32, 110u32);
    let (pw, ph) = ((lw as f64 * scale) as u32, (lh as f64 * scale) as u32);
    let orig = base(pw, ph);
    let mut a = orig.clone();
    annotate_image(&mut a, Rect::new(0, 0, lw, lh), &[ov], scale);
    let mut d = Document::new(pw, ph, scale as f32);
    d.push(kind);
    let b = qs_render::render(&d, &orig);
    let c = compare(&a, &b, &orig, tol);
    eprintln!("{name} @{scale}: ink={} bad={} ({:.1}%) maxdiff={}", c.ink, c.bad, 100.0 * c.bad as f32 / c.ink.max(1) as f32, c.max);
    c
}

fn p(x: i32, y: i32) -> Point {
    Point::new(x, y)
}
fn pf(x: i32, y: i32) -> PointF {
    PointF::new(x as f32, y as f32)
}

// Scale 1 only: the overlay's `stroke_cov` compares a *logical* width with a
// *physical* distance, so its strokes come out half as thick at scale 2.
// qs-render scales widths correctly (see `scale_two_doubles_output...`).
#[test]
fn shapes_match_overlay() {
    for scale in [1.0] {
        let w = 3.6f64;
        let st = Stroke::new(RGB, w as f32);
        let cases: Vec<(&str, Annotation, ObjectKind)> = vec![
            (
                "pen",
                Annotation::Pen { pts: vec![p(10, 10), p(60, 40), p(100, 20), p(140, 90)], w, rgb: RGB },
                ObjectKind::Pen { pts: vec![pf(10, 10), pf(60, 40), pf(100, 20), pf(140, 90)], stroke: st },
            ),
            (
                "marker",
                Annotation::Marker { pts: vec![p(10, 50), p(140, 50)], w: 9.5, rgb: RGB },
                ObjectKind::Highlighter { pts: vec![pf(10, 50), pf(140, 50)], stroke: Stroke { rgb: RGB, alpha: 0.42, width: 9.5 } },
            ),
            (
                "arrow",
                Annotation::Arrow { a: p(20, 90), b: p(130, 25), w, rgb: RGB },
                ObjectKind::Arrow { a: pf(20, 90), b: pf(130, 25), stroke: st, head: Default::default() },
            ),
            (
                "rect",
                Annotation::Rect { r: Rect::new(20, 20, 100, 60), w, rgb: RGB },
                ObjectKind::Rect { r: RectF::new(20.0, 20.0, 100.0, 60.0), stroke: st, fill: Fill::None, radius: 1.5 },
            ),
            (
                "ellipse",
                Annotation::Ellipse { r: Rect::new(20, 20, 110, 60), w, rgb: RGB },
                ObjectKind::Ellipse { r: RectF::new(20.0, 20.0, 110.0, 60.0), stroke: st, fill: Fill::None },
            ),
        ];
        for (name, ov, k) in cases {
            let c = run(scale, ov, k, 48, name);
            assert!(c.ink > 100, "{name}: nothing drawn");
            assert!(
                (c.bad as f32) < c.ink as f32 * 0.06,
                "{name} @{scale}: {} of {} ink pixels differ by >48",
                c.bad, c.ink
            );
        }
    }
}

#[test]
fn pixelate_matches_overlay() {
    for scale in [1.0, 2.0] {
        let c = run(
            scale,
            Annotation::Pixelate(Rect::new(16, 16, 80, 56)),
            ObjectKind::Blur { r: RectF::new(16.0, 16.0, 80.0, 56.0), mode: BlurMode::Pixelate, strength: 8.0 },
            0,
            "pixelate",
        );
        assert_eq!(c.bad, 0, "pixelate must be pixel-identical");
    }
}

#[test]
fn text_matches_overlay() {
    for scale in [1.0, 2.0] {
        for bold in [false, true] {
            let c = run(
                scale,
                Annotation::Text { pos: p(12, 20), text: "Quick shot 123".into(), px: 18.0, bold, rgb: RGB },
                ObjectKind::Text {
                    pos: pf(12, 20),
                    text: "Quick shot 123".into(),
                    style: TextStyle { px: 18.0, bold, rgb: RGB, ..Default::default() },
                    wrap_w: None,
                    fill: Fill::None,
                },
                64,
                if bold { "text-bold" } else { "text" },
            );
            assert!(c.ink > 200);
            assert!((c.bad as f32) < c.ink as f32 * 0.03, "text drift {} / {}", c.bad, c.ink);
        }
    }
}
