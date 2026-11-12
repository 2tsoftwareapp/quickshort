//! Preferences window — macOS System Settings language: a sidebar of pages,
//! grouped inset cards, keycap glyphs, sliding switches. Same stack as the
//! overlay: winit window, softbuffer pixels, SDF widgets, fontdue text
//! (system font looked up at runtime, no bundled assets).

use anyhow::{Context, Result};
use fontdue::{Font, FontSettings};
use qs_overlay::sdf::Canvas;
use softbuffer::{Context as SbContext, Surface};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Icon, Window, WindowId};

use crate::actions;
use crate::config::{accent_deep, Config, ACCENTS, AFTER_CAPTURE, POPUP_POS};
use crate::tray;
use crate::ui::{
    check_box, dropdown_btn, ghost, keycaps_w, mix, row_label, Glyph, LRect, Painter, ROW,
};
use qs_theme::{pal, THEMES};

const W: f64 = 820.0;
const H: f64 = 675.0;
const SIDE_W: f64 = 200.0;
/// content column: x origin and width
const CX: f64 = SIDE_W + 24.0;
const CW: f64 = W - CX - 24.0;

// colours come from qs-theme tokens (`pal()`); layout metrics stay here

/// What the host needs to know after a settings event.
pub enum SettingsEvent {
    /// Config changed + saved — re-register the hotkey, rebuild the menu.
    Changed,
    /// Hotkey recording toggled on/off — host must suspend the global
    /// grab while recording so the chord reaches this window instead of
    /// firing the capture handler.
    HotkeyRecord,
    /// About page wants a release check.
    CheckUpdates,
    /// About page wants to install the pending update.
    InstallUpdate,
    /// User closed the window.
    Closed,
}

/// Sidebar pages in order.
const PAGES: &[(&str, Glyph)] = &[
    ("Capture", Glyph::Crop),
    ("General", Glyph::Gear),
    ("Shortcuts", Glyph::Keys),
    ("Appearance", Glyph::Drop),
    ("OCR", Glyph::Text),
    ("Storage", Glyph::Folder),
    ("About", Glyph::Info),
];

/// Which recordable row is armed. `Global` = the app-wide capture hotkey;
/// the rest are ⌘+key overlay actions.
#[derive(Clone, Copy, PartialEq)]
enum Rec {
    Global,
    /// app-wide fullscreen hotkey — same chord recorder as `Global`
    GlobalFull,
    Save,
    Copy,
    Ocr,
    Share,
    Scroll,
    Record,
    Undo,
}

/// Shortcuts page rows — label + recordable slot.
const KEY_ROWS: &[(&str, Rec)] = &[
    ("Save", Rec::Save),
    ("Copy to clipboard", Rec::Copy),
    ("Extract text (OCR)", Rec::Ocr),
    ("Share link", Rec::Share),
    ("Scrolling capture", Rec::Scroll),
    ("Screen recording", Rec::Record),
    ("Undo / Redo", Rec::Undo),
];

#[derive(Clone, Copy, PartialEq)]
enum Drop {
    RecentN,
    KeepDays,
    PopupPos,
    AfterCapture,
    Theme,
}

#[derive(Clone, Copy, PartialEq)]
enum Sw {
    Login,
    AutoUpd,
    CopySave,
}

