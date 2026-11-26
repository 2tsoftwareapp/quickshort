//! Chrome layer — the post-capture album stack, fly-to-corner animation
//! and lightbox preview, drawn into one per-pixel-alpha surface per
//! monitor (see `chrome/macos.rs` for the AppKit presentation; other
//! platforms get a stub that declines `land` so the host falls back to
//! the quick-access card).
//!
//! Buffers are premultiplied `A<<24 | B<<16 | G<<8 | R` u32s — in memory
//! that is R,G,B,A bytes, what NSBitmapImageRep and wl_shm ARGB8888 take.
//! All drawing goes through `Canvas::blend_over`.

use anyhow::Result;
use image::RgbaImage;
use qs_overlay::sdf::{ellipse_sdf, fill_cov, rrect_sdf, stroke_cov, Canvas};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;

#[cfg(target_os = "linux")]
pub mod gtk_album;
#[cfg(target_os = "linux")]
pub mod gtk_layer;
#[cfg(target_os = "macos")]
#[path = "chrome/macos.rs"]
mod imp;
#[cfg(target_os = "windows")]
#[path = "chrome/windows.rs"]
mod imp;
#[cfg(target_os = "linux")]
#[path = "chrome/linux.rs"]
mod imp;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
#[path = "chrome/stub.rs"]
mod imp;

pub use imp::ChromeWin;

use crate::config;
use crate::widgets::Txt;

/// One stack surface on one monitor — the contract every backend in
/// `chrome/` fulfils (`stub.rs` is the readable form of it). Shared
/// logic above (`land`, `Chrome`) is written against this trait, so the
/// only `#[cfg]` left in this file is the `mod imp` platform selection.
trait PlatformChrome {
    fn new(
        el: &ActiveEventLoop,
        mon: &MonitorHandle,
        corner: Corner,
        font: Rc<fontdue::Font>,
        accent: u32,
    ) -> Result<Self>
    where
        Self: Sized;
    fn id(&self) -> winit::window::WindowId;
    fn set_dock_corner(&mut self, corner: Corner);
    fn mon_key(&self) -> &str;
    fn set_sink(&self, sink: Rc<dyn Fn(ChromeEv)>);
    fn land(&mut self, l: Land);
    fn stack(&self) -> &Stack;
    fn stack_mut(&mut self) -> &mut Stack;
    fn event(&mut self, ev: ChromeEv) -> Option<ChromeCmd>;
    fn redraw(&mut self);
    fn window_event(&mut self, ev: &winit::event::WindowEvent) -> bool;
    fn pump(&mut self) -> Option<Instant>;
    /// macOS drags shots natively — the shared remove/clear paths must
    /// cancel an in-flight drag before the shot underneath moves.
    /// Backends without native drags keep the default no-op.
    fn cancel_native(&mut self) {}
    /// App that was frontmost when the capture ran (provenance for
    /// Saved shots). Backends without a window API return `None`.
    fn frontmost_app_name() -> Option<String>
    where
        Self: Sized,
    {
        None
    }
}

mod stack;

pub use stack::{ChromeCmd, ChromeEv, Corner, FRect, Key, Shot, ShotKind, Stack};
pub use stack::{MENU_BEAUTIFY, MENU_CLEAR, MENU_COPY, MENU_DEL, MENU_OPEN,
                MENU_PIN, MENU_REVEAL, MENU_REVOKE, MENU_SAVE, MENU_SHARE};
#[cfg(target_os = "linux")]
pub(super) use stack::shelf_logical_size;


/// Name of the application in front of the user, if the platform reports one.
pub fn frontmost_app_name() -> Option<String> {
    ChromeWin::frontmost_app_name()
}

/// Owns one `ChromeWin` per monitor; lazily creates the surface on the
/// monitor the capture came from.
pub struct Chrome {
    pub wins: Vec<ChromeWin>,
    accent: u32,
    pos: String,
    /// builds an event sink for a window id — the host wires its
    /// EventLoopProxy through this so view callbacks become HostEvents
    pub sink: Sink,
}

/// builds an event sink for a window id — see `Chrome::sink`
pub type Sink = Box<dyn Fn(winit::window::WindowId) -> Rc<dyn Fn(ChromeEv)>>;

/// Per-call inputs for `land` — keeps the signature readable.
pub struct Land {
    pub img: RgbaImage,
    pub kind: ShotKind,
    pub path: Option<PathBuf>,
    /// selection in virtual logical coords — the fly's origin
    pub sel: Option<qs_core::Rect>,
    /// skip the fly-in (rehydrated shots land already settled)
    pub instant: bool,
    /// recording length in seconds — drives the `m:ss` badge on ▶
    /// thumbs; ignored for other kinds
    pub dur_s: Option<u64>,
    /// live share link — drawn as the ↗ chip; set by the host when the
    /// shot already has one (rehydrate, or Share ran before the fly-in)
    pub shared: Option<crate::share::ShareEntry>,
}

impl Chrome {
    pub fn new(accent: u32, pos: &str, sink: Sink) -> Self {
        Self {
            wins: Vec::new(),
            accent,
            pos: pos.to_string(),
            sink,
        }
    }

