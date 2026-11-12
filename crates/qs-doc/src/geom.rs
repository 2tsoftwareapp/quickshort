//! f32 logical geometry.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct PointF {
    pub x: f32,
    pub y: f32,
}

impl PointF {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn offset(self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }
    pub fn dist(self, o: PointF) -> f32 {
        ((self.x - o.x).powi(2) + (self.y - o.y).powi(2)).sqrt()
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct SizeF {
    pub w: f32,
    pub h: f32,
}

impl SizeF {
    pub const fn new(w: f32, h: f32) -> Self {
        Self { w, h }
    }
    pub fn is_finite(self) -> bool {
        self.w.is_finite() && self.h.is_finite()
    }
}

/// Axis-aligned rect; `w`/`h` may be negative while dragging — call
/// [`RectF::normalized`] before storing.
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct RectF {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectF {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Rect spanning two corner points in any order.
    pub fn from_points(a: PointF, b: PointF) -> Self {
        Self::new(a.x, a.y, b.x - a.x, b.y - a.y).normalized()
    }

    pub fn normalized(self) -> Self {
        let (x, w) = if self.w < 0.0 {
            (self.x + self.w, -self.w)
        } else {
            (self.x, self.w)
        };
        let (y, h) = if self.h < 0.0 {
            (self.y + self.h, -self.h)
        } else {
            (self.y, self.h)
        };
        Self { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn center(&self) -> PointF {
        PointF::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub fn offset(self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }
    pub fn inflate(self, d: f32) -> Self {
        Self::new(self.x - d, self.y - d, self.w + 2.0 * d, self.h + 2.0 * d)
    }
    pub fn contains(&self, p: PointF) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }
    pub fn intersects(&self, o: &RectF) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
    pub fn union(&self, o: &RectF) -> RectF {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        RectF::new(x, y, self.right().max(o.right()) - x, self.bottom().max(o.bottom()) - y)
    }
    pub fn intersect(&self, o: &RectF) -> Option<RectF> {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        (r > x && b > y).then(|| RectF::new(x, y, r - x, b - y))
    }
    /// Bounding box of points; None when empty.
    pub fn bounding(pts: impl IntoIterator<Item = PointF>) -> Option<RectF> {
        let mut it = pts.into_iter();
        let f = it.next()?;
        let (mut x0, mut y0, mut x1, mut y1) = (f.x, f.y, f.x, f.y);
        for p in it {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        Some(RectF::new(x0, y0, x1 - x0, y1 - y0))
    }
    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.w.is_finite() && self.h.is_finite()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_and_ops() {
        let r = RectF::from_points(PointF::new(10.0, 10.0), PointF::new(0.0, 5.0));
        assert_eq!(r, RectF::new(0.0, 5.0, 10.0, 5.0));
        assert!(r.contains(PointF::new(5.0, 7.0)));
        let u = r.union(&RectF::new(20.0, 0.0, 1.0, 1.0));
        assert_eq!(u, RectF::new(0.0, 0.0, 21.0, 10.0));
        assert!(r.intersect(&RectF::new(100.0, 100.0, 1.0, 1.0)).is_none());
        assert_eq!(
            RectF::bounding([PointF::new(1.0, 2.0), PointF::new(-1.0, 5.0)]),
            Some(RectF::new(-1.0, 2.0, 2.0, 3.0))
        );
    }
}
