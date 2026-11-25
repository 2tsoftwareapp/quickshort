//! Resident tray app: one persistent winit event loop; the overlay is
//! spawned on demand (tray menu click or global hotkey) and torn down when
//! the selection phase ends.

use anyhow::Result;
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use muda::{MenuEvent, MenuId};
use qs_overlay::{Overlay, OverlayEvent};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::WindowId;

use crate::actions::{self, Dispatch, Outcome, SaveTarget};
use crate::chrome::{self, Chrome, ChromeCmd, ChromeEv, Land, ShotKind};
use crate::config::{AfterCapture, Config};
use crate::pin::Pin;
use crate::quick_access::{QaAction, QuickAccess};
use crate::settings::{Settings, SettingsEvent};
use crate::toast::Toast;
use crate::tray;
use crate::update::{self, Release};

/// Everything that wakes the loop besides window events.
pub(crate) enum HostEvent {
    /// Tray "Capture Region" or the region global hotkey.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))] // Wayland-only sender
    Trigger,
    /// Tray "Capture Full Screen" or the fullscreen global hotkey —
    /// stitches the whole virtual screen and saves without the overlay.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))] // Wayland-only sender
    TriggerFull,
    /// A `global-hotkey` manager press, identified by the HotKey's id —
    /// routed to Trigger/TriggerFull against the registered chords.
    Hotkey(u32),
    Menu(MenuId),
    /// Action worker finished — refresh the menu, open the result card.
    Done(Result<Dispatch, String>),
    /// A share upload kicked off from the quick-access card finished.
    QuickShare(Result<crate::share::SharedLink, String>),
    /// A share upload kicked off from an album thumb finished — carries
    /// the shot coords so the ↗ chip can light up on the right thumb.
    ShareDone(Result<crate::share::SharedLink, String>, WindowId, usize),
    /// A revoke finished — Ok carries the share URL so the ledger +
    /// the shot's ↗ chip can both drop it.
    RevokeDone(
        Result<(), String>,
        crate::share::ShareEntry,
        Option<(WindowId, usize)>,
    ),
    /// Scroll-capture worker finished — the stitched tall image or error,
    /// plus the selection for the fly-in origin.
    ScrollDone(Result<image::RgbaImage, String>, qs_core::Rect),
    /// Recording worker finished — (first frame, mp4 path) or error, plus
    /// the selection.
    RecDone(
        Result<(image::RgbaImage, std::path::PathBuf, u64), String>,
        qs_core::Rect,
    ),
    /// Release check finished — `manual` gates the "up to date" toast.
    UpdateCheck {
        res: Result<Option<Release>, String>,
        manual: bool,
    },
    /// Update install finished — relaunch on success.
    UpdateInstall(Result<PathBuf, String>),
    /// Input raised by a chrome-layer view (stack hover/clicks/keys).
    Chrome(WindowId, ChromeEv),
}

/// Run the resident tray app. Blocks until Quit.
pub fn run(cfg: Config) -> Result<()> {
    let el = EventLoop::<HostEvent>::with_user_event().build()?;
    el.set_control_flow(ControlFlow::Wait);
    let proxy = el.create_proxy();

    #[cfg(target_os = "linux")]
    if let Err(e) = crate::ipc_linux::spawn_listener(proxy.clone()) {
        log::warn!("ipc listener: {e:#} — hotkeys will use one-shot capture");
    }

    // Menu clicks arrive on an mpsc channel — pump them into the event loop.
    let px = proxy.clone();
    std::thread::spawn(move || {
        while let Ok(e) = MenuEvent::receiver().recv() {
            if px.send_event(HostEvent::Menu(e.id().clone())).is_err() {
                return;
            }
        }
    });
    // Hotkey events: forward the HotKey id — the host maps it to the
    // region or fullscreen chord (ids are per-registration, so they
    // can't be baked in here).
    let px = proxy.clone();
    GlobalHotKeyEvent::set_event_handler(Some(Box::new(move |e: GlobalHotKeyEvent| {
        if e.state == HotKeyState::Pressed {
            let _ = px.send_event(HostEvent::Hotkey(e.id));
        }
    })));

    let mut host = Host::new(cfg, proxy);
    el.run_app(&mut host)?;
    Ok(())
}

mod hotkeys;
mod menus;
mod results;

