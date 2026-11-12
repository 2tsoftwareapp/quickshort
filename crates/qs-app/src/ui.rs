//! Shared UI kit for the softbuffer windows (settings today; the toast and
//! shelf can adopt it next). `Painter` holds the primitives (fill, stroke,
//! text, glyphs) and the components built on them (card, switch, swatch,
//! keycaps, buttons). Every colour comes from `qs_theme::pal()`.

use crate::widgets::face_for;
use fontdue::Font;
use qs_overlay::sdf::{ellipse_sdf, fill_cov, rrect_sdf, seg_sdf, stroke_cov, Canvas};
use qs_theme::pal;

/// Height of one card row, shared by every settings-style list.
pub(crate) const ROW: f64 = 52.0;

/// logical x, y, w, h
pub(crate) type LRect = (f64, f64, f64, f64);

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Glyph {
    Crop,
    Gear,
    Keys,
    Drop,
    Folder,
    Info,
    Bolt,
    Text,
    Display,
}

/// SDF + fontdue drawing helpers over the softbuffer surface.
pub(crate) struct Painter<'a> {
    pub(crate) font: &'a Font,
    pub(crate) scale: f64,
    /// current accent preset — mirrors the mockup's `--accent` variable
    pub(crate) accent: u32,
    /// accent darkened for white text/glyphs (nav pill, ticks, switches)
    pub(crate) accent_deep: u32,
}

