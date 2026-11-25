//! Stack rendering — the software paint path: thumbnails, badges,
//! fly tween and lightbox, into the premul canvas each backend
//! blits. Child of `stack` so it sees Stack fields directly.

use super::*;

/// Source-over a packed premul pixel `A<<24|B<<16|G<<8|R` at coverage
/// `cov`. Scaling the premult channels by `cov` keeps color and alpha
/// consistent. The bounds check is load-bearing: without it an
/// off-screen rect (e.g. a thumb hidden past the right edge) wraps
/// `y*w + x` into the next row — the image re-appears on the LEFT.
#[inline]
fn blit_px(cv: &mut Canvas, x: i32, y: i32, px: u32, cov: f64) {
    if cov <= 0.0 || x < 0 || y < 0 || x >= cv.w as i32 || y >= cv.h as i32 {
        return;
    }
    let i = y as usize * cv.w + x as usize;
    let sa = (px >> 24) as f64 / 255.0 * cov;
    let ia = 1.0 - sa;
    let d = cv.buf[i];
    let oa = sa * 255.0 + (d >> 24) as f64 * ia;
    let r = (px & 0xFF) as f64 * cov + (d & 0xFF) as f64 * ia;
    let g = ((px >> 8) & 0xFF) as f64 * cov + ((d >> 8) & 0xFF) as f64 * ia;
    let b = ((px >> 16) & 0xFF) as f64 * cov + ((d >> 16) & 0xFF) as f64 * ia;
    cv.buf[i] = (oa.round() as u32) << 24
        | (b.round() as u32) << 16
        | (g.round() as u32) << 8
        | r.round() as u32;
}

/// Nearest-neighbour blit of packed-premul `src` (sw×sh) into the
/// logical rect `dst`, clipped to a rounded rect `rad` (logical).
fn blit_img(cv: &mut Canvas, src: &[u32], sw: usize, sh: usize, dst: FRect, rad: f64) {
    let s = cv.scale;
    // clip to the buffer — off-screen pixels are dead work, and would
    // wrap into visible rows downstream
    let (x0, x1) = (
        ((dst.x * s) as i32).max(0),
        (((dst.x + dst.w) * s).ceil() as i32).min(cv.w as i32),
    );
    let (y0, y1) = (
        ((dst.y * s) as i32).max(0),
        (((dst.y + dst.h) * s).ceil() as i32).min(cv.h as i32),
    );
    for py in y0..y1 {
        for px in x0..x1 {
            let lx = px as f64 / s + 0.5 / s;
            let ly = py as f64 / s + 0.5 / s;
            if rad > 0.0 {
                let d = rrect_sdf(
                    lx,
                    ly,
                    dst.x + dst.w / 2.0,
                    dst.y + dst.h / 2.0,
                    dst.w / 2.0,
                    dst.h / 2.0,
                    rad,
                );
                if d > 0.0 {
                    continue;
                }
                let cov = fill_cov(d, s);
                let u = (((lx - dst.x) / dst.w) * sw as f64) as usize;
                let v = (((ly - dst.y) / dst.h) * sh as f64) as usize;
                blit_px(cv, px, py, src[v.min(sh - 1) * sw + u.min(sw - 1)], cov);
            } else {
                let u = (((lx - dst.x) / dst.w) * sw as f64) as usize;
                let v = (((ly - dst.y) / dst.h) * sh as f64) as usize;
                blit_px(cv, px, py, src[v.min(sh - 1) * sw + u.min(sw - 1)], 1.0);
            }
        }
    }
}

