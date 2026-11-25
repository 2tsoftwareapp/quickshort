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
mod tests {
    use super::*;
    use qs_doc::{FontFamily, ObjectKind, Stroke, TextOutline, MARKER_ALPHA};

    fn pt(x: f32, y: f32) -> PointF {
        PointF::new(x, y)
    }

    fn editor() -> Editor {
        Editor::new(Document::new(400, 300, 1.0))
    }

    fn drag(e: &mut Editor, a: PointF, b: PointF) {
        e.pointer_down(a);
        e.pointer_move(b, false);
        e.pointer_up(b);
    }

    fn rect_of(e: &Editor, i: usize) -> RectF {
        match &e.doc().objects[i].kind {
            ObjectKind::Rect { r, .. } => *r,
            k => panic!("{k:?}"),
        }
    }

    fn drawn_rect() -> Editor {
        let mut e = editor();
        e.set_tool(Tool::Rect);
        drag(&mut e, pt(10.0, 10.0), pt(60.0, 40.0));
        e.set_tool(Tool::Select);
        e.select(Some(e.doc().objects[0].id));
        e
    }

    #[test]
    fn rectangles_keep_the_tool_and_undo_individually() {
        let mut e = editor();
        e.set_tool(Tool::Rect);
        drag(&mut e, pt(10.0, 10.0), pt(60.0, 40.0));
        drag(&mut e, pt(70.0, 10.0), pt(120.0, 40.0));
        assert_eq!(e.doc().objects.len(), 2);
        assert_eq!(rect_of(&e, 0), RectF::new(10.0, 10.0, 50.0, 30.0));
        assert_eq!(e.selected(), None);
        assert_eq!(e.tool(), Tool::Rect);
        assert!(e.undo());
        assert_eq!(e.doc().objects.len(), 1);
        assert_eq!(e.tool(), Tool::Rect);
        e.escape();
        assert_eq!(e.tool(), Tool::Select);
        assert!(e.redo());
        assert_eq!(e.doc().objects.len(), 2);
    }

    #[test]
    fn accidental_click_creates_nothing_and_no_undo() {
        let mut e = editor();
        e.set_tool(Tool::Rect);
        drag(&mut e, pt(10.0, 10.0), pt(11.0, 11.0));
        assert!(e.doc().objects.is_empty());
        assert!(!e.can_undo());
    }

    #[test]
    fn move_then_undo() {
        let mut e = drawn_rect();
        drag(&mut e, pt(200.0, 200.0), pt(200.0, 200.0)); // deselect
        assert_eq!(e.selected(), None);
        drag(&mut e, pt(35.0, 25.0), pt(35.0, 25.0)); // inside, unfilled -> miss
        assert_eq!(e.selected(), None, "unfilled interior is click-through");
        drag(&mut e, pt(20.0, 10.0), pt(30.0, 30.0)); // on the top edge
        assert_eq!(rect_of(&e, 0), RectF::new(20.0, 30.0, 50.0, 30.0));
        assert!(e.undo());
        assert_eq!(rect_of(&e, 0), RectF::new(10.0, 10.0, 50.0, 30.0));
    }

    #[test]
    fn handle_resize_se_and_undo() {
        let mut e = drawn_rect();
        let se = e
            .selection_handles()
            .into_iter()
            .find(|h| h.id == HandleId::SE)
            .unwrap();
        assert_eq!(se.pos, pt(60.0, 40.0));
        drag(&mut e, se.pos, pt(100.0, 90.0));
        assert_eq!(rect_of(&e, 0), RectF::new(10.0, 10.0, 90.0, 80.0));
        e.undo();
        assert_eq!(rect_of(&e, 0), RectF::new(10.0, 10.0, 50.0, 30.0));
    }

    #[test]
    fn handle_drag_flips_past_opposite_side() {
        let mut e = drawn_rect();
        drag(&mut e, pt(60.0, 40.0), pt(0.0, 0.0));
        assert_eq!(rect_of(&e, 0), RectF::new(0.0, 0.0, 10.0, 10.0));
    }

    #[test]
    fn pen_is_sticky_and_not_selected() {
        let mut e = editor();
        e.set_tool(Tool::Pen);
        drag(&mut e, pt(5.0, 5.0), pt(50.0, 50.0));
        drag(&mut e, pt(5.0, 60.0), pt(50.0, 90.0));
        assert_eq!(e.tool(), Tool::Pen);
        assert_eq!(e.doc().objects.len(), 2);
        assert_eq!(e.selected(), None);
        e.undo();
        assert_eq!(e.doc().objects.len(), 1);
    }

    #[test]
    fn shift_snaps_arrow_angle_and_squares_rects() {
        let mut e = editor();
        e.set_tool(Tool::Arrow);
        e.pointer_down(pt(0.0, 0.0));
        e.pointer_move(pt(100.0, 10.0), true);
        e.pointer_up(pt(100.0, 10.0));
        match &e.doc().objects[0].kind {
            ObjectKind::Arrow { b, .. } => {
                assert!(b.y.abs() < 0.01 && b.x > 100.0, "{b:?}");
            }
            k => panic!("{k:?}"),
        }
        e.set_tool(Tool::Rect);
        e.pointer_down(pt(10.0, 100.0));
        e.pointer_move(pt(60.0, 120.0), true);
        e.pointer_up(pt(60.0, 120.0));
        let r = rect_of(&e, 1);
        assert_eq!((r.w, r.h), (50.0, 50.0));
    }

    #[test]
    fn text_tool_types_multiline_and_undoes_as_one_step() {
        let mut e = editor();
        e.set_tool(Tool::Text);
        e.pointer_down(pt(20.0, 20.0));
        e.pointer_up(pt(20.0, 20.0));
        assert!(e.edit().is_some());
        e.text_insert("Hi");
        e.text_insert("\nthere");
        e.text_backspace();
        e.end_text_edit();
        assert_eq!(e.doc().objects[0].kind.text(), Some("Hi\nther"));
        assert!(e.undo());
        assert!(
            e.doc().objects.is_empty(),
            "whole text session is one undo step"
        );
        assert!(e.redo());
        assert_eq!(e.doc().objects[0].kind.text(), Some("Hi\nther"));
    }