impl Sw {
    fn idx(self) -> usize {
        match self {
            Sw::Login => 0,
            Sw::AutoUpd => 1,
            Sw::CopySave => 2,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Hit {
    Nav(usize),
    Rec(Rec),
    Drop(Drop),
    /// index into the open dropdown's option list
    DropOpt(usize),
    Browse,
    OpenDir,
    Sw(Sw),
    Swatch(usize),
    Recent(usize),
    /// OCR page: auto-detect row
    OcrAuto,
    /// OCR page: index into qs_ocr::LANGS
    Lang(usize),
    /// OCR page: install the missing packs (linux)
    OcrInstall,
    ViewAll,
    /// check for updates, or install when `update_ver` is set
    Update,
    ResetKeys,
    ResetAll,
    Clean,
    Link(usize),
}

const LINKS: &[(&str, &str)] = &[
    ("GitHub", "https://github.com/truongnat/quickshort"),
    (
        "Report an issue",
        "https://github.com/truongnat/quickshort/issues",
    ),
    (
        "Request a feature",
        "https://truongnat.github.io/quickshort/#request",
    ),
    ("Support", "https://truongnat.github.io/quickshort/#donate"),
];

const RECENT_N: usize = 4;
const THUMB_W: f64 = 132.0;
const THUMB_H: f64 = 82.0;

pub struct Settings {
    win: Rc<Window>,
    surf: Surface<Rc<Window>, Rc<Window>>,
    font: Font,
    page: usize,
    /// recordable row waiting for input (global chord or ⌘+key)
    recording: Option<Rec>,
    /// open dropdown, if any
    drop_open: Option<Drop>,
    /// destructive action awaiting a second confirming click
    confirm: Option<Hit>,
    hover: Option<Hit>,
    /// keyboard focus target — drawn as a ring only after Tab navigation
    focus: Option<Hit>,
    focus_visible: bool,
    /// outcome of the last "Clean up now" run
    clean_msg: Option<String>,
    /// clickable rects registered during the last draw — hit() scans them
    /// topmost-first, so popups drawn last naturally win
    hits: Vec<(LRect, Hit)>,
    mods: ModifiersState,
    recents: Vec<PathBuf>,
    thumbs: Vec<Option<image::RgbaImage>>,
    /// (count, bytes) of captures in the save dir
    stats: (usize, u64),
    /// (total, free) bytes of the save dir's volume, for the usage chart
    disk: Option<(u64, u64)>,
    /// pending release version the About page can offer to install
    update_ver: Option<String>,
    last_pos: (f64, f64),
    /// switch knob positions 0..1, eased toward the config values
    sw_t: [f64; 3],
    /// recording pulse phase (advanced per frame)
    pulse: f64,
    /// a bare key was pressed while recording — hint a modifier is required
    want_mod: bool,
    /// folder picker is open — guard against re-entry
    dialog_open: bool,
    /// app logo at sidebar size (physical px)
    logo: Option<image::RgbaImage>,
    /// packs the OCR engine can use — feeds the "not installed" badges
    ocr_langs: Vec<String>,
    /// apt install running on a worker thread (pkexec pops the auth UI)
    install: Option<Arc<Mutex<InstallState>>>,
    /// last install's outcome line, shown until the next action
    install_msg: Option<String>,
    dirty: bool,
    closed: bool,
}

/// Shared with the install worker — about_to_wait polls it each frame.
#[derive(Clone)]
enum InstallState {
    Running,
    Ok(String),
    Failed(String),
}

fn app_logo() -> Option<image::RgbaImage> {
    image::load_from_memory(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/icon-256.png"
    )))
    .ok()
    .map(|i| i.to_rgba8())
}

impl Settings {
    pub fn open(el: &ActiveEventLoop, cfg: &Config) -> Result<Self> {
        let logo = app_logo();
        let mut attrs = Window::default_attributes()
            .with_title("Quickshort Settings")
            .with_inner_size(winit::dpi::LogicalSize::new(W, H))
            .with_resizable(false);
        if let Some(img) = &logo {
            if let Ok(ic) = Icon::from_rgba(img.as_raw().clone(), img.width(), img.height()) {
                attrs = attrs.with_window_icon(Some(ic));
            }
        }
        let win = Rc::new(el.create_window(attrs).context("settings window")?);
        force_dark_appearance(&win);
        // an accessory app opened from the tray stays deactivated — without
        // activation the window never gets KeyboardInput, so the hotkey
        // recorder looks dead
        refocus(&win);
        let scale = win.scale_factor();
        let logo_px = (64.0 * scale) as u32;
        let logo = logo.map(|i| {
            image::imageops::resize(&i, logo_px, logo_px, image::imageops::FilterType::Lanczos3)
        });
        win.set_cursor(CursorIcon::Default);
        let surf = SbContext::new(win.clone())
            .and_then(|c| Surface::new(&c, win.clone()))
            .map_err(|e| anyhow::anyhow!("settings surface: {e}"))?;
        let recents = actions::recent_captures(&cfg.save_dir(), RECENT_N);
        let thumbs = load_thumbs(&recents, scale);
        Ok(Self {
            win,
            surf,
            font: system_font()?,
            // QS_PAGE=i jumps straight to a page — dev/screenshot harness
            page: std::env::var("QS_PAGE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            recording: None,
            drop_open: None,
            confirm: None,
            hover: None,
            focus: None,
            focus_visible: false,
            clean_msg: None,
            hits: Vec::new(),
            mods: ModifiersState::empty(),
            stats: actions::storage_stats(&cfg.save_dir()),
            disk: actions::disk_stats(&cfg.save_dir()),
            recents,
            thumbs,
            update_ver: None,
            last_pos: (0.0, 0.0),
            sw_t: [
                cfg.launch_at_login as u8 as f64,
                cfg.auto_update as u8 as f64,
                cfg.copy_after_save as u8 as f64,
            ],
            pulse: 0.0,
            want_mod: false,
            dialog_open: false,
            logo,
            ocr_langs: qs_ocr::installed_langs(),
            install: None,
            install_msg: None,
            dirty: true,
            closed: false,
        })
    }

    pub fn win_id(&self) -> WindowId {
        self.win.id()
    }

    pub fn win(&self) -> &Window {
        &self.win
    }

    /// Bring the app + window to the front (see `refocus`).
    pub fn focus(&self) {
        refocus(&self.win);
    }

    /// The About page shows "Update to vX" once the host has a pending
    /// release; `None` restores the plain check button.
    pub fn set_update(&mut self, ver: Option<String>) {
        if self.update_ver != ver {
            self.update_ver = ver;
            self.dirty = true;
        }
    }

    /// True while any record button is armed — the host suspends the
    /// global hotkey grab so the chord reaches this window.
    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// True while a pack install is running — the host should wake the
    /// loop periodically so the page polls the worker's result.
    pub fn poll_needed(&self) -> bool {
        self.install.is_some()
    }

    pub fn about_to_wait(&mut self) {
        if let Some(arc) = &self.install {
            let state = arc.lock().unwrap().clone();
            match state {
                InstallState::Running => self.dirty = true,
                InstallState::Ok(m) | InstallState::Failed(m) => {
                    self.install = None;
                    self.install_msg = Some(m);
                    self.ocr_langs = qs_ocr::installed_langs();
                    self.dirty = true;
                }
            }
        }
        if self.dirty {
            self.win.request_redraw();
        }
    }

    /// Handle one event; `Some` means the host should act.
    pub fn window_event(&mut self, cfg: &mut Config, ev: WindowEvent) -> Option<SettingsEvent> {
        match ev {
            WindowEvent::CloseRequested => self.closed = true,
            WindowEvent::RedrawRequested => self.draw(cfg),
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::CursorMoved { position, .. } => {
                let s = self.win.scale_factor();
                let p = (position.x / s, position.y / s);
                self.last_pos = p;
                // pointer use retires the keyboard ring; focus itself stays
                if self.focus_visible {
                    self.focus_visible = false;
                    self.dirty = true;
                }
                let h = self.hit(p);
                if h != self.hover {
                    self.hover = h;
                    self.win.set_cursor(if h.is_some() {
                        CursorIcon::Pointer
                    } else {
                        CursorIcon::Default
                    });
                    self.dirty = true;
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let h = self.hit(self.last_pos);
                self.focus = h;
                self.focus_visible = false;
                return self.click(cfg, h);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if matches!(event.logical_key, Key::Named(NamedKey::Escape)) {
                    if self.drop_open.take().is_some() {
                        self.dirty = true;
                        return None;
                    }
                    if self.recording.take().is_some() {
                        self.dirty = true;
                        return Some(SettingsEvent::HotkeyRecord);
                    }
                    if self.focus_visible {
                        self.focus_visible = false;
                        self.dirty = true;
                        return None;
                    }
                    self.closed = true;
                    return None;
                }
                // keyboard navigation only while not recording a hotkey
                if self.recording.is_none() {
                    if self.drop_open.is_some() {
                        let dir = match event.logical_key {
                            Key::Named(NamedKey::ArrowDown) => 1i64,
                            Key::Named(NamedKey::ArrowUp) => -1,
                            _ => 0,
                        };
                        if dir != 0 {
                            self.step_option(dir);
                            return None;
                        }
                    }
                    match event.logical_key {
                        Key::Named(NamedKey::Tab) => {
                            self.move_focus(self.mods.shift_key());
                            return None;
                        }
                        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space)
                            if self.focus_visible =>
                        {
                            if let Some(f) = self.focus {
                                return self.click(cfg, Some(f));
                            }
                        }
                        _ => {}
                    }
                }
                match self.recording {
                    Some(Rec::Global) | Some(Rec::GlobalFull) => {
                        if let Some(hk) = self.chord_string(&event.logical_key) {
                            if hk.parse::<global_hotkey::hotkey::HotKey>().is_err() {
                                return None; // unparsable — keep recording
                            }
                            if self.recording == Some(Rec::GlobalFull) {
                                cfg.hotkey_full = hk;
                            } else {
                                cfg.hotkey = hk;
                            }
                            self.recording = None;
                            self.dirty = true;
                            return Some(SettingsEvent::Changed);
                        }
                        // a real key but no modifier — say why nothing happened
                        if key_token(&event.logical_key).is_some() {
                            self.want_mod = true;
                            self.dirty = true;
                        }
                    }
                    Some(slot) => {
                        if let Key::Character(c) = &event.logical_key {
                            if !(self.mods.super_key() || self.mods.control_key()) {
                                self.want_mod = true;
                                self.dirty = true;
                                return None;
                            }
                            if let Some(ch) = c.chars().next() {
                                let ch = ch.to_ascii_lowercase();
                                if ch.is_ascii_alphanumeric() {
                                    set_key(cfg, slot, ch);
                                    self.recording = None;
                                    self.dirty = true;
                                    return Some(SettingsEvent::Changed);
                                }
                            }
                        } else if key_token(&event.logical_key).is_some() {
                            self.want_mod = true;
                            self.dirty = true;
                        }
                    }
                    None => {}
                }
            }
            _ => {}
        }
        self.closed.then_some(SettingsEvent::Closed)
    }

    /// Compose a `global-hotkey` string from current modifiers + key.
    /// Modifier-only presses keep recording (wait for the real key).
    fn chord_string(&self, key: &Key) -> Option<String> {
        let token = key_token(key)?;
        let mut s = String::new();
        if self.mods.super_key() {
            s.push_str("CmdOrCtrl+");
        }
        if self.mods.control_key() {
            s.push_str("Ctrl+");
        }
        if self.mods.alt_key() {
            s.push_str("Alt+");
        }
        if self.mods.shift_key() {
            s.push_str("Shift+");
        }
        if s.is_empty() {
            // a bare key is a bad global hotkey — except PrintScreen, the
            // de-facto screenshot key, which carries no text meaning
            return (token == "PrintScreen").then_some(token);
        }
        Some(format!("{s}{token}"))
    }

    fn click(&mut self, cfg: &mut Config, h: Option<Hit>) -> Option<SettingsEvent> {
        // a click anywhere disarms a pending confirm unless it *is* the
        // confirm target (checked per arm below)
        let armed = self.confirm.take();
        self.confirm = None;
        match h {
            Some(Hit::Nav(i)) => {
                self.page = i;
                self.drop_open = None;
                self.dirty = true;
            }
            Some(Hit::Rec(r)) => {
                self.recording = if self.recording == Some(r) {
                    None
                } else {
                    Some(r)
                };
                self.drop_open = None;
                self.want_mod = false;
                self.dirty = true;
                return Some(SettingsEvent::HotkeyRecord);
            }
            Some(Hit::Drop(d)) => {
                self.drop_open = (self.drop_open != Some(d)).then_some(d);
                self.dirty = true;
            }
            Some(Hit::DropOpt(i)) => {
                if let Some(d) = self.drop_open.take() {
                    drop_apply(cfg, d, i);
                    if d == Drop::KeepDays {
                        self.clean_msg = None;
                    }
                    // the chosen row vanishes with the popup — park focus
                    // back on the button that owns it
                    self.focus = Some(Hit::Drop(d));
                    self.refresh_files(cfg);
                    self.dirty = true;
                    return Some(SettingsEvent::Changed);
                }
            }
            Some(Hit::Browse) => {
                if !self.dialog_open {
                    self.dialog_open = true;
                    let picked = rfd::FileDialog::new()
                        .set_directory(cfg.save_dir())
                        .set_parent(&*self.win)
                        .pick_folder();
                    self.dialog_open = false;
                    // an app-modal panel deactivates the accessory app —
                    // pull focus back or the window looks dead
                    refocus(&self.win);
                    self.dirty = true;
                    if let Some(d) = picked {
                        cfg.save_dir = Some(d);
                        self.refresh_files(cfg);
                        return Some(SettingsEvent::Changed);
                    }
                }
            }
            Some(Hit::OpenDir) | Some(Hit::ViewAll) => {
                actions::open_path(&cfg.save_dir());
            }
            Some(Hit::OcrAuto) => {
                // a checked checkbox must be uncheckable: leaving auto
                // starts a manual set from the first available language
                cfg.ocr_lang = if cfg.ocr_lang.trim() == "auto" {
                    self.ocr_langs
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "eng".into())
                } else {
                    "auto".into()
                };
                self.dirty = true;
                return Some(SettingsEvent::Changed);
            }
            Some(Hit::Lang(i)) => {
                if let Some(e) = qs_ocr::LANGS.get(i) {
                    let mut sel = qs_ocr::lang_codes(&cfg.ocr_lang);
                    if let Some(pos) = sel.iter().position(|c| c == e.0) {
                        sel.remove(pos);
                    } else {
                        sel.push(e.0.to_owned());
                    }
                    // an empty manual set means "scan everything" — auto
                    cfg.ocr_lang = if sel.is_empty() {
                        "auto".into()
                    } else {
                        sel.join("+")
                    };
                    self.dirty = true;
                    return Some(SettingsEvent::Changed);
                }
            }
            Some(Hit::OcrInstall) => {
                if self.install.is_some() {
                    return None;
                }
                let pkgs: Vec<String> = qs_ocr::LANGS
                    .iter()
                    .map(|e| e.0)
                    .filter(|c| !self.ocr_langs.iter().any(|l| l == c))
                    .map(|c| format!("tesseract-ocr-{c}"))
                    .collect();
                if pkgs.is_empty() {
                    return None;
                }
                let st = Arc::new(Mutex::new(InstallState::Running));
                let st2 = st.clone();
                std::thread::spawn(move || {
                    *st2.lock().unwrap() = install_packs(&pkgs);
                });
                self.install = Some(st);
                self.install_msg = None;
                self.dirty = true;
            }
            Some(Hit::Sw(s)) => {
                match s {
                    Sw::Login => {
                        let want = !cfg.launch_at_login;
                        match tray::set_launch_at_login(want) {
                            Ok(()) => cfg.launch_at_login = want,
                            Err(e) => {
                                eprintln!("launch-at-login: {e:#}");
                                return None;
                            }
                        }
                    }
                    Sw::AutoUpd => cfg.auto_update = !cfg.auto_update,
                    Sw::CopySave => cfg.copy_after_save = !cfg.copy_after_save,
                }
                self.dirty = true;
                return Some(SettingsEvent::Changed);
            }
            Some(Hit::Swatch(i)) => {
                if let Some((name, _)) = ACCENTS.get(i) {
                    cfg.accent = (*name).into();
                    self.dirty = true;
                    return Some(SettingsEvent::Changed);
                }
            }
            Some(Hit::Recent(i)) => {
                if let Some(p) = self.recents.get(i) {
                    actions::open_path(p);
                }
            }
            Some(Hit::Update) => {
                return Some(if self.update_ver.is_some() {
                    SettingsEvent::InstallUpdate
                } else {
                    SettingsEvent::CheckUpdates
                });
            }
            Some(Hit::ResetKeys) => {
                if armed == Some(Hit::ResetKeys) {
                    let d = Config::default();
                    cfg.key_save = d.key_save;
                    cfg.key_copy = d.key_copy;
                    cfg.key_ocr = d.key_ocr;
                    cfg.key_share = d.key_share;
                    cfg.key_scroll = d.key_scroll;
                    cfg.key_record = d.key_record;
                    cfg.key_undo = d.key_undo;
                    self.dirty = true;
                    return Some(SettingsEvent::Changed);
                }
                self.confirm = Some(Hit::ResetKeys);
                self.dirty = true;
            }
            Some(Hit::ResetAll) => {
                if armed == Some(Hit::ResetAll) {
                    let keep = (cfg.save_dir.clone(), cfg.last_update_check);
                    *cfg = Config::default();
                    cfg.save_dir = keep.0;
                    cfg.last_update_check = keep.1;
                    self.refresh_files(cfg);
                    self.dirty = true;
                    return Some(SettingsEvent::Changed);
                }
                self.confirm = Some(Hit::ResetAll);
                self.dirty = true;
            }
            Some(Hit::Clean) => {
                if cfg.keep_days == 0 {
                    self.clean_msg = Some("Retention is off — choose a period first.".into());
                    self.dirty = true;
                    return None;
                }
                if armed == Some(Hit::Clean) {
                    let n = actions::cleanup_captures(&cfg.save_dir(), cfg.keep_days);
                    self.clean_msg = Some(match n {
                        0 => "Nothing was old enough to remove.".into(),
                        1 => "Removed 1 old capture.".into(),
                        n => format!("Removed {n} old captures."),
                    });
                    self.refresh_files(cfg);
                    self.dirty = true;
                    return Some(SettingsEvent::Changed);
                }
                self.confirm = Some(Hit::Clean);
                self.dirty = true;
            }
            Some(Hit::Link(i)) => {
                if i == 1 {
                    // Report an issue — bundle diagnostics into the body
                    actions::report_issue(cfg);
                } else if let Some((_, url)) = LINKS.get(i) {
                    actions::open_url(url);
                }
            }
            None => {
                self.drop_open = None;
                if self.recording.take().is_some() {
                    self.dirty = true;
                    return Some(SettingsEvent::HotkeyRecord);
                }
                self.dirty = true;
            }
        }
        None
    }

