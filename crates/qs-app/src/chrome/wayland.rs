//! Wayland presenter — wlr-layer-shell corner shelf via a private
//! `wl_shm`/`zwlr_layer_surface` connection; see `linux.rs` module docs.

use std::ffi::c_void;

use anyhow::{Context, Result};

use super::{Corner, FRect, Key};


use std::os::fd::AsFd;
use std::os::unix::io::{AsRawFd, BorrowedFd, RawFd};

use wayland_client::protocol::{
    wl_buffer::{self, WlBuffer},
    wl_compositor::{self, WlCompositor},
    wl_keyboard::{self, WlKeyboard},
    wl_output::{self, WlOutput},
    wl_pointer::{self, WlPointer},
    wl_region::{self, WlRegion},
    wl_registry::{self, WlRegistry},
    wl_seat::{self, Capability, WlSeat},
    wl_shm::{self, WlShm},
    wl_shm_pool::{self, WlShmPool},
    wl_surface::{self, WlSurface},
};
use wayland_client::{Connection as WlConn, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};
use wayland_sys::client::{wayland_client_handle, wl_proxy};

/// Pointer events from a zwlr_layer_surface (winit doesn't route these).
pub(crate) enum WlChromeInput {
    Move { x: f64, y: f64 },
    Leave,
    Down { x: f64, y: f64 },
    Up { x: f64, y: f64 },
    RightDown { x: f64, y: f64 },
    Key(Key),
}

/// Linux evdev scancode (wl keycode = evdev + 8).
pub(crate) fn wl_keycode(code: u32) -> Option<Key> {
    match code {
        9 => Some(Key::Esc),
        22 => Some(Key::Back),
        36 => Some(Key::Enter),
        113 => Some(Key::Left),
        114 => Some(Key::Right),
        65 => Some(Key::Space),
        _ => None,
    }
}

/// wl_surface request opcodes (the core protocol is wire-stable).
const SURFACE_ATTACH: u32 = 1;
const SURFACE_DAMAGE: u32 = 2;
const SURFACE_SET_INPUT_REGION: u32 = 5;
const SURFACE_COMMIT: u32 = 6;
/// damage_buffer arrived at v4; winit binds surfaces at v4+ so buffer
/// coordinates are safe — but we still gate on the proxy version.
const SURFACE_DAMAGE_BUFFER: u32 = 9;

/// Requests destined for winit's wl_surface are marshalled raw: the
/// proxy lives on winit's event queue and adopting it via
/// `Backend::manage_object` would clobber winit's dispatch state. Our
/// own objects (shm buffers, the input region) ride on a private event
/// queue on the same wl_display — marshalling *to* a foreign proxy is
/// safe; only receiving events for it isn't.
///
/// The calls go through `wl_proxy_marshal` via the dlopen handle (winit
/// already loads libwayland-client for the window itself).
pub(crate) struct Wl {
    surface: *mut wl_proxy,
    conn: WlConn,
    queue: EventQueue<WlState>,
    state: WlState,
    /// Desktop overlay — not winit's xdg-toplevel surface.
    layer_mode: bool,
    /// Layer surface configured — safe to attach buffers.
    live: bool,
}

pub(crate) fn layer_anchor_full() -> Anchor {
    Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right
}

pub(crate) fn layer_anchor(corner: Corner) -> Anchor {
    let mut a = Anchor::empty();
    match corner {
        Corner::TL | Corner::TC | Corner::TR => a |= Anchor::Top,
        Corner::BL | Corner::BC | Corner::BR => a |= Anchor::Bottom,
    }
    match corner {
        Corner::TL | Corner::BL => a |= Anchor::Left,
        Corner::TR | Corner::BR => a |= Anchor::Right,
        _ => {}
    }
    a
}

pub(crate) fn layer_margins(corner: Corner) -> (i32, i32, i32, i32) {
    let m = 20;
    match corner {
        Corner::TR => (m, m, 0, 0),
        Corner::TL => (m, 0, 0, m),
        Corner::BR => (0, m, m, 0),
        Corner::BL => (0, 0, m, m),
        Corner::TC => (m, 0, 0, 0),
        Corner::BC => (0, 0, m, 0),
    }
}

