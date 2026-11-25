use super::*;
use image::{Rgba, RgbaImage};

fn shot_img(w: u32, h: u32, rgb: [u8; 3]) -> RgbaImage {
    // gradient blocks so thumbnails read as distinct captures
    RgbaImage::from_fn(w, h, |x, y| {
        let t = ((x * 255 / w) as u8).wrapping_add((y * 255 / h) as u8);
        Rgba([
            rgb[0].saturating_add(t / 3),
            rgb[1],
            rgb[2].saturating_sub(t / 4),
            255,
        ])
    })
}

/// Land a shot directly (no native flight) at scale 2 — the shape
/// every stack test shares.
fn tland(st: &mut Stack, img: RgbaImage, kind: ShotKind, path: Option<PathBuf>, from: FRect) {
    st.land(
        Land {
            img,
            kind,
            path,
            sel: None,
            instant: false,
            dur_s: None,
            shared: None,
        },
        from,
        true,
        2.0,
    );
}

/// Renders a 3-shot stack + count chip to /tmp/qs-chrome.png — run
/// `cargo test -p qs-app chrome` then eyeball the PNG.
#[test]
fn render_stack() {
    let mut st = Stack::new(Corner::BR);
    for (kind, rgb) in [
        (ShotKind::Saved, [40, 60, 90]),
        (ShotKind::Shared, [90, 40, 60]),
        (ShotKind::Record, [50, 80, 50]),
    ] {
        tland(
            &mut st,
            shot_img(1200, 800, rgb),
            kind,
            None,
            FRect {
                x: 100.0,
                y: 100.0,
                w: 400.0,
                h: 300.0,
            },
        );
    }
    let (lw, lh) = (1440.0, 900.0);
    let mut buf = vec![0u32; (lw * 2.0) as usize * (lh * 2.0) as usize];
    {
        let mut cv = Canvas {
            buf: &mut buf,
            w: 2880,
            h: 1800,
            scale: 2.0,
        };
        let font = crate::settings::system_font().expect("font");
        st.render(&mut cv, &font, 0x52C7EA, Instant::now());
    }
    // premul ABGR-packed → plain RGBA8 for the PNG
    let img = RgbaImage::from_fn(2880, 1800, |x, y| {
        let p = buf[(y as usize) * 2880 + x as usize];
        let a = ((p >> 24) & 0xFF) as u32;
        let unprem = |c: u32| -> u8 {
            (c * 255).checked_div(a).map_or(0, |v| v.min(255)) as u8
        };
        Rgba([
            unprem(p & 0xFF),
            unprem((p >> 8) & 0xFF),
            unprem((p >> 16) & 0xFF),
            a as u8,
        ])
    });
    img.save("/tmp/qs-chrome.png").unwrap();

    // hover the newest thumb → fan-out + × + accent ring
    st.pointer(
        ChromeEv::Move {
            x: 1440.0 - 20.0 - TW / 2.0,
            y: 900.0 - 20.0 - TH / 2.0,
        },
        1440.0,
        900.0,
    );
    // step the fan tween to its end
    st.settle(Instant::now() + Duration::from_millis(400));
    let mut buf = vec![0u32; 2880 * 1800];
    let mut cv = Canvas {
        buf: &mut buf,
        w: 2880,
        h: 1800,
        scale: 2.0,
    };
    let font = crate::settings::system_font().expect("font");
    st.render(&mut cv, &font, 0x52C7EA, Instant::now());
    let img = RgbaImage::from_fn(2880, 1800, |x, y| {
        let p = buf[(y as usize) * 2880 + x as usize];
        let a = ((p >> 24) & 0xFF) as u32;
        let unprem = |c: u32| -> u8 {
            (c * 255).checked_div(a).map_or(0, |v| v.min(255)) as u8
        };
        Rgba([
            unprem(p & 0xFF),
            unprem((p >> 8) & 0xFF),
            unprem((p >> 16) & 0xFF),
            a as u8,
        ])
    });
    img.save("/tmp/qs-chrome-hover.png").unwrap();
}

