//! The album stack — per-monitor shot model, layout/hit-test
//! state machine and its painting (`render.rs`). `chrome.rs` keeps
//! window orchestration and the platform seam.

use super::*;

mod geometry;
mod input;
mod render;
use render::{make_thumb, pack_image};

/* ---- layout constants (logical px) ---- */
const TW: f64 = 118.0;
const TH: f64 = 74.0;
const MARGIN: f64 = 20.0;
const STACK_DX: f64 = 6.0; // depth offset per older shot
const STACK_DY: f64 = 8.0;
const FAN: f64 = 128.0; // hover/pinned fan spacing
const MAX_VIS: usize = 5;
const MAX_VIS_PINNED: usize = 8;
const FLY_MS: f64 = 420.0;
const FAN_OPEN_MS: f64 = 280.0;
const FAN_CLOSE_MS: f64 = 180.0;
const BOX_MS: f64 = 200.0;
/// idle before the stack slides into the edge
const HIDE_AFTER_MS: f64 = 3000.0;
const HIDE_MS: f64 = 280.0; // slide-away
const UNHIDE_MS: f64 = 200.0; // slide-back
                              // thumbs slide fully off-screen when hidden — the edge-docked chip
                              // is the grab handle (a sliver + chip read as two disjoint artifacts)

/// Corner shelf panel — wide enough for a pinned fan, short enough not
/// to read as a fullscreen overlay (Linux/macOS compact chrome).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn shelf_logical_size() -> (f64, f64) {
    let w = MARGIN * 2.0 + TW + (MAX_VIS_PINNED as f64 - 1.0) * FAN;
    let h = MARGIN * 2.0 + TH + FAN + 48.0; // chip + clear pill
    (w, h)
}

/* ---- ui-2026 tokens ---- */

/// What produced the shot — drives the corner badge.
#[derive(Clone, Copy, PartialEq)]
pub enum ShotKind {
    Saved,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Copied,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    Ocr,
    Shared,
    Scroll,
    Record,
}

pub struct Shot {
    /// full-resolution capture, kept for Copy/Pin/Reveal actions
    pub img: Rc<RgbaImage>,
    /// premul-packed thumbnail pixels (physical px)
    thumb: Vec<u32>,
    /// thumbnail buffer dims in physical px
    tw: usize,
    th: usize,
    /// on disk already → Reveal; None → unsaved (Copy-only provenance)
    pub path: Option<PathBuf>,
    kind: ShotKind,
    /// seconds recorded — the ▶ thumb gets a `m:ss` duration badge
    pub dur_s: Option<u64>,
    /// live share link for this shot — draws the 🔗 chip and enables
    /// the "Revoke Link" menu row; set/cleared by the host
    pub shared: Option<crate::share::ShareEntry>,
}

impl Shot {
    /// Premul-packed thumbnail pixels + dims (physical px) — the macOS
    /// backend uses it as the drag-session image (Linux/Windows drag the
    /// file itself).
    #[cfg(target_os = "macos")]
    pub fn thumb(&self) -> (&[u32], usize, usize) {
        (&self.thumb, self.tw, self.th)
    }

    /// Library provenance for this shelf item. A recording and a scrolling
    /// capture say so; the rest cannot be told apart here, so they stay
    /// unknown rather than being guessed.
    pub fn capture_mode(&self) -> qs_library::CaptureMode {
        match self.kind {
            ShotKind::Scroll => qs_library::CaptureMode::Scrolling,
            ShotKind::Record => qs_library::CaptureMode::Recording,
            _ => qs_library::CaptureMode::Unknown,
        }
    }
}

#[derive(Clone, Copy)]
pub struct FRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

struct Fly {
    /// premul-packed full-res pixels — packed once at land-time,
    /// blitted scaled each frame (packing per frame was the jank)
    packed: Vec<u32>,
    iw: usize,
    ih: usize,
    from: FRect,
    t0: Instant,
}

