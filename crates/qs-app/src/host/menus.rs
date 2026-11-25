//! Menu actions — tray menu, chrome thumb context menu and the
//! quick-access card (the TrayAction arm of the user_event match).

use super::*;

impl Host {
    pub(super) fn on_menu(&mut self, el: &ActiveEventLoop, id: MenuId) {
        let id = id.0.as_str();
        match id {
            tray::ID_CAPTURE => self.trigger(el),
            tray::ID_CAPTURE_FULL => self.trigger_full(el),
            tray::ID_OPEN_DIR => actions::open_path(&self.cfg.save_dir()),
            tray::ID_EVIDENCE_START => {
                match crate::evidence::Session::start(&self.cfg.save_dir()) {
                    Ok(s) => self.evidence = Some(s),
                    Err(e) => eprintln!("evidence: {e:#}"),
                }
                self.rebuild_menu();
            }
            tray::ID_EVIDENCE_NEXT => {
                if let Some(s) = &mut self.evidence {
                    s.next_case();
                }
                self.rebuild_menu();
            }
            tray::ID_EVIDENCE_END => {
                if let Some(s) = self.evidence.take() {
                    actions::open_path(&s.finish());
                }
                self.rebuild_menu();
            }
            tray::ID_QUIT => el.exit(),
            tray::ID_DONATE => actions::open_path(std::path::Path::new(
                "https://truongnat.github.io/quickshort/#donate",
            )),
            tray::ID_UPDATE => {
                if let Some(rel) = self.pending_update.take() {
                    self.install_update(el, rel);
                } else {
                    self.check_updates(el, true);
                }
            }
            tray::ID_SETTINGS => {
                self.refresh_appearance();
                if self.settings.is_none() {
                    match Settings::open(el, &self.cfg) {
                        Ok(mut s) => {
                            s.set_update(self.pending_update.as_ref().map(|r| r.version.clone()));
                            self.settings = Some(s);
                        }
                        Err(e) => eprintln!("settings: {e:#}"),
                    }
                } else {
                    // focus the existing window
                    if let Some(s) = &self.settings {
                        s.win().request_user_attention(None);
                        s.focus();
                    }
                }
            }
            tray::ID_GDRIVE => {
                let before = self.cfg.clone();
                self.cfg.share_to_drive = !self.cfg.share_to_drive;
                self.cfg.save_changes(&before);
                self.rebuild_menu();
            }
            tray::ID_GDRIVE_DISCONNECT => {
                crate::gdrive::disconnect();
                self.rebuild_menu();
            }
            tray::ID_LOGIN => {
                let before = self.cfg.clone();
                self.cfg.launch_at_login = !self.cfg.launch_at_login;
                if let Err(e) = tray::set_launch_at_login(self.cfg.launch_at_login) {
                    eprintln!("launch-at-login: {e:#}");
                    self.cfg.launch_at_login = !self.cfg.launch_at_login; // revert
                }
                self.cfg.save_changes(&before);
                self.rebuild_menu();
            }
            _ => {
                if let Some(p) = id.strip_prefix(tray::ID_RECENT_PREFIX) {
                    actions::open_path(&PathBuf::from(p));
                }
            }
        }
    }

    /// Run a context-menu action the in-chrome menu emitted.
    pub(super) fn on_chrome_menu(&mut self, el: &ActiveEventLoop, wid: WindowId, idx: usize, id: &str) {
        match id {
            chrome::MENU_REVEAL => {
                if let Some(p) = self.chrome.shot(wid, idx).and_then(|s| s.path.clone()) {
                    actions::open_path(&p);
                }
            }
            chrome::MENU_COPY => {
                if let Some(img) = self.chrome.shot(wid, idx).map(|s| s.img.clone()) {
                    actions::copy_image(&img, false);
                }
            }
            chrome::MENU_SAVE => {
                let shot = self.chrome.shot(wid, idx);
                let mode = shot.map(|s| s.capture_mode()).unwrap_or_default();
                let img = shot.map(|s| s.img.clone());
                if let Some(img) = img {
                    let dir = self.cfg.save_dir();
                    match actions::save_to_dir(&img, &dir) {
                        Ok(p) => {
                            actions::record_saved(
                                &dir,
                                &p,
                                qs_library::Provenance {
                                    mode,
                                    source_app: chrome::frontmost_app_name(),
                                },
                            );
                            if let Some(s) = self.chrome.shot_mut(wid, idx) {
                                s.path = Some(p);
                            }
                            self.rebuild_menu();
                            self.toast_done(el, "Screenshot saved".to_string(), true);
                        }
                        Err(e) => self.toast_done(el, format!("Save failed: {e}"), false),
                    }
                }
            }
            chrome::MENU_PIN => {
                if let Some(img) = self.chrome.shot(wid, idx).map(|s| s.img.clone()) {
                    match Pin::open(el, &img) {
                        Ok(p) => self.pins.push(p),
                        Err(e) => eprintln!("pin: {e:#}"),
                    }
                }
            }
            chrome::MENU_BEAUTIFY => {
                // a beautified copy lands as its own shot — the original
                // stays untouched, like the card's flow
                if let Some(img) = self.chrome.shot(wid, idx).map(|s| s.img.clone()) {
                    let pretty = qs_core::beautify(&img);
                    let _ = self.chrome.land(
                        el,
                        Land {
                            img: pretty,
                            kind: chrome::ShotKind::Saved,
                            path: None,
                            sel: None,
                            instant: false,
                            dur_s: None,
                            shared: None,
                        },
                    );
                }
            }
            chrome::MENU_SHARE => {
                if let Some(img) = self.chrome.shot(wid, idx).map(|s| (*s.img).clone()) {
                    let sm = self
                        .share
                        .get_or_insert_with(crate::share::ShareManager::new)
                        .clone();
                    let px = self.proxy.clone();
                    std::thread::spawn(move || {
                        let res = sm.share(&img).map_err(|e| e.to_string());
                        let _ = px.send_event(HostEvent::ShareDone(res, wid, idx));
                    });
                }
            }
            chrome::MENU_REVOKE => {
                if let Some(e) = self.chrome.shot(wid, idx).and_then(|s| s.shared.clone()) {
                    self.revoke(el, e, Some((wid, idx)));
                }
            }
            chrome::MENU_OPEN => {
                // Open the shot in Quickshort Editor (or OS default image editor).
                // Unsaved shots go to a cache-dir PNG first so the editor gets a path.
                let shot_path = self.chrome.shot(wid, idx).and_then(|s| s.path.clone());
                let mut p = shot_path;
                if p.is_none() {
                    if let Some(img) = self.chrome.shot(wid, idx).map(|s| s.img.clone()) {
                        let dir = dirs::cache_dir()
                            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
                            .join("quickshort");
                        match actions::save_to_dir(&img, &dir) {
                            Ok(saved) => p = Some(saved),
                            Err(e) => eprintln!("open: save failed: {e:#}"),
                        }
                    }
                }
                if let Some(p) = p {
                    actions::open_editor(&p);
                }
            }
            chrome::MENU_DEL => {
                self.chrome.remove(wid, idx);
                self.chrome.sweep();
            }
            chrome::MENU_CLEAR => {
                self.chrome.clear_all(wid);
                self.chrome.sweep();
            }
            _ => {}
        }
    }