impl Painter<'_> {
    pub(crate) fn fill(&self, cv: &mut Canvas, r: LRect, rad: f64, rgb: u32, a: f64) {
        let (x, y, w, h) = r;
        let s = self.scale;
        let (x0, y0) = ((x * s) as i32, (y * s) as i32);
        let (x1, y1) = (((x + w) * s) as i32, ((y + h) * s) as i32);
        for py in y0..y1 {
            for px in x0..x1 {
                let lx = px as f64 / s + 0.5 / s;
                let ly = py as f64 / s + 0.5 / s;
                let d = if rad <= 0.0 {
                    // square corners: inside = negative distance to box
                    (lx - x - w / 2.0).abs().max(w / 2.0) + (ly - y - h / 2.0).abs().max(h / 2.0)
                        - (w / 2.0 + h / 2.0)
                } else {
                    rrect_sdf(lx, ly, x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0, rad)
                };
                let c = if rad <= 0.0 { a } else { fill_cov(d, s) * a };
                if c > 0.0 {
                    cv.blend(px, py, rgb, c);
                }
            }
        }
    }

    /// Outline stroke of width `w` (logical px) around a rounded rect.
    pub(crate) fn stroke(&self, cv: &mut Canvas, r: LRect, rad: f64, rgb: u32, a: f64, w: f64) {
        let (x, y, rw, rh) = r;
        let s = self.scale;
        let (x0, y0) = ((x * s) as i32 - 2, (y * s) as i32 - 2);
        let (x1, y1) = (((x + rw) * s) as i32 + 2, ((y + rh) * s) as i32 + 2);
        for py in y0.max(0)..y1.min(cv.h as i32) {
            for px in x0.max(0)..x1.min(cv.w as i32) {
                let lx = px as f64 / s + 0.5 / s;
                let ly = py as f64 / s + 0.5 / s;
                let d = rrect_sdf(lx, ly, x + rw / 2.0, y + rh / 2.0, rw / 2.0, rh / 2.0, rad);
                let c = stroke_cov(d, w, s) * a;
                if c > 0.0 {
                    cv.blend(px, py, rgb, c);
                }
            }
        }
    }

    /// Stroke a segment (logical coords) with SDF coverage.
    pub(crate) fn seg(
        &self,
        cv: &mut Canvas,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        w: f64,
        rgb: u32,
    ) {
        let s = self.scale;
        let (bx0, by0) = (x0.min(x1) - w - 1.0, y0.min(y1) - w - 1.0);
        let (bx1, by1) = (x0.max(x1) + w + 1.0, y0.max(y1) + w + 1.0);
        for py in (by0 * s) as i32..(by1 * s) as i32 {
            for px in (bx0 * s) as i32..(bx1 * s) as i32 {
                let lx = px as f64 / s + 0.5 / s;
                let ly = py as f64 / s + 0.5 / s;
                let c = stroke_cov(seg_sdf(lx, ly, x0, y0, x1, y1), w, s);
                if c > 0.0 {
                    cv.blend(px, py, rgb, c);
                }
            }
        }
    }

    /// Grouped inset card: flat white panel + hairline border.
    pub(crate) fn card(&self, cv: &mut Canvas, r: LRect) {
        self.fill(cv, r, 10.0, pal().surface, 1.0);
        self.stroke(cv, r, 10.0, pal().border, 1.0, 0.75);
    }

    /// Grouped inset card with subtle dividers between rows.
    pub(crate) fn card_rows(&self, cv: &mut Canvas, r: LRect, rows: usize) {
        self.card(cv, r);
        if rows > 1 {
            for i in 1..rows {
                let y = r.1 + i as f64 * ROW;
                self.line(cv, r.0 + 14.0, y, r.0 + r.2 - 14.0);
            }
        }
    }

    /// Text button — hairline border, fills accent on hover.
    pub(crate) fn ghost_button(&self, cv: &mut Canvas, r: LRect, label: &str, hover: bool) {
        if hover {
            self.fill(cv, r, r.3 / 2.0, pal().surface_hover, 1.0);
        } else {
            self.fill(cv, r, r.3 / 2.0, pal().control, 1.0);
        }
        self.stroke(cv, r, r.3 / 2.0, pal().border_strong, 1.0, 0.75);
        let tw = self.text_w(label, 11.5);
        self.text(
            cv,
            r.0 + (r.2 - tw) / 2.0,
            r.1 + r.3 / 2.0 + 4.0,
            11.5,
            label,
            pal().fg,
        );
    }

    /// Unavailable action — dashed-looking hairline, no hover response.
    pub(crate) fn ghost_disabled(&self, cv: &mut Canvas, r: LRect, label: &str) {
        self.fill(cv, r, r.3 / 2.0, pal().popover, 1.0);
        self.stroke(cv, r, r.3 / 2.0, pal().border, 1.0, 0.6);
        let tw = self.text_w(label, 11.5);
        self.text(
            cv,
            r.0 + (r.2 - tw) / 2.0,
            r.1 + r.3 / 2.0 + 4.0,
            11.5,
            label,
            pal().fg_faint,
        );
    }

    /// Keyboard-style keycaps, one per token.
    pub(crate) fn keycaps(&self, cv: &mut Canvas, hotkey: &str, x: f64, y: f64) {
        let mut px = x;
        for t in hotkey_tokens(hotkey) {
            let tw = self.text_w(&t, 11.5);
            let kw = (tw + 14.0).max(24.0);
            let kr = (px, y, kw, 28.0);
            self.fill(cv, kr, 6.0, pal().keycap, 1.0);
            // key bottom edge — makes it read as a physical key
            self.stroke(
                cv,
                (kr.0, kr.1 + 1.0, kr.2, kr.3 - 1.0),
                6.0,
                pal().keycap_edge,
                0.9,
                0.75,
            );
            self.text(cv, px + (kw - tw) / 2.0, y + 19.0, 11.5, &t, pal().fg);
            px += kw + 4.0;
        }
    }

    /// macOS-style sliding switch. `t` = knob position 0..1.
    pub(crate) fn switch(&self, cv: &mut Canvas, r: LRect, t: f64) {
        let track = mix(pal().switch_off, self.accent_deep, t);
        self.fill(cv, r, r.3 / 2.0, track, 1.0);
        let kr = r.3 / 2.0 - 1.5; // knob radius
        let kx = r.0 + r.3 / 2.0 + t * (r.2 - r.3);
        let ky = r.1 + r.3 / 2.0;
        let s = self.scale;
        let (x0, y0) = (((kx - kr) * s) as i32, ((ky - kr) * s) as i32);
        let (x1, y1) = (((kx + kr) * s) as i32, ((ky + kr) * s) as i32);
        for py in y0..y1 {
            for px in x0..x1 {
                let lx = px as f64 / s + 0.5 / s;
                let ly = py as f64 / s + 0.5 / s;
                let c = fill_cov(ellipse_sdf(lx, ly, kx, ky, kr, kr), s);
                if c > 0.0 {
                    cv.blend(px, py, pal().on_accent, c);
                }
            }
        }
    }

    /// Accent preset circle; selected gets a hairline ring.
    pub(crate) fn swatch(&self, cv: &mut Canvas, r: LRect, rgb: u32, selected: bool, hover: bool) {
        let (cx, cy) = (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0);
        let rad = r.3 / 2.0;
        let s = self.scale;
        let (x0, y0) = (((cx - rad - 2.0) * s) as i32, ((cy - rad - 2.0) * s) as i32);
        let (x1, y1) = (((cx + rad + 2.0) * s) as i32, ((cy + rad + 2.0) * s) as i32);
        for py in y0..y1 {
            for px in x0..x1 {
                let lx = px as f64 / s + 0.5 / s;
                let ly = py as f64 / s + 0.5 / s;
                let d = ellipse_sdf(lx, ly, cx, cy, rad, rad);
                let c = fill_cov(d, s);
                if c > 0.0 {
                    cv.blend(px, py, rgb, c);
                }
            }
        }
        if selected || hover {
            self.stroke(
                cv,
                (r.0 - 3.0, r.1 - 3.0, r.2 + 6.0, r.3 + 6.0),
                r.3,
                if selected { rgb } else { pal().fg_dim },
                if selected { 1.0 } else { 0.5 },
                1.2,
            );
        }
    }

    /// Recent-capture thumbnail clipped to a rounded rect + hairline ring.
    pub(crate) fn thumb(&self, cv: &mut Canvas, r: LRect, img: Option<&image::RgbaImage>) {
        let s = self.scale;
        match img {
            Some(img) => {
                let (x0, y0) = ((r.0 * s) as i32, (r.1 * s) as i32);
                for py in 0..img.height() as i32 {
                    for px in 0..img.width() as i32 {
                        let lx = (x0 + px) as f64 / s + 0.5 / s;
                        let ly = (y0 + py) as f64 / s + 0.5 / s;
                        let d = rrect_sdf(
                            lx,
                            ly,
                            r.0 + r.2 / 2.0,
                            r.1 + r.3 / 2.0,
                            r.2 / 2.0,
                            r.3 / 2.0,
                            8.0,
                        );
                        let cov = fill_cov(d, s);
                        if cov <= 0.0 {
                            continue;
                        }
                        let c = img.get_pixel(px as u32, py as u32);
                        let a = cov * c[3] as f64 / 255.0;
                        let rgb = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32;
                        cv.blend(x0 + px, y0 + py, rgb, a);
                    }
                }
            }
            None => self.fill(cv, r, 8.0, pal().tile, 1.0),
        }
        self.stroke(cv, r, 8.0, pal().border_strong, 1.0, 1.0);
    }

    /// Nav / row glyphs — 18×18 logical, SDF strokes.
    pub(crate) fn glyph(&self, cv: &mut Canvas, g: Glyph, x: f64, y: f64, s: f64, rgb: u32) {
        let w = s * 0.11;
        match g {
            Glyph::Crop => {
                let ins = s * 0.18;
                let len = s * 0.26;
                let (x0, y0, x1, y1) = (x + ins, y + ins, x + s - ins, y + s - ins);
                for (ax, ay, bx, by) in [
                    (x0, y0 + len, x0, y0),
                    (x0, y0, x0 + len, y0),
                    (x1 - len, y0, x1, y0),
                    (x1, y0, x1, y0 + len),
                    (x0, y1 - len, x0, y1),
                    (x0, y1, x0 + len, y1),
                    (x1 - len, y1, x1, y1),
                    (x1, y1 - len, x1, y1),
                ] {
                    self.seg(cv, ax, ay, bx, by, w, rgb);
                }
            }
            Glyph::Gear => {
                let (cx, cy) = (x + s / 2.0, y + s / 2.0);
                for i in 0..8 {
                    let a = i as f64 * std::f64::consts::FRAC_PI_4;
                    let (dx, dy) = (a.cos(), a.sin());
                    self.seg(
                        cv,
                        cx + dx * s * 0.36,
                        cy + dy * s * 0.36,
                        cx + dx * s * 0.48,
                        cy + dy * s * 0.48,
                        w * 1.3,
                        rgb,
                    );
                }
                self.circle(cv, cx, cy, s * 0.28, w, rgb);
            }
            Glyph::Keys => {
                self.stroke(
                    cv,
                    (x + s * 0.08, y + s * 0.24, s * 0.84, s * 0.52),
                    3.0,
                    rgb,
                    1.0,
                    w,
                );
                for r in 0..2 {
                    for c in 0..3 {
                        self.fill(
                            cv,
                            (
                                x + s * (0.18 + c as f64 * 0.24),
                                y + s * (0.32 + r as f64 * 0.2),
                                s * 0.12,
                                s * 0.12,
                            ),
                            1.0,
                            rgb,
                            1.0,
                        );
                    }
                }
            }
            Glyph::Drop => {
                // half-filled circle: reads as a paint/appearance swatch
                let (cx, cy) = (x + s / 2.0, y + s / 2.0);
                self.circle(cv, cx, cy, s * 0.4, w, rgb);
                let sd = self.scale;
                let (x0, y0) = (((cx - s * 0.4) * sd) as i32, (cy * sd) as i32);
                let (x1, y1) = (((cx + s * 0.4) * sd) as i32, ((cy + s * 0.4) * sd) as i32);
                for py in y0..y1 {
                    for px in x0..x1 {
                        let lx = px as f64 / sd + 0.5 / sd;
                        let ly = py as f64 / sd + 0.5 / sd;
                        if ly < cy {
                            continue;
                        }
                        let c = fill_cov(ellipse_sdf(lx, ly, cx, cy, s * 0.4, s * 0.4), sd);
                        if c > 0.0 {
                            cv.blend(px, py, rgb, c);
                        }
                    }
                }
            }
            Glyph::Folder => {
                let body = (x + s * 0.06, y + s * 0.3, s * 0.88, s * 0.56);
                self.stroke(cv, body, 3.0, rgb, 1.0, w);
                self.fill(
                    cv,
                    (x + s * 0.06, y + s * 0.2, s * 0.4, s * 0.14),
                    2.0,
                    rgb,
                    1.0,
                );
            }
            Glyph::Info => {
                let (cx, cy) = (x + s / 2.0, y + s / 2.0);
                self.circle(cv, cx, cy, s * 0.42, w, rgb);
                self.fill(
                    cv,
                    (cx - s * 0.07, cy - s * 0.3, s * 0.14, s * 0.14),
                    s * 0.07,
                    rgb,
                    1.0,
                );
                self.seg(cv, cx, cy - s * 0.06, cx, cy + s * 0.24, w * 1.4, rgb);
            }
            Glyph::Text => {
                // three staggered text lines — reads as a document
                for (i, frac) in [0.62_f64, 0.84, 0.5].iter().enumerate() {
                    let ly = y + s * (0.28 + i as f64 * 0.22);
                    self.seg(
                        cv,
                        x + s * 0.16,
                        ly,
                        x + s * (0.16 + frac),
                        ly,
                        w * 1.4,
                        rgb,
                    );
                }
            }
            Glyph::Bolt => {
                self.seg(
                    cv,
                    x + s * 0.58,
                    y + s * 0.06,
                    x + s * 0.3,
                    y + s * 0.55,
                    w * 1.4,
                    rgb,
                );
                self.seg(
                    cv,
                    x + s * 0.3,
                    y + s * 0.55,
                    x + s * 0.55,
                    y + s * 0.55,
                    w * 1.4,
                    rgb,
                );
                self.seg(
                    cv,
                    x + s * 0.55,
                    y + s * 0.55,
                    x + s * 0.4,
                    y + s * 0.94,
                    w * 1.4,
                    rgb,
                );
            }
            Glyph::Display => {
                let ins = s * 0.12;
                self.stroke(
                    cv,
                    (x + ins, y + s * 0.14, s - ins * 2.0, s * 0.54),
                    2.0,
                    rgb,
                    1.0,
                    w,
                );
                self.seg(
                    cv,
                    x + s * 0.5,
                    y + s * 0.68,
                    x + s * 0.5,
                    y + s * 0.86,
                    w,
                    rgb,
                );
                self.seg(
                    cv,
                    x + s * 0.32,
                    y + s * 0.86,
                    x + s * 0.68,
                    y + s * 0.86,
                    w,
                    rgb,
                );
            }
        }
    }

    /// Circle outline via ellipse SDF stroke.
    pub(crate) fn circle(&self, cv: &mut Canvas, cx: f64, cy: f64, r: f64, w: f64, rgb: u32) {
        let s = self.scale;
        let (x0, y0) = (((cx - r - w) * s) as i32, ((cy - r - w) * s) as i32);
        let (x1, y1) = (((cx + r + w) * s) as i32, ((cy + r + w) * s) as i32);
        for py in y0..y1 {
            for px in x0..x1 {
                let lx = px as f64 / s + 0.5 / s;
                let ly = py as f64 / s + 0.5 / s;
                let c = stroke_cov(ellipse_sdf(lx, ly, cx, cy, r, r), w, s);
                if c > 0.0 {
                    cv.blend(px, py, rgb, c);
                }
            }
        }
    }

    /// App glyph — the tray's crop-corners mark in an accent squircle.
    pub(crate) fn icon(&self, cv: &mut Canvas, x: f64, y: f64, size: f64) {
        self.fill(cv, (x, y, size, size), 9.0, self.accent_deep, 1.0);
        let st = size * 0.10;
        let ins = size * 0.24;
        let len = size * 0.17;
        let (x0, y0, x1, y1) = (x + ins, y + ins, x + size - ins, y + size - ins);
        let segs = [
            (x0, y0 + len, x0, y0),
            (x0, y0, x0 + len, y0),
            (x1 - len, y0, x1, y0),
            (x1, y0, x1, y0 + len),
            (x0, y1 - len, x0, y1),
            (x0, y1, x0 + len, y1),
            (x1 - len, y1, x1, y1),
            (x1, y1 - len, x1, y1),
        ];
        let s = self.scale;
        let (px0, py0) = ((x * s) as i32, (y * s) as i32);
        let (px1, py1) = (((x + size) * s) as i32, ((y + size) * s) as i32);
        for py in py0..py1 {
            for px in px0..px1 {
                let lx = px as f64 / s + 0.5 / s;
                let ly = py as f64 / s + 0.5 / s;
                let mut d = f64::INFINITY;
                for &(ax, ay, bx, by) in &segs {
                    d = d.min(seg_sdf(lx, ly, ax, ay, bx, by));
                }
                let c = stroke_cov(d, st, s);
                if c > 0.0 {
                    cv.blend(px, py, pal().on_accent, c);
                }
            }
        }
    }

    pub(crate) fn line(&self, cv: &mut Canvas, x0: f64, y: f64, x1: f64) {
        let s = self.scale;
        for px in (x0 * s) as i32..(x1 * s) as i32 {
            cv.blend(px, (y * s) as i32, pal().border, 1.0);
        }
    }

    /// Draw `s` with baseline at logical (x, y); returns advance width.
    pub(crate) fn text(
        &self,
        cv: &mut Canvas,
        x: f64,
        y: f64,
        size: f64,
        s: &str,
        rgb: u32,
    ) -> f64 {
        let scale = self.scale;
        let px = size * scale;
        let mut pen = x * scale;
        let baseline = y * scale;
        for ch in s.chars() {
            let (m, bmp) = face_for(self.font, ch).rasterize(ch, px as f32);
            let gx = (pen + m.xmin as f64) as i32;
            let gy = (baseline - m.height as f64 - m.ymin as f64) as i32;
            for row in 0..m.height {
                for col in 0..m.width {
                    let a = bmp[row * m.width + col] as f64 / 255.0;
                    if a > 0.0 {
                        cv.blend(gx + col as i32, gy + row as i32, rgb, a);
                    }
                }
            }
            pen += m.advance_width as f64;
        }
        (pen - x * scale) / scale
    }

    pub(crate) fn text_w(&self, s: &str, size: f64) -> f64 {
        s.chars()
            .map(|c| {
                let (m, _) = face_for(self.font, c).rasterize(c, (size * self.scale) as f32);
                m.advance_width as f64
            })
            .sum::<f64>()
            / self.scale
    }
}