/// Hit-test roles; `rects` maps these to screen areas each frame.
#[derive(Clone, Copy, PartialEq)]
pub enum Hit {
    /// stack thumbnail `i` (0 = newest)
    Thumb(usize),
    /// the × button on thumb `i`
    Del(usize),
    /// "N SHOTS" count chip — toggles the pinned fan
    Chip,
    /// the fanned row's "clear all" pill (Notification-Centre style)
    ClearAll,
    /// context-menu row `i`
    MenuRow(usize),
    /// menu panel background (swallows clicks, no action)
    MenuPanel,
    /// lightbox backdrop (click = close)
    Backdrop,
    /// lightbox action button `i` (BOX_ACTIONS order)
    BoxAction(usize),
}

/// What the host must run in response to a chrome click/key.
#[derive(Debug)]
pub enum ChromeCmd {
    /// state changed — repaint the window
    Redraw,
    /// a context-menu row was picked — `id` is a MENU_* const
    MenuAction {
        idx: usize,
        id: &'static str,
    },
    Remove(usize),
    /// dismiss the whole stack (files on disk are untouched)
    ClearAll,
    /// thumb dragged out — the backend that owns the window starts the
    /// real file-promise drag; consumed locally, never reaches the host
    DragBegin {
        idx: usize,
    },
}

/// Menu row ids — drawn in-chrome, stable across backends.
pub const MENU_REVEAL: &str = "qs-chrome.reveal";
pub const MENU_COPY: &str = "qs-chrome.copy";
pub const MENU_SAVE: &str = "qs-chrome.save";
pub const MENU_PIN: &str = "qs-chrome.pin";
pub const MENU_BEAUTIFY: &str = "qs-chrome.beautify";
pub const MENU_SHARE: &str = "qs-chrome.share";
pub const MENU_REVOKE: &str = "qs-chrome.revoke";
pub const MENU_DEL: &str = "qs-chrome.del";
pub const MENU_CLEAR: &str = "qs-chrome.clear";

/// Lightbox action ids — same MENU_* namespace so host handling routes
/// through the existing on_chrome_menu match.
pub const MENU_OPEN: &str = "qs-chrome.open";
const BOX_ACTIONS: &[&str] = &[MENU_COPY, MENU_OPEN, MENU_SHARE, MENU_DEL];
const BOX_BTN_W: f64 = 104.0;
const BOX_BTN_H: f64 = 32.0;
const BOX_BTN_GAP: f64 = 10.0;

/// Which screen corner the stack docks to.
#[derive(Clone, Copy, PartialEq)]
pub enum Corner {
    TL,
    TC,
    TR,
    BL,
    BC,
    BR,
}

impl Corner {
    pub fn from_popup_pos(pos: &str) -> Self {
        match pos {
            "top-left" => Self::TL,
            "top-center" => Self::TC,
            "bottom-left" => Self::BL,
            "bottom-center" => Self::BC,
            "bottom-right" => Self::BR,
            _ => Self::TR,
        }
    }
    pub(super) fn right(self) -> bool {
        matches!(self, Self::TR | Self::BR)
    }
    pub(super) fn bottom(self) -> bool {
        matches!(self, Self::BL | Self::BC | Self::BR)
    }
}

