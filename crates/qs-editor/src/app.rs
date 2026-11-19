//! Main standalone editor application implementing `eframe::App`.

use crate::{
    canvas::{self, CanvasView},
    clipboard,
    filmstrip::Filmstrip,
    fonts,
    icons::{self, ActionIcon},
    image_panel::{self, OrientCmd},
    recovery,
    styles,
    templates::{self, CombineLayout},
    theme::{self, accent_blue, border_subtle, success_green, text_primary},
    toolbar::{self, ZoomCmd},
};
use eframe::egui::{
    self, Color32, ColorImage, CursorIcon, FontId, Key, Modifiers, Pos2, Rect, Stroke, StrokeKind,
    TextureHandle, TextureOptions, Vec2,
};
use image::RgbaImage;
use qs_doc::{Document, ObjectId, PointF, RectF, qsdoc};
use qs_edit::{Alignment, Axis, Editor, Motion, Tool};
use std::path::{Path, PathBuf};

/// What the user picked in the "unsaved changes" dialog.
enum CloseDecision {
    Save,
    Discard,
    Cancel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ExportFormat {
    #[default]
    Png,
    Jpeg,
    Webp,
}

impl ExportFormat {
    fn ext(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG (Lossless)",
            Self::Jpeg => "JPEG (Compact)",
            Self::Webp => "WebP",
        }
    }
}

struct SavedDocSession {
    editor: Editor,
    base: RgbaImage,
    doc_path: Option<PathBuf>,
    view: CanvasView,
    unsaved: bool,
    saved_rev: u64,
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
    /// "Flatten all" confirmation is on screen.
    confirm_flatten: bool,
    canvas_insets: [f32; 4],
    canvas_background: u32,
    resize_w: u32,
    resize_h: u32,
    resize_lock: bool,
    export_dialog_open: bool,
    export_format: ExportFormat,
    export_quality: u8,
    combine_dialog_open: bool,
    combine_layout: CombineLayout,
    tray_selected: Vec<PathBuf>,
    show_layers: bool,
    show_inspector: bool,
    show_filmstrip: bool,
    last_cursor_doc: Option<qs_doc::PointF>,
    filmstrip: Filmstrip,
    session_cache: std::collections::HashMap<PathBuf, SavedDocSession>,
    canvas_anchor: (usize, usize),
    layer_drag_id: Option<ObjectId>,
    alignment_open: bool,
    /// Quick Styles: built-in presets + the user's saved ones.
    styles: styles::Library,
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

        let (init_w, init_h) = (base.width().max(1), base.height().max(1));
        let filmstrip = Filmstrip::new(doc_path.as_deref());
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
            confirm_flatten: false,
            canvas_insets: [0.0; 4],
            canvas_background,
            resize_w: init_w,
            resize_h: init_h,
            resize_lock: true,
            export_dialog_open: false,
            export_format: ExportFormat::Png,
            export_quality: 90,
            combine_dialog_open: false,
            combine_layout: CombineLayout::SideBySide,
            tray_selected: Vec::new(),
            canvas_anchor: (1, 1),
            show_layers: true,
            show_inspector: true,
            show_filmstrip: true,
            last_cursor_doc: None,
            filmstrip,
            session_cache: std::collections::HashMap::new(),
            layer_drag_id: None,
            alignment_open: false,
            styles: styles::Library::load(),
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
            .frame(theme::modal_frame())
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