/// One pooled shm buffer. `busy` flips off on wl_buffer.release — the
/// compositor is done reading and the slot can be rewritten. `dead`
/// marks a stale-size slot slated for teardown once released.
pub(crate) struct WlBuf {
    fd: RawFd,
    size: usize,
    w: i32,
    h: i32,
    pool: WlShmPool,
    buf: WlBuffer,
    /// the pixel conversion the buffer's format demands
    argb: bool,
    busy: bool,
    dead: bool,
    /// Physical-px rows dirtied since this slot was last written —
    /// slots alternate between frames, so a partial write must cover
    /// the catch-up union or the stale bands ghost.
    catch_up: Option<FRect>,
}

/// wayland-client bookkeeping state bound to our private event queue —
/// `WlBuffer::Release` is the only event we act on; every other object
/// is bound with a `()` udata and a noop dispatch.
#[derive(Default)]
pub(crate) struct WlState {
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    layer_shell: Option<ZwlrLayerShellV1>,
    layer_surface: Option<ZwlrLayerSurfaceV1>,
    chrome_surface: Option<WlSurface>,
    outputs: Vec<(WlOutput, String)>,
    seat: Option<WlSeat>,
    pointer: Option<WlPointer>,
    keyboard: Option<WlKeyboard>,
    /// formats the compositor advertised on wl_shm — argb8888/xrgb8888
    /// are the only guaranteed ones; abgr8888 (our native packing) is
    /// optional and weston refuses create_buffer with it
    shm_formats: Vec<wl_shm::Format>,
    bufs: Vec<Option<WlBuf>>,
    /// slot indices released by the server since the last dispatch
    released: Vec<usize>,
    inputs: Vec<WlChromeInput>,
    pointer_scale: f64,
    pointer_x: f64,
    pointer_y: f64,
    layer_configured: bool,
    /// registry globals seen (debug when layer-shell is missing)
    wl_globals: Vec<String>,
}

impl Wl {
    pub(crate) fn layer_mode(&self) -> bool {
        self.layer_mode
    }

    pub(crate) fn is_live(&self) -> bool {
        !self.layer_mode || self.live
    }

    pub(crate) fn take_inputs(&mut self) -> Vec<WlChromeInput> {
        std::mem::take(&mut self.state.inputs)
    }

    pub(crate) fn set_layer_corner(&mut self, corner: Corner) {
        if !self.layer_mode {
            return;
        }
        if let Some(layer) = &self.state.layer_surface {
            let _ = corner;
            layer.set_anchor(layer_anchor_full());
            layer.set_margin(0, 0, 0, 0);
            let _ = self.conn.flush();
        }
    }

    pub(crate) fn sync_layer_keyboard(&mut self, modal: bool) {
        if !self.layer_mode {
            return;
        }
        if let Some(layer) = &self.state.layer_surface {
            let ki = if modal {
                KeyboardInteractivity::Exclusive
            } else {
                KeyboardInteractivity::OnDemand
            };
            layer.set_keyboard_interactivity(ki);
            let _ = self.conn.flush();
        }
    }

    /// Present on winit's wl_surface (corner shelf window) when layer-shell
    /// isn't offered — no roundtrip (winit already owns the registry).
    pub(crate) fn new_foreign(display: *mut c_void, surface: *mut c_void) -> Result<Self> {
        let backend =
            unsafe { wayland_backend::client::Backend::from_foreign_display(display.cast()) };
        let conn = WlConn::from_backend(backend);
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        let registry = conn.display().get_registry(&qh, ());
        let mut state = WlState::default();
        queue
            .roundtrip(&mut state)
            .context("foreign wl registry")?;
        drop(registry);
        if state.compositor.is_none() || state.shm.is_none() {
            anyhow::bail!("foreign wl missing compositor/shm");
        }
        log::info!("chrome: winit shelf present (no layer-shell)");
        Ok(Self {
            surface: surface as *mut wl_proxy,
            conn,
            queue,
            state,
            layer_mode: false,
            live: true,
        })
    }

    pub(crate) fn new_layer_foreign(
        display: *mut c_void,
        corner: Corner,
        lw: f64,
        lh: f64,
        scale: f64,
        output_name: Option<&str>,
    ) -> Result<Self> {
        let backend =
            unsafe { wayland_backend::client::Backend::from_foreign_display(display.cast()) };
        let conn = WlConn::from_backend(backend);
        Self::finish_layer(conn, corner, lw, lh, scale, output_name)
    }