    /// Fly the shot into the stack. On failure the `Land` is handed back
    /// so the host can fall back to the quick-access card.
    pub fn land(
        &mut self,
        el: &winit::event_loop::ActiveEventLoop,
        l: Land,
    ) -> Result<(), Box<Land>> {
        land(self, el, l)
    }

    /// Window events (redraw, cursor-left). Returns true if consumed.
    pub fn window_event(
        &mut self,
        id: winit::window::WindowId,
        ev: &winit::event::WindowEvent,
    ) -> bool {
        self.wins
            .iter_mut()
            .any(|w| w.id() == id && w.window_event(ev))
    }

    /// Events raised by the platform view (mouse, keys). Internal Redraw
    /// commands are handled here; only host-level commands surface.
    pub fn event(
        &mut self,
        id: winit::window::WindowId,
        ev: ChromeEv,
    ) -> Option<(winit::window::WindowId, ChromeCmd)> {
        let w = self.wins.iter_mut().find(|w| w.id() == id)?;
        match w.event(ev) {
            Some(ChromeCmd::Redraw) => {
                w.redraw();
                None
            }
            other => other.map(|c| (id, c)),
        }
    }

    /// `about_to_wait` wake for the fly animation.
    pub fn pump(&mut self) -> Option<Instant> {
        let mut wake = None;
        for w in &mut self.wins {
            if let Some(t) = w.pump() {
                wake = Some(wake.map_or(t, |x: Instant| x.min(t)));
            }
        }
        wake
    }

    /// Look up a stack shot for context-menu actions.
    pub fn shot(&self, id: winit::window::WindowId, idx: usize) -> Option<&Shot> {
        self.wins.iter().find(|w| w.id() == id)?.stack().shot(idx)
    }

    /// Mutable variant — the host flips `shared` on share/revoke.
    pub fn shot_mut(&mut self, id: winit::window::WindowId, idx: usize) -> Option<&mut Shot> {
        self.wins
            .iter_mut()
            .find(|w| w.id() == id)?
            .stack_mut()
            .shot_mut(idx)
    }

    /// Repaint one stack window (badge/chip state changed).
    pub fn redraw(&mut self, id: winit::window::WindowId) {
        if let Some(w) = self.wins.iter_mut().find(|w| w.id() == id) {
            w.redraw();
        }
    }

    pub fn remove(&mut self, id: winit::window::WindowId, idx: usize) {
        if let Some(w) = self.wins.iter_mut().find(|w| w.id() == id) {
            // a removal reshuffles the landing slot — the fly layer is
            // pointing at geometry that no longer exists
            w.cancel_native();
            w.stack_mut().remove(idx);
            w.redraw();
        }
    }

    /// Dismiss the whole stack on window `id` (clear-all). The caller
    /// sweeps afterwards so the emptied window goes away.
    pub fn clear_all(&mut self, id: winit::window::WindowId) {
        if let Some(w) = self.wins.iter_mut().find(|w| w.id() == id) {
            w.cancel_native();
            w.stack_mut().clear_all();
        }
    }

    /// Drop windows whose stack emptied (call after remove).
    pub fn sweep(&mut self) {
        self.wins.retain(|w| !w.stack().is_empty());
    }

    pub fn accent(&self) -> u32 {
        self.accent
    }
    pub fn pos(&self) -> &str {
        &self.pos
    }

    /// Settings → Appearance dock corner; existing stacks retarget in place.
    pub fn set_popup_pos(&mut self, pos: &str) {
        self.pos = pos.to_string();
        let corner = Corner::from_popup_pos(pos);
        for w in &mut self.wins {
            w.set_dock_corner(corner);
            w.redraw();
        }
    }
}

/* ==================== entry point (shared by every backend) ========== */

/// Monitor containing the selection's centre, else the primary.
fn monitor_for(el: &ActiveEventLoop, sel: Option<qs_core::Rect>) -> Option<MonitorHandle> {
    let mut mons = el.available_monitors();
    let primary = el.primary_monitor();
    match sel {
        Some(r) => {
            let (cx, cy) = (r.x as f64 + r.w as f64 / 2.0, r.y as f64 + r.h as f64 / 2.0);
            mons.find(|m| {
                let sf = m.scale_factor();
                let p = m.position();
                let s = m.size();
                let (mx, my) = (p.x as f64 / sf, p.y as f64 / sf);
                let (mw, mh) = (s.width as f64 / sf, s.height as f64 / sf);
                cx >= mx && cx < mx + mw && cy >= my && cy < my + mh
            })
            .or(primary)
        }
        None => primary,
    }
}

/// Positions/name don't make a stable cross-platform id — the key only
/// needs to tell one monitor from another within a session.
fn mon_key(m: &MonitorHandle) -> String {
    format!(
        "{:?}@{:?}x{:?}s{:.3}",
        m.position(),
        m.size().width,
        m.size().height,
        m.scale_factor()
    )
}

