//! Global-hotkey capture intents — the Trigger/TriggerFull/Hotkey
//! arm of the user_event match plus the machinery they kick off.

use super::*;

impl Host {
    /// Hotkey flow — tray "Capture", region/fullscreen chords and
    /// Wayland portal senders all land here; the recorder suspension
    /// gates them the same way the unregistered X11 grab does.
    pub(super) fn on_capture_trigger(&mut self, el: &ActiveEventLoop, full: bool) {
        if self.hotkey_suspended {
            return;
        }
        if full {
            self.trigger_full(el);
        } else {
            self.trigger(el);
        }
    }

    pub(super) fn on_hotkey(&mut self, el: &ActiveEventLoop, id: u32) {

        // the manager's event source carries only the HotKey id —
        // map it back to the registered chord to pick the path
        if self.hotkey_full.is_some_and(|h| h.id() == id) {
            if !self.hotkey_suspended {
                self.trigger_full(el);
            }
        } else if self.hotkey.is_some_and(|h| h.id() == id) && !self.hotkey_suspended {
            self.trigger(el);
        }
            
    }

    /// Capture every monitor — shared by the region overlay and the
    /// fullscreen path. Failures toast once; on macOS a denied Screen
    /// Recording grant also opens its settings pane.
    pub(super) fn capture_frames(&mut self, el: &ActiveEventLoop) -> Option<Vec<qs_core::CapturedFrame>> {
        match qs_capture::backend().and_then(|b| b.capture_all()) {
            Ok(f) if !f.is_empty() => Some(f),
            Ok(_) => {
                log::warn!("no monitors captured");
                None
            }
            Err(e) => {
                log::warn!("capture failed: {e:#}");
                self.toast_done(el, format!("Capture failed: {e:#}"), false);
                // LSUIElement apps get no TCC prompt — open the pane so the
                // user can grant Screen Recording manually
                if cfg!(target_os = "macos") {
                    let _ = std::process::Command::new("open")
                        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
                        .spawn();
                }
                None
            }
        }
    }

    /// Capture every monitor and spawn the overlay session.
    pub(super) fn trigger(&mut self, el: &ActiveEventLoop) {
        self.refresh_appearance();
        if self.overlay.is_some() {
            // second trigger while a selection is up = reset: drop the
            // stale layer and start a fresh capture rather than
            // ignoring the press or stacking another overlay
            self.overlay = None;
            self.frames = None;
        }
        self.card = None; // a stale card owns the previous capture's image
        self.clear_own_popups();
        let Some(frames) = self.capture_frames(el) else {
            return;
        };
        let bounds = qs_core::virtual_bounds(&frames);
        let prefs = self.cfg.overlay_prefs();
        match Overlay::spawn(el, &frames, bounds, &prefs) {
            Ok(ov) => {
                self.frames = Some(frames);
                self.overlay = Some(ov);
            }
            Err(e) => log::warn!("overlay failed: {e:#}"),
        }
    }

    /// Fullscreen hotkey: stitch the whole virtual screen and save it —
    /// no overlay, no action picker. The PNG lands in the album like a
    /// region Save so Copy/Share/Pin stay one right-click away.
    pub(super) fn trigger_full(&mut self, el: &ActiveEventLoop) {
        self.refresh_appearance();
        if self.overlay.is_some() {
            // same reset as `trigger` — the fullscreen chord honors the
            // press instead of dead-ending behind the region overlay
            self.overlay = None;
            self.frames = None;
        }
        self.card = None;
        self.clear_own_popups();
        let Some(frames) = self.capture_frames(el) else {
            return;
        };
        let sel = qs_core::virtual_bounds(&frames);
        let px = self.proxy.clone();
        let target = self.save_target(qs_overlay::Action::Save);
        let copy = self.cfg.copy_after_save;
        let lang = self.cfg.ocr_lang.clone();
        let prov = actions::provenance_now(qs_library::CaptureMode::Fullscreen);
        std::thread::spawn(move || {
            let res = actions::dispatch(actions::DispatchArgs {
                frames,
                sel,
                action: qs_overlay::Action::Save,
                annots: &[],
                target: &target,
                wait_clipboard: false,
                share: None,
                copy_after_save: copy,
                ocr_lang: &lang,
                provenance: prov,
            })
            .map_err(|e| e.to_string());
            let _ = px.send_event(HostEvent::Done(res));
        });
    }