fn rrect(cv: &mut Canvas, r: FRect, rad: f64, fill: u32, fa: f64, border: Option<u32>) {
    let s = cv.scale;
    // +0.5/s: evaluate the SDF at pixel CENTERS like glass_card — corner
    // sampling lands the edge exactly on the top/left boundary but half
    // a pixel past it on bottom/right, so the hairline dies there
    for py in (r.y * s) as i32..((r.y + r.h) * s) as i32 {
        for px in (r.x * s) as i32..((r.x + r.w) * s) as i32 {
            let d = rrect_sdf(
                px as f64 / s + 0.5 / s,
                py as f64 / s + 0.5 / s,
                r.x + r.w / 2.0,
                r.y + r.h / 2.0,
                r.w / 2.0,
                r.h / 2.0,
                rad,
            );
            if d <= 0.0 {
                cv.blend_over(px, py, fill, fill_cov(d, s) * fa);
            }
            if let Some(b) = border {
                let rim = stroke_cov(d, 0.7, s);
                if rim > 0.0 {
                    cv.blend_over(px, py, b, rim);
                }
            }
        }
    }
}

fn disc(cv: &mut Canvas, cx: f64, cy: f64, r: f64, fill: u32, fa: f64) {
    let s = cv.scale;
    for py in ((cy - r) * s) as i32..((cy + r) * s) as i32 {
        for px in ((cx - r) * s) as i32..((cx + r) * s) as i32 {
            let d = ellipse_sdf(
                px as f64 / s + 0.5 / s,
                py as f64 / s + 0.5 / s,
                cx,
                cy,
                r,
                r,
            );
            if d <= 0.0 {
                cv.blend_over(px, py, fill, fill_cov(d, s) * fa);
            }
        }
    }
}

