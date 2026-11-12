//! Shared softbuffer widgets for the small always-on-top windows
//! (toast, quick-access card, recording stop). fontdue rasterization
//! and the card/pill SDF shapes live here once — every popup draws
//! itself on top of these.

use fontdue::Font;
use qs_overlay::sdf::Canvas;
use winit::dpi::PhysicalPosition;
use winit::monitor::MonitorHandle;

/// The monitor popups anchor to — the primary display. winit doesn't
/// expose the global cursor position at window-creation time, so this
/// is the stable, predictable choice.
pub fn popup_monitor(el: &winit::event_loop::ActiveEventLoop) -> Option<MonitorHandle> {
    el.primary_monitor().or_else(|| {
        let mut mons = el.available_monitors();
        mons.next()
    })
}

/// Physical-px outer position that docks a `w`×`h` logical popup at
/// `pos` (a key from `crate::config::POPUP_POS`) inside monitor `m`.
/// Unknown keys fall back to top-right.
/// Dock a popup window — winit leaves new windows at the screen center
/// when no position is set, which reads as a "middle toast" on GNOME.
pub fn place_popup(
    win: &winit::window::Window,
    el: &winit::event_loop::ActiveEventLoop,
    pos: &str,
    w: f64,
    h: f64,
) {
    let at = popup_monitor(el)
        .map(|m| dock_pos(pos, &m, w, h))
        .unwrap_or(PhysicalPosition::new(40.0, 40.0));
    let _ = win.set_outer_position(at);
}

pub fn dock_pos(pos: &str, m: &MonitorHandle, w: f64, h: f64) -> PhysicalPosition<f64> {
    let pos = if crate::config::POPUP_POS.iter().any(|(k, _)| *k == pos) {
        pos
    } else {
        "top-right"
    };
    let sf = m.scale_factor();
    let mp = m.position();
    let ms = m.size();
    let side = 20.0 * sf;
    // macOS keeps a menu bar pinned to the top edge of every screen
    let top = if cfg!(target_os = "macos") {
        44.0 * sf
    } else {
        20.0 * sf
    };
    let x = if pos.ends_with("left") {
        mp.x as f64 + side
    } else if pos.ends_with("right") {
        mp.x as f64 + ms.width as f64 - w * sf - side
    } else {
        mp.x as f64 + (ms.width as f64 - w * sf) / 2.0
    };
    let y = if pos.starts_with("top") {
        mp.y as f64 + top
    } else {
        mp.y as f64 + ms.height as f64 - h * sf - side
    };
    PhysicalPosition::new(x, y)
}

/// Clip a borderless popup window to its rounded-card shape. softbuffer
/// presents an opaque CGImage on macOS (alpha byte skipped), so without
/// a layer mask the space outside the card's rounded corners renders as
/// a solid black rectangle behind it.
#[cfg(target_os = "macos")]
pub fn round_window(win: &winit::window::Window, radius: f64) {
    use objc2_app_kit::NSView;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(h) = win.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(h) = h.as_raw() else {
        return;
    };
    let view: &NSView = unsafe { &*(h.ns_view.as_ptr() as *const NSView) };
    view.setWantsLayer(true);
    if let Some(layer) = view.layer() {
        layer.setCornerRadius(radius);
        layer.setMasksToBounds(true);
    }
}

/// Same clip on Windows: softbuffer's DIB is opaque too, so the window
/// region is rounded with SetWindowRgn instead.
#[cfg(target_os = "windows")]
pub fn round_window(win: &winit::window::Window, radius: f64) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};
    let Ok(h) = win.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(h) = h.as_raw() else {
        return;
    };
    let hwnd = HWND(h.hwnd.get() as *mut _);
    let size = win.inner_size();
    let r = (radius * win.scale_factor()).round() as i32;
    unsafe {
        let rgn = CreateRoundRectRgn(
            0,
            0,
            size.width as i32 + 1,
            size.height as i32 + 1,
            r * 2,
            r * 2,
        );
        // SetWindowRgn takes ownership of the region — never delete it
        let _ = SetWindowRgn(hwnd, Some(rgn), true);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn round_window(_win: &winit::window::Window, _radius: f64) {}

/// Symbols Be Vietnam Pro lacks (⌘ ⇧ ⌥ ⌃ …). Looked up once, from the
/// platform's symbol-capable system fonts; `None` if none is installed.
fn symbol_font() -> Option<&'static Font> {
    static F: std::sync::OnceLock<Option<Font>> = std::sync::OnceLock::new();
    F.get_or_init(|| {
        const CANDIDATES: &[&str] = &[
            #[cfg(target_os = "macos")]
            "/System/Library/Fonts/Apple Symbols.ttf",
            #[cfg(target_os = "macos")]
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
            #[cfg(target_os = "windows")]
            "C:\\Windows\\Fonts\\seguisym.ttf",
            #[cfg(target_os = "windows")]
            "C:\\Windows\\Fonts\\segoeui.ttf",
            #[cfg(target_os = "linux")]
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            #[cfg(target_os = "linux")]
            "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
            #[cfg(target_os = "linux")]
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
        ];
        CANDIDATES.iter().find_map(|p| {
            let bytes = std::fs::read(p).ok()?;
            Font::from_bytes(bytes.as_slice(), fontdue::FontSettings::default()).ok()
        })
    })
    .as_ref()
}

