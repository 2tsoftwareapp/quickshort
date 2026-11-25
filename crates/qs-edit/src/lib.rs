//! Headless editing engine for `qs-doc` documents.
//!
//! The UI (egui window, tests, future frontends) feeds pointer / keyboard
//! events in and reads the document + selection back. No pixels, no window.
//!
//! Undo model: whole-state snapshots (`objects`, `view`, `background`,
//! `step_start`). Drags and text sessions record one entry when they end,
//! and only if something actually changed.

mod effects;
mod history;
mod ops;
mod transform;

pub mod hit;
pub mod simplify;
pub mod trim;

pub use hit::{Handle, HandleId};

use history::Snap;
use ops::Drag;
use qs_doc::{
    ArrowHead, BlurMode, CalloutShape, Document, Fill, MagnifyShape, Object, ObjectId, PointF,
    RectF, StampKind, TextStyle, PALETTE, PIX_BLOCK, SIZES,
};

// The tool model (key, label, icon, drag gesture) is shared with the
// overlay — it lives in `qs-doc` and is re-exported so `qs_edit::Tool`
// keeps working for every caller.
pub use qs_doc::Tool;

/// Highlighter width = stroke width × this (S/M/L ≈ overlay marker sizes).
pub(crate) const MARKER_K: f32 = 2.6;
pub const DEFAULT_CALLOUT: (f32, f32) = (140.0, 50.0);

/// Style applied to newly created objects (and mirrored by the UI palette).
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub rgb: u32,
    pub width: f32,
    pub fill: Fill,
    /// px / bold / align; the colour comes from `rgb` (or the callout fill)
    pub text: TextStyle,
    pub arrow_head: ArrowHead,
    pub callout_shape: CalloutShape,
    pub blur_mode: BlurMode,
    pub blur_strength: f32,
    pub magnify_zoom: f32,
    pub magnify_shape: MagnifyShape,
    pub spot_dim: f32,
    pub spot_blur: f32,
    pub stamp_kind: StampKind,
    pub eraser_width: f32,
    pub step_radius: f32,
    pub cutout_axis: qs_doc::CutoutAxis,
    pub cutout_seam: qs_doc::CutoutSeam,
    /// Paint-bucket tolerance, per channel (0 = exact match).
    pub fill_tolerance: u8,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            rgb: PALETTE[0],
            width: SIZES[1],
            fill: Fill::None,
            text: TextStyle::default(),
            arrow_head: ArrowHead::Open,
            callout_shape: CalloutShape::RoundRect,
            blur_mode: BlurMode::Pixelate,
            blur_strength: PIX_BLOCK,
            magnify_zoom: 2.0,
            magnify_shape: MagnifyShape::Ellipse,
            spot_dim: 0.6,
            spot_blur: 0.0,
            stamp_kind: StampKind::Approved,
            eraser_width: 12.0,
            step_radius: 12.0,
            cutout_axis: qs_doc::CutoutAxis::Horizontal,
            cutout_seam: qs_doc::CutoutSeam::Jagged,
            fill_tolerance: 24,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    DocStart,
    DocEnd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZOrder {
    Front,
    Back,
    Forward,
    Backward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Left,
    HCenter,
    Right,
    Top,
    VCenter,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

/// Active text edit: caret + selection anchor (byte offsets).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub id: ObjectId,
    pub caret: usize,
    pub anchor: usize,
}

impl TextEdit {
    pub fn range(&self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }
}

/// Cursor-feedback view of the in-progress drag gesture (`drag_hint`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragHint {
    None,
    Move,
    Marquee,
    Handle(HandleId),
    Create,
    TextSelect,
    Crop,
    Cutout,
    SmartMove,
    Region,
}

pub struct Editor {
    doc: Document,
    tool: Tool,
    pub style: Style,
    /// Selected object ids; the last one is the primary (handles/text edit).
    selection: Vec<ObjectId>,
    drag: Drag,
    pre: Option<Snap>,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    edit: Option<TextEdit>,
    edit_pre: Option<Snap>,
    effects_pre: Option<Snap>,
    /// Hit slack in logical px (UI sets `6.0 / zoom`).
    pub hit_tol: f32,
    /// Bumped on every document change, undo and redo. Editors compare it
    /// against the value they last wrote to disk to know whether the document
    /// still has unsaved edits.
    rev: u64,
    /// Keep Crop, Cutout and SelectRegion active after committing.
    pub sticky_tools: bool,
    /// Crop rectangle being previewed by the Crop tool (transient, logical px).
    crop_rect: Option<RectF>,
    /// Cutout strip being previewed by the Cutout tool (transient, logical px).
    cutout_rect: Option<RectF>,
    /// Region rectangle being previewed by the SelectRegion tool (transient, logical px).
    region_rect: Option<RectF>,
    /// Marquee rectangle being previewed by the Select tool (transient, logical px).
    marquee_rect: Option<RectF>,
    /// Shift state of the in-flight pointer_down (for Select toggling).
    pending_shift: bool,
    /// Source rect of a Smart Move waiting for its destination click.
    pending_move: Option<RectF>,
    /// Source rect being dragged out by the Smart Move tool (transient).
    move_src_rect: Option<RectF>,
}

