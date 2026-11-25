//! Worker outcomes — action/share/scroll/rec/update results
//! landing back on the loop (the CaptureResult arm of user_event).

use super::*;

impl Host {
    pub(super) fn on_revoke_done(&mut self, el: &ActiveEventLoop, res: Result<(), String>, e: crate::share::ShareEntry, shot: Option<(WindowId, usize)>) {

        self.shares.remove_url(&e.url);
        self.shares.save();
        if self.card_share.as_ref().is_some_and(|c| c.url == e.url) {
            self.card_share = None;
            if let Some(c) = &mut self.card {
                c.set_shared(false);
            }
        }
        if let Some((wid, idx)) = shot {
            if let Some(s) = self.chrome.shot_mut(wid, idx) {
                if s.shared.as_ref().is_some_and(|x| x.url == e.url) {
                    s.shared = None;
                }
            }
            self.chrome.redraw(wid);
        }
        match res {
            Ok(()) => self.toast_done(el, "Link revoked".to_string(), true),
            // still drop local state — a 404 means it's already
            // gone, anything else is transient
            Err(err) => self.toast_done(el, format!("Revoke: {err}"), false),
        }
            
    }

    pub(super) fn on_share_done(&mut self, el: &ActiveEventLoop, res: Result<crate::share::SharedLink, String>, wid: WindowId, idx: usize) {
match res {
                Ok(link) => {
                    actions::copy_text(&link.url, false);
                    if let Some(s) = self.chrome.shot_mut(wid, idx) {
                        if let Some(p) = s.path.clone() {
                            let e = crate::share::entry(p, &link);
                            self.shares.put(e.clone());
                            self.shares.save();
                            s.shared = Some(e);
                        } else {
                            // unsaved shot — the link lives but there's
                            // nothing to rehydrate; keep it in memory
                            s.shared = Some(crate::share::entry(PathBuf::new(), &link));
                        }
                        self.chrome.redraw(wid);
                    }
                    self.toast_done(
                        el,
                        format!("Link copied — {}", link.note()),
                        true,
                    );
                }
                Err(e) => self.toast_done(el, format!("Share failed: {e}"), false),
            }
    }

    pub(super) fn on_quick_share(&mut self, _el: &ActiveEventLoop, res: Result<crate::share::SharedLink, String>) {

        let Some(c) = &mut self.card else { return };
        c.set_sharing(false);
        match res {
            Ok(link) => {
                actions::copy_text(&link.url, false);
                // card shots may be unsaved — the keyless path
                // just won't rehydrate after a restart
                let e = crate::share::entry(c.saved_path().unwrap_or_default(), &link);
                self.card_share = Some(e.clone());
                if !e.path.as_os_str().is_empty() {
                    self.shares.put(e);
                    self.shares.save();
                }
                c.set_shared(true);
                c.notify(&format!(
                    "Link copied — {} (Share again to revoke)", link.note()
                ));
            }
            Err(e) => c.notify(&format!("Share failed: {e}")),
        }
            
    }

    pub(super) fn on_rec_done(&mut self, el: &ActiveEventLoop, res: Result<(image::RgbaImage, PathBuf, u64), String>, sel: qs_core::Rect) {

        self.rec = None;
        match res {
            Ok((img, path, dur_s)) => self.land_shot(
                el,
                Land {
                    img,
                    kind: ShotKind::Record,
                    path: Some(path),
                    sel: Some(sel),
                    instant: false,
                    dur_s: Some(dur_s),
                    shared: None,
                },
                "Recording saved",
            ),
            Err(e) => self.toast_done(el, format!("Recording failed: {e}"), false),
        }
            
    }