pub(crate) fn mix(a: u32, b: u32, t: f64) -> u32 {
    let f = |sh: u32| ((a >> sh) & 0xFF) as f64 * (1.0 - t) + ((b >> sh) & 0xFF) as f64 * t;
    (f(16) as u32) << 16 | (f(8) as u32) << 8 | f(0) as u32
}

// components ----------------------------------------------------------------

/// Card row: icon tile + label (+ dim description) on the left.
pub(crate) fn row_label(p: &Painter, cv: &mut Canvas, r: LRect, g: Glyph, label: &str, desc: &str) {
    let tile = (r.0 + 12.0, r.1 + (r.3 - 36.0) / 2.0, 36.0, 36.0);
    p.fill(cv, tile, 9.0, pal().tile, 1.0);
    p.glyph(cv, g, tile.0 + 9.0, tile.1 + 9.0, 18.0, pal().tile_glyph);
    p.text(cv, r.0 + 60.0, r.1 + 21.0, 13.0, label, pal().fg);
    if !desc.is_empty() {
        p.text(cv, r.0 + 60.0, r.1 + 39.0, 10.5, desc, pal().fg_faint);
    }
}

pub(crate) fn dropdown_btn(
    p: &Painter,
    cv: &mut Canvas,
    r: LRect,
    label: &str,
    open: bool,
    hover: bool,
) {
    p.fill(
        cv,
        r,
        7.0,
        if open || hover {
            pal().surface_hover
        } else {
            pal().surface
        },
        1.0,
    );
    p.stroke(cv, r, 7.0, pal().border_strong, 1.0, 0.75);
    p.text(cv, r.0 + 10.0, r.1 + 20.0, 12.0, label, pal().fg);
    // chevron ⌄
    let (mx, my) = (r.0 + r.2 - 16.0, r.1 + r.3 / 2.0 - 2.0);
    p.seg(cv, mx, my, mx + 4.0, my + 5.0, 1.6, pal().fg_dim);
    p.seg(cv, mx + 8.0, my, mx + 4.0, my + 5.0, 1.6, pal().fg_dim);
}

