//! Transient status toast — a small undecorated always-on-top window that
//! reports long-running action progress (OCR) and auto-dismisses.

use anyhow::{Context, Result};
#[cfg(target_os = "macos")]
use block2::RcBlock;
use fontdue::Font;
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSAnimatablePropertyContainer, NSAnimationContext, NSView, NSWindow};
use qs_overlay::sdf::{fill_cov, rrect_sdf, stroke_cov, Canvas};
#[cfg(target_os = "macos")]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use softbuffer::{Context as SbContext, Surface};
use std::num::NonZeroU32;
#[cfg(target_os = "macos")]
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId, WindowLevel};

use crate::settings::system_font;
use crate::ui::{card_chrome, mark, Mark};
use crate::widgets::Txt;

const W: f64 = 300.0;
const H: f64 = 60.0;
const DONE_TTL: Duration = Duration::from_millis(1800);

#[derive(Clone, Copy, PartialEq)]
enum State {
    /// spinner running — an action is still in flight
    Working,
    Ok,
    Err,
}

pub struct Toast {
    win: Rc<Window>,
    surf: Surface<Rc<Window>, Rc<Window>>,
    font: Font,
    /// title weight — the bundled bold face, regular if it fails to load
    bold: Font,
    msg: String,
    accent: u32,
    state: State,
    pulse: f64,
    deadline: Option<Instant>,
    dirty: bool,
    /// native alpha fade-in runs once, after the first frame is on
    /// screen — an empty transparent window rising to full alpha would
    /// flash before the card exists
    #[cfg(target_os = "macos")]
    faded_in: bool,
    /// fade-out started at expiry; the host drops the window only after
    /// this deadline
    #[cfg(target_os = "macos")]
    closing: Option<Instant>,
}

impl Toast {
    /// Floating card docked at `pos` (a `config::POPUP_POS` key) on the
    /// primary monitor — winit exposes no global cursor position, so the
    /// dock can't chase the monitor the user is actually on.
    pub fn open(el: &ActiveEventLoop, msg: &str, accent: u32, pos: &str) -> Result<Self> {
        let attrs = Window::default_attributes()
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_inner_size(LogicalSize::new(W, H));
        let win = Rc::new(el.create_window(attrs).context("toast window")?);
        crate::widgets::round_window(&win, crate::widgets::popup_radius(qs_theme::radius::XL));
        crate::widgets::place_popup(&win, el, pos, W, H);
        let surf = SbContext::new(win.clone())
            .and_then(|c| Surface::new(&c, win.clone()))
            .map_err(|e| anyhow::anyhow!("toast surface: {e}"))?;
        let t = Self {
            win,
            surf,
            font: system_font()?,
            bold: bold_font().or_else(|_| system_font())?,
            msg: msg.to_string(),
            accent,
            state: State::Working,
            pulse: 0.0,
            deadline: None,
            dirty: true,
            #[cfg(target_os = "macos")]
            faded_in: false,
            #[cfg(target_os = "macos")]
            closing: None,
        };
        // start invisible; draw() fades in once the card is rendered
        #[cfg(target_os = "macos")]
        if let Some(w) = t.ns_window() {
            w.setAlphaValue(0.0);
        }
        Ok(t)
    }

    pub fn win_id(&self) -> WindowId {
        self.win.id()
    }

    /// AppKit window behind winit's view, for native alpha fades.
    #[cfg(target_os = "macos")]
    fn ns_window(&self) -> Option<Retained<NSWindow>> {
        let RawWindowHandle::AppKit(h) = self.win.window_handle().ok()?.as_raw() else {
            return None;
        };
        let view: &NSView = unsafe { &*(h.ns_view.as_ptr() as *const NSView) };
        view.window()
    }

    /// Animate the window's alpha through AppKit — WindowServer runs
    /// the fade, so it stays smooth even while the main thread works.
    #[cfg(target_os = "macos")]
    fn fade_window(&self, alpha: f64, secs: f64) {
        let Some(w) = self.ns_window() else {
            return;
        };
        let animator = w.animator();
        let block = RcBlock::new(move |ctx: NonNull<NSAnimationContext>| {
            unsafe { ctx.as_ref() }.setDuration(secs);
            animator.setAlphaValue(alpha);
        });
        NSAnimationContext::runAnimationGroup(&block);
    }

