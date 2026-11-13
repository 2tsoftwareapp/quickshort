//! Effects run after orientation. All compositing uses straight RGBA at this boundary.
use image::{Rgba, RgbaImage};
use qs_doc::Document;

fn rgba(color: u32, opacity: f32) -> Rgba<u8> {
    Rgba([
        (color >> 16) as u8,
        (color >> 8) as u8,
        color as u8,
        (((color >> 24) as f32) * opacity).round() as u8,
    ])
}

fn coverage(x: u32, y: u32, inset: f32, w: f32, h: f32, radius: f32) -> f32 {
    if w <= 2.0 * inset || h <= 2.0 * inset {
        return 0.0;
    }
    let r = radius
        .min((w - 2.0 * inset) / 2.0)
        .min((h - 2.0 * inset) / 2.0)
        .max(0.0);
    let mut n = 0;
    for dy in [0.25, 0.75] {
        for dx in [0.25, 0.75] {
            let (px, py) = (x as f32 + dx, y as f32 + dy);
            if px < inset || py < inset || px > w - inset || py > h - inset {
                continue;
            }
            let cx = px.clamp(inset + r, w - inset - r);
            let cy = py.clamp(inset + r, h - inset - r);
            if (px - cx).hypot(py - cy) <= r {
                n += 1;
            }
        }
    }
    n as f32 / 4.0
}

fn hash(seed: u32, i: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B1) ^ i.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x & 0xffff) as f32 / 65535.0
}

/// 1-D value noise in [0,1] with a lattice every `period` pixels.
fn noise(seed: u32, i: u32, period: f32) -> f32 {
    let f = i as f32 / period;
    let (k, t) = (f.floor() as u32, f.fract());
    let t = t * t * (3.0 - 2.0 * t);
    hash(seed, k) * (1.0 - t) + hash(seed, k + 1) * t
}

fn tear(img: &mut RgbaImage, e: &qs_doc::EdgeEffect, scale: f32) {
    let (w, h) = img.dimensions();
    let amp = (e.amplitude * scale).max(1.0);
    let depth = |seed: u32, i: u32| (1.0 + noise(seed, i, 6.0 * scale.max(1.0)) * (amp - 1.0)).round() as u32;
    for (side, on, len, seed) in [
        (0, e.top, w, e.seed),
        (1, e.bottom, w, e.seed.wrapping_add(1)),
        (2, e.left, h, e.seed.wrapping_add(2)),
        (3, e.right, h, e.seed.wrapping_add(3)),
    ] {
        if !on {
            continue;
        }
        for i in 0..len {
            for d in 0..depth(seed, i) {
                let (x, y) = match side {
                    0 => (i, d),
                    1 => (i, h.wrapping_sub(1 + d)),
                    2 => (d, i),
                    _ => (w.wrapping_sub(1 + d), i),
                };
                if x < w && y < h {
                    img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
                }
            }
        }
    }
}

fn stamp(img: &mut RgbaImage, wm: &qs_doc::WatermarkEffect, scale: f32) {
    if wm.text.is_empty() || wm.opacity <= 0.0 {
        return;
    }
    use ab_glyph::{point, Font, ScaleFont};
    use qs_doc::Anchor::*;
    let px = 24.0 * wm.font_scale;
    let style = qs_doc::TextStyle { px, ..Default::default() };
    let lay = crate::text::layout(&wm.text, &style, None);
    let (bw, bh) = (lay.width * scale, lay.height * scale);
    let margin = 8.0 * scale;
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    let x = match wm.anchor {
        TopLeft | Left | BottomLeft => margin,
        Top | Center | Bottom => (iw - bw) / 2.0,
        _ => iw - bw - margin,
    };
    let y = match wm.anchor {
        TopLeft | Top | TopRight => margin,
        Left | Center | Right => (ih - bh) / 2.0,
        _ => ih - bh - margin,
    };
    let (sr, sg, sb) = ((wm.color >> 16) as u8, (wm.color >> 8) as u8, wm.color as u8);
    let strength = wm.opacity * (wm.color >> 24) as f32 / 255.0;
    crate::text::with_font(px, false, |f| {
        let sc = f.as_scaled(px);
        for l in &lay.lines {
            let mut pen = l.x;
            let mut prev = None;
            for c in wm.text[l.start..l.end].chars() {
                let id = sc.glyph_id(c);
                if let Some(p) = prev {
                    pen += sc.kern(p, id);
                }
                let at = point(x + pen * scale, y + lay.ascent * scale);
                if let Some(og) = f.outline_glyph(id.with_scale_and_position(px * scale, at)) {
                    let bb = og.px_bounds();
                    og.draw(|gx, gy, cov| {
                        let (xx, yy) = (bb.min.x as i32 + gx as i32, bb.min.y as i32 + gy as i32);
                        if xx < 0 || yy < 0 || xx >= img.width() as i32 || yy >= img.height() as i32 {
                            return;
                        }
                        let a = (cov * strength).clamp(0.0, 1.0);
                        let p = img.get_pixel_mut(xx as u32, yy as u32);
                        let da = p[3] as f32 / 255.0;
                        let oa = a + da * (1.0 - a);
                        if oa <= 0.0 {
                            return;
                        }
                        for (k, s) in [sr, sg, sb].into_iter().enumerate() {
                            p[k] = ((s as f32 * a + p[k] as f32 * da * (1.0 - a)) / oa).round() as u8;
                        }
                        p[3] = (oa * 255.0).round() as u8;
                    });
                }
                pen += sc.h_advance(id);
                prev = Some(id);
            }
        }
    });
}