impl Stack {
    /// Paint the whole stack into the window buffer (already zeroed).
    pub fn render(&self, cv: &mut Canvas, font: &fontdue::Font, accent: u32, now: Instant) {
        let (lw, lh) = (cv.w as f64 / cv.scale, cv.h as f64 / cv.scale);
        let t = Txt {
            font,
            scale: cv.scale,
        };

        if let Some(i) = self.lightbox {
            // backdrop dim fades in; the shot scales up from 94% —
            // real alpha here is the chrome-layer payoff
            let bp = self
                .box_t0
                .map(|t0| (now.duration_since(t0).as_secs_f64() * 1000.0 / BOX_MS).min(1.0))
                .unwrap_or(1.0);
            // uniform dim over a zeroed buffer → direct fill, no blend
            let dim = ((0.55 * ease_out(bp) * 255.0).round() as u32) << 24;
            cv.buf.fill(dim);
            if self.shot(i).is_some() {
                let grow = 0.94 + 0.06 * ease_out_back(bp);
                let settled = self.box_img_rect(lw, lh);
                let r = FRect {
                    x: settled.x + settled.w * (1.0 - grow) / 2.0,
                    y: settled.y + settled.h * (1.0 - grow) / 2.0,
                    w: settled.w * grow,
                    h: settled.h * grow,
                };
                blit_img(cv, &self.box_img, self.box_iw, self.box_ih, r, 8.0);
                let fr = FRect {
                    x: r.x - 1.0,
                    y: r.y - 1.0,
                    w: r.w + 2.0,
                    h: r.h + 2.0,
                };
                rrect(
                    cv,
                    fr,
                    9.0,
                    qs_theme::pal().popover,
                    0.0,
                    Some(qs_theme::pal().border),
                );
                if bp >= 1.0 {
                    // action bar — the mouse path out of the preview
                    // (keys: Esc close, Enter copy, O open, S share,
                    // ⌫ remove)
                    for (bi, br) in self.box_action_rects(lw, lh).iter().enumerate() {
                        let hot = self.hover == Some(Hit::BoxAction(bi));
                        let danger = BOX_ACTIONS[bi] == MENU_DEL;
                        // white labels need the deepened accent — raw
                        // orange/pink carry white at only ~2-3:1
                        let fill = if hot {
                            config::deepen(accent)
                        } else {
                            qs_theme::pal().popover
                        };
                        let fa = if hot { 0.9 } else { 0.92 };
                        rrect(cv, *br, 8.0, fill, fa, Some(qs_theme::pal().border));
                        let label = Self::menu_label(BOX_ACTIONS[bi]);
                        let tw = t.width(10.0, label);
                        t.draw_over(
                            cv,
                            br.x + (br.w - tw) / 2.0,
                            br.y + 12.5,
                            10.0,
                            label,
                            if danger && !hot {
                                qs_theme::pal().danger
                            } else {
                                qs_theme::pal().on_accent
                            },
                        );
                    }
                    // shortcut legend, on its own pill so it stays legible
                    // over any wallpaper; skipped when a tall image leaves
                    // no room below the action bar
                    let hint = "ESC CLOSE · O OPEN · S SHARE · ⌫ REMOVE";
                    let hw = t.width(9.5, hint);
                    let hy = settled.y + settled.h + 58.0;
                    if hy + 22.0 <= lh - 8.0 {
                        let hr = FRect {
                            x: (lw - hw) / 2.0 - 10.0,
                            y: hy,
                            w: hw + 20.0,
                            h: 22.0,
                        };
                        rrect(
                            cv,
                            hr,
                            11.0,
                            qs_theme::pal().popover,
                            0.92,
                            Some(qs_theme::pal().border),
                        );
                        t.draw_over(
                            cv,
                            hr.x + 10.0,
                            hr.y + 15.0,
                            9.5,
                            hint,
                            qs_theme::pal().fg_dim,
                        );
                    }
                }
            }
            return;
        }

        // count chip
        if !self.shots.is_empty() {
            let cr = self.chip_rect(lw, lh);
            rrect(
                cv,
                cr,
                10.0,
                qs_theme::pal().popover,
                0.92,
                Some(qs_theme::pal().border),
            );
            let n = self.shots.len();
            let label = if n == 1 {
                "1 SHOT".to_string()
            } else {
                format!("{n} SHOTS")
            };
            let tw = t.width(9.5, &label);
            t.draw_over(
                cv,
                cr.x + (cr.w - tw) / 2.0,
                cr.y + 14.0,
                9.5,
                &label,
                qs_theme::pal().fg_dim,
            );
        }

        // thumbs, oldest first so newest draws on top
        let n = self
            .shots
            .len()
            .min(if self.pinned { MAX_VIS_PINNED } else { MAX_VIS });
        for i in (0..n).rev() {
            // hit index i counts from newest; draw order is reverse
            let idx = self.shots.len() - 1 - i; // storage idx
            let Some(shot) = self.shots.get(idx) else {
                continue;
            };
            let in_flight =
                idx == self.shots.len() - 1 && (self.fly.is_some() || self.native_flight);
            if in_flight {
                continue; // its pixels travel on the (native) fly layer
            }
            let r = self.thumb_rect(i, lw, lh);
            blit_img(cv, &shot.thumb, shot.tw, shot.th, r, 6.0);
            let hot = self.hover == Some(Hit::Thumb(i))
                || self.hover == Some(Hit::Del(i))
                || self.ring == Some(i);
            rrect(
                cv,
                r,
                6.0,
                qs_theme::pal().popover,
                0.0,
                Some(if hot { accent } else { qs_theme::pal().border }),
            );

            // badges are corner-anchored: while the stack is collapsed
            // only the top card can wear them, or every offset card
            // piles a badge into the same corner
            let detailed = i == 0 || self.fan_t >= 0.5;
            if detailed {
                // distinct marks per outcome — four different actions all
                // showing "✓" told the user nothing at a glance
                let (bg, glyph) = match shot.kind {
                    ShotKind::Saved => (qs_theme::pal().success, "✓"),
                    ShotKind::Copied => (qs_theme::pal().success, "⧉"),
                    ShotKind::Ocr => (qs_theme::pal().success, "T"),
                    ShotKind::Scroll => (qs_theme::pal().success, "↕"),
                    ShotKind::Shared => (accent, "↗"),
                    ShotKind::Record => (accent, "▶"),
                };
                let (bx, by) = (r.x + 6.0, r.y + r.h - 6.0);
                disc(cv, bx, by, 8.0, bg, 0.95);
                t.draw_over(
                    cv,
                    bx - t.width(9.0, glyph) / 2.0,
                    by + 3.4,
                    9.0,
                    glyph,
                    qs_theme::pal().fringe,
                );

                // a live share link gets its own dot on the opposite
                // corner — the kind badge marks provenance, ● marks
                // "this file is still public until revoked/expired"
                if shot.shared.is_some() {
                    let lx = r.x + r.w - 6.0;
                    let ly = r.y + r.h - 6.0;
                    disc(cv, lx, ly, 8.0, accent, 0.95);
                    t.draw_over(
                        cv,
                        lx - t.width(9.0, "●") / 2.0,
                        ly + 3.4,
                        9.0,
                        "●",
                        qs_theme::pal().fringe,
                    );
                }

                // recordings also carry their length — a `m:ss` pill
                // hugging the ▶ disc, bottom-left like CleanShot
                if shot.kind == ShotKind::Record {
                    if let Some(d) = shot.dur_s {
                        let label = format!("{}:{:02}", d / 60, d % 60);
                        let tw = t.width(8.5, &label);
                        let (px, py) = (bx + 10.0, by + 5.5);
                        rrect(
                            cv,
                            FRect {
                                x: px - 4.0,
                                y: py - 5.0,
                                w: tw + 8.0,
                                h: 11.0,
                            },
                            5.5,
                            qs_theme::pal().popover,
                            0.9,
                            None,
                        );
                        t.draw_over(cv, px, py, 8.5, &label, qs_theme::pal().on_accent);
                    }
                }
            }

            // × on hover
            if self.hover == Some(Hit::Thumb(i)) || self.hover == Some(Hit::Del(i)) {
                let (dx, dy) = self.del_pos(r);
                disc(cv, dx, dy, 8.0, qs_theme::pal().popover, 0.95);
                let del_hot = self.hover == Some(Hit::Del(i));
                t.draw_over(
                    cv,
                    dx - t.width(10.0, "×") / 2.0,
                    dy + 3.6,
                    10.0,
                    "×",
                    if del_hot {
                        qs_theme::pal().danger
                    } else {
                        qs_theme::pal().fg_dim
                    },
                );
            }
        }

        // "CLEAR SHELF" pill under the row, fading in with the fan —
        // red text on hover like the per-thumb × (dismiss-only)
        if let Some(cr) = self.clear_rect(lw, lh) {
            let a = ((self.fan_t - 0.4) / 0.6).clamp(0.0, 1.0);
            let hot = self.hover == Some(Hit::ClearAll);
            rrect(
                cv,
                cr,
                11.0,
                qs_theme::pal().popover,
                0.92 * a,
                Some(if hot { accent } else { qs_theme::pal().border }),
            );
            let label = "× CLEAR SHELF";
            let tw = t.width(9.5, label);
            t.draw_over(
                cv,
                cr.x + (cr.w - tw) / 2.0,
                cr.y + 15.0,
                9.5,
                label,
                if hot {
                    qs_theme::pal().danger
                } else {
                    qs_theme::pal().fg_dim
                },
            );
        }

        // the flight itself
        if let Some(f) = &self.fly {
            let p = ease_out(self.fly_t(now));
            let to = self.thumb_rect(0, lw, lh);
            let r = FRect {
                x: f.from.x + (to.x - f.from.x) * p,
                y: f.from.y + (to.y - f.from.y) * p,
                w: f.from.w + (to.w - f.from.w) * p,
                h: f.from.h + (to.h - f.from.h) * p,
            };
            // draw the capture scaled into the moving rect — border fades
            // from accent to hairline as it "becomes" a stack thumb
            blit_img(cv, &f.packed, f.iw, f.ih, r, 5.0);
            let border = if p < 0.6 {
                accent
            } else {
                qs_theme::pal().border
            };
            rrect(cv, r, 5.0, qs_theme::pal().popover, 0.0, Some(border));
        }

        // context menu floats above everything in the layer
        if let Some(m) = self.menu.as_ref() {
            if let Some(p) = self.menu_rect(lw, lh) {
                // dim everything behind the menu slightly so it pops
                // (backdrop clicks close it — hit() handles routing)
                rrect(
                    cv,
                    p,
                    10.0,
                    qs_theme::pal().popover,
                    0.96,
                    Some(qs_theme::pal().border),
                );
                for (i, &(id, enabled)) in m.rows.iter().enumerate() {
                    let Some(rr) = self.menu_row_rect(i, lw, lh) else {
                        continue;
                    };
                    let hot = self.hover == Some(Hit::MenuRow(i)) && enabled;
                    if hot {
                        rrect(cv, rr, 6.0, accent, 0.22, None);
                    }
                    let col = if !enabled {
                        qs_theme::pal().border
                    } else if hot {
                        qs_theme::pal().on_accent
                    } else {
                        qs_theme::pal().fg_dim
                    };
                    t.draw_over(
                        cv,
                        rr.x + 10.0,
                        rr.y + 19.0,
                        10.5,
                        Self::menu_label(id),
                        col,
                    );
                }
            }
        }
    }
}

