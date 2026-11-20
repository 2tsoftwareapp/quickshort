//! Rasterize a [`Document`] over its base image with tiny-skia.
//!
//! Output covers `doc.view` at `doc.scale` physical px per logical px.
//! Objects paint in list order; `Blur` objects read whatever is already on
//! the canvas, so z-order decides what they hide.

use crate::shapes::{callout_poly, ellipse_poly, rrect_poly};
use crate::text::{self, TextLayout};
use ab_glyph::{point, Font, ScaleFont};
use image::RgbaImage;
use qs_doc::{
    ArrowHead, BlurMode, Document, Fill, ObjectKind, Orientation, PointF, RectF, SizeF, StampKind, Stroke,
    TextStyle,
};
use tiny_skia::{
    Color, FillRule, IntSize, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, PixmapPaint,
    Transform,
};

/// Logical unit coords of each stamp glyph (drawn into a 100x100 box).
fn stamp_poly(kind: StampKind) -> Option<Vec<(f32, f32)>> {
    use StampKind::*;
    Some(match kind {
        Cursor => vec![(20., 12.), (52., 42.), (38., 48.), (46., 66.), (38., 70.), (30., 52.), (12., 68.)],
        Check => vec![(20., 52.), (42., 74.), (80., 26.)],
        Cross => vec![(28., 28.), (72., 72.), (72., 28.), (28., 72.)],
        Warning => vec![(50., 14.), (92., 82.), (8., 82.)],
        Star => (0..10).map(|i| {
            let a = std::f32::consts::PI / 2.0 + i as f32 * std::f32::consts::PI / 5.0;
            let r = if i % 2 == 0 { 42.0 } else { 18.0 };
            (50.0 + r * a.cos(), 50.0 - r * a.sin())
        }).collect(),
        _ => return None,
    })
}

fn draw_stamp(pm: &mut Pixmap, xf: &Xf, rect: &RectF, kind: StampKind, rgb: u32) {
    let r = rect.normalized();
    let fill = Fill::Solid { rgb, alpha: 1.0 };
    match kind {
        StampKind::Approved | StampKind::Draft | StampKind::Verified => {
            let pad = (r.h * 0.18).max(2.0);
            let body = RectF::new(r.x + pad, r.y + pad, r.w - 2.0 * pad, r.h - 2.0 * pad);
            if body.w > 1.0 && body.h > 1.0 {
                let path = poly_path(&rrect_poly(body, body.h * 0.2), true);
                let stroke = Stroke::new(rgb, body.h * 0.08);
                if let Some(path) = path {
                    fill_path(pm, xf, &path, &fill);
                    stroke_path(pm, xf, &path, &stroke, LineJoin::Round);
                }
                let label = kind.label();
                let px = (body.h * 0.52).min(body.w / (label.len() as f32 * 0.62));
                let st = TextStyle { px, bold: true, rgb: 0xFFFFFF, ..Default::default() };
                let lay = text::layout(label, &st, None);
                draw_text(pm, xf, PointF::new(body.center().x - lay.width / 2.0, body.center().y - lay.height / 2.0), label, &st, &lay);
            }
        }
        _ => {
            let pts = stamp_poly(kind).unwrap_or_default();
            if let Some(path) = poly_path(&pts.iter().map(|(x, y)| PointF::new(r.x + r.w * x / 100.0, r.y + r.h * y / 100.0)).collect::<Vec<_>>(), true) {
                fill_path(pm, xf, &path, &fill);
            }
        }
    }
}

/// Clears pixels along a stroke (round caps) to reveal the canvas background.
/// Works in straight alpha because tiny-skia stores premultiplied pixels.
fn erase_stroke(pm: &mut Pixmap, xf: &Xf, pts: &[PointF], width: f32, snap: &[u8]) {
    let pts: Vec<(f32, f32)> = pts.iter().map(|p| (xf.px(p.x), xf.py(p.y))).collect();
    let radius = width * xf.s / 2.0;
    if pts.is_empty() || radius <= 0.0 { return; }
    let (w, h) = (pm.width() as i32, pm.height() as i32);
    let xs: Vec<f32> = pts.iter().map(|p| p.0).collect();
    let ys: Vec<f32> = pts.iter().map(|p| p.1).collect();
    let (x0, x1) = ((xs.iter().fold(f32::INFINITY, |a, &b| a.min(b)) - radius).floor() as i32, (xs.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)) + radius).ceil() as i32);
    let (y0, y1) = ((ys.iter().fold(f32::INFINITY, |a, &b| a.min(b)) - radius).floor() as i32, (ys.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)) + radius).ceil() as i32);
    let seg = |a: (f32, f32), b: (f32, f32), p: (f32, f32)| -> f32 {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len2 = dx * dx + dy * dy;
        let t = if len2 <= 1e-6 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
        ((p.0 - a.0 - t * dx).powi(2) + (p.1 - a.1 - t * dy).powi(2)).sqrt()
    };
    for y in y0.max(0)..y1.min(h) {
        for x in x0.max(0)..x1.min(w) {
            let p = (x as f32 + 0.5, y as f32 + 0.5);
            let d = pts.windows(2).map(|w| seg(w[0], w[1], p)).fold(f32::INFINITY, f32::min)
                .min(pts.iter().map(|&q| ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt()).fold(f32::INFINITY, f32::min));
            let keep = ((d - radius) / (radius.max(1.0) * 0.5)).clamp(0.0, 1.0);
            if keep >= 1.0 { continue; }
            let i = ((y * w + x) * 4) as usize;
            let px = &mut pm.data_mut()[i..i + 4];
            // Blend toward the captured snapshot (premultiplied space).
            let si = i;
            for c in 0..4 {
                px[c] = (px[c] as f32 * keep + snap[si + c] as f32 * (1.0 - keep)).round() as u8;
            }
        }
    }
}

/// Inner padding of callout text, logical px.
pub const CALLOUT_PAD: f32 = 8.0;
const MAX_OUT: f32 = 32768.0;

/// logical -> output pixel mapping
struct Xf {
    s: f32,
    ox: f32,
    oy: f32,
}

impl Xf {
    fn ts(&self) -> Transform {
        Transform::from_row(self.s, 0.0, 0.0, self.s, self.ox, self.oy)
    }
    fn px(&self, x: f32) -> f32 {
        x * self.s + self.ox
    }
    fn py(&self, y: f32) -> f32 {
        y * self.s + self.oy
    }
}

fn paint(rgb: u32, alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.anti_alias = true;
    p.set_color_rgba8(
        (rgb >> 16) as u8,
        (rgb >> 8) as u8,
        rgb as u8,
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    );
    p
}

fn poly_path(pts: &[PointF], close: bool) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let (f, rest) = pts.split_first()?;
    pb.move_to(f.x, f.y);
    for p in rest {
        pb.line_to(p.x, p.y);
    }
    if close {
        pb.close();
    }
    pb.finish()
}

fn stroke_path(pm: &mut Pixmap, xf: &Xf, path: &Path, s: &Stroke, join: LineJoin) {
    if s.width <= 0.0 || s.alpha <= 0.0 {
        return;
    }
    let st = tiny_skia::Stroke {
        width: s.width,
        line_cap: LineCap::Round,
        line_join: join,
        miter_limit: 4.0,
        ..Default::default()
    };
    pm.stroke_path(path, &paint(s.rgb, s.alpha), &st, xf.ts(), None);
}

fn fill_path(pm: &mut Pixmap, xf: &Xf, path: &Path, f: &Fill) {
    if let Fill::Solid { rgb, alpha } = f {
        if *alpha > 0.0 {
            pm.fill_path(path, &paint(*rgb, *alpha), FillRule::Winding, xf.ts(), None);
        }
    }
}

