//! wlroots screen capture — `zwlr_screencopy_manager_v1` in-process.
//!
//! Instant still captures on Sway, River, Niri, Hyprland, Wayfire, labwc
//! and friends: no consent dialog, no PipeWire session. Each output's
//! logical layout comes from `xdg_output` (already scale/transform
//! applied); the buffer scale is derived from the returned pixels like
//! every other backend (`mk_frame`).

use anyhow::{bail, Context, Result};
use image::RgbaImage;
use qs_core::{CapturedFrame, Rect};
use std::ffi::CString;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_buffer, wl_output, wl_registry, wl_shm, wl_shm_pool};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1, zwlr_screencopy_manager_v1,
};

use super::wayland::mk_frame;
use crate::ScreenCapture;

pub struct WlrCapture;

#[derive(Default)]
struct OutInfo {
    /// xdg_output logical geometry (scale + transform already applied)
    logical: Option<Rect>,
    /// wl_output fallback: geometry position + mode size / integer scale
    geo_pos: (i32, i32),
    mode: (u32, u32),
    scale: i32,
}

struct Pending {
    /// negotiated shm params from the frame's `buffer` event
    fmt: Option<(u32, u32, u32, wl_shm::Format)>, // w, h, stride, format
    y_invert: bool,
    failed: Option<String>,
    done: bool,
}

struct State {
    shm: Option<wl_shm::WlShm>,
    copy_mgr: Option<zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1>,
    xdg_mgr: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
    outs: Vec<(wl_output::WlOutput, OutInfo)>,
    pending: Option<Pending>,
    /// shm objects of the in-flight frame — kept alive until pixels
    /// are copied out (fd, mmap, pool, buffer)
    keep: Option<Keep>,
}

struct Keep {
    buf: wl_buffer::WlBuffer,
    pool: wl_shm_pool::WlShmPool,
    _fd: OwnedFd,
    map: *mut libc::c_void,
    size: usize,
}

/// Does the compositor export `zwlr_screencopy_manager_v1`? Cheap probe —
/// connect, read the registry global list, no roundtrip needed.
pub fn available() -> bool {
    let Ok(conn) = Connection::connect_to_env() else {
        return false;
    };
    let Ok((globals, _)) = registry_queue_init::<ProbeState>(&conn) else {
        return false;
    };
    globals.contents().clone_list().iter().any(|g| {
        g.interface == zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1::interface().name
    })
}

struct ProbeState;
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for ProbeState {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl State {
    fn logical_of(&self, i: usize) -> Rect {
        let info = &self.outs[i].1;
        if let Some(r) = info.logical {
            return r;
        }
        // wl_output fallback: geometry pos is compositor-space (logical),
        // mode is physical -> divide by integer scale
        let s = info.scale.max(1) as f64;
        Rect::new(
            info.geo_pos.0,
            info.geo_pos.1,
            (info.mode.0 as f64 / s).round() as u32,
            (info.mode.1 as f64 / s).round() as u32,
        )
    }
}

/// wl_shm bytes -> RGBA8. `wl_shm::Format` names are byte-order-reversed
/// from intuition (Xrgb8888 = B,G,R,X in memory on little-endian).
fn shm_to_rgba(
    fmt: wl_shm::Format,
    w: u32,
    h: u32,
    stride: usize,
    y_invert: bool,
    raw: &[u8],
) -> Result<RgbaImage> {
    let (w, h) = (w as usize, h as usize);
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        let src_y = if y_invert { h - 1 - y } else { y };
        let si = src_y * stride;
        if si + w * 4 > raw.len() {
            break;
        }
        px[y * w * 4..(y + 1) * w * 4].copy_from_slice(&raw[si..si + w * 4]);
    }
    use wl_shm::Format as F;
    match fmt {
        // B,G,R,(X|A) in memory
        F::Argb8888 | F::Xrgb8888 => {
            for p in px.chunks_exact_mut(4) {
                p.swap(0, 2);
                p[3] = 255;
            }
        }
        // R,G,B,(X|A) in memory — already RGBA order
        F::Abgr8888 | F::Xbgr8888 => {
            for p in px.chunks_exact_mut(4) {
                p[3] = 255;
            }
        }
        // (X|A),R,G,B in memory — rotate left
        F::Rgbx8888 | F::Rgba8888 => {
            for p in px.chunks_exact_mut(4) {
                p.copy_from_slice(&[p[1], p[2], p[3], 255]);
            }
        }
        // (X|A),B,G,R in memory
        F::Bgrx8888 | F::Bgra8888 => {
            for p in px.chunks_exact_mut(4) {
                p.copy_from_slice(&[p[3], p[1], p[2], 255]);
            }
        }
        other => bail!("zwlr shm format {other:?} unsupported"),
    }
    RgbaImage::from_raw(w as u32, h as u32, px).context("wlr frame dims")
}