    /// Re-read captures + stats after the save dir or retention changed.
    fn refresh_files(&mut self, cfg: &Config) {
        self.recents = actions::recent_captures(&cfg.save_dir(), RECENT_N);
        self.thumbs = load_thumbs(&self.recents, self.win.scale_factor());
        self.stats = actions::storage_stats(&cfg.save_dir());
        self.disk = actions::disk_stats(&cfg.save_dir());
    }

    fn hit(&self, p: (f64, f64)) -> Option<Hit> {
        let (x, y) = p;
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3)
            .map(|&(_, h)| h)
    }

    /// Tab order = registration order from the last draw (sidebar, then
    /// page controls). An open dropdown is modal, so only its rows cycle.
    fn focusables(&self) -> Vec<Hit> {
        let mut v = Vec::new();
        for &(_, h) in &self.hits {
            if self.drop_open.is_some() && !matches!(h, Hit::DropOpt(_)) {
                continue;
            }
            if !v.contains(&h) {
                v.push(h);
            }
        }
        v
    }

    fn move_focus(&mut self, back: bool) {
        let list = self.focusables();
        if list.is_empty() {
            return;
        }
        let cur = self.focus.and_then(|f| list.iter().position(|h| *h == f));
        let next = match cur {
            Some(i) if back => (i + list.len() - 1) % list.len(),
            Some(i) => (i + 1) % list.len(),
            None if back => list.len() - 1,
            None => 0,
        };
        self.focus = Some(list[next]);
        self.focus_visible = true;
        self.dirty = true;
    }

    /// Arrow keys inside an open dropdown walk its rows.
    fn step_option(&mut self, dir: i64) {
        let opts: Vec<Hit> = self
            .hits
            .iter()
            .filter_map(|&(_, h)| matches!(h, Hit::DropOpt(_)).then_some(h))
            .collect();
        if opts.is_empty() {
            return;
        }
        let cur = self.focus.and_then(|f| opts.iter().position(|h| *h == f));
        let i = match cur {
            Some(i) => (i as i64 + dir).rem_euclid(opts.len() as i64) as usize,
            None if dir < 0 => opts.len() - 1,
            None => 0,
        };
        self.focus = Some(opts[i]);
        self.focus_visible = true;
        self.dirty = true;
    }

