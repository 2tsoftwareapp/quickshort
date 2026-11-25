//! `EditorApp` state decomposition: the sub-states the facade composes.
//! Grouping keeps `app.rs` a view/controller shell — document lifecycle,
//! viewport mapping and modal-tool working state each own their
//! invariants here instead of sitting as loose fields on the app struct.

use crate::{
    canvas::{CanvasView, FitMemo},
    templates::CombineLayout,
};
use eframe::egui::{Pos2, Rect, TextureHandle};
use image::RgbaImage;
use qs_doc::{Document, ObjectId, PointF};
use qs_edit::Editor;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::Instant;

/// The .qsdoc lifecycle: the document plus the image under it, the dirty
/// flag, autosave/recovery bookkeeping and the session cache the
/// filmstrip uses when swapping captures.
pub(crate) struct DocumentState {
    pub editor: Editor,
    pub base: RgbaImage,
    pub path: Option<PathBuf>,
    /// The document holds edits that are not on disk yet.
    pub unsaved: bool,
    /// Editor revision at the last save/load; `unsaved` is derived from it.
    pub saved_rev: u64,
    pub session_cache: HashMap<PathBuf, SavedDocSession>,
    /// Prevents overlapping background autosave threads.
    pub autosave_busy: Arc<AtomicBool>,
    /// When the recovery copy was last written.
    pub autosave_at: Option<Instant>,
    /// Document and base image before "Flatten all", so undo can bring
    /// the layers back.
    pub pre_flatten: Option<(Document, RgbaImage)>,
}

/// Canvas viewport: the zoom/pan mapping (`view`), fit bookkeeping, and
/// the filmstrip's keyboard selection.
pub(crate) struct ViewportState {
    pub view: CanvasView,
    pub needs_initial_fit: bool,
    pub last_canvas_rect: Option<Rect>,
    pub fit_memo: Option<FitMemo>,
    pub last_cursor_doc: Option<PointF>,
    pub show_filmstrip: bool,
    /// Keyboard cursor over the filmstrip (arrows move it, Space marks).
    pub filmstrip_focus: Option<usize>,
    pub tray_selected: Vec<PathBuf>,
}

/// Working state of the modal tool surfaces — Canvas & Resize, Export,
/// Combine and the Quick Styles filter — plus the active pointer and
/// inline-text sessions driven from `input.rs`.
pub(crate) struct ToolState {
    pub canvas_insets: [f32; 4],
    pub canvas_anchor: (usize, usize),
    pub canvas_background: u32,
    pub resize_w: u32,
    pub resize_h: u32,
    pub resize_lock: bool,
    pub export_format: ExportFormat,
    pub export_quality: u8,
    pub combine_layout: CombineLayout,
    /// Text filter for the quick-style bar (session state).
    pub style_filter: String,
    pub saw_drag_press: bool,
    /// In-flight IME composition (Telex/VNI preedit) drawn beside the caret.
    pub ime_preedit: String,
}

/// Per-capture stash used when the filmstrip swaps documents, so going
/// back restores edits, view and dirty state.
pub(crate) struct SavedDocSession {
    pub editor: Editor,
    pub base: RgbaImage,
    pub doc_path: Option<PathBuf>,
    pub view: CanvasView,
    pub unsaved: bool,
    pub saved_rev: u64,
}

/// Texture + CPU cache for the canvas repaint; `needs_render` is the dirty
/// flag that forces the next frame to rebuild `texture`.
#[derive(Default)]
pub(crate) struct RenderState {
    pub texture: Option<TextureHandle>,
    /// Reusable CPU render cache (premultiplied base pixmap + scratch pixmap).
    pub render_cache: qs_render::RenderCache,
    /// Cached 2x2 repeating checkerboard texture keyed by `pal().dark`.
    pub checker_tex: Option<(bool, TextureHandle)>,
    /// The rendered texture is stale and must be rebuilt this frame.
    pub needs_render: bool,
}

/// Open modals, dropdowns and the canvas context menu — at most one is open
/// at a time in practice; grouping makes that invariant visible.
/// `close_confirmed` isn't a dialog but lives here as part of the close
/// handshake.
#[derive(Default)]
pub(crate) struct Dialogs {
    /// Closing was already confirmed, so the next close request goes through.
    pub close_confirmed: bool,
    /// The unsaved-changes dialog is on screen.
    pub confirm_close: bool,
    /// Edits left behind by a crashed session, waiting for the user to accept
    /// or reject them.
    pub pending_recovery: Option<(Document, RgbaImage)>,
    pub canvas_dialog_open: bool,
    pub export_menu_open: bool,
    pub export_anchor: Option<Rect>,
    pub app_menu_open: bool,
    pub app_menu_anchor: Option<Rect>,
    pub combine_dialog_open: bool,
    /// Right-click context menu on canvas: `(screen_pos, clicked_object_id)`.
    pub context_menu: Option<(Pos2, Option<ObjectId>)>,
    /// Keyboard shortcuts cheat sheet modal (`?` or Status Bar button).
    pub shortcuts_open: bool,
    pub alignment_open: bool,
}

/// What the user picked in the "unsaved changes" dialog.
pub(crate) enum CloseDecision {
    Save,
    Discard,
    Cancel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ExportFormat {
    #[default]
    Png,
    Jpeg,
    Webp,
}

impl ExportFormat {
    pub fn ext(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG (Lossless)",
            Self::Jpeg => "JPEG (Compact)",
            Self::Webp => "WebP",
        }
    }
}