/// In-chrome context menu — a native popup can't position inside the
/// chrome surface (muda needs an NSView/HWND/gtk::Window per-OS), so the
/// menu is pixels in the same layer: one path on every backend.
struct Menu {
    /// thumb index the menu is anchored to (0 = newest)
    idx: usize,
    anchor: FRect,
    /// (MENU_* id, enabled)
    rows: Vec<(&'static str, bool)>,
}

const MENU_ROW_H: f64 = 28.0;
const MENU_PAD: f64 = 6.0;
const MENU_W: f64 = 210.0;

/// Per-window stack state — pure logic, platform code only forwards
/// events and asks `render` for pixels.
pub struct Stack {
    pub shots: Vec<Shot>, // newest LAST
    fly: Option<Fly>,
    /// newest thumb is reserved but withheld from the canvas while a
    /// native (Core Animation) fly layer carries it in — macOS only;
    /// the software path leaves this false and draws its own flight
    native_flight: bool,
    hover: Option<Hit>,
    /// cursor within the wider retention zone — hover hysteresis so a
    /// moving rect can't slide out from under a resting cursor (fan
    /// closes → thumb returns under the cursor → opens → jitter loop)
    inside: bool,
    /// stacked↔fanned blend — time-based tween toward `fan_to`
    fan_t: f64,
    fan_from: f64,
    fan_to: f64,
    fan_t0: Instant,
    fan_anim: bool,
    /// shown↔edge-hidden blend — after idle the stack slides into the
    /// screen edge — the docked chip is the only visible handle
    hide_t: f64,
    hide_from: f64,
    hide_to: f64,
    hide_t0: Instant,
    hide_anim: bool,
    /// last user activity — the idle countdown for edge-hiding
    idle_since: Instant,
    /// keyboard selection ring (macshot model — hover syncs it)
    ring: Option<usize>,
    pub pinned: bool,
    lightbox: Option<usize>,
    /// lightbox fade/scale-in start + its packed pixels (cached at open)
    box_t0: Option<Instant>,
    box_img: Vec<u32>,
    box_iw: usize,
    box_ih: usize,
    menu: Option<Menu>,
    /// a held left-press on a thumb — position + index; `Up` resolves
    /// it to a click, `Dragged` past the threshold to a drag-out
    press: Option<(f64, f64, usize)>,
    corner: Corner,
}
impl Stack {
    pub fn new(corner: Corner) -> Self {
        Self {
            shots: Vec::new(),
            fly: None,
            native_flight: false,
            hover: None,
            inside: false,
            fan_t: 0.0,
            fan_from: 0.0,
            fan_to: 0.0,
            fan_t0: Instant::now(),
            fan_anim: false,
            hide_t: 0.0,
            hide_from: 0.0,
            hide_to: 0.0,
            hide_t0: Instant::now(),
            hide_anim: false,
            idle_since: Instant::now(),
            ring: None,
            pinned: false,
            lightbox: None,
            box_t0: None,
            box_img: Vec::new(),
            box_iw: 0,
            box_ih: 0,
            menu: None,
            press: None,
            corner,
        }
    }

    pub(super) fn set_corner(&mut self, corner: Corner) {
        self.corner = corner;
    }

    /// Lightbox or in-chrome menu — layer-shell needs exclusive keyboard.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(super) fn modal_open(&self) -> bool {
        self.lightbox.is_some() || self.menu.is_some()
    }

    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(super) fn dismiss_menu(&mut self) -> bool {
        let was = self.menu.is_some();
        self.menu = None;
        was
    }


    /// Queue the fly-in; `from` is the selection rect in window-local
    /// logical coords. `rm` (reduce motion) lands instantly.
    pub fn land(&mut self, l: Land, from: FRect, rm: bool, scale: f64) {
        self.touch(); // a landing shot is activity — reveal the stack
        let img = Rc::new(l.img);
        let (thumb, tw, th) = make_thumb(&img, l.kind, scale);
        let shot = Shot {
            img: img.clone(),
            thumb,
            tw,
            th,
            path: l.path,
            kind: l.kind,
            dur_s: l.dur_s,
            shared: l.shared,
        };
        if rm {
            self.shots.push(shot);
        } else {
            self.fly = Some(Fly {
                iw: img.width() as usize,
                ih: img.height() as usize,
                packed: pack_image(&img),
                from,
                t0: Instant::now(),
            });
            self.shots.push(shot); // reserve the slot — drawn at the landing point
        }
    }


    pub fn is_empty(&self) -> bool {
        self.shots.is_empty() && self.fly.is_none()
    }