    fn draw(&mut self, cfg: &Config) {
        // advance animations; keep repainting while anything is moving
        let targets = [
            cfg.launch_at_login as u8 as f64,
            cfg.auto_update as u8 as f64,
            cfg.copy_after_save as u8 as f64,
        ];
        let mut animating = self.recording.is_some();
        for (t, &want) in self.sw_t.iter_mut().zip(&targets) {
            if (*t - want).abs() > 0.001 {
                animating = true;
            }
            *t += (want - *t) * 0.28;
        }
        self.pulse += 0.08;
        self.dirty = animating;

        let size = self.win.inner_size();
        let (sw, sh) = match (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
            (Some(w), Some(h)) => (w.get() as usize, h.get() as usize),
            _ => return,
        };
        if self
            .surf
            .resize(
                NonZeroU32::new(sw as u32).unwrap(),
                NonZeroU32::new(sh as u32).unwrap(),
            )
            .is_err()
        {
            return;
        }
        // snapshot state — buffer_mut holds &mut self.surf for the rest
        // of the fn, so only disjoint field borrows are allowed below
        let v = View {
            cfg,
            page: self.page,
            hover: self.hover,
            recording: self.recording,
            pulse: self.pulse,
            want_mod: self.want_mod,
            drop_open: self.drop_open,
            confirm: self.confirm,
            sw_t: self.sw_t,
            stats: self.stats,
            disk: self.disk,
            update_ver: self.update_ver.as_deref(),
            recents: &self.recents,
            thumbs: &self.thumbs,
            logo: self.logo.as_ref(),
            ocr_langs: &self.ocr_langs,
            installing: self.install.is_some(),
            install_msg: self.install_msg.as_deref(),
            clean_msg: self.clean_msg.as_deref(),
        };
        let Ok(mut buf) = self.surf.buffer_mut() else {
            return;
        };
        // window backdrop: faint vertical gradient, brighter at the top
        for (row, px) in buf.iter_mut().enumerate() {
            let t = (row / sw) as f64 / sh.max(1) as f64;
            *px = mix(pal().bg_top, pal().bg_bottom, t);
        }
        let scale = self.win.scale_factor();
        let mut cv = Canvas {
            buf: &mut buf,
            w: sw,
            h: sh,
            scale,
        };
        let p = Painter {
            font: &self.font,
            scale,
            accent: cfg.accent_rgb(),
            accent_deep: accent_deep(&cfg.accent),
        };
        let mut hits = std::mem::take(&mut self.hits);
        hits.clear();

        // sidebar: darker panel + nav, separator hairline
        p.fill(&mut cv, (0.0, 0.0, SIDE_W, H), 0.0, pal().sidebar, 1.0);
        for py in 0..sh {
            cv.blend((SIDE_W * scale) as i32, py as i32, pal().border, 1.0);
        }
        if let Some(logo) = v.logo {
            let (ox, oy) = ((18.0 * scale) as i32, (22.0 * scale) as i32);
            for (px, py, c) in logo.enumerate_pixels() {
                let a = c[3] as f64 / 255.0;
                if a > 0.0 {
                    let rgb = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32;
                    cv.blend(ox + px as i32, oy + py as i32, rgb, a);
                }
            }
            p.stroke(
                &mut cv,
                (18.0, 22.0, 64.0, 64.0),
                16.0,
                pal().border_strong,
                0.7,
                1.0,
            );
        } else {
            p.icon(&mut cv, 18.0, 22.0, 64.0);
        }
        p.text(&mut cv, 18.0, 106.0, 17.0, "Quickshort", pal().fg);
        p.text(
            &mut cv,
            18.0,
            124.0,
            11.0,
            concat!("v", env!("CARGO_PKG_VERSION")),
            pal().fg_faint,
        );
        let mut ny = 152.0;
        for (i, (name, g)) in PAGES.iter().enumerate() {
            let r = (10.0, ny, SIDE_W - 20.0, 36.0);
            let sel = v.page == i;
            if sel {
                p.fill(&mut cv, r, 9.0, p.accent_deep, 1.0);
            } else if v.hover == Some(Hit::Nav(i)) {
                p.fill(&mut cv, r, 9.0, pal().surface_hover, 1.0);
            }
            p.glyph(
                &mut cv,
                *g,
                r.0 + 9.0,
                r.1 + 10.0,
                16.0,
                if sel { pal().on_accent } else { pal().fg_dim },
            );
            p.text(
                &mut cv,
                r.0 + 34.0,
                r.1 + 24.0,
                13.0,
                name,
                if sel { pal().on_accent } else { pal().fg },
            );
            hits.push((r, Hit::Nav(i)));
            ny += 40.0;
        }
        p.text(
            &mut cv,
            18.0,
            H - 18.0,
            10.5,
            "Preferences save automatically",
            pal().fg_faint,
        );

        match v.page {
            0 => draw_capture(&p, &mut cv, &v, &mut hits),
            1 => draw_general(&p, &mut cv, &v, &mut hits),
            2 => draw_shortcuts(&p, &mut cv, &v, &mut hits),
            3 => draw_appearance(&p, &mut cv, &v, &mut hits),
            4 => draw_ocr(&p, &mut cv, &v, &mut hits),
            5 => draw_storage(&p, &mut cv, &v, &mut hits),
            _ => draw_about(&p, &mut cv, &v, &mut hits),
        }

        // open dropdown popup floats above the page — drawn last so its
        // hit rects win the topmost scan. Anchored to the button rect the
        // page registered in `hits`, so it can't drift from the control.
        if let Some(d) = v.drop_open {
            let (opts, cur) = drop_state(v.cfg, d);
            if let Some(&btn) = hits
                .iter()
                .find(|(_, h)| *h == Hit::Drop(d))
                .map(|(r, _)| r)
            {
                let ph = opts.len() as f64 * 30.0 + 8.0;
                // flip upward when the list would spill past the window edge
                let py = if btn.1 + btn.3 + 4.0 + ph > H - 8.0 {
                    (btn.1 - 4.0 - ph).max(8.0)
                } else {
                    btn.1 + btn.3 + 4.0
                };
                let pop = (btn.0, py, btn.2.max(160.0), ph);
                p.card(&mut cv, pop);
                for (i, o) in opts.iter().enumerate() {
                    let ir = (
                        pop.0 + 4.0,
                        pop.1 + 4.0 + i as f64 * 30.0,
                        pop.2 - 8.0,
                        30.0,
                    );
                    if v.hover == Some(Hit::DropOpt(i)) {
                        p.fill(&mut cv, ir, 6.0, pal().surface_hover, 1.0);
                    }
                    if i == cur {
                        p.text(&mut cv, ir.0 + 10.0, ir.1 + 20.0, 12.0, "✓", p.accent_deep);
                        p.text(&mut cv, ir.0 + 26.0, ir.1 + 20.0, 12.0, o, pal().fg);
                    } else {
                        p.text(&mut cv, ir.0 + 26.0, ir.1 + 20.0, 12.0, o, pal().fg);
                    }
                    hits.push((ir, Hit::DropOpt(i)));
                }
            }
        }

        // keyboard focus ring — white halo + accent keeps it legible on
        // both white cards and the accent-filled nav pill
        if self.focus_visible {
            if let Some(f) = self.focus {
                if let Some(&(r, _)) = hits.iter().find(|(_, h)| *h == f) {
                    p.stroke(
                        &mut cv,
                        (r.0 - 3.0, r.1 - 3.0, r.2 + 6.0, r.3 + 6.0),
                        10.0,
                        pal().fringe,
                        0.6,
                        3.0,
                    );
                    p.stroke(
                        &mut cv,
                        (r.0 - 1.0, r.1 - 1.0, r.2 + 2.0, r.3 + 2.0),
                        9.0,
                        p.accent,
                        1.0,
                        1.5,
                    );
                }
            }
        }

        self.hits = hits;
        // debug: QS_SHOT=<png path> dumps the first rendered frame
        if let Ok(path) = std::env::var("QS_SHOT") {
            std::env::remove_var("QS_SHOT");
            let mut rgba = Vec::with_capacity(sw * sh * 4);
            for &px in buf.iter() {
                rgba.extend_from_slice(&[
                    ((px >> 16) & 0xFF) as u8,
                    ((px >> 8) & 0xFF) as u8,
                    (px & 0xFF) as u8,
                    255,
                ]);
            }
            let _ = image::save_buffer(&path, &rgba, sw as u32, sh as u32, image::ColorType::Rgba8);
            if std::env::var("QS_EXIT").is_ok() {
                std::process::exit(0);
            }
        }
        let _ = buf.present();
    }
}

/// Immutable snapshot of everything the page renderers need — `draw`
/// builds this before `buffer_mut` so page fns stay free functions and
/// never touch `&mut self` while the surface is borrowed.
struct View<'a> {
    cfg: &'a Config,
    page: usize,
    hover: Option<Hit>,
    recording: Option<Rec>,
    pulse: f64,
    want_mod: bool,
    drop_open: Option<Drop>,
    confirm: Option<Hit>,
    sw_t: [f64; 3],
    stats: (usize, u64),
    disk: Option<(u64, u64)>,
    update_ver: Option<&'a str>,
    recents: &'a [PathBuf],
    thumbs: &'a [Option<image::RgbaImage>],
    logo: Option<&'a image::RgbaImage>,
    ocr_langs: &'a [String],
    /// a pack install is running — the Install button locks + pulses
    installing: bool,
    install_msg: Option<&'a str>,
    /// outcome of the last manual clean-up
    clean_msg: Option<&'a str>,
}

// layout helpers ------------------------------------------------------------

fn card(x: f64, y: f64, rows: usize) -> LRect {
    (x, y, CW, ROW * rows.max(1) as f64)
}

/// Right-aligned control slot inside a card row.
fn slot(y: f64, w: f64) -> LRect {
    (CX + CW - 14.0 - w, y + (ROW - 30.0) / 2.0, w, 30.0)
}

/// dropdown option list + current index for `d`
fn drop_state(cfg: &Config, d: Drop) -> (Vec<&'static str>, usize) {
    match d {
        Drop::RecentN => (
            vec!["4 items", "8 items", "12 items"],
            match cfg.recent_max {
                4 => 0,
                12 => 2,
                _ => 1,
            },
        ),
        Drop::KeepDays => (
            vec!["Forever", "7 days", "30 days"],
            match cfg.keep_days {
                7 => 1,
                30 => 2,
                _ => 0,
            },
        ),
        Drop::PopupPos => (
            POPUP_POS.iter().map(|(_, l)| *l).collect(),
            POPUP_POS
                .iter()
                .position(|(k, _)| *k == cfg.popup_pos)
                .unwrap_or(0),
        ),
        Drop::Theme => (
            THEMES.iter().map(|(_, l)| *l).collect(),
            THEMES
                .iter()
                .position(|(k, _)| *k == cfg.theme)
                .unwrap_or(0),
        ),
        Drop::AfterCapture => (
            AFTER_CAPTURE.iter().map(|(_, l)| *l).collect(),
            AFTER_CAPTURE
                .iter()
                .position(|(k, _)| *k == cfg.after_capture)
                .unwrap_or(0),
        ),
    }
}

fn section_title(p: &Painter, cv: &mut Canvas, y: f64, title: &str, sub: &str) {
    p.text(cv, CX, y, 13.5, title, pal().fg);
    p.text(cv, CX, y + 19.0, 11.5, sub, pal().fg_dim);
}

fn switch_row(
    p: &Painter,
    cv: &mut Canvas,
    v: &View,
    hits: &mut Vec<(LRect, Hit)>,
    r: LRect,
    g: Glyph,
    label: &str,
    desc: &str,
    sw: Sw,
) {
    if v.hover == Some(Hit::Sw(sw)) {
        p.fill(cv, r, 7.0, pal().surface_hover, 1.0);
    }
    row_label(p, cv, r, g, label, desc);
    p.switch(
        cv,
        (r.0 + r.2 - 14.0 - 44.0, r.1 + 13.0, 44.0, 26.0),
        v.sw_t[sw.idx()],
    );
    hits.push((r, Hit::Sw(sw)));
}

// pages ---------------------------------------------------------------------