    /// zwlr_layer_shell on a **separate** socket — try when foreign display
    /// doesn't advertise the global (some compositors are picky).
    pub(crate) fn new_layer(
        corner: Corner,
        lw: f64,
        lh: f64,
        scale: f64,
        output_name: Option<&str>,
    ) -> Result<Self> {
        let conn = WlConn::connect_to_env().context("WAYLAND_DISPLAY for album layer")?;
        Self::finish_layer(conn, corner, lw, lh, scale, output_name)
    }

    pub(crate) fn finish_layer(
        conn: WlConn,
        _corner: Corner,
        lw: f64,
        lh: f64,
        scale: f64,
        output_name: Option<&str>,
    ) -> Result<Self> {
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        let registry = conn.display().get_registry(&qh, ());
        let mut state = WlState {
            pointer_scale: scale,
            ..WlState::default()
        };
        queue
            .roundtrip(&mut state)
            .context("layer wl registry")?;
        let _ = queue.roundtrip(&mut state);
        drop(registry);
        let compositor = state
            .compositor
            .as_ref()
            .context("layer: no wl_compositor")?;
        let layer_shell = state.layer_shell.as_ref().with_context(|| {
            format!(
                "layer: no zwlr_layer_shell_v1 (compositor={}, shm={}, outputs={}, globals=[{}])",
                state.compositor.is_some(),
                state.shm.is_some(),
                state.outputs.len(),
                state.wl_globals.join(", ")
            )
        })?;
        let output = output_name.and_then(|want| {
            state
                .outputs
                .iter()
                .find(|(_, n)| n == want)
                .map(|(o, _)| o)
        });
        if output_name.is_some() && output.is_none() {
            log::warn!(
                "chrome: wl_output {:?} not found — anchoring layer to default output",
                output_name
            );
        }
        let surface = compositor.create_surface(&qh, ());
        let layer = layer_shell.get_layer_surface(
            &surface,
            output,
            Layer::Overlay,
            "quickshort-album".into(),
            &qh,
            (),
        );
        // Full output like macOS — stack layout picks the dock corner.
        layer.set_anchor(layer_anchor_full());
        layer.set_margin(0, 0, 0, 0);
        layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
        let pw = (lw * scale).round().max(1.0) as u32;
        let ph = (lh * scale).round().max(1.0) as u32;
        layer.set_size(pw, ph);
        state.layer_surface = Some(layer);
        state.chrome_surface = Some(surface);
        if let Some(seat) = state.seat.as_ref() {
            seat.get_pointer(&qh, ());
            seat.get_keyboard(&qh, ());
        }
        queue
            .roundtrip(&mut state)
            .context("layer surface configure")?;
        if !state.layer_configured {
            anyhow::bail!("layer surface never configured");
        }
        let surface_ptr = state
            .chrome_surface
            .as_ref()
            .context("layer surface missing")?
            .id()
            .as_ptr() as *mut wl_proxy;
        log::info!(
            "chrome: layer-shell album docked ({}×{} logical)",
            lw, lh
        );
        Ok(Self {
            surface: surface_ptr,
            conn,
            queue,
            state,
            layer_mode: true,
            live: true,
        })
    }

    /// Non-blocking read + dispatch for our private Wayland connection.
    pub(crate) fn pump_connection(&mut self) {
        let _ = self.conn.flush();
        loop {
            if self.queue.dispatch_pending(&mut self.state).unwrap_or(0) == 0 {
                break;
            }
        }
        if let Some(guard) = self.conn.prepare_read() {
            let fd = {
                let backend = self.conn.backend();
                backend.poll_fd().as_fd().as_raw_fd()
            };
            let mut p = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            if unsafe { libc::poll(&mut p, 1, 0) } > 0 && p.revents & libc::POLLIN != 0 {
                let _ = guard.read();
            }
        }
        loop {
            if self.queue.dispatch_pending(&mut self.state).unwrap_or(0) == 0 {
                break;
            }
        }
    }

