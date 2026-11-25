//! Input — pointer and key handling, context-menu bookkeeping.

use super::*;

impl Stack {

    /// Open the thumb menu at the click point. Row ids are the MENU_*
    /// consts; the host answers `MenuAction` events.
    pub(super) fn open_menu(&mut self, idx: usize, x: f64, y: f64) {
        let has_path = self.shot(idx).and_then(|s| s.path.as_ref()).is_some();
        let revocable = self
            .shot(idx)
            .and_then(|s| s.shared.as_ref())
            .is_some_and(|e| !e.delete_key.is_empty());
        self.menu = Some(Menu {
            idx,
            anchor: FRect {
                x,
                y,
                w: 0.0,
                h: 0.0,
            },
            rows: vec![
                (MENU_REVEAL, has_path),
                (MENU_SAVE, !has_path),
                (MENU_COPY, true),
                (MENU_PIN, true),
                (MENU_BEAUTIFY, true),
                (MENU_SHARE, !revocable),
                (MENU_REVOKE, revocable),
                (MENU_DEL, true),
                (MENU_CLEAR, true),
            ],
        });
    }


    /// Menu label for a row id — rendered at draw time.
    pub(super) fn menu_label(id: &'static str) -> &'static str {
        match id {
            MENU_REVEAL => {
                if cfg!(target_os = "macos") {
                    "Reveal in Finder"
                } else if cfg!(target_os = "windows") {
                    "Reveal in Explorer"
                } else {
                    "Reveal in Folder"
                }
            }
            MENU_SAVE => "Save to Screenshots",
            MENU_OPEN => {
                if cfg!(target_os = "macos") {
                    "Open in Preview"
                } else {
                    "Open in Editor"
                }
            }
            MENU_COPY => "Copy Image",
            MENU_PIN => "Pin",
            MENU_BEAUTIFY => "Beautify",
            MENU_SHARE => "Share Link",
            MENU_REVOKE => "Revoke Link",
            MENU_DEL => "Remove from Shelf",
            MENU_CLEAR => "Clear Shelf",
            _ => "?",
        }
    }


    /// Platform events distilled into state transitions; the returned
    /// command is for the host (menu, remove).
    pub fn pointer(&mut self, ev: ChromeEv, w: f64, h: f64) -> Option<ChromeCmd> {
        match ev {
            ChromeEv::Move { x, y } => {
                let hit = self.hit(x, y, w, h);
                let inside = hit.is_some() || self.zone_hit(x, y, w, h, self.inside);
                if hit != self.hover || inside != self.inside {
                    // real hover transitions are activity — synthetic
                    // Move re-emits (sync_input, global monitor) aren't
                    self.touch();
                    self.hover = hit;
                    self.inside = inside;
                    self.retarget_fan();
                    // hover drives the keyboard ring (macshot parity)
                    self.ring = match hit {
                        Some(Hit::Thumb(i)) | Some(Hit::Del(i)) => Some(i),
                        _ => self.ring.filter(|_| hit.is_some()),
                    };
                    return Some(ChromeCmd::Redraw);
                }
                None
            }
            ChromeEv::Leave => {
                self.inside = false;
                if self.hover.take().is_some() {
                    self.retarget_fan();
                    return Some(ChromeCmd::Redraw);
                }
                self.retarget_fan();
                None
            }
            ChromeEv::Down { x, y } => {
                self.touch();
                match self.hit(x, y, w, h) {
                    Some(Hit::Del(i)) => Some(ChromeCmd::Remove(i)),
                    Some(Hit::ClearAll) => Some(ChromeCmd::ClearAll),
                    Some(Hit::Chip) => {
                        self.pinned = !self.pinned;
                        self.retarget_fan();
                        Some(ChromeCmd::Redraw)
                    }
                    Some(Hit::Thumb(i)) => {
                        // click resolves on release — the same press may
                        // become a drag-out instead
                        self.press = Some((x, y, i));
                        Some(ChromeCmd::Redraw)
                    }
                    Some(Hit::MenuRow(i)) => {
                        let m = self.menu.take().unwrap();
                        let (id, enabled) = m.rows[i];
                        if enabled {
                            Some(ChromeCmd::MenuAction { idx: m.idx, id })
                        } else {
                            // disabled row: keep the menu open, like a native one
                            self.menu = Some(m);
                            Some(ChromeCmd::Redraw)
                        }
                    }
                    Some(Hit::MenuPanel) => None, // swallowed, menu stays
                    Some(Hit::BoxAction(i)) => self.lightbox.and_then(|idx| {
                        BOX_ACTIONS
                            .get(i)
                            .map(|&id| ChromeCmd::MenuAction { idx, id })
                    }),
                    Some(Hit::Backdrop) => {
                        self.lightbox = None;
                        self.menu = None;
                        Some(ChromeCmd::Redraw)
                    }
                    None => None,
                }
            }
            ChromeEv::RightDown { x, y } => {
                self.touch();
                match self.hit(x, y, w, h) {
                    Some(Hit::Thumb(i)) | Some(Hit::Del(i)) => {
                        self.open_menu(i, x, y);
                        Some(ChromeCmd::Redraw)
                    }
                    // a second right-click on another thumb retargets the
                    // menu; anywhere else it dismisses — matching native
                    // context-menu behaviour. hit() runs with the menu
                    // taken so thumbs are visible to it again.
                    _ => {
                        if self.menu.take().is_some() {
                            if let Some(Hit::Thumb(i) | Hit::Del(i)) = self.hit(x, y, w, h) {
                                self.open_menu(i, x, y);
                            }
                            Some(ChromeCmd::Redraw)
                        } else {
                            None
                        }
                    }
                }
            }
            ChromeEv::Dragged { x, y } => {
                self.touch();
                match self.press.take() {
                    Some((px, py, i)) if (x - px).hypot(y - py) >= 4.0 => {
                        // past the native drag threshold — the backend
                        // turns this into a real file-promise drag
                        self.press = None;
                        Some(ChromeCmd::DragBegin { idx: i })
                    }
                    Some((px, py, i)) => {
                        self.press = Some((px, py, i));
                        None
                    }
                    None => None,
                }
            }
            ChromeEv::Up { x, y } => {
                self.touch();
                match self.press.take() {
                    // released on the same thumb → the deferred click opens
                    // the editor straight away (CleanShot/Snagit); the
                    // lightbox preview stays on Space/Enter
                    Some((_, _, i)) if matches!(self.hit(x, y, w, h), Some(Hit::Thumb(j)) if j == i) => {
                        Some(ChromeCmd::MenuAction {
                            idx: i,
                            id: MENU_OPEN,
                        })
                    }
                    // released elsewhere — just a cancelled press
                    Some(_) => None,
                    None => None,
                }
            }
            ChromeEv::Key(_) => None, // keys go through `key()`
            ChromeEv::Tick => None,   // backend-driven — never enters pointer logic
        }
    }


    /// Keyboard input while the chrome window is key — arrows move the
    /// ring, Enter previews, O/S open and share a preview, ⌫ removes,
    /// Esc peels a layer off.
    pub fn key(&mut self, k: Key) -> Option<ChromeCmd> {
        self.touch();
        let n = self
            .shots
            .len()
            .min(if self.pinned { MAX_VIS_PINNED } else { MAX_VIS });
        if k == Key::Esc && self.menu.take().is_some() {
            return Some(ChromeCmd::Redraw);
        }
        if self.menu.is_some() {
            return None; // menu is modal while open
        }
        match (k, self.lightbox) {
            (Key::Esc, Some(_)) => {
                self.lightbox = None;
                Some(ChromeCmd::Redraw)
            }
            (Key::Esc, None) if self.pinned => {
                self.pinned = false;
                self.retarget_fan();
                Some(ChromeCmd::Redraw)
            }
            (Key::Esc, None) => None,
            (Key::Left, None) | (Key::Right, None) if n > 0 => {
                let d = if matches!(k, Key::Left) { 1 } else { -1 };
                let cur = self.ring.unwrap_or(if d > 0 { n } else { 0 });
                self.ring = Some((cur as i64 + d).clamp(0, n as i64 - 1) as usize);
                Some(ChromeCmd::Redraw)
            }
            (Key::Enter, None) | (Key::Space, None) => match self.key_target(n) {
                Some(i) => {
                    self.open_lightbox(i);
                    Some(ChromeCmd::Redraw)
                }
                None => None,
            },
            // Enter inside a lightbox copies the previewed shot — the
            // macshot mapping the r2 spec steals
            (Key::Enter, Some(_)) => self.key_target(n).map(|i| ChromeCmd::MenuAction {
                idx: i,
                id: MENU_COPY,
            }),
            // the lightbox legend's O / S — the same actions the bar
            // buttons expose, without a mouse
            (Key::Open, Some(_)) => self.key_target(n).map(|i| ChromeCmd::MenuAction {
                idx: i,
                id: MENU_OPEN,
            }),
            (Key::Share, Some(_)) => self.key_target(n).map(|i| ChromeCmd::MenuAction {
                idx: i,
                id: MENU_SHARE,
            }),
            (Key::Copy, _) => self.key_target(n).map(|i| ChromeCmd::MenuAction {
                idx: i,
                id: MENU_COPY,
            }),
            (Key::Back, _) => self.key_target(n).map(ChromeCmd::Remove),
            _ => None,
        }
    }


    /// The shot a keypress applies to: the ring selection first, then
    /// a hovered thumb, falling back to the newest — matches the
    /// hover-and-arrows-share-one-ring contract.
    pub(super) fn key_target(&self, n: usize) -> Option<usize> {
        // an open lightbox owns the action — ⌘C/⌫ apply to the shot
        // being previewed, not whatever sits under the ring
        self.lightbox
            .filter(|i| *i < n)
            .or(self.ring.filter(|i| *i < n))
            .or_else(|| {
                self.hover.and_then(|h| match h {
                    Hit::Thumb(i) | Hit::Del(i) => Some(i),
                    _ => None,
                })
            })
            .or(if n > 0 { Some(0) } else { None })
    }
}