fn draw_capture(p: &Painter, cv: &mut Canvas, v: &View, hits: &mut Vec<(LRect, Hit)>) {
    p.text(cv, CX, 52.0, 24.0, "Capture", pal().fg);
    p.text(
        cv,
        CX,
        76.0,
        12.5,
        "Configure how you capture and save screenshots.",
        pal().fg_dim,
    );
    section_title(
        p,
        cv,
        108.0,
        "Capture method",
        "Choose hotkey and capture behavior.",
    );
    let c = card(CX, 142.0, 2);
    p.card_rows(cv, c, 2);
    // row 1: region hotkey
    let r = (c.0, c.1, c.2, ROW);
    row_label(
        p,
        cv,
        r,
        Glyph::Crop,
        "Capture hotkey",
        "Select a region on screen",
    );
    let rb = slot(r.1, 76.0);
    if v.recording == Some(Rec::Global) {
        let a = 0.55 + 0.45 * v.pulse.sin().abs();
        p.stroke(
            cv,
            (c.0 + 8.0, r.1 + 11.0, c.2 - 16.0, 30.0),
            7.0,
            v.cfg.accent_rgb(),
            a,
            1.0,
        );
        let (msg, col) = if v.want_mod {
            ("Needs a modifier, e.g. ⌘ ⌥ ⌃", pal().fg_dim)
        } else {
            ("Press shortcut", p.accent_deep)
        };
        p.text(
            cv,
            rb.0 - p.text_w(msg, 11.5) - 12.0,
            r.1 + 29.0,
            11.5,
            msg,
            col,
        );
    } else {
        p.keycaps(
            cv,
            &v.cfg.hotkey,
            rb.0 - keycaps_w(p, &v.cfg.hotkey) - 10.0,
            r.1 + 12.0,
        );
    }
    p.ghost_button(cv, rb, "Change", v.hover == Some(Hit::Rec(Rec::Global)));
    hits.push((rb, Hit::Rec(Rec::Global)));

    // row 2: fullscreen hotkey — saves the whole screen, no selection
    let r = (c.0, c.1 + ROW, c.2, ROW);
    row_label(
        p,
        cv,
        r,
        Glyph::Display,
        "Fullscreen hotkey",
        "Save the whole screen, no selection",
    );
    let rb = slot(r.1, 76.0);
    if v.recording == Some(Rec::GlobalFull) {
        let a = 0.55 + 0.45 * v.pulse.sin().abs();
        p.stroke(
            cv,
            (c.0 + 8.0, r.1 + 11.0, c.2 - 16.0, 30.0),
            7.0,
            v.cfg.accent_rgb(),
            a,
            1.0,
        );
        let (msg, col) = if v.want_mod {
            ("Needs a modifier, e.g. ⌘ ⌥ ⌃", pal().fg_dim)
        } else {
            ("Press shortcut", p.accent_deep)
        };
        p.text(
            cv,
            rb.0 - p.text_w(msg, 11.5) - 12.0,
            r.1 + 29.0,
            11.5,
            msg,
            col,
        );
    } else {
        p.keycaps(
            cv,
            &v.cfg.hotkey_full,
            rb.0 - keycaps_w(p, &v.cfg.hotkey_full) - 10.0,
            r.1 + 12.0,
        );
    }
    p.ghost_button(cv, rb, "Change", v.hover == Some(Hit::Rec(Rec::GlobalFull)));
    hits.push((rb, Hit::Rec(Rec::GlobalFull)));

    section_title(
        p,
        cv,
        280.0,
        "Save location",
        "Choose where captured images are saved.",
    );
    let c = card(CX, 314.0, 1);
    p.card(cv, c);
    row_label(
        p,
        cv,
        c,
        Glyph::Folder,
        "Save folder",
        &tail(&short_path(&v.cfg.save_dir().display().to_string()), 38),
    );
    let bb = slot(c.1, 84.0);
    p.ghost_button(cv, bb, "Change…", v.hover == Some(Hit::Browse));
    hits.push((bb, Hit::Browse));

    section_title(
        p,
        cv,
        400.0,
        "Options",
        "Additional settings for capture and workflow.",
    );
    let c = card(CX, 434.0, 1);
    p.card(cv, c);
    switch_row(
        p,
        cv,
        v,
        hits,
        c,
        Glyph::Bolt,
        "Launch at login",
        "Start Quickshort when you log in",
        Sw::Login,
    );

    // recent captures
    p.text(cv, CX, 520.0, 14.0, "Recent captures", pal().fg);
    let va = (CX + CW - 84.0, 508.0, 84.0, 24.0);
    p.text(
        cv,
        va.0 + (va.2 - p.text_w("Open folder ›", 11.5)) / 2.0,
        521.0,
        11.5,
        "Open folder ›",
        if v.hover == Some(Hit::ViewAll) {
            pal().fg
        } else {
            p.accent_deep
        },
    );
    hits.push((va, Hit::ViewAll));
    if v.recents.is_empty() {
        p.text(
            cv,
            CX,
            566.0,
            12.0,
            "No captures yet — your latest shots will show up here.",
            pal().fg_dim,
        );
    }
    for (i, path) in v.recents.iter().enumerate() {
        let x = CX + i as f64 * (THUMB_W + 14.0);
        let tr = (x, 542.0, THUMB_W, THUMB_H);
        if v.hover == Some(Hit::Recent(i)) {
            p.fill(
                cv,
                (tr.0 - 3.0, tr.1 - 3.0, tr.2 + 6.0, tr.3 + 6.0),
                8.0,
                pal().surface_hover,
                1.0,
            );
        }
        p.thumb(cv, tr, v.thumbs.get(i).and_then(|t| t.as_ref()));
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let display_name = name.strip_prefix("quickshort-").unwrap_or(&name);
        p.text(
            cv,
            x,
            tr.1 + tr.3 + 14.0,
            10.5,
            &truncate_head(display_name, 18),
            pal().fg_dim,
        );
        p.text(
            cv,
            x,
            tr.1 + tr.3 + 28.0,
            10.0,
            &age_label(path),
            pal().fg_faint,
        );
        hits.push((
            (tr.0 - 3.0, tr.1 - 3.0, tr.2 + 6.0, tr.3 + 34.0),
            Hit::Recent(i),
        ));
    }
}

fn draw_general(p: &Painter, cv: &mut Canvas, v: &View, hits: &mut Vec<(LRect, Hit)>) {
    p.text(cv, CX, 52.0, 24.0, "General", pal().fg);
    p.text(
        cv,
        CX,
        76.0,
        12.5,
        "App behavior and system integration.",
        pal().fg_dim,
    );
    section_title(p, cv, 112.0, "Updates", "Keep Quickshort current.");
    let c = card(CX, 146.0, 1);
    p.card(cv, c);
    switch_row(
        p,
        cv,
        v,
        hits,
        c,
        Glyph::Info,
        "Automatically check for updates",
        "Once a day, in the background",
        Sw::AutoUpd,
    );
    section_title(p, cv, 228.0, "Menu", "What shows in the menu bar icon.");
    let c = card(CX, 262.0, 1);
    p.card(cv, c);
    row_label(
        p,
        cv,
        c,
        Glyph::Folder,
        "Recent items",
        "Captures listed in the tray menu",
    );
    let mb = (c.0 + c.2 - 14.0 - 130.0, c.1 + 11.0, 130.0, 30.0);
    dropdown_btn(
        p,
        cv,
        mb,
        &format!("{} items", v.cfg.recent_max),
        v.drop_open == Some(Drop::RecentN),
        v.hover == Some(Hit::Drop(Drop::RecentN)),
    );
    hits.push((mb, Hit::Drop(Drop::RecentN)));

    section_title(p, cv, 344.0, "Reset", "Restore every setting to defaults.");
    let c = card(CX, 378.0, 1);
    p.card(cv, c);
    row_label(
        p,
        cv,
        c,
        Glyph::Gear,
        "Reset all settings",
        "Hotkey, shortcuts, accent, folders — all of it",
    );
    let rb = slot(c.1, 76.0);
    ghost(
        p,
        cv,
        rb,
        "Reset…",
        v.hover == Some(Hit::ResetAll),
        v.confirm == Some(Hit::ResetAll),
    );
    hits.push((rb, Hit::ResetAll));
}

fn draw_shortcuts(p: &Painter, cv: &mut Canvas, v: &View, hits: &mut Vec<(LRect, Hit)>) {
    p.text(cv, CX, 52.0, 24.0, "Shortcuts", pal().fg);
    p.text(
        cv,
        CX,
        76.0,
        12.5,
        "Overlay action keys — held with ⌘ or ⌃ while annotating.",
        pal().fg_dim,
    );
    let c = card(CX, 104.0, KEY_ROWS.len());
    p.card(cv, c);
    for (i, (label, rec)) in KEY_ROWS.iter().enumerate() {
        let r = (c.0, c.1 + i as f64 * ROW, c.2, ROW);
        if i > 0 {
            p.line(cv, c.0 + 14.0, r.1, c.0 + c.2 - 14.0);
        }
        row_label(p, cv, r, Glyph::Keys, label, "");
        let rb = slot(r.1, 76.0);
        if v.recording == Some(*rec) {
            let a = 0.55 + 0.45 * v.pulse.sin().abs();
            p.stroke(
                cv,
                (c.0 + 8.0, r.1 + 11.0, c.2 - 16.0, 30.0),
                7.0,
                v.cfg.accent_rgb(),
                a,
                1.0,
            );
            let (msg, col) = if v.want_mod {
                ("Hold ⌘ or ⌃ plus a key", pal().fg_dim)
            } else {
                ("Press ⌘ + key", p.accent_deep)
            };
            p.text(
                cv,
                rb.0 - p.text_w(msg, 11.5) - 12.0,
                r.1 + 29.0,
                11.5,
                msg,
                col,
            );
        } else {
            let ks = format!("⌘+{}", key_str(v.cfg, *rec).to_uppercase());
            p.keycaps(cv, &ks, rb.0 - keycaps_w(p, &ks) - 10.0, r.1 + 12.0);
        }
        p.ghost_button(cv, rb, "Change", v.hover == Some(Hit::Rec(*rec)));
        hits.push((rb, Hit::Rec(*rec)));
    }
    p.text(
        cv,
        CX,
        c.1 + c.3 + 24.0,
        11.0,
        "The Undo key also redoes with ⇧. Fixed keys: Esc cancel, Enter commit.",
        pal().fg_faint,
    );
    let rb = (CX + CW - 130.0, c.1 + c.3 + 44.0, 130.0, 30.0);
    ghost(
        p,
        cv,
        rb,
        "Reset to defaults",
        v.hover == Some(Hit::ResetKeys),
        v.confirm == Some(Hit::ResetKeys),
    );
    hits.push((rb, Hit::ResetKeys));
}