    /// Drain buffered events — marks released buffers reusable. Called
    /// from `pump` because winit owns the display fd's read path.
    pub(crate) fn dispatch(&mut self) {
        if self.layer_mode {
            self.pump_connection();
        } else if self.queue.dispatch_pending(&mut self.state).is_err() {
            return;
        }
        for i in self.state.released.drain(..) {
            if let Some(Some(b)) = self.state.bufs.get_mut(i) {
                b.busy = false;
            }
        }
        // dead slots go once the server has let go of them — slot
        // indices are wl_buffer udata so slots stay put (`None` until
        // swept) and a late release just finds its slot gone
        let mut i = 0;
        while i < self.state.bufs.len() {
            match &self.state.bufs[i] {
                Some(b) if b.dead && !b.busy => self.state.bufs[i] = None,
                // interior holes get reused by `slot`; trailing holes
                // are swept so the vec can't grow unboundedly
                None if i == self.state.bufs.len() - 1 => {
                    self.state.bufs.truncate(i);
                }
                _ => i += 1,
            }
        }
    }

    /// Reuse or create a pooled slot holding `w×h` pixels; the map +
    /// copy happens here, so attach can run immediately after.
    pub(crate) fn slot(
        &mut self,
        qh: &QueueHandle<WlState>,
        pixels: &[u32],
        w: i32,
        h: i32,
        dirty: Option<FRect>,
    ) -> Option<usize> {
        let need = (w * h) as usize * 4;
        let free = self.state.bufs.iter().position(|b| {
            b.as_ref().map_or(true, |b| {
                !b.busy && !b.dead && b.size == need && b.w == w && b.h == h
            })
        });
        let idx = match free {
            Some(i) => i,
            None => {
                self.state.bufs.push(None);
                self.state.bufs.len() - 1
            }
        };
        let created = self.state.bufs[idx].is_none();
        if created {
            // wrong-size survivors become dead weight — `dispatch` frees
            // each once its release arrives
            for b in self.state.bufs.iter_mut().flatten() {
                if b.size != need || b.w != w || b.h != h {
                    b.dead = true;
                }
            }
            let fd = shm_fd(need)?;
            let pool = self.state.shm.as_ref()?.create_pool(
                unsafe { BorrowedFd::borrow_raw(fd) },
                need as i32,
                qh,
                (),
            );
            // only argb8888/xrgb8888 are spec-guaranteed; our buffer is
            // packed ABGR so that format wins when offered, else we pay
            // an R↔B swap for ARGB
            let fmt = if self.state.shm_formats.contains(&wl_shm::Format::Abgr8888) {
                wl_shm::Format::Abgr8888
            } else {
                wl_shm::Format::Argb8888
            };
            let buf = pool.create_buffer(0, w, h, w * 4, fmt, qh, idx);
            self.state.bufs[idx] = Some(WlBuf {
                fd,
                size: need,
                w,
                h,
                pool,
                buf,
                argb: fmt == wl_shm::Format::Argb8888,
                busy: false,
                dead: false,
                catch_up: None,
            });
        }
        // `dirty` is the physical-px region the caller repainted. This
        // slot's pixels are from whenever it was last written (usually
        // two frames back, slots alternate), so a partial write must
        // cover the union of every dirty band missed since then —
        // tracked in `catch_up`. Just-created slots always full-write
        // (their shm is uninitialized).
        {
            let b = self.state.bufs[idx].as_mut()?;
            match (dirty, b.catch_up.take(), created) {
                (Some(d), catch_up, false) => {
                    let r = catch_up.map_or(d, |c| {
                        let x0 = c.x.min(d.x);
                        let y0 = c.y.min(d.y);
                        FRect {
                            x: x0,
                            y: y0,
                            w: (c.x + c.w).max(d.x + d.w) - x0,
                            h: (c.y + c.h).max(d.y + d.h) - y0,
                        }
                    });
                    let y0 = (r.y.floor() as i32).clamp(0, h) as usize;
                    let y1 = ((r.y + r.h).ceil() as i32).clamp(0, h) as usize;
                    if y1 > y0 {
                        write_shm_rows(b.fd, pixels, b.size, w as usize, y0, y1, b.argb)?;
                    }
                }
                _ => write_shm(b.fd, pixels, b.size, b.argb)?,
            }
            b.busy = true;
        }
        // bands this slot skipped become the siblings' catch-up — each
        // writes them when its turn comes
        for (i, other) in self.state.bufs.iter_mut().enumerate() {
            if i != idx {
                if let Some(o) = other {
                    o.catch_up = match (o.catch_up, dirty) {
                        (Some(c), Some(d)) => Some({
                            let x0 = c.x.min(d.x);
                            let y0 = c.y.min(d.y);
                            FRect {
                                x: x0,
                                y: y0,
                                w: (c.x + c.w).max(d.x + d.w) - x0,
                                h: (c.y + c.h).max(d.y + d.h) - y0,
                            }
                        }),
                        (c, d) => c.or(d),
                    };
                }
            }
        }
        Some(idx)
    }