struct Host {
    cfg: Config,
    /// Running evidence session — numbers saved captures per test case.
    evidence: Option<crate::evidence::Session>,
    proxy: EventLoopProxy<HostEvent>,
    overlay: Option<Overlay>,
    /// Frames captured for the active overlay session — kept for stitching.
    frames: Option<Vec<qs_core::CapturedFrame>>,
    /// Linux: gtk pins the tray to its own thread — the handle is a channel
    #[cfg(target_os = "linux")]
    tray: Option<tray::TrayHandle>,
    #[cfg(not(target_os = "linux"))]
    tray: Option<tray_icon::TrayIcon>,
    hotkeys: Option<GlobalHotKeyManager>,
    hotkey: Option<HotKey>,
    /// the fullscreen-capture chord, registered beside `hotkey`
    hotkey_full: Option<HotKey>,
    /// hotkey unregistered while the settings recorder is armed
    hotkey_suspended: bool,
    /// Wayland compositor/portal binding — gsettings custom keybinding,
    /// KDE GlobalShortcuts session, sway/hyprland IPC. Present only when
    /// hotkey_linux::register found a desktop mechanism.
    #[cfg(target_os = "linux")]
    hotkey_guard: Option<crate::hotkey_linux::Guard>,
    settings: Option<Settings>,
    next_appearance_poll: Instant,
    toast: Option<Toast>,
    /// Post-capture thumbnail card — fallback where the chrome layer
    /// isn't implemented yet (Windows/Linux until their backends land).
    card: Option<QuickAccess>,
    /// Album stack surfaces — one transparent window per monitor.
    chrome: Chrome,
    /// Floating always-on-top reference images.
    pins: Vec<Pin>,
    /// Recording stop card + the flag its worker watches. The window is
    /// dropped on Stop; the flag lives on until the encoder finishes.
    rec: Option<(
        crate::recstop::RecStop,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
    )>,
    /// Newer release found, waiting for the user to click Update.
    pending_update: Option<Release>,
    /// Lazy — the listener + ssh tunnel only spawn on the first Share.
    share: Option<std::sync::Arc<crate::share::ShareManager>>,
    /// Live share links — backs the ↗ chip, the Revoke row, and undoing
    /// a card share. Loaded lazily, flushed after every mutation.
    shares: crate::share::ShareStore,
    /// The link the open card just minted — what its Revoke pill kills.
    card_share: Option<crate::share::ShareEntry>,
}

impl Host {
    fn new(cfg: Config, proxy: EventLoopProxy<HostEvent>) -> Self {
        let px = proxy.clone();
        #[cfg(target_os = "linux")]
        {
            let px_gtk = px.clone();
            crate::chrome::gtk_album::set_chrome_push(std::sync::Arc::new(move |wid, ev| {
                let _ = px_gtk.send_event(HostEvent::Chrome(wid, ev));
            }));
        }
        let chrome = Chrome::new(
            cfg.accent_rgb(),
            &cfg.popup_pos,
            Box::new(move |wid| {
                let px = px.clone();
                std::rc::Rc::new(move |ev| {
                    let _ = px.send_event(HostEvent::Chrome(wid, ev));
                })
            }),
        );
        Self {
            cfg,
            proxy,
            overlay: None,
            frames: None,
            tray: None,
            evidence: None,
            hotkeys: None,
            hotkey: None,
            hotkey_full: None,
            hotkey_suspended: false,
            #[cfg(target_os = "linux")]
            hotkey_guard: None,
            settings: None,
            next_appearance_poll: Instant::now(),
            toast: None,
            card: None,
            chrome,
            pins: Vec::new(),
            rec: None,
            pending_update: None,
            share: None,
            shares: crate::share::ShareStore::load(),
            card_share: None,
        }
    }

    fn rebuild_menu(&self) {
        let recents = actions::recent_captures(&self.cfg.save_dir(), self.cfg.recent_max as usize);
        let update = self.pending_update.as_ref().map(|r| r.version.clone());
        let evidence = self.evidence.as_ref().map(|s| s.label());
        #[cfg(target_os = "linux")]
        if let Some(t) = &self.tray {
            t.set_menu(self.cfg.clone(), recents, update, evidence);
        }
        #[cfg(not(target_os = "linux"))]
        if let Some(t) = &self.tray {
            t.set_menu(Some(Box::new(tray::build_menu(
                &self.cfg,
                &recents,
                update.as_deref(),
                evidence.as_deref(),
            ))));
        }
    }