fn blend(pm: &mut Pixmap, x: i32, y: i32, rgb: u32, a: f32) {
    let (w, h) = (pm.width() as i32, pm.height() as i32);
    if x < 0 || y < 0 || x >= w || y >= h || a <= 0.0 {
        return;
    }
    let a = a.min(1.0);
    let i = ((y * w + x) * 4) as usize;
    let d = &mut pm.data_mut()[i..i + 4];
    let src = [(rgb >> 16) & 255, (rgb >> 8) & 255, rgb & 255];
    for k in 0..3 {
        d[k] = (src[k] as f32 * a + d[k] as f32 * (1.0 - a)).round() as u8;
    }
    d[3] = (255.0 * a + d[3] as f32 * (1.0 - a)).round() as u8;
}

/// Draw a laid-out text block with its top-left at `pos` (logical).
///
/// Italic is a synthetic shear around the baseline, underline / strike are
/// rule rectangles, and `outline` is drawn as a dilated copy of the glyph
/// coverage underneath the fill colour.
fn draw_text(pm: &mut Pixmap, xf: &Xf, pos: PointF, text: &str, style: &TextStyle, lay: &TextLayout) {
    if text.is_empty() {
        return;
    }
    let big = style.px * xf.s;
    let shear = if style.italic { text::ITALIC_SHEAR } else { 0.0 };
    let rule_w = (style.px * 0.07).max(1.0) * xf.s;
    let outline = style
        .outline
        .filter(|o| o.width > 0.0)
        .map(|o| (o.rgb, (o.width * xf.s).round().max(1.0)));
    text::with_font(style.px, style.bold, style.family, |f| {
        let sc = f.as_scaled(style.px);
        for (li, l) in lay.lines.iter().enumerate() {
            let base = li as f32 * lay.line_h + lay.ascent;
            let base_px = xf.py(pos.y + base);
            // Underline / strikethrough rules first: glyphs sit on top.
            if (style.underline || style.strike) && l.width > 0.5 {
                let x0 = xf.px(pos.x + l.x);
                let w = (l.width * xf.s).max(1.0);
                let mut rule = |cy: f32| {
                    let Some(rect) = tiny_skia::Rect::from_xywh(x0, cy, w, rule_w) else {
                        return;
                    };
                    let path = PathBuilder::from_rect(rect);
                    pm.fill_path(
                        &path,
                        &paint(style.rgb, 1.0),
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                };
                if style.underline {
                    rule(base_px + sc.descent().abs() * 0.45);
                }
                if style.strike {
                    rule(base_px - sc.ascent() * 0.32);
                }
            }
            let mut pen = l.x;
            let mut prev = None;
            for c in text[l.start..l.end].chars() {
                let id = sc.glyph_id(c);
                if let Some(p) = prev {
                    pen += sc.kern(p, id);
                }
                if !c.is_whitespace() {
                    let at = point(xf.px(pos.x + pen), base_px);
                    if let Some(og) = f.outline_glyph(id.with_scale_and_position(big, at)) {
                        let bb = og.px_bounds();
                        let mut samples: Vec<(i32, i32, f32)> = Vec::new();
                        og.draw(|gx, gy, cov| {
                            let y = bb.min.y as i32 + gy as i32;
                            let mut x = bb.min.x as f32 + gx as f32;
                            if shear > 0.0 {
                                x += shear * (base_px - y as f32);
                            }
                            samples.push((x.round() as i32, y, cov));
                        });
                        if let Some((rgb, spread)) = outline {
                            let d = spread.round() as i32;
                            let k = ((spread * 0.72).round() as i32).max(1);
                            let ring = [
                                (d, 0),
                                (-d, 0),
                                (0, d),
                                (0, -d),
                                (k, k),
                                (-k, k),
                                (k, -k),
                                (-k, -k),
                            ];
                            for &(x, y, cov) in &samples {
                                for (ox, oy) in ring {
                                    blend(pm, x + ox, y + oy, rgb, cov);
                                }
                            }
                        }
                        for &(x, y, cov) in &samples {
                            blend(pm, x, y, style.rgb, cov);
                        }
                    }
                }
                pen += sc.h_advance(id);
                prev = Some(id);
            }
        }
    });
}

/// Layout + origin of a callout's text: wrapped to the body width, block
/// centred vertically.
pub fn callout_layout(body: RectF, text: &str, style: &TextStyle) -> (PointF, TextLayout) {
    let b = body.normalized();
    let wrap = (b.w - 2.0 * CALLOUT_PAD).max(8.0);
    let lay = text::layout(text, style, Some(wrap));
    let pos = PointF::new(b.x + CALLOUT_PAD, b.y + (b.h - lay.height) / 2.0);
    (pos, lay)
}

fn arrow_geometry(a: PointF, b: PointF, w: f32) -> Option<(f32, [PointF; 2])> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if len < 1e-3 {
        return None;
    }
    let (ux, uy) = (-dx / len, -dy / len);
    let hl = (6.0 + w * 2.0).min(len * 0.35);
    let (c, s) = (28f32.to_radians().cos(), 28f32.to_radians().sin());
    let wing = |t: f32| {
        let wx = ux * c - uy * s * t;
        let wy = ux * s * t + uy * c;
        PointF::new(b.x + wx * hl, b.y + wy * hl)
    };
    Some((hl, [wing(1.0), wing(-1.0)]))
}