    pub(crate) fn present(
        &mut self,
        buf: &[u32],
        pw: usize,
        ph: usize,
        rects: &[FRect],
        dirty: Option<FRect>,
    ) {
        if self.layer_mode && (!self.live || self.surface.is_null()) {
            return;
        }
        let (w, h) = (pw as i32, ph as i32);
        let qh = self.queue.handle();
        let Some(idx) = self.slot(&qh, buf, w, h, dirty) else {
            return; // shm setup failed — leave the window transparent
        };
        let bufp = self.state.bufs[idx].as_ref().map(|b| b.buf.id().as_ptr());
        let Some(bufp) = bufp else { return };

        // fresh wl_region per present — the server copies it into the
        // pending input region, so a one-shot object avoids mutating
        // shared state across commits and the giant-subtract trick
        // (±2³⁰ rects) some compositors choke on. Destroy after the
        // request is queued is safe: regions are copied, not referenced.
        if let Some(comp) = &self.state.compositor {
            let region = comp.create_region(&qh, ());
            for r in rects {
                let x0 = r.x.floor() as i32;
                let y0 = r.y.floor() as i32;
                let rw = (r.x + r.w).ceil() as i32 - x0;
                let rh = (r.y + r.h).ceil() as i32 - y0;
                if rw > 0 && rh > 0 {
                    region.add(x0, y0, rw, rh);
                }
            }
            unsafe {
                (wayland_client_handle().wl_proxy_marshal)(
                    self.surface,
                    SURFACE_SET_INPUT_REGION,
                    region.id().as_ptr(),
                );
            }
            region.destroy();
        }
        unsafe {
            let wl = wayland_client_handle();
            // damage_buffer (v4) scopes the repaint to the painted band;
            // fall back to full-surface damage on v3 surfaces — the shm
            // rows beyond the written band are stale either way and the
            // damage hint only costs compositor work, never correctness,
            // because slot() caught up every missed dirty band
            let ver = (wl.wl_proxy_get_version)(self.surface);
            match (ver >= 4, dirty) {
                (true, Some(d)) => {
                    let x0 = d.x.floor() as i32;
                    let y0 = d.y.floor() as i32;
                    let dw = (d.x + d.w).ceil() as i32 - x0;
                    let dh = (d.y + d.h).ceil() as i32 - y0;
                    (wl.wl_proxy_marshal)(
                        self.surface,
                        SURFACE_DAMAGE_BUFFER,
                        x0,
                        y0,
                        dw.max(1),
                        dh.max(1),
                    );
                }
                _ => {
                    (wl.wl_proxy_marshal)(self.surface, SURFACE_DAMAGE, 0i32, 0i32, w, h);
                }
            }
            (wl.wl_proxy_marshal)(self.surface, SURFACE_ATTACH, bufp, 0i32, 0i32);
            (wl.wl_proxy_marshal)(self.surface, SURFACE_COMMIT);
        }
        let _ = self.conn.flush();
    }
}

impl Drop for Wl {
    fn drop(&mut self) {
        // explicit destroys must run while the connection is still up
        // (winit disconnects the display after `exiting`, so Host/Demo
        // clear chrome.wins there); region objects are one-shot now
        for b in self.state.bufs.drain(..).flatten() {
            drop(b);
        }
    }
}

impl Drop for WlBuf {
    fn drop(&mut self) {
        self.buf.destroy();
        self.pool.destroy();
        if self.fd >= 0 {
            unsafe { libc::close(self.fd) };
        }
    }
}