    /// Open the post-capture thumbnail card; falls back to the plain toast
    /// when the window can't be created.
    #[cfg(not(target_os = "linux"))]
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(super) fn open_card(
        &mut self,
        el: &ActiveEventLoop,
        img: image::RgbaImage,
        msg: &str,
        saved: Option<PathBuf>,
    ) {
        self.toast = None;
        match QuickAccess::open(
            el,
            img,
            msg,
            saved,
            self.cfg.accent_rgb(),
            true,
            &self.cfg.popup_pos,
        ) {
            Ok(c) => self.card = Some(c),
            Err(e) => {
                eprintln!("card: {e:#}");
                self.toast_done(el, msg.to_string(), true);
            }
        }
    }

    /// A click on the card's action pills — everything runs with the image
    /// the card still owns.
    pub(super) fn on_card_action(&mut self, el: &ActiveEventLoop, act: QaAction) {
        if matches!(act, QaAction::Close) {
            self.card = None;
            return;
        }
        let mut rebuild = false;
        let mut pin = false;
        if let Some(c) = &mut self.card {
            match act {
                QaAction::Copy => {
                    actions::copy_image(c.image(), false);
                    c.notify("Copied to clipboard");
                }
                QaAction::SaveNow => {
                    let dir = self.cfg.save_dir();
                    match actions::save_to_dir(c.image(), &dir) {
                        Ok(p) => {
                            actions::record_saved(
                                &dir,
                                &p,
                                qs_library::Provenance {
                                    mode: qs_library::CaptureMode::Unknown,
                                    source_app: chrome::frontmost_app_name(),
                                },
                            );
                            c.set_saved(p);
                            c.notify("Screenshot saved");
                            rebuild = true;
                        }
                        Err(e) => c.notify(&format!("Save failed: {e}")),
                    }
                }
                QaAction::Reveal => {
                    if let Some(p) = c.saved_path() {
                        actions::open_path(&p);
                    }
                }
                QaAction::Beautify => c.beautify(),
                QaAction::Pin => pin = true,
                QaAction::Edit => {
                    // unsaved cards go through a cache PNG so the editor
                    // has a path — same as the shelf's Open
                    let p = c.saved_path().or_else(|| {
                        let dir = dirs::cache_dir()
                            .unwrap_or_else(|| PathBuf::from("/tmp"))
                            .join("quickshort");
                        actions::save_to_dir(c.image(), &dir)
                            .map_err(|e| eprintln!("edit: save failed: {e:#}"))
                            .ok()
                    });
                    if let Some(p) = p {
                        actions::open_editor(&p);
                    }
                }
                QaAction::Share => {
                    let sm = self
                        .share
                        .get_or_insert_with(crate::share::ShareManager::new)
                        .clone();
                    let img = c.image().clone();
                    let px = self.proxy.clone();
                    c.set_sharing(true);
                    std::thread::spawn(move || {
                        let res = sm.share(&img).map_err(|e| e.to_string());
                        let _ = px.send_event(HostEvent::QuickShare(res));
                    });
                }
                QaAction::Revoke => {
                    if let Some(e) = self.card_share.take() {
                        self.revoke(el, e, None);
                    }
                }
                QaAction::Close => unreachable!(),
            }
        }
        if rebuild {
            self.rebuild_menu();
        }
        if pin {
            if let Some(c) = &self.card {
                match Pin::open(el, c.image()) {
                    Ok(p) => self.pins.push(p),
                    Err(e) => eprintln!("pin: {e:#}"),
                }
            }
        }
    }
}