fn draw_object(pm: &mut Pixmap, xf: &Xf, kind: &ObjectKind, step_no: u32, snap: &[u8]) {
    match kind {
        ObjectKind::Pen { pts, stroke } | ObjectKind::Highlighter { pts, stroke } => {
            match pts.as_slice() {
                [] => {}
                [p] => {
                    if let Some(path) = PathBuilder::from_circle(p.x, p.y, stroke.width / 2.0) {
                        pm.fill_path(
                            &path,
                            &paint(stroke.rgb, stroke.alpha),
                            FillRule::Winding,
                            xf.ts(),
                            None,
                        );
                    }
                }
                _ => {
                    if let Some(path) = poly_path(pts, false) {
                        stroke_path(pm, xf, &path, stroke, LineJoin::Round);
                    }
                }
            }
        }
        ObjectKind::Line { a, b, stroke } => {
            if let Some(path) = poly_path(&[*a, *b], false) {
                stroke_path(pm, xf, &path, stroke, LineJoin::Round);
            }
        }
        ObjectKind::Arrow { a, b, stroke, head } => {
            let Some((hl, wings)) = arrow_geometry(*a, *b, stroke.width) else {
                return;
            };
            match head {
                ArrowHead::Open => {
                    let mut pb = PathBuilder::new();
                    pb.move_to(a.x, a.y);
                    pb.line_to(b.x, b.y);
                    for w in wings {
                        pb.move_to(b.x, b.y);
                        pb.line_to(w.x, w.y);
                    }
                    if let Some(path) = pb.finish() {
                        stroke_path(pm, xf, &path, stroke, LineJoin::Round);
                    }
                }
                ArrowHead::Filled => {
                    let mid = PointF::new((wings[0].x + wings[1].x) / 2.0, (wings[0].y + wings[1].y) / 2.0);
                    let _ = hl;
                    if let Some(path) = poly_path(&[*a, mid], false) {
                        stroke_path(pm, xf, &path, stroke, LineJoin::Round);
                    }
                    if let Some(tri) = poly_path(&[*b, wings[0], wings[1]], true) {
                        pm.fill_path(
                            &tri,
                            &paint(stroke.rgb, stroke.alpha),
                            FillRule::Winding,
                            xf.ts(),
                            None,
                        );
                        stroke_path(pm, xf, &tri, stroke, LineJoin::Round);
                    }
                }
            }
        }
        ObjectKind::Rect { r, stroke, fill, radius } => {
            if let Some(path) = poly_path(&rrect_poly(*r, *radius), true) {
                fill_path(pm, xf, &path, fill);
                stroke_path(pm, xf, &path, stroke, LineJoin::Round);
            }
        }
        ObjectKind::Ellipse { r, stroke, fill } => {
            if let Some(path) = poly_path(&ellipse_poly(*r), true) {
                fill_path(pm, xf, &path, fill);
                stroke_path(pm, xf, &path, stroke, LineJoin::Round);
            }
        }
        ObjectKind::Text { pos, text, style, wrap_w, fill } => {
            let lay = text::layout(text, style, *wrap_w);
            if !fill.is_none() {
                let bg = RectF::new(pos.x, pos.y, lay.width, lay.height).inflate(3.0);
                if let Some(path) = poly_path(&rrect_poly(bg, 2.0), true) {
                    fill_path(pm, xf, &path, fill);
                }
            }
            draw_text(pm, xf, *pos, text, style, &lay);
        }
        ObjectKind::Callout { shape, body, tail, stroke, fill, text, style } => {
            if let Some(path) = poly_path(&callout_poly(*shape, *body, *tail), true) {
                fill_path(pm, xf, &path, fill);
                stroke_path(pm, xf, &path, stroke, LineJoin::Miter);
            }
            let (pos, lay) = callout_layout(*body, text, style);
            draw_text(pm, xf, pos, text, style, &lay);
        }
        ObjectKind::Step { center, radius, rgb, text_rgb } => {
            if let Some(path) = PathBuilder::from_circle(center.x, center.y, *radius) {
                pm.fill_path(&path, &paint(*rgb, 1.0), FillRule::Winding, xf.ts(), None);
            }
            let st = TextStyle {
                px: radius * 1.15,
                bold: true,
                rgb: *text_rgb,
                ..Default::default()
            };
            let label = step_no.to_string();
            let lay = text::layout(&label, &st, None);
            // Digits have no descenders; adjust optical baseline vertically
            let optical_y_offset = (lay.height * 0.08).round().max(1.0);
            let pos = PointF::new(
                center.x - lay.width / 2.0,
                center.y - lay.height / 2.0 - optical_y_offset,
            );
            draw_text(pm, xf, pos, &label, &st, &lay);
        }
        ObjectKind::Magnify { rect, zoom, shape, stroke } => {
            let r = rect.normalized();
            let (x0, y0, x1, y1) = (
                xf.px(r.x).round() as i32,
                xf.py(r.y).round() as i32,
                xf.px(r.right()).round() as i32,
                xf.py(r.bottom()).round() as i32,
            );
            if x1 <= x0 || y1 <= y0 { return; }
            // Sample a frozen snapshot of the current composite: the lens itself
            // cannot feed its own pixels back into the source.
            let source = pm.data().to_vec();
            let (w, h) = (pm.width() as i32, pm.height() as i32);
            let (cx, cy) = ((x0 + x1) as f32 / 2.0, (y0 + y1) as f32 / 2.0);
            let (rx, ry) = ((x1 - x0) as f32 / 2.0, (y1 - y0) as f32 / 2.0);
            for y in y0.max(0)..y1.min(h) {
                for x in x0.max(0)..x1.min(w) {
                    if *shape == qs_doc::MagnifyShape::Ellipse {
                        let nx = (x as f32 + 0.5 - cx) / rx;
                        let ny = (y as f32 + 0.5 - cy) / ry;
                        if nx * nx + ny * ny > 1.0 { continue; }
                    }
                    let sx = (cx + (x as f32 + 0.5 - cx) / zoom).floor() as i32;
                    let sy = (cy + (y as f32 + 0.5 - cy) / zoom).floor() as i32;
                    if sx < 0 || sy < 0 || sx >= w || sy >= h { continue; }
                    let si = ((sy * w + sx) * 4) as usize;
                    let di = ((y * w + x) * 4) as usize;
                    pm.data_mut()[di..di + 4].copy_from_slice(&source[si..si + 4]);
                }
            }
            let path = match shape {
                qs_doc::MagnifyShape::Rect => poly_path(&rrect_poly(*rect, 3.0), true),
                qs_doc::MagnifyShape::Ellipse => poly_path(&ellipse_poly(*rect), true),
            };
            if let Some(path) = path { stroke_path(pm, xf, &path, stroke, LineJoin::Round); }
        }
        ObjectKind::Stamp { rect, kind, rgb } => draw_stamp(pm, xf, rect, *kind, *rgb),
        ObjectKind::Eraser { pts, width } => erase_stroke(pm, xf, pts, *width, snap),
        ObjectKind::Spotlight { rect, dim_opacity, blur } => {
            // Black overlay over the whole canvas with `rect` punched out;
            // `blur` feathers the hole outward. Premultiplied: c *= 1 - a.
            let r = rect.normalized();
            let (x0, y0) = (xf.px(r.x), xf.py(r.y));
            let (x1, y1) = (xf.px(r.right()), xf.py(r.bottom()));
            let feather = (blur * xf.s).max(0.0);
            let dim = dim_opacity.clamp(0.0, 1.0);
            let w = pm.width() as usize;
            for (i, px) in pm.data_mut().chunks_exact_mut(4).enumerate() {
                let (x, y) = ((i % w) as f32 + 0.5, (i / w) as f32 + 0.5);
                let dx = (x0 - x).max(x - x1).max(0.0);
                let dy = (y0 - y).max(y - y1).max(0.0);
                let d = dx.hypot(dy);
                let hole = if d <= 0.0 { 1.0 } else if feather > 0.0 { (1.0 - d / feather).max(0.0) } else { 0.0 };
                let a = dim * (1.0 - hole);
                if a <= 0.0 { continue; }
                let keep = 1.0 - a;
                for c in &mut px[..3] { *c = (*c as f32 * keep).round() as u8; }
                px[3] = (255.0 * a + px[3] as f32 * keep).round().min(255.0) as u8;
            }
        }
        ObjectKind::Blur { r, mode, strength } => {
            let (x0, y0) = (xf.px(r.x).round() as i32, xf.py(r.y).round() as i32);
            let (x1, y1) = (xf.px(r.right()).round() as i32, xf.py(r.bottom()).round() as i32);
            let amount = (strength * xf.s).round().max(2.0) as usize;
            match mode {
                BlurMode::Pixelate => pixelate(pm, x0, y0, x1, y1, amount),
                BlurMode::Gaussian => gaussian(pm, x0, y0, x1, y1, amount),
            }
        }
        ObjectKind::Fill { seed, rgb, tolerance } => {
            let (sx, sy) = (xf.px(seed.x).round() as i32, xf.py(seed.y).round() as i32);
            flood_fill(pm, sx, sy, *rgb, *tolerance);
        }
        ObjectKind::SmartMove { src, dst } => {
            let (x0, y0) = (xf.px(src.x).round() as i32, xf.py(src.y).round() as i32);
            let (x1, y1) = (xf.px(src.right()).round() as i32, xf.py(src.bottom()).round() as i32);
            let (dx, dy) = (xf.px(dst.x).round() as i32, xf.py(dst.y).round() as i32);
            smart_move(pm, x0, y0, x1, y1, dx, dy);
        }
    }
}

/// Paint-bucket fill of the region reachable from `(sx, sy)`.
///
/// Tolerance is the per-channel distance that still counts as "the same
/// colour". A visited mask is kept instead of relying on the fill colour to
/// stop the walk, so a fill colour close to the seed colour cannot make the
/// scan revisit pixels forever.
fn flood_fill(pm: &mut Pixmap, sx: i32, sy: i32, rgb: u32, tolerance: u8) {
    let w = pm.width() as i32;
    let h = pm.height() as i32;
    if sx < 0 || sy < 0 || sx >= w || sy >= h {
        return;
    }
    let fill = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255u8];
    let tol = tolerance as i32;
    let stride = w as usize;
    let idx = |x: i32, y: i32| (y as usize * stride + x as usize) * 4;
    let data = pm.data_mut();

    let seed = {
        let i = idx(sx, sy);
        [data[i], data[i + 1], data[i + 2], data[i + 3]]
    };
    if seed == fill {
        return;
    }
    let matches = |data: &[u8], i: usize| {
        (data[i] as i32 - seed[0] as i32).abs() <= tol
            && (data[i + 1] as i32 - seed[1] as i32).abs() <= tol
            && (data[i + 2] as i32 - seed[2] as i32).abs() <= tol
    };

    let mut seen = vec![false; (w as usize) * (h as usize)];
    let mut stack: Vec<(i32, i32)> = vec![(sx, sy)];
    while let Some((x, y)) = stack.pop() {
        let mut left = x;
        while left > 0 && !seen[y as usize * stride + left as usize - 1] && matches(data, idx(left - 1, y)) {
            left -= 1;
        }
        let mut right = x;
        while right + 1 < w
            && !seen[y as usize * stride + right as usize + 1]
            && matches(data, idx(right + 1, y))
        {
            right += 1;
        }
        for xx in left..=right {
            let i = idx(xx, y);
            if seen[y as usize * stride + xx as usize] {
                continue;
            }
            seen[y as usize * stride + xx as usize] = true;
            data[i..i + 4].copy_from_slice(&fill);
        }
        for ny in [y - 1, y + 1] {
            if ny < 0 || ny >= h {
                continue;
            }
            let mut xx = left;
            while xx <= right {
                if !seen[ny as usize * stride + xx as usize] && matches(data, idx(xx, ny)) {
                    stack.push((xx, ny));
                }
                xx += 1;
            }
        }
    }
}

