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
    crate::text::with_font(px, false, qs_doc::FontFamily::Sans, |f| {
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

fn apply_cutout_one(img: RgbaImage, c: &qs_doc::Cutout, scale: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let s = (c.start * scale).round().max(0.0) as u32;
    let e = (c.end * scale).round().max(0.0) as u32;
    match c.axis {
        qs_doc::CutoutAxis::Horizontal => {
            let s = s.min(h);
            let e = e.min(h);
            if e <= s || (e - s) >= h {
                return img;
            }
            let new_h = h - (e - s);
            let mut out = RgbaImage::new(w, new_h);
            for y in 0..s {
                for x in 0..w {
                    out.put_pixel(x, y, *img.get_pixel(x, y));
                }
            }
            for y in e..h {
                let dy = y - (e - s);
                for x in 0..w {
                    out.put_pixel(x, dy, *img.get_pixel(x, y));
                }
            }
            let seam_amp = (4.0 * scale).round().max(2.0);
            for x in 0..w {
                let offset = match c.seam {
                    qs_doc::CutoutSeam::Clean => 0i32,
                    qs_doc::CutoutSeam::Jagged => {
                        let period = (10.0 * scale).max(4.0);
                        let t = (x as f32 % period) / period;
                        let tri = if t < 0.5 { t * 2.0 } else { (1.0 - t) * 2.0 };
                        ((tri - 0.5) * seam_amp).round() as i32
                    }
                    qs_doc::CutoutSeam::Wave => {
                        let freq = std::f32::consts::TAU / (16.0 * scale).max(6.0);
                        ((x as f32 * freq).sin() * seam_amp * 0.5).round() as i32
                    }
                };
                let sy = (s as i32 + offset).clamp(0, new_h as i32 - 1) as u32;
                let p = out.get_pixel(x, sy);
                let seam_col = Rgba([
                    p[0].saturating_sub(45),
                    p[1].saturating_sub(45),
                    p[2].saturating_sub(45),
                    p[3].max(180),
                ]);
                out.put_pixel(x, sy, seam_col);
            }
            out
        }
        qs_doc::CutoutAxis::Vertical => {
            let s = s.min(w);
            let e = e.min(w);
            if e <= s || (e - s) >= w {
                return img;
            }
            let new_w = w - (e - s);
            let mut out = RgbaImage::new(new_w, h);
            for y in 0..h {
                for x in 0..s {
                    out.put_pixel(x, y, *img.get_pixel(x, y));
                }
                for x in e..w {
                    out.put_pixel(x - (e - s), y, *img.get_pixel(x, y));
                }
            }
            let seam_amp = (4.0 * scale).round().max(2.0);
            for y in 0..h {
                let offset = match c.seam {
                    qs_doc::CutoutSeam::Clean => 0i32,
                    qs_doc::CutoutSeam::Jagged => {
                        let period = (10.0 * scale).max(4.0);
                        let t = (y as f32 % period) / period;
                        let tri = if t < 0.5 { t * 2.0 } else { (1.0 - t) * 2.0 };
                        ((tri - 0.5) * seam_amp).round() as i32
                    }
                    qs_doc::CutoutSeam::Wave => {
                        let freq = std::f32::consts::TAU / (16.0 * scale).max(6.0);
                        ((y as f32 * freq).sin() * seam_amp * 0.5).round() as i32
                    }
                };
                let sx = (s as i32 + offset).clamp(0, new_w as i32 - 1) as u32;
                let p = out.get_pixel(sx, y);
                let seam_col = Rgba([
                    p[0].saturating_sub(45),
                    p[1].saturating_sub(45),
                    p[2].saturating_sub(45),
                    p[3].max(180),
                ]);
                out.put_pixel(sx, y, seam_col);
            }
            out
        }
    }
}

fn apply_filter(img: &mut RgbaImage, filter: qs_doc::ColorFilter) {
    match filter {
        qs_doc::ColorFilter::None => {}
        qs_doc::ColorFilter::Grayscale => {
            for p in img.pixels_mut() {
                let l = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                p[0] = l;
                p[1] = l;
                p[2] = l;
            }
        }
        qs_doc::ColorFilter::Sepia => {
            for p in img.pixels_mut() {
                let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
                p[0] = (0.393 * r + 0.769 * g + 0.189 * b).round().clamp(0.0, 255.0) as u8;
                p[1] = (0.349 * r + 0.686 * g + 0.168 * b).round().clamp(0.0, 255.0) as u8;
                p[2] = (0.272 * r + 0.534 * g + 0.131 * b).round().clamp(0.0, 255.0) as u8;
            }
        }
        qs_doc::ColorFilter::Invert => {
            for p in img.pixels_mut() {
                p[0] = 255 - p[0];
                p[1] = 255 - p[1];
                p[2] = 255 - p[2];
            }
        }
    }
}

fn apply_page_curl(img: &mut RgbaImage, pc: &qs_doc::PageCurlEffect, scale: f32) {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let sz = (pc.size * scale).min(w * 0.45).min(h * 0.45).max(4.0);
    if w < 8.0 || h < 8.0 {
        return;
    }
    let sh_w = (sz * 0.25).max(2.0);
    for (x, y, p) in img.enumerate_pixels_mut() {
        let px = x as f32 + 0.5;
        let py = y as f32 + 0.5;
        // Distance from the corner in normalized corner-local coords (0..sz)
        let (cx, cy) = match pc.corner {
            qs_doc::Anchor::TopLeft => (px, py),
            qs_doc::Anchor::TopRight => (w - px, py),
            qs_doc::Anchor::BottomLeft => (px, h - py),
            _ => (w - px, h - py),
        };
        if cx < 0.0 || cy < 0.0 {
            continue;
        }
        let sum = cx + cy;
        if sum < sz {
            // Inside the curled-away corner: either paper back (inside triangle) or cut away
            let in_fold = cx <= sz * 0.5 && cy <= sz * 0.5 && sum >= sz * 0.5;
            if in_fold || (cx < sz && cy < sz && (cx - sz * 0.5).abs() + (cy - sz * 0.5).abs() <= sz * 0.5 && sum >= sz * 0.55) {
                let shade = (225.0 + 25.0 * ((sz - sum) / sz)).clamp(190.0, 250.0) as u8;
                *p = Rgba([shade, shade, shade.saturating_sub(4), 255]);
            } else {
                *p = Rgba([0, 0, 0, 0]);
            }
        } else if sum < sz + sh_w && pc.shadow > 0.0 {
            // Subtle fold shadow along the diagonal
            let t = 1.0 - (sum - sz) / sh_w;
            let dim = 1.0 - 0.35 * pc.shadow * t;
            p[0] = (p[0] as f32 * dim).round() as u8;
            p[1] = (p[1] as f32 * dim).round() as u8;
            p[2] = (p[2] as f32 * dim).round() as u8;
        }
    }
}

/// Move every pixel within `tolerance` of the target colour toward the
/// replacement, blended by `strength`.
fn colorize(img: &mut RgbaImage, eff: &qs_doc::ColorizeEffect) {
    let strength = eff.strength.clamp(0.0, 1.0);
    if strength <= 0.0 || eff.target == eff.replacement {
        return;
    }
    let (tr, tg, tb) = (
        (eff.target >> 16) as u8,
        (eff.target >> 8) as u8,
        eff.target as u8,
    );
    let (rr, rg, rb) = (
        (eff.replacement >> 16) as u8,
        (eff.replacement >> 8) as u8,
        eff.replacement as u8,
    );
    let tol = eff.tolerance as i32;
    for p in img.pixels_mut() {
        let [r, g, b, a] = p.0;
        let near = (r as i32 - tr as i32).abs() <= tol
            && (g as i32 - tg as i32).abs() <= tol
            && (b as i32 - tb as i32).abs() <= tol;
        if !near {
            continue;
        }
        let mix = |orig: u8, want: u8| {
            (orig as f32 + (want as f32 - orig as f32) * strength).round() as u8
        };
        p.0 = [mix(r, rr), mix(g, rg), mix(b, rb), a];
    }
}

pub fn apply(doc: &Document, mut content: RgbaImage) -> RgbaImage {
    for c in &doc.cutouts {
        content = apply_cutout_one(content, c, doc.scale);
    }
    // Adjust content + annotations first so watermark/border/shadow keep their chosen colors.
    if let Some(c) = &doc.effects.color_adjust {
        adjust(&mut content, c);
    }
    if let Some(c) = &doc.effects.colorize {
        colorize(&mut content, c);
    }
    if let Some(f) = doc.effects.color_filter {
        apply_filter(&mut content, f);
    }
    if let Some(wm) = &doc.effects.watermark {
        stamp(&mut content, wm, doc.scale);
    }
    if let Some(e) = &doc.effects.edge {
        tear(&mut content, e, doc.scale);
    }
    if let Some(pc) = &doc.effects.page_curl {
        apply_page_curl(&mut content, pc, doc.scale);
    }
    if doc.effects.border.is_none()
        && doc.effects.shadow.is_none()
        && doc.effects.reflection.is_none()
    {
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
    if let Some(rf) = &doc.effects.reflection {
        if rf.opacity > 0.0 && rf.height > 0.0 {
            let rh = ((rf.height * scale).round() as u32).min(frame.height());
            let gap = (rf.gap * scale).round() as i64;
            let top_y = fy + frame.height() as i64 + gap;
            for ry in 0..rh {
                let dst_y = top_y + ry as i64;
                if dst_y < 0 || dst_y >= out.height() as i64 {
                    continue;
                }
                let src_y = frame.height() - 1 - ry;
                let fade = rf.opacity * (1.0 - ry as f32 / rh.max(1) as f32);
                for rx in 0..frame.width() {
                    let dst_x = fx + rx as i64;
                    if dst_x < 0 || dst_x >= out.width() as i64 {
                        continue;
                    }
                    let sp = frame.get_pixel(rx, src_y);
                    let sa = (sp[3] as f32 / 255.0) * fade;
                    if sa <= 0.0 {
                        continue;
                    }
                    let dp = out.get_pixel_mut(dst_x as u32, dst_y as u32);
                    let da = dp[3] as f32 / 255.0;
                    let oa = sa + da * (1.0 - sa);
                    if oa > 0.0 {
                        for k in 0..3 {
                            dp[k] = ((sp[k] as f32 * sa + dp[k] as f32 * da * (1.0 - sa)) / oa)
                                .round()
                                .clamp(0.0, 255.0) as u8;
                        }
                        dp[3] = (oa * 255.0).round() as u8;
                    }
                }
            }
        }
    }
    out
}