/// Idle past HIDE_AFTER_MS slides the stack into the edge and the
/// cascade collapses — only the newest thumb's sliver shows.
#[test]
fn idle_hides_to_single_sliver() {
    let mut st = Stack::new(Corner::BR);
    for i in 0..3u8 {
        tland(
            &mut st,
            shot_img(1200, 800, [40 + i * 40, 60, 90]),
            ShotKind::Saved,
            None,
            FRect {
                x: 100.0,
                y: 100.0,
                w: 400.0,
                h: 300.0,
            },
        );
    }
    // wind the idle clock back past the threshold
    st.idle_since = Instant::now() - Duration::from_secs(10);
    st.settle(Instant::now());
    assert!(
        st.hide_anim,
        "idle past the deadline must start the hide tween"
    );
    st.settle(Instant::now() + Duration::from_millis(400));
    assert_eq!(st.hide_t, 1.0);

    // every thumb converged on the newest one's rect — one sliver
    let r0 = st.thumb_rect(0, 1440.0, 900.0);
    for i in 1..3 {
        let r = st.thumb_rect(i, 1440.0, 900.0);
        assert_eq!(r.x, r0.x, "thumb {i} must collapse behind the newest");
        assert_eq!(r.y, r0.y);
    }
    // the thumbs sit fully off the right edge; the chip docks as
    // the sole handle flush against it
    assert!(r0.x >= 1440.0, "thumb must be off-screen, x={}", r0.x);
    let c = st.chip_rect(1440.0, 900.0);
    assert!((c.x - (1440.0 - c.w - 2.0)).abs() < 1.0, "chip x={}", c.x);

    // activity brings it back
    st.touch();
    st.settle(Instant::now() + Duration::from_millis(400));
    assert_eq!(st.hide_t, 0.0);

    // mid-hide frame for eyeballing the slide path
    st.hide_t = 0.5;
    let mut buf = vec![0u32; 2880 * 1800];
    let mut cv = Canvas {
        buf: &mut buf,
        w: 2880,
        h: 1800,
        scale: 2.0,
    };
    let font = crate::settings::system_font().expect("font");
    st.render(&mut cv, &font, 0x52C7EA, Instant::now());
    let img = RgbaImage::from_fn(2880, 1800, |x, y| {
        let p = buf[(y as usize) * 2880 + x as usize];
        let a = ((p >> 24) & 0xFF) as u32;
        let unprem = |c: u32| -> u8 {
            (c * 255).checked_div(a).map_or(0, |v| v.min(255)) as u8
        };
        Rgba([
            unprem(p & 0xFF),
            unprem((p >> 8) & 0xFF),
            unprem((p >> 16) & 0xFF),
            a as u8,
        ])
    });
    img.save("/tmp/qs-chrome-midhide.png").unwrap();
}

/// A thumb parked fully past the RIGHT edge must not wrap into the
/// next buffer row — an unchecked `y*w + x` used to re-emit it on
/// the LEFT edge.
#[test]
fn hidden_thumbs_dont_wrap_to_left_edge() {
    let mut st = Stack::new(Corner::BR);
    tland(
        &mut st,
        shot_img(1200, 800, [90, 60, 40]),
        ShotKind::Saved,
        None,
        FRect {
            x: 100.0,
            y: 100.0,
            w: 400.0,
            h: 300.0,
        },
    );
    st.hide_t = 1.0; // fully slid off the right edge
    let (bw, bh) = (2880usize, 1800usize);
    let mut buf = vec![0u32; bw * bh];
    {
        let mut cv = Canvas {
            buf: &mut buf,
            w: bw,
            h: bh,
            scale: 2.0,
        };
        let font = crate::settings::system_font().expect("font");
        st.render(&mut cv, &font, 0x52C7EA, Instant::now());
    }
    // nothing but the docked chip may be opaque — and it lives at
    // the right edge, so the left half must be empty
    for y in 0..bh {
        for x in 0..bw / 2 {
            assert_eq!(
                buf[y * bw + x],
                0,
                "hidden thumb wrapped to left edge at phys ({x},{y})"
            );
        }
    }
}