/// The face to draw `ch` with: `primary` when it has the glyph, else the
/// symbol fallback, else `primary` (draws its .notdef box, as before).
pub fn face_for(primary: &Font, ch: char) -> &Font {
    if ch.is_ascii() || primary.lookup_glyph_index(ch) != 0 {
        return primary;
    }
    match symbol_font() {
        Some(f) if f.lookup_glyph_index(ch) != 0 => f,
        _ => primary,
    }
}
/// Text rasterizer: `x`/`y` in logical px, `y` = glyph baseline.
pub struct Txt<'a> {
    pub font: &'a Font,
    pub scale: f64,
}

impl Txt<'_> {
    pub fn draw(&self, cv: &mut Canvas, x: f64, y: f64, size: f64, s: &str, rgb: u32) {
        let px = size * self.scale;
        let mut pen = x * self.scale;
        let baseline = y * self.scale;
        for ch in s.chars() {
            let (m, bmp) = face_for(self.font, ch).rasterize(ch, px as f32);
            let gx = (pen + m.xmin as f64) as i32;
            let gy = (baseline - m.height as f64 - m.ymin as f64) as i32;
            for row in 0..m.height {
                for col in 0..m.width {
                    let a = bmp[row * m.width + col] as f64 / 255.0;
                    if a > 0.0 {
                        cv.blend(gx + col as i32, gy + row as i32, rgb, a);
                    }
                }
            }
            pen += m.advance_width as f64;
        }
    }

    pub fn width(&self, size: f64, s: &str) -> f64 {
        let px = (size * self.scale) as f32;
        s.chars()
            .map(|c| face_for(self.font, c).metrics(c, px).advance_width)
            .sum::<f32>() as f64
            / self.scale
    }

    /// Same as `draw` but composites onto a premultiplied-alpha surface
    /// (the chrome layer) — see `Canvas::blend_over`.
    pub fn draw_over(&self, cv: &mut Canvas, x: f64, y: f64, size: f64, s: &str, rgb: u32) {
        let px = size * self.scale;
        let mut pen = x * self.scale;
        let baseline = y * self.scale;
        for ch in s.chars() {
            let (m, bmp) = face_for(self.font, ch).rasterize(ch, px as f32);
            let gx = (pen + m.xmin as f64) as i32;
            let gy = (baseline - m.height as f64 - m.ymin as f64) as i32;
            for row in 0..m.height {
                for col in 0..m.width {
                    let a = bmp[row * m.width + col] as f64 / 255.0;
                    if a > 0.0 {
                        cv.blend_over(gx + col as i32, gy + row as i32, rgb, a);
                    }
                }
            }
            pen += m.advance_width as f64;
        }
    }
}

#[cfg(test)]
mod face_tests {
    use super::*;

    fn bundled() -> Font {
        let bytes = include_bytes!("../../qs-render/fonts/BeVietnamPro-Regular.ttf");
        Font::from_bytes(bytes.as_slice(), fontdue::FontSettings::default()).unwrap()
    }

    #[test]
    fn ascii_stays_on_the_primary_face() {
        let f = bundled();
        assert!(std::ptr::eq(face_for(&f, 'S'), &f));
    }

    #[test]
    fn modifier_symbols_fall_back_when_a_symbol_font_exists() {
        let f = bundled();
        if symbol_font().is_none() {
            eprintln!("no symbol font installed — skipped");
            return;
        }
        for ch in ['⌘', '⇧', '⌥', '⌃'] {
            assert_eq!(
                f.lookup_glyph_index(ch),
                0,
                "{ch} is not in the bundled font"
            );
            let (m, bmp) = face_for(&f, ch).rasterize(ch, 14.0);
            assert!(
                m.width > 0 && bmp.iter().any(|&a| a > 0),
                "{ch} drew nothing"
            );
        }
    }
}