/// memfd-backed shm pool for one frame; the returned `Keep` holds fd,
/// mapping, pool and buffer alive until the pixels are copied out.
fn alloc_frame_buffer(
    shm: &wl_shm::WlShm,
    w: u32,
    h: u32,
    stride: u32,
    fmt: wl_shm::Format,
    qh: &QueueHandle<State>,
) -> Result<Keep> {
    let size = (stride * h) as usize;
    let name = CString::new("qs-screencopy")?;
    let raw = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
    if raw < 0 {
        bail!("memfd_create: {}", std::io::Error::last_os_error());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    if unsafe { libc::ftruncate(fd.as_raw_fd(), size as libc::off_t) } != 0 {
        bail!("ftruncate: {}", std::io::Error::last_os_error());
    }
    let map = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ,
            libc::MAP_SHARED,
            fd.as_raw_fd(),
            0,
        )
    };
    if map == libc::MAP_FAILED {
        bail!("mmap shm pool: {}", std::io::Error::last_os_error());
    }
    let pool = shm.create_pool(fd.as_fd(), size as i32, qh, ());
    let buf = pool.create_buffer(0, w as i32, h as i32, stride as i32, fmt, qh, ());
    Ok(Keep {
        buf,
        pool,
        _fd: fd,
        map,
        size,
    })
}

impl ScreenCapture for WlrCapture {
    fn capture_all(&self) -> Result<Vec<CapturedFrame>> {
        let conn = Connection::connect_to_env().context("wayland connect")?;
        let (globals, mut eq) = registry_queue_init::<State>(&conn).context("wayland registry")?;
        let qh = eq.handle();
        let registry = globals.registry().clone();

        let mut state = State {
            shm: None,
            copy_mgr: None,
            xdg_mgr: None,
            outs: Vec::new(),
            pending: None,
            keep: None,
        };
        for g in globals.contents().clone_list() {
            let interface = g.interface.as_str();
            if interface == wl_shm::WlShm::interface().name {
                state.shm =
                    Some(registry.bind(g.name, g.version.min(1), &qh, ()));
            } else if interface
                == zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1::interface().name
            {
                state.copy_mgr =
                    Some(registry.bind(g.name, g.version.min(3), &qh, ()));
            } else if interface == zxdg_output_manager_v1::ZxdgOutputManagerV1::interface().name {
                state.xdg_mgr =
                    Some(registry.bind(g.name, g.version.min(3), &qh, ()));
            } else if interface == wl_output::WlOutput::interface().name {
                let idx = state.outs.len();
                let out: wl_output::WlOutput =
                    registry.bind(g.name, g.version.min(4), &qh, idx);
                state.outs.push((out, OutInfo::default()));
            }
        }
        let (Some(copy_mgr), Some(shm)) = (&state.copy_mgr, &state.shm) else {
            bail!("compositor lacks zwlr_screencopy_manager_v1 / wl_shm");
        };
        let (copy_mgr, shm) = (copy_mgr.clone(), shm.clone());
        if state.outs.is_empty() {
            bail!("no wl_outputs");
        }
        // one xdg_output per wl_output — events carry the output index
        if let Some(mgr) = &state.xdg_mgr {
            for (i, (out, _)) in state.outs.iter().enumerate() {
                let _ = mgr.get_xdg_output(out, &qh, i);
            }
        }
        eq.roundtrip(&mut state).context("wayland roundtrip")?;

        let mut frames = Vec::with_capacity(state.outs.len());
        for i in 0..state.outs.len() {
            state.pending = Some(Pending {
                fmt: None,
                y_invert: false,
                failed: None,
                done: false,
            });
            let _frame = copy_mgr.capture_output(0, &state.outs[i].0, &qh, ());
            while !state
                .pending
                .as_ref()
                .map(|p| p.done || p.failed.is_some())
                .unwrap_or(true)
            {
                eq.blocking_dispatch(&mut state).context("wayland dispatch")?;
            }
            let p = state.pending.take().unwrap();
            if let Some(e) = p.failed {
                bail!("zwlr capture output {i}: {e}");
            }
            let Some((w, h, stride, fmt)) = p.fmt else {
                bail!("zwlr frame finished with no buffer event");
            };
            // copy pixels out of the shm mapping, then drop the objects
            let keep = state.keep.take().context("no shm pool kept")?;
            let raw = unsafe { std::slice::from_raw_parts(keep.map as *const u8, keep.size) };
            let img = shm_to_rgba(fmt, w, h, stride as usize, p.y_invert, raw)?;
            unsafe { libc::munmap(keep.map, keep.size) };
            keep.buf.destroy();
            keep.pool.destroy();
            frames.push(mk_frame(img, state.logical_of(i)));
        }
        let _ = shm; // bound above; kept alive for the whole capture
        Ok(frames)
    }