    pub(super) fn on_scroll_done(&mut self, el: &ActiveEventLoop, res: Result<image::RgbaImage, String>, sel: qs_core::Rect) {
match res {
                Ok(img) => {
                    // a stitched page is the slowest capture to redo — it
                    // goes to disk before any surface can drop it
                    let dir = self.cfg.save_dir();
                    let (saved, msg) = match actions::save_to_dir(&img, &dir) {
                        Ok(p) => {
                            let prov = actions::provenance_now(qs_library::CaptureMode::Scrolling);
                            actions::record_saved(&dir, &p, prov);
                            self.rebuild_menu();
                            let msg = format!("Saved {}", file_label(&p));
                            (Some(p), msg)
                        }
                        Err(e) => {
                            #[cfg(target_os = "macos")]
                            {
                                self.toast_done(el, format!("Could not save scrolling capture: {e}"), false);
                                return;
                            }
                            #[cfg(not(target_os = "macos"))]
                            {
                                eprintln!("scroll save: {e:#}");
                                (None, "Captured — pick an action".to_string())
                            }
                        }
                    };
                    self.land_shot(
                        el,
                        Land {
                            img,
                            kind: ShotKind::Scroll,
                            path: saved,
                            sel: Some(sel),
                            instant: false,
                            dur_s: None,
                            shared: None,
                        },
                        &msg,
                    )
                }
                Err(e) => self.toast_done(el, format!("Scroll failed: {e}"), false),
            }
    }

    pub(super) fn on_done(&mut self, el: &ActiveEventLoop, res: Result<Dispatch, String>) {

        eprintln!("done: {:?}", res.as_ref().map(|d| &d.outcome));
        match res {
            // image outcomes fly to the album stack, except Copy —
            // nothing to act on later, so it confirms on the toast
            Ok(d) => match d.outcome {
                Outcome::Edited(p, doc) => {
                    // PNG lands like a Save; the editor opens the
                    // `.qsdoc` so annotations stay editable
                    self.card = None;
                    self.toast = None;
                    self.rebuild_menu();
                    actions::open_editor(&doc);
                    // the editor window is the confirmation on
                    // Linux — a "Saved" popup beside it is noise
                    #[cfg(target_os = "linux")]
                    let _ = (p, d.sel, d.img);
                    #[cfg(not(target_os = "linux"))]
                    {
                        let name = p
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        self.land_shot(
                            el,
                            Land {
                                img: d.img,
                                kind: ShotKind::Saved,
                                path: Some(p),
                                sel: Some(d.sel),
                                instant: false,
                                dur_s: None,
                                shared: None,
                            },
                            &format!("Saved {name}"),
                        );
                    }
                }
                Outcome::Saved(p) => self.land_saved(el, d.sel, d.img, p, false),
                // Done saved the file too, so it lands like a Save
                Outcome::Copied(Some(p)) => self.land_saved(el, d.sel, d.img, p, true),
                Outcome::Copied(None) => {
                    self.toast_done(el, "Copied to clipboard".to_string(), true);
                }
                Outcome::Shared(link, path) => {
                    let e = crate::share::entry(path.clone(), &link);
                    self.shares.put(e.clone());
                    self.shares.save();
                    #[cfg(not(target_os = "macos"))]
                    self.land_shot(
                        el,
                        Land {
                            img: d.img,
                            kind: ShotKind::Shared,
                            path: Some(path),
                            sel: Some(d.sel),
                            instant: false,
                            dur_s: None,
                            shared: Some(e),
                        },
                        "Shared",
                    );
                    self.toast_done(
                        el,
                        format!("Link copied — {}", link.note()),
                        true,
                    );
                }
                Outcome::TextCopied(n) => {
                    #[cfg(not(target_os = "macos"))]
                    self.land_shot(
                        el,
                        Land {
                            img: d.img,
                            kind: ShotKind::Ocr,
                            path: None,
                            sel: Some(d.sel),
                            instant: false,
                            dur_s: None,
                            shared: None,
                        },
                        &format!("Copied {n} chars"),
                    );
                    self.toast_done(el, format!("Copied {n} chars"), true);
                }
                Outcome::NoText => {
                    #[cfg(not(target_os = "macos"))]
                    self.land_shot(
                        el,
                        Land {
                            img: d.img,
                            kind: ShotKind::Ocr,
                            path: None,
                            sel: Some(d.sel),
                            instant: false,
                            dur_s: None,
                            shared: None,
                        },
                        "No text found",
                    );
                    self.toast_done(el, "No text found".to_string(), false);
                }
            },
            Err(e) => self.toast_done(el, format!("Failed: {e}"), false),
        }
            
    }