    /// Withhold the newest thumb from the canvas while a native flight
    /// layer carries it in; the reserved slot still counts in the chip
    /// and paint bounds, so landing swaps in cleanly.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn hold_newest(&mut self, hold: bool) {
        self.native_flight = hold;
    }


    /// Retarget the fan tween whenever hover/pinned flips — opening
    /// overshoots slightly (ease-out-back, ~280ms) like a native
    /// popover; closing is a quicker plain ease-out (~180ms).
    fn retarget_fan(&mut self) {
        let t = if self.pinned || self.inside { 1.0 } else { 0.0 };
        if t != self.fan_to {
            self.fan_from = self.fan_t;
            self.fan_to = t;
            self.fan_t0 = Instant::now();
            self.fan_anim = true;
        }
    }


    /// Any user activity resets the idle clock and pulls a hidden
    /// stack back out of the edge.
    fn touch(&mut self) {
        self.idle_since = Instant::now();
        if self.hide_to != 0.0 {
            self.hide_from = self.hide_t;
            self.hide_to = 0.0;
            self.hide_t0 = self.idle_since;
            self.hide_anim = true;
        }
    }


    /// Next wake for the idle-hide countdown — the host's pump sleeps
    /// until this when nothing is animating. None while hidden.
    pub fn idle_deadline(&self) -> Option<Instant> {
        if self.hide_to == 0.0
            && !self.inside
            && !self.pinned
            && self.lightbox.is_none()
            && !self.shots.is_empty()
        {
            Some(self.idle_since + Duration::from_millis(HIDE_AFTER_MS as u64))
        } else {
            None
        }
    }


    /// Fly + fan + lightbox + hide progress → true while frames are needed.
    pub fn animating(&self) -> bool {
        self.fly.is_some() || self.fan_anim || self.hide_anim || self.box_t0.is_some()
    }


    /// Open the lightbox on thumb `i` with a fade/scale-in — packs the
    /// shot's pixels once so frames only blit.
    fn open_lightbox(&mut self, i: usize) {
        if let Some(s) = self.shot(i) {
            let (p, w, h) = (
                pack_image(&s.img),
                s.img.width() as usize,
                s.img.height() as usize,
            );
            self.box_img = p;
            self.box_iw = w;
            self.box_ih = h;
        }
        self.lightbox = Some(i);
        self.box_t0 = Some(Instant::now());
    }


    fn fly_t(&self, now: Instant) -> f64 {
        self.fly
            .as_ref()
            .map(|f| (now.duration_since(f.t0).as_secs_f64() * 1000.0 / FLY_MS).min(1.0))
            .unwrap_or(1.0)
    }


    /// Advance animations one tick — drop finished flights, run the
    /// fan tween and the lightbox fade-in to their ends.
    pub fn settle(&mut self, now: Instant) {
        if self.fly_t(now) >= 1.0 {
            self.fly = None;
        }
        if self.fan_anim {
            let ms = if self.fan_to > self.fan_from {
                FAN_OPEN_MS
            } else {
                FAN_CLOSE_MS
            };
            let p = (now.duration_since(self.fan_t0).as_secs_f64() * 1000.0 / ms).min(1.0);
            let e = if self.fan_to > self.fan_from {
                ease_out_back(p)
            } else {
                ease_out(p)
            };
            self.fan_t = self.fan_from + (self.fan_to - self.fan_from) * e;
            if p >= 1.0 {
                self.fan_t = self.fan_to;
                self.fan_anim = false;
            }
        }
        // idle → slide into the edge (a fresh shot resets the clock at land)
        if self.hide_to == 0.0
            && self.hide_t == 0.0
            && !self.inside
            && !self.pinned
            && self.lightbox.is_none()
            && self.fly.is_none()
            && now.duration_since(self.idle_since).as_secs_f64() * 1000.0 > HIDE_AFTER_MS
        {
            self.hide_from = self.hide_t;
            self.hide_to = 1.0;
            self.hide_t0 = now;
            self.hide_anim = true;
        }
        if self.hide_anim {
            let ms = if self.hide_to > self.hide_from {
                HIDE_MS
            } else {
                UNHIDE_MS
            };
            let p = (now.duration_since(self.hide_t0).as_secs_f64() * 1000.0 / ms).min(1.0);
            let e = if self.hide_to > self.hide_from {
                ease_out(p)
            } else {
                ease_out_back(p)
            };
            self.hide_t = self.hide_from + (self.hide_to - self.hide_from) * e;
            if p >= 1.0 {
                self.hide_t = self.hide_to;
                self.hide_anim = false;
            }
        }
        if let Some(t0) = self.box_t0 {
            if now.duration_since(t0).as_secs_f64() * 1000.0 >= BOX_MS {
                self.box_t0 = None;
            }
        }
    }


    /// Delete thumb `i` (0 = newest).
    pub fn remove(&mut self, i: usize) {
        let idx = self.shots.len() - 1 - i;
        if idx < self.shots.len() {
            self.shots.remove(idx);
        }
        if self.lightbox == Some(i) {
            self.lightbox = None;
        }
        self.hover = None;
        self.menu = None;
        self.retarget_fan();
    }


    /// Dismiss the whole stack — "clear all" like Notification Centre.
    /// Files on disk are untouched; this only empties the shelf.
    pub fn clear_all(&mut self) {
        self.shots.clear();
        self.fly = None;
        self.native_flight = false;
        self.lightbox = None;
        self.box_t0 = None;
        self.box_img.clear();
        self.menu = None;
        self.hover = None;
        self.inside = false;
        self.pinned = false;
        self.ring = None;
        self.fan_t = 0.0;
        self.fan_from = 0.0;
        self.fan_to = 0.0;
        self.fan_anim = false;
        self.hide_t = 0.0;
        self.hide_from = 0.0;
        self.hide_to = 0.0;
        self.hide_anim = false;
        self.idle_since = Instant::now();
    }


    /// shot for hit-index i (0 = newest).
    pub fn shot(&self, i: usize) -> Option<&Shot> {
        self.shots.get(self.shots.len().checked_sub(1 + i)?)
    }


    pub fn shot_mut(&mut self, i: usize) -> Option<&mut Shot> {
        let idx = self.shots.len().checked_sub(1 + i)?;
        self.shots.get_mut(idx)
    }
}

