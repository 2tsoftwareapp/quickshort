//! Axis-aligned bounds of objects, including stroke width — used for hit
//! testing, selection handles and dirty rects.

use crate::draw::callout_layout;
use crate::text;
use qs_doc::{ObjectKind, PointF, RectF};

pub fn bounds(kind: &ObjectKind) -> RectF {
    use ObjectKind::*;
    match kind {
        Pen { pts, stroke } | Highlighter { pts, stroke } => {
            RectF::bounding(pts.iter().copied()).unwrap_or_default().inflate(stroke.width / 2.0)
        }
        Line { a, b, stroke } => RectF::from_points(*a, *b).inflate(stroke.width / 2.0),
        Arrow { a, b, stroke, .. } => {
            let hl = 6.0 + stroke.width * 2.0;
            RectF::from_points(*a, *b).inflate(stroke.width / 2.0 + hl * 0.5)
        }
        Rect { r, stroke, .. } | Ellipse { r, stroke, .. } => r.normalized().inflate(stroke.width / 2.0),
        Text { pos, text, style, wrap_w, fill } => {
            let lay = text::layout(text, style, *wrap_w);
            let r = RectF::new(pos.x, pos.y, lay.width.max(style.px * 0.5), lay.height);
            if fill.is_none() { r } else { r.inflate(3.0) }
        }
        Callout { body, tail, stroke, .. } => {
            let t = RectF::new(tail.x, tail.y, 0.0, 0.0);
            body.normalized().union(&t).inflate(stroke.width / 2.0)
        }
        Step { center, radius, .. } => RectF::new(center.x - radius, center.y - radius, radius * 2.0, radius * 2.0),
        Blur { r, .. } => r.normalized(),
    }
}

/// Where a text-bearing object's glyph block sits: (origin, layout).
pub fn text_block(kind: &ObjectKind) -> Option<(PointF, text::TextLayout)> {
    match kind {
        ObjectKind::Text { pos, text: t, style, wrap_w, .. } => Some((*pos, text::layout(t, style, *wrap_w))),
        ObjectKind::Callout { body, text: t, style, .. } => Some(callout_layout(*body, t, style)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qs_doc::{CalloutShape, Fill, Stroke, TextStyle};

    #[test]
    fn bounds_cover_stroke_and_tail() {
        let s = Stroke::new(0, 10.0);
        let b = bounds(&ObjectKind::Rect { r: RectF::new(10.0, 10.0, 20.0, 20.0), stroke: s, fill: Fill::None, radius: 0.0 });
        assert_eq!(b, RectF::new(5.0, 5.0, 30.0, 30.0));
        let c = bounds(&ObjectKind::Callout {
            shape: CalloutShape::Pill,
            body: RectF::new(0.0, 0.0, 50.0, 20.0),
            tail: PointF::new(25.0, 80.0),
            stroke: Stroke::new(0, 2.0),
            fill: Fill::None,
            text: String::new(),
            style: TextStyle::default(),
        });
        assert!(c.contains(PointF::new(25.0, 80.0)) && c.contains(PointF::new(0.0, 0.0)));
    }

    #[test]
    fn text_bounds_grow_with_lines() {
        let mk = |t: &str| ObjectKind::Text {
            pos: PointF::new(0.0, 0.0),
            text: t.into(),
            style: TextStyle::default(),
            wrap_w: None,
            fill: Fill::None,
        };
        assert!(bounds(&mk("a\nb\nc")).h > bounds(&mk("a")).h * 2.0);
        assert!(text_block(&mk("a")).is_some());
    }
}