    #[test]
    fn empty_text_is_discarded_without_undo_entry() {
        let mut e = editor();
        e.set_tool(Tool::Text);
        e.pointer_down(pt(20.0, 20.0));
        e.end_text_edit();
        assert!(e.doc().objects.is_empty());
        assert!(!e.can_undo());
    }

    #[test]
    fn text_and_callout_keep_the_tool_between_annotations() {
        for tool in [Tool::Text, Tool::Callout] {
            let mut e = editor();
            e.set_tool(tool);
            drag(&mut e, pt(20.0, 20.0), pt(20.0, 20.0));
            e.text_insert("First");
            drag(&mut e, pt(250.0, 150.0), pt(250.0, 150.0));
            e.text_insert("Second");
            assert_eq!(e.tool(), tool);
            e.escape();
            assert_eq!(e.tool(), Tool::Select);
            assert_eq!(e.selected(), None);
            assert_eq!(e.doc().objects[0].kind.text(), Some("First"));
            assert_eq!(e.doc().objects[1].kind.text(), Some("Second"));
        }
    }

    #[test]
    fn caret_motion_vietnamese_and_lines() {
        let mut e = editor();
        e.set_tool(Tool::Text);
        e.pointer_down(pt(20.0, 20.0));
        e.text_insert("abc\nđề");
        assert_eq!(e.edit().unwrap().caret, "abc\nđề".len());
        e.text_backspace();
        assert_eq!(e.doc().objects[0].kind.text(), Some("abc\nđ"));
        e.text_insert("ề");
        e.text_move(Motion::Up, false);
        assert!(e.edit().unwrap().caret <= 3);
        e.text_move(Motion::Down, false);
        assert!(e.edit().unwrap().caret >= 4);
        e.text_move(Motion::Home, false);
        assert_eq!(e.edit().unwrap().caret, 4);
        e.text_move(Motion::DocEnd, true);
        assert_eq!(e.text_selection().as_deref(), Some("đề"));
        e.text_insert("x");
        assert_eq!(e.doc().objects[0].kind.text(), Some("abc\nx"));
        e.text_move(Motion::Left, false);
        e.text_delete();
        assert_eq!(e.doc().objects[0].kind.text(), Some("abc\n"));
    }

    #[test]
    fn callout_drag_edits_text_and_grows() {
        let mut e = editor();
        e.set_tool(Tool::Callout);
        drag(&mut e, pt(100.0, 100.0), pt(220.0, 150.0));
        assert!(e.edit().is_some(), "callout opens straight into text edit");
        let body0 = match &e.doc().objects[0].kind {
            ObjectKind::Callout { body, tail, .. } => {
                assert!(tail.y > body.bottom());
                *body
            }
            k => panic!("{k:?}"),
        };
        assert_eq!(body0, RectF::new(100.0, 100.0, 120.0, 50.0));
        e.text_insert("one two three four five six seven eight nine ten eleven twelve");
        e.end_text_edit();
        match &e.doc().objects[0].kind {
            ObjectKind::Callout { body, .. } => assert!(body.h > body0.h, "{body:?}"),
            k => panic!("{k:?}"),
        }
        assert!(e.undo());
        assert!(e.doc().objects.is_empty());
    }

    #[test]
    fn callout_click_gets_default_size_and_double_click_edits() {
        let mut e = editor();
        e.set_tool(Tool::Callout);
        drag(&mut e, pt(50.0, 50.0), pt(50.0, 50.0));
        match &e.doc().objects[0].kind {
            ObjectKind::Callout { body, .. } => assert_eq!((body.w, body.h), DEFAULT_CALLOUT),
            k => panic!("{k:?}"),
        }
        e.text_insert("hello");
        e.end_text_edit();
        e.set_tool(Tool::Select);
        e.double_click(pt(100.0, 70.0));
        assert!(e.edit().is_some());
        e.text_insert("!");
        e.end_text_edit();
        assert_eq!(e.doc().objects[0].kind.text(), Some("hello!"));
    }