    /// Switch to the finished look; the toast lingers for `DONE_TTL`.
    pub fn done(&mut self, msg: String, ok: bool) {
        self.msg = msg;
        self.state = if ok { State::Ok } else { State::Err };
        self.deadline = Some(Instant::now() + DONE_TTL);
        self.dirty = true;
        // a late completion can land while the window is fading out
        #[cfg(target_os = "macos")]
        if self.closing.take().is_some() {
            self.fade_window(1.0, 0.12);
        }
        self.win.request_redraw();
    }

    pub fn expired(&self) -> bool {
        self.deadline.is_some_and(|d| Instant::now() >= d)
    }

    /// Called from the host's `about_to_wait`. Returns the next wake instant
    /// while alive, `None` once expired — the host should then drop the toast.
    pub fn pump(&mut self) -> Option<Instant> {
        if self.expired() {
            // let AppKit fade the window before the host drops it
            #[cfg(target_os = "macos")]
            {
                let now = Instant::now();
                if self.closing.is_none() {
                    self.fade_window(0.0, 0.14);
                    self.closing = Some(now + Duration::from_millis(150));
                }
                if let Some(end) = self.closing {
                    if now < end {
                        return Some(end);
                    }
                }
            }
            return None;
        }
        if self.dirty || self.state == State::Working || self.deadline.is_some() {
            self.win.request_redraw();
        }
        match self.state {
            State::Working => Some(Instant::now() + Duration::from_millis(60)),
            _ => Some(Instant::now() + Duration::from_millis(100)),
        }
    }

    /// returns true when the window should be torn down
    pub fn window_event(&mut self, ev: WindowEvent) -> bool {
        match ev {
            WindowEvent::CloseRequested => true,
            WindowEvent::RedrawRequested => {
                self.draw();
                false
            }
            _ => false,
        }
    }

    fn draw(&mut self) {
        let working = self.state == State::Working;
        self.pulse += 0.12;
        self.dirty = working;

        let size = self.win.inner_size();
        let (sw, sh) = match (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
            (Some(w), Some(h)) => (w.get() as usize, h.get() as usize),
            _ => return,
        };
        let _ = self.surf.resize(
            NonZeroU32::new(sw as u32).unwrap(),
            NonZeroU32::new(sh as u32).unwrap(),
        );
        let Ok(mut buf) = self.surf.buffer_mut() else {
            return;
        };
        buf.fill(0);
        let scale = self.win.scale_factor();
        let mut cv = Canvas {
            buf: &mut buf,
            w: sw,
            h: sh,
            scale,
        };

        // softbuffer ignores per-pixel alpha — the card must fill the whole
        // window or the uncovered corners render as an opaque black frame.
        card_chrome(
            &mut cv,
            W,
            H,
            crate::widgets::popup_radius(qs_theme::radius::XL),
            qs_theme::pal().popover,
            qs_theme::pal().popover_border,
            qs_theme::pal().fringe,
        );

        // macOS-style notification: tinted icon tile, app name, message
        let p = qs_theme::pal();
        let (cx, cy) = (14.0 + 16.0, H / 2.0);
        let tint = match self.state {
            State::Working => self.accent,
            State::Ok => p.success,
            State::Err => p.danger,
        };
        tile(&mut cv, cx, cy, 16.0, 9.0, tint, if p.dark { 0.24 } else { 0.14 });
        match self.state {
            State::Working => spinner(&mut cv, cx, cy, 7.0, 2.0, self.pulse, tint),
            State::Ok => mark(&mut cv, Mark::Tick, cx, cy, 2.2, tint),
            State::Err => mark(&mut cv, Mark::Cross, cx, cy, 2.2, tint),
        }

        let tx = cx + 16.0 + 12.0;
        let max_w = W - tx - 16.0;
        let title = Txt {
            font: &self.bold,
            scale,
        };
        title.draw(&mut cv, tx, 25.0, 12.0, "Quickshort", p.fg);
        // the card is fixed-width, so long text must be ellipsized or it
        // spills past the right edge
        let t = Txt {
            font: &self.font,
            scale,
        };
        let msg = ellipsize(&t, 13.0, &self.msg, max_w);
        t.draw(&mut cv, tx, 43.0, 13.0, &msg, if working { p.fg_dim } else { p.fg });

        let _ = buf.present();
        #[cfg(target_os = "macos")]
        if !self.faded_in {
            self.faded_in = true;
            self.fade_window(1.0, 0.16);
        }
    }
}

