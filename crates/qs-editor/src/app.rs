//! Main standalone editor application implementing `eframe::App`.

use crate::{
    canvas::{self, CanvasView},
    clipboard, fonts,
    icons::{self, ActionIcon},
    image_panel::{self, OrientCmd},
    recovery,
    theme::{self, accent_blue, border_subtle, success_green, text_primary},
    toolbar::{self, ZoomCmd},
};
use eframe::egui::{
    self, Color32, ColorImage, CursorIcon, FontId, Key, Modifiers, Pos2, Rect, Stroke, StrokeKind,
    TextureHandle, TextureOptions, Vec2,
};
use image::RgbaImage;
use qs_doc::{qsdoc, Document, PointF, RectF};
use qs_edit::{Editor, Motion, Tool};
use std::path::{Path, PathBuf};

/// What the user picked in the "unsaved changes" dialog.
enum CloseDecision {
    Save,
    Discard,
    Cancel,
}

pub struct EditorApp {
    editor: Editor,
    base: RgbaImage,
    doc_path: Option<PathBuf>,
    view: CanvasView,
    texture: Option<TextureHandle>,
    /// The rendered texture is stale and must be rebuilt this frame.
    needs_render: bool,
    /// The document holds edits that are not on disk yet.
    unsaved: bool,
    /// Editor revision at the last save/load; `unsaved` is derived from it.
    saved_rev: u64,
    /// Closing was already confirmed, so the next close request goes through.
    close_confirmed: bool,
    /// The unsaved-changes dialog is on screen.
    confirm_close: bool,
    /// Edits left behind by a crashed session, waiting for the user to accept
    /// or reject them.
    pending_recovery: Option<(Document, RgbaImage)>,
    /// When the recovery copy was last written.
    autosave_at: Option<std::time::Instant>,
    /// Title currently pushed to the viewport, to avoid re-sending it.
    window_title: Option<String>,
    needs_initial_fit: bool,
    last_canvas_rect: Option<Rect>,
    saw_drag_press: bool,
    toast: Option<(String, std::time::Instant)>,
    canvas_dialog_open: bool,
    canvas_insets: [f32; 4],
    canvas_background: u32,
    canvas_anchor: (usize, usize),
}

fn anchored_insets(vertical: f32, horizontal: f32, anchor: (usize, usize)) -> [f32; 4] {
    let distribute = |total: f32, position: usize| match position {
        0 => (0.0, total),
        1 => (total * 0.5, total - total * 0.5),
        _ => (total, 0.0),
    };
    let (top, bottom) = distribute(vertical, anchor.1);
    let (left, right) = distribute(horizontal, anchor.0);
    [top, right, bottom, left]
}

impl EditorApp {
    pub fn new(
        doc: Document,
        base: RgbaImage,
        doc_path: Option<PathBuf>,
        cc: &eframe::CreationContext<'_>,
    ) -> Self {
        fonts::install(&cc.egui_ctx);
        theme::apply_macos_visuals(&cc.egui_ctx);

        // A previous session may have died with edits that never reached the
        // file; hold them until the first frame can ask what to do.
        let pending = Self::pending_recovery(doc_path.as_deref());

        let canvas_background = doc.background;
        let mut view = CanvasView::default();
        let base_rect = RectF::new(0.0, 0.0, doc.base_size.w, doc.base_size.h);
        view.sync(&doc);
        view.center = view.map.to_display(base_rect.center());
        let initial_zoom = (700.0 / doc.base_size.w.max(1.0))
            .min(500.0 / doc.base_size.h.max(1.0))
            .clamp(0.2, 1.0);
        view.zoom = initial_zoom;

        let mut app = Self {
            editor: Editor::new(doc),
            base,
            doc_path,
            view,
            texture: None,
            needs_render: true,
            unsaved: false,
            saved_rev: 0,
            close_confirmed: false,
            confirm_close: false,
            pending_recovery: pending,
            autosave_at: None,
            window_title: None,
            needs_initial_fit: true,
            last_canvas_rect: None,
            saw_drag_press: false,
            toast: None,
            canvas_dialog_open: false,
            canvas_insets: [0.0; 4],
            canvas_background,
            canvas_anchor: (1, 1),
        };
        app.update_texture(&cc.egui_ctx);
        app
    }

    /// Loads the recovery copy of `doc_path` when it is newer than the file on
    /// disk. The copy itself stays put until the user answers, so a crash
    /// inside the prompt loses nothing.
    fn pending_recovery(doc_path: Option<&Path>) -> Option<(Document, RgbaImage)> {
        recovery::prune_orphans(recovery::ORPHAN_MAX_AGE);
        let slot = recovery::slot_path(doc_path)?;
        if !recovery::is_newer(&slot, doc_path) {
            return None;
        }
        match qsdoc::read(&slot) {
            Ok(loaded) => Some((loaded.doc, loaded.base)),
            Err(_) => {
                // an unreadable copy would only ask again every launch
                recovery::clear(&slot);
                None
            }
        }
    }