/// The retention zone covers BOTH fan endpoints — a cursor resting
/// inside the cascade's overhang keeps `inside` after the fan opens
/// and slides the thumb away. Without it the hover flapped
/// open/closed every tween (the reported bottom-edge jitter).
#[test]
fn hover_survives_fan_sweep() {
    let mut st = Stack::new(Corner::TR);
    for i in 0..3u8 {
        tland(
            &mut st,
            shot_img(1200, 800, [40 + i * 40, 60, 90]),
            ShotKind::Saved,
            None,
            FRect {
                x: 100.0,
                y: 100.0,
                w: 400.0,
                h: 300.0,
            },
        );
    }
    // cursor parked inside the collapsed cascade's low edge — a spot
    // the fanned row vacates (cascade dy collapses on open)
    let r2 = st.thumb_rect_at(2, 1440.0, 900.0, 0.0, 0.0);
    let (cx, cy) = (r2.x + r2.w / 2.0, r2.y + r2.h + 10.0); // just below
    let _ = st.pointer(ChromeEv::Move { x: cx, y: cy }, 1440.0, 900.0);
    assert!(
        st.inside,
        "cursor on the cascade edge should engage the zone"
    );
    // run the fan open to its end — the cascade's dy is gone now
    st.settle(Instant::now() + Duration::from_millis(400));
    assert_eq!(st.fan_t, 1.0);
    // same resting cursor: still inside via the union, no flap
    let _ = st.pointer(ChromeEv::Move { x: cx, y: cy }, 1440.0, 900.0);
    assert!(st.inside, "fan sweep must not drop the hover zone");
    assert_eq!(st.fan_to, 1.0, "fan must stay open under a resting cursor");
}

/// Click thumb0 → lightbox opens; backdrop click closes it.
#[test]
fn thumb_click_opens_editor() {
    let mut st = Stack::new(Corner::TR);
    tland(
        &mut st,
        shot_img(1200, 800, [40, 60, 90]),
        ShotKind::Saved,
        None,
        FRect {
            x: 100.0,
            y: 100.0,
            w: 400.0,
            h: 300.0,
        },
    );
    let r = st.thumb_rect(0, 1440.0, 900.0);
    // a click resolves on release — press alone must not open the
    // lightbox, or drag-out could never preempt it
    let cmd = st.pointer(
        ChromeEv::Down {
            x: r.x + r.w / 2.0,
            y: r.y + r.h / 2.0,
        },
        1440.0,
        900.0,
    );
    assert!(matches!(cmd, Some(ChromeCmd::Redraw)));
    assert!(st.lightbox.is_none());
    // the release is the click: straight to the editor, no lightbox
    let cmd = st.pointer(
        ChromeEv::Up {
            x: r.x + r.w / 2.0,
            y: r.y + r.h / 2.0,
        },
        1440.0,
        900.0,
    );
    assert!(matches!(
        cmd,
        Some(ChromeCmd::MenuAction {
            idx: 0,
            id: MENU_OPEN
        })
    ));
    assert!(st.lightbox.is_none());
    // the preview is still one call away, and a backdrop click closes it
    st.open_lightbox(0);
    assert!(st.lightbox.is_some());
    st.pointer(ChromeEv::Down { x: 700.0, y: 450.0 }, 1440.0, 900.0);
    assert!(st.lightbox.is_none());
}

/// O / S in the lightbox route to the same actions as the bar buttons.
#[test]
fn lightbox_keys_open_and_share() {
    let mut st = Stack::new(Corner::TR);
    tland(
        &mut st,
        shot_img(1200, 800, [40, 60, 90]),
        ShotKind::Saved,
        None,
        FRect {
            x: 100.0,
            y: 100.0,
            w: 400.0,
            h: 300.0,
        },
    );
    // thumb clicks open the editor now; the preview comes from Space
    st.open_lightbox(0);
    assert!(st.lightbox.is_some());
    assert!(matches!(
        st.key(Key::Open),
        Some(ChromeCmd::MenuAction {
            idx: 0,
            id: MENU_OPEN
        })
    ));
    assert!(matches!(
        st.key(Key::Share),
        Some(ChromeCmd::MenuAction {
            idx: 0,
            id: MENU_SHARE
        })
    ));
    // outside a preview, O/S have no shot to act on
    st.lightbox = None;
    assert!(st.key(Key::Open).is_none());
}