/// Dev harness: `quickshort --toast` shows a working->done toast cycle.
pub fn standalone() -> Result<()> {
    use winit::application::ApplicationHandler;
    use winit::event_loop::{ControlFlow, EventLoop};

    struct Run {
        t: Option<Toast>,
        t0: Instant,
    }
    impl ApplicationHandler for Run {
        fn resumed(&mut self, el: &ActiveEventLoop) {
            let cfg = crate::config::Config::load();
            match Toast::open(el, "Extracting text…", cfg.accent_rgb(), &cfg.popup_pos) {
                Ok(t) => self.t = Some(t),
                Err(e) => {
                    eprintln!("toast: {e:#}");
                    el.exit();
                }
            }
        }
        fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, ev: WindowEvent) {
            if let Some(t) = &mut self.t {
                if t.win_id() == id && t.window_event(ev) {
                    self.t = None;
                    el.exit();
                }
            }
        }
        fn about_to_wait(&mut self, el: &ActiveEventLoop) {
            if self.t0.elapsed() > Duration::from_millis(900) {
                if let Some(t) = &mut self.t {
                    if t.deadline.is_none() {
                        t.done("Copied 683 chars".to_string(), true);
                    }
                }
            }
            match self.t.as_mut().and_then(|t| t.pump()) {
                Some(wake) => el.set_control_flow(ControlFlow::WaitUntil(wake)),
                None => el.exit(),
            }
        }
    }
    let el = EventLoop::new()?;
    let mut run = Run {
        t: None,
        t0: Instant::now(),
    };
    el.run_app(&mut run)?;
    Ok(())
}

// --- tiny SDF helpers (logical coords, self-contained) ----------------------

fn bold_font() -> Result<Font> {
    const BOLD: &[u8] = include_bytes!("../../qs-render/fonts/BeVietnamPro-Bold.ttf");
    Font::from_bytes(BOLD, fontdue::FontSettings::default()).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Rounded-square icon tile, `half` px from centre to edge, tinted `a`.
fn tile(cv: &mut Canvas, cx: f64, cy: f64, half: f64, r: f64, rgb: u32, a: f64) {
    let s = cv.scale;
    for py in ((cy - half - 1.0) * s) as i32..=((cy + half + 1.0) * s) as i32 {
        for px in ((cx - half - 1.0) * s) as i32..=((cx + half + 1.0) * s) as i32 {
            let (lx, ly) = ((px as f64 + 0.5) / s, (py as f64 + 0.5) / s);
            let c = fill_cov(rrect_sdf(lx, ly, cx, cy, half, half, r), s);
            if c > 0.0 {
                cv.blend(px, py, rgb, c * a);
            }
        }
    }
}

/// Activity spinner: a 270° arc rotating with `phase`.
fn spinner(cv: &mut Canvas, cx: f64, cy: f64, r: f64, w: f64, phase: f64, rgb: u32) {
    use std::f64::consts::TAU;
    let s = cv.scale;
    let start = (phase * 0.9).rem_euclid(TAU);
    let reach = r + w;
    for py in ((cy - reach) * s) as i32..=((cy + reach) * s) as i32 {
        for px in ((cx - reach) * s) as i32..=((cx + reach) * s) as i32 {
            let (dx, dy) = ((px as f64 + 0.5) / s - cx, (py as f64 + 0.5) / s - cy);
            let along = (dy.atan2(dx) - start).rem_euclid(TAU) / TAU;
            if along > 0.75 {
                continue;
            }
            let c = stroke_cov(dx.hypot(dy) - r, w, s);
            if c > 0.0 {
                // tail fades in so the arc reads as motion
                cv.blend(px, py, rgb, c * (0.25 + 0.75 * along / 0.75));
            }
        }
    }
}

/// Trim `s` with a trailing "…" until it fits `max_w` logical px.
fn ellipsize(t: &Txt, size: f64, s: &str, max_w: f64) -> String {
    if t.width(size, s) <= max_w {
        return s.to_string();
    }
    let mut out = String::new();
    for ch in s.chars() {
        if t.width(size, &format!("{out}{ch}…")) > max_w {
            break;
        }
        out.push(ch);
    }
    format!("{out}…")
}