    /// Switch to another capture/document, preserving the current one in the
    /// session cache (zoom, pan, unsaved state, and undo/redo history).
    pub fn switch_to_path(&mut self, path: &Path) -> anyhow::Result<()> {
        if self.doc_path.as_deref() == Some(path) {
            return Ok(());
        }
        if let Some(cur_path) = self.doc_path.clone() {
            let blank_doc = Document::new(1, 1, 1.0);
            let old_editor = std::mem::replace(&mut self.editor, Editor::new(blank_doc));
            let old_base = std::mem::replace(
                &mut self.base,
                RgbaImage::new(1, 1),
            );
            let old_view = std::mem::take(&mut self.view);
            self.session_cache.insert(
                cur_path.clone(),
                SavedDocSession {
                    editor: old_editor,
                    base: old_base,
                    doc_path: Some(cur_path),
                    view: old_view,
                    unsaved: self.unsaved,
                    saved_rev: self.saved_rev,
                },
            );
        }

        if let Some(cached) = self.session_cache.remove(path) {
            self.editor = cached.editor;
            self.base = cached.base;
            self.doc_path = cached.doc_path;
            self.view = cached.view;
            self.unsaved = cached.unsaved;
            self.saved_rev = cached.saved_rev;
            self.needs_render = true;
            self.needs_initial_fit = false;
            self.pending_recovery = None;
            let name = self.display_name().to_string();
            self.show_toast(format!("Switched to {name}"));
            return Ok(());
        }

        self.load_path(path)
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
        // Finalize pending gestures before remembering the saved revision.
        self.editor.finish_effects();
        self.editor.end_text_edit();
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

    /// "Flatten all" confirmation: baking annotations cannot be undone.
    fn flatten_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_flatten {
            return;
        }
        let mut open = self.confirm_flatten;
        let mut go = false;
        egui::Window::new("Flatten all annotations")
            .frame(theme::modal_frame())
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Every annotation is baked into the image.");
                ui.label("The layers and the undo history are removed.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Flatten").clicked() {
                        go = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_flatten = false;
                    }
                });
            });
        self.confirm_flatten = open;
        if go {
            self.confirm_flatten = false;
            if let Err(e) = self.flatten_all() {
                self.show_toast(format!("Flatten failed: {e}"));
            }
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
        let mut resample = false;
        egui::Window::new("Canvas")
            .frame(theme::modal_frame())
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Canvas insets (px)");
                egui::Grid::new("canvas_insets")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        for (label, index) in [("Top", 0), ("Right", 1), ("Bottom", 2), ("Left", 3)]
                        {
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
                                    self.canvas_insets =
                                        anchored_insets(vertical, horizontal, (column, row));
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
                    if ui.button("Trim edges").clicked() {
                        trim = true;
                    }
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
                ui.add_space(6.0);
                ui.separator();
                ui.label("Resample image (px)");
                ui.horizontal(|ui| {
                    let (bw, bh) = (self.base.width().max(1), self.base.height().max(1));
                    let aspect = bw as f32 / bh as f32;
                    let w_resp = ui.add(
                        egui::DragValue::new(&mut self.resize_w)
                            .range(1..=16384)
                            .prefix("W: "),
                    );
                    if w_resp.changed() && self.resize_lock {
                        self.resize_h = ((self.resize_w as f32 / aspect).round() as u32).max(1);
                    }
                    let h_resp = ui.add(
                        egui::DragValue::new(&mut self.resize_h)
                            .range(1..=16384)
                            .prefix("H: "),
                    );
                    if h_resp.changed() && self.resize_lock {
                        self.resize_w = ((self.resize_h as f32 * aspect).round() as u32).max(1);
                    }
                    ui.checkbox(&mut self.resize_lock, "Lock");
                    if ui.button("Resize").clicked() {
                        resample = true;
                    }
                });
            });
        self.canvas_dialog_open = open;

        if resample {
            let nw = self.resize_w.clamp(1, 16384);
            let nh = self.resize_h.clamp(1, 16384);
            if nw != self.base.width() || nh != self.base.height() {
                let resized =
                    image::imageops::resize(&self.base, nw, nh, image::imageops::FilterType::Lanczos3);
                let sx = nw as f32 / self.base.width().max(1) as f32;
                let sy = nh as f32 / self.base.height().max(1) as f32;
                let mut doc = self.editor.doc().clone();
                doc.base_size = qs_doc::SizeF::new(nw as f32 / doc.scale, nh as f32 / doc.scale);
                doc.view = RectF::new(
                    doc.view.x * sx,
                    doc.view.y * sy,
                    doc.view.w * sx,
                    doc.view.h * sy,
                );
                self.apply_document(doc, resized);
                self.editor.mark_modified();
                self.touch();
                self.show_toast(format!("Resized image to {nw}×{nh}"));
            }
            self.canvas_dialog_open = false;
        } else if trim {
            if let Some(rect) = qs_edit::trim::calculate_trim_rect(self.editor.doc(), &self.base, 6)
            {
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
            if self
                .editor
                .update_canvas(top, right, bottom, left, self.canvas_background)
            {
                self.touch();
            }
            self.canvas_insets = [0.0; 4];
            self.canvas_dialog_open = false;
        } else if fit_to_objects {
            if self
                .editor
                .fit_canvas_to_objects_with_background(self.canvas_background)
            {
                self.touch();
            }
            self.canvas_insets = [0.0; 4];
            self.canvas_dialog_open = false;
        } else if apply {
            // the dialog's edges are the displayed ones
            let insets = self
                .editor
                .doc()
                .orientation
                .insets_to_input(self.canvas_insets);
            let [top, right, bottom, left] = insets;
            if self
                .editor
                .update_canvas(top, right, bottom, left, self.canvas_background)
            {
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
            .frame(theme::modal_frame())
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

    fn export_with_dialog(&mut self, fmt: ExportFormat, quality: u8) -> anyhow::Result<()> {
        let default_name = self
            .doc_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("export");
        let Some(out_path) = rfd::FileDialog::new()
            .set_file_name(format!("{default_name}.{}", fmt.ext()))
            .add_filter(fmt.label(), &[fmt.ext()])
            .save_file()
        else {
            return Ok(());
        };
        let flat = qs_render::render(self.editor.doc(), &self.base);
        match fmt {
            ExportFormat::Png => flat.save(&out_path)?,
            ExportFormat::Jpeg => {
                let rgb = image::DynamicImage::ImageRgba8(flat).to_rgb8();
                let mut f = std::fs::File::create(&out_path)?;
                let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, quality);
                enc.encode_image(&rgb)?;
            }
            ExportFormat::Webp => {
                image::DynamicImage::ImageRgba8(flat).save(&out_path)?;
            }
        }
        let name = out_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("export");
        self.show_toast(format!("Exported {name}"));
        Ok(())
    }

    fn print_image(&mut self) -> anyhow::Result<()> {
        let flat = qs_render::render(self.editor.doc(), &self.base);
        let tmp = std::env::temp_dir().join(format!("quickshort_print_{}.png", std::process::id()));
        flat.save(&tmp)?;
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .args(["-a", "Preview"])
                .arg(&tmp)
                .spawn()?;
        }
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open").arg(&tmp).spawn()?;
        }
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/c", "start", ""])
                .arg(&tmp)
                .spawn()?;
        }
        self.show_toast("Opened in system print viewer");
        Ok(())
    }

    fn export_dialog(&mut self, ctx: &egui::Context) {
        if !self.export_dialog_open {
            return;
        }
        let mut open = self.export_dialog_open;
        let mut do_export = false;
        let mut do_copy = false;
        let mut do_save = false;
        let mut do_print = false;

        egui::Window::new("Export & Share")
            .frame(theme::modal_frame())
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(280.0);
                theme::section_header(ui, "Format");
                ui.horizontal(|ui| {
                    for fmt in [ExportFormat::Png, ExportFormat::Jpeg, ExportFormat::Webp] {
                        ui.selectable_value(&mut self.export_format, fmt, fmt.label());
                    }
                });
                if self.export_format == ExportFormat::Jpeg {
                    ui.add_space(4.0);
                    ui.add(
                        egui::Slider::new(&mut self.export_quality, 40..=100)
                            .suffix("%")
                            .text("Quality"),
                    );
                }
                ui.add_space(10.0);
                theme::section_header(ui, "Destination");
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Save Image As…").clicked() {
                        do_export = true;
                    }
                    if ui.button("Copy Flattened").clicked() {
                        do_copy = true;
                    }
                    if ui.button("Save .qsdoc").clicked() {
                        do_save = true;
                    }
                    if ui.button("Print…").clicked() {
                        do_print = true;
                    }
                });
            });
        self.export_dialog_open = open;

        if do_export {
            if let Err(e) = self.export_with_dialog(self.export_format, self.export_quality) {
                self.show_toast(format!("Export failed: {e}"));
            }
            self.export_dialog_open = false;
        } else if do_copy {
            let _ = self.copy_to_clipboard();
            self.export_dialog_open = false;
        } else if do_save {
            let _ = self.save();
            self.export_dialog_open = false;
        } else if do_print {
            if let Err(e) = self.print_image() {
                self.show_toast(format!("Print failed: {e}"));
            }
            self.export_dialog_open = false;
        }
    }

    fn combine_dialog(&mut self, ctx: &egui::Context) {
        if !self.combine_dialog_open {
            return;
        }
        let mut open = self.combine_dialog_open;
        let mut create = false;
        egui::Window::new("Combine in Template")
            .frame(theme::modal_frame())
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(260.0);
                ui.label(format!(
                    "Combining {} captures with numbered Step badges:",
                    self.tray_selected.len()
                ));
                ui.add_space(6.0);
                for layout in [
                    CombineLayout::SideBySide,
                    CombineLayout::VerticalSteps,
                    CombineLayout::Grid2x2,
                ] {
                    ui.radio_value(&mut self.combine_layout, layout, layout.label());
                }
                ui.add_space(10.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Create Combined Sheet").clicked() {
                        create = true;
                    }
                });
            });
        self.combine_dialog_open = open;

        if create {
            match templates::build_combined_document(
                &self.tray_selected,
                self.combine_layout,
                self.editor.doc().background,
            ) {
                Ok((doc, base)) => {
                    self.doc_path = None;
                    self.apply_document(doc, base);
                    self.editor.mark_modified();
                    self.touch();
                    self.tray_selected.clear();
                    self.combine_dialog_open = false;
                    self.show_toast("Created combined template");
                }
                Err(e) => {
                    self.show_toast(format!("Combine failed: {e}"));
                }
            }
        }
    }

    pub fn copy_to_clipboard(&mut self) -> anyhow::Result<()> {
        let region = self.editor.doc().region;
        let (flat, what) = match region.and_then(|r| {
            qs_render::render_region(self.editor.doc(), &self.base, r)
        }) {
            Some(cropped) => (cropped, "selection"),
            None => (qs_render::render(self.editor.doc(), &self.base), "image"),
        };
        clipboard::copy_image(&flat)?;
        self.show_toast(format!("Copied {what} to clipboard"));
        Ok(())
    }

    /// Bake every annotation into the image and start a fresh document over
    /// the result. Destructive: the annotations and the undo history are gone.
    pub fn flatten_all(&mut self) -> anyhow::Result<()> {
        let flat = qs_render::render(self.editor.doc(), &self.base);
        let old = self.editor.doc().clone();
        let mut doc = Document::new(flat.width(), flat.height(), old.scale);
        doc.meta = old.meta.clone();
        doc.background = old.background;
        self.apply_document(doc, flat);
        self.editor.mark_modified();
        self.touch();
        self.show_toast("Flattened annotations into the image");
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
        let mut group = false;
        let mut ungroup = false;

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

            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::G)
                || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::G)
            {
                ungroup = true;
            } else if i.consume_key(Modifiers::COMMAND, Key::G)
                || i.consume_key(Modifiers::CTRL, Key::G)
            {
                group = true;
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
                } else if i.key_pressed(Key::I) {
                    Some(Tool::Magnify)
                } else if i.key_pressed(Key::F) {
                    Some(Tool::Spotlight)
                } else if i.key_pressed(Key::G) {
                    Some(Tool::Stamp)
                } else if i.key_pressed(Key::E) {
                    Some(Tool::Eraser)
                } else if i.key_pressed(Key::K) {
                    Some(Tool::Crop)
                } else if i.key_pressed(Key::X) {
                    Some(Tool::Cutout)
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
        if group && self.editor.group_selected() {
            self.touch();
        }
        if ungroup && self.editor.ungroup_selected() {
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
        self.filmstrip.poll(&ctx);
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
        let styles_mutated = if !styles::presets_for(&self.editor, &self.styles).is_empty() {
            egui::Panel::top("quick_style_bar")
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    styles::render_bar(ui, &mut self.editor, &mut self.styles)
                })
                .inner
        } else {
            false
        };
        if styles_mutated {
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
            self.export_dialog_open = !self.export_dialog_open;
        }
        if top_res.canvas {
            self.canvas_dialog_open = !self.canvas_dialog_open;
            self.canvas_background = self.editor.doc().background;
            self.resize_w = self.base.width().max(1);
            self.resize_h = self.base.height().max(1);
        }
        if top_res.zoom != ZoomCmd::None {
            self.apply_zoom_cmd(top_res.zoom);
        }
        self.canvas_dialog(&ctx);
        self.flatten_dialog(&ctx);
        self.export_dialog(&ctx);
        self.combine_dialog(&ctx);

        // Bottom Studio Status Bar (must be added before CentralPanel)
        egui::Panel::bottom("status_bar")
            .exact_size(26.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::bg_status())
                    .stroke(egui::Stroke::new(1.0, theme::border_dark_hairline()))
                    .inner_margin(egui::Margin::symmetric(10, 3)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if ui
                        .selectable_label(self.show_layers, "Layers")
                        .on_hover_text("Toggle Layers panel")
                        .clicked()
                    {
                        self.show_layers = !self.show_layers;
                    }
                    if ui
                        .selectable_label(self.show_inspector, "Inspector")
                        .on_hover_text("Toggle Properties & Effects panel")
                        .clicked()
                    {
                        self.show_inspector = !self.show_inspector;
                    }
                    if ui
                        .selectable_label(self.show_filmstrip, "Tray")
                        .on_hover_text("Toggle Recent Captures filmstrip")
                        .clicked()
                    {
                        self.show_filmstrip = !self.show_filmstrip;
                    }
                    ui.separator();

                    let view_r = self.view.display_rect();
                    ui.label(
                        egui::RichText::new(format!(
                            "{} × {} px",
                            view_r.w.round() as i32,
                            view_r.h.round() as i32
                        ))
                        .size(11.0)
                        .color(theme::text_secondary()),
                    );

                    if let Some(p) = self.last_cursor_doc {
                        ui.separator();
                        ui.label(
                            egui::RichText::new(format!(
                                "X: {}  Y: {}",
                                p.x.round() as i32,
                                p.y.round() as i32
                            ))
                            .size(11.0)
                            .color(theme::text_tertiary()),
                        );
                    }

                    if let Some(b) = self.editor.selection_bounds() {
                        ui.separator();
                        let count = self.editor.selection().len();
                        let label = if count > 1 {
                            format!(
                                "Sel ({count}): {}×{}",
                                b.w.round() as i32,
                                b.h.round() as i32
                            )
                        } else {
                            format!("Sel: {}×{}", b.w.round() as i32, b.h.round() as i32)
                        };
                        ui.label(
                            egui::RichText::new(label)
                                .size(11.0)
                                .color(theme::accent_blue_soft()),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for (label, target_zoom) in [("200%", 2.0f32), ("100%", 1.0), ("50%", 0.5)]
                        {
                            let active = (self.view.zoom - target_zoom).abs() < 0.01;
                            if ui
                                .selectable_label(active, egui::RichText::new(label).size(11.0))
                                .clicked()
                            {
                                if let Some(rect) = self.last_canvas_rect {
                                    self.view.zoom_at(
                                        target_zoom / self.view.zoom.max(0.01),
                                        rect.center(),
                                        rect,
                                    );
                                }
                            }
                        }
                        if ui
                            .selectable_label(false, egui::RichText::new("Fit").size(11.0))
                            .on_hover_text("Fit to window")
                            .clicked()
                        {
                            self.apply_zoom_cmd(ZoomCmd::Fit);
                        }
                    });
                });
            });

        // Collapsible Recent Captures Filmstrip (sits cleanly above Status Bar)
        if self.show_filmstrip {
            let mut switch_target: Option<PathBuf> = None;
            let mut refresh_tray = false;
            egui::Panel::bottom("filmstrip_panel")
                .exact_size(94.0)
                .frame(
                    egui::Frame::NONE
                        .fill(theme::bg_inspector_bot())
                        .stroke(egui::Stroke::new(1.0, theme::border_dark_hairline()))
                        .inner_margin(egui::Margin::symmetric(10, 6)),
                )
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(74.0);
                            ui.label(
                                egui::RichText::new("RECENT")
                                    .size(10.0)
                                    .strong()
                                    .color(theme::text_tertiary()),
                            );
                            ui.add_space(3.0);
                            if ui
                                .small_button("Refresh")
                                .on_hover_text("Rescan recent captures (Shift+Click to multi-select)")
                                .clicked()
                            {
                                refresh_tray = true;
                            }
                            if self.tray_selected.len() >= 2
                                && ui
                                    .small_button(format!("Combine ({})", self.tray_selected.len()))
                                    .on_hover_text("Combine selected captures into a template")
                                    .clicked()
                            {
                                self.combine_dialog_open = true;
                            }
                        });
                        ui.separator();
                        if self.filmstrip.items.is_empty() {
                            ui.centered_and_justified(|ui| {
                                let msg = if self.filmstrip.scanning {
                                    "Scanning recent captures…"
                                } else {
                                    "No recent captures found in ~/Pictures/quickshort"
                                };
                                ui.label(
                                    egui::RichText::new(msg)
                                        .size(11.5)
                                        .color(theme::text_tertiary()),
                                );
                            });
                        } else {
                            egui::ScrollArea::horizontal()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 8.0;
                                        for item in &self.filmstrip.items {
                                            let is_active =
                                                self.doc_path.as_deref() == Some(item.path.as_path());
                                            let is_tray_sel = self
                                                .tray_selected
                                                .iter()
                                                .any(|p| p == &item.path);
                                            let is_dirty = is_active && self.unsaved
                                                || self
                                                    .session_cache
                                                    .get(&item.path)
                                                    .is_some_and(|s| s.unsaved);
                                            let (rect, resp) = ui.allocate_exact_size(
                                                Vec2::new(108.0, 78.0),
                                                egui::Sense::click(),
                                            );
                                            let bg = if is_active {
                                                theme::bg_pill_active_bot()
                                            } else if resp.hovered() {
                                                theme::bg_btn_secondary_hover_bot()
                                            } else {
                                                theme::bg_card()
                                            };
                                            let border = if is_tray_sel {
                                                Stroke::new(1.5, theme::success_green())
                                            } else if is_active {
                                                Stroke::new(1.5, theme::accent_blue())
                                            } else if resp.hovered() {
                                                Stroke::new(1.0, theme::border_hot())
                                            } else {
                                                Stroke::new(1.0, theme::border_subtle())
                                            };
                                            ui.painter().rect(
                                                rect,
                                                theme::RADIUS_PILL,
                                                bg,
                                                border,
                                                StrokeKind::Inside,
                                            );
                                            if let Some(tex) = &item.texture {
                                                let avail = rect.shrink2(Vec2::new(6.0, 14.0));
                                                let img_rect = Rect::from_min_max(
                                                    Pos2::new(avail.left(), rect.top() + 5.0),
                                                    Pos2::new(avail.right(), rect.bottom() - 18.0),
                                                );
                                                let tsz = tex.size_vec2();
                                                let s = (img_rect.width() / tsz.x.max(1.0))
                                                    .min(img_rect.height() / tsz.y.max(1.0));
                                                let fit = Rect::from_center_size(
                                                    img_rect.center(),
                                                    tsz * s,
                                                );
                                                ui.painter().image(
                                                    tex.id(),
                                                    fit,
                                                    Rect::from_min_max(
                                                        Pos2::ZERO,
                                                        Pos2::new(1.0, 1.0),
                                                    ),
                                                    Color32::WHITE,
                                                );
                                            }
                                            let mut name = item.label.clone();
                                            if name.len() > 14 {
                                                name.truncate(11);
                                                name.push_str("…");
                                            }
                                            ui.painter().text(
                                                Pos2::new(rect.center().x, rect.bottom() - 9.0),
                                                egui::Align2::CENTER_CENTER,
                                                &name,
                                                FontId::proportional(10.0),
                                                if is_active {
                                                    theme::text_primary()
                                                } else {
                                                    theme::text_secondary()
                                                },
                                            );
                                            if is_dirty {
                                                ui.painter().circle_filled(
                                                    Pos2::new(rect.right() - 7.0, rect.top() + 7.0),
                                                    3.5,
                                                    theme::amber_orange(),
                                                );
                                            }
                                            if resp
                                                .on_hover_text(format!(
                                                    "{} (Shift+Click to select for Combine)",
                                                    item.label
                                                ))
                                                .clicked()
                                            {
                                                if ui.input(|i| i.modifiers.shift) {
                                                    if let Some(pos) = self
                                                        .tray_selected
                                                        .iter()
                                                        .position(|p| p == &item.path)
                                                    {
                                                        self.tray_selected.remove(pos);
                                                    } else {
                                                        self.tray_selected.push(item.path.clone());
                                                    }
                                                } else {
                                                    switch_target = Some(item.path.clone());
                                                }
                                            }
                                        }
                                    });
                                });
                        }
                    });
                });
            if refresh_tray {
                self.filmstrip.refresh(self.doc_path.as_deref());
            }
            if let Some(target) = switch_target {
                if let Err(e) = self.switch_to_path(&target) {
                    self.show_toast(format!("Failed to open: {e}"));
                }
            }
        }

        let layers_mutated = if self.show_layers {
            egui::Panel::left("layers_panel")
            .resizable(true)
            .default_size(208.0)
            .size_range(176.0..=300.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::bg_inspector_bot())
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading("Layers");
                    ui.label(format!("{}", self.editor.doc().objects.len()));
                    let can_group = self.editor.selection().len() >= 2;
                    if ui
                        .add_enabled(can_group, egui::Button::new("Group").small())
                        .on_hover_text("Group selection (Ctrl+G)")
                        .clicked()
                        && self.editor.group_selected()
                    {
                        self.touch();
                    }
                    if ui
                        .add_enabled(
                            self.editor.selection_grouped(),
                            egui::Button::new("Ungroup").small(),
                        )
                        .on_hover_text("Ungroup (Ctrl+Shift+G)")
                        .clicked()
                        && self.editor.ungroup_selected()
                    {
                        self.touch();
                    }
                    if self.editor.selection().len() >= 2 {
                        self.alignment_open =
                            ui.small_button("Align").clicked() || self.alignment_open;
                    }
                });
                ui.separator();
                let mut changed = false;
                let rows: Vec<_> = self
                    .editor
                    .doc()
                    .objects
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(index, o)| {
                        (
                            index,
                            o.id,
                            o.kind.label(),
                            o.visible,
                            o.locked,
                            o.group.is_some(),
                            o.id == self.editor.selected().unwrap_or(ObjectId(0)),
                        )
                    })
                    .collect();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (index, id, label, visible, locked, grouped, selected) in rows {
                        let response = ui
                            .horizontal(|ui| {
                                let eye = if visible { "◉" } else { "○" };
                                if ui
                                    .small_button(eye)
                                    .on_hover_text(if visible {
                                        "Hide layer"
                                    } else {
                                        "Show layer"
                                    })
                                    .clicked()
                                {
                                    changed |= self.editor.set_visible(id, !visible);
                                }
                                if ui
                                    .small_button(if locked { "🔒" } else { "🔓" })
                                    .on_hover_text(if locked {
                                        "Unlock layer"
                                    } else {
                                        "Lock layer"
                                    })
                                    .clicked()
                                {
                                    changed |= self.editor.set_locked(id, !locked);
                                }
                                let text = if grouped {
                                    format!("▦ {label}  #{}", id.0)
                                } else {
                                    format!("{label}  #{}", id.0)
                                };
                                if ui
                                    .selectable_label(selected, text)
                                    .on_hover_text(format!("Drag to reorder · index {}", index + 1))
                                    .clicked()
                                {
                                    self.editor.select(Some(id));
                                }
                            })
                            .response;
                        if response.drag_started() {
                            self.layer_drag_id = Some(id);
                        }
                        if response.dragged() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                        }
                        if response.drag_stopped() {
                            if let Some(drag_id) = self.layer_drag_id.take() {
                                if drag_id != id {
                                    changed |= self.editor.move_layer(drag_id, index);
                                }
                            }
                        }
                    }
                });
                changed
            })
            .inner
        } else {
            false
        };
        if layers_mutated {
            self.touch();
        }
        if self.alignment_open {
            let mut open = self.alignment_open;
            egui::Window::new("Align & Distribute")
                .frame(theme::modal_frame())
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(&ctx, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (label, a) in [
                            ("Left", Alignment::Left),
                            ("Center X", Alignment::HCenter),
                            ("Right", Alignment::Right),
                            ("Top", Alignment::Top),
                            ("Center Y", Alignment::VCenter),
                            ("Bottom", Alignment::Bottom),
                        ] {
                            if ui.button(label).clicked() && self.editor.align_selected(a) {
                                self.touch();
                            }
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        let enabled = self.editor.selection().len() >= 3;
                        ui.add_enabled_ui(enabled, |ui| {
                            if ui.button("Distribute horizontally").clicked()
                                && self.editor.distribute_selected(Axis::Horizontal)
                            {
                                self.touch();
                            }
                            if ui.button("Distribute vertically").clicked()
                                && self.editor.distribute_selected(Axis::Vertical)
                            {
                                self.touch();
                            }
                        });
                    });
                });
            self.alignment_open = open;
        }

        let panel_mutated = if self.show_inspector {
            egui::Panel::right("image_panel")
                .resizable(false)
                .exact_size(244.0)
                .frame(
                    egui::Frame::NONE
                        .fill(theme::bg_inspector_bot())
                        .inner_margin(egui::Margin::same(10)),
                )
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .show(ui, |ui| {
                            let changed = image_panel::render(ui, &mut self.editor);
                            ui.add_space(8.0);
                            if ui
                                .button("Flatten all…")
                                .on_hover_text("Bake every annotation into the image")
                                .clicked()
                            {
                                self.confirm_flatten = true;
                            }
                            changed
                        })
                        .inner
                })
                .inner
        } else {
            false
        };
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
                self.last_cursor_doc = ui
                    .input(|i| i.pointer.latest_pos())
                    .filter(|p| screen_rect.contains(*p))
                    .map(|p| self.view.screen_to_doc(p, screen_rect));

                if self.needs_initial_fit
                    && screen_rect.width() > 32.0
                    && screen_rect.height() > 32.0
                {
                    self.view
                        .fit_to(self.view.display_rect(), screen_rect, 40.0);
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
                        self.editor.pointer_down_mod(doc_p, shift_held);
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
                        self.editor.pointer_down_mod(doc_p, shift_held);
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
                let img_screen_rect = self
                    .view
                    .display_rect_to_screen(self.view.display_rect(), screen_rect);

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

                // 5b. Crop / Cutout preview
                if self.editor.tool() == Tool::Crop {
                    if let Some(crop) = self.editor.crop_rect() {
                        let crop_screen = self.view.doc_rect_to_screen(crop, screen_rect);
                        canvas::paint_crop_overlay(&mut painter, crop_screen, screen_rect);
                    }
                }
                if self.editor.tool() == Tool::Cutout {
                    if let Some(strip) = self.editor.cutout_rect() {
                        let mut s_rect = self.view.doc_rect_to_screen(strip, screen_rect);
                        match self.editor.style.cutout_axis {
                            qs_doc::CutoutAxis::Horizontal => {
                                s_rect.min.x = img_screen_rect.min.x;
                                s_rect.max.x = img_screen_rect.max.x;
                            }
                            qs_doc::CutoutAxis::Vertical => {
                                s_rect.min.y = img_screen_rect.min.y;
                                s_rect.max.y = img_screen_rect.max.y;
                            }
                        }
                        canvas::paint_cutout_overlay(&mut painter, s_rect, screen_rect);
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

                // 5d. Marquee preview while dragging on empty canvas
                if self.editor.tool() == Tool::Select {
                    if let Some(m) = self.editor.marquee_rect() {
                        if m.w >= 1.0 || m.h >= 1.0 {
                            let m_screen =
                                self.view.doc_rect_to_screen(m.normalized(), screen_rect);
                            canvas::paint_region(&mut painter, m_screen);
                        }
                    }
                }

                // 5e. Multi-selection bounding outline (no handles)
                if self.editor.selection().len() > 1 {
                    if let Some(b) = self.editor.selection_bounds() {
                        let b_screen = self.view.doc_rect_to_screen(b, screen_rect);
                        painter.rect_stroke(
                            b_screen,
                            0.0,
                            Stroke::new(1.0, accent_blue()),
                            StrokeKind::Outside,
                        );
                    }
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
        if self.unsaved {
            // A second edit inside the throttle interval must be recovered
            // even when no more input events wake this window.
            ctx.request_repaint_after(recovery::AUTOSAVE_INTERVAL);
        }
        self.sync_window_title(&ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_live_effects_finalizes_undo_before_recording_saved_revision() {
        let base = RgbaImage::from_pixel(20, 10, image::Rgba([30, 60, 90, 255]));
        let mut app = EditorApp {
            editor: Editor::new(Document::new(20, 10, 1.0)),
            base,
            doc_path: None,
            view: CanvasView::default(),
            texture: None,
            needs_render: true,
            unsaved: false,
            saved_rev: 0,
            close_confirmed: false,
            confirm_close: false,
            pending_recovery: None,
            autosave_at: None,
            window_title: None,
            needs_initial_fit: true,
            last_canvas_rect: None,
            saw_drag_press: false,
            show_layers: true,
            show_inspector: true,
            show_filmstrip: true,
            last_cursor_doc: None,
            filmstrip: Filmstrip::new(None),
            session_cache: std::collections::HashMap::new(),
            toast: None,
            canvas_dialog_open: false,
            confirm_flatten: false,
            canvas_insets: [0.0; 4],
            canvas_background: 0,
            resize_w: 20,
            resize_h: 10,
            resize_lock: true,
            export_dialog_open: false,
            export_format: ExportFormat::Png,
            export_quality: 90,
            combine_dialog_open: false,
            combine_layout: CombineLayout::SideBySide,
            tray_selected: Vec::new(),
            canvas_anchor: (1, 1),
            layer_drag_id: None,
            alignment_open: false,
            styles: styles::Library::default(),
        };
        app.editor.preview_effects(qs_doc::Effects {
            border: Some(qs_doc::BorderEffect::default()),
            ..Default::default()
        });
        let path =
            std::env::temp_dir().join(format!("qs_effect_save_{}.qsdoc", std::process::id()));
        app.save_to(&path).unwrap();
        let loaded = qsdoc::read(&path).unwrap();
        assert_eq!(loaded.doc.effects, app.editor.doc().effects);
        assert_eq!(
            qs_render::render(&loaded.doc, &loaded.base),
            qs_render::render(app.editor.doc(), &app.base)
        );
        app.editor.finish_effects();
        app.sync_unsaved();
        assert!(
            !app.unsaved,
            "finishing already-saved effects must not dirty the document"
        );
        assert!(app.editor.undo());
        assert_eq!(app.editor.doc().effects, qs_doc::Effects::default());
        assert!(!app.editor.undo());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn canvas_anchor_rebalances_added_space_without_changing_canvas_size() {
        assert_eq!(anchored_insets(24.0, 36.0, (0, 2)), [24.0, 36.0, 0.0, 0.0]);
        assert_eq!(
            anchored_insets(24.0, 36.0, (1, 1)),
            [12.0, 18.0, 12.0, 18.0]
        );
        assert_eq!(
            anchored_insets(-20.0, -30.0, (2, 0)),
            [0.0, 0.0, -20.0, -30.0]
        );
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