    /// While the settings recorder is armed, drop the global grabs so the
    /// chords reach the settings window instead of triggering capture.
    pub(super) fn suspend_hotkey(&mut self, suspended: bool) {
        if suspended == self.hotkey_suspended {
            return;
        }
        if let Some(m) = &self.hotkeys {
            for hk in [self.hotkey, self.hotkey_full].into_iter().flatten() {
                let r = if suspended {
                    m.unregister(hk)
                } else {
                    m.register(hk)
                };
                if let Err(e) = r {
                    // keep going — the other slot must not stay grabbed
                    log::warn!("hotkey suspend: {e:#}");
                }
            }
        }
        // compositor/portal binding needs the same treatment — otherwise
        // the grabbed chord never reaches the settings recorder
        #[cfg(target_os = "linux")]
        if let Some(g) = &mut self.hotkey_guard {
            g.suspend(suspended);
        }
        self.hotkey_suspended = suspended;
    }

    /// (Re)register one global chord into `hotkey`/`hotkey_full`. Compares
    /// chords by key+mods — `HotKey`'s PartialEq includes its per-grab id,
    /// so `==` would re-register on every settings change. An empty chord
    /// disables the slot; a chord equal to the other slot is refused
    /// (both grabs would fire on the same press).
    pub(super) fn rebind_slot(&mut self, el: &ActiveEventLoop, chord: &str, full: bool) {
        let label = if full { "Fullscreen hotkey" } else { "Hotkey" };
        // toast text decided inside the borrow scope — `toast_done` needs
        // &mut self, which can't overlap the manager/slot borrows
        let mut msg: Option<(String, bool)> = None;
        {
            let Some(m) = &self.hotkeys else { return };
            // read the sibling slot before taking &mut on ours
            let other = if full { self.hotkey } else { self.hotkey_full };
            let slot = if full {
                &mut self.hotkey_full
            } else {
                &mut self.hotkey
            };
            let chord = chord.trim();
            if chord.is_empty() {
                if let Some(old) = slot.take() {
                    let _ = m.unregister(old);
                }
                return;
            }
            let hk: HotKey = match chord.parse() {
                Ok(h) => h,
                Err(e) => {
                    log::warn!("bad {} {chord:?}: {e}", label.to_lowercase());
                    return;
                }
            };
            if other.is_some_and(|o| o.key == hk.key && o.mods == hk.mods) {
                let which = if full { "region" } else { "fullscreen" };
                msg = Some((
                    format!("{label} {chord} duplicates the {which} hotkey — ignored"),
                    false,
                ));
            } else if !slot.is_some_and(|old| old.key == hk.key && old.mods == hk.mods) {
                if let Some(old) = *slot {
                    let _ = m.unregister(old);
                }
                match m.register(hk) {
                    Ok(()) => {
                        *slot = Some(hk);
                        msg = Some((format!("{label} set to {chord}"), true));
                    }
                    Err(e) => {
                        log::warn!("{label} register failed: {e:#}");
                        // restore the old grab — a dead hotkey is worse
                        // than a stale one, and the slot stays truthful
                        if let Some(old) = *slot {
                            let _ = m.register(old);
                        }
                        msg = Some((
                            format!("{label} {chord} is already in use — kept the old one"),
                            false,
                        ));
                    }
                }
            }
        }
        if let Some((s, ok)) = msg {
            self.toast_done(el, s, ok);
        }
    }
}
