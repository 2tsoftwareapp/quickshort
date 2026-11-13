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

pub fn apply(doc: &Document, content: RgbaImage) -> RgbaImage {
    if doc.effects == Default::default() {
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