    fn probe(&self) -> bool {
        available()
    }
}

// ------------------------------------------------------------------ events

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_output::WlOutput, usize> for State {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        ev: wl_output::Event,
        &i: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let info = &mut state.outs[i].1;
        match ev {
            wl_output::Event::Geometry { x, y, .. } => info.geo_pos = (x, y),
            wl_output::Event::Mode { width, height, .. } => {
                info.mode = (width.max(0) as u32, height.max(0) as u32)
            }
            wl_output::Event::Scale { factor } => info.scale = factor,
            _ => {}
        }
    }
}

impl Dispatch<zxdg_output_v1::ZxdgOutputV1, usize> for State {
    fn event(
        state: &mut Self,
        _: &zxdg_output_v1::ZxdgOutputV1,
        ev: zxdg_output_v1::Event,
        &i: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let info = &mut state.outs[i].1;
        match ev {
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                let (x, y) = info.logical.map(|r| (r.x, r.y)).unwrap_or((0, 0));
                info.logical =
                    Some(Rect::new(x, y, width.max(0) as u32, height.max(0) as u32));
            }
            zxdg_output_v1::Event::LogicalPosition { x, y } => {
                let (w, h) = info.logical.map(|r| (r.w, r.h)).unwrap_or((0, 0));
                info.logical = Some(Rect::new(x, y, w, h));
            }
            _ => {}
        }
    }
}

impl Dispatch<zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1, ()> for State {
    fn event(
        state: &mut Self,
        frame: &zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1,
        ev: zwlr_screencopy_frame_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let Some(p) = state.pending.as_mut() else {
            return;
        };
        match ev {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format,
                width,
                height,
                stride,
            } => {
                let format = format.into_result().unwrap_or(wl_shm::Format::Argb8888);
                p.fmt = Some((width, height, stride, format));
                match alloc_frame_buffer(state.shm.as_ref().unwrap(), width, height, stride, format, qh)
                {
                    Ok(keep) => {
                        frame.copy(&keep.buf);
                        state.keep = Some(keep);
                    }
                    Err(e) => p.failed = Some(format!("shm pool: {e:#}")),
                }
            }
            zwlr_screencopy_frame_v1::Event::Flags { flags } => {
                p.y_invert = matches!(flags, WEnum::Value(f) if f.contains(zwlr_screencopy_frame_v1::Flags::YInvert));
            }
            zwlr_screencopy_frame_v1::Event::Failed => {
                p.failed = Some("compositor reported frame failure".into());
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => p.done = true,
            _ => {}
        }
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm::WlShm, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zxdg_output_manager_v1::ZxdgOutputManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &zxdg_output_manager_v1::ZxdgOutputManagerV1,
        _: zxdg_output_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
        _: zwlr_screencopy_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // needs a live wlroots session
    fn wlr_capture_returns_one_frame_per_output() {
        let frames = WlrCapture.capture_all().unwrap();
        assert!(!frames.is_empty());
        for f in &frames {
            assert!(f.pixels.width() > 0 && f.pixels.height() > 0);
        }
    }
}
