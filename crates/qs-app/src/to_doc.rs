//! Overlay annotations → editable `qs_doc::Document` — the hand-off when a
//! capture goes to Quickshort Editor. Overlay coords are screen-logical;
//! document coords are logical relative to the selection's top-left.

use qs_core::{Point, Rect};
use qs_doc::{BlurMode, Document, Fill, ObjectKind, PointF, RectF, Stroke, TextStyle};
use qs_overlay::Annotation;

/// Must match `qs_overlay::annotate::MARKER_A` — the highlighter alpha.
const MARKER_ALPHA: f32 = 0.42;

/// Build a document over the un-annotated `px_w`×`px_h` base, with one
/// object per overlay annotation (same z-order).
pub fn from_overlay(
    px_w: u32,
    px_h: u32,
    scale: f64,
    sel: Rect,
    annots: &[Annotation],
) -> Document {
    let mut doc = Document::new(px_w, px_h, scale as f32);
    let pt = |p: &Point| PointF::new((p.x - sel.x) as f32, (p.y - sel.y) as f32);
    let rc = |r: &Rect| {
        RectF::new(
            (r.x - sel.x) as f32,
            (r.y - sel.y) as f32,
            r.w as f32,
            r.h as f32,
        )
    };
    for a in annots {
        let kind = match a {
            Annotation::Pen { pts, w, rgb } => ObjectKind::Pen {
                pts: pts.iter().map(pt).collect(),
                stroke: Stroke::new(*rgb, *w as f32),
            },
            Annotation::Marker { pts, w, rgb } => ObjectKind::Highlighter {
                pts: pts.iter().map(pt).collect(),
                stroke: Stroke {
                    rgb: *rgb,
                    alpha: MARKER_ALPHA,
                    width: *w as f32,
                },
            },
            Annotation::Arrow { a, b, w, rgb } => ObjectKind::Arrow {
                a: pt(a),
                b: pt(b),
                stroke: Stroke::new(*rgb, *w as f32),
                head: Default::default(),
            },
            Annotation::Rect { r, w, rgb } => ObjectKind::Rect {
                r: rc(r),
                stroke: Stroke::new(*rgb, *w as f32),
                fill: Fill::None,
                radius: 1.5,
            },
            Annotation::Ellipse { r, w, rgb } => ObjectKind::Ellipse {
                r: rc(r),
                stroke: Stroke::new(*rgb, *w as f32),
                fill: Fill::None,
            },
            Annotation::Pixelate(r) => ObjectKind::Blur {
                r: rc(r),
                mode: BlurMode::Pixelate,
                strength: qs_overlay::PIX_BLOCK as f32,
            },
            Annotation::Text {
                pos,
                text,
                px,
                bold,
                rgb,
            } => ObjectKind::Text {
                pos: pt(pos),
                text: text.clone(),
                style: TextStyle {
                    px: *px as f32,
                    bold: *bold,
                    rgb: *rgb,
                    ..Default::default()
                },
                wrap_w: None,
                fill: Fill::None,
            },
        };
        doc.push(kind);
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_every_kind_relative_to_selection() {
        let sel = Rect::new(100, 50, 200, 120);
        let annots = vec![
            Annotation::Arrow {
                a: Point::new(110, 60),
                b: Point::new(200, 150),
                w: 3.6,
                rgb: 0xFF0000,
            },
            Annotation::Pixelate(Rect::new(120, 70, 40, 30)),
            Annotation::Text {
                pos: Point::new(130, 90),
                text: "Xin chào".into(),
                px: 18.0,
                bold: true,
                rgb: 0x00FF00,
            },
            Annotation::Marker {
                pts: vec![Point::new(100, 50), Point::new(150, 50)],
                w: 9.5,
                rgb: 0xFFFF00,
            },
        ];
        let d = from_overlay(400, 240, 2.0, sel, &annots);
        assert_eq!(d.scale, 2.0);
        assert_eq!((d.base_size.w, d.base_size.h), (200.0, 120.0));
        assert_eq!(d.objects.len(), 4);
        match &d.objects[0].kind {
            ObjectKind::Arrow { a, b, .. } => {
                assert_eq!((a.x, a.y), (10.0, 10.0));
                assert_eq!((b.x, b.y), (100.0, 100.0));
            }
            k => panic!("arrow expected, got {k:?}"),
        }
        match &d.objects[1].kind {
            ObjectKind::Blur { r, mode, .. } => {
                assert_eq!((r.x, r.y, r.w, r.h), (20.0, 20.0, 40.0, 30.0));
                assert_eq!(*mode, BlurMode::Pixelate);
            }
            k => panic!("blur expected, got {k:?}"),
        }
        match &d.objects[2].kind {
            ObjectKind::Text {
                pos, text, style, ..
            } => {
                assert_eq!((pos.x, pos.y), (30.0, 40.0));
                assert_eq!(text, "Xin chào");
                assert!(style.bold);
            }
            k => panic!("text expected, got {k:?}"),
        }
        match &d.objects[3].kind {
            ObjectKind::Highlighter { stroke, .. } => assert_eq!(stroke.alpha, MARKER_ALPHA),
            k => panic!("highlighter expected, got {k:?}"),
        }
        d.validate().expect("doc must validate");
    }
}