#[allow(dead_code)] // Leave is part of the portable contract (Wayland wl_pointer.leave)
pub enum ChromeEv {
    Move {
        x: f64,
        y: f64,
    },
    Leave,
    Down {
        x: f64,
        y: f64,
    },
    RightDown {
        x: f64,
        y: f64,
    },
    /// The left button moved past the drag threshold while held — the
    /// backend distinguishes this from `Move` itself (macOS
    /// `mouseDragged:`, winit `CursorMoved` + cached button state).
    Dragged {
        x: f64,
        y: f64,
    },
    /// Left button released — a thumb click resolves here so the same
    /// press can become a `Dragged` instead.
    Up {
        x: f64,
        y: f64,
    },
    Key(Key),
    /// Native vsync tick — the backend fires this from its display clock
    /// (CADisplayLink / wl_surface.frame) so animation settles and
    /// presents on the compositor's cadence instead of a timer.
    Tick,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Key {
    Esc,
    Left,
    Right,
    Enter,
    Back,
    /// Space — quick look, same preview the Enter key opens
    Space,
    /// ⌘C / Ctrl+C — copy the focused shot (backends supply the
    /// modifier check)
    Copy,
    /// O — open the shot being previewed
    Open,
    /// S — share the shot being previewed
    Share,
}

/* ==================== rendering ==================== */

fn ease_out(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

/// easeOutBack — subtle overshoot (≈8%) so the fan "settles" rather
/// than stops dead, the way a native popover does.
fn ease_out_back(t: f64) -> f64 {
    let c = 1.70158 * 0.8;
    let u = t - 1.0;
    1.0 + (c + 1.0) * u * u * u + c * u * u
}


#[cfg(test)]
mod tests;