    pub(super) fn on_update_install(&mut self, el: &ActiveEventLoop, res: Result<PathBuf, String>) {
match res {
                Ok(app) => {
                    let _ = update::relaunch(&app);
                    el.exit();
                }
                Err(e) => self.toast_done(el, format!("Update failed: {e}"), false),
            }
    }

    pub(super) fn on_update_check(&mut self, el: &ActiveEventLoop, res: Result<Option<Release>, String>, manual: bool) {
match res {
                Ok(Some(rel)) => {
                    let v = rel.version.clone();
                    self.pending_update = Some(rel);
                    if let Some(s) = &mut self.settings {
                        s.set_update(Some(v.clone()));
                    }
                    self.rebuild_menu();
                    self.toast_done(el, format!("v{v} available — Update from the menu"), true);
                }
                Ok(None) => {
                    if let Some(s) = &mut self.settings {
                        s.set_update(None);
                    }
                    if manual {
                        self.toast_done(el, "You're up to date".to_string(), true);
                    }
                }
                Err(e) => {
                    if manual {
                        self.toast_done(el, format!("Update check failed: {e}"), false);
                    } else {
                        eprintln!("update check: {e}");
                    }
                }
            }
    }

    /// Hit GitHub Releases on a worker; `manual` shows a busy toast now and
    /// an "up to date" toast later.
    pub(super) fn check_updates(&mut self, el: &ActiveEventLoop, manual: bool) {
        // `el` is only used by the non-Linux toast path; keep the parameter
        // used on Linux so the build stays warning-free.
        #[cfg(target_os = "linux")]
        let _ = el;
        let before = self.cfg.clone();
        self.cfg.last_update_check = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.cfg.save_changes(&before);
        #[cfg(not(target_os = "linux"))]
        if manual && self.toast.is_none() {
            if let Ok(t) = Toast::open(
                el,
                "Checking for updates…",
                self.cfg.accent_rgb(),
                &self.cfg.popup_pos,
            ) {
                self.toast = Some(t);
            }
        }
        #[cfg(target_os = "linux")]
        if manual {
            crate::linux_notify::send("Checking for updates…", true);
        }
        let px = self.proxy.clone();
        std::thread::spawn(move || {
            let _ = px.send_event(HostEvent::UpdateCheck {
                res: update::check().map_err(|e| format!("{e:#}")),
                manual,
            });
        });
    }

    /// Download + swap the pending release on a worker.
    pub(super) fn install_update(&mut self, el: &ActiveEventLoop, rel: Release) {
        // `el` is only used by the non-Linux toast path.
        #[cfg(target_os = "linux")]
        let _ = el;
        #[cfg(not(target_os = "linux"))]
        if self.toast.is_none() {
            let msg = format!("Updating to v{}…", rel.version);
            if let Ok(t) = Toast::open(el, &msg, self.cfg.accent_rgb(), &self.cfg.popup_pos) {
                self.toast = Some(t);
            }
        }
        #[cfg(target_os = "linux")]
        crate::linux_notify::send(&format!("Updating to v{}…", rel.version), true);
        let px = self.proxy.clone();
        std::thread::spawn(move || {
            let _ = px.send_event(HostEvent::UpdateInstall(
                update::install(&rel).map_err(|e| format!("{e:#}")),
            ));
        });
    }