/// Readable text colour on `rgb`.
pub fn contrast(rgb: u32) -> u32 {
    let (r, g, b) = ((rgb >> 16) & 255, (rgb >> 8) & 255, rgb & 255);
    let l = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    if l > 150.0 {
        0x000000
    } else {
        0xFFFFFF
    }
}

impl Editor {
    pub fn new(doc: Document) -> Self {
        Self {
            doc,
            tool: Tool::Select,
            style: Style::default(),
            selection: Vec::new(),
            drag: Drag::None,
            pre: None,
            undo: Vec::new(),
            redo: Vec::new(),
            edit: None,
            edit_pre: None,
            effects_pre: None,
            hit_tol: 5.0,
            sticky_tools: false,
            rev: 0,
            crop_rect: None,
            cutout_rect: None,
            region_rect: None,
            marquee_rect: None,
            pending_shift: false,
            pending_move: None,
            move_src_rect: None,
        }
    }

    /// Monotonic counter of document edits; see [`Editor::rev`].
    pub fn revision(&self) -> u64 {
        self.rev
    }

    /// Crop rectangle currently previewed by the Crop tool, if a drag is on.
    pub fn crop_rect(&self) -> Option<RectF> {
        self.crop_rect
    }

    pub fn cutout_rect(&self) -> Option<RectF> {
        self.cutout_rect
    }

    /// Source rect being dragged out by the Smart Move tool.
    pub fn move_src_rect(&self) -> Option<RectF> {
        self.move_src_rect
    }

    /// Marquee rectangle preview (Select tool drag on empty canvas).
    pub fn marquee_rect(&self) -> Option<RectF> {
        self.marquee_rect
    }

    /// Region selected by the SelectRegion tool (committed rect, logical px).
    pub fn region(&self) -> Option<RectF> {
        self.doc.region
    }

    // ---- read access ---------------------------------------------------

    pub fn doc(&self) -> &Document {
        &self.doc
    }
    pub fn into_doc(self) -> Document {
        self.doc
    }
    pub fn tool(&self) -> Tool {
        self.tool
    }
    /// Primary (last-clicked) selected object.
    pub fn selected(&self) -> Option<ObjectId> {
        self.selection.last().copied()
    }
    /// All selected ids, primary last.
    pub fn selection(&self) -> &[ObjectId] {
        &self.selection
    }
    pub fn selected_object(&self) -> Option<&Object> {
        self.selected().and_then(|id| self.doc.get(id))
    }
    /// Union of selected objects' bounds (for a multi-selection outline).
    pub fn selection_bounds(&self) -> Option<RectF> {
        self.selection
            .iter()
            .filter_map(|id| self.doc.get(*id))
            .filter(|o| o.visible)
            .map(|o| qs_render::bounds(&o.kind))
            .reduce(|a, b| a.union(&b))
    }
    pub fn edit(&self) -> Option<&TextEdit> {
        self.edit.as_ref()
    }
    pub fn is_dragging(&self) -> bool {
        !matches!(self.drag, Drag::None)
    }
    /// The kind of gesture currently dragged, for pointer-cursor feedback.
    pub fn drag_hint(&self) -> DragHint {
        match &self.drag {
            Drag::None => DragHint::None,
            Drag::Move { .. } => DragHint::Move,
            Drag::Marquee { .. } => DragHint::Marquee,
            Drag::Handle { h, .. } => DragHint::Handle(*h),
            Drag::Create { .. } => DragHint::Create,
            Drag::TextSelect => DragHint::TextSelect,
            Drag::Crop { .. } => DragHint::Crop,
            Drag::Cutout { .. } => DragHint::Cutout,
            Drag::SmartMoveSrc { .. } => DragHint::SmartMove,
            Drag::Region { .. } => DragHint::Region,
        }
    }

    /// Resize handles — only for a single unlocked selection.
    pub fn selection_handles(&self) -> Vec<Handle> {
        if self.selection.len() != 1 {
            return vec![];
        }
        match self.selected_object() {
            Some(o) if !o.locked => hit::handles(&o.kind),
            _ => vec![],
        }
    }

    /// Top-most visible object under `p`.
    pub fn pick(&self, p: PointF) -> Option<ObjectId> {
        self.doc
            .objects
            .iter()
            .rev()
            .find(|o| o.visible && hit::hit(&o.kind, p, self.hit_tol))
            .map(|o| o.id)
    }
}

#[cfg(test)]
mod tests;
