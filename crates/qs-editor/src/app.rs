//! Main standalone editor application implementing `eframe::App`.

use crate::{
    canvas::CanvasView,
    clipboard,
    filmstrip::Filmstrip,
    fonts,
    recovery,
    state::{CloseDecision, Dialogs, DocumentState, ExportFormat, RenderState,
            SavedDocSession, ToolState, ViewportState},
    styles,
    templates::{self, CombineLayout},
    theme::{self},
    toolbar::ZoomCmd,
};
use eframe::egui::{
    self, ColorImage,
    TextureOptions, Vec2,
};
#[cfg(test)]
use eframe::egui::Modifiers;
use image::RgbaImage;
#[cfg(test)]
use qs_doc::ObjectId;
use qs_doc::{Document, RectF, qsdoc};
use qs_edit::Editor;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

/// The application facade: composes the sub-states from `state.rs` and
/// owns the window chrome (panels, dialogs, menus). Keyboard routing
/// lives in `input.rs`.
pub struct EditorApp {
    /// Document lifecycle: .qsdoc, dirty flag, sessions, recovery.
    pub(crate) doc: DocumentState,
    /// Zoom/pan mapping and filmstrip selection.
    pub(crate) viewport: ViewportState,
    /// Working state of the modal tool surfaces and pointer/text sessions.
    pub(crate) tools: ToolState,
    pub(crate) render_state: RenderState,
    pub(crate) dialogs: Dialogs,
    /// Title currently pushed to the viewport, to avoid re-sending it.
    pub(crate) window_title: Option<String>,
    pub(crate) toast: Option<(String, std::time::Instant)>,
    pub(crate) show_layers: bool,
    /// Last painted width of the resizable layers panel, so the open/close
    /// animation returns to the user's own width rather than the default.
    pub(crate) layers_w: f32,
    pub(crate) show_inspector: bool,
    pub(crate) filmstrip: Filmstrip,
    pub(crate) prefs: crate::prefs::AppPrefs,
    pub(crate) next_prefs_poll: Instant,
    /// Quick Styles: built-in presets + the user's saved ones.
    pub(crate) styles: styles::Library,
    /// OCR line hints for the Smart Move tool, region copy and Find.
    pub(crate) ocr_hints: crate::ocr_hint::OcrHints,
    /// Find in capture: window state, plus the focus we last panned to so the
    /// canvas is not re-centred every frame (which would fight panning).
    pub(crate) find: crate::find::FindState,
    pub(crate) last_find_focus: Option<(String, usize)>,
    /// Capture library browser (search, tags, favourites, pruning).
    pub(crate) library: crate::library::Library,
}


impl EditorApp {
    pub fn new(
        doc: Document,
        base: RgbaImage,
        doc_path: Option<PathBuf>,
        cc: &eframe::CreationContext<'_>,
    ) -> Self {
        fonts::install(&cc.egui_ctx);
        let prefs = crate::prefs::AppPrefs::load();
        prefs.apply_live();
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
        let show_layers = doc.objects.len() >= 2;
        let filmstrip = Filmstrip::new(doc_path.as_deref());
        let mut app = Self {
            doc: DocumentState {
                editor: Editor::new(doc),
                base,
                path: doc_path,
                unsaved: false,
                saved_rev: 0,
                session_cache: std::collections::HashMap::new(),
                autosave_busy: Arc::new(AtomicBool::new(false)),
                autosave_at: None,
                pre_flatten: None,
            },
            viewport: ViewportState {
                view,
                needs_initial_fit: true,
                last_canvas_rect: None,
                fit_memo: None,
                last_cursor_doc: None,
                show_filmstrip: false,
                filmstrip_focus: None,
                tray_selected: Vec::new(),
            },
            tools: ToolState {
                canvas_insets: [0.0; 4],
                canvas_anchor: (1, 1),
                canvas_background,
                resize_w: init_w,
                resize_h: init_h,
                resize_lock: true,
                export_format: ExportFormat::Png,
                export_quality: 90,
                combine_layout: CombineLayout::SideBySide,
                style_filter: String::new(),
                saw_drag_press: false,
                ime_preedit: String::new(),
            },
            render_state: RenderState {
                needs_render: true,
                ..Default::default()
            },
            dialogs: Dialogs {
                pending_recovery: pending,
                ..Default::default()
            },
            window_title: None,
            toast: None,
            show_layers,
            layers_w: 236.0,
            show_inspector: true,
            filmstrip,
            prefs,
            next_prefs_poll: Instant::now(),
            styles: styles::Library::load(),
            ocr_hints: crate::ocr_hint::OcrHints::default(),
            find: crate::find::FindState::default(),
            last_find_focus: None,
            library: crate::library::Library::default(),
        };
        app.update_texture(&cc.egui_ctx);
        app
    }