    /// Show the platform's capture feedback. macOS confirms with a toast.
    pub(super) fn land_shot(&mut self, el: &ActiveEventLoop, l: Land, msg: &str) {
        self.toast = None;
        #[cfg(target_os = "macos")]
        {
            let _ = l;
            self.toast_done(el, msg.to_owned(), true);
        }
        #[cfg(target_os = "linux")]
        {
            let _ = (l.sel, &l.img, l.dur_s, &l.shared);
            // an image on disk gets the thumbnail + Edit notification (the
            // Linux stand-in for the corner shelf); recordings and
            // unsaved results keep the plain confirmation
            if let Some(p) = l.path.clone().filter(|_| l.kind != ShotKind::Record) {
                eprintln!("linux: post-capture card ({msg})");
                self.toast = None;
                crate::linux_notify::send_shot(msg, p);
                return;
            }
            let ok = !msg.contains("No text found");
            eprintln!("linux: post-capture toast ({msg})");
            self.toast_done(el, msg.to_string(), ok);
            return;
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            if l.sel.is_none() {
                self.open_card(el, l.img, msg, l.path);
                return;
            }
            match self.chrome.land(el, l) {
                Ok(()) => {
                    eprintln!("chrome: shot queued in corner album");
                    let _ = self.chrome.pump();
                }
                Err(mut l) => {
                    eprintln!("chrome: fly-in missed — settling stack at corner");
                    l.instant = true;
                    match self.chrome.land(el, *l) {
                        Ok(()) => {
                            let _ = self.chrome.pump();
                        }
                        Err(l) => {
                            eprintln!("chrome: album unavailable ({msg})");
                            self.open_card(el, l.img, msg, l.path);
                        }
                    }
                }
            }
        }
    }

    /// A capture that is on disk — from Save, or Done (`copied`: already on
    /// the clipboard).
    pub(super) fn land_saved(
        &mut self,
        el: &ActiveEventLoop,
        sel: qs_core::Rect,
        img: image::RgbaImage,
        p: PathBuf,
        copied: bool,
    ) {
        self.card = None;
        self.toast = None;
        self.rebuild_menu();
        match self.cfg.after_capture() {
            AfterCapture::Editor => actions::open_editor(&p),
            AfterCapture::Copy => {
                if !copied && !self.cfg.copy_after_save {
                    actions::copy_image(&img, false);
                }
                self.toast_done(el, "Copied to clipboard".to_string(), true);
                return;
            }
            AfterCapture::Shelf => {}
        }
        let name = file_label(&p);
        let msg = if copied {
            format!("Copied · saved {name}")
        } else {
            format!("Saved {name}")
        };
        self.land_shot(
            el,
            Land {
                img,
                kind: ShotKind::Saved,
                path: Some(p),
                sel: Some(sel),
                instant: false,
                dur_s: None,
                shared: None,
            },
            &msg,
        );
    }

    /// Mirror a finished action into the toast (opened for OCR; lazily
    /// created for failures so errors are visible even for fast actions).
    pub(super) fn toast_done(&mut self, el: &ActiveEventLoop, msg: String, ok: bool) {
        #[cfg(target_os = "linux")]
        {
            let _ = el;
            self.toast = None;
            crate::linux_notify::send(&msg, ok);
            return;
        }
        #[cfg(not(target_os = "linux"))]
        match &mut self.toast {
            Some(t) => t.done(msg, ok),
            None => match Toast::open(el, &msg, self.cfg.accent_rgb(), &self.cfg.popup_pos) {
                Ok(mut t) => {
                    t.done(msg, ok);
                    self.toast = Some(t);
                }
                Err(e) => eprintln!("toast: {e:#}"),
            },
        }
    }

    /// Kill a live share link off-thread; `shot` (if any) gets its ↗
    /// chip cleared when the result lands. The ledger entry drops
    /// regardless of network success — a dead link is a dead link.
    pub(super) fn revoke(
        &mut self,
        _el: &ActiveEventLoop,
        e: crate::share::ShareEntry,
        shot: Option<(WindowId, usize)>,
    ) {
        let sm = self
            .share
            .get_or_insert_with(crate::share::ShareManager::new)
            .clone();
        let px = self.proxy.clone();
        std::thread::spawn(move || {
            let res = sm
                .revoke(&e.url, &e.delete_key)
                .map_err(|err| err.to_string());
            let _ = px.send_event(HostEvent::RevokeDone(res, e, shot));
        });
    }
}
