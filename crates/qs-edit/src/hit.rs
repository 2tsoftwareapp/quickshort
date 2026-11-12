//! Hit testing, resize handles and handle-drag geometry.

use qs_doc::{Fill, ObjectKind, PointF, RectF};
use qs_render::{bounds, shapes, text, text_block};

const MIN_BOX: f32 = 4.0;
pub const MIN_TEXT_W: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HandleId {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
    /// line/arrow endpoints
    Start,
    End,
    /// callout tail tip
    Tail,
    /// step badge radius
    Radius,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle {
    pub id: HandleId,
    pub pos: PointF,
}

pub fn seg_dist(p: PointF, a: PointF, b: PointF) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-9 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0)
    };
    p.dist(PointF::new(a.x + t * dx, a.y + t * dy))
}

pub fn poly_dist(pts: &[PointF], p: PointF, closed: bool) -> f32 {
    match pts.len() {
        0 => f32::INFINITY,
        1 => p.dist(pts[0]),
        n => {
            let segs = if closed { n } else { n - 1 };
            (0..segs)
                .map(|i| seg_dist(p, pts[i], pts[(i + 1) % n]))
                .fold(f32::INFINITY, f32::min)
        }
    }
}

fn filled(f: &Fill) -> bool {
    matches!(f, Fill::Solid { alpha, .. } if *alpha > 0.0)
}

/// Is `p` on / in the object? `tol` is extra slack in logical px.
pub fn hit(kind: &ObjectKind, p: PointF, tol: f32) -> bool {
    use ObjectKind::*;
    match kind {
        Pen { pts, stroke } | Highlighter { pts, stroke } => {
            poly_dist(pts, p, false) <= stroke.width / 2.0 + tol
        }
        Line { a, b, stroke } | Arrow { a, b, stroke, .. } => {
            seg_dist(p, *a, *b) <= stroke.width / 2.0 + tol
        }
        Rect { r, stroke, fill, radius } => {
            let poly = shapes::rrect_poly(*r, *radius);
            (filled(fill) && shapes::point_in_poly(p, &poly))
                || poly_dist(&poly, p, true) <= stroke.width / 2.0 + tol
        }
        Ellipse { r, stroke, fill } => {
            let poly = shapes::ellipse_poly(*r);
            (filled(fill) && shapes::point_in_poly(p, &poly))
                || poly_dist(&poly, p, true) <= stroke.width / 2.0 + tol
        }
        Text { .. } => bounds(kind).inflate(tol).contains(p),
        Callout { shape, body, tail, stroke, .. } => {
            let poly = shapes::callout_poly(*shape, *body, *tail);
            shapes::point_in_poly(p, &poly) || poly_dist(&poly, p, true) <= stroke.width / 2.0 + tol
        }
        Step { center, radius, .. } => p.dist(*center) <= radius + tol,
        Blur { r, .. } => r.normalized().inflate(tol).contains(p),
    }
}

fn h(id: HandleId, x: f32, y: f32) -> Handle {
    Handle { id, pos: PointF::new(x, y) }
}

fn box_handles(r: RectF) -> Vec<Handle> {
    use HandleId::*;
    let r = r.normalized();
    let c = r.center();
    vec![
        h(NW, r.x, r.y),
        h(N, c.x, r.y),
        h(NE, r.right(), r.y),
        h(E, r.right(), c.y),
        h(SE, r.right(), r.bottom()),
        h(S, c.x, r.bottom()),
        h(SW, r.x, r.bottom()),
        h(W, r.x, c.y),
    ]
}

/// Handles to draw / hit for a selected object.
pub fn handles(kind: &ObjectKind) -> Vec<Handle> {
    use ObjectKind::*;
    match kind {
        Rect { r, .. } | Ellipse { r, .. } | Blur { r, .. } => box_handles(*r),
        Callout { body, tail, .. } => {
            let mut v = box_handles(*body);
            v.push(h(HandleId::Tail, tail.x, tail.y));
            v
        }
        Line { a, b, .. } | Arrow { a, b, .. } => vec![h(HandleId::Start, a.x, a.y), h(HandleId::End, b.x, b.y)],
        Step { center, radius, .. } => vec![h(HandleId::Radius, center.x + radius, center.y)],
        Pen { pts, .. } | Highlighter { pts, .. } => {
            RectF::bounding(pts.iter().copied()).map(box_handles).unwrap_or_default()
        }
        Text { .. } => match text_block(kind) {
            Some((o, lay)) => vec![
                h(HandleId::W, o.x, o.y + lay.height / 2.0),
                h(HandleId::E, o.x + lay.width, o.y + lay.height / 2.0),
            ],
            None => vec![],
        },
    }
}

/// New rect after dragging `hd` of `r` to `to`. Dragging past the opposite
/// side flips (the rect is re-normalized).
pub fn resize_rect(r: RectF, hd: HandleId, to: PointF) -> RectF {
    use HandleId::*;
    let r = r.normalized();
    let (mut l, mut t, mut rt, mut b) = (r.x, r.y, r.right(), r.bottom());
    match hd {
        W | NW | SW => l = to.x,
        E | NE | SE => rt = to.x,
        _ => {}
    }
    match hd {
        N | NE | NW => t = to.y,
        S | SE | SW => b = to.y,
        _ => {}
    }
    RectF::from_points(PointF::new(l, t), PointF::new(rt, b))
}

fn min_size(r: RectF) -> RectF {
    RectF::new(r.x, r.y, r.w.max(MIN_BOX), r.h.max(MIN_BOX))
}

fn remap(pts: &mut [PointF], old: RectF, new: RectF) {
    let sx = if old.w > 1e-3 { new.w / old.w } else { 1.0 };
    let sy = if old.h > 1e-3 { new.h / old.h } else { 1.0 };
    for p in pts {
        p.x = new.x + (p.x - old.x) * sx;
        p.y = new.y + (p.y - old.y) * sy;
    }
}

/// `orig` with handle `hd` dragged to `to`. Always computed from the
/// pre-drag object so the result does not accumulate error.
pub fn resized(orig: &ObjectKind, hd: HandleId, to: PointF) -> ObjectKind {
    use ObjectKind::*;
    let mut k = orig.clone();
    match &mut k {
        Rect { r, .. } | Ellipse { r, .. } | Blur { r, .. } => *r = min_size(resize_rect(*r, hd, to)),
        Callout { body, tail, .. } => {
            if hd == HandleId::Tail {
                *tail = to;
            } else {
                *body = min_size(resize_rect(*body, hd, to));
            }
        }
        Line { a, b, .. } | Arrow { a, b, .. } => match hd {
            HandleId::Start => *a = to,
            HandleId::End => *b = to,
            _ => {}
        },
        Step { center, radius, .. } => {
            if hd == HandleId::Radius {
                *radius = center.dist(to).clamp(6.0, 200.0);
            }
        }
        Pen { pts, .. } | Highlighter { pts, .. } => {
            let old = RectF::bounding(pts.iter().copied()).unwrap_or_default();
            remap(pts, old, resize_rect(old, hd, to));
        }
        Text { pos, text: t, style, wrap_w, .. } => {
            let cur = wrap_w.unwrap_or_else(|| text::layout(t, style, None).width).max(MIN_TEXT_W);
            let right = pos.x + cur;
            match hd {
                HandleId::E => *wrap_w = Some((to.x - pos.x).max(MIN_TEXT_W)),
                HandleId::W => {
                    let nx = to.x.min(right - MIN_TEXT_W);
                    pos.x = nx;
                    *wrap_w = Some(right - nx);
                }
                _ => {}
            }
        }
    }
    k
}