/// Straight-RGBA per-pixel adjustment; alpha is untouched.
fn adjust(img: &mut RgbaImage, c: &qs_doc::ColorAdjustEffect) {
    let k = if c.contrast >= 0.0 { 1.0 + c.contrast * 3.0 } else { 1.0 + c.contrast };
    for p in img.pixels_mut() {
        let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
        let luma = 0.299 * r + 0.587 * g + 0.114 * b;
        for (i, v) in [r, g, b].into_iter().enumerate() {
            let v = luma + (v - luma) * c.saturation;
            let v = (v - 128.0) * k + 128.0 + c.brightness * 255.0;
            p[i] = v.round().clamp(0.0, 255.0) as u8;
        }
    }
}

pub fn apply(doc: &Document, mut content: RgbaImage) -> RgbaImage {
    // Adjust content + annotations first so watermark/border/shadow keep their chosen colors.
    if let Some(c) = &doc.effects.color_adjust {
        adjust(&mut content, c);
    }
    if let Some(wm) = &doc.effects.watermark {
        stamp(&mut content, wm, doc.scale);
    }
    if let Some(e) = &doc.effects.edge {
        tear(&mut content, e, doc.scale);
    }
    if doc.effects.border.is_none() && doc.effects.shadow.is_none() {
        return content;
    }
    let scale = doc.scale;
    let bw = doc
        .effects
        .border
        .as_ref()
        .filter(|b| !b.inset)
        .map_or(0, |b| (b.width * scale).ceil() as u32);
    let mut frame = RgbaImage::new(content.width() + bw * 2, content.height() + bw * 2);
    image::imageops::overlay(&mut frame, &content, bw as i64, bw as i64);
    if let Some(b) = &doc.effects.border {
        let width = if b.inset { b.width * scale } else { bw as f32 };
        let (w, h) = (frame.width() as f32, frame.height() as f32);
        for (x, y, p) in frame.enumerate_pixels_mut() {
            let outer = coverage(x, y, 0.0, w, h, b.radius * scale);
            let inner = coverage(x, y, width, w, h, (b.radius * scale - width).max(0.0));
            let ring = (outer - inner).max(0.0);
            let ink = rgba(b.color, 1.0);
            let ba = ink[3] as f32 / 255.0;
            let ca = p[3] as f32 / 255.0;
            let content_coverage = if b.inset { outer - ring * ba } else { inner };
            let a = ca * content_coverage + ba * ring;
            for k in 0..3 {
                p[k] = if a > 0.0 {
                    ((p[k] as f32 * ca * content_coverage + ink[k] as f32 * ba * ring) / a).round()
                        as u8
                } else {
                    0
                };
            }
            p[3] = (a * 255.0).round() as u8;
        }
    }
    let layout = doc.effect_layout();
    let (w, h) = (
        (layout.size.w * scale).round() as u32,
        (layout.size.h * scale).round() as u32,
    );
    let (fx, fy) = (
        (layout.content_origin.x * scale).round() as i64 - bw as i64,
        (layout.content_origin.y * scale).round() as i64 - bw as i64,
    );
    let mut out = RgbaImage::new(w, h);
    if let Some(s) = &doc.effects.shadow {
        if s.opacity > 0.0 && s.color >> 24 != 0 {
            let mut alpha = image::GrayImage::new(w, h);
            let (dx, dy) = (
                (s.offset.x * scale).round() as i64,
                (s.offset.y * scale).round() as i64,
            );
            for (x, y, p) in frame.enumerate_pixels() {
                let (xx, yy) = (x as i64 + fx + dx, y as i64 + fy + dy);
                if xx >= 0 && yy >= 0 && xx < w as i64 && yy < h as i64 {
                    alpha.put_pixel(xx as u32, yy as u32, image::Luma([p[3]]));
                }
            }
            let blurred = if s.blur_radius > 0.0 {
                image::imageops::blur(&alpha, s.blur_radius * scale)
            } else {
                alpha
            };
            for (p, a) in out.pixels_mut().zip(blurred.pixels()) {
                *p = rgba(s.color, s.opacity * a[0] as f32 / 255.0);
            }
        }
    }
    image::imageops::overlay(&mut out, &frame, fx, fy);
    out
}
