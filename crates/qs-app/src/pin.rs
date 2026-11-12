//! Pin-to-screen: an always-on-top floating window showing a capture for
//! reference while the user works. Undecorated; drag anywhere to move,
//! the corner × closes it.

use crate::ui::close_button;
use anyhow::{Context, Result};
use image::RgbaImage;
use qs_overlay::sdf::Canvas;
use softbuffer::{Context as SbContext, Surface};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId, WindowLevel};

const MAX_W: f64 = 560.0;
const MAX_H: f64 = 400.0;
const CLOSE: f64 = 9.0; // × circle radius
const CLOSE_PAD: f64 = 4.0;

pub struct Pin {
    win: Rc<Window>,
    surf: Surface<Rc<Window>, Rc<Window>>,
    /// display-sized ARGB pixels + dims — scaling happens once at open
    px: Vec<u32>,
    pw: usize,
    ph: usize,
    hover_close: bool,
}

impl Pin {
    pub fn open(el: &ActiveEventLoop, img: &RgbaImage) -> Result<Self> {
        let (iw, ih) = (img.width() as f64, img.height() as f64);
        let s = (MAX_W / iw).min(MAX_H / ih).min(1.0);
        let (pw, ph) = ((iw * s).max(40.0) as usize, (ih * s).max(40.0) as usize);
        let attrs = Window::default_attributes()
            .with_decorations(false)
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_inner_size(LogicalSize::new(pw as f64, ph as f64));
        let win = Rc::new(el.create_window(attrs).context("pin window")?);
        // centre of the primary monitor — stacking more pins is the
        // compositor's business
        if let Some(m) = el
            .primary_monitor()
            .or_else(|| el.available_monitors().next())
        {
            let p = m.position();
            let ms = m.size();
            let x = p.x as f64 + (ms.width as f64 - pw as f64) / 2.0;
            let y = p.y as f64 + (ms.height as f64 - ph as f64) / 2.0;
            win.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
        }
        let surf = SbContext::new(win.clone())
            .and_then(|c| Surface::new(&c, win.clone()))
            .map_err(|e| anyhow::anyhow!("pin surface: {e}"))?;
        Ok(Self {
            px: scale_argb(img, pw, ph),
            pw,
            ph,
            win,
            surf,
            hover_close: false,
        })
    }

    pub fn win_id(&self) -> WindowId {
        self.win.id()
    }

    /// `true` when the host should drop the pin.
    pub fn window_event(&mut self, ev: WindowEvent) -> bool {
        match ev {
            WindowEvent::CloseRequested => true,
            WindowEvent::RedrawRequested => {
                self.draw();
                false
            }
            WindowEvent::CursorMoved { position, .. } => {
                let sf = self.win.scale_factor();
                let hot = is_close(position.x / sf, position.y / sf, self.pw);
                if hot != self.hover_close {
                    self.hover_close = hot;
                    self.win.request_redraw();
                }
                false
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if self.hover_close {
                    return true;
                }
                // anywhere but the ×: drag the window
                let _ = self.win.drag_window();
                false
            }
            _ => false,
        }
    }

    fn draw(&mut self) {
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
        let scale = self.win.scale_factor();
        // image blit — the physical buffer maps back to logical px coords
        for y in 0..sh {
            let sy = ((y as f64 / scale) as usize).min(self.ph - 1);
            for x in 0..sw {
                let sx = ((x as f64 / scale) as usize).min(self.pw - 1);
                buf[y * sw + x] = self.px[sy * self.pw + sx];
            }
        }
        let mut cv = Canvas {
            buf: &mut buf,
            w: sw,
            h: sh,
            scale,
        };
        // thin border ring
        let (lw, lh) = (self.pw as f64, self.ph as f64);
        for py in 0..sh as i32 {
            for px in 0..sw as i32 {
                let lx = px as f64 / scale;
                let ly = py as f64 / scale;
                if lx < 0.6 || ly < 0.6 || lx >= lw - 0.6 || ly >= lh - 0.6 {
                    cv.blend(px, py, qs_theme::pal().popover_border, 0.9);
                }
            }
        }
        // close ×, dim circle + white cross (hot = red tint)
        let (cx, cy) = (self.pw as f64 - CLOSE - CLOSE_PAD, CLOSE + CLOSE_PAD);
        let bg = if self.hover_close {
            qs_theme::pal().danger
        } else {
            qs_theme::pal().control
        };
        close_button(&mut cv, cx, cy, CLOSE, bg, 0.85, 1.4);
        let _ = buf.present();
    }
}

fn is_close(x: f64, y: f64, w: usize) -> bool {
    let (cx, cy) = (w as f64 - CLOSE - CLOSE_PAD, CLOSE + CLOSE_PAD);
    (x - cx).powi(2) + (y - cy).powi(2) <= (CLOSE + 2.0) * (CLOSE + 2.0)
}

/// Nearest-neighbour downscale to display size, ARGB without alpha.
fn scale_argb(img: &RgbaImage, w: usize, h: usize) -> Vec<u32> {
    let (iw, ih) = (img.width() as usize, img.height() as usize);
    let mut out = vec![0u32; w * h];
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel((x * iw / w) as u32, (y * ih / h) as u32);
            out[y * w + x] = ((p[0] as u32) << 16) | ((p[1] as u32) << 8) | p[2] as u32;
        }
    }
    out
}