fn draw_appearance(p: &Painter, cv: &mut Canvas, v: &View, hits: &mut Vec<(LRect, Hit)>) {
    p.text(cv, CX, 52.0, 24.0, "Appearance", pal().fg);
    p.text(cv, CX, 76.0, 12.5, "Interface and behavior.", pal().fg_dim);
    section_title(p, cv, 112.0, "Theme", "Mode and highlight color.");
    let c = card(CX, 146.0, 2);
    p.card_rows(cv, c, 2);
    row_label(
        p,
        cv,
        c,
        Glyph::Drop,
        "Accent color",
        "Used for selections and highlights",
    );
    for (i, (name, rgb)) in ACCENTS.iter().enumerate() {
        let r = (
            c.0 + c.2 - 14.0 - (ACCENTS.len() - i) as f64 * 34.0,
            c.1 + 15.0,
            26.0,
            22.0,
        );
        p.swatch(
            cv,
            r,
            *rgb,
            v.cfg.accent == *name,
            v.hover == Some(Hit::Swatch(i)),
        );
        hits.push((r, Hit::Swatch(i)));
    }
    row_label(
        p,
        cv,
        (c.0, c.1 + ROW, c.2, ROW),
        Glyph::Display,
        "Mode",
        "Dark Studio or Light surfaces",
    );
    let tb = slot(c.1 + ROW, 160.0);
    let label = THEMES
        .iter()
        .find(|(k, _)| *k == v.cfg.theme)
        .map(|(_, l)| *l)
        .unwrap_or(THEMES[0].1);
    dropdown_btn(
        p,
        cv,
        tb,
        label,
        v.drop_open == Some(Drop::Theme),
        v.hover == Some(Hit::Drop(Drop::Theme)),
    );
    hits.push((tb, Hit::Drop(Drop::Theme)));
    section_title(
        p,
        cv,
        280.0,
        "After capture",
        "What happens once a capture is saved.",
    );
    let c = card(CX, 314.0, 2);
    p.card_rows(cv, c, 2);
    row_label(
        p,
        cv,
        (c.0, c.1, c.2, ROW),
        Glyph::Crop,
        "Then",
        "Thumbnail, open Quickshort Editor, or copy",
    );
    let ab = slot(c.1, 160.0);
    let label = AFTER_CAPTURE
        .iter()
        .find(|(k, _)| *k == v.cfg.after_capture)
        .map(|(_, l)| *l)
        .unwrap_or(AFTER_CAPTURE[0].1);
    dropdown_btn(
        p,
        cv,
        ab,
        label,
        v.drop_open == Some(Drop::AfterCapture),
        v.hover == Some(Hit::Drop(Drop::AfterCapture)),
    );
    hits.push((ab, Hit::Drop(Drop::AfterCapture)));
    switch_row(
        p,
        cv,
        v,
        hits,
        (c.0, c.1 + ROW, c.2, ROW),
        Glyph::Drop,
        "Copy captures after saving",
        "Saved screenshots also go to the clipboard",
        Sw::CopySave,
    );

    section_title(p, cv, 448.0, "Popups", "Where floating cards appear.");
    let c = card(CX, 482.0, 1);
    p.card(cv, c);
    row_label(
        p,
        cv,
        c,
        Glyph::Drop,
        "Popup position",
        "Toasts, capture cards and the REC pill",
    );
    let pb = (c.0 + c.2 - 14.0 - 140.0, c.1 + 11.0, 140.0, 30.0);
    let label = POPUP_POS
        .iter()
        .find(|(k, _)| *k == v.cfg.popup_pos)
        .map(|(_, l)| *l)
        .unwrap_or(POPUP_POS[0].1);
    dropdown_btn(
        p,
        cv,
        pb,
        label,
        v.drop_open == Some(Drop::PopupPos),
        v.hover == Some(Hit::Drop(Drop::PopupPos)),
    );
    hits.push((pb, Hit::Drop(Drop::PopupPos)));
}

/// apt install of missing packs — pkexec raises the polkit auth dialog
/// so the app never needs root itself. Worker thread reports via the
/// shared InstallState that about_to_wait polls.
#[cfg(target_os = "linux")]
fn install_packs(pkgs: &[String]) -> InstallState {
    let status = std::process::Command::new("pkexec")
        .arg("apt-get")
        .arg("install")
        .arg("-y")
        .args(pkgs)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => InstallState::Ok(format!("Installed {} pack(s)", pkgs.len())),
        Ok(s) => InstallState::Failed(format!("apt exited {s} — nothing installed")),
        Err(e) => InstallState::Failed(format!("pkexec failed: {e}")),
    }
}

#[cfg(not(target_os = "linux"))]
fn install_packs(_: &[String]) -> InstallState {
    // unreachable — the button only renders where packs can be installed
    InstallState::Failed("pack install is Linux-only".into())
}

fn draw_ocr(p: &Painter, cv: &mut Canvas, v: &View, hits: &mut Vec<(LRect, Hit)>) {
    p.text(cv, CX, 52.0, 24.0, "OCR", pal().fg);
    p.text(
        cv,
        CX,
        76.0,
        12.5,
        "Languages the text action (T) recognizes.",
        pal().fg_dim,
    );

    let auto = v.cfg.ocr_lang.trim() == "auto";
    section_title(
        p,
        cv,
        112.0,
        "Mode",
        if auto {
            "Every installed script is scanned — choose a language below to limit it."
        } else {
            "Only checked languages run — uncheck them all to return to auto."
        },
    );
    let c = card(CX, 146.0, 1);
    p.card(cv, c);
    let r = (c.0, c.1, c.2, ROW);
    row_label(
        p,
        cv,
        r,
        Glyph::Text,
        "Auto-detect",
        "Let the engine identify the script — recommended",
    );
    check_box(
        p,
        cv,
        r.0 + r.2 - 34.0,
        r.1 + 16.0,
        auto,
        v.hover == Some(Hit::OcrAuto),
    );
    hits.push((r, Hit::OcrAuto));

    // Media.Ocr is single-language; tesseract and Vision take lists
    let sel_note = if auto {
        "Auto-detect is on — ticking a language switches to manual."
    } else if cfg!(target_os = "windows") {
        "The first checked language wins — Media.Ocr takes one."
    } else {
        "All checked languages run together."
    };
    section_title(p, cv, 214.0, "Languages", sel_note);
    // two-column checklist, 40px rows
    const LROW: f64 = 40.0;
    let rows = qs_ocr::LANGS.len().div_ceil(2);
    let gy = 248.0;
    let gh = rows as f64 * LROW + 12.0;
    p.card(cv, (CX, gy, CW, gh));
    let col_w = (CW - 16.0) / 2.0;
    let sel = qs_ocr::lang_codes(&v.cfg.ocr_lang);
    for (i, e) in qs_ocr::LANGS.iter().enumerate() {
        let col = i / rows;
        let row = i % rows;
        let x = CX + 8.0 + col as f64 * col_w;
        let y = gy + 6.0 + row as f64 * LROW;
        let hr = (x, y, col_w, LROW);
        let on = sel.iter().any(|s| s == e.0);
        // a language the engine can't use isn't a control — except an
        // already-checked one, which must stay uncheckable
        let missing = !v.ocr_langs.iter().any(|l| l == e.0);
        let live = !missing || on;
        let hover = live && v.hover == Some(Hit::Lang(i));
        if hover {
            p.fill(cv, hr, 6.0, pal().surface_hover, 1.0);
        }
        if missing && !on {
            p.fill(cv, (x + 6.0, y + 10.0, 20.0, 20.0), 5.0, pal().tile, 0.8);
            p.stroke(
                cv,
                (x + 6.0, y + 10.0, 20.0, 20.0),
                5.0,
                pal().border,
                0.8,
                1.0,
            );
        } else {
            check_box(p, cv, x + 6.0, y + 10.0, on, hover);
        }
        p.text(
            cv,
            x + 34.0,
            y + 25.0,
            13.0,
            e.1,
            if missing { pal().fg_faint } else { pal().fg },
        );
        if missing {
            let note = "not installed";
            p.text(
                cv,
                x + col_w - 16.0 - p.text_w(note, 10.5),
                y + 25.0,
                10.5,
                note,
                pal().fg_faint,
            );
        } else {
            // code sits beside the name so the two read as one option
            p.text(
                cv,
                x + 34.0 + p.text_w(e.1, 13.0) + 8.0,
                y + 25.0,
                10.5,
                e.0,
                pal().fg_faint,
            );
        }
        if live {
            hits.push((hr, Hit::Lang(i)));
        }
    }

    // footer: install action / progress / outcome / per-OS hint
    let fy = gy + gh + 22.0;
    #[cfg(target_os = "linux")]
    {
        if v.installing {
            // pulses while the worker runs — the polkit auth dialog is
            // the user's cue that the click did something
            let a = 0.35 + 0.65 * v.pulse.sin().abs();
            p.text(
                cv,
                CX + 8.0,
                fy + 4.0,
                11.5,
                "Installing language packs… approve the system prompt",
                mix(pal().fg_faint, pal().fg_dim, a),
            );
        } else {
            let missing: Vec<&str> = qs_ocr::LANGS
                .iter()
                .map(|e| e.0)
                .filter(|c| !v.ocr_langs.iter().any(|l| l == *c))
                .collect();
            if !missing.is_empty() {
                let lb = (CX, fy - 6.0, 190.0, 30.0);
                p.ghost_button(
                    cv,
                    lb,
                    &format!("Install {} missing packs", missing.len()),
                    v.hover == Some(Hit::OcrInstall),
                );
                hits.push((lb, Hit::OcrInstall));
                p.text(
                    cv,
                    lb.0 + lb.2 + 14.0,
                    fy + 14.0,
                    11.0,
                    "or: sudo apt install tesseract-ocr-<code>",
                    pal().fg_faint,
                );
            }
            // outcome sits under the button row so the two never overlap
            let my = if missing.is_empty() {
                fy + 4.0
            } else {
                fy + 42.0
            };
            if let Some(m) = v.install_msg {
                p.text(cv, CX + 8.0, my, 11.5, m, pal().fg_dim);
            } else if missing.is_empty() {
                p.text(
                    cv,
                    CX + 8.0,
                    my,
                    11.5,
                    "All listed languages are installed.",
                    pal().fg_faint,
                );
            }
        }
    }
    #[cfg(target_os = "macos")]
    p.text(
        cv,
        CX + 8.0,
        fy + 4.0,
        11.5,
        "Recognition runs through macOS Vision — dimmed entries are unsupported.",
        pal().fg_faint,
    );
    #[cfg(target_os = "windows")]
    p.text(
        cv,
        CX + 8.0,
        fy + 4.0,
        11.5,
        "Missing packs: Windows Settings > Time & Language > Language & region.",
        pal().fg_faint,
    );
}