/// `Chrome::land` — the shared entry. Err hands the `Land` back intact
/// so the host can fall back to the quick-access card.
fn land(chrome: &mut Chrome, el: &ActiveEventLoop, l: Land) -> Result<(), Box<Land>> {
    let Some(mon) = monitor_for(el, l.sel) else {
        log::warn!("chrome land: no monitor for selection {:?}", l.sel);
        return Err(Box::new(l));
    };
    let key = mon_key(&mon);
    if chrome.wins.iter().all(|w| w.mon_key() != key) {
        let font = Rc::new(match crate::settings::system_font() {
            Ok(f) => f,
            Err(e) => {
                log::warn!("chrome land: system font: {e:#}");
                return Err(Box::new(l));
            }
        });
        let corner = Corner::from_popup_pos(chrome.pos());
        let w = match ChromeWin::new(el, &mon, corner, font, chrome.accent()) {
            Ok(w) => w,
            Err(e) => {
                log::warn!("chrome win: {e:#}");
                return Err(Box::new(l));
            }
        };
        // the sink binds the window id, which only exists after creation
        w.set_sink((chrome.sink)(w.id()));
        chrome.wins.push(w);
    }
    let w = chrome.wins.iter_mut().find(|w| w.mon_key() == key).unwrap();
    w.land(l);
    Ok(())
}

/// `--chrome` demo: a bare event loop hosting one stack on the primary
/// monitor, landing synthetic shots so the layer can be verified by hand.
pub fn standalone(pos: &str, accent: u32) -> anyhow::Result<()> {
    use winit::application::ApplicationHandler;
    use winit::event_loop::{ControlFlow, EventLoop, EventLoopProxy};
    use winit::window::WindowId;

    enum Ev {
        View(WindowId, ChromeEv),
    }

    fn shot(i: usize) -> Land {
        let img = image::RgbaImage::from_fn(1200, 800, |x, y| {
            image::Rgba([
                ((x * 200 / 1200) as u8).saturating_add(i as u8 * 40),
                (y * 200 / 800) as u8,
                120u8.saturating_add(i as u8 * 30),
                255,
            ])
        });
        Land {
            img,
            kind: [ShotKind::Saved, ShotKind::Shared, ShotKind::Record][i % 3],
            path: None,
            sel: Some(qs_core::Rect::new(200, 200, 400, 300)),
            instant: false,
            dur_s: None,
            shared: None,
        }
    }

    struct Demo {
        chrome: Chrome,
        drops: usize,
        next_drop: Instant,
    }
    impl Demo {
        fn drop(&mut self, el: &ActiveEventLoop) {
            let n = self.drops;
            if self.chrome.land(el, shot(n)).is_ok() {
                self.drops += 1;
            }
            self.next_drop = Instant::now() + Duration::from_secs(1);
        }
    }
    impl ApplicationHandler<Ev> for Demo {
        fn resumed(&mut self, el: &ActiveEventLoop) {
            self.drop(el);
        }
        fn window_event(
            &mut self,
            _el: &ActiveEventLoop,
            id: WindowId,
            ev: winit::event::WindowEvent,
        ) {
            self.chrome.window_event(id, &ev);
        }
        fn user_event(&mut self, _el: &ActiveEventLoop, ev: Ev) {
            let Ev::View(id, e) = ev;
            if let Some((wid, cmd)) = self.chrome.event(id, e) {
                match cmd {
                    ChromeCmd::Remove(i) => {
                        self.chrome.remove(wid, i);
                        self.chrome.sweep();
                    }
                    ChromeCmd::ClearAll => {
                        self.chrome.clear_all(wid);
                        self.chrome.sweep();
                    }
                    ChromeCmd::MenuAction { idx, id } => log::info!("menu: thumb {idx} → {id}"),
                    ChromeCmd::DragBegin { idx, .. } => log::info!("drag: thumb {idx}"),
                    ChromeCmd::Redraw => {}
                }
            }
        }
        fn about_to_wait(&mut self, el: &ActiveEventLoop) {
            if self.drops < 3 && Instant::now() >= self.next_drop {
                self.drop(el);
            }
            let mut wake = self.chrome.pump();
            if self.drops < 3 {
                wake = Some(wake.map_or(self.next_drop, |w| w.min(self.next_drop)));
            }
            el.set_control_flow(match wake {
                Some(w) => ControlFlow::WaitUntil(w),
                None => ControlFlow::Wait,
            });
        }

        fn exiting(&mut self, _el: &ActiveEventLoop) {
            // Wl::drop marshals destroys over winit's wl_display —
            // only safe before the loop disconnects it.
            self.chrome.wins.clear();
        }
    }

    let el = EventLoop::<Ev>::with_user_event().build()?;
    let proxy: EventLoopProxy<Ev> = el.create_proxy();
    let px = proxy.clone();
    let chrome = Chrome::new(
        accent,
        pos,
        Box::new(move |wid| {
            let px = px.clone();
            Rc::new(move |e| {
                let _ = px.send_event(Ev::View(wid, e));
            })
        }),
    );
    let mut demo = Demo {
        chrome,
        drops: 0,
        next_drop: Instant::now(),
    };
    el.run_app(&mut demo)?;
    Ok(())
}