    /// Loads the recovery copy of `doc_path` when it is newer than the file on
    /// disk. The copy itself stays put until the user answers, so a crash
    /// inside the prompt loses nothing.
    pub(crate) fn pending_recovery(doc_path: Option<&Path>) -> Option<(Document, RgbaImage)> {
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
    pub(crate) fn apply_document(&mut self, doc: Document, base: RgbaImage) {
        let canvas_background = doc.background;
        let base_rect = RectF::new(0.0, 0.0, doc.base_size.w, doc.base_size.h);
        self.viewport.view = CanvasView::default();
        self.viewport.view.sync(&doc);
        self.viewport.view.center = self.viewport.view.map.to_display(base_rect.center());
        self.viewport.view.zoom = (700.0 / doc.base_size.w.max(1.0))
            .min(500.0 / doc.base_size.h.max(1.0))
            .clamp(0.2, 1.0);
        self.doc.base = base;
        self.render_state.render_cache.invalidate_base();
        self.doc.editor = Editor::new(doc);
        self.doc.pre_flatten = None;
        // A different document can share the previous one's size, scale and
        // view; the hint cache is keyed on those, so it has to be dropped
        // explicitly rather than waiting for a key mismatch.
        self.ocr_hints.clear();
        self.tools.canvas_background = canvas_background;
        self.tools.canvas_insets = [0.0; 4];
        self.tools.canvas_anchor = (1, 1);
        self.render_state.texture = None;
        self.render_state.needs_render = true;
        self.viewport.needs_initial_fit = true;
    }

    /// Asks what to do with edits recovered from a crashed session.
    pub(crate) fn recovery_guard(&mut self, ctx: &egui::Context) {
        let Some((doc, base)) = self.dialogs.pending_recovery.take() else {
            return;
        };
        let mut choice = None;
        crate::components::modal_window(
            ctx,
            "recovery_modal",
            "Recover unsaved changes?",
            340.0,
            |ui| {
                ui.label(
                    egui::RichText::new(
                        "The last session ended with edits that were never saved.",
                    )
                    .size(qs_theme::font::SMALL as f32 + 0.5)
                    .color(theme::text_secondary()),
                );
                ui.add_space(qs_theme::space::LG as f32);
                crate::components::dialog_actions(ui, |ui| {
                    if crate::components::pill_button_primary(
                        ui,
                        "Recover",
                        "Restore the recovered edits",
                    ) {
                        choice = Some(true);
                    }
                    ui.add_space(qs_theme::space::XS as f32);
                    if crate::components::pill_button_colored(
                        ui,
                        "Discard",
                        "Delete the recovered copy",
                        theme::danger_red(),
                    ) {
                        choice = Some(false);
                    }
                });
            },
        );

        match choice {
            Some(true) => {
                self.apply_document(doc, base);
                self.doc.editor.mark_modified();
                self.sync_unsaved();
                self.show_toast("Recovered unsaved changes");
            }
            Some(false) => {}
            // still open: keep the copy in hand for the next frame
            None => self.dialogs.pending_recovery = Some((doc, base)),
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
    /// Build a combined sheet from the marked captures and open it as the
    /// current document. Split out of the dialog so the flow is reachable
    /// without a frame.
    pub fn create_combined_sheet(&mut self) -> anyhow::Result<()> {
        let (doc, base) = templates::build_combined_document(
            &self.viewport.tray_selected,
            self.tools.combine_layout,
            self.doc.editor.doc().background,
        )?;
        self.doc.path = None;
        self.apply_document(doc, base);
        self.doc.editor.mark_modified();
        self.touch();
        // The sheet is a new unsaved document; state it here instead of waiting
        // for the next frame's sync.
        self.sync_unsaved();
        self.viewport.tray_selected.clear();
        self.dialogs.combine_dialog_open = false;
        Ok(())
    }

    /// Mark captures for Combine, for tests.
    #[doc(hidden)]
    pub fn set_tray_selection(&mut self, paths: Vec<std::path::PathBuf>) {
        self.viewport.tray_selected = paths;
    }

    /// True when the current document has unsaved edits, for tests.
    #[doc(hidden)]
    pub fn has_unsaved_edits(&self) -> bool {
        self.doc.unsaved
    }

    /// Current document, for tests.
    #[doc(hidden)]
    pub fn document(&self) -> &Document {
        self.doc.editor.doc()
    }

    /// Blank editor for tests: same field set as the internal test fixture,
    /// without needing an eframe `CreationContext`.
    #[doc(hidden)]
    pub fn for_tests(doc: Document, base: RgbaImage) -> Self {
        let w = doc.base_size.w.max(1.0) as u32;
        let h = doc.base_size.h.max(1.0) as u32;
Self {
            doc: DocumentState {
                editor: Editor::new(doc),
                base,
                path: None,
                unsaved: false,
                saved_rev: 0,
                session_cache: std::collections::HashMap::new(),
                autosave_busy: Arc::new(AtomicBool::new(false)),
                autosave_at: None,
                pre_flatten: None,
            },
            viewport: ViewportState {
                view: CanvasView::default(),
                needs_initial_fit: true,
                last_canvas_rect: None,
                fit_memo: None,
                last_cursor_doc: None,
                show_filmstrip: false,
                filmstrip_focus: None,
                tray_selected: Vec::new(),
            },
            tools: ToolState {
                canvas_insets: [0.0; 4],
                canvas_anchor: (1, 1),
                canvas_background: 0,
                resize_w: w,
                resize_h: h,
                resize_lock: true,
                export_format: ExportFormat::Png,
                export_quality: 90,
                combine_layout: CombineLayout::SideBySide,
                style_filter: String::new(),
                saw_drag_press: false,
                ime_preedit: String::new(),
            },
            render_state: RenderState {
                needs_render: true,
                ..Default::default()
            },
            dialogs: Dialogs::default(),
            window_title: None,
            toast: None,
            show_layers: true,
            layers_w: 236.0,
            show_inspector: true,
            filmstrip: Filmstrip::new(None),
            prefs: crate::prefs::AppPrefs::default(),
            next_prefs_poll: Instant::now(),
            styles: styles::Library::default(),
            ocr_hints: crate::ocr_hint::OcrHints::default(),
            find: crate::find::FindState::default(),
            last_find_focus: None,
            library: crate::library::Library::default(),
        }
    }

    pub fn load_path(&mut self, path: &Path) -> anyhow::Result<()> {
        let (doc, base, doc_path) = load_document(path)?;
        self.apply_document(doc, base);
        self.doc.path = Some(doc_path);
        self.mark_saved();
        self.dialogs.pending_recovery = Self::pending_recovery(self.doc.path.as_deref());
        let name = self.display_name().to_string();
        self.show_toast(format!("Opened {name}"));
        Ok(())
    }

    /// Switch to another capture/document, preserving the current one in the
    /// session cache (zoom, pan, unsaved state, and undo/redo history).
    pub fn switch_to_path(&mut self, path: &Path) -> anyhow::Result<()> {
        if self.doc.path.as_deref() == Some(path) {
            return Ok(());
        }
        if let Some(cur_path) = self.doc.path.clone() {
            let blank_doc = Document::new(1, 1, 1.0);
            let old_editor = std::mem::replace(&mut self.doc.editor, Editor::new(blank_doc));
            let old_base = std::mem::replace(
                &mut self.doc.base,
                RgbaImage::new(1, 1),
            );
            let old_view = std::mem::take(&mut self.viewport.view);
            self.doc.session_cache.insert(
                cur_path.clone(),
                SavedDocSession {
                    editor: old_editor,
                    base: old_base,
                    doc_path: Some(cur_path),
                    view: old_view,
                    unsaved: self.doc.unsaved,
                    saved_rev: self.doc.saved_rev,
                },
            );
        }

        if let Some(cached) = self.doc.session_cache.remove(path) {
            self.doc.editor = cached.editor;
            self.doc.base = cached.base;
            self.render_state.render_cache.invalidate_base();
            self.doc.path = cached.doc_path;
            self.viewport.view = cached.view;
            self.doc.unsaved = cached.unsaved;
            self.doc.saved_rev = cached.saved_rev;
            self.render_state.needs_render = true;
            self.viewport.needs_initial_fit = false;
            self.dialogs.pending_recovery = None;
            let name = self.display_name().to_string();
            self.show_toast(format!("Switched to {name}"));
            return Ok(());
        }

        self.load_path(path)
    }

    /// File name to show in titles, dialogs and toasts.
    pub(crate) fn display_name(&self) -> &str {
        self.doc.path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled")
    }

    pub(crate) fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), std::time::Instant::now()));
    }

    pub(crate) fn checker_texture_id(&mut self, ctx: &egui::Context) -> egui::TextureId {
        let dark = qs_theme::pal().dark;
        if let Some((cached_dark, tex)) = &self.render_state.checker_tex {
            if *cached_dark == dark {
                return tex.id();
            }
        }
        let c1 = theme::bg_toolbar_bot();
        let c2 = theme::bg_btn_secondary_top();
        let img = ColorImage {
            size: [2, 2],
            source_size: Vec2::new(2.0, 2.0),
            pixels: vec![c1, c2, c2, c1],
        };
        let tex = ctx.load_texture("editor_checkerboard", img, TextureOptions::NEAREST_REPEAT);
        let id = tex.id();
        self.render_state.checker_tex = Some((dark, tex));
        id
    }

    pub(crate) fn update_texture(&mut self, ctx: &egui::Context) {
        let flat = self.render_state.render_cache.render(self.doc.editor.doc(), &self.doc.base);
        let size = [flat.width() as usize, flat.height() as usize];
        let color_img = ColorImage::from_rgba_unmultiplied(size, flat.as_raw());
        if let Some(tex) = &mut self.render_state.texture {
            tex.set(color_img, TextureOptions::LINEAR);
        } else {
            self.render_state.texture =
                Some(ctx.load_texture("rendered_doc", color_img, TextureOptions::LINEAR));
        }
        self.render_state.needs_render = false;
    }

    /// Saves to the current path, asking for one when the document has never
    /// been written. A cancelled dialog leaves `unsaved` set.
    pub fn save(&mut self) -> anyhow::Result<()> {
        match self.doc.path.clone() {
            Some(p) => self.save_to(&p),
            None => self.save_as(),
        }
    }

    /// Asks for a destination, then saves there.
    pub fn save_as(&mut self) -> anyhow::Result<()> {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Quickshort document", &[qsdoc::EXT])
            .set_file_name(self.display_name());
        if let Some(dir) = self.doc.path.as_ref().and_then(|p| p.parent()) {
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
        let previous_slot = recovery::slot_path(self.doc.path.as_deref());
        self.save_to(&path)?;
        // an untitled or renamed document leaves its old recovery copy behind
        if let Some(old) = previous_slot {
            if Some(old.clone()) != recovery::slot_path(self.doc.path.as_deref()) {
                recovery::clear(&old);
            }
        }
        Ok(())
    }

    pub(crate) fn save_to(&mut self, path: &Path) -> anyhow::Result<()> {
        // Finalize pending gestures before remembering the saved revision.
        self.doc.editor.finish_effects();
        self.doc.editor.end_text_edit();
        let flat = qs_render::render(self.doc.editor.doc(), &self.doc.base);
        let thumb = qsdoc::make_thumb(&flat);
        qsdoc::write(path, self.doc.editor.doc(), &self.doc.base, Some(&thumb))?;
        self.doc.path = Some(path.to_path_buf());
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


    /// The rendered texture is stale after an interaction; whether the
    /// document is actually dirty is decided by [`EditorApp::sync_unsaved`].
    pub(crate) fn touch(&mut self) {
        self.render_state.needs_render = true;
    }

    /// Derives the unsaved flag from the editor's revision counter, so a click
    /// that changes nothing never marks the document dirty.
    pub(crate) fn sync_unsaved(&mut self) {
        self.doc.unsaved = self.doc.editor.revision() != self.doc.saved_rev;
    }

    /// Remembers the current state as the one on disk.
    pub(crate) fn mark_saved(&mut self) {
        self.doc.saved_rev = self.doc.editor.revision();
        self.doc.unsaved = false;
    }

    pub(crate) fn clear_autosave(&mut self) {
        if let Some(slot) = recovery::slot_path(self.doc.path.as_deref()) {
            recovery::clear(&slot);
        }
    }

    /// Refreshes the recovery copy, at most once per
    /// [`recovery::AUTOSAVE_INTERVAL`], on a background thread so PNG encoding
    /// and fsync never stall the UI thread.
    pub(crate) fn autosave_tick(&mut self) {
        if !self.doc.unsaved {
            return;
        }
        let now = std::time::Instant::now();
        if self
            .doc.autosave_at
            .is_some_and(|last| now.duration_since(last) < recovery::AUTOSAVE_INTERVAL)
        {
            return;
        }
        if self
            .doc.autosave_busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        self.doc.autosave_at = Some(now);
        let Some(slot) = recovery::slot_path(self.doc.path.as_deref()) else {
            self.doc.autosave_busy.store(false, Ordering::Release);
            return;
        };
        let doc = self.doc.editor.doc().clone();
        let base = self.doc.base.clone();
        let busy = Arc::clone(&self.doc.autosave_busy);
        std::thread::spawn(move || {
            let _ = qsdoc::write(&slot, &doc, &base, None);
            busy.store(false, Ordering::Release);
        });
    }

    /// Keeps the window title in step with the file name and the unsaved mark.
    pub(crate) fn sync_window_title(&mut self, ctx: &egui::Context) {
        let mark = if self.doc.unsaved { "\u{2022} " } else { "" };
        let title = format!("{mark}{} — Quickshort", self.display_name());
        if self.window_title.as_deref() != Some(title.as_str()) {
            self.window_title = Some(title.clone());
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }

    /// Intercepts a close request while edits are pending and runs the
    /// Save / Discard / Cancel dialog.
    pub(crate) fn close_guard(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.doc.unsaved && !self.dialogs.close_confirmed {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.dialogs.confirm_close = true;
        }
        if !self.dialogs.confirm_close {
            return;
        }

        let mut decision = None;
        let doc_name = self.display_name().to_owned();
        crate::components::modal_window(
            ctx,
            "unsaved_changes_modal",
            "Save changes before closing?",
            340.0,
            |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "“{doc_name}” has edits that haven’t been saved yet."
                    ))
                    .size(qs_theme::font::SMALL as f32 + 0.5)
                    .color(theme::text_secondary()),
                );
                ui.add_space(qs_theme::space::LG as f32);
                crate::components::dialog_actions(ui, |ui| {
                    if crate::components::pill_button_primary(ui, "Save", "Save and close") {
                        decision = Some(CloseDecision::Save);
                    }
                    ui.add_space(qs_theme::space::XS as f32);
                    if crate::components::pill_button_colored(
                        ui,
                        "Discard",
                        "Close without saving",
                        theme::danger_red(),
                    ) {
                        decision = Some(CloseDecision::Discard);
                    }
                    ui.add_space(qs_theme::space::XS as f32);
                    if crate::components::pill_button(ui, "Cancel", "Keep editing") {
                        decision = Some(CloseDecision::Cancel);
                    }
                });
            },
        );

        if decision.is_none()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        {
            decision = Some(CloseDecision::Save);
        }

        match decision {
            // a cancelled Save-As leaves the document unsaved: keep the window
            Some(CloseDecision::Save) => {
                if self.save().is_ok() && !self.doc.unsaved {
                    self.finish_close(ctx);
                }
            }
            Some(CloseDecision::Discard) => {
                self.clear_autosave();
                self.mark_saved();
                self.finish_close(ctx);
            }
            Some(CloseDecision::Cancel) => self.dialogs.confirm_close = false,
            None => {}
        }
    }

    pub(crate) fn finish_close(&mut self, ctx: &egui::Context) {
        self.dialogs.confirm_close = false;
        self.dialogs.close_confirmed = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    pub fn export_png(&mut self, path: Option<&Path>) -> anyhow::Result<()> {
        let flat = qs_render::render(self.doc.editor.doc(), &self.doc.base);
        let out_path = path
            .map(PathBuf::from)
            .or_else(|| self.doc.path.as_ref().map(|p| p.with_extension("png")))
            .unwrap_or_else(|| PathBuf::from("export.png"));
        flat.save(&out_path)?;
        let name = out_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("export.png");
        self.show_toast(format!("Exported {name}"));
        Ok(())
    }

    pub(crate) fn export_with_dialog(&mut self, fmt: ExportFormat, quality: u8) -> anyhow::Result<()> {
        let default_name = self
            .doc.path
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
        let flat = qs_render::render(self.doc.editor.doc(), &self.doc.base);
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
        self.tools.export_format = fmt;
        Ok(())
    }

    pub(crate) fn print_image(&mut self) -> anyhow::Result<()> {
        let flat = qs_render::render(self.doc.editor.doc(), &self.doc.base);
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


    pub(crate) fn refresh_preferences(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if now >= self.next_prefs_poll {
            self.next_prefs_poll = now + Duration::from_millis(500);
            if let Ok(prefs) = crate::prefs::AppPrefs::read() {
                if prefs.theme != self.prefs.theme || prefs.accent != self.prefs.accent {
                    prefs.apply_live();
                    theme::sync_egui_visuals(ctx);
                }
                self.prefs = prefs;
            }
        }
        ctx.request_repaint_after(self.next_prefs_poll.saturating_duration_since(now));
    }




    /// Copy the recognized text inside the selection region.
    ///
    /// Every outcome says something: an empty clipboard is indistinguishable
    /// from a broken command, so "nothing recognized here" and "the engine is
    /// unavailable" stay distinct messages.
    pub(crate) fn copy_text_in_region(&mut self) {
        let Some(region) = self.doc.editor.region() else {
            self.show_toast("Select a region first");
            return;
        };
        if self.ocr_hints.is_scanning() {
            self.show_toast("Still reading text — try again in a moment");
            return;
        }
        if let Some(err) = self.ocr_hints.error().map(str::to_string) {
            self.show_toast(format!("OCR unavailable: {err}"));
            return;
        }
        let text = self.ocr_hints.text_in(region);
        if text.is_empty() {
            let why = if self.ocr_hints.is_empty() {
                "No text was recognized in this capture"
            } else {
                "The selection contains no recognized text"
            };
            self.show_toast(why);
            return;
        }
        match crate::clipboard::copy_text(&text) {
            Ok(()) => {
                let lines = text.lines().count();
                self.show_toast(format!(
                    "Copied {lines} line{}",
                    if lines == 1 { "" } else { "s" }
                ));
            }
            Err(e) => self.show_toast(format!("Copy failed: {e}")),
        }
    }





    pub fn copy_to_clipboard(&mut self) -> anyhow::Result<()> {
        let region = self.doc.editor.doc().region;
        let (flat, what) = match region.and_then(|r| {
            qs_render::render_region(self.doc.editor.doc(), &self.doc.base, r)
        }) {
            Some(cropped) => (cropped, "selection"),
            None => (qs_render::render(self.doc.editor.doc(), &self.doc.base), "image"),
        };
        clipboard::copy_image(&flat)?;
        self.show_toast(format!("Copied {what} to clipboard"));
        Ok(())
    }

    /// Bake every annotation into the image and start a fresh document over
    /// the result. Destructive: the annotations and the undo history are gone.
    pub fn flatten_all(&mut self) -> anyhow::Result<()> {
        let flat = qs_render::render(self.doc.editor.doc(), &self.doc.base);
        let old = self.doc.editor.doc().clone();
        let mut doc = Document::new(flat.width(), flat.height(), old.scale);
        doc.meta = old.meta.clone();
        doc.background = old.background;
        let before = (old, self.doc.base.clone());
        self.apply_document(doc, flat);
        self.doc.pre_flatten = Some(before);
        self.doc.editor.mark_modified();
        self.touch();
        self.show_toast("Flattened into the image \u{00b7} Ctrl+Z to undo");
        Ok(())
    }

    pub(crate) fn apply_zoom_cmd(&mut self, cmd: ZoomCmd) {
        let Some(rect) = self.viewport.last_canvas_rect else {
            return;
        };
        let center = rect.center();
        match cmd {
            ZoomCmd::None => {}
            ZoomCmd::In => self.viewport.view.zoom_at(1.25, center, rect),
            ZoomCmd::Out => self.viewport.view.zoom_at(0.8, center, rect),
            ZoomCmd::Fit => {
                self.viewport.view.sync(self.doc.editor.doc());
                self.viewport.view.fit_to(self.viewport.view.display_rect(), rect, 40.0);
                self.viewport.fit_memo = Some(crate::canvas::FitMemo::of(&self.viewport.view, rect.size(), 1.5));
            }
            ZoomCmd::Set(target) => {
                let factor = target / self.viewport.view.zoom.max(0.01);
                self.viewport.view.zoom_at(factor, center, rect);
            }
        }
    }

    /// Close the window, asking first when edits are pending.
    ///
    /// The red button and cmd+W both land here, so there is one rule for
    /// throwing edits away.
    pub fn request_close(&mut self, ctx: &egui::Context) {
        if self.doc.unsaved && !self.dialogs.close_confirmed {
            self.dialogs.confirm_close = true;
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// True while the "unsaved changes" dialog is up, for tests.
    #[doc(hidden)]
    pub fn is_confirming_close(&self) -> bool {
        self.dialogs.confirm_close
    }

    /// Which panels are open: canvas, export menu, align.
    #[doc(hidden)]
    pub fn open_dialogs(&self) -> (bool, bool, bool) {
        (
            self.dialogs.canvas_dialog_open,
            self.dialogs.export_menu_open,
            self.dialogs.alignment_open,
        )
    }

    /// Filmstrip visibility, for tests.
    #[doc(hidden)]
    pub fn filmstrip_visible(&self) -> bool {
        self.viewport.show_filmstrip
    }

}


impl eframe::App for EditorApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::bg_workspace_top().to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render_ui(ui);
    }
}

impl EditorApp {
}



pub(crate) fn load_document(p: &Path) -> anyhow::Result<(Document, RgbaImage, PathBuf)> {
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


/// Runs the sibling `quickshort` binary with `arg` (`--capture`,
/// `--settings`), so the editor reaches Capture and the one Settings window
/// without a trip to the tray.
pub(crate) fn spawn_app(arg: &str) {
    let name = if cfg!(windows) {
        "quickshort.exe"
    } else {
        "quickshort"
    };
    let exe = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.join(name)))
        .filter(|p| p.exists())
        .unwrap_or_else(|| name.into());
    if let Err(e) = std::process::Command::new(&exe).arg(arg).spawn() {
        eprintln!("{arg}: failed to launch {}: {e}", exe.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::anchored_insets;
    use eframe::egui::{Key, Pos2, Rect};
    use qs_doc::PointF;
    use qs_edit::Tool;

    #[test]
    fn saving_live_effects_finalizes_undo_before_recording_saved_revision() {
        let base = RgbaImage::from_pixel(20, 10, image::Rgba([30, 60, 90, 255]));
        let mut app = EditorApp::for_tests(Document::new(20, 10, 1.0), base);
        app.doc.editor.preview_effects(qs_doc::Effects {
            border: Some(qs_doc::BorderEffect::default()),
            ..Default::default()
        });
        let path =
            std::env::temp_dir().join(format!("qs_effect_save_{}.qsdoc", std::process::id()));
        app.save_to(&path).unwrap();
        let loaded = qsdoc::read(&path).unwrap();
        assert_eq!(loaded.doc.effects, app.doc.editor.doc().effects);
        assert_eq!(
            qs_render::render(&loaded.doc, &loaded.base),
            qs_render::render(app.doc.editor.doc(), &app.doc.base)
        );
        app.doc.editor.finish_effects();
        app.sync_unsaved();
        assert!(
            !app.doc.unsaved,
            "finishing already-saved effects must not dirty the document"
        );
        assert!(app.doc.editor.undo());
        assert_eq!(app.doc.editor.doc().effects, qs_doc::Effects::default());
        assert!(!app.doc.editor.undo());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn editor_drag_uses_the_original_press_and_destination() {
        let ctx = egui::Context::default();
        let mut doc = Document::new(640, 400, 1.0);
        for x in [10.0, 40.0, 70.0] {
            doc.push(qs_doc::ObjectKind::Rect {
                r: RectF::new(x, 10.0, 20.0, 20.0),
                stroke: qs_doc::Stroke::new(0, 2.0),
                fill: qs_doc::Fill::None,
                radius: 0.0,
            });
        }
        let mut app = EditorApp::for_tests(doc, RgbaImage::new(640, 400));
        app.next_prefs_poll = Instant::now() + Duration::from_secs(60);
        let frame = |app: &mut EditorApp, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1180.0, 720.0))),
                    events,
                    ..Default::default()
                },
                |ui| app.render_ui(ui),
            );
            output.textures_delta.clear();
            output
        };
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: Modifiers::NONE,
        };
        let drag = |app: &mut EditorApp, from: Pos2, threshold: Pos2, to: Pos2| {
            frame(app, vec![egui::Event::PointerMoved(from), button(from, true)]);
            frame(app, vec![egui::Event::PointerMoved(threshold)]);
            frame(app, vec![egui::Event::PointerMoved(to), button(to, false)]);
        };

        frame(&mut app, vec![]);
        let output = frame(&mut app, vec![]);
        let row_label = |name: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == name => {
                        Some(text.pos + Vec2::new(5.0, text.galley.size().y / 2.0))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("layer row missing: {name}"))
        };
        let first = row_label("Rectangle #3");
        let last = row_label("Rectangle #1");
        drag(&mut app, first, first.lerp(last, 0.5), last);
        assert_eq!(app.document().objects[0].id, ObjectId(3));

        app.doc.editor.set_tool(Tool::Rect);
        frame(&mut app, vec![]);
        let canvas = app.viewport.last_canvas_rect.unwrap();
        let start = app.viewport.view.doc_to_screen(PointF::new(200.0, 100.0), canvas);
        let end = app.viewport.view.doc_to_screen(PointF::new(310.0, 180.0), canvas);
        drag(&mut app, start, start.lerp(end, 0.7), end);

        let qs_doc::ObjectKind::Rect { r, .. } = app.document().objects.last().unwrap().kind else {
            panic!("expected a rectangle");
        };
        for (actual, expected) in [(r.x, 200.0), (r.y, 100.0), (r.w, 110.0), (r.h, 80.0)] {
            assert!((actual - expected).abs() < 0.1, "{r:?}");
        }
    }

    #[test]
    fn step_tool_places_a_number_on_each_rapid_canvas_click() {
        let ctx = egui::Context::default();
        let doc = Document::new(640, 400, 1.0);
        let mut app = EditorApp::for_tests(doc, RgbaImage::new(640, 400));
        app.next_prefs_poll = Instant::now() + Duration::from_secs(60);
        let frame = |app: &mut EditorApp, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1180.0, 720.0))),
                    events,
                    ..Default::default()
                },
                |ui| app.render_ui(ui),
            );
            output.textures_delta.clear();
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        frame(&mut app, vec![key(Key::S)]);
        assert_eq!(app.doc.editor.tool(), Tool::Step);
        let canvas = app.viewport.last_canvas_rect.unwrap();
        let pos = app.viewport.view.doc_to_screen(PointF::new(200.0, 100.0), canvas);
        frame(&mut app, vec![egui::Event::PointerMoved(pos)]);
        for number in 1..=2 {
            for pressed in [true, false] {
                frame(
                    &mut app,
                    vec![egui::Event::PointerButton {
                        pos,
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: Modifiers::NONE,
                    }],
                );
            }
            assert_eq!(app.doc.editor.tool(), Tool::Step);
            assert_eq!(app.document().objects.len(), number as usize);
            let id = app.document().objects.last().unwrap().id;
            assert_eq!(app.document().step_number(id), Some(number));
        }
        frame(&mut app, vec![key(Key::V)]);
        assert_eq!(app.doc.editor.tool(), Tool::Select);
        frame(&mut app, vec![key(Key::R)]);
        assert_eq!(app.doc.editor.tool(), Tool::Rect);
        frame(&mut app, vec![key(Key::Escape)]);
        assert_eq!(app.doc.editor.tool(), Tool::Select);
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
