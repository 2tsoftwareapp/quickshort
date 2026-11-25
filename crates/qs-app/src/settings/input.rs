//! Input half of the settings window — keyboard/mouse dispatch,
//! click targets, focus ring traversal and dropdown stepping.
//! Methods on `Settings` (child module sees the parent's fields).

use super::*;

impl Settings {
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
                    *st2.lock().unwrap() = ocr::install_packs(&pkgs);
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
                    qs_theme::set_accent(cfg.accent_rgb());
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
            Some(Hit::Close) => {
                self.closed = true;
                return Some(SettingsEvent::Closed);
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
}
