//! X11 presenter — paints and reshapes a foreign winit window over xcb.


use anyhow::{Context, Result};

use super::FRect;


use x11rb::connection::{Connection as XcbConn, RequestConnection};
use x11rb::protocol::{shape, xproto};
use x11rb::wrapper::ConnectionExt;

/// X11 presenter — a private xcb connection (any client may paint and
/// reshape a foreign window once it knows the XID). winit's
/// `with_transparent` already gave the window a 32-bit ARGB visual.
pub(crate) struct X11 {
    conn: x11rb::rust_connection::RustConnection,
    win: u32,
    gc: u32,
    depth: u8,
    /// staging for the R/B swap — our buf is premul RGBA, X11 ARGB32
    /// pixmaps want native-endian A<<24|R<<16|G<<8|B
    xbuf: Vec<u8>,
    /// put_image max request cap, in bytes of image data per call
    max_band: usize,
}

impl X11 {
    pub(crate) fn new(window: u32) -> Result<Self> {
        let (conn, _) = x11rb::connect(None).context("xcb connect")?;
        let depth = xproto::get_geometry(&conn, window)
            .context("xcb get_geometry")?
            .reply()
            .context("xcb get_geometry reply")?
            .depth;
        let gc = conn.generate_id().context("xcb gen_id")?;
        xproto::create_gc(&conn, gc, window, &xproto::CreateGCAux::new())
            .context("xcb create_gc")?;
        // a fullscreen album shouldn't sit in the taskbar or alt-tab —
        // winit has no X11 flag for this, so append the atoms ourselves
        let wm_state = atom(&conn, "_NET_WM_STATE");
        let skip_tb = atom(&conn, "_NET_WM_STATE_SKIP_TASKBAR");
        let skip_pager = atom(&conn, "_NET_WM_STATE_SKIP_PAGER");
        if wm_state != x11rb::NONE {
            let data: Vec<u32> = [skip_tb, skip_pager]
                .into_iter()
                .filter(|&a| a != x11rb::NONE)
                .collect();
            if !data.is_empty() {
                let _ = conn.change_property32(
                    xproto::PropMode::APPEND,
                    window,
                    wm_state,
                    xproto::AtomEnum::ATOM,
                    &data,
                );
            }
        }
        let _ = conn.flush();
        // PutImage can't exceed the (BigRequests-extended) maximum
        // request length — band the image if the cap kicks in
        let max_band = conn.maximum_request_bytes().saturating_sub(64);
        Ok(Self {
            conn,
            win: window,
            gc,
            depth,
            xbuf: Vec::new(),
            max_band,
        })
    }

    pub(crate) fn present(
        &mut self,
        buf: &[u32],
        pw: usize,
        ph: usize,
        rects: &[FRect],
        scale: f64,
        dirty: Option<FRect>,
    ) {
        // A<<24|B<<16|G<<8|R → A<<24|R<<16|G<<8|B, one pixel per 32-bit
        // unit regardless of window depth (ZPixmap pads 24bpp to 4B).
        // `dirty` (physical px) scopes conversion + upload to the band
        // that actually changed.
        let (x0, y0, x1, y1) = match dirty {
            Some(d) => (
                (d.x.floor() as usize).min(pw),
                (d.y.floor() as usize).min(ph),
                ((d.x + d.w).ceil() as usize).min(pw),
                ((d.y + d.h).ceil() as usize).min(ph),
            ),
            None => (0, 0, pw, ph),
        };
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let dw = x1 - x0;
        self.xbuf.resize(dw * (y1 - y0) * 4, 0);
        let mut i = 0;
        for row in buf[y0 * pw..y1 * pw].chunks_exact(pw) {
            for &px in &row[x0.min(row.len())..(x0 + dw).min(row.len())] {
                let p = px & 0xff00_ff00 | (px >> 16) & 0xff | (px << 16) & 0x00ff_0000;
                self.xbuf[i * 4..i * 4 + 4].copy_from_slice(&p.to_ne_bytes());
                i += 1;
            }
        }
        let rows_per_band = (self.max_band / (dw * 4).max(1)).max(1);
        let mut y = 0usize;
        let dh = y1 - y0;
        while y < dh {
            let rows = rows_per_band.min(dh - y);
            let off = y * dw * 4;
            let _ = xproto::put_image(
                &self.conn,
                xproto::ImageFormat::Z_PIXMAP,
                self.win,
                self.gc,
                dw as u16,
                rows as u16,
                x0 as i16,
                (y0 + y) as i16,
                0,
                self.depth,
                &self.xbuf[off..off + rows * dw * 4],
            );
            y += rows;
        }
        let rects: Vec<xproto::Rectangle> = rects
            .iter()
            .filter_map(|r| {
                // X windows are physical px — scale the logical zone
                let x0 = (r.x * scale).floor() as i32;
                let y0 = (r.y * scale).floor() as i32;
                let x1 = ((r.x + r.w) * scale).ceil() as i32;
                let y1 = ((r.y + r.h) * scale).ceil() as i32;
                let x0 = x0.clamp(0, i16::MAX as i32);
                let y0 = y0.clamp(0, i16::MAX as i32);
                let (w, h) = (x1 - x0, y1 - y0);
                (w > 0 && h > 0).then_some(xproto::Rectangle {
                    x: x0 as i16,
                    y: y0 as i16,
                    width: w.min(u16::MAX as i32) as u16,
                    height: h.min(u16::MAX as i32) as u16,
                })
            })
            .collect();
        // SO::SET + empty rects = click-through-everything; union of
        // zone rects = the hover hysteresis area stays live
        let _ = shape::rectangles(
            &self.conn,
            shape::SO::SET,
            shape::SK::INPUT,
            xproto::ClipOrdering::UNSORTED,
            self.win,
            0,
            0,
            &rects,
        );
        let _ = self.conn.flush();
    }
}

pub(crate) fn atom(conn: &impl RequestConnection, name: &str) -> u32 {
    xproto::intern_atom(conn, false, name.as_bytes())
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| r.atom)
        .unwrap_or(x11rb::NONE)
}