fn draw_storage(p: &Painter, cv: &mut Canvas, v: &View, hits: &mut Vec<(LRect, Hit)>) {
    p.text(cv, CX, 52.0, 24.0, "Storage", pal().fg);
    p.text(cv, CX, 76.0, 12.5, "Manage captured files.", pal().fg_dim);
    section_title(p, cv, 112.0, "Location", "Where screenshots live.");
    let c = card(CX, 146.0, 2);
    p.card(cv, c);
    row_label(
        p,
        cv,
        (c.0, c.1, c.2, ROW),
        Glyph::Folder,
        &short_path(&v.cfg.save_dir().display().to_string()),
        &format!("{} captures · {}", v.stats.0, human_bytes(v.stats.1)),
    );
    let bb = slot(c.1, 110.0);
    p.ghost_button(cv, bb, "Open in Finder", v.hover == Some(Hit::OpenDir));
    hits.push((bb, Hit::OpenDir));

    // second row: volume headroom — a segmented disk chart made a few
    // MB of captures indistinguishable from rounding noise
    p.line(cv, c.0 + 14.0, c.1 + ROW, c.0 + c.2 - 14.0);
    let r = (c.0, c.1 + ROW, c.2, ROW);
    row_label(
        p,
        cv,
        r,
        Glyph::Folder,
        "Free space",
        "On the disk holding your captures",
    );
    if let Some((total, free)) = v.disk {
        let txt = format!("{} free of {}", human_bytes(free), human_bytes(total));
        p.text(
            cv,
            r.0 + r.2 - 14.0 - p.text_w(&txt, 10.5),
            r.1 + 32.0,
            10.5,
            &txt,
            pal().fg_dim,
        );
    }

    section_title(
        p,
        cv,
        280.0,
        "Retention",
        "Automatically delete old captures.",
    );
    let c = card(CX, 314.0, 2);
    p.card(cv, c);
    let r = (c.0, c.1, c.2, ROW);
    row_label(
        p,
        cv,
        r,
        Glyph::Gear,
        "Keep captures",
        if v.cfg.keep_days == 0 {
            "Files stay until you remove them"
        } else {
            "Older files are removed automatically"
        },
    );
    let kb = (r.0 + r.2 - 14.0 - 150.0, r.1 + 11.0, 150.0, 30.0);
    let label = match v.cfg.keep_days {
        7 => "7 days",
        30 => "30 days",
        _ => "Forever",
    };
    dropdown_btn(
        p,
        cv,
        kb,
        label,
        v.drop_open == Some(Drop::KeepDays),
        v.hover == Some(Hit::Drop(Drop::KeepDays)),
    );
    hits.push((kb, Hit::Drop(Drop::KeepDays)));
    p.line(cv, c.0 + 14.0, c.1 + ROW, c.0 + c.2 - 14.0);
    let r = (c.0, c.1 + ROW, c.2, ROW);
    row_label(
        p,
        cv,
        r,
        Glyph::Folder,
        "Clean up now",
        if v.cfg.keep_days == 0 {
            "Choose a retention period first"
        } else {
            "Apply the retention policy immediately"
        },
    );
    let rb = slot(r.1, 110.0);
    if v.cfg.keep_days == 0 {
        // no policy, no action — a live button that deletes nothing is
        // worse than a visibly disabled one
        p.ghost_disabled(cv, rb, "Clean now");
    } else {
        ghost(
            p,
            cv,
            rb,
            "Clean now",
            v.hover == Some(Hit::Clean),
            v.confirm == Some(Hit::Clean),
        );
        hits.push((rb, Hit::Clean));
    }
    if let Some(m) = v.clean_msg {
        p.text(cv, CX + 8.0, c.1 + c.3 + 26.0, 11.5, m, pal().fg_dim);
    }
}

fn draw_about(p: &Painter, cv: &mut Canvas, v: &View, hits: &mut Vec<(LRect, Hit)>) {
    let mid = CX + CW / 2.0;
    p.text(
        cv,
        mid - p.text_w("Quickshort", 22.0) / 2.0,
        140.0,
        22.0,
        "Quickshort",
        pal().fg,
    );
    let ver = concat!("v", env!("CARGO_PKG_VERSION"));
    p.text(
        cv,
        mid - p.text_w(ver, 12.5) / 2.0,
        162.0,
        12.5,
        ver,
        pal().fg_dim,
    );
    p.text(
        cv,
        mid - p.text_w("Fast native screenshots — annotate, OCR, share.", 11.5) / 2.0,
        186.0,
        11.5,
        "Fast native screenshots — annotate, OCR, share.",
        pal().fg_faint,
    );

    let (label, w) = match v.update_ver {
        Some(vr) => (format!("Update to v{vr} — Restart"), 240.0),
        None => ("Check for Updates…".to_string(), 180.0),
    };
    let ub = (mid - w / 2.0, 220.0, w, 32.0);
    p.ghost_button(cv, ub, &label, v.hover == Some(Hit::Update));
    hits.push((ub, Hit::Update));

    // links
    let lw: f64 = LINKS
        .iter()
        .map(|(t, _)| p.text_w(t, 12.0) + 28.0)
        .sum::<f64>()
        - 28.0;
    let mut lx = mid - lw / 2.0;
    for (i, (t, _)) in LINKS.iter().enumerate() {
        let tw = p.text_w(t, 12.0);
        p.text(
            cv,
            lx,
            292.0,
            12.0,
            t,
            if v.hover == Some(Hit::Link(i)) {
                pal().fg
            } else {
                p.accent_deep
            },
        );
        if v.hover == Some(Hit::Link(i)) {
            p.seg(cv, lx, 298.0, lx + tw, 298.0, 1.0, pal().fg);
        }
        hits.push(((lx, 282.0, tw, 20.0), Hit::Link(i)));
        lx += tw + 28.0;
    }

    p.text(
        cv,
        mid - p.text_w("Free and open source — MIT", 10.5) / 2.0,
        570.0,
        10.5,
        "Free and open source — MIT",
        pal().fg_faint,
    );
}

// config plumbing -----------------------------------------------------------

fn key_str(cfg: &Config, r: Rec) -> &str {
    match r {
        Rec::Save => &cfg.key_save,
        Rec::Copy => &cfg.key_copy,
        Rec::Ocr => &cfg.key_ocr,
        Rec::Share => &cfg.key_share,
        Rec::Scroll => &cfg.key_scroll,
        Rec::Record => &cfg.key_record,
        Rec::Undo => &cfg.key_undo,
        Rec::Global | Rec::GlobalFull => "",
    }
}

/// Assign `ch` to a key slot; if another slot already uses it, swap so the
/// binding stays unique — silent duplicates would make one action dead.
fn set_key(cfg: &mut Config, slot: Rec, ch: char) {
    let old = key_str(cfg, slot).chars().next().unwrap_or(' ');
    for other in [
        Rec::Save,
        Rec::Copy,
        Rec::Ocr,
        Rec::Share,
        Rec::Scroll,
        Rec::Record,
        Rec::Undo,
    ] {
        if other != slot && key_str(cfg, other).chars().next() == Some(ch) {
            *key_mut(cfg, other) = old.to_string();
        }
    }
    *key_mut(cfg, slot) = ch.to_string();
}

