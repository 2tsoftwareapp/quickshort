//! Polygon outlines for shapes and callouts (logical px).

use qs_doc::{CalloutShape, PointF, RectF};

fn dedup(mut v: Vec<PointF>) -> Vec<PointF> {
    v.dedup_by(|a, b| a.dist(*b) < 1e-3);
    if v.len() > 1 && v[0].dist(v[v.len() - 1]) < 1e-3 {
        v.pop();
    }
    v
}

/// Rounded rectangle, clockwise from the top-right corner start.
pub fn rrect_poly(r: RectF, radius: f32) -> Vec<PointF> {
    let r = r.normalized();
    let rad = radius.clamp(0.0, r.w.min(r.h) / 2.0);
    if rad < 0.01 {
        return vec![
            PointF::new(r.x, r.y),
            PointF::new(r.right(), r.y),
            PointF::new(r.right(), r.bottom()),
            PointF::new(r.x, r.bottom()),
        ];
    }
    let segs = ((rad.sqrt() * 3.0).ceil() as usize).clamp(4, 16);
    let corners = [
        (r.right() - rad, r.y + rad, -90.0f32),
        (r.right() - rad, r.bottom() - rad, 0.0),
        (r.x + rad, r.bottom() - rad, 90.0),
        (r.x + rad, r.y + rad, 180.0),
    ];
    let mut v = Vec::with_capacity(4 * (segs + 1));
    for (cx, cy, a0) in corners {
        for i in 0..=segs {
            let a = (a0 + 90.0 * i as f32 / segs as f32).to_radians();
            v.push(PointF::new(cx + rad * a.cos(), cy + rad * a.sin()));
        }
    }
    dedup(v)
}

pub fn ellipse_poly(r: RectF) -> Vec<PointF> {
    let r = r.normalized();
    let c = r.center();
    let (rx, ry) = (r.w / 2.0, r.h / 2.0);
    let n = (((rx + ry) * 0.6) as usize).clamp(48, 160);
    (0..n)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / n as f32 - std::f32::consts::FRAC_PI_2;
            PointF::new(c.x + rx * a.cos(), c.y + ry * a.sin())
        })
        .collect()
}

pub fn body_poly(shape: CalloutShape, body: RectF) -> Vec<PointF> {
    let b = body.normalized();
    match shape {
        CalloutShape::RoundRect => rrect_poly(b, (b.w.min(b.h) * 0.2).clamp(4.0, 14.0)),
        CalloutShape::Pill => rrect_poly(b, b.w.min(b.h) / 2.0),
        CalloutShape::Ellipse => ellipse_poly(b),
    }
}

pub fn point_in_poly(p: PointF, poly: &[PointF]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Half-width of the tail where it leaves the body.
pub fn tail_half_base(body: RectF) -> f32 {
    let b = body.normalized();
    (b.w.min(b.h) * 0.2).clamp(6.0, 18.0)
}

/// One closed outline: the body with a wedge toward `tail` spliced in,
/// so a single stroke traces body + tail with no seam. When the tail point
/// lies inside the body this is just the body.
pub fn callout_poly(shape: CalloutShape, body: RectF, tail: PointF) -> Vec<PointF> {
    let poly = body_poly(shape, body);
    let n = poly.len();
    if n < 3 || point_in_poly(tail, &poly) {
        return poly;
    }
    let c = body.normalized().center();
    let (dx, dy) = (tail.x - c.x, tail.y - c.y);
    let mut cum = Vec::with_capacity(n + 1);
    cum.push(0.0f32);
    for i in 0..n {
        cum.push(cum[i] + poly[i].dist(poly[(i + 1) % n]));
    }
    let total = cum[n];
    // nearest boundary crossing of the ray centre -> tail
    let mut best: Option<(f32, usize, f32)> = None;
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let (ex, ey) = (q.x - p.x, q.y - p.y);
        let den = dx * ey - dy * ex;
        if den.abs() < 1e-9 {
            continue;
        }
        let t = ((p.x - c.x) * ey - (p.y - c.y) * ex) / den;
        let u = ((p.x - c.x) * dy - (p.y - c.y) * dx) / den;
        if t > 0.0 && (0.0..=1.0).contains(&u) && best.is_none_or(|b| t < b.0) {
            best = Some((t, i, u));
        }
    }
    let Some((_, seg, u)) = best else {
        return poly;
    };
    let s0 = cum[seg] + u * (cum[seg + 1] - cum[seg]);
    let hb = tail_half_base(body).min(total / 8.0);
    let at = |s: f32| -> PointF {
        let s = s.rem_euclid(total);
        let i = cum.partition_point(|&c| c <= s).saturating_sub(1).min(n - 1);
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let l = cum[i + 1] - cum[i];
        let f = if l > 1e-6 { (s - cum[i]) / l } else { 0.0 };
        PointF::new(p.x + (q.x - p.x) * f, p.y + (q.y - p.y) * f)
    };
    let (sa, sb) = (s0 + hb, s0 - hb + total);
    let mut out = vec![at(sa)];
    for k in 0..2 * n {
        let s = cum[k % n] + total * (k / n) as f32;
        if s > sa && s < sb {
            out.push(poly[k % n]);
        }
    }
    out.push(at(sb));
    out.push(tail);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrect_inside_rect_bounds() {
        let r = RectF::new(10.0, 20.0, 100.0, 40.0);
        for p in rrect_poly(r, 8.0) {
            assert!(r.inflate(0.01).contains(p));
        }
        assert_eq!(rrect_poly(r, 0.0).len(), 4);
    }

    #[test]
    fn tail_outside_adds_wedge_inside_does_not() {
        let body = RectF::new(0.0, 0.0, 100.0, 50.0);
        let base = body_poly(CalloutShape::RoundRect, body);
        assert_eq!(callout_poly(CalloutShape::RoundRect, body, PointF::new(50.0, 25.0)), base);
        let tail = PointF::new(50.0, 120.0);
        let p = callout_poly(CalloutShape::RoundRect, body, tail);
        assert_eq!(*p.last().unwrap(), tail);
        // wedge leaves through the bottom edge
        assert!(p[0].y > 49.0 && p[p.len() - 2].y > 49.0);
        assert!((p[0].x - p[p.len() - 2].x).abs() > 2.0);
    }

    #[test]
    fn tail_for_every_shape_and_direction() {
        let body = RectF::new(0.0, 0.0, 120.0, 60.0);
        for shape in [CalloutShape::RoundRect, CalloutShape::Pill, CalloutShape::Ellipse] {
            for ang in (0..360).step_by(15) {
                let a = (ang as f32).to_radians();
                let t = PointF::new(60.0 + 200.0 * a.cos(), 30.0 + 200.0 * a.sin());
                let p = callout_poly(shape, body, t);
                assert!(p.len() >= 5, "{shape:?} {ang}");
                assert!(p.iter().all(|q| q.is_finite()));
                assert_eq!(*p.last().unwrap(), t);
            }
        }
    }
}