/// A thumb press that drifts ≥4px before release becomes a drag —
/// `DragBegin` fires and no lightbox opens; a sub-threshold wobble
/// stays a click.
#[test]
fn thumb_drag_emits_drag_begin_not_lightbox() {
    let mut st = Stack::new(Corner::TR);
    tland(
        &mut st,
        shot_img(1200, 800, [40, 60, 90]),
        ShotKind::Saved,
        None,
        FRect {
            x: 100.0,
            y: 100.0,
            w: 400.0,
            h: 300.0,
        },
    );
    let r = st.thumb_rect(0, 1440.0, 900.0);
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    st.pointer(ChromeEv::Down { x: cx, y: cy }, 1440.0, 900.0);
    let cmd = st.pointer(
        ChromeEv::Dragged {
            x: cx + 12.0,
            y: cy,
        },
        1440.0,
        900.0,
    );
    assert!(matches!(cmd, Some(ChromeCmd::DragBegin { idx: 0 })));
    assert!(st.lightbox.is_none());
    // release must not resurrect the click
    st.pointer(
        ChromeEv::Up {
            x: cx + 12.0,
            y: cy,
        },
        1440.0,
        900.0,
    );
    assert!(st.lightbox.is_none());
    // a sub-threshold wobble stays a click (→ editor)
    st.pointer(ChromeEv::Down { x: cx, y: cy }, 1440.0, 900.0);
    st.pointer(ChromeEv::Dragged { x: cx + 2.0, y: cy }, 1440.0, 900.0);
    let cmd = st.pointer(ChromeEv::Up { x: cx + 2.0, y: cy }, 1440.0, 900.0);
    assert!(matches!(
        cmd,
        Some(ChromeCmd::MenuAction {
            idx: 0,
            id: MENU_OPEN
        })
    ));
}

/// The "CLEAR ALL" pill rides the far end of the fanned row: it only
/// exists once the fan is open with ≥2 shots, clicking it emits
/// ChromeCmd::ClearAll, and clear_all() empties every bit of state.
#[test]
fn clear_all_pill_empties_stack() {
    let (lw, lh) = (1440.0, 900.0);
    let mut st = Stack::new(Corner::TR);
    for i in 0..3u8 {
        tland(
            &mut st,
            shot_img(1200, 800, [40 + i * 40, 60, 90]),
            ShotKind::Saved,
            None,
            FRect {
                x: 100.0,
                y: 100.0,
                w: 400.0,
                h: 300.0,
            },
        );
    }
    // no pill while the stack sits collapsed
    assert!(st.clear_rect(lw, lh).is_none());
    // hover → fan open to the end
    let r0 = st.thumb_rect_at(0, lw, lh, 0.0, 0.0);
    let _ = st.pointer(
        ChromeEv::Move {
            x: r0.x + r0.w / 2.0,
            y: r0.y + r0.h / 2.0,
        },
        lw,
        lh,
    );
    st.settle(Instant::now() + Duration::from_millis(400));
    assert_eq!(st.fan_t, 1.0);

    // the pill sits centred under the fanned row
    let cr = st.clear_rect(lw, lh).expect("pill exists when fanned");
    let row = st.thumb_rect_at(0, lw, lh, 1.0, 0.0);
    assert!(cr.y >= row.y + row.h, "pill must sit below the row");
    assert!(cr.x + cr.w / 2.0 > row.x - 400.0 && cr.x + cr.w / 2.0 < row.x + row.w + 1.0);
    let cmd = st.pointer(
        ChromeEv::Down {
            x: cr.x + cr.w / 2.0,
            y: cr.y + cr.h / 2.0,
        },
        lw,
        lh,
    );
    assert!(matches!(cmd, Some(ChromeCmd::ClearAll)));

    // clearing empties everything — shots, hover, lightbox, pinned
    st.clear_all();
    assert!(st.is_empty());
    assert!(st.shots.is_empty());
    assert!(st.clear_rect(lw, lh).is_none());
    assert!(st.input_rects(lw, lh).is_empty());

    // also clears a lightbox/pinned state mid-flight
    let mut st = Stack::new(Corner::TR);
    for _ in 0..2 {
        tland(
            &mut st,
            shot_img(1200, 800, [40, 60, 90]),
            ShotKind::Saved,
            None,
            FRect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
        );
    }
    st.pinned = true;
    st.lightbox = Some(0);
    st.clear_all();
    assert!(st.is_empty());
    assert!(st.lightbox.is_none());
    assert!(!st.pinned);
}

