//! Layout math — thumbnail fan, chip, menu and zone rects plus hit-testing.

use super::*;

impl Stack {

    /// Where a native fly should land — the newest thumb at the settled
    /// fan/hide endpoints, so it can't miss a target that is still
    /// tweening when the flight starts.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn landing_rect(&self, w: f64, h: f64) -> FRect {
        self.thumb_rect_at(0, w, h, self.fan_to, self.hide_to)
    }


    /// The anchor point of the newest thumb (fan index 0).
    pub(super) fn anchor(&self, w: f64, h: f64) -> (f64, f64) {
        let x = if self.corner.right() {
            w - MARGIN - TW
        } else if self.corner == Corner::TC || self.corner == Corner::BC {
            (w - TW) / 2.0
        } else {
            MARGIN
        };
        let y = if self.corner.bottom() {
            h - MARGIN - TH
        } else {
            MARGIN
        };
        // (the chrome view already starts below the menu bar — macOS
        // clamps the window to visibleFrame — so no extra top inset)
        (x, y)
    }


    /// Rect of the thumb `i` positions behind the newest (0 = newest).
    /// Hover/pinned fans them horizontally toward the screen middle.
    pub(super) fn thumb_rect(&self, i: usize, w: f64, h: f64) -> FRect {
        self.thumb_rect_at(i, w, h, self.fan_t, self.hide_t)
    }


    /// Same math with explicit tween values — `zone_hit` tests the
    /// endpoints so a rect mid-sweep can't flap the retention zone.
    pub(super) fn thumb_rect_at(&self, i: usize, w: f64, h: f64, fan_t: f64, hide_t: f64) -> FRect {
        let (ax, ay) = self.anchor(w, h);
        let inward = if self.corner.right() { -1.0 } else { 1.0 };
        // hiding collapses every offset so the edge shows ONE sliver
        // (the newest thumb) instead of a diagonal trail of peeks
        let collapse = 1.0 - hide_t;
        let dx = inward * (STACK_DX + (FAN - STACK_DX) * fan_t) * i as f64 * collapse;
        // edge-hidden slides thumbs fully off-screen — the docked chip
        // is the only visible handle (a sliver + floating chip read as
        // two disjoint artifacts)
        let hide_dx = -inward * hide_t * (TW + MARGIN);
        // the fan is a straight horizontal row — the cascade's vertical
        // offset collapses as fan_t opens (was spreading diagonally)
        let dy = if self.corner.bottom() {
            -STACK_DY
        } else {
            STACK_DY
        } * i as f64
            * (1.0 - fan_t)
            * collapse;
        FRect {
            x: ax + dx + hide_dx,
            y: ay + dy,
            w: TW,
            h: TH,
        }
    }


    pub(super) fn chip_rect(&self, w: f64, h: f64) -> FRect {
        self.chip_rect_at(w, h, self.hide_t)
    }


    pub(super) fn chip_rect_at(&self, w: f64, h: f64, hide_t: f64) -> FRect {
        let (ax, ay) = self.anchor(w, h);
        let (cw, ch) = (64.0, 20.0);
        // beside the newest thumb, aligned to its far edge — never
        // clipped by the screen edge regardless of the dock corner
        let shown = if self.corner.right() {
            ax - cw - 8.0
        } else {
            ax + TW + 8.0
        };
        // hidden: dock flush against the screen edge as the sole handle
        let docked = if self.corner.right() {
            w - cw - 2.0
        } else {
            2.0
        };
        let x = shown + (docked - shown) * hide_t;
        let y = if self.corner.bottom() {
            ay + TH - ch
        } else {
            ay
        };
        FRect { x, y, w: cw, h: ch }
    }


    /// "CLEAR SHELF" pill centred under the fanned row — like the
    /// Clear All at the edge of Notification Centre's list, it only
    /// dismisses the shelf; files on disk are untouched. Exists once
    /// the fan is mostly open (hit/live gating matches `input_rects`).
    pub(super) fn clear_rect(&self, w: f64, h: f64) -> Option<FRect> {
        if self.shots.len() < 2 || self.fan_t < 0.5 {
            return None;
        }
        let n = self
            .shots
            .len()
            .min(if self.pinned { MAX_VIS_PINNED } else { MAX_VIS });
        let first = self.thumb_rect_at(0, w, h, self.fan_t, self.hide_t);
        let last = self.thumb_rect_at(n - 1, w, h, self.fan_t, self.hide_t);
        let (pw, ph) = (92.0, 22.0);
        let (left, right) = if first.x < last.x {
            (first.x, last.x + last.w)
        } else {
            (last.x, first.x + first.w)
        };
        let x = (left + right) / 2.0 - pw / 2.0;
        // below the row for top corners, above it for bottom ones
        let y = if self.corner.bottom() {
            first.y - ph - 10.0
        } else {
            first.y + first.h + 10.0
        };
        Some(FRect { x, y, w: pw, h: ph })
    }

    /* ---- in-chrome context menu ---- */


    /// Menu panel rect — anchored at the click, clamped inside the
    /// monitor like a native menu.
    pub(super) fn menu_rect(&self, w: f64, h: f64) -> Option<FRect> {
        let m = self.menu.as_ref()?;
        let mh = m.rows.len() as f64 * MENU_ROW_H + MENU_PAD * 2.0;
        let x = m.anchor.x.min(w - MENU_W - 4.0).max(4.0);
        let y = m.anchor.y.min(h - mh - 4.0).max(4.0);
        Some(FRect {
            x,
            y,
            w: MENU_W,
            h: mh,
        })
    }


    /// Row rect `i` inside the panel.
    pub(super) fn menu_row_rect(&self, i: usize, w: f64, h: f64) -> Option<FRect> {
        let p = self.menu_rect(w, h)?;
        Some(FRect {
            x: p.x + MENU_PAD,
            y: p.y + MENU_PAD + i as f64 * MENU_ROW_H,
            w: p.w - MENU_PAD * 2.0,
            h: MENU_ROW_H,
        })
    }


    pub(super) fn menu_row_at(&self, x: f64, y: f64, w: f64, h: f64) -> Option<usize> {
        let m = self.menu.as_ref()?;
        for i in 0..m.rows.len() {
            if let Some(r) = self.menu_row_rect(i, w, h) {
                if x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h {
                    return Some(i);
                }
            }
        }
        None
    }


    /// The wide retention zone as rects — platform backends feed these
    /// to their input-region API (Wayland `wl_region`, X11 XShape,
    /// Windows `SetWindowRgn`) so pointer moves inside the hysteresis
    /// zone still reach the surface while the rest of the desktop stays
    /// click-through. Rects may overlap; regions union them.
    #[allow(dead_code)] // consumed by the Linux/Windows backends' input regions
    pub fn zone_rects(&self, w: f64, h: f64) -> Vec<FRect> {
        if self.lightbox.is_some() || self.menu.is_some() {
            return vec![FRect {
                x: 0.0,
                y: 0.0,
                w,
                h,
            }];
        }
        let mut v = Vec::new();
        let n = self
            .shots
            .len()
            .min(if self.pinned { MAX_VIS_PINNED } else { MAX_VIS });
        for &ht in &[self.hide_t, 0.0, 1.0] {
            for &ft in &[self.fan_t, 0.0, 1.0] {
                for i in 0..n {
                    v.push(self.thumb_rect_at(i, w, h, ft, ht));
                }
                if !self.shots.is_empty() {
                    v.push(self.chip_rect_at(w, h, ht));
                }
            }
        }
        for r in &mut v {
            r.x -= 28.0;
            r.y -= 28.0;
            r.w += 56.0;
            r.h += 56.0;
        }
        v
    }


    /// Union of everything `render` can touch this frame, in logical
    /// window coords — backends use it as the dirty rect so the layer
    /// upload covers the moving primitives, not the whole monitor.
    /// `None` means nothing is on screen (empty + fully hidden).
    pub fn paint_bounds(&self, w: f64, h: f64, now: Instant) -> Option<FRect> {
        if self.lightbox.is_some() || self.menu.is_some() {
            return Some(FRect {
                x: 0.0,
                y: 0.0,
                w,
                h,
            });
        }
        let mut b: Option<FRect> = None;
        let mut acc = |r: FRect| match &mut b {
            Some(u) => {
                let x0 = u.x.min(r.x);
                let y0 = u.y.min(r.y);
                u.w = (u.x + u.w).max(r.x + r.w) - x0;
                u.h = (u.y + u.h).max(r.y + r.h) - y0;
                u.x = x0;
                u.y = y0;
            }
            None => b = Some(r),
        };
        // thumbs at both tween ends (fan/hide sweep between them) —
        // shadows and the × disc live in the pad
        let n = self
            .shots
            .len()
            .min(if self.pinned { MAX_VIS_PINNED } else { MAX_VIS });
        for i in 0..n {
            for &ft in &[self.fan_t, self.fan_to] {
                for &ht in &[self.hide_t, self.hide_to] {
                    acc(self.thumb_rect_at(i, w, h, ft, ht));
                }
            }
        }
        for &ht in &[self.hide_t, self.hide_to] {
            acc(self.chip_rect_at(w, h, ht));
        }
        if let Some(r) = self.clear_rect(w, h) {
            acc(r);
        }
        if let Some(f) = &self.fly {
            // current position; the previous frame's rect is covered by
            // the caller's prev-dirty union
            let p = ease_out(self.fly_t(now));
            let to = self.thumb_rect(0, w, h);
            acc(FRect {
                x: f.from.x + (to.x - f.from.x) * p,
                y: f.from.y + (to.y - f.from.y) * p,
                w: f.from.w + (to.w - f.from.w) * p,
                h: f.from.h + (to.h - f.from.h) * p,
            });
        }
        b.map(|mut r| {
            r.x -= 30.0;
            r.y -= 30.0;
            r.w += 60.0;
            r.h += 60.0;
            r.x = r.x.max(0.0);
            r.y = r.y.max(0.0);
            r.w = r.w.min(w - r.x);
            r.h = r.h.min(h - r.y);
            r
        })
    }


    /// rects are padded so the × disc overhanging the edge stays live.
    pub fn input_rects(&self, w: f64, h: f64) -> Vec<(FRect, Hit)> {
        if self.lightbox.is_some() || self.menu.is_some() {
            return vec![(
                FRect {
                    x: 0.0,
                    y: 0.0,
                    w,
                    h,
                },
                Hit::Backdrop,
            )];
        }
        let mut v = Vec::new();
        // the pill sits in the thumbs' hit padding — first so it wins
        if let Some(c) = self.clear_rect(w, h) {
            v.push((c, Hit::ClearAll));
        }
        let n = self
            .shots
            .len()
            .min(if self.pinned { MAX_VIS_PINNED } else { MAX_VIS });
        // a native flight has not landed yet — its slot must not catch
        // clicks at a position where no thumb is drawn
        for i in usize::from(self.native_flight)..n {
            let mut r = self.thumb_rect(i, w, h);
            r.x -= 12.0;
            r.y -= 12.0;
            r.w += 24.0;
            r.h += 24.0;
            v.push((r, Hit::Thumb(i)));
        }
        if !self.shots.is_empty() {
            v.push((self.chip_rect(w, h), Hit::Chip));
        }
        v
    }


    /// Retention zone around the interactive rects — while `wide` the
    /// fan/hide tweens only collapse once the cursor leaves the union of
    /// BOTH animation endpoints (the union covers the swept path, so
    /// mid-tween geometry can never flip the answer). Without the union
    /// a thumb can slide out from under a resting cursor, hover drops,
    /// the tween reverses, the thumb slides back — a jitter loop.
    /// Acquire (`wide` = false) stays at current geometry so the
    /// hover area doesn't swallow the screen.
    pub(super) fn zone_hit(&self, x: f64, y: f64, w: f64, h: f64, wide: bool) -> bool {
        if self.lightbox.is_some() || self.menu.is_some() {
            return true;
        }
        let pad = if wide { 28.0 } else { 12.0 };
        let fan_opts: &[f64] = if wide {
            &[self.fan_t, 0.0, 1.0][..]
        } else {
            &[self.fan_t][..]
        };
        let hide_opts: &[f64] = if wide {
            &[self.hide_t, 0.0, 1.0][..]
        } else {
            &[self.hide_t][..]
        };
        let n = self
            .shots
            .len()
            .min(if self.pinned { MAX_VIS_PINNED } else { MAX_VIS });
        for &ht in hide_opts {
            for &ft in fan_opts {
                for i in usize::from(self.native_flight)..n {
                    let r = self.thumb_rect_at(i, w, h, ft, ht);
                    if x >= r.x - pad
                        && x <= r.x + r.w + pad
                        && y >= r.y - pad
                        && y <= r.y + r.h + pad
                    {
                        return true;
                    }
                }
                if !self.shots.is_empty() {
                    let c = self.chip_rect_at(w, h, ht);
                    if x >= c.x - pad
                        && x <= c.x + c.w + pad
                        && y >= c.y - pad
                        && y <= c.y + c.h + pad
                    {
                        return true;
                    }
                }
            }
        }
        false
    }


    /// Point (logical) → role.
    pub fn hit(&self, x: f64, y: f64, w: f64, h: f64) -> Option<Hit> {
        if self.menu.is_some() {
            if let Some(i) = self.menu_row_at(x, y, w, h) {
                return Some(Hit::MenuRow(i));
            }
            if let Some(p) = self.menu_rect(w, h) {
                if x >= p.x && x <= p.x + p.w && y >= p.y && y <= p.y + p.h {
                    return Some(Hit::MenuPanel);
                }
            }
            return Some(Hit::Backdrop);
        }
        if self.lightbox.is_none() {
            // the × disc only exists while its thumb is hovered
            if let Some(Hit::Thumb(i) | Hit::Del(i)) = self.hover {
                let r = self.thumb_rect(i, w, h);
                let (dx, dy) = self.del_pos(r);
                if (x - dx).powi(2) + (y - dy).powi(2) <= 11.0 * 11.0 {
                    return Some(Hit::Del(i));
                }
            }
        }
        // lightbox action bar sits inside the backdrop — check its
        // buttons first so a click hits the action, not "close"
        if self.lightbox.is_some() {
            for (i, r) in self.box_action_rects(w, h).iter().enumerate() {
                if x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h {
                    return Some(Hit::BoxAction(i));
                }
            }
            return Some(Hit::Backdrop);
        }
        for (r, hit) in self.input_rects(w, h) {
            if x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h {
                return Some(hit);
            }
        }
        None
    }


    pub(super) fn del_pos(&self, r: FRect) -> (f64, f64) {
        if self.corner.bottom() {
            (r.x + r.w - 4.0, r.y - 4.0)
        } else {
            (r.x + r.w - 4.0, r.y + r.h + 4.0)
        }
    }


    /// Settled-position image rect the lightbox draws into — shared by
    /// the action bar layout and hit testing.
    pub(super) fn box_img_rect(&self, w: f64, h: f64) -> FRect {
        let fit = (w * 0.8 / self.box_iw as f64).min(h * 0.8 / self.box_ih as f64);
        let dw = self.box_iw as f64 * fit;
        let dh = self.box_ih as f64 * fit;
        FRect {
            x: (w - dw) / 2.0,
            y: (h - dh) / 2.0 - 10.0,
            w: dw,
            h: dh,
        }
    }


    /// One rect per BOX_ACTIONS entry, centred under the preview.
    pub(super) fn box_action_rects(&self, w: f64, h: f64) -> Vec<FRect> {
        let img = self.box_img_rect(w, h);
        let n = BOX_ACTIONS.len() as f64;
        let total = n * BOX_BTN_W + (n - 1.0) * BOX_BTN_GAP;
        let x0 = (w - total) / 2.0;
        let mut y = img.y + img.h + 18.0;
        if y + BOX_BTN_H > h - 12.0 {
            // tall images: dock the bar over the bottom margin instead
            y = img.y + img.h - BOX_BTN_H - 14.0;
        }
        BOX_ACTIONS
            .iter()
            .enumerate()
            .map(|(i, _)| FRect {
                x: x0 + i as f64 * (BOX_BTN_W + BOX_BTN_GAP),
                y,
                w: BOX_BTN_W,
                h: BOX_BTN_H,
            })
            .collect()
    }
}
