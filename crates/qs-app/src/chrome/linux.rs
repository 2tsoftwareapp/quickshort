//! Linux chrome backend: on Wayland a **layer-shell** corner shelf (desktop
//! overlay, not an app toplevel); on X11 a positioned winit window with
//! skip-taskbar. Pixels go through `wl_shm` on Wayland or XPutImage on X11.
//!
//! Click-through comes from the compositor's own input routing:
//! `wl_surface_set_input_region` / XSHAPE `SK_INPUT` confine delivery to
//! the stack's live zone rects, so moves outside them land on the apps
//! underneath us instead. Unlike the macOS global-monitor trick this
//! happens inside the compositor — no accessibility permission needed.
//!
//! Transparency on X11 requires a running composite manager; on the rare
//! bare setup the window degrades to opaque and stays functional.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use qs_overlay::sdf::Canvas;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key as WKey, ModifiersState, NamedKey};
use winit::monitor::MonitorHandle;
use winit::window::{Fullscreen, Window, WindowId, WindowLevel};

use super::{ChromeCmd, ChromeEv, Corner, FRect, Key, Land, PlatformChrome, Stack, shelf_logical_size};

/* ==================== window ==================== */

/// One chrome surface per monitor.
pub struct ChromeWin {
    win: Rc<Window>,
    imp: Imp,
    pub stack: Stack,
    /// premul `A<<24|B<<16|G<<8|R` — memory RGBA, the wl_shm ABGR8888 layout
    buf: Vec<u32>,
    sink: RefCell<Option<Rc<dyn Fn(ChromeEv)>>>,
    /// last cursor position in surface-local physical px — MouseInput
    /// carries no position, so Down/Up replay this
    cursor: PhysicalPosition<f64>,
    /// left button held — CursorMoved becomes Dragged, not Move
    held: bool,
    /// ctrl held — turns C into Copy (no menu bar on Linux to do it)
    mods: ModifiersState,
    mon_key: String,
    /// logical size/origin of this monitor
    lw: f64,
    lh: f64,
    mx: f64,
    my: f64,
    scale: f64,
    font: Rc<fontdue::Font>,
    accent: u32,
    /// macOS paints before `setVisible:` — map only after the first
    /// frame with stack pixels so GNOME doesn't flash a fullscreen black
    /// layer while the buffer is still empty.
    shown: bool,
    /// Wayland layer-shell: logical shelf size is fixed; winit only holds
    /// a 1×1 handle for WindowId (not the visible surface).
    fixed_shelf: bool,
    /// macOS-style full-monitor overlay (gtk-layer-shell on GNOME).
    full_monitor: bool,
    /// Pixels go to winit's surface — must map the window (GNOME has no zwlr).
    winit_maps: bool,
}

/// The presenter — chosen once from the raw handle; Wayland marshals
/// surface requests over winit's own wl_display, X11 paints through a
/// private xcb connection.
enum Imp {
    Wl(Wl),
    Gtk(super::gtk_layer::GtkLayer),
    X11(X11),
    /// GTK utility overlay on the tray thread (GNOME — no "Unknown" app).
    GtkAlbum(super::gtk_album::GtkAlbum),
}

/// Physical outer position for the shelf panel on `mon`.
fn shelf_outer_pos(
    mon: &MonitorHandle,
    corner: Corner,
    sw: f64,
    sh: f64,
) -> PhysicalPosition<f64> {
    let sf = mon.scale_factor();
    let mp = mon.position();
    let ms = mon.size();
    let side = 20.0 * sf;
    let top = 20.0 * sf;
    let x = if corner.right() {
        mp.x as f64 + ms.width as f64 - sw * sf - side
    } else if corner == Corner::TC || corner == Corner::BC {
        mp.x as f64 + (ms.width as f64 - sw * sf) / 2.0
    } else {
        mp.x as f64 + side
    };
    let y = if corner.bottom() {
        mp.y as f64 + ms.height as f64 - sh * sf - side
    } else {
        mp.y as f64 + top
    };
    PhysicalPosition::new(x, y)
}

