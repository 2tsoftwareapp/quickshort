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
    ArrowHead, BlurMode, Document, Fill, ObjectKind, Orientation, PointF, RectF, SizeF, Stroke,
    TextStyle,
};
use tiny_skia::{
    Color, FillRule, IntSize, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, PixmapPaint,
    Transform,
};

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
fn draw_text(pm: &mut Pixmap, xf: &Xf, pos: PointF, text: &str, style: &TextStyle, lay: &TextLayout) {
    if text.is_empty() {
        return;
    }
    let big = style.px * xf.s;
    text::with_font(style.px, style.bold, |f| {
        let sc = f.as_scaled(style.px);
        for (li, l) in lay.lines.iter().enumerate() {
            let base = li as f32 * lay.line_h + lay.ascent;
            let mut pen = l.x;
            let mut prev = None;
            for c in text[l.start..l.end].chars() {
                let id = sc.glyph_id(c);
                if let Some(p) = prev {
                    pen += sc.kern(p, id);
                }
                if !c.is_whitespace() {
                    let at = point(xf.px(pos.x + pen), xf.py(pos.y + base));
                    if let Some(og) = f.outline_glyph(id.with_scale_and_position(big, at)) {
                        let bb = og.px_bounds();
                        og.draw(|gx, gy, cov| {
                            blend(
                                pm,
                                bb.min.x as i32 + gx as i32,
                                bb.min.y as i32 + gy as i32,
                                style.rgb,
                                cov,
                            );
                        });
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

fn draw_object(pm: &mut Pixmap, xf: &Xf, kind: &ObjectKind, step_no: u32) {
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
        ObjectKind::Blur { r, mode, strength } => {
            let (x0, y0) = (xf.px(r.x).round() as i32, xf.py(r.y).round() as i32);
            let (x1, y1) = (xf.px(r.right()).round() as i32, xf.py(r.bottom()).round() as i32);
            let amount = (strength * xf.s).round().max(2.0) as usize;
            match mode {
                BlurMode::Pixelate => pixelate(pm, x0, y0, x1, y1, amount),
                BlurMode::Gaussian => gaussian(pm, x0, y0, x1, y1, amount),
            }
        }
    }
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
    let (w, h) = content_size(doc);
    if doc.orientation.swaps_axes() {
        (h, w)
    } else {
        (w, h)
    }
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
    orient(&render_content(doc, base), doc.orientation)
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
    let mut step_no = doc.step_start;
    for o in &doc.objects {
        let is_step = matches!(o.kind, ObjectKind::Step { .. });
        if o.visible {
            draw_object(&mut pm, &xf, &o.kind, step_no);
        }
        if is_step {
            step_no += 1;
        }
    }
    to_image(&pm)
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
}