/// Move the pixels under the source rect to `(dx, dy)`, healing the hole with
/// the dominant colour of the ring just outside it.
///
/// The patch is captured before the hole is covered, so a destination that
/// overlaps the source still receives the original content.
fn smart_move(pm: &mut Pixmap, x0: i32, y0: i32, x1: i32, y1: i32, dx: i32, dy: i32) {
    let Some((x0, y0, x1, y1)) = clip(pm, x0, y0, x1, y1) else {
        return;
    };
    let stride = pm.width() as usize;
    let w = x1 - x0;
    let h = y1 - y0;

    // 1. Capture what is being moved.
    let mut patch = Vec::with_capacity(w * h * 4);
    {
        let data = pm.data();
        for y in y0..y1 {
            let start = (y * stride + x0) * 4;
            patch.extend_from_slice(&data[start..start + w * 4]);
        }
    }

    // 2. Heal the hole with the colour that surrounds it.
    let heal = ring_colour(pm, x0, y0, x1, y1);
    {
        let data = pm.data_mut();
        for y in y0..y1 {
            let start = (y * stride + x0) * 4;
            for x in 0..w {
                let i = start + x * 4;
                data[i..i + 4].copy_from_slice(&heal);
            }
        }
    }

    // 3. Paste at the destination, clipped to the canvas.
    let Some((px0, py0, px1, py1)) = clip(pm, dx, dy, dx + w as i32, dy + h as i32) else {
        return;
    };
    let data = pm.data_mut();
    for y in py0..py1 {
        for x in px0..px1 {
            let sx = x as i32 - dx;
            let sy = y as i32 - dy;
            let src = (sy as usize * w + sx as usize) * 4;
            let dst = (y * stride + x) * 4;
            data[dst..dst + 4].copy_from_slice(&patch[src..src + 4]);
        }
    }
}

/// The most common colour in the one-pixel ring around a rect. Falls back to
/// the rect's own border average when the ring is entirely off-canvas, so the
/// heal colour always comes from the image rather than an invented one.
fn ring_colour(pm: &Pixmap, x0: usize, y0: usize, x1: usize, y1: usize) -> [u8; 4] {
    use std::collections::HashMap;
    let stride = pm.width() as usize;
    let data = pm.data();
    let mut counts: HashMap<[u8; 4], usize> = HashMap::new();
    let mut push = |x: usize, y: usize| {
        let i = (y * stride + x) * 4;
        *counts.entry([data[i], data[i + 1], data[i + 2], data[i + 3]]).or_insert(0) += 1;
    };
    let (rx0, ry0) = (x0.saturating_sub(1), y0.saturating_sub(1));
    let rx1 = (x1 + 1).min(stride);
    let ry1 = (y1 + 1).min(pm.height() as usize);
    for x in rx0..rx1 {
        if ry0 < y0 {
            push(x, ry0);
        }
        if ry1 > y1 {
            push(x, ry1 - 1);
        }
    }
    for y in ry0..ry1 {
        if rx0 < x0 {
            push(rx0, y);
        }
        if rx1 > x1 {
            push(rx1 - 1, y);
        }
    }
    if let Some((best, _)) = counts.iter().max_by_key(|(_, n)| **n) {
        return *best;
    }
    // No ring at all (the source covers the canvas): average its own border.
    let mut sum = [0u32; 4];
    let mut n = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            if x != x0 && x + 1 != x1 && y != y0 && y + 1 != y1 {
                continue;
            }
            let i = (y * stride + x) * 4;
            for c in 0..4 {
                sum[c] += data[i + c] as u32;
            }
            n += 1;
        }
    }
    if n == 0 {
        return [0, 0, 0, 0];
    }
    [
        (sum[0] / n) as u8,
        (sum[1] / n) as u8,
        (sum[2] / n) as u8,
        (sum[3] / n) as u8,
    ]
}

fn clip(pm: &Pixmap, x0: i32, y0: i32, x1: i32, y1: i32) -> Option<(usize, usize, usize, usize)> {
    let (x0, y0) = (x0.max(0), y0.max(0));
    let (x1, y1) = (x1.min(pm.width() as i32), y1.min(pm.height() as i32));
    (x1 > x0 && y1 > y0).then_some((x0 as usize, y0 as usize, x1 as usize, y1 as usize))
}

/// Block-average mosaic (same maths as the overlay's `pixelate_rgba`).
pub fn pixelate(pm: &mut Pixmap, x0: i32, y0: i32, x1: i32, y1: i32, block: usize) {
    let Some((x0, y0, x1, y1)) = clip(pm, x0, y0, x1, y1) else {
        return;
    };
    let stride = pm.width() as usize * 4;
    let raw = pm.data_mut();
    for by in (y0..y1).step_by(block) {
        for bx in (x0..x1).step_by(block) {
            let (bw, bh) = (block.min(x1 - bx), block.min(y1 - by));
            let mut s = [0u64; 4];
            for y in by..by + bh {
                for x in bx..bx + bw {
                    let p = &raw[y * stride + x * 4..][..4];
                    for k in 0..4 {
                        s[k] += p[k] as u64;
                    }
                }
            }
            let n = (bw * bh) as u64;
            let avg = [s[0] / n, s[1] / n, s[2] / n, s[3] / n].map(|v| v as u8);
            for y in by..by + bh {
                for x in bx..bx + bw {
                    raw[y * stride + x * 4..][..4].copy_from_slice(&avg);
                }
            }
        }
    }
}

/// 3-pass box blur (≈ gaussian) of the region, radius `r` px.
pub fn gaussian(pm: &mut Pixmap, x0: i32, y0: i32, x1: i32, y1: i32, r: usize) {
    let Some((x0, y0, x1, y1)) = clip(pm, x0, y0, x1, y1) else {
        return;
    };
    let (w, h) = (x1 - x0, y1 - y0);
    let stride = pm.width() as usize * 4;
    let raw = pm.data_mut();
    let mut buf = vec![0u8; w * h * 4];
    for y in 0..h {
        buf[y * w * 4..(y + 1) * w * 4]
            .copy_from_slice(&raw[(y0 + y) * stride + x0 * 4..][..w * 4]);
    }
    for _ in 0..3 {
        box_pass(&mut buf, w, h, r, true);
        box_pass(&mut buf, w, h, r, false);
    }
    for y in 0..h {
        raw[(y0 + y) * stride + x0 * 4..][..w * 4].copy_from_slice(&buf[y * w * 4..(y + 1) * w * 4]);
    }
}

fn box_pass(buf: &mut [u8], w: usize, h: usize, r: usize, horizontal: bool) {
    let (lines, n) = if horizontal { (h, w) } else { (w, h) };
    let idx = |line: usize, i: usize| -> usize {
        if horizontal {
            (line * w + i) * 4
        } else {
            (i * w + line) * 4
        }
    };
    let mut pre = vec![[0u32; 4]; n + 1];
    for line in 0..lines {
        for i in 0..n {
            let o = idx(line, i);
            for k in 0..4 {
                pre[i + 1][k] = pre[i][k] + buf[o + k] as u32;
            }
        }
        for i in 0..n {
            let (lo, hi) = (i.saturating_sub(r), (i + r).min(n - 1));
            let cnt = (hi - lo + 1) as u32;
            let o = idx(line, i);
            for k in 0..4 {
                buf[o + k] = ((pre[hi + 1][k] - pre[lo][k] + cnt / 2) / cnt) as u8;
            }
        }
    }
}