    #[test]
    fn callout_tail_handle_moves_tail_only() {
        let mut e = editor();
        e.set_tool(Tool::Callout);
        drag(&mut e, pt(100.0, 100.0), pt(220.0, 150.0));
        e.end_text_edit();
        e.set_tool(Tool::Select);
        e.select(Some(e.doc().objects[0].id));
        let tail = e
            .selection_handles()
            .into_iter()
            .find(|h| h.id == HandleId::Tail)
            .unwrap();
        drag(&mut e, tail.pos, pt(10.0, 10.0));
        match &e.doc().objects[0].kind {
            ObjectKind::Callout { body, tail, .. } => {
                assert_eq!(*tail, pt(10.0, 10.0));
                assert_eq!(*body, RectF::new(100.0, 100.0, 120.0, 50.0));
            }
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn text_width_handle_sets_wrap() {
        let mut e = editor();
        e.set_tool(Tool::Text);
        e.pointer_down(pt(10.0, 10.0));
        e.text_insert("hello wide world of words");
        e.end_text_edit();
        e.set_tool(Tool::Select);
        e.select(Some(e.doc().objects[0].id));
        let east = e
            .selection_handles()
            .into_iter()
            .find(|h| h.id == HandleId::E)
            .unwrap();
        drag(&mut e, east.pos, pt(80.0, east.pos.y));
        match &e.doc().objects[0].kind {
            ObjectKind::Text { wrap_w, .. } => assert_eq!(*wrap_w, Some(70.0)),
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn pen_resize_scales_points() {
        let k = ObjectKind::Pen {
            pts: vec![pt(0.0, 0.0), pt(10.0, 10.0), pt(20.0, 0.0)],
            stroke: Stroke::new(0, 2.0),
        };
        match hit::resized(&k, HandleId::SE, pt(40.0, 20.0)) {
            ObjectKind::Pen { pts, .. } => {
                assert_eq!(pts, vec![pt(0.0, 0.0), pt(20.0, 20.0), pt(40.0, 0.0)]);
            }
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn steps_number_and_renumber() {
        let mut e = editor();
        e.set_tool(Tool::Step);
        e.set_color(0xFF0000);
        e.pointer_down(pt(20.0, 20.0));
        e.pointer_up(pt(20.0, 20.0));
        e.set_color(0x0000FF);
        e.pointer_down(pt(60.0, 20.0));
        e.pointer_up(pt(60.0, 20.0));
        assert!(e.undo());
        assert_eq!(e.tool(), Tool::Step);
        e.pointer_down(pt(70.0, 30.0));
        e.pointer_up(pt(70.0, 30.0));
        let (a, b) = (e.doc().objects[0].id, e.doc().objects[1].id);
        assert_eq!(e.doc().step_number(a), Some(1));
        assert_eq!(e.doc().step_number(b), Some(2));
        assert!(matches!(
            e.doc().objects[0].kind,
            ObjectKind::Step { rgb: 0xFF0000, .. }
        ));
        assert!(matches!(
            e.doc().objects[1].kind,
            ObjectKind::Step { rgb: 0x0000FF, .. }
        ));
        e.set_tool(Tool::Select);
        e.select(Some(a));
        assert!(e.delete_selected());
        assert_eq!(e.doc().step_number(b), Some(1));
        e.undo();
        assert_eq!(e.doc().step_number(b), Some(2));
    }

    #[test]
    fn layer_visibility_lock_reorder_and_undo() {
        let mut e = two_rects();
        let low = e.doc().objects[0].id;
        let high = e.doc().objects[1].id;
        assert!(e.set_visible(low, false));
        assert!(!e.doc().get(low).unwrap().visible);
        assert!(e.undo());
        assert!(e.doc().get(low).unwrap().visible);
        assert!(e.redo());
        assert!(!e.doc().get(low).unwrap().visible);

        assert!(e.set_locked(high, true));
        assert!(e.doc().get(high).unwrap().locked);
        assert!(e.undo());
        assert!(!e.doc().get(high).unwrap().locked);

        assert!(e.move_layer(high, 0));
        assert_eq!(e.doc().objects[0].id, high);
        assert!(e.undo());
        assert_eq!(e.doc().objects[0].id, low);
        assert!(e.redo());
        assert_eq!(e.doc().objects[0].id, high);
    }

    fn three_rects() -> Editor {
        let mut e = editor();
        e.set_tool(Tool::Rect);
        for (x, y) in [(10.0, 10.0), (50.0, 35.0), (120.0, 80.0)] {
            drag(&mut e, pt(x, y), pt(x + 20.0, y + 10.0));
        }
        e.set_tool(Tool::Select);
        let ids = e.doc().objects.iter().map(|o| o.id).collect::<Vec<_>>();
        e.select_many(ids);
        e
    }

    #[test]
    fn alignment_uses_union_edges_and_one_undo_step() {
        let mut e = three_rects();
        assert!(e.align_selected(Alignment::Left));
        let b = e
            .doc()
            .objects
            .iter()
            .map(|o| qs_render::bounds(&o.kind))
            .collect::<Vec<_>>();
        assert!(b.iter().all(|r| (r.x - b[0].x).abs() < 0.01));
        assert!(e.undo());
        assert_eq!(rect_of(&e, 0), RectF::new(10.0, 10.0, 20.0, 10.0));
        assert!(e.redo());
    }

    #[test]
    fn center_and_bottom_alignment_are_correct() {
        let mut e = three_rects();
        assert!(e.align_selected(Alignment::HCenter));
        let bounds = e
            .doc()
            .objects
            .iter()
            .map(|o| qs_render::bounds(&o.kind))
            .collect::<Vec<_>>();
        assert!(bounds
            .iter()
            .all(|r| (r.center().x - bounds[0].center().x).abs() < 0.01));
        assert!(e.align_selected(Alignment::Bottom));
        let bounds = e
            .doc()
            .objects
            .iter()
            .map(|o| qs_render::bounds(&o.kind))
            .collect::<Vec<_>>();
        assert!(bounds
            .iter()
            .all(|r| (r.bottom() - bounds[0].bottom()).abs() < 0.01));
    }

    #[test]
    fn horizontal_distribution_evenly_spaces_centers_and_undoes() {
        let mut e = three_rects();
        assert!(e.distribute_selected(Axis::Horizontal));
        let mut xs = e
            .doc()
            .objects
            .iter()
            .map(|o| qs_render::bounds(&o.kind).center().x)
            .collect::<Vec<_>>();
        xs.sort_by(f32::total_cmp);
        assert!(((xs[1] - xs[0]) - (xs[2] - xs[1])).abs() < 0.01);
        assert!(e.undo());
        assert_eq!(rect_of(&e, 1), RectF::new(50.0, 35.0, 20.0, 10.0));
        assert!(e.redo());
    }

    #[test]
    fn align_and_distribute_enforce_minimum_counts_and_locked_filter() {
        let mut e = three_rects();
        let first = e.doc().objects[0].id;
        e.select_many([first]);
        assert!(!e.align_selected(Alignment::Left));
        assert!(!e.distribute_selected(Axis::Vertical));
        let ids = e.doc().objects.iter().map(|o| o.id).collect::<Vec<_>>();
        e.select_many(ids);
        e.set_locked(first, true);
        assert!(e.align_selected(Alignment::Right));
        assert_eq!(
            rect_of(&e, 0),
            RectF::new(10.0, 10.0, 20.0, 10.0),
            "locked objects do not move"
        );
    }

    #[test]
    fn z_order_duplicate_delete() {
        let mut e = editor();
        e.set_tool(Tool::Rect);
        for i in 0..3 {
            let x = 10.0 + 70.0 * i as f32;
            drag(&mut e, pt(x, 10.0), pt(x + 50.0, 60.0));
        }
        let ids: Vec<_> = e.doc().objects.iter().map(|o| o.id).collect();
        e.set_tool(Tool::Select);
        e.select(Some(ids[0]));
        assert!(e.reorder(ZOrder::Front));
        assert_eq!(e.doc().objects.last().unwrap().id, ids[0]);
        assert!(e.reorder(ZOrder::Backward));
        assert_eq!(e.doc().objects[1].id, ids[0]);
        let dup = e.duplicate_selected().unwrap();
        assert_eq!(e.selected(), Some(dup));
        assert_eq!(e.doc().objects.len(), 4);
        assert!(e.delete_selected());
        assert_eq!(e.doc().objects.len(), 3);
        e.undo();
        e.undo();
        assert_eq!(e.doc().objects.len(), 3);
    }

    #[test]
    fn locked_object_cannot_move_or_delete() {
        let mut e = drawn_rect();
        let id = e.doc().objects[0].id;
        e.set_locked(id, true);
        assert!(!e.nudge(5.0, 5.0));
        assert!(!e.delete_selected());
        drag(&mut e, pt(20.0, 10.0), pt(80.0, 80.0));
        assert_eq!(rect_of(&e, 0), RectF::new(10.0, 10.0, 50.0, 30.0));
        assert!(e.selection_handles().is_empty());
    }

    #[test]
    fn color_width_fill_apply_to_selection_and_defaults() {
        let mut e = drawn_rect();
        e.set_color(0x00FF00);
        e.set_stroke_width(6.0);
        e.set_fill(Fill::Solid {
            rgb: 0x0000FF,
            alpha: 0.5,
        });
        match &e.doc().objects[0].kind {
            ObjectKind::Rect { stroke, fill, .. } => {
                assert_eq!((stroke.rgb, stroke.width), (0x00FF00, 6.0));
                assert_eq!(
                    *fill,
                    Fill::Solid {
                        rgb: 0x0000FF,
                        alpha: 0.5
                    }
                );
            }
            k => panic!("{k:?}"),
        }
        assert_eq!(e.style.rgb, 0x00FF00);
        e.undo();
        e.undo();
        e.undo();
        match &e.doc().objects[0].kind {
            ObjectKind::Rect { stroke, fill, .. } => {
                assert_eq!(stroke.rgb, PALETTE[0]);
                assert!(fill.is_none());
            }
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn independent_fill_and_stroke_on_new_objects() {
        let mut e = editor();
        e.set_color(0x112233);
        e.set_fill(Fill::Solid {
            rgb: 0xFFFF00,
            alpha: 1.0,
        });
        e.set_tool(Tool::Ellipse);
        drag(&mut e, pt(10.0, 10.0), pt(80.0, 60.0));
        match &e.doc().objects[0].kind {
            ObjectKind::Ellipse { stroke, fill, .. } => {
                assert_eq!(stroke.rgb, 0x112233);
                assert_eq!(
                    *fill,
                    Fill::Solid {
                        rgb: 0xFFFF00,
                        alpha: 1.0
                    }
                );
            }
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn highlighter_width_scales_and_keeps_alpha() {
        let mut e = editor();
        e.set_tool(Tool::Highlighter);
        drag(&mut e, pt(5.0, 5.0), pt(90.0, 5.0));
        e.select(Some(e.doc().objects[0].id));
        e.set_stroke_width(6.0);
        match &e.doc().objects[0].kind {
            ObjectKind::Highlighter { stroke, .. } => {
                assert!((stroke.width - 6.0 * MARKER_K).abs() < 1e-4);
                assert_eq!(stroke.alpha, MARKER_ALPHA);
            }
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn crop_is_one_undo_step() {
        let mut e = editor();
        e.set_view(RectF::new(10.0, 10.0, 100.0, 80.0));
        assert_eq!(e.doc().view, RectF::new(10.0, 10.0, 100.0, 80.0));
        e.set_view(RectF::new(0.0, 0.0, 0.0, 0.0)); // rejected
        assert_eq!(e.doc().view.w, 100.0);
        e.undo();
        assert_eq!(e.doc().view, RectF::new(0.0, 0.0, 400.0, 300.0));
    }

    #[test]
    fn redo_cleared_by_new_edit() {
        let mut e = drawn_rect();
        e.undo();
        assert!(e.can_redo());
        e.set_tool(Tool::Ellipse);
        drag(&mut e, pt(10.0, 10.0), pt(50.0, 50.0));
        assert!(!e.can_redo());
    }

    #[test]
    fn hit_tests_fill_edge_and_tail() {
        let st = Stroke::new(0, 2.0);
        let r = ObjectKind::Rect {
            r: RectF::new(0.0, 0.0, 100.0, 50.0),
            stroke: st,
            fill: Fill::None,
            radius: 0.0,
        };
        assert!(hit::hit(&r, pt(50.0, 0.0), 3.0));
        assert!(!hit::hit(&r, pt(50.0, 25.0), 3.0));
        let f = ObjectKind::Rect {
            r: RectF::new(0.0, 0.0, 100.0, 50.0),
            stroke: st,
            fill: Fill::Solid { rgb: 0, alpha: 1.0 },
            radius: 0.0,
        };
        assert!(hit::hit(&f, pt(50.0, 25.0), 3.0));
        let c = ObjectKind::Callout {
            shape: CalloutShape::RoundRect,
            body: RectF::new(0.0, 0.0, 100.0, 50.0),
            tail: pt(50.0, 120.0),
            stroke: st,
            fill: Fill::None,
            text: String::new(),
            style: TextStyle::default(),
        };
        assert!(
            hit::hit(&c, pt(50.0, 25.0), 3.0),
            "callout interior is hittable"
        );
        assert!(hit::hit(&c, pt(50.0, 118.0), 3.0), "tail tip");
        assert!(!hit::hit(&c, pt(200.0, 200.0), 3.0));
    }

    #[test]
    fn shape_arrow_blur_setters_update_selection_and_undo() {
        let mut e = editor();
        e.set_tool(Tool::Arrow);
        drag(&mut e, pt(10.0, 10.0), pt(80.0, 10.0));
        e.set_tool(Tool::Select);
        e.select(Some(e.doc().objects[0].id));
        e.set_arrow_head(ArrowHead::Filled);
        assert!(matches!(
            e.doc().objects[0].kind,
            ObjectKind::Arrow {
                head: ArrowHead::Filled,
                ..
            }
        ));
        assert!(e.undo());
        assert!(matches!(
            e.doc().objects[0].kind,
            ObjectKind::Arrow {
                head: ArrowHead::Open,
                ..
            }
        ));

        e.set_tool(Tool::Callout);
        drag(&mut e, pt(20.0, 40.0), pt(120.0, 90.0));
        e.end_text_edit();
        e.select(Some(e.doc().objects[1].id));
        e.set_callout_shape(CalloutShape::Pill);
        assert!(matches!(
            e.doc().objects[1].kind,
            ObjectKind::Callout {
                shape: CalloutShape::Pill,
                ..
            }
        ));
        assert!(e.undo());
        assert!(matches!(
            e.doc().objects[1].kind,
            ObjectKind::Callout {
                shape: CalloutShape::RoundRect,
                ..
            }
        ));

        e.set_tool(Tool::Blur);
        drag(&mut e, pt(20.0, 120.0), pt(100.0, 160.0));
        e.set_tool(Tool::Select);
        e.select(Some(e.doc().objects[2].id));
        e.set_blur_mode(BlurMode::Gaussian);
        e.set_blur_strength(18.0);
        match &e.doc().objects[2].kind {
            ObjectKind::Blur { mode, strength, .. } => {
                assert_eq!(*mode, BlurMode::Gaussian);
                assert_eq!(*strength, 18.0);
            }
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn fit_canvas_to_objects_expands_for_visible_annotations_in_one_step() {
        let mut doc = Document::new(400, 300, 1.0);
        doc.push(ObjectKind::Rect {
            r: RectF::new(-10.0, -20.0, 30.0, 40.0),
            stroke: Stroke::new(0, 2.0),
            fill: Fill::None,
            radius: 0.0,
        });
        let hidden = doc.push(ObjectKind::Rect {
            r: RectF::new(-500.0, -500.0, 20.0, 20.0),
            stroke: Stroke::new(0, 2.0),
            fill: Fill::None,
            radius: 0.0,
        });
        doc.get_mut(hidden).unwrap().visible = false;
        let mut e = Editor::new(doc);
        assert!(e.fit_canvas_to_objects());
        assert_eq!(e.doc().view, RectF::new(-11.0, -21.0, 411.0, 321.0));
        assert!(e.undo());
        assert_eq!(e.doc().view, RectF::new(0.0, 0.0, 400.0, 300.0));
        assert!(!e.can_undo());
        assert!(e.redo());
        assert_eq!(e.doc().view, RectF::new(-11.0, -21.0, 411.0, 321.0));
    }

    #[test]
    fn fit_canvas_to_objects_and_background_share_one_undo_step() {
        let mut doc = Document::new(400, 300, 1.0);
        doc.push(ObjectKind::Rect {
            r: RectF::new(-10.0, -20.0, 30.0, 40.0),
            stroke: Stroke::new(0, 2.0),
            fill: Fill::None,
            radius: 0.0,
        });
        let mut e = Editor::new(doc);
        assert!(e.fit_canvas_to_objects_with_background(0xFF123456));
        assert_eq!(e.doc().view, RectF::new(-11.0, -21.0, 411.0, 321.0));
        assert_eq!(e.doc().background, 0xFF123456);
        assert!(e.undo());
        assert_eq!(e.doc().view, RectF::new(0.0, 0.0, 400.0, 300.0));
        assert_eq!(e.doc().background, 0xFFFF_FFFF);
        assert!(!e.can_undo());
    }

    #[test]
    fn fit_canvas_to_objects_is_noop_when_objects_are_inside_canvas() {
        let mut doc = Document::new(400, 300, 1.0);
        doc.push(ObjectKind::Rect {
            r: RectF::new(10.0, 10.0, 50.0, 30.0),
            stroke: Stroke::new(0, 2.0),
            fill: Fill::None,
            radius: 0.0,
        });
        let mut e = Editor::new(doc);
        assert!(!e.fit_canvas_to_objects());
        assert_eq!(e.doc().view, RectF::new(0.0, 0.0, 400.0, 300.0));
        assert!(!e.can_undo());
    }

    #[test]
    fn resize_canvas_updates_background_in_one_undo_step() {
        let mut e = editor();
        assert!(e.update_canvas(10.0, 20.0, 30.0, 40.0, 0xFF123456));
        assert_eq!(e.doc().view, RectF::new(-40.0, -10.0, 460.0, 340.0));
        assert_eq!(e.doc().background, 0xFF123456);
        assert!(e.undo());
        assert_eq!(e.doc().view, RectF::new(0.0, 0.0, 400.0, 300.0));
        assert_eq!(e.doc().background, 0xFFFF_FFFF);
        assert!(!e.can_undo());
        assert!(e.redo());
        assert_eq!(e.doc().view, RectF::new(-40.0, -10.0, 460.0, 340.0));
        assert_eq!(e.doc().background, 0xFF123456);
    }

    fn two_rects() -> Editor {
        let mut e = editor();
        e.set_tool(Tool::Rect);
        drag(&mut e, pt(10.0, 10.0), pt(60.0, 40.0));
        drag(&mut e, pt(100.0, 100.0), pt(150.0, 140.0));
        e.set_tool(Tool::Select);
        e
    }

    #[test]
    fn marquee_selects_all_intersecting_objects() {
        let mut e = two_rects();
        e.pointer_down_mod(pt(0.0, 0.0), false);
        e.pointer_move(pt(200.0, 200.0), false);
        e.pointer_up(pt(200.0, 200.0));
        assert_eq!(e.selection().len(), 2);
        assert_eq!(e.doc().objects.len(), 2);
    }

    #[test]
    fn marquee_partial_overlap_selects_and_shift_adds() {
        let mut e = two_rects();
        // Marquee covering only the first rect.
        e.pointer_down_mod(pt(0.0, 0.0), false);
        e.pointer_move(pt(70.0, 50.0), false);
        e.pointer_up(pt(70.0, 50.0));
        assert_eq!(e.selection().len(), 1);
        // Shift+marquee over the second adds it.
        e.pointer_down_mod(pt(90.0, 90.0), true);
        e.pointer_move(pt(200.0, 200.0), true);
        e.pointer_up(pt(200.0, 200.0));
        assert_eq!(e.selection().len(), 2);
    }

    #[test]
    fn shift_click_toggles_membership() {
        let mut e = two_rects();
        let ids: Vec<_> = e.doc().objects.iter().map(|o| o.id).collect();
        e.select_many(ids);
        assert_eq!(e.selection().len(), 2);
        let id = e.doc().objects[0].id;
        e.pointer_down_mod(pt(10.0, 25.0), true); // on the stroke edge
        e.pointer_up(pt(10.0, 25.0));
        assert_eq!(e.selection().len(), 1);
        assert_eq!(e.selected(), Some(e.doc().objects[1].id));
        let _ = id;
    }

    #[test]
    fn multi_move_delete_and_color_are_single_undo_steps() {
        let mut e = two_rects();
        e.pointer_down_mod(pt(0.0, 0.0), false);
        e.pointer_move(pt(200.0, 200.0), false);
        e.pointer_up(pt(200.0, 200.0));
        assert_eq!(e.selection().len(), 2);

        // Move both by dragging the first rect's stroke edge.
        e.pointer_down(pt(10.0, 25.0));
        e.pointer_move(pt(20.0, 35.0), false);
        e.pointer_up(pt(20.0, 35.0));
        assert_eq!(rect_of(&e, 0), RectF::new(20.0, 20.0, 50.0, 30.0));
        assert_eq!(rect_of(&e, 1), RectF::new(110.0, 110.0, 50.0, 40.0));
        assert!(e.undo(), "one undo reverts the whole multi-move");
        assert_eq!(rect_of(&e, 0), RectF::new(10.0, 10.0, 50.0, 30.0));
        assert_eq!(rect_of(&e, 1), RectF::new(100.0, 100.0, 50.0, 40.0));
        assert!(e.redo());

        // Color applies to every selected object.
        e.set_color(0xFF0000);
        for o in &e.doc().objects {
            match &o.kind {
                ObjectKind::Rect { stroke, .. } => assert_eq!(stroke.rgb, 0xFF0000),
                k => panic!("{k:?}"),
            }
        }
        assert!(e.undo());
        match &e.doc().objects[0].kind {
            ObjectKind::Rect { stroke, .. } => assert_ne!(stroke.rgb, 0xFF0000),
            _ => unreachable!(),
        }

        // Delete removes all selected in one step.
        assert!(e.delete_selected());
        assert!(e.doc().objects.is_empty());
        assert!(e.undo());
        assert_eq!(e.doc().objects.len(), 2);
    }

    #[test]
    fn resize_canvas_rejects_non_finite_or_empty_dimensions() {
        let mut e = editor();
        assert!(!e.update_canvas(0.0, 0.0, 0.0, -400.0, 0xFF000000));
        assert!(!e.update_canvas(f32::NAN, 0.0, 0.0, 0.0, 0xFF000000));
        assert_eq!(e.doc().view, RectF::new(0.0, 0.0, 400.0, 300.0));
        assert_eq!(e.doc().background, 0xFFFF_FFFF);
        assert!(!e.can_undo());
    }

    #[test]
    fn crop_drag_commits_view_and_undoes() {
        let mut e = editor();
        e.set_tool(Tool::Crop);
        drag(&mut e, pt(10.0, 10.0), pt(110.0, 80.0));
        assert_eq!(e.doc().view, RectF::new(10.0, 10.0, 100.0, 70.0));
        assert_eq!(e.crop_rect(), None);
        assert!(e.undo());
        assert_eq!(e.doc().view, RectF::new(0.0, 0.0, 400.0, 300.0));
        assert!(e.redo());
        assert_eq!(e.doc().view, RectF::new(10.0, 10.0, 100.0, 70.0));
    }

    #[test]
    fn crop_drag_below_minimum_is_ignored() {
        let mut e = editor();
        e.set_tool(Tool::Crop);
        drag(&mut e, pt(10.0, 10.0), pt(10.0, 10.0));
        assert_eq!(e.doc().view, RectF::new(0.0, 0.0, 400.0, 300.0));
        assert!(!e.can_undo());
        assert_eq!(e.crop_rect(), None);
    }

    #[test]
    fn region_drag_commits_and_undoes() {
        let mut e = editor();
        e.set_tool(Tool::SelectRegion);
        drag(&mut e, pt(10.0, 10.0), pt(110.0, 80.0));
        assert_eq!(e.doc().region, Some(RectF::new(10.0, 10.0, 100.0, 70.0)));
        assert_eq!(e.tool(), Tool::Select);
        assert!(e.undo());
        assert_eq!(e.doc().region, None);
        assert!(e.redo());
        assert_eq!(e.doc().region, Some(RectF::new(10.0, 10.0, 100.0, 70.0)));
    }

    #[test]
    fn region_drag_below_minimum_is_ignored() {
        let mut e = editor();
        e.set_tool(Tool::SelectRegion);
        drag(&mut e, pt(10.0, 10.0), pt(10.0, 10.0));
        assert_eq!(e.doc().region, None);
        assert!(!e.can_undo());
    }

    #[test]
    fn rotate_and_flip_are_single_undo_steps() {
        use qs_doc::Orientation;
        let mut e = drawn_rect();
        let id = e.doc().objects[0].id;
        let before = e.doc().get(id).unwrap().kind.clone();
        let rev = e.revision();
        e.rotate_cw();
        assert_eq!(e.doc().orientation, Orientation::Rotate90);
        assert!(e.revision() != rev);
        e.flip_h();
        assert_eq!(e.doc().orientation, Orientation::Transpose);
        assert_eq!(
            e.doc().get(id).unwrap().kind,
            before,
            "objects are untouched"
        );
        assert!(e.undo());
        assert_eq!(e.doc().orientation, Orientation::Rotate90);
        assert!(e.undo());
        assert_eq!(e.doc().orientation, Orientation::Normal);
        assert_eq!(e.doc().objects.len(), 1, "third undo would remove the rect");
        assert!(e.redo());
        assert_eq!(e.doc().orientation, Orientation::Rotate90);
    }

    #[test]
    fn four_rotations_and_double_flips_preserve_document() {
        let mut e = drawn_rect();
        let start = e.doc().clone();
        for _ in 0..4 {
            e.rotate_cw();
        }
        assert_eq!(e.doc(), &start);
        e.flip_h();
        e.flip_h();
        e.flip_v();
        e.rotate_ccw();
        e.rotate_cw();
        e.flip_v();
        assert_eq!(e.doc(), &start);
    }

    #[test]
    fn moving_a_group_layer_keeps_the_group_contiguous() {
        let mut e = three_rects();
        let ids = e.doc().objects.iter().map(|o| o.id).collect::<Vec<_>>();
        e.select_many([ids[0], ids[1]]);
        assert!(e.group_selected());
        e.select(None);

        // Bring the group to the front: both members end up above the third.
        e.select(Some(ids[0]));
        assert!(e.reorder(ZOrder::Front));
        let order = e.doc().objects.iter().map(|o| o.id).collect::<Vec<_>>();
        assert_eq!(order, vec![ids[2], ids[0], ids[1]]);
        assert!(e.undo());
        assert_eq!(
            e.doc().objects.iter().map(|o| o.id).collect::<Vec<_>>(),
            ids
        );

        // Free object to the bottom, then the group back under it: the group
        // moves as a block and stays adjacent.
        e.select(Some(ids[2]));
        assert!(e.move_layer(ids[2], 0));
        assert_eq!(
            e.doc().objects.iter().map(|o| o.id).collect::<Vec<_>>(),
            vec![ids[2], ids[0], ids[1]]
        );
        e.select(Some(ids[1]));
        assert!(e.move_layer(ids[1], 0));
        assert_eq!(
            e.doc().objects.iter().map(|o| o.id).collect::<Vec<_>>(),
            vec![ids[0], ids[1], ids[2]]
        );
        assert!(!e.move_layer(ids[1], 0), "already there");
    }

    #[test]
    fn grouping_selects_whole_group_and_undoes_as_one_step() {
        let mut e = three_rects();
        assert!(e.group_selected());
        let group = e.selected_group().expect("selection is grouped");
        assert_eq!(e.doc().group_members(group).len(), 3);
        assert_eq!(e.selection().len(), 3);

        // clicking any member selects the whole group
        let member = e.doc().objects[1].id;
        e.select(None);
        e.select(Some(member));
        assert_eq!(e.selection().len(), 3, "group members select together");

        // dragging one member moves all of them, in one undo step
        let before: Vec<RectF> = (0..3).map(|i| rect_of(&e, i)).collect();
        drag(&mut e, pt(12.0, 12.0), pt(22.0, 20.0));
        for (i, b) in before.iter().enumerate() {
            assert_eq!(rect_of(&e, i), RectF::new(b.x + 10.0, b.y + 8.0, b.w, b.h));
        }
        assert!(e.undo());
        for (i, b) in before.iter().enumerate() {
            assert_eq!(rect_of(&e, i), *b, "one undo step restores the group");
        }
        assert!(
            e.selection_grouped(),
            "undo keeps the group and its selection"
        );

        // ungroup breaks the group, also one step
        assert!(e.ungroup_selected());
        assert!(!e.selection_grouped());
        assert!(!e.ungroup_selected(), "nothing left to ungroup");
        assert!(e.undo());
        assert!(e.selection_grouped());
    }

    #[test]
    fn grouping_needs_two_objects_and_merges_existing_groups() {
        let mut e = three_rects();
        let ids = e.doc().objects.iter().map(|o| o.id).collect::<Vec<_>>();
        e.select_many([ids[0]]);
        assert!(!e.group_selected(), "a single object is not a group");

        e.select_many([ids[0], ids[1]]);
        assert!(e.group_selected());
        let g1 = e.selected_group().unwrap();
        e.select_many([ids[1], ids[2]]);
        assert!(e.group_selected(), "grouping one object plus a group works");
        let g2 = e.selected_group().unwrap();
        assert_ne!(g1, g2, "merging allocates a new group id");
        assert_eq!(e.doc().group_members(g2).len(), 3);
        assert_eq!(e.doc().group_members(g1), Vec::new());
        assert_eq!(e.selection().len(), 3);
    }

    #[test]
    fn groups_align_as_a_unit_and_keep_their_internal_layout() {
        let mut e = three_rects();
        let ids = e.doc().objects.iter().map(|o| o.id).collect::<Vec<_>>();
        e.select_many([ids[0], ids[1]]);
        assert!(e.group_selected());
        e.select_many([ids[0], ids[1], ids[2]]);
        let before = (rect_of(&e, 0), rect_of(&e, 1), rect_of(&e, 2));
        assert!(e.align_selected(Alignment::Left));
        let (a, b, c) = (rect_of(&e, 0), rect_of(&e, 1), rect_of(&e, 2));
        assert_eq!(a.x, c.x, "group snaps to the union's left edge");
        assert_eq!(
            b.x - a.x,
            before.1.x - before.0.x,
            "gap inside the group stays"
        );
        assert_eq!(b.y - a.y, before.1.y - before.0.y);
        assert!(e.undo());
        assert_eq!(rect_of(&e, 0), before.0);
    }

    #[test]
    fn marquee_selects_whole_groups_and_duplicates_get_a_fresh_group() {
        let mut e = three_rects();
        let ids = e.doc().objects.iter().map(|o| o.id).collect::<Vec<_>>();
        e.select_many([ids[0], ids[1]]);
        assert!(e.group_selected());
        e.select(None);
        drag(&mut e, pt(5.0, 5.0), pt(200.0, 110.0));
        assert_eq!(e.selection().len(), 3, "marquee expands touched groups");

        e.select_many([ids[0]]);
        let copy = e.duplicate_selected().unwrap();
        assert_eq!(e.selection().len(), 2, "copies of a group stay together");
        let src = e.doc().group_of(ids[0]).unwrap();
        let dup = e.doc().group_of(copy).unwrap();
        assert_ne!(src, dup, "copies never join the original group");
    }

    #[test]
    fn typography_setters_update_the_selection_and_the_tool_defaults() {
        let mut e = editor();
        e.set_tool(Tool::Text);
        e.pointer_down(pt(20.0, 20.0));
        e.pointer_up(pt(20.0, 20.0));
        e.text_insert("Tiếng Việt");
        e.end_text_edit();
        let id = e.doc().objects[0].id;
        e.select(Some(id));

        e.set_text_italic(true);
        e.set_text_underline(true);
        e.set_text_strike(true);
        e.set_text_family(FontFamily::Mono);
        e.set_text_outline(Some(TextOutline {
            width: 3.0,
            rgb: 0x000000,
        }));

        match &e.doc().get(id).unwrap().kind {
            ObjectKind::Text { style, .. } => {
                assert!(style.italic && style.underline && style.strike);
                assert_eq!(style.family, FontFamily::Mono);
                assert_eq!(style.outline, Some(TextOutline { width: 3.0, rgb: 0 }));
            }
            k => panic!("{k:?}"),
        }
        assert!(e.style.text.italic, "new text inherits the style");
        assert_eq!(e.style.text.family, FontFamily::Mono);
        assert_eq!(e.current_style().text.outline.unwrap().width, 3.0);

        for _ in 0..5 {
            assert!(e.undo());
        }
        match &e.doc().get(id).unwrap().kind {
            ObjectKind::Text { style, .. } => {
                assert!(!style.italic && !style.underline && !style.strike);
                assert_eq!(style.family, FontFamily::Sans);
                assert_eq!(style.outline, None);
            }
            k => panic!("{k:?}"),
        }
        assert!(e.doc().objects.len() == 1, "undo keeps the text object");
    }

    #[test]
    fn text_style_drag_is_one_undo_step() {
        let mut e = editor();
        e.set_tool(Tool::Text);
        e.pointer_down(pt(20.0, 20.0));
        e.pointer_up(pt(20.0, 20.0));
        e.text_insert("Outline");
        e.end_text_edit();
        let id = e.doc().objects[0].id;
        e.select(Some(id));

        let base = e.current_style().text;
        for w in [1.0, 2.0, 4.0, 6.0] {
            let mut st = base;
            st.outline = Some(TextOutline {
                width: w,
                rgb: 0xFF0000,
            });
            assert!(e.preview_text_style(st));
        }
        e.finish_text_style();
        match &e.doc().get(id).unwrap().kind {
            ObjectKind::Text { style, .. } => assert_eq!(style.outline.unwrap().width, 6.0),
            k => panic!("{k:?}"),
        }
        assert!(e.undo());
        match &e.doc().get(id).unwrap().kind {
            ObjectKind::Text { style, .. } => assert_eq!(style.outline, None),
            k => panic!("{k:?}"),
        }
        assert!(e.redo());
    }

    #[test]
    fn opacity_applies_to_the_selection_clamps_and_undoes() {
        let mut e = three_rects();
        assert!((e.selected_opacity() - 1.0).abs() < f32::EPSILON);
        assert!(e.set_opacity(0.4));
        for o in e.doc().objects.iter() {
            assert!((o.opacity - 0.4).abs() < f32::EPSILON);
        }
        assert!(!e.set_opacity(0.4), "no change, no undo entry");
        assert!(e.set_opacity(2.0), "clamped to 1.0 is still a change");
        assert!((e.doc().objects[0].opacity - 1.0).abs() < f32::EPSILON);
        assert!(e.undo());
        assert!((e.doc().objects[0].opacity - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn opacity_drag_is_a_single_undo_step() {
        let mut e = three_rects();
        for v in [0.9, 0.7, 0.5, 0.3] {
            assert!(e.preview_opacity(v));
        }
        e.finish_opacity();
        assert!((e.doc().objects[0].opacity - 0.3).abs() < f32::EPSILON);
        assert!(e.undo());
        assert!((e.doc().objects[0].opacity - 1.0).abs() < f32::EPSILON);
        assert!(e.redo());
        assert!(
            (e.doc().objects[0].opacity - 0.3).abs() < f32::EPSILON,
            "the whole drag was one entry"
        );
    }
    #[test]
    fn fill_is_one_undo_step_and_uses_the_style_colour() {
        let mut e = editor();
        e.style.rgb = 0x00FF00;
        e.style.fill_tolerance = 40;
        e.set_tool(Tool::Fill);

        e.pointer_down(pt(10.0, 10.0));
        e.pointer_up(pt(10.0, 10.0));

        match &e.doc().objects[0].kind {
            ObjectKind::Fill { rgb, tolerance, .. } => {
                assert_eq!(*rgb, 0x00FF00);
                assert_eq!(*tolerance, 40);
            }
            k => panic!("{k:?}"),
        }
        assert!(e.selection().is_empty());
        assert_eq!(e.tool(), Tool::Fill);

        assert!(e.undo());
        assert!(e.doc().objects.is_empty(), "one undo removes the fill");
    }

    #[test]
    fn smart_move_needs_a_box_then_a_drop_and_is_one_undo_step() {
        let mut e = editor();
        e.set_tool(Tool::SmartMove);

        // A stray click arms nothing.
        drag(&mut e, pt(50.0, 50.0), pt(50.0, 50.0));
        assert!(e.pending_smart_move().is_none(), "degenerate box rejected");
        assert!(e.doc().objects.is_empty());

        // Drag out the source, then click the destination.
        drag(&mut e, pt(10.0, 10.0), pt(60.0, 40.0));
        assert_eq!(
            e.pending_smart_move(),
            Some(RectF::new(10.0, 10.0, 50.0, 30.0))
        );
        assert!(e.doc().objects.is_empty(), "nothing committed yet");

        e.pointer_down(pt(200.0, 120.0));
        e.pointer_up(pt(200.0, 120.0));
        match &e.doc().objects[0].kind {
            ObjectKind::SmartMove { src, dst } => {
                assert_eq!(*src, RectF::new(10.0, 10.0, 50.0, 30.0));
                assert_eq!(*dst, pt(200.0, 120.0));
            }
            k => panic!("{k:?}"),
        }
        assert!(e.pending_smart_move().is_none(), "pending state cleared");

        assert!(e.undo());
        assert!(e.doc().objects.is_empty(), "one undo removes the move");
    }
}