/// RgbaImage → packed premul `A<<24|B<<16|G<<8|R` pixels.
pub(super) fn pack_image(img: &RgbaImage) -> Vec<u32> {
    img.pixels()
        .map(|p| {
            let (r, g, b, a) = (p[0] as u32, p[1] as u32, p[2] as u32, p[3] as u32);
            (a << 24) | ((b * a / 255) << 16) | ((g * a / 255) << 8) | (r * a / 255)
        })
        .collect()
}

/// Cover-crop normal aspects, letterbox extreme ones (scroll shots read
/// as "tall" — the shape is the metadata).
pub(super) fn make_thumb(img: &RgbaImage, kind: ShotKind, scale: f64) -> (Vec<u32>, usize, usize) {
    let aspect = img.width() as f64 / img.height() as f64;
    let (tw, th) = ((TW * scale) as usize, (TH * scale) as usize);
    let letter = aspect < 0.5 || aspect > 3.0 || kind == ShotKind::Scroll;
    let (iw, ih) = (img.width() as usize, img.height() as usize);
    let mut out = vec![0u32; tw * th]; // transparent → #050505 mat drawn at blit
    let pack = |x: usize, y: usize| -> u32 {
        let p = img.get_pixel(x.min(iw - 1) as u32, y.min(ih - 1) as u32);
        let (r, g, b, a) = (p[0] as u32, p[1] as u32, p[2] as u32, p[3] as u32);
        (a << 24) | ((b * a / 255) << 16) | ((g * a / 255) << 8) | (r * a / 255)
    };
    if letter {
        let fit = (tw as f64 / iw as f64).min(th as f64 / ih as f64);
        let (dw, dh) = ((iw as f64 * fit) as usize, (ih as f64 * fit) as usize);
        let (ox, oy) = ((tw - dw) / 2, (th - dh) / 2);
        for y in 0..dh {
            for x in 0..dw {
                out[(oy + y) * tw + ox + x] =
                    pack((x as f64 / fit) as usize, (y as f64 / fit) as usize);
            }
        }
    } else {
        let s = iw.max(ih) as f64 / tw.max(th) as f64;
        let (ox, oy) = (
            (iw as f64 - tw as f64 * s).max(0.0) / 2.0,
            (ih as f64 - th as f64 * s).max(0.0) / 2.0,
        );
        for y in 0..th {
            for x in 0..tw {
                out[y * tw + x] = pack((ox + x as f64 * s) as usize, (oy + y as f64 * s) as usize);
            }
        }
    }
    (out, tw, th)
}

/* ==================== manager ==================== */