fn key_mut(cfg: &mut Config, r: Rec) -> &mut String {
    match r {
        Rec::Save => &mut cfg.key_save,
        Rec::Copy => &mut cfg.key_copy,
        Rec::Ocr => &mut cfg.key_ocr,
        Rec::Share => &mut cfg.key_share,
        Rec::Scroll => &mut cfg.key_scroll,
        Rec::Record => &mut cfg.key_record,
        Rec::Undo => &mut cfg.key_undo,
        Rec::Global | Rec::GlobalFull => unreachable!(),
    }
}

fn drop_apply(cfg: &mut Config, d: Drop, i: usize) {
    match d {
        Drop::RecentN => cfg.recent_max = [4u8, 8, 12][i.min(2)],
        Drop::KeepDays => {
            cfg.keep_days = [0u32, 7, 30][i.min(2)];
            // apply the new policy immediately, not just on next launch
            actions::cleanup_captures(&cfg.save_dir(), cfg.keep_days);
        }
        Drop::PopupPos => {
            cfg.popup_pos = POPUP_POS
                .get(i)
                .map(|(k, _)| *k)
                .unwrap_or(POPUP_POS[0].0)
                .into();
        }
        Drop::Theme => {
            cfg.theme = THEMES.get(i).map(|(k, _)| *k).unwrap_or(THEMES[0].0).into();
            // live: the next frame of every window reads the new palette
            qs_theme::set_theme(&cfg.theme);
        }
        Drop::AfterCapture => {
            cfg.after_capture = AFTER_CAPTURE
                .get(i)
                .map(|(k, _)| *k)
                .unwrap_or(AFTER_CAPTURE[0].0)
                .into();
        }
    }
}

/// Thumbnails sized in *physical* px for the window's current scale.
fn load_thumbs(recents: &[PathBuf], scale: f64) -> Vec<Option<image::RgbaImage>> {
    let (tw, th) = ((THUMB_W * scale) as u32, (THUMB_H * scale) as u32);
    recents
        .iter()
        .map(|p| {
            image::open(p).ok().map(|i| {
                i.resize_to_fill(tw, th, image::imageops::FilterType::Triangle)
                    .to_rgba8()
            })
        })
        .collect()
}

/// Restore app activation + window focus after a modal panel closes —
/// without this an accessory app stays deactivated and the settings
/// window stops receiving keyboard input.
fn refocus(win: &Window) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        if let Some(mtm) = MainThreadMarker::new() {
            NSApplication::sharedApplication(mtm).activate();
        }
    }
    win.focus_window();
    win.request_redraw();
}

/// Pin the window to the Dark appearance — the settings UI is
/// modern dark studio-themed, matching the macOS dark titlebar.
#[cfg(target_os = "macos")]
fn force_dark_appearance(win: &Window) {
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua, NSView,
    };
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = win.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(h) = handle.as_raw() else {
        return;
    };
    let ns_view: &NSView = unsafe { &*(h.ns_view.as_ptr() as *const NSView) };
    let Some(ns_win) = ns_view.window() else {
        return;
    };
    unsafe {
        ns_win.setAppearance(NSAppearance::appearanceNamed(NSAppearanceNameDarkAqua).as_deref());
    }
}

#[cfg(not(target_os = "macos"))]
fn force_dark_appearance(_win: &Window) {}

/// right-trimmed tail for display
fn tail(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let keep: String = s.chars().rev().take(n).collect::<String>();
        format!("…{}", keep.chars().rev().collect::<String>())
    }
}

/// left-preserved prefix with ellipsis for filenames
fn truncate_head(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let keep: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{keep}…")
    }
}

/// "/Users/x/…" -> "~/…" for display
fn short_path(s: &str) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let h = home.to_string_lossy();
        if s.starts_with(h.as_ref()) {
            return format!("~{}", &s[h.len()..]);
        }
    }
    s.to_string()
}

/// "1.4 MB" from a byte count.
fn human_bytes(b: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let (mut v, mut u) = (b as f64, 0);
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

/// "just now" / "5m ago" / "3h ago" / "2d ago" / "6w ago" from file mtime
fn age_label(p: &PathBuf) -> String {
    let Ok(m) = std::fs::metadata(p).and_then(|m| m.modified()) else {
        return String::new();
    };
    let Ok(d) = m.elapsed() else {
        return String::new();
    };
    let s = d.as_secs();
    if s < 60 {
        "just now".into()
    } else if s < 3600 {
        format!("{}m ago", s / 60)
    } else if s < 86_400 {
        format!("{}h ago", s / 3600)
    } else if s < 7 * 86_400 {
        format!("{}d ago", s / 86_400)
    } else {
        format!("{}w ago", s / (7 * 86_400))
    }
}

/// winit `Key` -> global-hotkey key token.
fn key_token(key: &Key) -> Option<String> {
    match key {
        Key::Character(c) => {
            let u = c.to_uppercase();
            (u.chars().count() == 1).then_some(u)
        }
        Key::Named(n) => Some(match n {
            NamedKey::Space => "Space".into(),
            NamedKey::Enter => "Enter".into(),
            NamedKey::Tab => "Tab".into(),
            NamedKey::Backspace => "Backspace".into(),
            NamedKey::Delete => "Delete".into(),
            NamedKey::Insert => "Insert".into(),
            NamedKey::Home => "Home".into(),
            NamedKey::End => "End".into(),
            NamedKey::PageUp => "PageUp".into(),
            NamedKey::PageDown => "PageDown".into(),
            NamedKey::ArrowUp => "Up".into(),
            NamedKey::ArrowDown => "Down".into(),
            NamedKey::ArrowLeft => "Left".into(),
            NamedKey::ArrowRight => "Right".into(),
            NamedKey::PrintScreen => "PrintScreen".into(),
            other => {
                let d = format!("{other:?}");
                if d.starts_with('F') && d[1..].chars().all(|c| c.is_ascii_digit()) {
                    d
                } else {
                    return None;
                }
            }
        }),
        _ => None,
    }
}

/// First usable system font (fontdue reads TTF/OTF) — bundled Be Vietnam Pro first,
/// then fallback to platform system fonts. Shared with the toast and quick access.
pub(crate) fn system_font() -> Result<Font> {
    const BUNDLED: &[u8] = include_bytes!("../../qs-render/fonts/BeVietnamPro-Regular.ttf");
    if let Ok(f) = Font::from_bytes(BUNDLED, FontSettings::default()) {
        return Ok(f);
    }
    const CANDIDATES: &[&str] = &[
        #[cfg(target_os = "macos")]
        "/System/Library/Fonts/SFNS.ttf",
        #[cfg(target_os = "macos")]
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        #[cfg(target_os = "macos")]
        "/System/Library/Fonts/Supplemental/Helvetica.ttf",
        #[cfg(target_os = "windows")]
        "C:\\Windows\\Fonts\\segoeui.ttf",
        #[cfg(target_os = "windows")]
        "C:\\Windows\\Fonts\\arial.ttf",
        #[cfg(target_os = "linux")]
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        #[cfg(target_os = "linux")]
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ];
    for p in CANDIDATES {
        if let Ok(bytes) = std::fs::read(p) {
            if let Ok(f) = Font::from_bytes(bytes.as_slice(), FontSettings::default()) {
                return Ok(f);
            }
        }
    }
    anyhow::bail!("no usable system font found");
}

/// Dev harness: `quickshort --settings` opens just the preferences window.
pub fn standalone() -> Result<()> {
    struct Run {
        cfg: Config,
        s: Option<Settings>,
    }
    impl winit::application::ApplicationHandler for Run {
        fn resumed(&mut self, el: &ActiveEventLoop) {
            match Settings::open(el, &self.cfg) {
                Ok(s) => self.s = Some(s),
                Err(e) => {
                    eprintln!("settings: {e:#}");
                    el.exit();
                }
            }
        }
        fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, ev: WindowEvent) {
            let Some(s) = &mut self.s else { return };
            if s.win_id() != id {
                return;
            }
            if let Some(e) = s.window_event(&mut self.cfg, ev) {
                match e {
                    SettingsEvent::Changed => self.cfg.save(),
                    SettingsEvent::HotkeyRecord
                    | SettingsEvent::CheckUpdates
                    | SettingsEvent::InstallUpdate => {} // standalone: no host
                    SettingsEvent::Closed => el.exit(),
                }
            }
        }
        fn about_to_wait(&mut self, el: &ActiveEventLoop) {
            if let Some(s) = &mut self.s {
                s.about_to_wait();
                // a pack install reports via shared state — wake the loop
                // to poll it instead of waiting for a window event
                if s.poll_needed() {
                    el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                        std::time::Instant::now() + std::time::Duration::from_millis(250),
                    ));
                }
            }
        }
    }
    let el = winit::event_loop::EventLoop::new()?;
    let mut cfg = Config::load();
    // QS_ACCENT=name recolors the snapshot without touching saved prefs
    if let Ok(a) = std::env::var("QS_ACCENT") {
        cfg.accent = a;
    }
    // QS_THEME=dark|light previews a theme without touching saved prefs
    if let Ok(t) = std::env::var("QS_THEME") {
        cfg.theme = t;
        qs_theme::set_theme(&cfg.theme);
    }
    let mut run = Run { cfg, s: None };
    el.run_app(&mut run)?;
    Ok(())
}
