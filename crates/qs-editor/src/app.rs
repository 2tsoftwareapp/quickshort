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
    fit_memo: Option<crate::canvas::FitMemo>,
    saw_drag_press: bool,
    toast: Option<(String, std::time::Instant)>,
    canvas_dialog_open: bool,
    /// Document and base image before "Flatten all", so undo can bring
    /// the layers back.
    pre_flatten: Option<(Document, RgbaImage)>,
    canvas_insets: [f32; 4],
    canvas_background: u32,
    resize_w: u32,
    resize_h: u32,
    resize_lock: bool,
    export_menu_open: bool,
    export_anchor: Option<Rect>,
    export_format: ExportFormat,
    export_quality: u8,
    combine_dialog_open: bool,
    combine_layout: CombineLayout,
    tray_selected: Vec<PathBuf>,
    show_layers: bool,
    show_inspector: bool,
    show_filmstrip: bool,
    /// Keyboard cursor over the filmstrip (arrows move it, Space marks).
    filmstrip_focus: Option<usize>,
    last_cursor_doc: Option<qs_doc::PointF>,
    filmstrip: Filmstrip,
    session_cache: std::collections::HashMap<PathBuf, SavedDocSession>,
    canvas_anchor: (usize, usize),
    layer_drag_id: Option<ObjectId>,
    alignment_open: bool,
    prefs: crate::prefs::AppPrefs,
    /// Quick Styles: built-in presets + the user's saved ones.
    styles: styles::Library,
    /// Text filter for the quick-style bar (session state).
    style_filter: String,
    /// OCR line hints for the Smart Move tool, region copy and Find.
    ocr_hints: crate::ocr_hint::OcrHints,
    /// Find in capture: window state, plus the focus we last panned to so the
    /// canvas is not re-centred every frame (which would fight panning).
    find: crate::find::FindState,
    last_find_focus: Option<(String, usize)>,
    /// Capture library browser (search, tags, favourites, pruning).
    library: crate::library::Library,
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
            fit_memo: None,
            saw_drag_press: false,
            toast: None,
            canvas_dialog_open: false,
            pre_flatten: None,
            canvas_insets: [0.0; 4],
            canvas_background,
            resize_w: init_w,
            resize_h: init_h,
            resize_lock: true,
            export_menu_open: false,
            export_anchor: None,
            export_format: ExportFormat::Png,
            export_quality: 90,
            combine_dialog_open: false,
            combine_layout: CombineLayout::SideBySide,
            tray_selected: Vec::new(),
            canvas_anchor: (1, 1),
            show_layers: true,
            show_inspector: true,
            show_filmstrip: false,
            filmstrip_focus: None,
            last_cursor_doc: None,
            filmstrip,
            session_cache: std::collections::HashMap::new(),
            layer_drag_id: None,
            alignment_open: false,
            prefs: crate::prefs::AppPrefs::load(),
            styles: styles::Library::load(),
            library: crate::library::Library::default(),
            style_filter: String::new(),
            ocr_hints: crate::ocr_hint::OcrHints::default(),
            find: crate::find::FindState::default(),
            last_find_focus: None,
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
        self.pre_flatten = None;
        // A different document can share the previous one's size, scale and
        // view; the hint cache is keyed on those, so it has to be dropped
        // explicitly rather than waiting for a key mismatch.
        self.ocr_hints.clear();
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
    /// Build a combined sheet from the marked captures and open it as the
    /// current document. Split out of the dialog so the flow is reachable
    /// without a frame.
    pub fn create_combined_sheet(&mut self) -> anyhow::Result<()> {
        let (doc, base) = templates::build_combined_document(
            &self.tray_selected,
            self.combine_layout,
            self.editor.doc().background,
        )?;
        self.doc_path = None;
        self.apply_document(doc, base);
        self.editor.mark_modified();
        self.touch();
        // The sheet is a new unsaved document; state it here instead of waiting
        // for the next frame's sync.
        self.sync_unsaved();
        self.tray_selected.clear();
        self.combine_dialog_open = false;
        Ok(())
    }

    /// Mark captures for Combine, for tests.
    #[doc(hidden)]
    pub fn set_tray_selection(&mut self, paths: Vec<std::path::PathBuf>) {
        self.tray_selected = paths;
    }

    /// True when the current document has unsaved edits, for tests.
    #[doc(hidden)]
    pub fn has_unsaved_edits(&self) -> bool {
        self.unsaved
    }

    /// Current document, for tests.
    #[doc(hidden)]
    pub fn document(&self) -> &Document {
        self.editor.doc()
    }

    /// Blank editor for tests: same field set as the internal test fixture,
    /// without needing an eframe `CreationContext`.
    #[doc(hidden)]
    pub fn for_tests(doc: Document, base: RgbaImage) -> Self {
        let w = doc.base_size.w.max(1.0) as u32;
        let h = doc.base_size.h.max(1.0) as u32;
        Self {
            editor: Editor::new(doc),
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
            fit_memo: None,
            saw_drag_press: false,
            show_layers: true,
            show_inspector: true,
            show_filmstrip: false,
            filmstrip_focus: None,
            last_cursor_doc: None,
            filmstrip: Filmstrip::new(None),
            session_cache: std::collections::HashMap::new(),
            toast: None,
            canvas_dialog_open: false,
            pre_flatten: None,
            canvas_insets: [0.0; 4],
            canvas_background: 0,
            resize_w: w,
            resize_h: h,
            resize_lock: true,
            export_menu_open: false,
            export_anchor: None,
            export_format: ExportFormat::Png,
            export_quality: 90,
            combine_dialog_open: false,
            combine_layout: CombineLayout::SideBySide,
            tray_selected: Vec::new(),
            canvas_anchor: (1, 1),
            layer_drag_id: None,
            alignment_open: false,
            prefs: crate::prefs::AppPrefs::default(),
            styles: styles::Library::default(),
            library: crate::library::Library::default(),
            style_filter: String::new(),
            ocr_hints: crate::ocr_hint::OcrHints::default(),
            find: crate::find::FindState::default(),
            last_find_focus: None,
        }
    }

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
            // Fixed body height + resizable: without it the window grows to the
            // content and then clips against the screen, with no scrollbar.
            .default_size(egui::vec2(
                360.0,
                (ctx.content_rect().height() - 120.0).clamp(320.0, 560.0),
            ))
            .resizable(true)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height())
                    .show(ui, |ui| {
                ui.label("Canvas insets (px)");
                egui::Grid::new("canvas_insets")
                    .num_columns(2)
                    .spacing([
                        qs_theme::space::SM as f32,
                        qs_theme::space::XS as f32,
                    ])
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
                // Drawn grid: the arrow glyphs (↖ ↑ ↗ …) are absent from the
                // bundled fonts, so the picker rendered as one empty square.
                let cell = qs_theme::space::XL as f32;
                for row in 0..3usize {
                    ui.horizontal(|ui| {
                        for column in 0..3usize {
                            let (r, resp) =
                                ui.allocate_exact_size(Vec2::splat(cell), egui::Sense::click());
                            let active = self.canvas_anchor == (column, row);
                            let painter = ui.painter();
                            painter.rect_filled(
                                r,
                                egui::CornerRadius::same(4),
                                if active {
                                    theme::accent_alpha(60)
                                } else {
                                    theme::bg_card()
                                },
                            );
                            painter.rect_stroke(
                                r,
                                egui::CornerRadius::same(4),
                                egui::Stroke::new(
                                    1.0,
                                    if active || resp.hovered() {
                                        theme::border_hot()
                                    } else {
                                        theme::border_subtle()
                                    },
                                ),
                                egui::StrokeKind::Inside,
                            );
                            painter.circle_filled(
                                r.center(),
                                2.6,
                                if active {
                                    theme::accent_blue()
                                } else {
                                    theme::text_disabled()
                                },
                            );
                            if resp.clicked() {
                                let vertical = self.canvas_insets[0] + self.canvas_insets[2];
                                let horizontal = self.canvas_insets[1] + self.canvas_insets[3];
                                self.canvas_insets =
                                    anchored_insets(vertical, horizontal, (column, row));
                                self.canvas_anchor = (column, row);
                            }
                        }
                    });
                }
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
                    if crate::components::pill_button(ui, "Transparent", "") {
                        self.canvas_background = 0;
                    }
                    if crate::components::pill_button(ui, "White", "") {
                        self.canvas_background = u32::MAX;
                    }
                    if crate::components::pill_button(ui, "Black", "") {
                        self.canvas_background = 255 << 24;
                    }
                });

                ui.separator();
                // Right-to-left inside a horizontal row let the left group run
                // under Apply at narrow widths.
                ui.horizontal_wrapped(|ui| {
                    if crate::components::pill_button(ui, "Trim edges", "") {
                        trim = true;
                    }
                    if crate::components::pill_button(ui, "Fit to image", "") {
                        fit_to_image = true;
                    }
                    if crate::components::pill_button(ui, "Fit to objects", "") {
                        fit_to_objects = true;
                    }
                });
                ui.add_space(qs_theme::space::SM as f32);
                crate::components::dialog_actions(ui, |ui| {
                    if crate::components::pill_button(ui, "Apply", "Apply the canvas insets") {
                        apply = true;
                    }
                });
                ui.add_space(qs_theme::space::SM as f32);
                ui.add_space(qs_theme::space::SM as f32);
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
                    if crate::components::pill_button(ui, "Resize", "") {
                        resample = true;
                    }
                });
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

    /// Export is a dropdown under the Export button: one click picks the
    /// format and the OS save dialog is the only window that follows.
    fn export_menu(&mut self, ctx: &egui::Context) {
        if !self.export_menu_open {
            return;
        }
        let anchor = self.export_anchor.unwrap_or_else(|| {
            let r = ctx.content_rect();
            Rect::from_min_size(egui::pos2(r.right() - 220.0, r.top()), Vec2::new(1.0, 36.0))
        });
        let mut pick: Option<ExportFormat> = None;
        let (mut copy, mut print) = (false, false);
        let area = egui::Area::new(egui::Id::unique("export_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor.left_bottom() + Vec2::new(0.0, qs_theme::space::XS as f32))
            .show(ctx, |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(200.0);
                    ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                        for fmt in [ExportFormat::Png, ExportFormat::Jpeg, ExportFormat::Webp] {
                            let mut label = egui::RichText::new(format!(
                                "Save as {}\u{2026}",
                                fmt.label()
                            ));
                            // the bundled font has no check mark glyph, so the
                            // last-used format is marked by weight and accent
                            if fmt == self.export_format {
                                label = label.strong().color(theme::accent_blue());
                            }
                            if ui.add(egui::Button::new(label).frame(false)).clicked() {
                                pick = Some(fmt);
                            }
                        }
                        ui.separator();
                        if ui
                            .add(egui::Button::new("Copy image").shortcut_text("Ctrl+C").frame(false))
                            .clicked()
                        {
                            copy = true;
                        }
                        if ui.add(egui::Button::new("Print\u{2026}").frame(false)).clicked() {
                            print = true;
                        }
                    });
                });
            });
        let menu_rect = area.response.rect;
        let clicked_outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i
                    .pointer
                    .interact_pos()
                    .is_some_and(|p| !menu_rect.contains(p) && !anchor.contains(p))
        });
        if pick.is_some() || copy || print || clicked_outside {
            self.export_menu_open = false;
        }
        if let Some(fmt) = pick {
            self.export_format = fmt;
            if let Err(e) = self.export_with_dialog(fmt, self.export_quality) {
                self.show_toast(format!("Export failed: {e}"));
            }
        } else if copy {
            let _ = self.copy_to_clipboard();
        } else if print {
            if let Err(e) = self.print_image() {
                self.show_toast(format!("Print failed: {e}"));
            }
        }
    }

    /// Find in capture: the window paints, this owns the matching and panning.
    fn find_dialog(&mut self, ctx: &egui::Context) {
        if !self.find.open {
            return;
        }
        let hits = self.ocr_hints.find(&self.find.query);
        match crate::find::render(
            ctx,
            &mut self.find,
            hits.len(),
            self.ocr_hints.is_scanning(),
        ) {
            crate::find::FindAction::None => {}
            crate::find::FindAction::Close => {
                self.find.open = false;
                self.last_find_focus = None;
            }
            crate::find::FindAction::Step(step) => {
                if !hits.is_empty() {
                    let count = hits.len() as i32;
                    let current = self.find.index.min(hits.len() - 1) as i32;
                    self.find.index = (current + step).rem_euclid(count) as usize;
                }
            }
        }

        // Centre the current match once per change. Re-centring every frame
        // would make the canvas impossible to pan while the dialog is open.
        let hits = self.ocr_hints.find(&self.find.query);
        if hits.is_empty() {
            return;
        }
        let idx = self.find.index.min(hits.len() - 1);
        let focus = (self.find.query.clone(), idx);
        if self.last_find_focus.as_ref() == Some(&focus) {
            return;
        }
        self.last_find_focus = Some(focus);
        let line = &self.ocr_hints.lines()[hits[idx]];
        let centre = PointF::new(
            line.rect.x + line.rect.w / 2.0,
            line.rect.y + line.rect.h / 2.0,
        );
        self.view.center = self.view.map.to_display(centre);
    }

    /// Copy the recognized text inside the selection region.
    ///
    /// Every outcome says something: an empty clipboard is indistinguishable
    /// from a broken command, so "nothing recognized here" and "the engine is
    /// unavailable" stay distinct messages.
    fn copy_text_in_region(&mut self) {
        let Some(region) = self.editor.region() else {
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

    /// Library view: keep it pointed at the capture folder and apply the one
    /// action it can request (open, or reveal in the file manager).
    fn library_view(&mut self, ui: &mut egui::Ui) {
        self.library.set_dir(self.prefs.capture_dir());
        match self.library.show(ui) {
            Some(crate::library::LibraryAction::Open(path)) => {
                if let Err(e) = self.switch_to_path(&path) {
                    self.show_toast(format!("Failed to open: {e}"));
                } else {
                    self.library.open = false;
                }
            }
            Some(crate::library::LibraryAction::Reveal(path)) => {
                if let Err(e) = reveal_in_file_manager(&path) {
                    self.show_toast(format!("Could not reveal file: {e}"));
                }
            }
            None => {}
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
                ui.add_space(qs_theme::space::SM as f32);
                for layout in [
                    CombineLayout::SideBySide,
                    CombineLayout::VerticalSteps,
                    CombineLayout::Grid2x2,
                ] {
                    ui.radio_value(&mut self.combine_layout, layout, layout.label());
                }
                ui.add_space(qs_theme::space::MD as f32);
                ui.add_space(qs_theme::space::SM as f32);
                ui.separator();
                ui.add_space(qs_theme::space::XS as f32);
                crate::components::dialog_actions(ui, |ui| {
                    if crate::components::pill_button_primary(
                        ui,
                        "Create Combined Sheet",
                        "Build the sheet",
                    ) {
                        create = true;
                    }
                });
            });
        self.combine_dialog_open = open;

        if create {
            match self.create_combined_sheet() {
                Ok(()) => self.show_toast("Created combined template"),
                Err(e) => self.show_toast(format!("Combine failed: {e}")),
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
        let before = (old, self.base.clone());
        self.apply_document(doc, flat);
        self.pre_flatten = Some(before);
        self.editor.mark_modified();
        self.touch();
        self.show_toast("Flattened into the image \u{00b7} Ctrl+Z to undo");
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
                self.fit_memo = Some(crate::canvas::FitMemo::of(&self.view, rect.size(), 1.5));
            }
        }
    }

    /// Close the window, asking first when edits are pending.
    ///
    /// The red button and cmd+W both land here, so there is one rule for
    /// throwing edits away.
    pub fn request_close(&mut self, ctx: &egui::Context) {
        if self.unsaved && !self.close_confirmed {
            self.confirm_close = true;
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// True while the "unsaved changes" dialog is up, for tests.
    #[doc(hidden)]
    pub fn is_confirming_close(&self) -> bool {
        self.confirm_close
    }

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
            // Escape closes whichever dialog is on top. Without this the panels
            // could only be dismissed by hitting their small close button.
            if i.consume_key(Modifiers::NONE, Key::Escape) {
                if self.export_menu_open {
                    self.export_menu_open = false;
                } else if self.canvas_dialog_open {
                    self.canvas_dialog_open = false;
                } else if self.combine_dialog_open {
                    self.combine_dialog_open = false;
                } else if self.alignment_open {
                    self.alignment_open = false;
                } else if self.library.open {
                    self.library.open = false;
                }
            }
            // Settings has the standard macOS shortcut; Align needs a panel that
            // used to be reachable only from the Layers header.
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::T)
                || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::T)
            {
                self.show_filmstrip = !self.show_filmstrip;
            }
            if i.consume_key(Modifiers::COMMAND, Key::Comma) {
                self.canvas_dialog_open = false;
                self.export_menu_open = false;
                self.alignment_open = false;
                spawn_app("--settings");
            }
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::A)
                || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::A)
            {
                self.canvas_dialog_open = false;
                self.export_menu_open = false;
                self.alignment_open = true;
            }
            // Dialog shortcuts: these panels were only reachable by mouse.
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::C)
                || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::C)
            {
                self.export_menu_open = false;
                self.canvas_dialog_open = true;
            }
            if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::E)
                || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::E)
            {
                self.canvas_dialog_open = false;
                self.export_menu_open = true;
            }
            // cmd+shift+F never reached us (macOS swallows it), so the
            // destructive flatten confirmation uses option+cmd+F.
            if i.consume_key(Modifiers::COMMAND | Modifiers::ALT, Key::F)
                || i.consume_key(Modifiers::CTRL | Modifiers::ALT, Key::F)
            {
                self.canvas_dialog_open = false;
                self.export_menu_open = false;
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

    /// Which panels are open: canvas, export menu, align.
    #[doc(hidden)]
    pub fn open_dialogs(&self) -> (bool, bool, bool) {
        (
            self.canvas_dialog_open,
            self.export_menu_open,
            self.alignment_open,
        )
    }

    /// Filmstrip visibility, for tests.
    #[doc(hidden)]
    pub fn filmstrip_visible(&self) -> bool {
        self.show_filmstrip
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

        // An open overlay owns the navigation keys: otherwise the canvas nudge
        // and the Escape-to-deselect path swallow them before the dialog sees
        // them, which made the Library keyboard route dead on arrival.
        let overlay_keys = self.library.open || self.find.open;
        // A focused text field owns plain keystrokes: typing "e" in Find or in
        // the tag box used to switch the active tool as well.
        let typing = ctx.memory(|m| m.focused().is_some());
        ctx.input_mut(|i| {
            if self.find.open && i.consume_key(Modifiers::NONE, Key::Escape) {
                self.find.open = false;
            }
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

            if !overlay_keys && i.consume_key(Modifiers::NONE, Key::Escape) {
                esc = true;
            }

            if !typing && i.modifiers.is_none() {
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
                } else if i.key_pressed(Key::U) {
                    Some(Tool::Fill)
                } else if i.key_pressed(Key::Y) {
                    Some(Tool::SmartMove)
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
            if !overlay_keys && !typing && !self.show_filmstrip && i.consume_key(Modifiers::NONE, Key::ArrowLeft)
                || i.consume_key(Modifiers::SHIFT, Key::ArrowLeft)
            {
                nudge = Some((-nudge_step, 0.0));
            }
            if !overlay_keys && !typing && !self.show_filmstrip && i.consume_key(Modifiers::NONE, Key::ArrowRight)
                || i.consume_key(Modifiers::SHIFT, Key::ArrowRight)
            {
                nudge = Some((nudge_step, 0.0));
            }
            if !overlay_keys && !typing && !self.show_filmstrip && i.consume_key(Modifiers::NONE, Key::ArrowUp)
                || i.consume_key(Modifiers::SHIFT, Key::ArrowUp)
            {
                nudge = Some((0.0, -nudge_step));
            }
            if !overlay_keys && !typing && !self.show_filmstrip && i.consume_key(Modifiers::NONE, Key::ArrowDown)
                || i.consume_key(Modifiers::SHIFT, Key::ArrowDown)
            {
                nudge = Some((0.0, nudge_step));
            }
        });

        if undo {
            if !self.editor.undo() {
                if let Some((doc, base)) = self.pre_flatten.take() {
                    self.apply_document(doc, base);
                    self.editor.mark_modified();
                    self.show_toast("Flatten undone");
                }
            }
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
        self.handle_dialog_keys(&ctx);
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
                // Without an explicit fill the panel rendered as a black strip
                // in the light theme.
                .frame(
                    egui::Frame::NONE
                        .fill(theme::bg_inspector_bot())
                        .stroke(egui::Stroke::new(1.0, theme::border_subtle()))
                        .inner_margin(egui::Margin::symmetric(
                            qs_theme::space::MD as i8,
                            qs_theme::space::XS as i8,
                        )),
                )
                .show(ui, |ui| {
                    styles::render_bar(
                        ui,
                        &mut self.editor,
                        &mut self.styles,
                        &mut self.style_filter,
                    )
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
        if top_res.export_rect.is_some() {
            self.export_anchor = top_res.export_rect;
        }
        if top_res.export {
            self.export_menu_open = !self.export_menu_open;
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
        self.export_menu(&ctx);
        self.combine_dialog(&ctx);
        self.find_dialog(&ctx);

        // Bottom Studio Status Bar (32px = 4px + 24px hit target + 4px, single top hairline)
        let status_resp = egui::Panel::bottom("status_bar")
            // 26 px controls + 4 px padding top and bottom.
            .exact_size(crate::components::CONTROL_H + 2.0 * qs_theme::space::XS as f32)
            .frame(
                egui::Frame::NONE
                    .fill(theme::bg_status())
                    .inner_margin(egui::Margin::symmetric(
                        qs_theme::space::MD as i8,
                        qs_theme::space::XS as i8,
                    )),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = qs_theme::space::XS as f32;
                    if crate::components::status_pill(
                        ui,
                        "Layers",
                        self.show_layers,
                        "Toggle Layers panel",
                    ) {
                        self.show_layers = !self.show_layers;
                    }
                    if crate::components::status_pill(
                        ui,
                        "Inspector",
                        self.show_inspector,
                        "Toggle Properties & Effects panel",
                    ) {
                        self.show_inspector = !self.show_inspector;
                    }
                    if crate::components::status_pill(
                        ui,
                        "Recent",
                        self.show_filmstrip,
                        "Toggle the recent captures strip (Ctrl+Shift+T)",
                    ) {
                        self.show_filmstrip = !self.show_filmstrip;
                    }
                    status_divider(ui);

                    let view_r = self.view.display_rect();
                    ui.label(
                        egui::RichText::new(format!(
                            "{} × {} px",
                            view_r.w.round() as i32,
                            view_r.h.round() as i32
                        ))
                        .size(qs_theme::font::SMALL as f32)
                        .color(theme::text_secondary()),
                    );

                    if let Some(p) = self.last_cursor_doc {
                        status_divider(ui);
                        ui.label(
                            egui::RichText::new(format!(
                                "X: {}  Y: {}",
                                p.x.round() as i32,
                                p.y.round() as i32
                            ))
                            .size(qs_theme::font::SMALL as f32)
                            .color(theme::text_tertiary()),
                        );
                    }

                    if let Some(b) = self.editor.selection_bounds() {
                        status_divider(ui);
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
                                .size(qs_theme::font::SMALL as f32)
                                .color(theme::accent_blue_soft()),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::components::status_pill(
                            ui,
                            "New capture",
                            false,
                            "Capture a new screenshot (\u{2318}N)",
                        ) {
                            spawn_app("--capture");
                        }
                        if self.editor.region().is_some()
                            && crate::components::status_pill(
                                ui,
                                "Copy text",
                                false,
                                "Copy the recognized text inside the selection",
                            )
                        {
                            self.copy_text_in_region();
                        }
                        if crate::components::status_pill(
                            ui,
                            "Library",
                            self.library.open,
                            "Browse, search and tag captures (\u{2318}L)",
                        ) {
                            self.library.toggle();
                        }
                        if crate::components::status_pill(
                            ui,
                            "Settings",
                            false,
                            "Open Quickshort Settings (\u{2318},)",
                        ) {
                            spawn_app("--settings");
                        }
                        let is_dark = qs_theme::pal().dark;
                        let (theme_lbl, theme_tip) = if is_dark {
                            ("Dark", "Theme: Dark \u{2014} click for Light")
                        } else {
                            ("Light", "Theme: Light \u{2014} click for Dark")
                        };
                        if crate::components::status_pill(ui, theme_lbl, false, theme_tip) {
                            let next = if is_dark { "light" } else { "dark" };
                            self.prefs.theme = next.into();
                            self.prefs.apply_live();
                            self.prefs.save();
                            theme::sync_egui_visuals(&ctx);
                        }
                        status_divider(ui);
                        for (label, target_zoom) in [("200%", 2.0f32), ("100%", 1.0), ("50%", 0.5)]
                        {
                            let active = (self.view.zoom - target_zoom).abs() < 0.01;
                            if crate::components::status_pill(ui, label, active, "") {
                                if let Some(rect) = self.last_canvas_rect {
                                    self.view.zoom_at(
                                        target_zoom / self.view.zoom.max(0.01),
                                        rect.center(),
                                        rect,
                                    );
                                }
                            }
                        }
                        if crate::components::status_pill(ui, "Fit", false, "Fit to window") {
                            self.apply_zoom_cmd(ZoomCmd::Fit);
                        }
                    });
                });
            });
        crate::components::panel_divider(
            ui,
            status_resp.response.rect,
            crate::components::DividerEdge::Top,
        );

        // Collapsible Recent Captures Filmstrip (sits cleanly above Status Bar)
        if self.show_filmstrip {
            let mut switch_target: Option<PathBuf> = None;
            let mut refresh_tray = false;
            // Tall enough for the RECENT column (label + three pill buttons);
            // at 94 the thumbnails were clipped by the panel edge.
            egui::Panel::bottom("filmstrip_panel")
                .exact_size(124.0)
                .frame(
                    egui::Frame::NONE
                        .fill(theme::bg_inspector_bot())
                        .stroke(egui::Stroke::new(1.0, theme::border_dark_hairline()))
                        .inner_margin(egui::Margin::symmetric(10, 6)),
                )
                .show(ui, |ui| {
                    // Keyboard route: arrows move a cursor, Space marks for
                    // Combine, Enter opens the marked set. Without this the tray
                    // was mouse-only.
                    if ui.ctx().memory(|m| m.focused().is_none()) {
                        let n = self.filmstrip.items.len();
                        let (left, right, space, enter) = ui.input(|i| {
                            (
                                i.key_pressed(egui::Key::ArrowLeft),
                                i.key_pressed(egui::Key::ArrowRight),
                                i.key_pressed(egui::Key::Space),
                                i.key_pressed(egui::Key::Enter),
                            )
                        });
                        let (focus, action) = crate::filmstrip_keys::tray_keys(
                            self.filmstrip_focus,
                            n,
                            self.tray_selected.len(),
                            crate::filmstrip_keys::TrayInput {
                                left,
                                right,
                                space,
                                enter,
                            },
                        );
                        self.filmstrip_focus = focus;
                        match action {
                            crate::filmstrip_keys::TrayAction::None => {}
                            crate::filmstrip_keys::TrayAction::ToggleMark(index) => {
                                if let Some(item) = self.filmstrip.items.get(index) {
                                    let path = item.path.clone();
                                    if let Some(pos) =
                                        self.tray_selected.iter().position(|p| p == &path)
                                    {
                                        self.tray_selected.remove(pos);
                                    } else {
                                        self.tray_selected.push(path);
                                    }
                                }
                            }
                            crate::filmstrip_keys::TrayAction::Open(index) => {
                                if let Some(item) = self.filmstrip.items.get(index) {
                                    switch_target = Some(item.path.clone());
                                }
                            }
                            crate::filmstrip_keys::TrayAction::OpenCombine => {
                                self.combine_dialog_open = true;
                            }
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            // Wide enough for the "Combine (N)" pill.
                            ui.set_width(104.0);
                            ui.label(
                                egui::RichText::new("RECENT")
                                    .size(10.0)
                                    .strong()
                                    .color(theme::text_tertiary()),
                            );
                            ui.add_space(qs_theme::space::XS as f32);
                            if ui
                                .small_button("Refresh")
                                .on_hover_text("Rescan recent captures (Shift+Click to multi-select)")
                                .clicked()
                            {
                                refresh_tray = true;
                            }
                            if ui
                                .small_button("Library")
                                .on_hover_text("Search, tag and prune every capture")
                                .clicked()
                            {
                                self.library.toggle();
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
                        status_divider(ui);
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
                                        for (item_idx, item) in
                                            self.filmstrip.items.iter().enumerate()
                                        {
                                            let focused = self.filmstrip_focus == Some(item_idx);
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
                                            if focused {
                                                ui.painter().rect_stroke(
                                                    rect,
                                                    theme::RADIUS_PILL,
                                                    egui::Stroke::new(2.0, theme::accent_blue()),
                                                    StrokeKind::Outside,
                                                );
                                            }
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

        let layers_mutated = if self.show_layers && !self.library.open {
            let layers_resp = egui::Panel::left("layers_panel")
            .resizable(true)
            .default_size(204.0)
            .size_range(176.0..=300.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::bg_inspector_top())
                    .inner_margin(egui::Margin::same(12)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("LAYERS")
                            .size(qs_theme::font::CAPTION as f32)
                            .strong()
                            .color(theme::text_tertiary()),
                    );
                    ui.label(
                        egui::RichText::new(format!("{}", self.editor.doc().objects.len()))
                            .size(qs_theme::font::CAPTION as f32)
                            .color(theme::text_tertiary()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.editor.selection().len() >= 2 {
                            self.alignment_open = crate::components::pill_button(
                                ui,
                                "Align",
                                "Align & distribute",
                            ) || self.alignment_open;
                        }
                        if self.editor.selection_grouped()
                            && crate::components::pill_button(
                                ui,
                                "Ungroup",
                                "Ungroup (Ctrl+Shift+G)",
                            )
                            && self.editor.ungroup_selected()
                        {
                            self.touch();
                        }
                        if self.editor.selection().len() >= 2
                            && crate::components::pill_button(ui, "Group", "Group (Ctrl+G)")
                            && self.editor.group_selected()
                        {
                            self.touch();
                        }
                    });
                });
                ui.add_space(qs_theme::space::SM as f32);
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
                if rows.is_empty() {
                    if crate::components::empty_state(
                        ui,
                        "No annotations yet",
                        "Pick a tool above or start\nwith an arrow callout",
                        Some("Draw Arrow"),
                    ) {
                        self.editor.set_tool(Tool::Arrow);
                    }
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                    for (index, id, label, visible, locked, grouped, selected) in rows {
                        // Padded row like the Library list: selection is a wash,
                        // hover a hairline, and every control has a fixed box.
                        let row_fill = if selected {
                            theme::accent_alpha(if qs_theme::pal().dark { 72 } else { 36 })
                        } else {
                            Color32::TRANSPARENT
                        };
                        let response = egui::Frame::NONE
                            .inner_margin(egui::Margin::symmetric(
                                qs_theme::space::SM as i8,
                                qs_theme::space::XS as i8,
                            ))
                            .corner_radius(egui::CornerRadius::same(6))
                            .fill(row_fill)
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.x = qs_theme::space::XS as f32;
                                // Same row rhythm as the Library list.
                                ui.set_min_height(crate::components::CONTROL_H);
                                ui.horizontal(|ui| {
                                    // Vector icons, not glyphs: the bundled
                                    // fonts have no ◉/○/🔒, so the controls
                                    // rendered as invisible buttons.
                                    let (eye_rect, eye_resp) = ui.allocate_exact_size(
                                        egui::Vec2::splat(20.0),
                                        egui::Sense::click(),
                                    );
                                    crate::icons::draw_action_icon(
                                        ui.painter(),
                                        eye_rect.center(),
                                        if visible {
                                            crate::icons::ActionIcon::Eye
                                        } else {
                                            crate::icons::ActionIcon::EyeOff
                                        },
                                        if visible {
                                            theme::text_secondary()
                                        } else {
                                            theme::text_disabled()
                                        },
                                    );
                                    if eye_resp
                                        .on_hover_text(if visible {
                                            "Hide layer"
                                        } else {
                                            "Show layer"
                                        })
                                        .clicked()
                                    {
                                        changed |= self.editor.set_visible(id, !visible);
                                    }
                                    let (lock_rect, lock_resp) = ui.allocate_exact_size(
                                        egui::Vec2::splat(20.0),
                                        egui::Sense::click(),
                                    );
                                    crate::icons::draw_action_icon(
                                        ui.painter(),
                                        lock_rect.center(),
                                        if locked {
                                            crate::icons::ActionIcon::Lock
                                        } else {
                                            crate::icons::ActionIcon::Unlock
                                        },
                                        if locked {
                                            theme::text_primary()
                                        } else {
                                            theme::text_tertiary()
                                        },
                                    );
                                    if lock_resp
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
                                        .add(
                                            egui::Label::new(
                                                egui::RichText::new(text).size(
                                                    qs_theme::font::SMALL as f32,
                                                ),
                                            )
                                            .sense(egui::Sense::click()),
                                        )
                                        .on_hover_text(format!(
                                            "Drag to reorder · index {}",
                                            index + 1
                                        ))
                                        .clicked()
                                    {
                                        self.editor.select(Some(id));
                                    }
                                });
                            })
                            .response;
                        if response.hovered() {
                            ui.painter().rect_stroke(
                                response.rect,
                                egui::CornerRadius::same(6),
                                egui::Stroke::new(1.0, theme::border_subtle()),
                                egui::StrokeKind::Inside,
                            );
                        }
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
                }
                changed
            });
            let lr = layers_resp.response.rect;
            ui.painter().line_segment(
                [lr.right_top(), lr.right_bottom()],
                egui::Stroke::new(1.0, theme::border_dark_hairline()),
            );
            layers_resp.inner
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
                    // One row per axis: wrapping all six mixed them up at 720.
                    ui.horizontal(|ui| {
                        for (label, a) in [
                            ("Left", Alignment::Left),
                            ("Center X", Alignment::HCenter),
                            ("Right", Alignment::Right),
                        ] {
                            if crate::components::pill_button(ui, label, "") {
                                if self.editor.align_selected(a) {
                                    self.touch();
                                }
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        for (label, a) in [
                            ("Top", Alignment::Top),
                            ("Center Y", Alignment::VCenter),
                            ("Bottom", Alignment::Bottom),
                        ] {
                            if crate::components::pill_button(ui, label, "") {
                                if self.editor.align_selected(a) {
                                    self.touch();
                                }
                            }
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        let enabled = self.editor.selection().len() >= 3;
                        ui.add_enabled_ui(enabled, |ui| {
                            if crate::components::pill_button(ui, "Distribute horizontally", "")
                                && self.editor.distribute_selected(Axis::Horizontal)
                            {
                                self.touch();
                            }
                            if crate::components::pill_button(ui, "Distribute vertically", "")
                                && self.editor.distribute_selected(Axis::Vertical)
                            {
                                self.touch();
                            }
                        });
                    });
                });
            self.alignment_open = open;
        }

        let panel_mutated = if self.show_inspector && !self.library.open {
            let panel_resp = egui::Panel::right("image_panel")
                .resizable(false)
                .exact_size(256.0)
                .frame(
                    egui::Frame::NONE
                        .fill(theme::bg_inspector_top())
                        .inner_margin(egui::Margin::same(12)),
                )
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .show(ui, |ui| {
                            let changed = image_panel::render(ui, &mut self.editor);
                            ui.add_space(qs_theme::space::MD as f32);
                            crate::components::card(ui, |ui| {
                                crate::components::section_header(ui, "Quick Actions");
                                if crate::components::button(
                                    ui,
                                    "Simplify UI",
                                    crate::components::ButtonVariant::Tonal,
                                    "Convert UI text/controls into clean editable blocks",
                                )
                                .clicked()
                                {
                                    let n = self.editor.simplify_from_image(&self.base);
                                    if n > 0 {
                                        self.touch();
                                        self.show_toast(format!("Simplified {n} UI block(s)"));
                                    } else {
                                        self.show_toast("No UI blocks detected");
                                    }
                                }
                                ui.add_space(qs_theme::space::SM as f32);
                                if crate::components::button(
                                    ui,
                                    "Flatten all",
                                    crate::components::ButtonVariant::Secondary,
                                    "Bake every annotation into the image (undo with Ctrl+Z)",
                                )
                                .clicked()
                                {
                                    if let Err(e) = self.flatten_all() {
                                        self.show_toast(format!("Flatten failed: {e}"));
                                    }
                                }
                            });
                            changed
                        })
                        .inner
                });
            let pr = panel_resp.response.rect;
            ui.painter().line_segment(
                [pr.left_top(), pr.left_bottom()],
                egui::Stroke::new(1.0, theme::border_dark_hairline()),
            );
            panel_resp.inner
        } else {
            false
        };
        if panel_mutated {
            self.touch();
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if self.library.open {
                    self.library_view(ui);
                    return;
                }
                let (screen_rect, response) =
                    ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
                self.last_canvas_rect = Some(screen_rect);
                self.view.sync(self.editor.doc());
                self.last_cursor_doc = ui
                    .input(|i| i.pointer.latest_pos())
                    .filter(|p| screen_rect.contains(*p))
                    .map(|p| self.view.screen_to_doc(p, screen_rect));

                // Smart Move: keep line hints in step with the base image, and
                // keep repainting while the worker runs so the proposal shows
                // up without the user moving the mouse again.
                // Hints feed two things: the Smart Move proposal, and copying
                // the text inside the selection region.
                let wants_hints = self.editor.tool() == Tool::SmartMove
                    || self.editor.region().is_some()
                    || self.find.open;
                if wants_hints {
                    self.ocr_hints.ensure(
                        self.editor.doc(),
                        &self.base,
                        &self.prefs.ocr_lang,
                    );
                    if self.ocr_hints.is_scanning() {
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_millis(120));
                    }
                    self.ocr_hints.poll();
                }
                let hint_rect = if self.editor.tool() == Tool::SmartMove
                    && self.editor.pending_smart_move().is_none()
                    && self.editor.move_src_rect().is_none()
                {
                    self.last_cursor_doc
                        .and_then(|p| self.ocr_hints.hit(p).map(|h| h.rect))
                } else {
                    None
                };

                let fit_cap = if self.needs_initial_fit {
                    Some(1.0)
                } else {
                    self.fit_memo
                        .filter(|m| m.wants_refit(&self.view, screen_rect.size()))
                        .map(|m| m.max_zoom)
                };
                if let Some(cap) = fit_cap {
                    if screen_rect.width() > 32.0 && screen_rect.height() > 32.0 {
                        self.view
                            .fit_to(self.view.display_rect(), screen_rect, 40.0);
                        self.view.zoom = self.view.zoom.min(cap);
                        self.needs_initial_fit = false;
                        self.fit_memo = Some(crate::canvas::FitMemo::of(
                            &self.view,
                            screen_rect.size(),
                            cap,
                        ));
                    }
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
                        // Smart Move: a click on a recognized line arms that
                        // line; anywhere else keeps the manual drag flow.
                        let arm = if self.editor.tool() == Tool::SmartMove
                            && self.editor.pending_smart_move().is_none()
                        {
                            self.ocr_hints.hit(doc_p).map(|h| h.rect)
                        } else {
                            None
                        };
                        let armed = arm.is_some_and(|r| self.editor.begin_smart_move(r));
                        if !armed {
                            self.editor.pointer_down_mod(doc_p, shift_held);
                            self.editor.pointer_up(doc_p);
                        }
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

                // 5a. Find in capture: every match dimly, the current one framed
                if self.find.open {
                    let hits = self.ocr_hints.find(&self.find.query);
                    if !hits.is_empty() {
                        let rects: Vec<Rect> = hits
                            .iter()
                            .map(|i| {
                                let r = self.ocr_hints.lines()[*i].rect;
                                self.view.doc_rect_to_screen(r, screen_rect)
                            })
                            .collect();
                        let current = self.find.index.min(rects.len() - 1);
                        canvas::paint_find_hits(&painter, &rects, current);
                    }
                }

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
                if self.editor.tool() == Tool::SmartMove {
                    let armed = self.editor.pending_smart_move().is_some();
                    let rect = if armed {
                        self.editor.pending_smart_move()
                    } else {
                        self.editor.move_src_rect().or(hint_rect)
                    };
                    if let Some(r) = rect {
                        let s_rect = self.view.doc_rect_to_screen(r, screen_rect);
                        canvas::paint_move_overlay(&painter, s_rect, armed);
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
            fit_memo: None,
            saw_drag_press: false,
            show_layers: true,
            show_inspector: true,
            show_filmstrip: false,
            filmstrip_focus: None,
            last_cursor_doc: None,
            filmstrip: Filmstrip::new(None),
            session_cache: std::collections::HashMap::new(),
            toast: None,
            canvas_dialog_open: false,
            pre_flatten: None,
            canvas_insets: [0.0; 4],
            canvas_background: 0,
            resize_w: 20,
            resize_h: 10,
            resize_lock: true,
            export_menu_open: false,
            export_anchor: None,
            export_format: ExportFormat::Png,
            export_quality: 90,
            combine_dialog_open: false,
            combine_layout: CombineLayout::SideBySide,
            tray_selected: Vec::new(),
            canvas_anchor: (1, 1),
            layer_drag_id: None,
            alignment_open: false,
            prefs: crate::prefs::AppPrefs::default(),
            styles: styles::Library::default(),
            library: crate::library::Library::default(),
            style_filter: String::new(),
            ocr_hints: crate::ocr_hint::OcrHints::default(),
            find: crate::find::FindState::default(),
            last_find_focus: None,
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
/// Hairline group divider for the status bar.
///
/// `ui.separator()` inside a dense bar inherits the widget stroke and the
/// surrounding item spacing, which made the bar's dividers heavier than the
/// ones elsewhere. This draws a short token-coloured line with token gaps.
fn status_divider(ui: &mut egui::Ui) {
    ui.add_space(qs_theme::space::XS as f32);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(1.0, qs_theme::space::LG as f32),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(rect, 0.0, theme::divider_vertical());
    ui.add_space(qs_theme::space::XS as f32);
}

/// Reveal a capture in the platform's file manager, selecting it where the
/// platform supports it.
fn reveal_in_file_manager(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = path.parent().unwrap_or(path);
        std::process::Command::new("xdg-open").arg(target).spawn()?;
    }
    Ok(())
}

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

/// Runs the sibling `quickshort` binary with `arg` (`--capture`,
/// `--settings`), so the editor reaches Capture and the one Settings window
/// without a trip to the tray.
fn spawn_app(arg: &str) {
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