pub(crate) fn ghost(
    p: &Painter,
    cv: &mut Canvas,
    r: LRect,
    label: &str,
    hover: bool,
    confirm: bool,
) -> LRect {
    if confirm {
        p.fill(cv, r, r.3 / 2.0, pal().danger, 1.0);
        p.text(
            cv,
            r.0 + (r.2 - p.text_w("Confirm?", 11.5)) / 2.0,
            r.1 + r.3 / 2.0 + 4.0,
            11.5,
            "Confirm?",
            pal().on_accent,
        );
    } else {
        p.ghost_button(cv, r, label, hover);
    }
    r
}

/// Square checkbox + tick — a checklist control where a switch would
/// imply exclusivity.
pub(crate) fn check_box(p: &Painter, cv: &mut Canvas, x: f64, y: f64, on: bool, hover: bool) {
    if on {
        p.fill(cv, (x, y, 20.0, 20.0), 5.0, p.accent_deep, 1.0);
        p.seg(
            cv,
            x + 5.5,
            y + 10.5,
            x + 9.0,
            y + 14.0,
            2.0,
            pal().on_accent,
        );
        p.seg(
            cv,
            x + 9.0,
            y + 14.0,
            x + 15.0,
            y + 6.5,
            2.0,
            pal().on_accent,
        );
    } else {
        p.fill(cv, (x, y, 20.0, 20.0), 5.0, pal().on_accent, 1.0);
        p.stroke(
            cv,
            (x, y, 20.0, 20.0),
            5.0,
            if hover {
                pal().border_strong
            } else {
                pal().border
            },
            1.0,
            1.5,
        );
    }
}

