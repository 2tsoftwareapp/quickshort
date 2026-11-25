//! Draft gestures: the geometry mechanics of "press, drag, release" every
//! annotation surface shares. Each surface maps its armed tool to a
//! [`Gesture`] (see [`crate::Tool::gesture`]), feeds pointer positions
//! through a [`Draft`], and turns the resulting [`DraftShape`] into its own
//! object model — `qs_overlay::Annotation` on screen, [`ObjectKind`] in a
//! document — so the two can never drift on how a drag behaves.

use crate::{ObjectKind, PointF, RectF};

/// The shape of drag a drawing tool performs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    /// Freehand polyline — each move appends a sample (pen, highlighter,
    /// eraser).
    Stroke,
    /// Anchor + cursor endpoint (arrow, line).
    Segment,
    /// Box from anchor to cursor (rect, ellipse, blur, callout, shapes).
    Rect,
    /// Position only — follows the pointer (step badge, text caret).
    Point,
}

/// Geometry produced by a [`Draft`]: preview it live, then commit it into
/// the surface's object model on pointer-up.
#[derive(Clone, Debug, PartialEq)]
pub enum DraftShape {
    Stroke(Vec<PointF>),
    Segment(PointF, PointF),
    Rect(RectF),
    Point(PointF),
}

/// An in-progress drawing gesture (pointer held down).
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    gesture: Gesture,
    anchor: PointF,
    cur: PointF,
    pts: Vec<PointF>,
}

impl Draft {
    pub fn begin(gesture: Gesture, at: PointF) -> Self {
        Self {
            gesture,
            anchor: at,
            cur: at,
            pts: vec![at],
        }
    }

    pub fn gesture(&self) -> Gesture {
        self.gesture
    }

    pub fn anchor(&self) -> PointF {
        self.anchor
    }

    /// Feed a pointer position. `min_step` drops polyline samples closer
    /// than this to the previous one; pass 0 to keep every sample.
    pub fn update(&mut self, at: PointF, min_step: f32) {
        self.cur = at;
        if self.gesture == Gesture::Stroke && self.pts.last().is_none_or(|l| l.dist(at) >= min_step)
        {
            self.pts.push(at);
        }
    }

    /// Geometry as it currently stands.
    pub fn shape(&self) -> DraftShape {
        match self.gesture {
            Gesture::Stroke => DraftShape::Stroke(self.pts.clone()),
            Gesture::Segment => DraftShape::Segment(self.anchor, self.cur),
            Gesture::Rect => DraftShape::Rect(RectF::from_points(self.anchor, self.cur)),
            Gesture::Point => DraftShape::Point(self.cur),
        }
    }

    /// True when the drag produced nothing worth keeping: a stroke with a
    /// single sample, or an extent smaller than `min` logical px.
    pub fn degenerate(&self, min: f32) -> bool {
        match self.shape() {
            DraftShape::Stroke(p) => p.len() < 2,
            DraftShape::Segment(a, b) => a.dist(b) < min,
            DraftShape::Rect(r) => r.w.abs() < min || r.h.abs() < min,
            DraftShape::Point(_) => false,
        }
    }
}

/// Tail point hanging under a callout body — below center, scaled with its
/// height.
pub fn default_callout_tail(body: RectF) -> PointF {
    let b = body.normalized();
    PointF::new(b.center().x, b.bottom() + (b.h * 0.6).max(24.0))
}

/// Write a draft's current geometry into an existing object. The editor's
/// live-create path calls this on every move; the overlay applies the same
/// shape to its `Annotation` twin instead.
pub fn apply(kind: &mut ObjectKind, shape: DraftShape) {
    match (kind, shape) {
        (
            ObjectKind::Pen { pts, .. }
            | ObjectKind::Highlighter { pts, .. }
            | ObjectKind::Eraser { pts, .. },
            DraftShape::Stroke(p),
        ) => *pts = p,
        (ObjectKind::Arrow { b, .. } | ObjectKind::Line { b, .. }, DraftShape::Segment(_, q)) => {
            *b = q
        }
        (
            ObjectKind::Rect { r, .. }
            | ObjectKind::Ellipse { r, .. }
            | ObjectKind::Blur { r, .. }
            | ObjectKind::Polygon { r, .. },
            DraftShape::Rect(rc),
        ) => *r = rc,
        (
            ObjectKind::Magnify { rect, .. }
            | ObjectKind::Spotlight { rect, .. }
            | ObjectKind::Stamp { rect, .. },
            DraftShape::Rect(rc),
        ) => *rect = rc,
        (ObjectKind::Callout { body, tail, .. }, DraftShape::Rect(rc)) => {
            *body = rc;
            *tail = default_callout_tail(rc);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> PointF {
        PointF::new(x, y)
    }

    #[test]
    fn stroke_accumulates_samples() {
        let mut d = Draft::begin(Gesture::Stroke, p(0.0, 0.0));
        d.update(p(0.4, 0.0), 0.5); // below min_step — dropped
        d.update(p(0.6, 0.0), 0.5);
        assert!(matches!(d.shape(), DraftShape::Stroke(pts) if pts.len() == 2));
        assert!(!d.degenerate(3.0));
    }

    #[test]
    fn segment_and_rect_degenerate() {
        let mut s = Draft::begin(Gesture::Segment, p(0.0, 0.0));
        s.update(p(2.0, 0.0), 0.0);
        assert!(s.degenerate(3.0));
        s.update(p(4.0, 0.0), 0.0);
        assert!(!s.degenerate(3.0));

        let mut r = Draft::begin(Gesture::Rect, p(10.0, 10.0));
        r.update(p(20.0, 30.0), 0.0);
        let DraftShape::Rect(rc) = r.shape() else {
            panic!()
        };
        assert_eq!((rc.x, rc.y, rc.w, rc.h), (10.0, 10.0, 10.0, 20.0));
        assert!(!r.degenerate(3.0));
    }
}