fn to_pixmap(img: &RgbaImage) -> Option<Pixmap> {
    let mut data = img.as_raw().clone();
    for p in data.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a != 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * a + 127) / 255) as u8;
            }
        }
    }
    Pixmap::from_vec(data, IntSize::from_wh(img.width(), img.height())?)
}

fn to_image(pm: &Pixmap) -> RgbaImage {
    let mut out = RgbaImage::new(pm.width(), pm.height());
    for (o, p) in out.pixels_mut().zip(pm.pixels()) {
        let c = p.demultiply();
        *o = image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    out
}

/// Output size in physical px for `doc`.
pub fn output_size(doc: &Document) -> (u32, u32) {
    checked_output_size(doc).unwrap_or((1,1))
}

fn checked_output_size(doc: &Document) -> Option<(u32,u32)> {
    let layout = doc.effect_layout();
    let w = (layout.size.w*doc.scale).round();
    let h = (layout.size.h*doc.scale).round();
    if doc.validate().is_err() || !w.is_finite() || !h.is_finite() || w < 1.0 || h < 1.0 || w > MAX_OUT || h > MAX_OUT || w*h > 64.0*1024.0*1024.0 { return None; }
    Some((w as u32,h as u32))
}

/// Size of the rendered `view` before orientation, physical px.
fn content_size(doc: &Document) -> (u32, u32) {
    let w = (doc.view.w * doc.scale).round().clamp(1.0, MAX_OUT);
    let h = (doc.view.h * doc.scale).round().clamp(1.0, MAX_OUT);
    (w as u32, h as u32)
}

/// Exact pixel remap of `img` by one of the eight rectangle symmetries;
/// pixel centres follow [`Orientation::map`].
pub fn orient(img: &RgbaImage, o: Orientation) -> RgbaImage {
    if o == Orientation::Normal {
        return img.clone();
    }
    let (w, h) = img.dimensions();
    let s = SizeF::new(w as f32, h as f32);
    let out_s = o.output_size(s);
    let mut out = RgbaImage::new(out_s.w as u32, out_s.h as u32);
    for (x, y, p) in img.enumerate_pixels() {
        let q = o.map(PointF::new(x as f32 + 0.5, y as f32 + 0.5), s);
        out.put_pixel(q.x as u32, q.y as u32, *p);
    }
    out
}

/// Flatten `doc` over `base` into a bitmap.
pub fn render(doc: &Document, base: &RgbaImage) -> RgbaImage {
    if checked_output_size(doc).is_none() { return RgbaImage::new(1,1); }
    crate::effects::apply(doc, orient(&render_content(doc, base), doc.orientation))
}

/// Render only `region` (document coordinates, clipped to `view`) with the
/// same pipeline as [`render`]: orientation, effects and annotations are
/// recomputed for the crop, so the exported pixels match the full render.
/// `None` when the region does not overlap the view.
pub fn render_region(doc: &Document, base: &RgbaImage, region: RectF) -> Option<RgbaImage> {
    let r = region.normalized();
    let v = doc.view.normalized();
    let x = r.x.max(v.x);
    let y = r.y.max(v.y);
    let right = r.right().min(v.right());
    let bottom = r.bottom().min(v.bottom());
    if right - x < 0.5 || bottom - y < 0.5 {
        return None;
    }
    let mut cropped = doc.clone();
    cropped.view = RectF::new(x, y, right - x, bottom - y);
    cropped.region = None;
    Some(render(&cropped, base))
}

/// `view` with every annotation, before orientation and effects.
fn render_content(doc: &Document, base: &RgbaImage) -> RgbaImage {
    let (w, h) = content_size(doc);
    let Some(mut pm) = Pixmap::new(w, h) else {
        return RgbaImage::new(1, 1);
    };
    let bg = doc.background;
    pm.fill(Color::from_rgba8((bg >> 16) as u8, (bg >> 8) as u8, bg as u8, (bg >> 24) as u8));
    let xf = Xf {
        s: doc.scale,
        ox: (-doc.view.x * doc.scale).round(),
        oy: (-doc.view.y * doc.scale).round(),
    };
    if let Some(bp) = to_pixmap(base) {
        pm.draw_pixmap(xf.ox as i32, xf.oy as i32, bp.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    }
    // Snapshot the composited canvas (background + base image) so the
    // eraser reveals exactly what was there before annotations.
    let bg_snap: Vec<u8> = pm.data().to_vec();
    let mut step_no = doc.step_start;
    for o in &doc.objects {
        let is_step = matches!(o.kind, ObjectKind::Step { .. });
        if o.visible {
            draw_with_opacity(&mut pm, &xf, &o.kind, o.opacity, step_no, &bg_snap);
        }
        if is_step {
            step_no += 1;
        }
    }
    to_image(&pm)
}

/// Kinds that only add paint. Everything else (blur, magnify, eraser) reads
/// the canvas underneath and must draw directly, so opacity is ignored for
/// them.
fn is_paint_only(kind: &ObjectKind) -> bool {
    !matches!(
        kind,
        ObjectKind::Blur { .. }
            | ObjectKind::Magnify { .. }
            | ObjectKind::Eraser { .. }
            | ObjectKind::Fill { .. }
            | ObjectKind::SmartMove { .. }
    )
}

/// `draw_object`, optionally through a transparent layer composited with
/// `opacity` so strokes and fills fade together instead of overlapping.
fn draw_with_opacity(
    pm: &mut Pixmap,
    xf: &Xf,
    kind: &ObjectKind,
    opacity: f32,
    step_no: u32,
    snap: &[u8],
) {
    if opacity >= 1.0 || !is_paint_only(kind) {
        draw_object(pm, xf, kind, step_no, snap);
        return;
    }
    if opacity <= 0.0 {
        return;
    }
    let Some(mut layer) = Pixmap::new(pm.width(), pm.height()) else {
        return;
    };
    draw_object(&mut layer, xf, kind, step_no, snap);
    pm.draw_pixmap(
        0,
        0,
        layer.as_ref(),
        &PixmapPaint {
            opacity,
            ..PixmapPaint::default()
        },
        Transform::identity(),
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use qs_doc::{CalloutShape, PALETTE};

    fn gray(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, image::Rgba([200, 200, 200, 255]))
    }
    fn red() -> Stroke {
        Stroke::new(0xFF0000, 4.0)
    }
    fn px(img: &RgbaImage, x: u32, y: u32) -> [u8; 3] {
        let p = img.get_pixel(x, y).0;
        [p[0], p[1], p[2]]
    }

    #[test]
    fn empty_doc_is_the_base() {
        let base = RgbaImage::from_fn(30, 20, |x, y| image::Rgba([x as u8 * 7, y as u8 * 9, 5, 255]));
        let d = Document::new(30, 20, 1.0);
        assert_eq!(render(&d, &base).as_raw(), base.as_raw());
    }

    #[test]
    fn render_region_crops_to_the_selection_and_clips_to_the_view() {
        let base = RgbaImage::from_fn(100, 80, |x, _| image::Rgba([x as u8, 0, 0, 255]));
        let mut d = Document::new(100, 80, 1.0);
        d.push(ObjectKind::Rect {
            r: RectF::new(20.0, 20.0, 20.0, 20.0),
            stroke: Stroke::new(0x00FF00, 4.0),
            fill: Fill::Solid { rgb: 0x0000FF, alpha: 1.0 },
            radius: 0.0,
        });
        let out = render_region(&d, &base, RectF::new(10.0, 10.0, 40.0, 40.0)).unwrap();
        assert_eq!((out.width(), out.height()), (40, 40));
        assert_eq!(px(&out, 20, 20), [0, 0, 255], "object moved with the crop");
        assert_eq!(px(&out, 5, 5), [15, 0, 0], "base pixel at x=15 of the doc");
        assert_eq!(px(&out, 39, 39), [49, 0, 0], "bottom-right maps to x=49");

        // clipped to the view: half of the rect request overlaps
        let clipped = render_region(&d, &base, RectF::new(90.0, 70.0, 40.0, 40.0)).unwrap();
        assert_eq!((clipped.width(), clipped.height()), (10, 10));

        assert!(
            render_region(&d, &base, RectF::new(400.0, 400.0, 10.0, 10.0)).is_none(),
            "a region outside the view copies nothing"
        );
        assert!(
            render_region(&d, &base, RectF::new(10.0, 10.0, 0.2, 0.2)).is_none(),
            "degenerate regions are rejected"
        );
    }

    #[test]
    fn object_opacity_fades_fill_and_stroke_together() {
        let base = gray(100, 80);
        let mut d = Document::new(100, 80, 1.0);
        let r = RectF::new(20.0, 20.0, 40.0, 30.0);
        let id = d.push(ObjectKind::Rect {
            r,
            stroke: red(),
            fill: Fill::Solid { rgb: 0x0000FF, alpha: 1.0 },
            radius: 1.5,
        });
        d.get_mut(id).unwrap().opacity = 0.5;
        let o = render(&d, &base);
        let mid = px(&o, 40, 35);
        // 50% blue over grey 200
        for (got, want) in mid.iter().zip([100u16, 100, 227]) {
            assert!(
                (*got as i16 - want as i16).abs() <= 2,
                "fill blended at 50%: {mid:?}"
            );
        }
        let edge = px(&o, 20, 35);
        for (got, want) in edge.iter().zip([227u16, 100, 100]) {
            assert!(
                (*got as i16 - want as i16).abs() <= 2,
                "stroke blended at 50%: {edge:?}"
            );
        }

        d.get_mut(id).unwrap().opacity = 0.0;
        assert_eq!(px(&render(&d, &base), 40, 35), [200, 200, 200], "0 = hidden");

        d.get_mut(id).unwrap().opacity = 1.0;
        assert_eq!(px(&render(&d, &base), 40, 35), [0, 0, 255]);
    }

    #[test]
    fn opacity_is_ignored_by_canvas_reading_tools() {
        let base = gray(60, 60);
        let mut d = Document::new(60, 60, 1.0);
        d.push(ObjectKind::Rect {
            r: RectF::new(10.0, 20.0, 40.0, 20.0),
            stroke: red(),
            fill: Fill::Solid { rgb: 0x0000FF, alpha: 1.0 },
            radius: 0.0,
        });
        let id = d.push(ObjectKind::Eraser {
            pts: vec![PointF::new(10.0, 30.0), PointF::new(50.0, 30.0)],
            width: 12.0,
        });
        let painted = render(&d, &base);
        assert_eq!(px(&painted, 30, 30), [200, 200, 200], "eraser on top");
        d.get_mut(id).unwrap().opacity = 0.2;
        let erased = render(&d, &base);
        assert_eq!(
            px(&erased, 30, 30),
            [200, 200, 200],
            "eraser always reveals what is below it, whatever its opacity"
        );
        assert_eq!(erased.as_raw(), painted.as_raw(), "pixels are untouched");
    }

    #[test]
    fn rect_stroke_and_fill_are_independent() {
        let base = gray(100, 80);
        let mut d = Document::new(100, 80, 1.0);
        let r = RectF::new(20.0, 20.0, 40.0, 30.0);
        let id = d.push(ObjectKind::Rect { r, stroke: red(), fill: Fill::None, radius: 1.5 });
        let o = render(&d, &base);
        assert_eq!(px(&o, 20, 35), [255, 0, 0], "stroke centred on the edge");
        assert_eq!(px(&o, 40, 35), [200, 200, 200], "no fill -> interior untouched");
        assert_eq!(px(&o, 5, 5), [200, 200, 200]);

        if let Some(ObjectKind::Rect { fill, .. }) = d.get_mut(id).map(|o| &mut o.kind) {
            *fill = Fill::Solid { rgb: 0x0000FF, alpha: 1.0 };
        }
        let o = render(&d, &base);
        assert_eq!(px(&o, 40, 35), [0, 0, 255]);
        assert_eq!(px(&o, 20, 35), [255, 0, 0], "stroke still wins on the edge");
    }

    #[test]
    fn scale_two_doubles_output_and_keeps_geometry() {
        let base = gray(200, 160);
        let mut d = Document::new(200, 160, 2.0);
        d.push(ObjectKind::Line { a: PointF::new(10.0, 40.0), b: PointF::new(60.0, 40.0), stroke: red() });
        let o = render(&d, &base);
        assert_eq!(o.dimensions(), (200, 160));
        assert_eq!(px(&o, 60, 80), [255, 0, 0]); // (30,40) logical -> (60,80) px
        assert_eq!(px(&o, 60, 40), [200, 200, 200]);
    }

    #[test]
    fn highlighter_is_translucent_and_single_pass() {
        let base = gray(60, 40);
        let mut d = Document::new(60, 40, 1.0);
        let st = Stroke { rgb: 0xFFFF00, alpha: 0.5, width: 10.0 };
        // self-crossing stroke must not double up where it overlaps itself
        d.push(ObjectKind::Highlighter {
            pts: vec![PointF::new(10.0, 20.0), PointF::new(50.0, 20.0), PointF::new(30.0, 20.0)],
            stroke: st,
        });
        let o = render(&d, &base);
        let p = px(&o, 30, 20);
        assert_eq!(p, [228, 228, 100]); // 0.5*(255,255,0)+0.5*200
    }

    #[test]
    fn pixelate_makes_uniform_blocks() {
        let base = RgbaImage::from_fn(64, 64, |x, y| image::Rgba([(x * 4) as u8, (y * 4) as u8, 0, 255]));
        let mut d = Document::new(64, 64, 1.0);
        d.push(ObjectKind::Blur { r: RectF::new(8.0, 8.0, 32.0, 32.0), mode: BlurMode::Pixelate, strength: 8.0 });
        let o = render(&d, &base);
        for (x, y) in [(8, 8), (15, 15), (8, 15), (15, 8)] {
            assert_eq!(px(&o, x, y), px(&o, 8, 8));
        }
        assert_ne!(px(&o, 8, 8), px(&o, 16, 8));
        assert_eq!(px(&o, 41, 41), px(&base, 41, 41), "outside the rect untouched");
    }

    #[test]
    fn gaussian_smooths_an_edge() {
        let base = RgbaImage::from_fn(60, 20, |x, _| {
            if x < 30 { image::Rgba([0, 0, 0, 255]) } else { image::Rgba([255, 255, 255, 255]) }
        });
        let mut d = Document::new(60, 20, 1.0);
        d.push(ObjectKind::Blur { r: RectF::new(0.0, 0.0, 60.0, 20.0), mode: BlurMode::Gaussian, strength: 5.0 });
        let o = render(&d, &base);
        let mid = o.get_pixel(30, 10).0[0];
        assert!(mid > 10 && mid < 245, "edge softened, got {mid}");
        assert!(o.get_pixel(0, 10).0[0] < 30 && o.get_pixel(59, 10).0[0] > 225);
    }

    #[test]
    fn hidden_object_is_skipped() {
        let base = gray(40, 40);
        let mut d = Document::new(40, 40, 1.0);
        let id = d.push(ObjectKind::Line { a: PointF::new(0.0, 20.0), b: PointF::new(40.0, 20.0), stroke: red() });
        d.get_mut(id).unwrap().visible = false;
        assert_eq!(render(&d, &base).as_raw(), base.as_raw());
    }

    #[test]
    fn crop_and_extend_canvas() {
        let base = RgbaImage::from_fn(40, 40, |x, y| image::Rgba([x as u8 * 6, y as u8 * 6, 0, 255]));
        let mut d = Document::new(40, 40, 1.0);
        d.view = RectF::new(10.0, 5.0, 20.0, 10.0);
        let o = render(&d, &base);
        assert_eq!(o.dimensions(), (20, 10));
        assert_eq!(px(&o, 0, 0), px(&base, 10, 5));

        d.view = RectF::new(-10.0, -10.0, 60.0, 60.0);
        d.background = 0xFF00FF00;
        let o = render(&d, &base);
        assert_eq!(o.dimensions(), (60, 60));
        assert_eq!(px(&o, 2, 2), [0, 255, 0]);
        assert_eq!(px(&o, 10, 10), px(&base, 0, 0));
    }

    fn ink(img: &RgbaImage, r: RectF, bg: [u8; 3]) -> usize {
        let mut n = 0;
        for y in r.y.max(0.0) as u32..(r.bottom() as u32).min(img.height()) {
            for x in r.x.max(0.0) as u32..(r.right() as u32).min(img.width()) {
                if px(img, x, y) != bg {
                    n += 1;
                }
            }
        }
        n
    }

    /// Mean x of non-background pixels inside `r` (0 when empty).
    fn centroid_x(img: &RgbaImage, r: RectF, bg: [u8; 3]) -> f32 {
        let (mut sum, mut n) = (0.0f32, 0usize);
        for y in r.y.max(0.0) as u32..(r.bottom() as u32).min(img.height()) {
            for x in r.x.max(0.0) as u32..(r.right() as u32).min(img.width()) {
                if px(img, x, y) != bg {
                    sum += x as f32;
                    n += 1;
                }
            }
        }
        if n == 0 {
            0.0
        } else {
            sum / n as f32
        }
    }

    #[test]
    fn underline_and_strike_add_a_rule_under_and_across_the_text() {
        let base = gray(240, 120);
        let text = |style: TextStyle| {
            let mut d = Document::new(240, 120, 1.0);
            d.push(ObjectKind::Text {
                pos: PointF::new(20.0, 20.0),
                text: "Hamburg\nHamburg".into(),
                style,
                wrap_w: None,
                fill: Fill::None,
            });
            render(&d, &base)
        };
        let plain_style = TextStyle { px: 24.0, rgb: 0xFF0000, ..Default::default() };
        let plain = text(plain_style);
        let lay = text::layout("Hamburg\nHamburg", &plain_style, None);
        // Below the first line's glyphs but above the second line.
        let band = RectF::new(
            20.0,
            20.0 + lay.ascent + 2.0,
            lay.width,
            (lay.line_h - lay.ascent - 4.0).max(2.0),
        );
        let plain_band = ink(&plain, band, [200; 3]);

        let underlined = text(TextStyle { underline: true, ..plain_style });
        let lined = ink(&underlined, band, [200; 3]);
        assert!(
            lined > plain_band + 40,
            "underline adds a rule: {plain_band} -> {lined}"
        );

        let struck = text(TextStyle { strike: true, ..plain_style });
        let mid = RectF::new(20.0, 20.0 + lay.ascent * 0.4, lay.width, lay.ascent * 0.35);
        assert!(ink(&struck, mid, [200; 3]) > ink(&plain, mid, [200; 3]) + 20);
    }

    #[test]
    fn outline_rings_the_glyphs_and_italic_shears_them() {
        let base = gray(240, 120);
        let draw = |style: TextStyle| {
            let mut d = Document::new(240, 120, 1.0);
            d.push(ObjectKind::Text {
                pos: PointF::new(20.0, 20.0),
                text: "IIIII".into(),
                style,
                wrap_w: None,
                fill: Fill::None,
            });
            render(&d, &base)
        };
        let plain_style = TextStyle { px: 40.0, rgb: 0xFFFFFF, ..Default::default() };
        let plain = draw(plain_style);
        let laid = text::layout("IIIII", &plain_style, None);
        let block = RectF::new(18.0, 18.0, laid.width + 8.0, laid.height + 6.0);
        let plain_ink = ink(&plain, block, [200; 3]);

        let outlined = draw(TextStyle {
            outline: Some(qs_doc::TextOutline { width: 3.0, rgb: 0x000000 }),
            ..plain_style
        });
        let outline_ink = ink(&outlined, block, [200; 3]);
        assert!(
            outline_ink > plain_ink + 20,
            "outline adds pixels around the glyphs: {plain_ink} -> {outline_ink}"
        );

        // Italic: the top of the stroke leans right of its baseline end.
        let slanted = draw(TextStyle { italic: true, ..plain_style });
        let top = centroid_x(&slanted, RectF::new(0.0, 18.0, 240.0, laid.ascent * 0.4), [200; 3]);
        let bot = centroid_x(
            &slanted,
            RectF::new(0.0, 18.0 + laid.ascent * 0.6, 240.0, laid.ascent * 0.4),
            [200; 3],
        );
        assert!(top > bot + 2.0, "italic leans right: {top} vs {bot}");
    }

    #[test]
    fn text_paints_only_inside_its_block() {
        let base = gray(200, 120);
        let mut d = Document::new(200, 120, 1.0);
        let st = TextStyle { px: 20.0, rgb: 0x000000, ..Default::default() };
        d.push(ObjectKind::Text {
            pos: PointF::new(20.0, 30.0),
            text: "Hello\nTiếng Việt".into(),
            style: st,
            wrap_w: None,
            fill: Fill::None,
        });
        let o = render(&d, &base);
        let lay = text::layout("Hello\nTiếng Việt", &st, None);
        let blk = RectF::new(20.0, 30.0, lay.width, lay.height).inflate(3.0);
        let inside = ink(&o, blk, [200; 3]);
        assert!(inside > 150, "ink {inside}");
        assert_eq!(ink(&o, RectF::new(0.0, 0.0, 200.0, 25.0), [200; 3]), 0);
        assert_eq!(ink(&o, RectF::new(0.0, 100.0, 200.0, 20.0), [200; 3]), 0);
    }

    #[test]
    fn callout_tail_reaches_its_tip_and_text_is_inside() {
        let base = gray(200, 160);
        let mut d = Document::new(200, 160, 1.0);
        let body = RectF::new(40.0, 20.0, 120.0, 50.0);
        d.push(ObjectKind::Callout {
            shape: CalloutShape::RoundRect,
            body,
            tail: PointF::new(100.0, 140.0),
            stroke: Stroke::new(0xFF0000, 3.0),
            fill: Fill::Solid { rgb: 0xFFFFFF, alpha: 1.0 },
            text: "Click here".into(),
            style: TextStyle { px: 16.0, rgb: 0x000000, align: qs_doc::Align::Center, ..Default::default() },
        });
        let o = render(&d, &base);
        assert_eq!(px(&o, 100, 120), [255, 255, 255], "tail interior filled");
        assert!(ink(&o, RectF::new(96.0, 134.0, 8.0, 8.0), [200; 3]) > 0, "tail tip drawn");
        assert_eq!(px(&o, 100, 150), [200, 200, 200], "nothing past the tip");
        assert!(ink(&o, body.inflate(-6.0), [255; 3]) > 40, "text drawn in the body");
    }

    #[test]
    fn step_numbers_follow_order_and_start() {
        let base = gray(120, 40);
        let mut d = Document::new(120, 40, 1.0);
        d.step_start = 5;
        for x in [20.0, 60.0] {
            d.push(ObjectKind::Step { center: PointF::new(x, 20.0), radius: 14.0, rgb: PALETTE[0], text_rgb: 0xFFFFFF });
        }
        let o = render(&d, &base);
        assert_eq!(px(&o, 8, 20), [255, 69, 58]);
        assert!(ink(&o, RectF::new(14.0, 12.0, 12.0, 16.0), [255, 69, 58]) > 8, "digit ink inside badge");
        // renumber: dropping the first shifts the second to 5 — different ink
        let before = o.clone();
        let first = d.objects[0].id;
        d.remove(first);
        let after = render(&d, &base);
        assert_ne!(before.as_raw(), after.as_raw());
    }

    #[test]
    fn arrow_heads_render() {
        let base = gray(100, 100);
        for head in [ArrowHead::Open, ArrowHead::Filled] {
            let mut d = Document::new(100, 100, 1.0);
            d.push(ObjectKind::Arrow { a: PointF::new(10.0, 50.0), b: PointF::new(90.0, 50.0), stroke: red(), head });
            let o = render(&d, &base);
            assert_eq!(px(&o, 50, 50), [255, 0, 0]);
            assert!(ink(&o, RectF::new(78.0, 40.0, 12.0, 20.0), [200; 3]) > 20, "{head:?} head");
        }
    }

    #[test]
    fn orientation_rotates_base_and_annotations_together() {
        use qs_doc::Orientation;
        let base = RgbaImage::from_fn(30, 20, |x, y| image::Rgba([x as u8 * 8, y as u8 * 12, 7, 255]));
        let mut d = Document::new(30, 20, 1.0);
        d.push(ObjectKind::Rect {
            r: RectF::new(2.0, 2.0, 4.0, 3.0),
            stroke: Stroke::new(0xFF0000, 0.0),
            fill: Fill::Solid { rgb: 0x0000FF, alpha: 1.0 },
            radius: 0.0,
        });
        let plain = render(&d, &base);
        d.orientation = Orientation::Rotate90;
        assert_eq!(output_size(&d), (20, 30));
        let o = render(&d, &base);
        assert_eq!(o.dimensions(), (20, 30));
        // physical pixel (x, y) -> (h-1-y, x) under a clockwise turn
        for (x, y) in [(0, 0), (29, 0), (3, 3), (17, 11), (29, 19)] {
            assert_eq!(o.get_pixel(19 - y, x), plain.get_pixel(x, y), "({x},{y})");
        }
        assert_eq!(px(&o, 19 - 3, 3), [0, 0, 255], "annotation follows the base");
    }

    #[test]
    fn four_quarter_turns_and_double_flips_restore_pixels() {
        use qs_doc::Orientation;
        let img = RgbaImage::from_fn(7, 5, |x, y| image::Rgba([x as u8 * 30, y as u8 * 40, (x * y) as u8, 200]));
        let mut r = img.clone();
        for _ in 0..4 {
            r = orient(&r, Orientation::Rotate90);
        }
        assert_eq!(r, img);
        for o in [Orientation::FlipHorizontal, Orientation::FlipVertical, Orientation::Transpose] {
            assert_ne!(orient(&img, o), img);
            assert_eq!(orient(&orient(&img, o), o), img, "{o:?}");
        }
        assert_eq!(orient(&img, Orientation::Transpose).get_pixel(4, 6), img.get_pixel(6, 4));
    }

    #[test]
    fn every_orientation_matches_document_point_mapping() {
        use qs_doc::{Orientation, SizeF};
        let img = RgbaImage::from_fn(6, 4, |x, y| image::Rgba([x as u8, y as u8, 0, 255]));
        let s = SizeF::new(6.0, 4.0);
        for o in [
            Orientation::Normal, Orientation::Rotate90, Orientation::Rotate180, Orientation::Rotate270,
            Orientation::FlipHorizontal, Orientation::FlipVertical, Orientation::Transpose, Orientation::Transverse,
        ] {
            let out = orient(&img, o);
            for (x, y) in [(0u32, 0u32), (5, 0), (2, 3), (5, 3)] {
                let q = o.map(PointF::new(x as f32 + 0.5, y as f32 + 0.5), s);
                assert_eq!(out.get_pixel(q.x as u32, q.y as u32).0[..2], [x as u8, y as u8], "{o:?}");
            }
        }
    }
    fn stripe_base() -> RgbaImage {
        // White with a black divider at x = 20, so a fill has a wall to stop at.
        RgbaImage::from_fn(40, 30, |x, _| {
            if x == 20 {
                image::Rgba([0, 0, 0, 255])
            } else {
                image::Rgba([255, 255, 255, 255])
            }
        })
    }

    #[test]
    fn fill_stops_at_a_colour_boundary() {
        let base = stripe_base();
        let mut d = Document::new(40, 30, 1.0);
        d.push(ObjectKind::Fill {
            seed: qs_doc::PointF::new(5.0, 5.0),
            rgb: 0xFF0000,
            tolerance: 8,
        });
        let out = render(&d, &base);
        assert_eq!(px(&out, 5, 5), [255, 0, 0], "seed area filled");
        assert_eq!(px(&out, 19, 5), [255, 0, 0], "reaches the divider");
        assert_eq!(px(&out, 20, 5), [0, 0, 0], "divider untouched");
        assert_eq!(px(&out, 25, 5), [255, 255, 255], "far side untouched");
    }

    #[test]
    fn fill_tolerance_controls_what_counts_as_the_same_colour() {
        // Left half 200, right half 205 — five apart.
        let base = RgbaImage::from_fn(40, 10, |x, _| {
            if x < 20 {
                image::Rgba([200, 200, 200, 255])
            } else {
                image::Rgba([205, 205, 205, 255])
            }
        });
        let render_with = |tolerance: u8| {
            let mut d = Document::new(40, 10, 1.0);
            d.push(ObjectKind::Fill {
                seed: qs_doc::PointF::new(2.0, 2.0),
                rgb: 0x00FF00,
                tolerance,
            });
            render(&d, &base)
        };
        assert_eq!(
            px(&render_with(0), 25, 2),
            [205, 205, 205],
            "tolerance 0 stops at the seam"
        );
        assert_eq!(
            px(&render_with(16), 25, 2),
            [0, 255, 0],
            "tolerance 16 crosses it"
        );
    }

    #[test]
    fn fill_outside_the_canvas_is_a_no_op() {
        let base = stripe_base();
        let mut d = Document::new(40, 30, 1.0);
        d.push(ObjectKind::Fill {
            seed: qs_doc::PointF::new(-50.0, -50.0),
            rgb: 0xFF0000,
            tolerance: 255,
        });
        assert_eq!(render(&d, &base).as_raw(), base.as_raw());
    }

    #[test]
    fn hidden_fill_restores_the_original_pixels() {
        let base = stripe_base();
        let mut d = Document::new(40, 30, 1.0);
        let id = d.push(ObjectKind::Fill {
            seed: qs_doc::PointF::new(5.0, 5.0),
            rgb: 0xFF0000,
            tolerance: 8,
        });
        d.get_mut(id).unwrap().visible = false;
        assert_eq!(
            render(&d, &base).as_raw(),
            base.as_raw(),
            "a fill is non-destructive"
        );
    }

    fn block_base() -> RgbaImage {
        // White 80x40 with a black 10x6 block at (5,5).
        let mut img = RgbaImage::from_pixel(80, 40, image::Rgba([255, 255, 255, 255]));
        for y in 5..11 {
            for x in 5..15 {
                img.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
            }
        }
        img
    }

    #[test]
    fn smart_move_pastes_the_block_and_heals_the_hole() {
        let base = block_base();
        let mut d = Document::new(80, 40, 1.0);
        d.push(ObjectKind::SmartMove {
            src: RectF::new(5.0, 5.0, 10.0, 6.0),
            dst: qs_doc::PointF::new(40.0, 5.0),
        });
        let out = render(&d, &base);
        assert_eq!(px(&out, 45, 8), [0, 0, 0], "block landed at the destination");
        assert_eq!(px(&out, 10, 8), [255, 255, 255], "hole healed with the background");
        assert_eq!(px(&out, 60, 8), [255, 255, 255], "nothing else disturbed");
    }

    #[test]
    fn smart_move_overlapping_the_source_keeps_the_original_content() {
        let base = block_base();
        let mut d = Document::new(80, 40, 1.0);
        d.push(ObjectKind::SmartMove {
            src: RectF::new(5.0, 5.0, 10.0, 6.0),
            dst: qs_doc::PointF::new(10.0, 5.0),
        });
        let out = render(&d, &base);
        // The patch is captured before the hole is healed, so the block's right
        // half survives in the overlap.
        assert_eq!(px(&out, 12, 8), [0, 0, 0]);
    }

    #[test]
    fn smart_move_at_the_edge_is_clipped_not_panicked() {
        let base = block_base();
        let mut d = Document::new(80, 40, 1.0);
        d.push(ObjectKind::SmartMove {
            src: RectF::new(5.0, 5.0, 10.0, 6.0),
            dst: qs_doc::PointF::new(78.0, 38.0),
        });
        let out = render(&d, &base);
        assert_eq!((out.width(), out.height()), (80, 40));
    }
}
