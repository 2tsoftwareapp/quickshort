//! Keyboard and shortcut dispatch for `EditorApp`.
//!
//! Split out of `app.rs` so the View layer stays layout-only; key routing
//! follows the same consume-then-act pattern as `filmstrip_keys.rs`:
//! gather what was pressed inside the input lock, apply after it.

use crate::{
    app::{spawn_app, EditorApp},
    clipboard,
    image_panel::{self, OrientCmd},
    toolbar::ZoomCmd,
};
use eframe::egui::{self, Key, Modifiers};
use qs_edit::{Motion, Tool};

impl EditorApp {
    /// Dialog chords and Escape. Split out of `handle_shortcuts` so tests can
    /// drive them without an eframe frame.
    pub fn handle_dialog_keys(&mut self, ctx: &egui::Context) {
        // `send_viewport_cmd` needs the context outside the input lock.
        let mut close_requested = false;
        let mut flatten_now = false;
        ctx.input_mut(|i| {
            // macOS close chord: same guard as the window's red button.
            if i.consume_key(Modifiers::COMMAND, Key::W) {
                close_requested = true;
            }
            // Escape closes whichever dialog or popup is on top.
            if i.modifiers.is_none() && i.key_pressed(Key::Escape) {
                if self.dialogs.context_menu.is_some() {
                    self.dialogs.context_menu = None;
                    i.consume_key(Modifiers::NONE, Key::Escape);
                } else if let Some(open) = [
                    &mut self.dialogs.shortcuts_open,
                    &mut self.dialogs.confirm_close,
                    &mut self.dialogs.app_menu_open,
                    &mut self.dialogs.export_menu_open,
                    &mut self.dialogs.canvas_dialog_open,
                    &mut self.dialogs.combine_dialog_open,
                    &mut self.dialogs.alignment_open,
                    &mut self.library.open,
                ]
                .into_iter()
                .find(|open| **open)
                {
                    *open = false;
                    i.consume_key(Modifiers::NONE, Key::Escape);
                } else if self.find.open {
                    self.find.open = false;
                    self.last_find_focus = None;
                    i.consume_key(Modifiers::NONE, Key::Escape);
                }
            }
            // Settings has the standard macOS shortcut; Align needs a panel that
            // used to be reachable only from the Layers header.
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::T)
                || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::T)
            {
                self.viewport.show_filmstrip = !self.viewport.show_filmstrip;
            }
            if i.consume_key(Modifiers::COMMAND, Key::Comma) {
                self.dialogs.canvas_dialog_open = false;
                self.dialogs.export_menu_open = false;
                self.dialogs.alignment_open = false;
                spawn_app("--settings");
            }
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::A)
                || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::A)
            {
                self.dialogs.canvas_dialog_open = false;
                self.dialogs.export_menu_open = false;
                self.dialogs.alignment_open = true;
                self.show_inspector = true;
            }
            // Dialog shortcuts: these panels were only reachable by mouse.
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::C)
                || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::C)
            {
                self.dialogs.export_menu_open = false;
                self.dialogs.canvas_dialog_open = true;
                self.show_inspector = true;
            }
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::E)
                || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::E)
            {
                self.dialogs.canvas_dialog_open = false;
                self.dialogs.export_menu_open = true;
            }
            // cmd+shift+F never reached us (macOS swallows it), so the
            // destructive flatten confirmation uses option+cmd+F.
            if i.consume_key(Modifiers::COMMAND | Modifiers::ALT, Key::F)
                || i.consume_key(Modifiers::CTRL | Modifiers::ALT, Key::F)
            {
                self.dialogs.canvas_dialog_open = false;
                self.dialogs.export_menu_open = false;
                flatten_now = true;
            }
        });
        if flatten_now {
            if let Err(e) = self.flatten_all() {
                self.show_toast(format!("Flatten failed: {e}"));
            }
        }
        if close_requested {
            self.request_close(ctx);
        }
    }


    pub(crate) fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let mut undo = false;
        let mut redo = false;
        let mut copy = false;
        let mut save = false;
        let mut save_as = false;
        let mut quick_export = false;
        let mut open = false;
        let mut del = false;
        let mut dup = false;
        let mut select_all = false;
        let mut esc = false;
        let mut tool = None;
        let mut nudge = None;
        let mut orient = None;
        let mut zoom_cmd = ZoomCmd::None;
        let mut width_delta: f32 = 0.0;
        let mut group = false;
        let mut ungroup = false;

        // An open overlay owns the navigation keys: otherwise the canvas nudge
        // and the Escape-to-deselect path swallow them before the dialog sees
        // them, which made the Library keyboard route dead on arrival.
        let overlay_keys = self.library.open || self.find.open || self.dialogs.shortcuts_open;
        // A focused text field owns plain keystrokes AND standard text-editing
        // chords (Backspace, Delete, Ctrl+C, Ctrl+Z, Ctrl+A) so typing in Find,
        // Watermark, Zoom %, or Sliders never mutates the canvas underneath.
        let typing = ctx.memory(|m| m.focused().is_some());
        ctx.input_mut(|i| {
            if self.find.open && i.consume_key(Modifiers::NONE, Key::Escape) {
                self.find.open = false;
            }
            // While inline-editing a Text/Callout on canvas: still allow Save
            // (Ctrl+S), Undo/Redo (commits edit first), and Ctrl+Enter to finish.
            if self.doc.editor.edit().is_some() {
                if i.consume_key(Modifiers::COMMAND, Key::S) || i.consume_key(Modifiers::CTRL, Key::S)
                {
                    save = true;
                }
                if i.consume_key(Modifiers::COMMAND, Key::Enter)
                    || i.consume_key(Modifiers::CTRL, Key::Enter)
                {
                    self.doc.editor.end_text_edit();
                    self.touch();
                } else if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                    || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Z)
                    || i.consume_key(Modifiers::COMMAND, Key::Y)
                    || i.consume_key(Modifiers::CTRL, Key::Y)
                {
                    self.doc.editor.end_text_edit();
                    redo = true;
                } else if i.consume_key(Modifiers::COMMAND, Key::Z)
                    || i.consume_key(Modifiers::CTRL, Key::Z)
                {
                    self.doc.editor.end_text_edit();
                    undo = true;
                }
                return;
            }

            if !typing {
                if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                    || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Z)
                    || i.consume_key(Modifiers::COMMAND, Key::Y)
                    || i.consume_key(Modifiers::CTRL, Key::Y)
                {
                    redo = true;
                } else if i.consume_key(Modifiers::COMMAND, Key::Z)
                    || i.consume_key(Modifiers::CTRL, Key::Z)
                {
                    undo = true;
                }

                if i.consume_key(Modifiers::COMMAND, Key::C) || i.consume_key(Modifiers::CTRL, Key::C)
                {
                    copy = true;
                }
                if i.consume_key(Modifiers::COMMAND, Key::A) || i.consume_key(Modifiers::CTRL, Key::A)
                {
                    select_all = true;
                }
            }

            if i.consume_key(Modifiers::COMMAND, Key::E) || i.consume_key(Modifiers::CTRL, Key::E) {
                quick_export = true;
            }

            if i.consume_key(Modifiers::COMMAND, Key::L) || i.consume_key(Modifiers::CTRL, Key::L) {
                // One overlay at a time: two stacked dialogs hide each other's
                // content and both keep stealing the keyboard.
                self.find.open = false;
                self.library.toggle();
            }

            if i.consume_key(Modifiers::COMMAND, Key::N) || i.consume_key(Modifiers::CTRL, Key::N) {
                spawn_app("--capture");
            }

            if i.consume_key(Modifiers::COMMAND, Key::F) || i.consume_key(Modifiers::CTRL, Key::F) {
                self.library.open = false;
                self.find.open = true;
                self.find.focus_pending = true;
            }

            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)
                || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::S)
            {
                save_as = true;
            } else if i.consume_key(Modifiers::COMMAND, Key::S)
                || i.consume_key(Modifiers::CTRL, Key::S)
            {
                save = true;
            }

            if i.consume_key(Modifiers::COMMAND, Key::O) || i.consume_key(Modifiers::CTRL, Key::O) {
                open = true;
            }

            // Shift first: consume_key ignores an unrequested Shift
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::R)
                || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::R)
            {
                orient = Some(OrientCmd::RotateCcw);
            } else if i.consume_key(Modifiers::COMMAND, Key::R)
                || i.consume_key(Modifiers::CTRL, Key::R)
            {
                orient = Some(OrientCmd::RotateCw);
            }

            if i.consume_key(Modifiers::COMMAND, Key::Plus)
                || i.consume_key(Modifiers::CTRL, Key::Plus)
                || i.consume_key(Modifiers::COMMAND, Key::Equals)
                || i.consume_key(Modifiers::CTRL, Key::Equals)
            {
                zoom_cmd = ZoomCmd::In;
            } else if i.consume_key(Modifiers::COMMAND, Key::Minus)
                || i.consume_key(Modifiers::CTRL, Key::Minus)
            {
                zoom_cmd = ZoomCmd::Out;
            } else if i.consume_key(Modifiers::COMMAND, Key::Num0)
                || i.consume_key(Modifiers::CTRL, Key::Num0)
            {
                zoom_cmd = ZoomCmd::Fit;
            } else if i.consume_key(Modifiers::COMMAND, Key::Num1)
                || i.consume_key(Modifiers::CTRL, Key::Num1)
            {
                zoom_cmd = ZoomCmd::Set(1.0);
            }

            if !typing
                && !overlay_keys
                && (i.consume_key(Modifiers::NONE, Key::Delete)
                    || i.consume_key(Modifiers::NONE, Key::Backspace))
            {
                del = true;
            }

            if !typing
                && (i.consume_key(Modifiers::COMMAND, Key::D)
                    || i.consume_key(Modifiers::CTRL, Key::D))
            {
                dup = true;
            }

            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::G)
                || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::G)
            {
                ungroup = true;
            } else if i.consume_key(Modifiers::COMMAND, Key::G)
                || i.consume_key(Modifiers::CTRL, Key::G)
            {
                group = true;
            }

            if !overlay_keys && !typing && i.consume_key(Modifiers::NONE, Key::Escape) {
                esc = true;
            }

            if !typing && !overlay_keys {
                if i.consume_key(Modifiers::NONE, Key::OpenBracket) {
                    width_delta = -1.0;
                } else if i.consume_key(Modifiers::NONE, Key::CloseBracket) {
                    width_delta = 1.0;
                } else if i.consume_key(Modifiers::SHIFT, Key::Questionmark)
                    || i.consume_key(Modifiers::NONE, Key::Questionmark)
                    || i.consume_key(Modifiers::NONE, Key::F1)
                {
                    self.dialogs.shortcuts_open = !self.dialogs.shortcuts_open;
                }
            }

            if !typing && i.modifiers.is_none() {
                // `Tool::key()` is the single source of truth — the palette
                // subscript reads the same letter this dispatches on.
                let pressed_tool = Tool::ALL.iter().copied().find(|t| {
                    egui::Key::from_name(&t.key().to_ascii_uppercase().to_string())
                        .is_some_and(|k| i.key_pressed(k))
                });
                if let Some(t) = pressed_tool {
                    tool = Some(t);
                }
            }

            let nudge_step = if i.modifiers.shift { 10.0 } else { 1.0 };
            if !overlay_keys && !typing && !self.viewport.show_filmstrip && i.consume_key(Modifiers::NONE, Key::ArrowLeft)
                || i.consume_key(Modifiers::SHIFT, Key::ArrowLeft)
            {
                nudge = Some((-nudge_step, 0.0));
            }
            if !overlay_keys && !typing && !self.viewport.show_filmstrip && i.consume_key(Modifiers::NONE, Key::ArrowRight)
                || i.consume_key(Modifiers::SHIFT, Key::ArrowRight)
            {
                nudge = Some((nudge_step, 0.0));
            }
            if !overlay_keys && !typing && !self.viewport.show_filmstrip && i.consume_key(Modifiers::NONE, Key::ArrowUp)
                || i.consume_key(Modifiers::SHIFT, Key::ArrowUp)
            {
                nudge = Some((0.0, -nudge_step));
            }
            if !overlay_keys && !typing && !self.viewport.show_filmstrip && i.consume_key(Modifiers::NONE, Key::ArrowDown)
                || i.consume_key(Modifiers::SHIFT, Key::ArrowDown)
            {
                nudge = Some((0.0, nudge_step));
            }
        });

        if undo {
            if self.doc.editor.undo() {
                self.touch();
                self.show_toast("Undo");
            } else if let Some((doc, base)) = self.doc.pre_flatten.take() {
                self.apply_document(doc, base);
                self.doc.editor.mark_modified();
                self.touch();
                self.show_toast("Flatten undone");
            }
        }
        if redo
            && self.doc.editor.redo() {
                self.touch();
                self.show_toast("Redo");
            }
        if copy {
            let _ = self.copy_to_clipboard();
        }
        if select_all {
            let n = self.doc.editor.select_all();
            if n > 0 {
                self.show_toast(format!("Selected {n} object(s)"));
            }
        }
        if quick_export {
            if let Err(e) = self.export_png(None) {
                self.show_toast(format!("Export failed: {e}"));
            }
        }
        if width_delta != 0.0 {
            let next = (self.doc.editor.style.width + width_delta).clamp(1.0, 24.0);
            self.doc.editor.set_stroke_width(next);
            self.touch();
        }
        if save {
            let _ = self.save();
        }
        if save_as {
            let _ = self.save_as();
        }
        if open {
            let _ = self.open_dialog();
        }
        if zoom_cmd != ZoomCmd::None {
            self.apply_zoom_cmd(zoom_cmd);
        }
        if del && self.doc.editor.delete_selected() {
            self.show_toast("Deleted");
            self.touch();
        }
        if dup && self.doc.editor.duplicate_selected().is_some() {
            self.touch();
        }
        if group && self.doc.editor.group_selected() {
            self.touch();
        }
        if ungroup && self.doc.editor.ungroup_selected() {
            self.touch();
        }
        if esc {
            self.doc.editor.escape();
        }
        if let Some(t) = tool {
            self.doc.editor.set_tool(t);
        }
        if let Some(cmd) = orient {
            image_panel::apply_orient(&mut self.doc.editor, cmd);
            self.touch();
        }
        if let Some((dx, dy)) = nudge {
            // arrows move what the user sees, whatever the orientation
            let (dx, dy) = self.doc.editor.doc().orientation.unmap_vector(dx, dy);
            self.doc.editor.nudge(dx, dy);
            self.touch();
        }
    }


    pub(crate) fn handle_text_events(&mut self, ctx: &egui::Context) {
        if self.doc.editor.edit().is_none() {
            self.tools.ime_preedit.clear();
            return;
        }
        let events = ctx.input(|i| i.raw.events.clone());
        for ev in events {
            match ev {
                egui::Event::Text(s) => {
                    self.doc.editor.text_insert(&s);
                    self.touch();
                }
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    let extend = modifiers.shift;
                    match key {
                        Key::Enter => {
                            self.doc.editor.text_insert("\n");
                            self.touch();
                        }
                        Key::Backspace => {
                            self.doc.editor.text_backspace();
                            self.touch();
                        }
                        Key::Delete => {
                            self.doc.editor.text_delete();
                            self.touch();
                        }
                        Key::ArrowLeft => self.doc.editor.text_move(Motion::Left, extend),
                        Key::ArrowRight => self.doc.editor.text_move(Motion::Right, extend),
                        Key::ArrowUp => self.doc.editor.text_move(Motion::Up, extend),
                        Key::ArrowDown => self.doc.editor.text_move(Motion::Down, extend),
                        Key::Home => self.doc.editor.text_move(Motion::Home, extend),
                        Key::End => self.doc.editor.text_move(Motion::End, extend),
                        Key::Escape => {
                            self.doc.editor.escape();
                            self.touch();
                        }
                        Key::A if modifiers.command || modifiers.ctrl => {
                            self.doc.editor.text_select_all();
                        }
                        Key::C if modifiers.command || modifiers.ctrl => {
                            if let Some(sel) = self.doc.editor.text_selection() {
                                let _ = clipboard::copy_text(&sel);
                            }
                        }
                        _ => {}
                    }
                }
                egui::Event::Paste(s) => {
                    self.doc.editor.text_insert(&s);
                    self.touch();
                }
                // IME-composed text (Vietnamese Telex/VNI, dead keys): Preedit is
                // the in-flight composition shown beside the caret; Commit is
                // the finished text. Without these only raw ASCII gets through.
                egui::Event::Ime(ime) => match ime {
                    egui::ImeEvent::Preedit { text, .. } => {
                        self.tools.ime_preedit = text;
                    }
                    egui::ImeEvent::Commit(text) => {
                        self.tools.ime_preedit.clear();
                        self.doc.editor.text_insert(&text);
                        self.touch();
                    }
                    egui::ImeEvent::DeleteSurrounding { before_chars, .. } => {
                        for _ in 0..before_chars {
                            self.doc.editor.text_backspace();
                        }
                        self.touch();
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

}