/// memfd first (no unlink dance), `shm_open` for pre-3.17 kernels.
pub(crate) fn shm_fd(size: usize) -> Option<RawFd> {
    unsafe {
        let name = b"qs-chrome\0";
        let mut fd = libc::memfd_create(name.as_ptr().cast(), libc::MFD_CLOEXEC);
        if fd < 0 {
            static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path =
                std::ffi::CString::new(format!("/qs-chrome-{}-{n}", std::process::id())).ok()?;
            fd = libc::shm_open(
                path.as_ptr(),
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
                0o600,
            );
            if fd >= 0 {
                libc::shm_unlink(path.as_ptr()); // stays valid until close
            }
        }
        if fd < 0 || libc::ftruncate(fd, size as libc::off_t) != 0 {
            if fd >= 0 {
                libc::close(fd);
            }
            return None;
        }
        Some(fd)
    }
}

/// Write pixels into the shm fd. `argb` selects the channel order:
/// our buffer is ABGR (A<<24|B<<16|G<<8|R = memory R,G,B,A) while
/// wl_shm Argb8888 wants A<<24|R<<16|G<<8|B — swap R↔B per pixel.
pub(crate) fn write_shm(fd: RawFd, pixels: &[u32], size: usize, argb: bool) -> Option<()> {
    unsafe {
        let p = libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        );
        if p == libc::MAP_FAILED {
            return None;
        }
        let dst = p.cast::<u32>();
        if argb {
            for (i, &px) in pixels.iter().enumerate() {
                let v = px & 0xff00_ff00 | (px >> 16) & 0xff | (px << 16) & 0x00ff_0000;
                dst.add(i).write_unaligned(v);
            }
        } else {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), dst.cast(), pixels.len());
        }
        libc::munmap(p, size);
    }
    Some(())
}

/// Row-banded variant: only rows `[y0, y1)` of a `w`-wide buffer are
/// written — the rest of the shm keeps the previous frame's pixels.
/// Animation frames dirty a corner, not the whole monitor, so this
/// skips most of the shm memcpy that made wl presents expensive.
pub(crate) fn write_shm_rows(
    fd: RawFd,
    pixels: &[u32],
    size: usize,
    w: usize,
    y0: usize,
    y1: usize,
    argb: bool,
) -> Option<()> {
    unsafe {
        let p = libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        );
        if p == libc::MAP_FAILED {
            return None;
        }
        let dst = p.cast::<u32>();
        for y in y0..y1 {
            let row = &pixels[y * w..(y + 1) * w];
            let d = dst.add(y * w);
            if argb {
                for (i, &px) in row.iter().enumerate() {
                    let v = px & 0xff00_ff00 | (px >> 16) & 0xff | (px << 16) & 0x00ff_0000;
                    d.add(i).write_unaligned(v);
                }
            } else {
                std::ptr::copy_nonoverlapping(row.as_ptr(), d.cast(), row.len());
            }
        }
        libc::munmap(p, size);
    }
    Some(())
}