    /// Puts an already-loaded document in place of the current one.
    fn apply_document(&mut self, doc: Document, base: RgbaImage) {
        let canvas_background = doc.background;
        let base_rect = RectF::new(0.0, 0.0, doc.base_size.w, doc.base_size.h);
        self.view = CanvasView::default();
        self.view.sync(&doc);
        self.view.center = self.view.map.to_display(base_rect.center());
        self.view.zoom = (700.0 / doc.base_size.w.max(1.0))
            .min(500.0 / doc.base_size.h.max(1.0))
            .clamp(0.2, 1.0);
        self.base = base;
        self.editor = Editor::new(doc);
        self.canvas_background = canvas_background;
        self.canvas_insets = [0.0; 4];
        self.canvas_anchor = (1, 1);
        self.texture = None;
        self.needs_render = true;
        self.needs_initial_fit = true;
    }

    /// Asks what to do with edits recovered from a crashed session.
    fn recovery_guard(&mut self, ctx: &egui::Context) {
        let Some((doc, base)) = self.pending_recovery.take() else {
            return;
        };
        let mut choice = None;
        egui::Window::new("Recover unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("The last session ended with edits that were never saved.");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Recover").clicked() {
                        choice = Some(true);
                    }
                    if ui.button("Discard").clicked() {
                        choice = Some(false);
                    }
                });
            });

        match choice {
            Some(true) => {
                self.apply_document(doc, base);
                self.editor.mark_modified();
                self.sync_unsaved();
                self.show_toast("Recovered unsaved changes");
            }
            Some(false) => {}
            // still open: keep the copy in hand for the next frame
            None => self.pending_recovery = Some((doc, base)),
        }
        if choice.is_some() {
            self.clear_autosave();
        }
    }

    pub fn open_file(
        path: impl AsRef<Path>,
        cc: &eframe::CreationContext<'_>,
    ) -> anyhow::Result<Self> {
        let (doc, base, doc_path) = load_document(path.as_ref())?;
        Ok(Self::new(doc, base, Some(doc_path), cc))
    }

    /// Swaps in another document, dropping the current one and its history.
    pub fn load_path(&mut self, path: &Path) -> anyhow::Result<()> {
        let (doc, base, doc_path) = load_document(path)?;
        self.apply_document(doc, base);
        self.doc_path = Some(doc_path);
        self.mark_saved();
        self.pending_recovery = Self::pending_recovery(self.doc_path.as_deref());
        let name = self.display_name().to_string();
        self.show_toast(format!("Opened {name}"));
        Ok(())
    }

    /// File name to show in titles, dialogs and toasts.
    fn display_name(&self) -> &str {
        self.doc_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
    }

    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), std::time::Instant::now()));
    }

    fn update_texture(&mut self, ctx: &egui::Context) {
        let flat = qs_render::render(self.editor.doc(), &self.base);
        let size = [flat.width() as usize, flat.height() as usize];
        let color_img = ColorImage::from_rgba_unmultiplied(size, flat.as_raw());
        if let Some(tex) = &mut self.texture {
            tex.set(color_img, TextureOptions::LINEAR);
        } else {
            self.texture =
                Some(ctx.load_texture("rendered_doc", color_img, TextureOptions::LINEAR));
        }
        self.needs_render = false;
    }

    /// Saves to the current path, asking for one when the document has never
    /// been written. A cancelled dialog leaves `unsaved` set.
    pub fn save(&mut self) -> anyhow::Result<()> {
        match self.doc_path.clone() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    /// Asks for a destination, then saves there.
    pub fn save_as(&mut self) -> anyhow::Result<()> {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Quickshort document", &[qsdoc::EXT])
            .set_file_name(self.display_name());
        if let Some(dir) = self.doc_path.as_ref().and_then(|p| p.parent()) {
            dialog = dialog.set_directory(dir);
        }
        let Some(path) = dialog.save_file() else {
            self.show_toast("Save cancelled");
            return Ok(());
        };
        let path = if path.extension().is_some() {
            path
        } else {
            path.with_extension(qsdoc::EXT)
        };
        let previous_slot = recovery::slot_path(self.doc_path.as_deref());
        self.save_to(&path)?;
        // an untitled or renamed document leaves its old recovery copy behind
        if let Some(old) = previous_slot {
            if Some(old.clone()) != recovery::slot_path(self.doc_path.as_deref()) {
                recovery::clear(&old);
            }
        }
        Ok(())
    }

    fn save_to(&mut self, path: &Path) -> anyhow::Result<()> {
        let flat = qs_render::render(self.editor.doc(), &self.base);
        let thumb = qsdoc::make_thumb(&flat);
        qsdoc::write(path, self.editor.doc(), &self.base, Some(&thumb))?;
        self.doc_path = Some(path.to_path_buf());
        self.mark_saved();
        self.clear_autosave();
        let name = self.display_name().to_string();
        self.show_toast(format!("Saved {name}"));
        Ok(())
    }

    /// Asks for a `.qsdoc` or an image and opens it in place of the current
    /// document.
    pub fn open_dialog(&mut self) -> anyhow::Result<()> {
        let picked = rfd::FileDialog::new()
            .add_filter("Quickshort document", &[qsdoc::EXT])
            .add_filter("Image", &["png", "jpg", "jpeg", "webp", "bmp"])
            .pick_file();
        match picked {
            Some(path) => self.load_path(&path),
            None => Ok(()),
        }
    }

    fn canvas_dialog(&mut self, ctx: &egui::Context) {
        if !self.canvas_dialog_open {
            return;
        }

        let mut open = self.canvas_dialog_open;
        let mut apply = false;
        let mut fit_to_image = false;
        let mut fit_to_objects = false;
        let mut trim = false;
        egui::Window::new("Canvas")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Canvas insets (px)");
                egui::Grid::new("canvas_insets")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        for (label, index) in [("Top", 0), ("Right", 1), ("Bottom", 2), ("Left", 3)] {
                            ui.label(label);
                            ui.add(
                                egui::DragValue::new(&mut self.canvas_insets[index])
                                    .speed(1.0)
                                    .range(-100_000.0..=100_000.0),
                            );
                            ui.end_row();
                        }
                    });
                ui.label("Anchor");
                egui::Grid::new("canvas_anchor")
                    .num_columns(3)
                    .spacing([4.0, 4.0])
                    .show(ui, |ui| {
                        let symbols = [["↖", "↑", "↗"], ["←", "•", "→"], ["↙", "↓", "↘"]];
                        for (row, row_symbols) in symbols.iter().enumerate() {
                            for (column, symbol) in row_symbols.iter().enumerate() {
                                if ui
                                    .selectable_label(self.canvas_anchor == (column, row), *symbol)
                                    .clicked()
                                {
                                    let vertical = self.canvas_insets[0] + self.canvas_insets[2];
                                    let horizontal = self.canvas_insets[1] + self.canvas_insets[3];
                                    self.canvas_insets = anchored_insets(vertical, horizontal, (column, row));
                                    self.canvas_anchor = (column, row);
                                }
                            }
                            ui.end_row();
                        }
                    });
                ui.label("Negative values crop that edge.");

                let [a, r, g, b] = self.canvas_background.to_be_bytes();
                let mut color = Color32::from_rgba_unmultiplied(r, g, b, a);
                ui.horizontal(|ui| {
                    ui.label("Canvas color");
                    ui.color_edit_button_srgba(&mut color);
                });
                let [r, g, b, a] = color.to_srgba_unmultiplied();
                self.canvas_background = u32::from_be_bytes([a, r, g, b]);
                ui.horizontal(|ui| {
                    if ui.button("Transparent").clicked() {
                        self.canvas_background = 0;
                    }
                    if ui.button("White").clicked() {
                        self.canvas_background = u32::MAX;
                    }
                    if ui.button("Black").clicked() {
                        self.canvas_background = 255 << 24;
                    }
                });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Trim edges").clicked() { trim = true; }
                    if ui.button("Fit to image").clicked() {
                        fit_to_image = true;
                    }
                    if ui.button("Fit to objects").clicked() {
                        fit_to_objects = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Apply").clicked() {
                            apply = true;
                        }
                    });
                });
            });
        self.canvas_dialog_open = open;

        if trim {
            if let Some(rect) = qs_edit::trim::calculate_trim_rect(self.editor.doc(), &self.base, 6) {
                self.editor.set_view(rect);
                self.touch();
                self.canvas_dialog_open = false;
            } else {
                self.show_toast("No safe uniform border to trim");
            }
        } else if fit_to_image {
            let view = self.editor.doc().view;
            let base = self.editor.doc().base_size;
            let top = view.y;
            let left = view.x;
            let right = base.w - view.w - left;
            let bottom = base.h - view.h - top;
            if self.editor.update_canvas(top, right, bottom, left, self.canvas_background) {
                self.touch();
            }
            self.canvas_insets = [0.0; 4];
            self.canvas_dialog_open = false;
        } else if fit_to_objects {
            if self.editor.fit_canvas_to_objects_with_background(self.canvas_background) {
                self.touch();
            }
            self.canvas_insets = [0.0; 4];
            self.canvas_dialog_open = false;
        } else if apply {
            // the dialog's edges are the displayed ones
            let insets = self.editor.doc().orientation.insets_to_input(self.canvas_insets);
            let [top, right, bottom, left] = insets;
            if self.editor.update_canvas(top, right, bottom, left, self.canvas_background) {
                self.touch();
            }
            self.canvas_insets = [0.0; 4];
            self.canvas_dialog_open = false;
        }
    }

    /// The rendered texture is stale after an interaction; whether the
    /// document is actually dirty is decided by [`EditorApp::sync_unsaved`].
    fn touch(&mut self) {
        self.needs_render = true;
    }

    /// Derives the unsaved flag from the editor's revision counter, so a click
    /// that changes nothing never marks the document dirty.
    fn sync_unsaved(&mut self) {
        self.unsaved = self.editor.revision() != self.saved_rev;
    }

    /// Remembers the current state as the one on disk.
    fn mark_saved(&mut self) {
        self.saved_rev = self.editor.revision();
        self.unsaved = false;
    }

    fn clear_autosave(&mut self) {
        if let Some(slot) = recovery::slot_path(self.doc_path.as_deref()) {
            recovery::clear(&slot);
        }
    }

    /// Refreshes the recovery copy, at most once per
    /// [`recovery::AUTOSAVE_INTERVAL`].
    fn autosave_tick(&mut self) {
        if !self.unsaved {
            return;
        }
        let now = std::time::Instant::now();
        if self
            .autosave_at
            .is_some_and(|last| now.duration_since(last) < recovery::AUTOSAVE_INTERVAL)
        {
            return;
        }
        self.autosave_at = Some(now);
        let Some(slot) = recovery::slot_path(self.doc_path.as_deref()) else {
            return;
        };
        let _ = qsdoc::write(&slot, self.editor.doc(), &self.base, None);
    }

    /// Keeps the window title in step with the file name and the unsaved mark.
    fn sync_window_title(&mut self, ctx: &egui::Context) {
        let mark = if self.unsaved { "\u{2022} " } else { "" };
        let title = format!("{mark}{} — Quickshort", self.display_name());
        if self.window_title.as_deref() != Some(title.as_str()) {
            self.window_title = Some(title.clone());
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }

    /// Intercepts a close request while edits are pending and runs the
    /// Save / Discard / Cancel dialog.
    fn close_guard(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.unsaved && !self.close_confirmed {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        if !self.confirm_close {
            return;
        }

        let mut decision = None;
        egui::Window::new("Unsaved changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!(
                    "“{}” has edits that are not saved yet.",
                    self.display_name()
                ));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        decision = Some(CloseDecision::Save);
                    }
                    if ui.button("Discard").clicked() {
                        decision = Some(CloseDecision::Discard);
                    }
                    if ui.button("Cancel").clicked() {
                        decision = Some(CloseDecision::Cancel);
                    }
                });
            });

        match decision {
            // a cancelled Save-As leaves the document unsaved: keep the window
            Some(CloseDecision::Save) => {
                if self.save().is_ok() && !self.unsaved {
                    self.finish_close(ctx);
                }
            }
            Some(CloseDecision::Discard) => {
                self.clear_autosave();
                self.mark_saved();
                self.finish_close(ctx);
            }
            Some(CloseDecision::Cancel) => self.confirm_close = false,
            None => {}
        }
    }

    fn finish_close(&mut self, ctx: &egui::Context) {
        self.confirm_close = false;
        self.close_confirmed = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    pub fn export_png(&mut self, path: Option<&Path>) -> anyhow::Result<()> {
        let flat = qs_render::render(self.editor.doc(), &self.base);
        let out_path = path
            .map(PathBuf::from)
            .or_else(|| self.doc_path.as_ref().map(|p| p.with_extension("png")))
            .unwrap_or_else(|| PathBuf::from("export.png"));
        flat.save(&out_path)?;
        let name = out_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("export.png");
        self.show_toast(format!("Exported {name}"));
        Ok(())
    }

    pub fn copy_to_clipboard(&mut self) -> anyhow::Result<()> {
        let flat = qs_render::render(self.editor.doc(), &self.base);
        clipboard::copy_image(&flat)?;
        self.show_toast("Copied image to clipboard");
        Ok(())
    }

    fn apply_zoom_cmd(&mut self, cmd: ZoomCmd) {
        let Some(rect) = self.last_canvas_rect else {
            return;
        };
        let center = rect.center();
        match cmd {
            ZoomCmd::None => {}
            ZoomCmd::In => self.view.zoom_at(1.25, center, rect),
            ZoomCmd::Out => self.view.zoom_at(0.8, center, rect),
            ZoomCmd::Fit => {
                self.view.sync(self.editor.doc());
                self.view.fit_to(self.view.display_rect(), rect, 40.0);
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let mut undo = false;
        let mut redo = false;
        let mut copy = false;
        let mut save = false;
        let mut save_as = false;
        let mut open = false;
        let mut del = false;
        let mut dup = false;
        let mut esc = false;
        let mut tool = None;
        let mut nudge = None;
        let mut orient = None;
        let mut zoom_cmd = ZoomCmd::None;

        ctx.input_mut(|i| {
            if self.editor.edit().is_some() {
                return;
            }

            if i.consume_key(Modifiers::COMMAND, Key::Z) || i.consume_key(Modifiers::CTRL, Key::Z) {
                if i.modifiers.shift {
                    redo = true;
                } else {
                    undo = true;
                }
                self.touch();
            } else if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
                || i.consume_key(Modifiers::CTRL, Key::Y)
            {
                redo = true;
                self.touch();
            }

            if i.consume_key(Modifiers::COMMAND, Key::C) || i.consume_key(Modifiers::CTRL, Key::C) {
                copy = true;
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
            }

            if i.consume_key(Modifiers::NONE, Key::Delete)
                || i.consume_key(Modifiers::NONE, Key::Backspace)
            {
                del = true;
            }

            if i.consume_key(Modifiers::COMMAND, Key::D) || i.consume_key(Modifiers::CTRL, Key::D) {
                dup = true;
            }

            if i.consume_key(Modifiers::NONE, Key::Escape) {
                esc = true;
            }

            if i.modifiers.is_none() {
                let pressed_tool = if i.key_pressed(Key::V) {
                    Some(Tool::Select)
                } else if i.key_pressed(Key::A) {
                    Some(Tool::Arrow)
                } else if i.key_pressed(Key::C) {
                    Some(Tool::Callout)
                } else if i.key_pressed(Key::T) {
                    Some(Tool::Text)
                } else if i.key_pressed(Key::S) {
                    Some(Tool::Step)
                } else if i.key_pressed(Key::R) {
                    Some(Tool::Rect)
                } else if i.key_pressed(Key::O) {
                    Some(Tool::Ellipse)
                } else if i.key_pressed(Key::L) {
                    Some(Tool::Line)
                } else if i.key_pressed(Key::P) {
                    Some(Tool::Pen)
                } else if i.key_pressed(Key::H) {
                    Some(Tool::Highlighter)
                } else if i.key_pressed(Key::B) {
                    Some(Tool::Blur)
                } else if i.key_pressed(Key::K) {
                    Some(Tool::Crop)
                } else if i.key_pressed(Key::M) {
                    Some(Tool::SelectRegion)
                } else {
                    None
                };
                if let Some(t) = pressed_tool {
                    tool = Some(t);
                }
            }

            let nudge_step = if i.modifiers.shift { 10.0 } else { 1.0 };
            if i.consume_key(Modifiers::NONE, Key::ArrowLeft)
                || i.consume_key(Modifiers::SHIFT, Key::ArrowLeft)
            {
                nudge = Some((-nudge_step, 0.0));
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowRight)
                || i.consume_key(Modifiers::SHIFT, Key::ArrowRight)
            {
                nudge = Some((nudge_step, 0.0));
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowUp)
                || i.consume_key(Modifiers::SHIFT, Key::ArrowUp)
            {
                nudge = Some((0.0, -nudge_step));
            }
            if i.consume_key(Modifiers::NONE, Key::ArrowDown)
                || i.consume_key(Modifiers::SHIFT, Key::ArrowDown)
            {
                nudge = Some((0.0, nudge_step));
            }
        });

        if undo {
            self.editor.undo();
            self.touch();
        }
        if redo {
            self.editor.redo();
            self.touch();
        }
        if copy {
            let _ = self.copy_to_clipboard();
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
        if del && self.editor.delete_selected() {
            self.touch();
        }
        if dup && self.editor.duplicate_selected().is_some() {
            self.touch();
        }
        if esc {
            self.editor.escape();
        }
        if let Some(t) = tool {
            self.editor.set_tool(t);
        }
        if let Some(cmd) = orient {
            image_panel::apply_orient(&mut self.editor, cmd);
            self.touch();
        }
        if let Some((dx, dy)) = nudge {
            // arrows move what the user sees, whatever the orientation
            let (dx, dy) = self.editor.doc().orientation.unmap_vector(dx, dy);
            self.editor.nudge(dx, dy);
            self.touch();
        }
    }

    fn handle_text_events(&mut self, ctx: &egui::Context) {
        if self.editor.edit().is_none() {
            return;
        }
        let events = ctx.input(|i| i.raw.events.clone());
        for ev in events {
            match ev {
                egui::Event::Text(s) => {
                    self.editor.text_insert(&s);
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
                            self.editor.text_insert("\n");
                            self.touch();
                        }
                        Key::Backspace => {
                            self.editor.text_backspace();
                            self.touch();
                        }
                        Key::Delete => {
                            self.editor.text_delete();
                            self.touch();
                        }
                        Key::ArrowLeft => self.editor.text_move(Motion::Left, extend),
                        Key::ArrowRight => self.editor.text_move(Motion::Right, extend),
                        Key::ArrowUp => self.editor.text_move(Motion::Up, extend),
                        Key::ArrowDown => self.editor.text_move(Motion::Down, extend),
                        Key::Home => self.editor.text_move(Motion::Home, extend),
                        Key::End => self.editor.text_move(Motion::End, extend),
                        Key::Escape => {
                            self.editor.end_text_edit();
                            self.touch();
                        }
                        Key::A if modifiers.command || modifiers.ctrl => {
                            self.editor.text_select_all();
                        }
                        Key::C if modifiers.command || modifiers.ctrl => {
                            if let Some(sel) = self.editor.text_selection() {
                                let _ = clipboard::copy_text(&sel);
                            }
                        }
                        _ => {}
                    }
                }
                egui::Event::Paste(s) => {
                    self.editor.text_insert(&s);
                    self.touch();
                }
                _ => {}
            }
        }
    }

    fn update_canvas_cursor(&self, ctx: &egui::Context, screen_rect: Rect, dragging: bool) {
        let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        if !screen_rect.contains(pos) {
            return;
        }
        let icon = match self.editor.tool() {
            Tool::Text => CursorIcon::Text,
            Tool::Select => {
                if dragging {
                    CursorIcon::Grabbing
                } else {
                    let doc_p = self.view.screen_to_doc(pos, screen_rect);
                    let slack = self.editor.hit_tol * 1.8;
                    let on_handle = self
                        .editor
                        .selection_handles()
                        .iter()
                        .any(|h| h.pos.dist(doc_p) <= slack);
                    if on_handle || self.editor.pick(doc_p).is_some() {
                        CursorIcon::Grab
                    } else {
                        CursorIcon::Default
                    }
                }
            }
            _ => CursorIcon::Crosshair,
        };
        ctx.set_cursor_icon(icon);
    }
}

impl eframe::App for EditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        self.recovery_guard(&ctx);
        self.close_guard(&ctx);
        self.handle_text_events(&ctx);
        self.handle_shortcuts(&ctx);

        let top_res = egui::Panel::top("top_bar")
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                toolbar::render_top_bar(
                    ui,
                    &mut self.editor,
                    self.view.zoom,
                    self.doc_path.as_deref(),
                )
            })
            .inner;

        let prop_mutated = egui::Panel::top("properties_bar")
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                toolbar::render_properties_bar(ui, &mut self.editor)
            })
            .inner;

        if top_res.mutated || prop_mutated {
            self.touch();
        }
        if top_res.copy {
            let _ = self.copy_to_clipboard();
        }
        if top_res.save {
            let _ = self.save();
        }
        if top_res.save_as {
            let _ = self.save_as();
        }
        if top_res.open {
            let _ = self.open_dialog();
        }
        if top_res.export {
            let _ = self.export_png(None);
        }
        if top_res.canvas {
            self.canvas_dialog_open = !self.canvas_dialog_open;
            self.canvas_background = self.editor.doc().background;
        }
        if top_res.zoom != ZoomCmd::None {
            self.apply_zoom_cmd(top_res.zoom);
        }
        self.canvas_dialog(&ctx);

        let panel_mutated = egui::Panel::right("image_panel")
            .resizable(false)
            .exact_size(236.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::bg_inspector_bot())
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(ui, |ui| image_panel::render(ui, &mut self.editor))
            .inner;
        if panel_mutated {
            self.touch();
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let (screen_rect, response) =
                    ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
                self.last_canvas_rect = Some(screen_rect);
                self.view.sync(self.editor.doc());

                if self.needs_initial_fit
                    && screen_rect.width() > 32.0
                    && screen_rect.height() > 32.0
                {
                    self.view.fit_to(self.view.display_rect(), screen_rect, 40.0);
                    self.view.zoom = self.view.zoom.min(1.0);
                    self.needs_initial_fit = false;
                }

                let mut painter = ui.painter_at(screen_rect);

                // Zoom & Pan via mouse wheel
                ui.input(|i| {
                    if screen_rect.contains(i.pointer.latest_pos().unwrap_or_default()) {
                        let zoom_delta = i.zoom_delta();
                        if (zoom_delta - 1.0).abs() > 0.001 {
                            if let Some(pos) = i.pointer.latest_pos() {
                                self.view.zoom_at(zoom_delta, pos, screen_rect);
                            }
                        } else if i.smooth_scroll_delta != Vec2::ZERO {
                            if i.modifiers.ctrl || i.modifiers.command {
                                let factor = (1.0 + i.smooth_scroll_delta.y * 0.01).clamp(0.5, 2.0);
                                if let Some(pos) = i.pointer.latest_pos() {
                                    self.view.zoom_at(factor, pos, screen_rect);
                                }
                            } else {
                                self.view.pan(i.smooth_scroll_delta);
                            }
                        }
                    }
                });

                // Pointer interactions on canvas
                let shift_held = ui.input(|i| i.modifiers.shift);
                self.editor.hit_tol = 6.0 / self.view.zoom.max(0.1);

                if response.double_clicked() {
                    self.saw_drag_press = false;
                    if let Some(pos) = response.interact_pointer_pos() {
                        let doc_p = self.view.screen_to_doc(pos, screen_rect);
                        self.editor.double_click(doc_p);
                        self.touch();
                    }
                } else if response.drag_started() {
                    self.saw_drag_press = true;
                    if let Some(pos) = response.interact_pointer_pos() {
                        let doc_p = self.view.screen_to_doc(pos, screen_rect);
                        self.editor.pointer_down(doc_p);
                        self.touch();
                    }
                } else if response.dragged() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let doc_p = self.view.screen_to_doc(pos, screen_rect);
                        self.editor.pointer_move(doc_p, shift_held);
                        self.touch();
                    }
                } else if response.drag_stopped() {
                    self.saw_drag_press = false;
                    if let Some(pos) = response.interact_pointer_pos() {
                        let doc_p = self.view.screen_to_doc(pos, screen_rect);
                        self.editor.pointer_up(doc_p);
                        self.touch();
                    }
                } else if response.clicked() && !self.saw_drag_press {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let doc_p = self.view.screen_to_doc(pos, screen_rect);
                        self.editor.pointer_down(doc_p);
                        self.editor.pointer_up(doc_p);
                        self.touch();
                    }
                }

                self.update_canvas_cursor(&ctx, screen_rect, response.dragged());

                if self.needs_render {
                    self.update_texture(&ctx);
                }

                // 1. Calm matte macOS workspace backdrop
                canvas::paint_workspace(&painter, screen_rect);

                // 2. Document screen bounds + soft multi-layer drop shadow
                self.view.sync(self.editor.doc());
                let img_screen_rect =
                    self.view.display_rect_to_screen(self.view.display_rect(), screen_rect);

                canvas::paint_document_shadow(&painter, img_screen_rect);

                // 3. Checkerboard strictly inside document bounds (visible only through alpha)
                canvas::paint_checkerboard(&painter, img_screen_rect);

                // 4. Rendered flattened document
                if let Some(tex) = &self.texture {
                    painter.image(
                        tex.id(),
                        img_screen_rect,
                        Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }

                // 5. Crisp hairline rim around document
                canvas::paint_document_frame(&painter, img_screen_rect);

                // 5b. Crop preview: dim everything outside the pending crop rect
                if self.editor.tool() == Tool::Crop {
                    if let Some(crop) = self.editor.crop_rect() {
                        let crop_screen = self.view.doc_rect_to_screen(crop, screen_rect);
                        canvas::paint_crop_overlay(&mut painter, crop_screen, screen_rect);
                    }
                }

                // 5c. Committed selection region outline (when not cropping)
                if self.editor.tool() != Tool::Crop
                    && self.editor.tool() != Tool::SelectRegion
                    && self.editor.region().is_some()
                {
                    let region = self.editor.region().unwrap();
                    let region_screen = self.view.doc_rect_to_screen(region, screen_rect);
                    canvas::paint_region(&mut painter, region_screen);
                }

                // 6. Selection outline & handles
                let handles = self.editor.selection_handles();
                if !handles.is_empty() {
                    canvas::paint_handles(
                        &painter,
                        &handles,
                        self.editor.selected_object().map(|o| &o.kind),
                        &self.view,
                        screen_rect,
                    );
                }

                // 7. Active text-edit caret
                if let Some(te) = self.editor.edit() {
                    if let Some(obj) = self.editor.doc().get(te.id) {
                        if let Some((origin, lay)) = qs_render::text_block(&obj.kind) {
                            let (cx, cy) = lay.caret(te.caret);
                            let p1 = self.view.doc_to_screen(
                                PointF::new(origin.x + cx, origin.y + cy),
                                screen_rect,
                            );
                            let p2 = self.view.doc_to_screen(
                                PointF::new(origin.x + cx, origin.y + cy + 18.0),
                                screen_rect,
                            );
                            painter.line_segment([p1, p2], Stroke::new(2.0, accent_blue()));
                        }
                    }
                }

                // 8. Floating macOS Toast HUD
                if let Some((msg, time)) = &self.toast {
                    let elapsed = time.elapsed().as_secs_f32();
                    if elapsed < 2.6 {
                        let fade = if elapsed > 2.2 {
                            ((2.6 - elapsed) / 0.4).clamp(0.0, 1.0)
                        } else {
                            1.0
                        };
                        let text_galley = painter.layout_no_wrap(
                            msg.clone(),
                            FontId::proportional(12.5),
                            text_primary(),
                        );
                        let w = (text_galley.size().x + 52.0).clamp(160.0, 420.0);
                        let hud_rect = Rect::from_center_size(
                            Pos2::new(screen_rect.center().x, screen_rect.bottom() - 34.0),
                            Vec2::new(w, 34.0),
                        );
                        let alpha_bg = (232.0 * fade) as u8;
                        let alpha_fg = (255.0 * fade) as u8;
                        theme::paint_soft_shadow(
                            &painter,
                            hud_rect,
                            17.0,
                            5.0,
                            14.0,
                            (100.0 * fade) as u8,
                        );
                        painter.rect_filled(
                            hud_rect,
                            17.0,
                            Color32::from_rgba_unmultiplied(
                                qs_theme::pal().panel_top.to_be_bytes()[1],
                                qs_theme::pal().panel_top.to_be_bytes()[2],
                                qs_theme::pal().panel_top.to_be_bytes()[3],
                                alpha_bg,
                            ),
                        );
                        painter.rect_stroke(
                            hud_rect,
                            17.0,
                            Stroke::new(
                                1.0,
                                Color32::from_rgba_unmultiplied(
                                    border_subtle().r(),
                                    border_subtle().g(),
                                    border_subtle().b(),
                                    (32.0 * fade) as u8,
                                ),
                            ),
                            StrokeKind::Inside,
                        );
                        icons::draw_action_icon(
                            &painter,
                            Pos2::new(hud_rect.left() + 18.0, hud_rect.center().y),
                            ActionIcon::CheckCircle,
                            Color32::from_rgba_unmultiplied(
                                success_green().r(),
                                success_green().g(),
                                success_green().b(),
                                alpha_fg,
                            ),
                        );
                        painter.text(
                            Pos2::new(hud_rect.left() + 34.0, hud_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            msg,
                            FontId::proportional(12.5),
                            Color32::from_rgba_unmultiplied(
                                text_primary().r(),
                                text_primary().g(),
                                text_primary().b(),
                                alpha_fg,
                            ),
                        );
                        ctx.request_repaint();
                    }
                }
            });

        self.sync_unsaved();
        self.autosave_tick();
        self.sync_window_title(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_anchor_rebalances_added_space_without_changing_canvas_size() {
        assert_eq!(anchored_insets(24.0, 36.0, (0, 2)), [24.0, 36.0, 0.0, 0.0]);
        assert_eq!(anchored_insets(24.0, 36.0, (1, 1)), [12.0, 18.0, 12.0, 18.0]);
        assert_eq!(anchored_insets(-20.0, -30.0, (2, 0)), [0.0, 0.0, -20.0, -30.0]);
    }
}

/// Reads `.qsdoc`, an image beside its `.qsdoc`, or a bare image.
fn load_document(p: &Path) -> anyhow::Result<(Document, RgbaImage, PathBuf)> {
    let side = qsdoc::sibling_path(p);
    // a flattened PNG with its `.qsdoc` beside it (overlay Edit) reopens
    // with every annotation still an object, not baked pixels
    let side_doc = (!qsdoc::is_qsdoc(p) && side.exists())
        .then(|| qsdoc::read(&side).ok())
        .flatten();
    if qsdoc::is_qsdoc(p) {
        let loaded = qsdoc::read(p)?;
        Ok((loaded.doc, loaded.base, p.to_path_buf()))
    } else if let Some(loaded) = side_doc {
        Ok((loaded.doc, loaded.base, side))
    } else {
        let img = image::open(p)?.to_rgba8();
        let (w, h) = (img.width(), img.height());
        let doc = Document::new(w, h, 1.0);
        Ok((doc, img, qsdoc::sibling_path(p)))
    }
}