/// Right-click opens the in-chrome menu; a row click emits the row's
/// MenuAction; Esc/backdrop dismiss without acting.
#[test]
fn context_menu_lifecycle() {
    let (lw, lh) = (1440.0, 900.0);
    let mut st = Stack::new(Corner::TR);
    for i in 0..2u8 {
        tland(
            &mut st,
            shot_img(1200, 800, [40 + i * 40, 60, 90]),
            ShotKind::Saved,
            Some(PathBuf::from(format!("/tmp/shot{i}.png"))),
            FRect {
                x: 100.0,
                y: 100.0,
                w: 400.0,
                h: 300.0,
            },
        );
    }
    let r = st.thumb_rect(0, lw, lh);
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);

    // right-click on the newest thumb opens the menu — and widens
    // the input region to the whole screen for the dismiss-outside
    st.pointer(ChromeEv::RightDown { x: cx, y: cy }, lw, lh);
    assert!(st.menu.is_some());
    assert_eq!(
        st.zone_rects(lw, lh).len(),
        1,
        "menu should cover the screen"
    );
    assert!(matches!(st.hit(5.0, 5.0, lw, lh), Some(Hit::Backdrop)));

    // a row click emits the row's action and closes the menu
    let m = st.menu.as_ref().unwrap();
    let copy_row = m.rows.iter().position(|(id, _)| *id == MENU_COPY).unwrap();
    let rr = st.menu_row_rect(copy_row, lw, lh).unwrap();
    let cmd = st.pointer(
        ChromeEv::Down {
            x: rr.x + rr.w / 2.0,
            y: rr.y + rr.h / 2.0,
        },
        lw,
        lh,
    );
    assert!(
        matches!(
            cmd,
            Some(ChromeCmd::MenuAction {
                idx: 0,
                id: MENU_COPY
            })
        ),
        "row click must emit MenuAction, got {cmd:?}"
    );
    assert!(st.menu.is_none());

    // reopen, then Esc peels the menu off without touching the stack
    st.pointer(ChromeEv::RightDown { x: cx, y: cy }, lw, lh);
    assert!(st.menu.is_some());
    let cmd = st.key(Key::Esc);
    assert!(matches!(cmd, Some(ChromeCmd::Redraw)));
    assert!(st.menu.is_none());
    assert_eq!(st.shots.len(), 2);

    // a disabled row never emits — Reveal needs a saved path
    let mut st = Stack::new(Corner::TR);
    tland(
        &mut st,
        shot_img(1200, 800, [40, 60, 90]),
        ShotKind::Copied,
        None,
        FRect {
            x: 100.0,
            y: 100.0,
            w: 400.0,
            h: 300.0,
        },
    );
    let r = st.thumb_rect(0, lw, lh);
    st.pointer(
        ChromeEv::RightDown {
            x: r.x + r.w / 2.0,
            y: r.y + r.h / 2.0,
        },
        lw,
        lh,
    );
    let m = st.menu.as_ref().unwrap();
    let reveal_row = m
        .rows
        .iter()
        .position(|(id, _)| *id == MENU_REVEAL)
        .unwrap();
    assert!(
        !m.rows[reveal_row].1,
        "Reveal must be disabled without a path"
    );
    let rr = st.menu_row_rect(reveal_row, lw, lh).unwrap();
    let cmd = st.pointer(
        ChromeEv::Down {
            x: rr.x + rr.w / 2.0,
            y: rr.y + rr.h / 2.0,
        },
        lw,
        lh,
    );
    assert!(
        matches!(cmd, Some(ChromeCmd::Redraw)),
        "disabled row must not act"
    );
    assert!(st.menu.is_some(), "disabled row leaves the menu open");
}