impl Dispatch<WlSurface, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &WlConn,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<ZwlrLayerShellV1, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &ZwlrLayerShellV1,
        _: zwlr_layer_shell_v1::Event,
        _: &(),
        _: &WlConn,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<ZwlrLayerSurfaceV1, ()> for WlState {
    fn event(
        state: &mut Self,
        layer: &ZwlrLayerSurfaceV1,
        ev: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &WlConn,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure { serial, .. } = ev {
            layer.ack_configure(serial);
            state.layer_configured = true;
        }
    }
}
impl Dispatch<WlSeat, ()> for WlState {
    fn event(
        state: &mut Self,
        seat: &WlSeat,
        ev: wl_seat::Event,
        _: &(),
        _: &WlConn,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(caps),
        } = ev
        {
            if caps.contains(Capability::Pointer) && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
            if caps.contains(Capability::Keyboard) && state.keyboard.is_none() {
                state.keyboard = Some(seat.get_keyboard(qh, ()));
            }
        }
    }
}
impl Dispatch<WlKeyboard, ()> for WlState {
    fn event(
        state: &mut Self,
        _: &WlKeyboard,
        ev: wl_keyboard::Event,
        _: &(),
        _: &WlConn,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key {
            key,
            state: WEnum::Value(wl_keyboard::KeyState::Pressed),
            ..
        } = ev
        {
            if let Some(k) = wl_keycode(key) {
                state.inputs.push(WlChromeInput::Key(k));
            }
        }
    }
}
impl Dispatch<WlOutput, ()> for WlState {
    fn event(
        state: &mut Self,
        output: &WlOutput,
        ev: wl_output::Event,
        _: &(),
        _: &WlConn,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = ev {
            if let Some((_, n)) = state.outputs.iter_mut().find(|(o, _)| o == output) {
                *n = name.to_string();
            }
        }
    }
}
impl Dispatch<WlPointer, ()> for WlState {
    fn event(
        state: &mut Self,
        _: &WlPointer,
        ev: wl_pointer::Event,
        _: &(),
        _: &WlConn,
        _: &QueueHandle<Self>,
    ) {
        let scale = state.pointer_scale;
        let on_surface = |surf: &WlSurface| {
            state
                .chrome_surface
                .as_ref()
                .is_some_and(|s| s == surf)
        };
        match ev {
            wl_pointer::Event::Enter { surface, surface_x, surface_y, .. } => {
                if on_surface(&surface) {
                    state.pointer_x = surface_x / scale;
                    state.pointer_y = surface_y / scale;
                    state.inputs.push(WlChromeInput::Move {
                        x: state.pointer_x,
                        y: state.pointer_y,
                    });
                }
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer_x = surface_x / scale;
                state.pointer_y = surface_y / scale;
                state.inputs.push(WlChromeInput::Move {
                    x: state.pointer_x,
                    y: state.pointer_y,
                });
            }
            wl_pointer::Event::Leave { .. } => state.inputs.push(WlChromeInput::Leave),
            wl_pointer::Event::Button {
                state: WEnum::Value(st),
                button,
                ..
            } => {
                let x = state.pointer_x;
                let y = state.pointer_y;
                match (st, button) {
                    (wl_pointer::ButtonState::Pressed, 0x110) => {
                        state.inputs.push(WlChromeInput::Down { x, y });
                    }
                    (wl_pointer::ButtonState::Released, 0x110) => {
                        state.inputs.push(WlChromeInput::Up { x, y });
                    }
                    (wl_pointer::ButtonState::Pressed, 0x111) => {
                        state.inputs.push(WlChromeInput::RightDown { x, y });
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlRegistry, ()> for WlState {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &WlConn,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            state.wl_globals.push(interface.to_string());
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind::<WlCompositor, _, _>(name, 1, qh, ()));
                }
                "wl_shm" => {
                    state.shm = Some(registry.bind::<WlShm, _, _>(name, 1, qh, ()));
                }
                "zwlr_layer_shell_v1" if state.layer_shell.is_none() => {
                    state.layer_shell =
                        Some(registry.bind::<ZwlrLayerShellV1, _, _>(name, 1, qh, ()));
                }
                other if other.contains("layer") => {
                    log::info!("chrome: wl global {other} (ignored)");
                }
                "wl_seat" if state.seat.is_none() => {
                    state.seat = Some(registry.bind::<WlSeat, _, _>(name, 7, qh, ()));
                }
                "wl_output" => {
                    let o = registry.bind::<WlOutput, _, _>(name, 4, qh, ());
                    state.outputs.push((o, String::new()));
                }
                _ => {}
            }
        }
    }
}

// everything else we bind is event-silent by construction — the only
// signal needed is buffer.release below
impl Dispatch<WlCompositor, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &WlConn,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<WlShm, ()> for WlState {
    fn event(
        state: &mut Self,
        _: &WlShm,
        ev: wl_shm::Event,
        _: &(),
        _: &WlConn,
        _: &QueueHandle<Self>,
    ) {
        if let wl_shm::Event::Format {
            format: WEnum::Value(f),
        } = ev
        {
            state.shm_formats.push(f);
        }
    }
}
impl Dispatch<WlShmPool, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &WlConn,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<WlRegion, ()> for WlState {
    fn event(
        _: &mut Self,
        _: &WlRegion,
        _: wl_region::Event,
        _: &(),
        _: &WlConn,
        _: &QueueHandle<Self>,
    ) {
    }
}
impl Dispatch<WlBuffer, usize> for WlState {
    fn event(
        state: &mut Self,
        _proxy: &WlBuffer,
        event: wl_buffer::Event,
        idx: &usize,
        _conn: &WlConn,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            state.released.push(*idx);
        }
    }
}