/// total width of a keycap row for `hotkey` (for right-alignment)
pub(crate) fn keycaps_w(p: &Painter, hotkey: &str) -> f64 {
    hotkey_tokens(hotkey)
        .iter()
        .map(|t| (p.text_w(t, 11.5) + 14.0).max(24.0) + 4.0)
        .sum::<f64>()
        - 4.0
}

/// "CmdOrCtrl+Shift+S" -> per-keycap tokens ("⌘", "⇧", "S" on macOS).
/// "⌘+s" style strings pass straight through.
pub(crate) fn hotkey_tokens(s: &str) -> Vec<String> {
    s.split('+')
        .map(|t| {
            let t = t.trim();
            #[cfg(target_os = "macos")]
            {
                match t {
                    "CmdOrCtrl" | "Cmd" | "Super" | "Command" => "⌘".to_string(),
                    "Ctrl" | "Control" => "⌃".to_string(),
                    "Alt" | "Option" => "⌥".to_string(),
                    "Shift" => "⇧".to_string(),
                    other => other.to_uppercase(),
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                match t {
                    "CmdOrCtrl" | "Ctrl" | "Control" => "Ctrl".to_string(),
                    "Super" | "Command" | "Win" => "Super".to_string(),
                    "Alt" | "Option" => "Alt".to_string(),
                    "Shift" => "Shift".to_string(),
                    other => other.to_uppercase(),
                }
            }
        })
        .collect()
}

// canvas primitives ----------------------------------------------------------
// Free functions over a `Canvas` for the small popups (toast, shelf, REC pill,
// pin) that need no font or accent state. Pixels are sampled at
// `px / scale` (no half-pixel offset) — the convention those windows were
// drawn with, so moving them here is pixel-identical.

fn px_range(lo: f64, hi: f64, s: f64) -> std::ops::Range<i32> {
    (lo * s) as i32..(hi * s) as i32
}

/// Filled anti-aliased circle.
pub(crate) fn disc(cv: &mut Canvas, cx: f64, cy: f64, r: f64, rgb: u32, a: f64) {
    let s = cv.scale;
    for py in px_range(cy - r, cy + r, s) {
        for px in px_range(cx - r, cx + r, s) {
            let d = ellipse_sdf(px as f64 / s, py as f64 / s, cx, cy, r, r);
            let c = fill_cov(d, s) * a;
            if c > 0.0 {
                cv.blend(px, py, rgb, c);
            }
        }
    }
}

/// Circle outline of stroke width `w`.
pub(crate) fn ring(cv: &mut Canvas, cx: f64, cy: f64, r: f64, w: f64, rgb: u32, a: f64) {
    let s = cv.scale;
    let pad = r + w + 2.0;
    for py in px_range(cy - pad, cy + pad, s) {
        for px in px_range(cx - pad, cx + pad, s) {
            let d = ellipse_sdf(px as f64 / s, py as f64 / s, cx, cy, r, r);
            let c = stroke_cov(d, w, s) * a;
            if c > 0.0 {
                cv.blend(px, py, rgb, c);
            }
        }
    }
}

/// Status marks drawn inside a disc.
#[derive(Clone, Copy)]
pub(crate) enum Mark {
    Tick,
    Cross,
}

/// A tick or × centred on (cx, cy), stroke width `w`.
pub(crate) fn mark(cv: &mut Canvas, m: Mark, cx: f64, cy: f64, w: f64, rgb: u32) {
    let s = cv.scale;
    let segs: [(f64, f64, f64, f64); 2] = match m {
        Mark::Tick => [(-4.0, 0.0, -1.0, 3.0), (-1.0, 3.0, 4.5, -3.5)],
        Mark::Cross => [(-3.0, -3.0, 3.0, 3.0), (-3.0, 3.0, 3.0, -3.0)],
    };
    for py in px_range(cy - 6.0, cy + 6.0, s) {
        for px in px_range(cx - 6.0, cx + 6.0, s) {
            let (lx, ly) = (px as f64 / s, py as f64 / s);
            let d = segs
                .iter()
                .map(|&(x0, y0, x1, y1)| seg_sdf(lx, ly, cx + x0, cy + y0, cx + x1, cy + y1))
                .fold(f64::INFINITY, f64::min);
            let c = stroke_cov(d, w, s);
            if c > 0.0 {
                cv.blend(px, py, rgb, c);
            }
        }
    }
}

/// Close control: tinted disc with a × on it.
pub(crate) fn close_button(cv: &mut Canvas, cx: f64, cy: f64, r: f64, bg: u32, a: f64, w: f64) {
    disc(cv, cx, cy, r, bg, a);
    mark(cv, Mark::Cross, cx, cy, w, pal().on_accent);
}

/// Square-cornered strip (countdown bars).
pub(crate) fn strip(cv: &mut Canvas, r: LRect, rgb: u32, a: f64) {
    let s = cv.scale;
    let (x, y, w, h) = r;
    for py in px_range(y, y + h, s) {
        for px in px_range(x, x + w, s) {
            cv.blend(px, py, rgb, a);
        }
    }
}

/// Window-sized rounded card: fill + rim inside, soft fringe outside
/// (stands in for the drop shadow a transparent window can't show —
/// softbuffer ignores per-pixel alpha, so the whole surface is painted).
pub(crate) fn card_chrome(
    cv: &mut Canvas,
    w: f64,
    h: f64,
    rad: f64,
    fill: u32,
    border: u32,
    fringe: u32,
) {
    let scale = cv.scale;
    for py in 0..cv.h as i32 {
        for px in 0..cv.w as i32 {
            let d = rrect_sdf(
                px as f64 / scale,
                py as f64 / scale,
                w / 2.0,
                h / 2.0,
                w / 2.0,
                h / 2.0,
                rad,
            );
            if d > 0.0 {
                cv.blend(px, py, fringe, 0.35 * (1.0 - d / 2.5).clamp(0.0, 1.0));
                continue;
            }
            cv.blend(px, py, fill, fill_cov(d, scale));
            let rim = stroke_cov(d, 0.7, scale);
            if rim > 0.0 {
                cv.blend(px, py, border, rim);
            }
        }
    }
}

/// Rounded button background + rim. `alpha` dims the fill (disabled).
pub(crate) fn pill(
    cv: &mut Canvas,
    rect: (f64, f64, f64, f64),
    rad: f64,
    bg: u32,
    border: u32,
    alpha: f64,
) {
    let (bx, by, bw, bh) = rect;
    let scale = cv.scale;
    for py in (by * scale) as i32..((by + bh) * scale) as i32 {
        for px in (bx * scale) as i32..((bx + bw) * scale) as i32 {
            let d = rrect_sdf(
                px as f64 / scale,
                py as f64 / scale,
                bx + bw / 2.0,
                by + bh / 2.0,
                bw / 2.0,
                bh / 2.0,
                rad,
            );
            if d <= 0.0 {
                cv.blend(px, py, bg, fill_cov(d, scale) * alpha);
                let rim = stroke_cov(d, 0.7, scale);
                if rim > 0.0 {
                    cv.blend(px, py, border, rim);
                }
            }
        }
    }
}

#[cfg(test)]
mod prim_tests {
    //! The popup primitives replaced hand-rolled loops in toast/pin/recstop/
    //! shelf. These references are the old toast code, verbatim; the shared
    //! versions must paint the same pixels at 1x and 2x.
    use super::*;
    use qs_overlay::sdf::{ellipse_sdf, fill_cov, seg_sdf, stroke_cov};

    fn old_fill_circle(cv: &mut Canvas, cx: f64, cy: f64, r: f64, rgb: u32, a: f64, s: f64) {
        for py in ((cy - r) * s) as i32..((cy + r) * s) as i32 {
            for px in ((cx - r) * s) as i32..((cx + r) * s) as i32 {
                let d = ellipse_sdf(px as f64 / s, py as f64 / s, cx, cy, r, r);
                let c = fill_cov(d, s) * a;
                if c > 0.0 {
                    cv.blend(px, py, rgb, c);
                }
            }
        }
    }

    fn old_ring(cv: &mut Canvas, cx: f64, cy: f64, r: f64, rgb: u32, a: f64, s: f64) {
        for py in ((cy - r - 2.0) * s) as i32..((cy + r + 2.0) * s) as i32 {
            for px in ((cx - r - 2.0) * s) as i32..((cx + r + 2.0) * s) as i32 {
                let d = ellipse_sdf(px as f64 / s, py as f64 / s, cx, cy, r, r);
                let c = stroke_cov(d, 1.0, s) * a;
                if c > 0.0 {
                    cv.blend(px, py, rgb, c);
                }
            }
        }
    }

    fn old_tick(cv: &mut Canvas, cx: f64, cy: f64, s: f64) {
        for py in ((cy - 6.0) * s) as i32..((cy + 6.0) * s) as i32 {
            for px in ((cx - 6.0) * s) as i32..((cx + 6.0) * s) as i32 {
                let lx = px as f64 / s;
                let ly = py as f64 / s;
                let d = seg_sdf(lx, ly, cx - 4.0, cy, cx - 1.0, cy + 3.0).min(seg_sdf(
                    lx,
                    ly,
                    cx - 1.0,
                    cy + 3.0,
                    cx + 4.5,
                    cy - 3.5,
                ));
                let c = stroke_cov(d, 1.6, s);
                if c > 0.0 {
                    cv.blend(px, py, qs_theme::pal().on_accent, c);
                }
            }
        }
    }

    fn old_cross(cv: &mut Canvas, cx: f64, cy: f64, s: f64) {
        for py in ((cy - 6.0) * s) as i32..((cy + 6.0) * s) as i32 {
            for px in ((cx - 6.0) * s) as i32..((cx + 6.0) * s) as i32 {
                let lx = px as f64 / s;
                let ly = py as f64 / s;
                let d = seg_sdf(lx, ly, cx - 3.0, cy - 3.0, cx + 3.0, cy + 3.0).min(seg_sdf(
                    lx,
                    ly,
                    cx - 3.0,
                    cy + 3.0,
                    cx + 3.0,
                    cy - 3.0,
                ));
                let c = stroke_cov(d, 1.6, s);
                if c > 0.0 {
                    cv.blend(px, py, qs_theme::pal().on_accent, c);
                }
            }
        }
    }

    fn render(scale: f64, f: impl Fn(&mut Canvas)) -> Vec<u32> {
        let (w, h) = ((60.0 * scale) as usize, (60.0 * scale) as usize);
        let mut buf = vec![0x202020u32; w * h];
        let mut cv = Canvas {
            buf: &mut buf,
            w,
            h,
            scale,
        };
        f(&mut cv);
        buf
    }

    #[test]
    fn shared_primitives_match_the_old_toast_pixels() {
        for s in [1.0, 2.0, 1.25] {
            let on = pal().on_accent;
            assert!(
                render(s, |c| old_fill_circle(
                    c, 28.0, 29.5, 11.0, 0xFF0000, 0.7, s
                )) == render(s, |c| disc(c, 28.0, 29.5, 11.0, 0xFF0000, 0.7)),
                "disc @{s}"
            );
            assert!(
                render(s, |c| old_ring(c, 28.0, 29.5, 14.5, 0x00FF00, 0.4, s))
                    == render(s, |c| ring(c, 28.0, 29.5, 14.5, 1.0, 0x00FF00, 0.4)),
                "ring @{s}"
            );
            assert!(
                render(s, |c| old_tick(c, 28.0, 29.5, s))
                    == render(s, |c| mark(c, Mark::Tick, 28.0, 29.5, 1.6, on)),
                "tick @{s}"
            );
            assert!(
                render(s, |c| old_cross(c, 28.0, 29.5, s))
                    == render(s, |c| mark(c, Mark::Cross, 28.0, 29.5, 1.6, on)),
                "cross @{s}"
            );
        }
    }
}