    /// Where a capture is written: the next numbered evidence file while a
    /// session runs (saving actions only), else the capture folder.
    fn save_target(&mut self, action: qs_overlay::Action) -> SaveTarget {
        use qs_overlay::Action as A;
        match &mut self.evidence {
            Some(s) if matches!(action, A::Copy | A::Save | A::Edit) => {
                let p = s.next_path();
                self.rebuild_menu();
                SaveTarget::File(p)
            }
            _ => SaveTarget::Dir(self.cfg.save_dir()),
        }
    }




    /// Our own toast and notifications must not end up in the next shot.
    fn clear_own_popups(&mut self) {
        self.toast = None;
        #[cfg(target_os = "linux")]
        if crate::linux_notify::dismiss_all() {
            // give the notification server a frame to unmap the popup
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }

    fn refresh_appearance(&mut self) {
        if self.cfg.refresh_appearance() {
            if let Some(settings) = &self.settings {
                settings.win().request_redraw();
            }
        }
    }






    /// Persist config, (re)register the hotkeys, refresh the tray menu —
    /// called after the settings window mutates `cfg`.
    fn apply_cfg(&mut self, el: &ActiveEventLoop, before: &Config) {
        self.cfg.save_changes(before);
        self.chrome.set_popup_pos(&self.cfg.popup_pos);
        let (region, full) = (self.cfg.hotkey.clone(), self.cfg.hotkey_full.clone());
        self.rebind_slot(el, &region, false);
        self.rebind_slot(el, &full, true);
        // Wayland: re-issue the compositor/portal bindings for the new
        // chords (re-BindShortcuts for KDE, gsettings/swaymsg/hyprctl
        // overwrite)
        #[cfg(target_os = "linux")]
        if let Some(g) = &mut self.hotkey_guard {
            if let Err(e) = g.rebind(&region, &full) {
                log::warn!("hotkey rebind failed: {e:#}");
            }
            // a Changed event while the recorder is still armed must not
            // leave the binding live
            g.suspend(self.hotkey_suspended);
        }
        self.rebuild_menu();
    }







    fn on_chrome_ev(&mut self, el: &ActiveEventLoop, id: WindowId, ev: ChromeEv) {

        let Some((wid, cmd)) = self.chrome.event(id, ev) else {
            return;
        };
        match cmd {
            ChromeCmd::MenuAction { idx, id } => self.on_chrome_menu(el, wid, idx, id),
            ChromeCmd::Remove(i) => {
                self.chrome.remove(wid, i);
                self.chrome.sweep();
            }
            ChromeCmd::ClearAll => {
                self.chrome.clear_all(wid);
                self.chrome.sweep();
            }
            ChromeCmd::Redraw => {} // handled inside Chrome::event
            // consumed inside the backend — beginDraggingSession
            // already ran; this arm only satisfies exhaustiveness
            ChromeCmd::DragBegin { .. } => {}
        }
            
    }
}

impl ApplicationHandler<HostEvent> for Host {
    fn resumed(&mut self, _el: &ActiveEventLoop) {
        // Tray-only app — hide the Dock icon (macOS only; elsewhere no-op).
        #[cfg(target_os = "macos")]
        {
            use objc2::MainThreadMarker;
            use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
            if let Some(mtm) = MainThreadMarker::new() {
                NSApplication::sharedApplication(mtm)
                    .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            }
        }

        // On Wayland the X11 grabs below never fire — register through the
        // compositor's own mechanism (gsettings / portal / swaymsg /
        // hyprctl) and skip the manager entirely when that works.
        #[cfg(target_os = "linux")]
        let try_manager = if crate::hotkey_linux::is_wayland() {
            match crate::hotkey_linux::register(
                self.proxy.clone(),
                &self.cfg.hotkey,
                &self.cfg.hotkey_full,
            ) {
                Ok(guard) => {
                    self.hotkey_guard = guard;
                    if self.hotkey_guard.is_none() {
                        log::info!(
                            "note: no hotkey mechanism for this Wayland desktop — \
                             bind `quickshort --capture` in your compositor config"
                        );
                    }
                    false
                }
                Err(e) => {
                    log::warn!(
                        "hotkey: compositor registration failed: {e:#} — \
                               bind `quickshort --capture` in your compositor config"
                    );
                    true
                }
            }
        } else {
            true
        };
        #[cfg(not(target_os = "linux"))]
        let try_manager = true;

        let recents = actions::recent_captures(&self.cfg.save_dir(), self.cfg.recent_max as usize);
        // Rehydrate the album only where the capture UI uses it by default.
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        for p in recents.iter().take(5).rev() {
            if let Ok(img) = image::open(p) {
                let _ = self.chrome.land(
                    _el,
                    Land {
                        img: img.to_rgba8(),
                        kind: ShotKind::Saved,
                        path: Some(p.clone()),
                        sel: None,
                        instant: true,
                        dur_s: None,
                        shared: self.shares.get(p).cloned(),
                    },
                );
            }
        }
        #[cfg(target_os = "linux")]
        match tray::linux_tray::spawn(&self.cfg, &recents) {
            Ok(t) => self.tray = Some(t),
            Err(e) => log::warn!("tray icon failed: {e:#}"),
        }
        #[cfg(not(target_os = "linux"))]
        {
            let builder = tray_icon::TrayIcon::builder()
                .with_tooltip("Quickshort — region screenshot")
                .with_menu(Box::new(tray::build_menu(&self.cfg, &recents, None, None)))
                .with_icon(tray::tray_icon(44));
            match builder.build() {
                Ok(t) => self.tray = Some(t),
                Err(e) => log::warn!("tray icon failed: {e:#}"),
            }
        }

        if try_manager {
            // the manager survives a failed grab — register both chords on
            // one instance so a fallback for the region chord doesn't
            // strand the fullscreen one on a dead manager
            let manager = GlobalHotKeyManager::new().ok();
            match self.cfg.hotkey.parse::<HotKey>() {
                Ok(hk) => {
                    let res = manager.as_ref().map(|m| m.register(hk)).unwrap_or(Err(
                        global_hotkey::Error::FailedToRegister("manager unavailable".into()),
                    ));
                    match res {
                        Ok(()) => self.hotkey = Some(hk),
                        Err(e) => {
                            log::warn!("hotkey register failed: {e:#}");
                            // the configured chord may be shell-owned (Windows
                            // claims PrintScreen for Snipping on fresh installs)
                            // — fall back to a chord that is always free
                            const FALLBACK: &str = "Ctrl+Shift+S";
                            if self.cfg.hotkey != FALLBACK {
                                if let Ok(fbk) = FALLBACK.parse::<HotKey>() {
                                    match manager.as_ref().map(|m| m.register(fbk)) {
                                        Some(Ok(())) => {
                                            log::info!("hotkey fell back to {FALLBACK}");
                                            self.hotkey = Some(fbk);
                                        }
                                        _ => {
                                            log::warn!("fallback register failed")
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => log::warn!("bad hotkey {:?}: {e}", self.cfg.hotkey),
            }
            // fullscreen chord — skipped silently when unset or a
            // duplicate of the region chord
            let full = self.cfg.hotkey_full.trim();
            if !full.is_empty() {
                match full.parse::<HotKey>() {
                    Ok(hk)
                        if self
                            .hotkey
                            .is_some_and(|r| r.key == hk.key && r.mods == hk.mods) =>
                    {
                        log::info!("fullscreen hotkey {full:?} duplicates the region hotkey");
                    }
                    Ok(hk) => match manager.as_ref().map(|m| m.register(hk)) {
                        Some(Ok(())) => self.hotkey_full = Some(hk),
                        Some(Err(e)) => log::warn!("fullscreen hotkey register failed: {e:#}"),
                        None => log::warn!("fullscreen hotkey: manager unavailable"),
                    },
                    Err(e) => log::warn!("bad fullscreen hotkey {full:?}: {e}"),
                }
            }
            self.hotkeys = manager;
        }

        // honor the retention policy, then the once-a-day update check
        let gone = actions::cleanup_captures(&self.cfg.save_dir(), self.cfg.keep_days);
        if gone > 0 {
            log::info!("retention sweep removed {gone} old captures");
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if self.cfg.auto_update && now.saturating_sub(self.cfg.last_update_check) > 24 * 3600 {
            self.check_updates(_el, false);
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, ev: HostEvent) {
        match ev {
            HostEvent::Trigger => self.on_capture_trigger(el, false),
            HostEvent::TriggerFull => self.on_capture_trigger(el, true),
            HostEvent::Hotkey(id) => self.on_hotkey(el, id),
            HostEvent::Menu(id) => self.on_menu(el, id),
            HostEvent::UpdateCheck { res, manual } => self.on_update_check(el, res, manual),
            HostEvent::UpdateInstall(res) => self.on_update_install(el, res),
            HostEvent::Done(res) => self.on_done(el, res),
            HostEvent::ScrollDone(res, sel) => self.on_scroll_done(el, res, sel),
            HostEvent::RecDone(res, sel) => self.on_rec_done(el, res, sel),
            HostEvent::QuickShare(res) => self.on_quick_share(el, res),
            HostEvent::ShareDone(res, wid, idx) => self.on_share_done(el, res, wid, idx),
            HostEvent::RevokeDone(res, e, shot) => self.on_revoke_done(el, res, e, shot),
            HostEvent::Chrome(id, ev) => self.on_chrome_ev(el, id, ev),
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.chrome.window_event(id, &event) {
            return;
        }
        if let Some(i) = self.pins.iter().position(|p| p.win_id() == id) {
            if self.pins[i].window_event(event) {
                self.pins.remove(i);
            }
            return;
        }
        if let Some(c) = &mut self.card {
            if c.win_id() == id {
                if let Some(act) = c.window_event(event) {
                    self.on_card_action(el, act);
                }
                return;
            }
        }
        if let Some(t) = &mut self.toast {
            if t.win_id() == id {
                if t.window_event(event) {
                    self.toast = None;
                }
                return;
            }
        }
        if let Some((r, stop)) = &mut self.rec {
            if r.win_id() == id {
                if r.window_event(event) {
                    stop.store(true, std::sync::atomic::Ordering::Relaxed);
                    self.rec = None; // encoder finalizes → RecDone follows
                }
                return;
            }
        }
        let is_settings = self.settings.as_ref().is_some_and(|s| s.win_id() == id);
        if is_settings {
            let s = self.settings.as_mut().unwrap();
            let before = self.cfg.clone();
            let ev = s.window_event(&mut self.cfg, event);
            let recording = s.is_recording();
            match ev {
                Some(SettingsEvent::Changed) => {
                    // resume the grab only when the chord actually ended
                    // recording — a Browse/login change while still armed
                    // must keep it suspended or the next chord fires capture
                    self.suspend_hotkey(recording);
                    self.apply_cfg(el, &before);
                }
                Some(SettingsEvent::HotkeyRecord) => self.suspend_hotkey(recording),
                Some(SettingsEvent::CheckUpdates) => self.check_updates(el, true),
                Some(SettingsEvent::InstallUpdate) => {
                    if let Some(rel) = self.pending_update.take() {
                        self.install_update(el, rel);
                    }
                }
                Some(SettingsEvent::Closed) => {
                    self.suspend_hotkey(false);
                    self.settings = None;
                }
                None => {}
            }
            return;
        }
        let Some(ov) = &mut self.overlay else { return };
        let Some(out) = ov.window_event(id, event) else {
            return;
        };
        self.overlay = None; // dropping destroys the overlay windows
        let Some(frames) = self.frames.take() else {
            return;
        };
        if let OverlayEvent::Done(sel, action, annots) = out {
            // scrolling capture drives its own capture loop and reports
            // the stitched image through ScrollDone
            if action == qs_overlay::Action::Scroll {
                #[cfg(not(target_os = "linux"))]
                match Toast::open(
                    el,
                    crate::scroll::HINT,
                    self.cfg.accent_rgb(),
                    &self.cfg.popup_pos,
                ) {
                    Ok(t) => self.toast = Some(t),
                    Err(e) => log::warn!("toast: {e:#}"),
                }
                #[cfg(target_os = "linux")]
                crate::linux_notify::send(crate::scroll::HINT, true);
                let px = self.proxy.clone();
                std::thread::spawn(move || {
                    let res = crate::scroll::run(sel).map_err(|e| e.to_string());
                    let _ = px.send_event(HostEvent::ScrollDone(res, sel));
                });
                return;
            }
            // recording keeps its own stream open and stops via the
            // floating RecStop card — RecDone carries (thumb, path)
            if action == qs_overlay::Action::Record {
                let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                // without the stop card there'd be no way to end the
                // recording — refuse to start instead of capturing forever
                let r = match crate::recstop::RecStop::open(el, &self.cfg.popup_pos) {
                    Ok(r) => r,
                    Err(e) => {
                        self.toast_done(el, format!("Recording failed: {e:#}"), false);
                        return;
                    }
                };
                self.rec = Some((r, stop.clone()));
                let px = self.proxy.clone();
                let dir = self.cfg.save_dir();
                let stop2 = stop.clone();
                std::thread::spawn(move || {
                    let res = crate::record::run(sel, &dir, stop2).map_err(|e| e.to_string());
                    let _ = px.send_event(HostEvent::RecDone(res, sel));
                });
                return;
            }
            // slow actions get a live toast while the worker runs
            let wait_msg = match action {
                qs_overlay::Action::Ocr => Some("Extracting text…"),
                qs_overlay::Action::Share => Some("Publishing link…"),
                _ => None,
            };
            #[cfg(not(target_os = "linux"))]
            if self.toast.is_none() {
                if let Some(m) = wait_msg {
                    match Toast::open(el, m, self.cfg.accent_rgb(), &self.cfg.popup_pos) {
                        Ok(t) => self.toast = Some(t),
                        Err(e) => log::warn!("toast: {e:#}"),
                    }
                }
            }
            #[cfg(target_os = "linux")]
            if let Some(m) = wait_msg {
                crate::linux_notify::send(m, true);
            }
            // stitch + encode off the UI thread; Done updates menu/toast later
            let px = self.proxy.clone();
            let target = self.save_target(action);
            let share = (action == qs_overlay::Action::Share).then(|| {
                self.share
                    .get_or_insert_with(crate::share::ShareManager::new)
                    .clone()
            });
            let copy = self.cfg.copy_after_save;
            let lang = self.cfg.ocr_lang.clone();
            let prov = actions::provenance_now(qs_library::CaptureMode::Region);
            std::thread::spawn(move || {
                let res = actions::dispatch(actions::DispatchArgs {
                    frames,
                    sel,
                    action,
                    annots: &annots,
                    target: &target,
                    wait_clipboard: false,
                    share,
                    copy_after_save: copy,
                    ocr_lang: &lang,
                    provenance: prov,
                })
                .map_err(|e| e.to_string());
                let _ = px.send_event(HostEvent::Done(res));
            });
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if let Some(ov) = &mut self.overlay {
            ov.about_to_wait();
        }
        // earliest wake wins — the settings poll (pack install) must not
        // be starved by a longer toast timeout, and vice versa
        let mut wake = None;
        if self.settings.is_some() {
            let now = Instant::now();
            if now >= self.next_appearance_poll {
                self.refresh_appearance();
                self.next_appearance_poll = now + Duration::from_millis(500);
            }
            wake = Some(self.next_appearance_poll);
        }
        if let Some(s) = &mut self.settings {
            s.about_to_wait();
            if s.poll_needed() {
                let poll = Instant::now() + Duration::from_millis(250);
                wake = Some(wake.map_or(poll, |next| next.min(poll)));
            }
        }
        if let Some(t) = &mut self.toast {
            match t.pump() {
                Some(w) => wake = Some(wake.map_or(w, |x| x.min(w))),
                None => self.toast = None,
            }
        }
        if let Some(c) = &mut self.card {
            match c.pump() {
                Some(w) => wake = Some(wake.map_or(w, |x| x.min(w))),
                None => self.card = None,
            }
        }
        if let Some((r, _)) = &mut self.rec {
            let w = r.pump();
            wake = Some(wake.map_or(w, |x| x.min(w)));
        }
        // Album chrome shares winit's Wayland display — pumping it during
        // region selection steals compositor time and makes the drag janky.
        if self.overlay.is_none() {
            if let Some(w) = self.chrome.pump() {
                wake = Some(wake.map_or(w, |x| x.min(w)));
            }
        }
        el.set_control_flow(match wake {
            Some(w) => ControlFlow::WaitUntil(w),
            None => ControlFlow::Wait,
        });
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        // Wl::drop sends wl_buffer/wl_region destroys over winit's
        // wl_display — valid only while the loop is still connected,
        // so the windows must die here, not in Demo/Host teardown.
        self.chrome.wins.clear();
    }
}

/// File name for toasts and notifications.
fn file_label(p: &std::path::Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