thread_local! {
    /// One hidden winit handle for GTK album stacks — avoids a wl_surface per
    /// monitor (GNOME protocol errors after overlay teardown).
    static GTK_HANDLE: RefCell<Option<Rc<Window>>> = const { RefCell::new(None) };
}

fn gtk_winit_handle(el: &ActiveEventLoop) -> Result<Rc<Window>> {
    GTK_HANDLE.with(|h| {
        if let Some(w) = h.borrow().as_ref() {
            return Ok(w.clone());
        }
        let attrs = Window::default_attributes()
            .with_title("quickshort-chrome")
            .with_decorations(false)
            .with_resizable(false)
            .with_visible(false)
            .with_active(false)
            .with_inner_size(LogicalSize::new(2.0, 2.0));
        let win = Rc::new(
            el.create_window(attrs)
                .context("chrome gtk winit handle")?,
        );
        *h.borrow_mut() = Some(win.clone());
        Ok(win)
    })
}

fn winit_chrome_window(
    el: &ActiveEventLoop,
    mon: &MonitorHandle,
    corner: Corner,
    sw: f64,
    sh: f64,
) -> Result<Rc<Window>> {
    let outer = shelf_outer_pos(mon, corner, sw, sh);
    let mut attrs = Window::default_attributes()
        .with_title("Quickshort")
        .with_transparent(true)
        .with_decorations(false)
        .with_resizable(false)
        .with_visible(false)
        .with_active(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_inner_size(LogicalSize::new(sw, sh))
        .with_position(outer);
    // Both extension traits set the same app_id/WM_CLASS; winit applies
    // whichever matches the backend it picks at runtime.
    {
        use winit::platform::wayland::WindowAttributesExtWayland as _;
        attrs = attrs.with_name("quickshort", "quickshort-chrome");
    }
    {
        use winit::platform::x11::{WindowAttributesExtX11 as _, WindowType as X11WindowType};
        attrs = attrs.with_x11_window_type(vec![
            X11WindowType::Utility,
            X11WindowType::Notification,
        ]);
        attrs = attrs.with_name("quickshort", "quickshort-chrome");
    }
    Ok(Rc::new(el.create_window(attrs).context("chrome window")?))
}

fn try_gtk_album(
    mon: &MonitorHandle,
    corner: Corner,
    sw: f64,
    sh: f64,
    scale: f64,
) -> Result<super::gtk_album::GtkAlbum> {
    if std::env::var("WAYLAND_DISPLAY")
        .ok()
        .is_none_or(|s| s.is_empty())
    {
        anyhow::bail!("not wayland");
    }
    let out = mon.name();
    let pos = mon.position();
    super::gtk_album::GtkAlbum::new(
        corner,
        sw,
        sh,
        scale,
        out.clone(),
        pos.x,
        pos.y,
    )
}

impl ChromeWin {


    fn reveal_if_ready(&mut self, painted: bool) {
        // zwlr/gtk layer — winit is only a 1×1 handle; mapping it lands
        // a shelf-sized surface in the compositor's default placement.
        if !self.winit_maps {
            if painted {
                self.shown = true;
            }
            return;
        }
        if !self.shown && painted {
            self.win.set_visible(true);
            self.win.request_redraw();
            self.shown = true;
        }
    }









    fn emit(&self, ev: ChromeEv) {
        if let Some(s) = self.sink.borrow().clone() {
            s(ev);
        }
    }

    /// Render the stack into the buffer and push it to the window.
    /// `paint_bounds` caps the work to the dirty region — a fullscreen
    /// copy + full-surface damage per frame was the animation jank.
    fn present(&mut self) {
        let now = Instant::now();
        // track size/scale drift (fractional-scale flips, mode changes)
        // here instead of a dedicated resize path - buffers simply
        // follow inner_size each frame
        self.scale = self.win.scale_factor();
        if !self.fixed_shelf && !self.full_monitor {
            let size = self.win.inner_size();
            let (pw, ph) = (size.width.max(1) as usize, size.height.max(1) as usize);
            self.lw = pw as f64 / self.scale;
            self.lh = ph as f64 / self.scale;
        }
        let (pw, ph) = (
            (self.lw * self.scale).round().max(1.0) as usize,
            (self.lh * self.scale).round().max(1.0) as usize,
        );
        if self.buf.len() != pw * ph {
            self.buf = vec![0; pw * ph];
        }
        let bounds = self.stack.paint_bounds(self.lw, self.lh, now);
        if let Some(b) = bounds {
            let s = self.scale;
            let y0 = ((b.y * s).floor() as usize).min(ph);
            let y1 = (((b.y + b.h) * s).ceil() as usize).min(ph);
            for row in &mut self.buf[y0.min(y1) * pw..y1 * pw] {
                *row = 0;
            }
        } else {
            self.buf.fill(0);
        }
        {
            let mut cv = Canvas {
                buf: &mut self.buf,
                w: pw,
                h: ph,
                scale: self.scale,
            };
            let font = self.font.clone();
            self.stack.render(&mut cv, &font, self.accent, now);
        }
        // zone rects (not input_rects): the wider retention zone is the
        // hysteresis that keeps hover alive between moving thumbs
        let rects = self.stack.zone_rects(self.lw, self.lh);
        // Wayland: arm winit's wl_surface.frame callback before our raw
        // commit. Without it FrameCallbackState stays None and every
        // request_redraw delivers RedrawRequested immediately - an
        // unthrottled busy-loop instead of vsync-paced animation.
        // dirty in physical px for the backends
        let dirty = bounds.map(|b| FRect {
            x: b.x * self.scale,
            y: b.y * self.scale,
            w: b.w * self.scale,
            h: b.h * self.scale,
        });
        if self.fixed_shelf {
            if let Imp::Wl(w) = &mut self.imp {
                w.sync_layer_keyboard(self.stack.modal_open());
            }
        }
        let show = self.shown || bounds.is_some() || self.stack.animating();
        self.imp
            .present(&self.buf, pw, ph, &rects, self.scale, dirty, show);
        self.reveal_if_ready(bounds.is_some() || self.stack.animating());
    }



    fn apply_wl_input(&mut self, ev: WlChromeInput) {
        match ev {
            WlChromeInput::Move { x, y } => {
                self.cursor = PhysicalPosition::new(x * self.scale, y * self.scale);
                self.emit(if self.held {
                    ChromeEv::Dragged { x, y }
                } else {
                    ChromeEv::Move { x, y }
                });
            }
            WlChromeInput::Leave => {
                self.held = false;
                self.emit(ChromeEv::Leave);
            }
            WlChromeInput::Down { x, y } => {
                self.held = true;
                self.emit(ChromeEv::Down { x, y });
            }
            WlChromeInput::Up { x, y } => {
                self.held = false;
                self.emit(ChromeEv::Up { x, y });
            }
            WlChromeInput::RightDown { x, y } => {
                self.emit(ChromeEv::RightDown { x, y });
            }
            WlChromeInput::Key(k) => self.emit(ChromeEv::Key(k)),
        }
    }









    /// unused outside macOS — the host cfg-gates the call site
    pub fn view_ptr(&self) -> *const c_void {
        std::ptr::null()
    }
}

/// Shared chrome contract — signatures enforced by
/// `PlatformChrome` in `chrome.rs`; helpers stay inherent.
impl PlatformChrome for ChromeWin {
    fn new(
        el: &ActiveEventLoop,
        mon: &MonitorHandle,
        corner: Corner,
        font: Rc<fontdue::Font>,
        accent: u32,
    ) -> Result<Self> {
        let sf = mon.scale_factor();
        let pos = mon.position();
        let ms = mon.size();
        let (mlw, mlh) = (ms.width as f64 / sf, ms.height as f64 / sf);
        let (sw, sh) = shelf_logical_size();
        let (win, imp) = match try_gtk_album(mon, corner, sw, sh, sf) {
            Ok(album) => (gtk_winit_handle(el)?, Imp::GtkAlbum(album)),
            Err(e) => {
                log::warn!("chrome: gtk-album unavailable ({e:#}) — wl/x11 fallback");
                let win = winit_chrome_window(el, mon, corner, sw, sh)?;
                (win.clone(), Imp::new(&win, mon, corner, mlw, mlh, sw, sh, sf)?)
            }
        };
        if let Imp::GtkAlbum(a) = &imp {
            a.bind_winit(win.id());
        }
        let gtk_shelf = matches!(imp, Imp::GtkAlbum(_));
        let fixed_shelf = imp.layer_mode();
        let winit_maps = matches!(&imp, Imp::X11(_));
        // macOS / layer-shell: full-monitor canvas. GNOME gtk-album: shelf only.
        let full_monitor = if gtk_shelf {
            false
        } else {
            matches!(imp, Imp::Gtk(_)) || fixed_shelf || winit_maps
        };
        let mut sf = sf;
        if winit_maps {
            win.set_fullscreen(Some(Fullscreen::Borderless(Some(mon.clone()))));
        }
        let (mut lw, mut lh) = if full_monitor {
            (mlw, mlh)
        } else {
            let size = win.inner_size();
            (size.width as f64 / sf, size.height as f64 / sf)
        };
        if let Imp::Gtk(g) = &imp {
            lw = g.lw;
            lh = g.lh;
            sf = g.scale;
        }
        if let Imp::GtkAlbum(g) = &imp {
            lw = g.lw;
            lh = g.lh;
            sf = g.scale;
        }
        let (pw, ph) = (
            (lw * sf).round().max(1.0) as usize,
            (lh * sf).round().max(1.0) as usize,
        );
        let (mx, my) = if gtk_shelf {
            let outer = shelf_outer_pos(mon, corner, sw, sh);
            (outer.x / sf, outer.y / sf)
        } else {
            (pos.x as f64 / sf, pos.y as f64 / sf)
        };

        Ok(Self {
            win,
            imp,
            stack: Stack::new(corner),
            buf: vec![0; pw * ph],
            sink: RefCell::new(None),
            cursor: PhysicalPosition::new(0.0, 0.0),
            held: false,
            mods: ModifiersState::default(),
            mon_key: super::mon_key(mon),
            lw,
            lh,
            mx,
            my,
            scale: sf,
            font,
            accent,
            shown: false,
            fixed_shelf,
            full_monitor,
            winit_maps,
        })
    }

    fn id(&self) -> WindowId {
        self.win.id()
    }

    /// Settings → Appearance: retarget layer-shell anchor (Wayland) or
    /// stack layout (all paths).
    fn set_dock_corner(&mut self, corner: Corner) {
        self.stack.set_corner(corner);
        match &mut self.imp {
            Imp::Wl(w) => w.set_layer_corner(corner),
            Imp::Gtk(g) => g.set_layer_corner(corner),
            _ => {}
        }
    }

    fn mon_key(&self) -> &str {
        &self.mon_key
    }

    /// Store the host's event sink — `land` binds it once the real
    /// WindowId exists. RefCell because this is `&self`.
    fn set_sink(&self, sink: Rc<dyn Fn(ChromeEv)>) {
        *self.sink.borrow_mut() = Some(sink);
    }

    fn stack(&self) -> &Stack {
        &self.stack
    }

    fn stack_mut(&mut self) -> &mut Stack {
        &mut self.stack
    }

    /// Fly a finished shot into this monitor's stack.
    fn land(&mut self, l: Land) {
        let from = l
            .sel
            .map(|r| FRect {
                x: r.x as f64 - self.mx,
                y: r.y as f64 - self.my,
                w: r.w as f64,
                h: r.h as f64,
            })
            .unwrap_or(FRect {
                x: self.lw / 2.0 - 60.0,
                y: self.lh / 2.0 - 40.0,
                w: 120.0,
                h: 80.0,
            });
        let rm = l.instant || !self.full_monitor;
        let scale = self.scale;
        self.stack.land(l, from, rm, scale);
        if self.winit_maps {
            if !self.shown {
                self.win.set_visible(true);
                self.shown = true;
            }
            self.win.request_redraw();
        } else {
            self.present();
        }
    }

    /// Platform event → stack → optional host command. Redraw is handled
    /// by the caller (Chrome::event) via `redraw`.
    fn event(&mut self, ev: ChromeEv) -> Option<ChromeCmd> {
        let cmd = match ev {
            ChromeEv::Key(k) => self.stack.key(k),
            e => self.stack.pointer(e, self.lw, self.lh),
        };
        match cmd {
            // a real wl_data_source drag-out is a later phase — until
            // then the drag simply dies here instead of reaching the host
            Some(ChromeCmd::DragBegin { .. }) => None,
            c => c,
        }
    }

    fn redraw(&mut self) {
        self.present();
    }

    /// winit window event — translate to ChromeEv for the sink, repaint
    /// on request.
    fn window_event(&mut self, ev: &WindowEvent) -> bool {
        match ev {
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = *position;
                let (x, y) = (position.x / self.scale, position.y / self.scale);
                // held + moved = a pending drag; the stack applies the
                // 4px threshold so wobble doesn't escalate
                self.emit(if self.held {
                    ChromeEv::Dragged { x, y }
                } else {
                    ChromeEv::Move { x, y }
                });
            }
            WindowEvent::CursorLeft { .. } => {
                self.held = false;
                self.emit(ChromeEv::Leave);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (x, y) = (self.cursor.x / self.scale, self.cursor.y / self.scale);
                match (state, button) {
                    (ElementState::Pressed, MouseButton::Left) => {
                        // grab keyboard focus on first real interaction
                        // (a no-op on Wayland — the compositor focuses
                        // fullscreen surfaces itself)
                        self.win.focus_window();
                        self.held = true;
                        self.emit(ChromeEv::Down { x, y });
                    }
                    (ElementState::Released, MouseButton::Left) => {
                        self.held = false;
                        self.emit(ChromeEv::Up { x, y });
                    }
                    (ElementState::Pressed, MouseButton::Right) => {
                        self.win.focus_window();
                        self.emit(ChromeEv::RightDown { x, y });
                    }
                    _ => {}
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let key = match &event.logical_key {
                    WKey::Named(NamedKey::Escape) => Some(Key::Esc),
                    WKey::Named(NamedKey::ArrowLeft) => Some(Key::Left),
                    WKey::Named(NamedKey::ArrowRight) => Some(Key::Right),
                    WKey::Named(NamedKey::Enter) => Some(Key::Enter),
                    WKey::Named(NamedKey::Backspace) => Some(Key::Back),
                    WKey::Named(NamedKey::Space) => Some(Key::Space),
                    WKey::Character(c) if self.mods.control_key() && c.as_str() == "c" => {
                        Some(Key::Copy)
                    }
                    WKey::Character(c) if !self.mods.control_key() && c.as_str() == "o" => {
                        Some(Key::Open)
                    }
                    WKey::Character(c) if !self.mods.control_key() && c.as_str() == "s" => {
                        Some(Key::Share)
                    }
                    _ => None,
                };
                if let Some(k) = key {
                    self.emit(ChromeEv::Key(k));
                }
            }
            WindowEvent::RedrawRequested => {
                // Wayland: winit gates this event on wl_surface.frame —
                // it IS the vsync tick, so settle+present+chain. X11
                // gets the event promptly; the pump's timer drives it.
                if matches!(self.imp, Imp::Wl(_)) {
                    self.stack.settle(Instant::now());
                    self.present();
                    if self.stack.animating() {
                        self.win.request_redraw();
                    }
                } else {
                    self.present();
                }
            }
            _ => {}
        }
        true // the window exists; nothing else needs handling
    }

    /// Animation driver — Wayland ticks on the compositor's frame
    /// callback (winit throttles RedrawRequested to it), X11 has no
    /// reliable vsync so it keeps the 16ms timer.
    fn pump(&mut self) -> Option<Instant> {
        if let Imp::Wl(wl) = &mut self.imp {
            wl.dispatch();
            for ev in wl.take_inputs() {
                self.apply_wl_input(ev);
            }
        }
        let now = Instant::now();
        let was = self.stack.animating();
        self.stack.settle(now); // may kick off the idle-hide tween
        match &self.imp {
            Imp::Wl(w) if self.fixed_shelf && !w.is_live() => {
                // layer-shell still wiring — keep trying each tick
                Some(now + Duration::from_millis(16))
            }
            Imp::Wl(_) if self.fixed_shelf => {
                if was || self.stack.animating() || !self.shown {
                    self.present();
                }
                if self.stack.animating() {
                    Some(now + Duration::from_millis(16))
                } else {
                    self.stack.idle_deadline()
                }
            }
            Imp::Wl(_) => {
                if was || self.stack.animating() || !self.shown {
                    self.present();
                }
                if self.stack.animating() {
                    self.win.request_redraw();
                    Some(now + Duration::from_millis(16))
                } else {
                    self.stack.idle_deadline()
                }
            }
            Imp::GtkAlbum(_) | Imp::Gtk(_) | Imp::X11(_) => {
                // present while flying, plus the one settle that lands
                // on the end-state — else the tween freezes a frame shy
                if was || self.stack.animating() {
                    self.present();
                }
                if self.stack.animating() {
                    Some(now + Duration::from_millis(16))
                } else {
                    self.stack.idle_deadline()
                }
            }
        }
    }
}


impl Imp {
    fn new(
        win: &Window,
        mon: &MonitorHandle,
        corner: Corner,
        mlw: f64,
        mlh: f64,
        _sw: f64,
        _sh: f64,
        scale: f64,
    ) -> Result<Self> {
        match win
            .window_handle()
            .map_err(|e| anyhow!("chrome handle: {e}"))?
            .as_raw()
        {
            RawWindowHandle::Wayland(_h) => {
                let RawDisplayHandle::Wayland(dh) = win
                    .display_handle()
                    .map_err(|e| anyhow!("chrome display: {e}"))?
                    .as_raw()
                else {
                    anyhow::bail!("wayland surface without wayland display");
                };
                let display = dh.display.as_ptr();
                let out = mon.name();
                match Wl::new_layer_foreign(display, corner, mlw, mlh, scale, out.as_deref()) {
                    Ok(w) => {
                        log::info!("chrome: layer-shell on winit display");
                        return Ok(Self::Wl(w));
                    }
                    Err(e) => log::warn!("chrome: foreign layer: {e:#}"),
                }
                match Wl::new_layer(corner, mlw, mlh, scale, out.as_deref()) {
                    Ok(w) => return Ok(Self::Wl(w)),
                    Err(e) => log::warn!("chrome: env layer: {e:#}"),
                }
                anyhow::bail!("wayland chrome: gtk album should be created before winit window");
            }
            RawWindowHandle::Xlib(h) => X11::new(h.window as u32).map(Self::X11),
            RawWindowHandle::Xcb(h) => X11::new(h.window.get()).map(Self::X11),
            other => anyhow::bail!("chrome: unsupported linux display backend {other:?}"),
        }
    }

    fn layer_mode(&self) -> bool {
        matches!(self, Self::Wl(w) if w.layer_mode())
    }

    fn present(
        &mut self,
        buf: &[u32],
        pw: usize,
        ph: usize,
        rects: &[FRect],
        scale: f64,
        dirty: Option<FRect>,
        visible: bool,
    ) {
        match self {
            Self::Wl(w) => w.present(buf, pw, ph, rects, dirty),
            Self::Gtk(g) => g.present(buf, pw, ph, rects, scale, dirty),
            Self::GtkAlbum(g) => g.present(buf, pw, ph, rects, scale, dirty, visible),
            Self::X11(x) => x.present(buf, pw, ph, rects, scale, dirty),
        }
    }
}

mod wayland;
mod x11;
use wayland::{Wl, WlChromeInput};
use x11::X11;

