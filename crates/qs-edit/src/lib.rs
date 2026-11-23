//! Headless editing engine for `qs-doc` documents.
//!
//! The UI (egui window, tests, future frontends) feeds pointer / keyboard
//! events in and reads the document + selection back. No pixels, no window.
//!
//! Undo model: whole-state snapshots (`objects`, `view`, `background`,
//! `step_start`). Drags and text sessions record one entry when they end,
//! and only if something actually changed.

pub mod hit;
pub mod simplify;
pub mod trim;

pub use hit::{Handle, HandleId};

use qs_doc::{
    Align, ArrowHead, BlurMode, CalloutShape, Document, Fill, FontFamily, MARKER_ALPHA,
    MagnifyShape, Object, ObjectId, ObjectKind, Orientation, PALETTE, PIX_BLOCK, PointF, RectF,
    SIZES, StampKind, Stroke, TextOutline, TextStyle, GroupId,
};
use qs_render::{CALLOUT_PAD, bounds, text, text_block};

const MAX_UNDO: usize = 200;
/// Highlighter width = stroke width × this (S/M/L ≈ overlay marker sizes).
const MARKER_K: f32 = 2.6;
pub const DEFAULT_CALLOUT: (f32, f32) = (140.0, 50.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Pen,
    Highlighter,
    Arrow,
    Line,
    Rect,
    Ellipse,
    Text,
    Callout,
    Step,
    Blur,
    Magnify,
    Spotlight,
    Stamp,
    Eraser,
    Crop,
    Cutout,
    SelectRegion,
    /// Paint bucket: click fills the region under the cursor.
    Fill,
    /// Move existing canvas content: drag a box, then click where it lands.
    SmartMove,
}

impl Tool {
    /// Freehand tools stay active after each stroke.
    fn keeps_tool(self) -> bool {
        matches!(self, Tool::Pen | Tool::Highlighter)
    }
}

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

#[derive(Clone, PartialEq)]
struct Snap {
    objects: Vec<Object>,
    view: RectF,
    region: Option<RectF>,
    background: u32,
    step_start: u32,
    orientation: Orientation,
    effects: qs_doc::Effects,
    cutouts: Vec<qs_doc::Cutout>,
}

#[derive(Clone, Debug, PartialEq)]
enum Drag {
    None,
    /// `items`: (id, kind at drag start) for every moved object.
    Move {
        items: Vec<(ObjectId, ObjectKind)>,
        start: PointF,
    },
    Marquee {
        start: PointF,
    },
    Handle {
        id: ObjectId,
        h: HandleId,
        orig: ObjectKind,
    },
    Create {
        id: ObjectId,
        start: PointF,
    },
    TextSelect,
    Crop {
        start: PointF,
    },
    Cutout {
        start: PointF,
    },
    /// Smart Move: dragging out the region whose content will move.
    SmartMoveSrc {
        start: PointF,
    },
    Region {
        start: PointF,
    },
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
    /// Keep the drawing tool active after creating an object.
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
    if l > 150.0 { 0x000000 } else { 0xFFFFFF }
}

fn clamp_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn prev_boundary(s: &str, i: usize) -> usize {
    let i = clamp_boundary(s, i);
    s[..i]
        .char_indices()
        .next_back()
        .map(|(b, _)| b)
        .unwrap_or(0)
}

fn next_boundary(s: &str, i: usize) -> usize {
    let i = clamp_boundary(s, i);
    s[i..].chars().next().map(|c| i + c.len_utf8()).unwrap_or(i)
}

fn default_tail(body: RectF) -> PointF {
    let b = body.normalized();
    PointF::new(b.center().x, b.bottom() + (b.h * 0.6).max(24.0))
}

fn constrain(tool: Tool, a: PointF, b: PointF, shift: bool) -> PointF {
    if !shift {
        return b;
    }
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    match tool {
        Tool::Arrow | Tool::Line => {
            let step = std::f32::consts::PI / 12.0;
            let ang = ((dy.atan2(dx)) / step).round() * step;
            let len = dx.hypot(dy);
            PointF::new(a.x + len * ang.cos(), a.y + len * ang.sin())
        }
        Tool::Rect
        | Tool::Ellipse
        | Tool::Blur
        | Tool::Callout
        | Tool::Magnify
        | Tool::Spotlight
        | Tool::Stamp => {
            let m = dx.abs().max(dy.abs());
            PointF::new(a.x + m * dx.signum(), a.y + m * dy.signum())
        }
        _ => b,
    }
}

fn degenerate(k: &ObjectKind) -> bool {
    match k {
        ObjectKind::Arrow { a, b, .. } | ObjectKind::Line { a, b, .. } => a.dist(*b) < 3.0,
        ObjectKind::Rect { r, .. }
        | ObjectKind::Ellipse { r, .. }
        | ObjectKind::Blur { r, .. }
        | ObjectKind::Magnify { rect: r, .. }
        | ObjectKind::Spotlight { rect: r, .. }
        | ObjectKind::Stamp { rect: r, .. } => r.w.abs() < 3.0 || r.h.abs() < 3.0,
        ObjectKind::Eraser { pts, .. } => pts.len() < 2 || pts.iter().all(|p| p.dist(pts[0]) < 1.0),
        _ => false,
    }
}

fn stroke_mut(k: &mut ObjectKind) -> Option<&mut Stroke> {
    use ObjectKind::*;
    match k {
        Pen { stroke, .. }
        | Highlighter { stroke, .. }
        | Arrow { stroke, .. }
        | Line { stroke, .. }
        | Rect { stroke, .. }
        | Ellipse { stroke, .. }
        | Callout { stroke, .. } => Some(stroke),
        _ => None,
    }
}

fn fill_mut(k: &mut ObjectKind) -> Option<&mut Fill> {
    use ObjectKind::*;
    match k {
        Rect { fill, .. } | Ellipse { fill, .. } | Text { fill, .. } | Callout { fill, .. } => {
            Some(fill)
        }
        _ => None,
    }
}

fn text_style_mut(k: &mut ObjectKind) -> Option<&mut TextStyle> {
    match k {
        ObjectKind::Text { style, .. } | ObjectKind::Callout { style, .. } => Some(style),
        _ => None,
    }
}

/// Colour a selection swatch should show for `k`.
fn selection_rgb(k: &ObjectKind) -> Option<u32> {
    use ObjectKind::*;
    match k {
        Pen { stroke, .. }
        | Highlighter { stroke, .. }
        | Line { stroke, .. }
        | Arrow { stroke, .. }
        | Rect { stroke, .. }
        | Ellipse { stroke, .. }
        | Callout { stroke, .. }
        | Magnify { stroke, .. } => Some(stroke.rgb),
        Text { style, .. } => Some(style.rgb),
        Step { rgb, .. } | Stamp { rgb, .. } => Some(*rgb),
        _ => None,
    }
}

/// Overwrite every style knob `k` exposes with the values in `st`, leaving
/// geometry alone (Quick Styles / "paste style").
fn apply_style_to_kind(k: &mut ObjectKind, st: &Style) {
    use ObjectKind::*;
    let is_marker = matches!(k, Highlighter { .. });
    if let Some(s) = stroke_mut(k) {
        s.rgb = st.rgb;
        s.width = if is_marker {
            st.width * MARKER_K
        } else {
            st.width
        };
    }
    if let Some(f) = fill_mut(k) {
        *f = st.fill;
    }
    if let Some(t) = text_style_mut(k) {
        *t = st.text.clone();
        t.rgb = st.rgb;
    }
    match k {
        Arrow { head, .. } => *head = st.arrow_head,
        Callout { shape, .. } => *shape = st.callout_shape,
        Step { rgb, text_rgb, .. } => {
            *rgb = st.rgb;
            *text_rgb = contrast(st.rgb);
        }
        Blur { mode, strength, .. } => {
            *mode = st.blur_mode;
            *strength = st.blur_strength;
        }
        Magnify { zoom, shape, .. } => {
            *zoom = st.magnify_zoom;
            *shape = st.magnify_shape;
        }
        Spotlight {
            dim_opacity, blur, ..
        } => {
            *dim_opacity = st.spot_dim;
            *blur = st.spot_blur;
        }
        Stamp { kind, rgb, .. } => {
            *kind = st.stamp_kind;
            *rgb = st.rgb;
        }
        Eraser { width, .. } => *width = st.eraser_width,
        _ => {}
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

    /// Commit / clear the selection region. Cleared (or a rejected rect) does
    /// not push an undo step.
    pub fn set_region(&mut self, region: Option<RectF>) {
        self.record(|e| e.doc.region = region);
    }

    /// Reports the current document as modified without touching its content,
    /// so an editor comparing revisions treats it as unsaved. Used when edits
    /// are recovered from a crashed session.
    pub fn mark_modified(&mut self) {
        self.rev = self.rev.wrapping_add(1);
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
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn is_dragging(&self) -> bool {
        !matches!(self.drag, Drag::None)
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

    // ---- snapshots / undo ----------------------------------------------

    fn snap(&self) -> Snap {
        Snap {
            objects: self.doc.objects.clone(),
            view: self.doc.view,
            region: self.doc.region,
            background: self.doc.background,
            step_start: self.doc.step_start,
            orientation: self.doc.orientation,
            effects: self.doc.effects.clone(),
            cutouts: self.doc.cutouts.clone(),
        }
    }

    fn restore(&mut self, s: Snap) {
        self.doc.objects = s.objects;
        self.doc.view = s.view;
        self.doc.region = s.region;
        self.doc.background = s.background;
        self.doc.step_start = s.step_start;
        self.doc.orientation = s.orientation;
        self.doc.effects = s.effects;
        self.doc.cutouts = s.cutouts;
        self.selection.retain(|id| self.doc.get(*id).is_some());
    }

    fn push_undo(&mut self, pre: Snap) {
        self.rev = self.rev.wrapping_add(1);
        self.undo.push(pre);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn begin_change(&mut self) {
        self.finish_effects();
        if self.pre.is_none() {
            self.pre = Some(self.snap());
        }
    }

    fn end_change(&mut self) {
        if let Some(pre) = self.pre.take() {
            if pre != self.snap() {
                self.push_undo(pre);
            }
        }
    }

    /// Run `f` as one undo step (skipped when it changes nothing). Inside a
    /// text session the change folds into that session's step instead.
    fn record(&mut self, f: impl FnOnce(&mut Self)) {
        self.finish_effects();
        if self.edit.is_some() {
            let pre = self.snap();
            f(self);
            if pre != self.snap() {
                self.mark_modified();
            }
            return;
        }
        let pre = self.snap();
        f(self);
        if pre != self.snap() {
            self.push_undo(pre);
        }
    }

    /// Live preview across slider frames; finish at the end of the gesture.
    pub fn preview_effects(&mut self, effects: qs_doc::Effects) -> bool {
        if effects.validate().is_err() || effects == self.doc.effects {
            return false;
        }
        self.end_text_edit();
        if self.effects_pre.is_none() {
            self.effects_pre = Some(self.snap());
        }
        self.doc.effects = effects;
        self.rev = self.rev.wrapping_add(1);
        true
    }

    pub fn finish_effects(&mut self) {
        if let Some(pre) = self.effects_pre.take() {
            if pre != self.snap() {
                self.push_undo(pre);
            }
        }
    }

    /// Apply an entire effects draft as one history entry.
    pub fn set_effects(&mut self, effects: qs_doc::Effects) -> bool {
        if effects.validate().is_err() || effects == self.doc.effects {
            return false;
        }
        self.end_text_edit();
        self.record(|s| s.doc.effects = effects);
        true
    }

    pub fn undo(&mut self) -> bool {
        self.finish_effects();
        self.end_text_edit();
        if self.is_dragging() {
            return false;
        }
        let Some(s) = self.undo.pop() else {
            return false;
        };
        let cur = self.snap();
        self.redo.push(cur);
        self.restore(s);
        self.rev = self.rev.wrapping_add(1);
        true
    }

    pub fn redo(&mut self) -> bool {
        self.finish_effects();
        self.end_text_edit();
        if self.is_dragging() {
            return false;
        }
        let Some(s) = self.redo.pop() else {
            return false;
        };
        let cur = self.snap();
        self.undo.push(cur);
        self.restore(s);
        self.rev = self.rev.wrapping_add(1);
        true
    }

    // ---- tool / selection ------------------------------------------------

    pub fn set_tool(&mut self, t: Tool) {
        self.finish_effects();
        self.end_text_edit();
        self.tool = t;
        if t != Tool::Select {
            self.selection.clear();
        }
    }

    /// Select exactly `id` — or its whole group when it belongs to one.
    pub fn select(&mut self, id: Option<ObjectId>) {
        self.end_text_edit();
        self.selection = match id {
            Some(i) if self.doc.get(i).is_some() => self.group_span(i),
            _ => Vec::new(),
        };
    }

    /// Replace the selection with `ids` (invalid ids dropped, primary last).
    pub fn select_many(&mut self, ids: impl IntoIterator<Item = ObjectId>) {
        self.end_text_edit();
        self.selection = ids
            .into_iter()
            .filter(|i| self.doc.get(*i).is_some())
            .collect();
    }

    /// Toggle `id` in the selection (Shift+click). A grouped object toggles
    /// its whole group. Returns the new membership.
    pub fn toggle_select(&mut self, id: ObjectId) -> bool {
        self.end_text_edit();
        let span = self.group_span(id);
        if span.iter().all(|m| self.selection.contains(m)) {
            self.selection.retain(|i| !span.contains(i));
            false
        } else if self.doc.get(id).is_some() {
            for member in span {
                if !self.selection.contains(&member) {
                    self.selection.push(member);
                }
            }
            true
        } else {
            false
        }
    }

    /// Esc: leave text edit, else deselect, else back to Select / clear region.
    pub fn escape(&mut self) {
        if matches!(self.drag, Drag::Crop { .. }) {
            self.drag = Drag::None;
            self.crop_rect = None;
            self.tool = Tool::Select;
            return;
        }
        if matches!(self.drag, Drag::Cutout { .. }) {
            self.drag = Drag::None;
            self.cutout_rect = None;
            self.tool = Tool::Select;
            return;
        }
        if matches!(self.drag, Drag::SmartMoveSrc { .. }) {
            self.drag = Drag::None;
            self.move_src_rect = None;
            self.tool = Tool::Select;
            return;
        }
        if matches!(self.drag, Drag::Region { .. }) {
            self.drag = Drag::None;
            self.region_rect = None;
            self.tool = Tool::Select;
            return;
        }
        if self.edit.is_some() {
            self.end_text_edit();
        } else if !self.selection.is_empty() {
            self.selection.clear();
        } else if self.tool == Tool::SelectRegion {
            self.set_region(None);
            self.tool = Tool::Select;
        } else if self.region_rect.is_none() {
            self.set_region(None);
            self.tool = Tool::Select;
        }
    }

    fn reset_tool(&mut self) {
        if !self.sticky_tools && !self.tool.keeps_tool() {
            self.tool = Tool::Select;
        }
    }

    // ---- pointer ---------------------------------------------------------

    fn text_hit_rect(kind: &ObjectKind) -> RectF {
        match kind {
            ObjectKind::Callout { body, .. } => body.normalized(),
            k => bounds(k),
        }
    }

    fn edit_block(&self) -> Option<(PointF, text::TextLayout)> {
        let te = self.edit?;
        text_block(&self.doc.get(te.id)?.kind)
    }

    pub fn pointer_down(&mut self, p: PointF) {
        self.pointer_down_mod(p, false);
    }

    /// `pointer_down` with the Shift modifier state (multi-select).
    pub fn pointer_down_mod(&mut self, p: PointF, shift: bool) {
        self.pending_shift = shift;
        self.finish_effects();
        self.drag = Drag::None;
        if let Some(te) = self.edit {
            let inside = self.doc.get(te.id).is_some_and(|o| {
                Self::text_hit_rect(&o.kind)
                    .inflate(self.hit_tol)
                    .contains(p)
            });
            if inside {
                if let Some((origin, lay)) = self.edit_block() {
                    let i = lay.index_at(p.x - origin.x, p.y - origin.y);
                    self.edit = Some(TextEdit {
                        id: te.id,
                        caret: i,
                        anchor: i,
                    });
                    self.drag = Drag::TextSelect;
                }
                return;
            }
            self.end_text_edit();
        }
        if self.tool == Tool::Crop {
            self.drag = Drag::Crop { start: p };
            return;
        }
        if self.tool == Tool::Cutout {
            self.drag = Drag::Cutout { start: p };
            return;
        }
        if self.tool == Tool::Fill {
            self.fill_at(p);
            return;
        }
        if self.tool == Tool::SmartMove {
            // Second click drops the armed content; the first drag picks it.
            if self.pending_move.is_some() {
                self.commit_smart_move(p);
            } else {
                self.drag = Drag::SmartMoveSrc { start: p };
            }
            return;
        }
        if self.tool == Tool::SelectRegion {
            self.drag = Drag::Region { start: p };
            return;
        }
        match self.tool {
            Tool::Select => self.select_down(p, shift),
            Tool::Text => self.text_down(p),
            Tool::Step => self.step_down(p),
            _ => self.create_down(p),
        }
    }

    /// Double-click: edit the text of the object under the cursor.
    pub fn double_click(&mut self, p: PointF) {
        if self.edit.is_some() {
            return;
        }
        let Some(id) = self.pick(p) else { return };
        self.drag = Drag::None;
        self.pre = None;
        let pre = self.snap();
        if self.begin_text_edit_with(id, pre) {
            if let Some((origin, lay)) = self.edit_block() {
                let i = lay.index_at(p.x - origin.x, p.y - origin.y);
                self.edit = Some(TextEdit {
                    id,
                    caret: i,
                    anchor: i,
                });
            }
        }
    }

    fn select_down(&mut self, p: PointF, shift: bool) {
        // Resize handles only exist for a single selection.
        if self.selection.len() == 1 {
            if let Some(o) = self.selected_object() {
                if !o.locked {
                    let (id, orig) = (o.id, o.kind.clone());
                    let slack = self.hit_tol * 1.8;
                    if let Some(hd) = hit::handles(&orig)
                        .into_iter()
                        .find(|hd| hd.pos.dist(p) <= slack)
                    {
                        self.begin_change();
                        self.drag = Drag::Handle { id, h: hd.id, orig };
                        return;
                    }
                }
            }
        }
        let picked = self.pick(p);
        match picked {
            Some(id) if shift => {
                // Shift+click toggles membership without starting a drag.
                self.toggle_select(id);
                return;
            }
            Some(id) => {
                let span = self.group_span(id);
                if !span.iter().all(|m| self.selection.contains(m)) {
                    self.selection = span;
                }
            }
            None => {
                if !shift {
                    self.selection.clear();
                }
                self.drag = Drag::Marquee { start: p };
                self.marquee_rect = Some(RectF::new(p.x, p.y, 0.0, 0.0));
                return;
            }
        }
        // Move every selected, unlocked object together (one undo step).
        let items: Vec<(ObjectId, ObjectKind)> = self
            .selection
            .iter()
            .filter_map(|id| self.doc.get(*id))
            .filter(|o| !o.locked)
            .map(|o| (o.id, o.kind.clone()))
            .collect();
        if !items.is_empty() {
            self.begin_change();
            self.drag = Drag::Move { items, start: p };
        }
    }

    fn create_down(&mut self, p: PointF) {
        let st = self.style.clone();
        let stroke = Stroke::new(st.rgb, st.width);
        let r0 = RectF::new(p.x, p.y, 0.0, 0.0);
        let kind = match self.tool {
            Tool::Pen => ObjectKind::Pen {
                pts: vec![p],
                stroke,
            },
            Tool::Highlighter => ObjectKind::Highlighter {
                pts: vec![p],
                stroke: Stroke {
                    rgb: st.rgb,
                    alpha: MARKER_ALPHA,
                    width: st.width * MARKER_K,
                },
            },
            Tool::Arrow => ObjectKind::Arrow {
                a: p,
                b: p,
                stroke,
                head: st.arrow_head,
            },
            Tool::Line => ObjectKind::Line { a: p, b: p, stroke },
            Tool::Rect => ObjectKind::Rect {
                r: r0,
                stroke,
                fill: st.fill,
                radius: 1.5,
            },
            Tool::Ellipse => ObjectKind::Ellipse {
                r: r0,
                stroke,
                fill: st.fill,
            },
            Tool::Blur => ObjectKind::Blur {
                r: r0,
                mode: st.blur_mode,
                strength: st.blur_strength,
            },
            Tool::Magnify => ObjectKind::Magnify {
                rect: r0,
                zoom: st.magnify_zoom,
                shape: st.magnify_shape,
                stroke,
            },
            Tool::Spotlight => ObjectKind::Spotlight {
                rect: r0,
                dim_opacity: st.spot_dim,
                blur: st.spot_blur,
            },
            Tool::Eraser => {
                let pts = vec![p, p];
                ObjectKind::Eraser {
                    pts,
                    width: st.eraser_width,
                }
            }
            Tool::Stamp => ObjectKind::Stamp {
                rect: r0,
                kind: st.stamp_kind,
                rgb: st.rgb,
            },
            Tool::Callout => {
                let fill = if st.fill.is_none() {
                    Fill::Solid {
                        rgb: 0xFFFFFF,
                        alpha: 1.0,
                    }
                } else {
                    st.fill
                };
                let rgb = match fill {
                    Fill::Solid { rgb, .. } => contrast(rgb),
                    Fill::None => st.rgb,
                };
                ObjectKind::Callout {
                    shape: st.callout_shape,
                    body: r0,
                    tail: p,
                    stroke,
                    fill,
                    text: String::new(),
                    style: TextStyle { rgb, ..st.text },
                }
            }
            _ => return,
        };
        self.begin_change();
        let id = self.doc.push(kind);
        self.drag = Drag::Create { id, start: p };
    }

    fn text_down(&mut self, p: PointF) {
        let existing = self.pick(p).filter(|id| {
            self.doc
                .get(*id)
                .is_some_and(|o| o.kind.text().is_some() && !o.locked)
        });
        let pre = self.snap();
        if let Some(id) = existing {
            if self.begin_text_edit_with(id, pre) {
                if let Some((origin, lay)) = self.edit_block() {
                    let i = lay.index_at(p.x - origin.x, p.y - origin.y);
                    self.edit = Some(TextEdit {
                        id,
                        caret: i,
                        anchor: i,
                    });
                }
            }
            return;
        }
        let style = TextStyle {
            rgb: self.style.rgb,
            ..self.style.text
        };
        let id = self.doc.push(ObjectKind::Text {
            pos: p,
            text: String::new(),
            style,
            wrap_w: None,
            fill: Fill::None,
        });
        self.begin_text_edit_with(id, pre);
        self.reset_tool();
    }

    fn step_down(&mut self, p: PointF) {
        let pre = self.snap();
        let id = self.doc.push(ObjectKind::Step {
            center: p,
            radius: self.style.step_radius,
            rgb: self.style.rgb,
            text_rgb: contrast(self.style.rgb),
        });
        self.selection = vec![id];
        self.push_undo(pre);
        self.reset_tool();
    }

    pub fn pointer_move(&mut self, p: PointF, shift: bool) {
        match &self.drag {
            Drag::None => {}
            Drag::TextSelect => {
                if let (Some(te), Some((origin, lay))) = (self.edit, self.edit_block()) {
                    let i = lay.index_at(p.x - origin.x, p.y - origin.y);
                    self.edit = Some(TextEdit { caret: i, ..te });
                }
            }
            Drag::Crop { start } => {
                let start = *start;
                self.crop_rect = Some(RectF::from_points(start, p));
            }
            Drag::Cutout { start } => {
                let start = *start;
                self.cutout_rect = Some(RectF::from_points(start, p));
            }
            Drag::SmartMoveSrc { start } => {
                let start = *start;
                self.move_src_rect = Some(RectF::from_points(start, p));
            }
            Drag::Region { start } => {
                let start = *start;
                self.region_rect = Some(RectF::from_points(start, p));
            }
            Drag::Marquee { start } => {
                let start = *start;
                self.marquee_rect = Some(RectF::from_points(start, p));
            }
            Drag::Move { items, start } => {
                let (items, start) = (items.clone(), *start);
                let (dx, dy) = (p.x - start.x, p.y - start.y);
                for (id, orig) in &items {
                    let mut k = orig.clone();
                    k.translate(dx, dy);
                    self.set_kind(*id, k);
                }
            }
            Drag::Handle { id, h, orig } => {
                let (id, k) = (*id, hit::resized(orig, *h, p));
                self.set_kind(id, k);
            }
            Drag::Create { id, start } => {
                let (id, start) = (*id, *start);
                let q = constrain(self.tool, start, p, shift);
                if let Some(o) = self.doc.get_mut(id) {
                    match &mut o.kind {
                        ObjectKind::Pen { pts, .. }
                        | ObjectKind::Highlighter { pts, .. }
                        | ObjectKind::Eraser { pts, .. } => {
                            if pts.last().is_none_or(|l| l.dist(p) >= 0.5) {
                                pts.push(p);
                            }
                        }
                        ObjectKind::Arrow { b, .. } | ObjectKind::Line { b, .. } => *b = q,
                        ObjectKind::Rect { r, .. }
                        | ObjectKind::Ellipse { r, .. }
                        | ObjectKind::Blur { r, .. } => *r = RectF::from_points(start, q),
                        ObjectKind::Magnify { rect, .. }
                        | ObjectKind::Spotlight { rect, .. }
                        | ObjectKind::Stamp { rect, .. } => *rect = RectF::from_points(start, q),
                        ObjectKind::Callout { body, tail, .. } => {
                            *body = RectF::from_points(start, q);
                            *tail = default_tail(*body);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    pub fn pointer_up(&mut self, _p: PointF) {
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::Create { id, start } => self.finish_create(id, start),
            Drag::Crop { .. } => {
                if let Some(r) = self.crop_rect.take() {
                    self.set_view(r);
                }
                self.reset_tool();
            }
            Drag::SmartMoveSrc { .. } => {
                if let Some(r) = self.move_src_rect.take() {
                    // Degenerate boxes are rejected by `begin_smart_move`, so a
                    // stray click leaves nothing armed.
                    self.begin_smart_move(r);
                }
            }
            Drag::Cutout { .. } => {
                if let Some(r) = self.cutout_rect.take() {
                    let a = self.doc.to_display(PointF::new(r.x, r.y));
                    let b = self.doc.to_display(PointF::new(r.right(), r.bottom()));
                    let (start, end) = match self.style.cutout_axis {
                        qs_doc::CutoutAxis::Horizontal => (a.y.min(b.y), a.y.max(b.y)),
                        qs_doc::CutoutAxis::Vertical => (a.x.min(b.x), a.x.max(b.x)),
                    };
                    self.apply_cutout(
                        self.style.cutout_axis,
                        start,
                        end,
                        self.style.cutout_seam,
                    );
                }
                self.reset_tool();
            }
            Drag::Region { .. } => {
                if let Some(r) = self.region_rect.take() {
                    if r.w >= 1.0 && r.h >= 1.0 {
                        self.set_region(Some(r));
                    }
                }
                self.reset_tool();
            }
            Drag::Move { .. } | Drag::Handle { .. } => self.end_change(),
            Drag::Marquee { .. } => {
                let additive = self.pending_shift;
                if let Some(r) = self.marquee_rect.take() {
                    let r = r.normalized();
                    if r.w >= 3.0 && r.h >= 3.0 {
                        let mut hits: Vec<ObjectId> = self
                            .doc
                            .objects
                            .iter()
                            .filter(|o| o.visible)
                            .filter(|o| {
                                let b = qs_render::bounds(&o.kind);
                                !(b.right() < r.x
                                    || b.x > r.right()
                                    || b.bottom() < r.y
                                    || b.y > r.bottom())
                            })
                            .map(|o| o.id)
                            .collect();
                        hits = self.expand_groups(&hits);
                        if additive {
                            for id in hits {
                                if !self.selection.contains(&id) {
                                    self.selection.push(id);
                                }
                            }
                        } else {
                            self.selection = hits;
                        }
                    }
                }
            }
            Drag::TextSelect | Drag::None => {}
        }
    }

    fn set_kind(&mut self, id: ObjectId, k: ObjectKind) {
        if let Some(o) = self.doc.get_mut(id) {
            o.kind = k;
        }
    }

    fn finish_create(&mut self, id: ObjectId, start: PointF) {
        let pre = self.pre.take();
        let Some(o) = self.doc.get_mut(id) else {
            return;
        };
        let is_callout = matches!(o.kind, ObjectKind::Callout { .. });
        let keep = match &mut o.kind {
            ObjectKind::Callout { body, tail, .. } => {
                let b = body.normalized();
                if b.w < 6.0 && b.h < 6.0 {
                    *body = RectF::new(start.x, start.y, DEFAULT_CALLOUT.0, DEFAULT_CALLOUT.1);
                    *tail = default_tail(*body);
                }
                true
            }
            k => !degenerate(k),
        };
        if !keep {
            self.doc.remove(id);
            self.selection.retain(|i| *i != id);
            return;
        }
        let freehand = self.tool.keeps_tool();
        self.selection = if freehand { Vec::new() } else { vec![id] };
        let pre = pre.unwrap_or_else(|| self.snap());
        if is_callout {
            if pre != self.snap() {
                self.mark_modified();
            }
            self.begin_text_edit_with(id, pre);
        } else if pre != self.snap() {
            self.push_undo(pre);
        }
        self.reset_tool();
    }

    // ---- text editing --------------------------------------------------------

    /// Start editing the text of `id` (caret at the end).
    pub fn begin_text_edit(&mut self, id: ObjectId) -> bool {
        self.finish_effects();
        let pre = self.snap();
        self.begin_text_edit_with(id, pre)
    }

    fn begin_text_edit_with(&mut self, id: ObjectId, pre: Snap) -> bool {
        if self.edit.is_some() {
            self.end_text_edit();
        }
        let Some(o) = self.doc.get(id) else {
            return false;
        };
        if o.locked {
            return false;
        }
        let Some(t) = o.kind.text() else { return false };
        let len = t.len();
        self.selection = vec![id];
        self.edit = Some(TextEdit {
            id,
            caret: len,
            anchor: len,
        });
        self.edit_pre = Some(pre);
        true
    }

    pub fn end_text_edit(&mut self) {
        let Some(te) = self.edit.take() else { return };
        let pre = self.edit_pre.take();
        let empty_text = self.doc.get(te.id).is_some_and(|o| {
            matches!(o.kind, ObjectKind::Text { .. })
                && o.kind.text().is_some_and(|t| t.trim().is_empty())
        });
        if empty_text {
            self.doc.remove(te.id);
            self.selection.retain(|i| *i != te.id);
        }
        if let Some(pre) = pre {
            if pre != self.snap() {
                self.push_undo(pre);
            }
        }
    }

    fn edit_text(&mut self, f: impl FnOnce(&mut String, &mut TextEdit)) -> bool {
        let Some(mut te) = self.edit else {
            return false;
        };
        let Some(t) = self.doc.get_mut(te.id).and_then(|o| o.kind.text_mut()) else {
            return false;
        };
        te.caret = clamp_boundary(t, te.caret);
        te.anchor = clamp_boundary(t, te.anchor);
        let before = t.clone();
        f(t, &mut te);
        let changed = *t != before;
        self.edit = Some(te);
        self.fit_callout(te.id);
        if changed {
            // Recovery/close guards must see live edits, while the session
            // still contributes only one undo entry when it ends.
            self.mark_modified();
        }
        true
    }

    /// Grow a callout so its wrapped text fits (never shrinks).
    fn fit_callout(&mut self, id: ObjectId) {
        let Some(o) = self.doc.get_mut(id) else {
            return;
        };
        if let ObjectKind::Callout {
            body,
            text: t,
            style,
            ..
        } = &mut o.kind
        {
            let b = body.normalized();
            let lay = text::layout(t, style, Some((b.w - 2.0 * CALLOUT_PAD).max(8.0)));
            let need = lay.height + 2.0 * CALLOUT_PAD;
            if need > b.h {
                *body = RectF::new(b.x, b.y, b.w, need);
            }
        }
    }

    pub fn text_insert(&mut self, s: &str) {
        let clean: String = s
            .chars()
            .filter(|c| *c == '\n' || !c.is_control())
            .collect();
        if clean.is_empty() {
            return;
        }
        self.edit_text(|t, te| {
            let (a, b) = te.range();
            t.replace_range(a..b, &clean);
            te.caret = a + clean.len();
            te.anchor = te.caret;
        });
    }

    pub fn text_backspace(&mut self) {
        self.edit_text(|t, te| {
            let (a, b) = te.range();
            if a != b {
                t.replace_range(a..b, "");
                te.caret = a;
            } else if a > 0 {
                let p = prev_boundary(t, a);
                t.replace_range(p..a, "");
                te.caret = p;
            }
            te.anchor = te.caret;
        });
    }

    pub fn text_delete(&mut self) {
        self.edit_text(|t, te| {
            let (a, b) = te.range();
            if a != b {
                t.replace_range(a..b, "");
                te.caret = a;
            } else if a < t.len() {
                let n = next_boundary(t, a);
                t.replace_range(a..n, "");
                te.caret = a;
            }
            te.anchor = te.caret;
        });
    }

    pub fn text_select_all(&mut self) {
        let Some(te) = self.edit else { return };
        let len = self
            .doc
            .get(te.id)
            .and_then(|o| o.kind.text())
            .map_or(0, str::len);
        self.edit = Some(TextEdit {
            caret: len,
            anchor: 0,
            ..te
        });
    }

    /// Currently selected text, for copy / cut.
    pub fn text_selection(&self) -> Option<String> {
        let te = self.edit?;
        let t = self.doc.get(te.id)?.kind.text()?;
        let (a, b) = te.range();
        let (a, b) = (clamp_boundary(t, a), clamp_boundary(t, b));
        (a != b).then(|| t[a..b].to_string())
    }

    pub fn text_move(&mut self, m: Motion, extend: bool) {
        let Some(te) = self.edit else { return };
        let (a, b) = te.range();
        let collapse = !extend && a != b;
        let new = match (collapse, m) {
            (true, Motion::Left) => a,
            (true, Motion::Right) => b,
            _ => self.motion_target(te, m),
        };
        self.edit = Some(TextEdit {
            caret: new,
            anchor: if extend { te.anchor } else { new },
            ..te
        });
    }

    fn motion_target(&self, te: TextEdit, m: Motion) -> usize {
        let Some(o) = self.doc.get(te.id) else {
            return te.caret;
        };
        let Some(t) = o.kind.text() else {
            return te.caret;
        };
        let i = clamp_boundary(t, te.caret);
        match m {
            Motion::Left => prev_boundary(t, i),
            Motion::Right => next_boundary(t, i),
            Motion::DocStart => 0,
            Motion::DocEnd => t.len(),
            Motion::Home | Motion::End | Motion::Up | Motion::Down => {
                let Some((_, lay)) = text_block(&o.kind) else {
                    return i;
                };
                let li = lay.line_of(i);
                match m {
                    Motion::Home => lay.lines[li].start,
                    Motion::End => lay.lines[li].end,
                    Motion::Up if li == 0 => 0,
                    Motion::Up => lay.index_on_line(li - 1, lay.caret(i).0),
                    _ if li + 1 >= lay.lines.len() => t.len(),
                    _ => lay.index_on_line(li + 1, lay.caret(i).0),
                }
            }
        }
    }

    // ---- commands ----------------------------------------------------------------

    /// Selected, unlocked object ids (deletion / batch edits target these).
    fn selected_unlocked_ids(&self) -> Vec<ObjectId> {
        self.selection
            .iter()
            .copied()
            .filter(|id| self.doc.get(*id).is_some_and(|o| !o.locked))
            .collect()
    }

    /// Every id that selects, moves and aligns together with `id`: the whole
    /// group for a grouped object, otherwise just `id`. Document z-order.
    pub fn group_span(&self, id: ObjectId) -> Vec<ObjectId> {
        match self.doc.group_of(id) {
            Some(g) => self.doc.group_members(g),
            None => vec![id],
        }
    }

    /// Group of the primary selection, for the layers panel / toolbar state.
    pub fn selected_group(&self) -> Option<GroupId> {
        self.selected().and_then(|id| self.doc.group_of(id))
    }

    /// True when any selected object belongs to a group.
    pub fn selection_grouped(&self) -> bool {
        self.selection
            .iter()
            .any(|id| self.doc.group_of(*id).is_some())
    }

    /// Selected ids expanded to whole groups (deduped, z-order), plus the
    /// locked state of each group's members untouched.
    fn expand_groups(&self, ids: &[ObjectId]) -> Vec<ObjectId> {
        let mut out: Vec<ObjectId> = Vec::new();
        for id in ids {
            for member in self.group_span(*id) {
                if !out.contains(&member) {
                    out.push(member);
                }
            }
        }
        out.sort_by_key(|id| self.doc.index_of(*id).unwrap_or(usize::MAX));
        out
    }

    /// Selection collapsed into transform units: free objects stay alone,
    /// grouped objects share one unit with the union of their bounds.
    fn selection_units(&self) -> Vec<(Vec<ObjectId>, RectF)> {
        let mut units: Vec<(Vec<ObjectId>, RectF)> = Vec::new();
        let mut seen_group: Vec<GroupId> = Vec::new();
        for id in self.selected_unlocked_ids() {
            let Some(obj) = self.doc.get(id) else {
                continue;
            };
            let b = qs_render::bounds(&obj.kind);
            match obj.group {
                None => units.push((vec![id], b)),
                Some(g) => {
                    if seen_group.contains(&g) {
                        continue;
                    }
                    seen_group.push(g);
                    let members = self
                        .selection
                        .iter()
                        .copied()
                        .filter(|m| self.doc.group_of(*m) == Some(g))
                        .filter(|m| self.doc.get(*m).is_some_and(|o| !o.locked))
                        .collect::<Vec<_>>();
                    let union = members
                        .iter()
                        .filter_map(|m| self.doc.get(*m).map(|o| qs_render::bounds(&o.kind)))
                        .reduce(|a, b| a.union(&b));
                    if let Some(union) = union {
                        units.push((members, union));
                    }
                }
            }
        }
        units
    }

    /// Group the selection (merging any groups it touches) as one undo step.
    /// Returns false when fewer than two unlocked objects would be grouped.
    pub fn group_selected(&mut self) -> bool {
        self.end_text_edit();
        let ids = self.selected_unlocked_ids();
        if ids.len() < 2 {
            return false;
        }
        let members = self.expand_groups(&ids);
        if members.len() < 2 {
            return false;
        }
        let groups: Vec<GroupId> = {
            let mut g = Vec::new();
            for id in &members {
                if let Some(gid) = self.doc.group_of(*id) {
                    if !g.contains(&gid) {
                        g.push(gid);
                    }
                }
            }
            g
        };
        let already_one_group = groups.len() == 1
            && members.iter().all(|id| self.doc.group_of(*id) == Some(groups[0]));
        if already_one_group && self.selection.len() == members.len() {
            return false;
        }
        self.record(|e| {
            let gid = e.doc.alloc_group();
            for id in &members {
                if let Some(o) = e.doc.get_mut(*id) {
                    o.group = Some(gid);
                }
            }
        });
        self.selection = members;
        true
    }

    /// Break every group touched by the selection; objects stay put.
    pub fn ungroup_selected(&mut self) -> bool {
        self.end_text_edit();
        let mut groups: Vec<GroupId> = Vec::new();
        for id in &self.selection {
            if let Some(g) = self.doc.group_of(*id) {
                if !groups.contains(&g) {
                    groups.push(g);
                }
            }
        }
        if groups.is_empty() {
            return false;
        }
        self.record(|e| {
            for o in e.doc.objects.iter_mut() {
                if o.group.is_some_and(|g| groups.contains(&g)) {
                    o.group = None;
                }
            }
        });
        true
    }

    /// Delete every selected unlocked object in one undo step.
    pub fn delete_selected(&mut self) -> bool {
        self.end_text_edit();
        let ids = self.selected_unlocked_ids();
        if ids.is_empty() {
            return false;
        }
        self.record(|e| {
            for id in &ids {
                e.doc.remove(*id);
            }
        });
        self.selection.retain(|id| !ids.contains(id));
        true
    }

    /// Duplicate every selected object (+12px), selecting the copies.
    /// Copies of grouped objects get a fresh group of their own.
    /// Returns the primary (last) new id.
    pub fn duplicate_selected(&mut self) -> Option<ObjectId> {
        self.end_text_edit();
        let ids = self.expand_groups(&self.selection);
        if ids.is_empty() {
            return None;
        }
        let mut news = Vec::new();
        let mut grouped: Vec<ObjectId> = Vec::new();
        self.record(|e| {
            for id in &ids {
                let Some(src) = e.doc.get(*id) else {
                    continue;
                };
                let (was_grouped, opacity) = (src.group.is_some(), src.opacity);
                let mut k = src.kind.clone();
                k.translate(12.0, 12.0);
                let new_id = e.doc.push(k);
                if let Some(o) = e.doc.get_mut(new_id) {
                    o.opacity = opacity;
                }
                if was_grouped {
                    grouped.push(new_id);
                }
                news.push(new_id);
            }
            if grouped.len() > 1 {
                let gid = e.doc.alloc_group();
                for new_id in &grouped {
                    if let Some(o) = e.doc.get_mut(*new_id) {
                        o.group = Some(gid);
                    }
                }
            }
        });
        let last = news.last().copied();
        self.selection = news;
        last
    }

    /// Align selected objects (groups align as one unit) by their bounds. The
    /// reference bounds are the union of the current selection.
    pub fn align_selected(&mut self, alignment: Alignment) -> bool {
        let units = self.selection_units();
        if units.len() < 2 {
            return false;
        }
        let union = units
            .iter()
            .map(|(_, b)| *b)
            .reduce(|a, b| a.union(&b))
            .unwrap();
        let offsets = units
            .iter()
            .map(|(ids, b)| {
                let (dx, dy) = match alignment {
                    Alignment::Left => (union.x - b.x, 0.0),
                    Alignment::HCenter => (union.center().x - b.center().x, 0.0),
                    Alignment::Right => (union.right() - b.right(), 0.0),
                    Alignment::Top => (0.0, union.y - b.y),
                    Alignment::VCenter => (0.0, union.center().y - b.center().y),
                    Alignment::Bottom => (0.0, union.bottom() - b.bottom()),
                };
                (ids.clone(), dx, dy)
            })
            .collect::<Vec<_>>();
        if offsets
            .iter()
            .all(|(_, dx, dy)| dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON)
        {
            return false;
        }
        self.record(|e| {
            for (ids, dx, dy) in offsets {
                for id in ids {
                    if let Some(o) = e.doc.get_mut(id) {
                        o.kind.translate(dx, dy);
                    }
                }
            }
        });
        true
    }

    /// Evenly distribute selected objects (groups count as one unit) between
    /// the outermost centers along the requested axis. Requires three units.
    pub fn distribute_selected(&mut self, axis: Axis) -> bool {
        let units = self.selection_units();
        if units.len() < 3 {
            return false;
        }
        let mut items = units;
        items.sort_by(|a, b| {
            let av = if axis == Axis::Horizontal {
                a.1.center().x
            } else {
                a.1.center().y
            };
            let bv = if axis == Axis::Horizontal {
                b.1.center().x
            } else {
                b.1.center().y
            };
            av.total_cmp(&bv)
        });
        let first = items.first().unwrap().1;
        let last = items.last().unwrap().1;
        let start = if axis == Axis::Horizontal {
            first.center().x
        } else {
            first.center().y
        };
        let end = if axis == Axis::Horizontal {
            last.center().x
        } else {
            last.center().y
        };
        let step = (end - start) / (items.len() - 1) as f32;
        let offsets = items
            .iter()
            .enumerate()
            .map(|(i, (ids, b))| {
                let current = if axis == Axis::Horizontal {
                    b.center().x
                } else {
                    b.center().y
                };
                let delta = start + step * i as f32 - current;
                (
                    ids.clone(),
                    if axis == Axis::Horizontal { delta } else { 0.0 },
                    if axis == Axis::Vertical { delta } else { 0.0 },
                )
            })
            .collect::<Vec<_>>();
        if offsets
            .iter()
            .all(|(_, dx, dy)| dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON)
        {
            return false;
        }
        self.record(|e| {
            for (ids, dx, dy) in offsets {
                for id in ids {
                    if let Some(o) = e.doc.get_mut(id) {
                        o.kind.translate(dx, dy);
                    }
                }
            }
        });
        true
    }

    /// Z-order command for the primary selection; groups move as one block.
    pub fn nudge(&mut self, dx: f32, dy: f32) -> bool {
        self.update_selected(|k| k.translate(dx, dy))
    }

    fn set_layer_visible_and_locked(&mut self, id: ObjectId, visible: bool, locked: bool) -> bool {
        let Some(object) = self.doc.get(id) else {
            return false;
        };
        if object.visible == visible && object.locked == locked {
            return false;
        }
        self.record(|e| {
            if let Some(o) = e.doc.get_mut(id) {
                o.visible = visible;
                o.locked = locked;
            }
        });
        true
    }

    pub fn set_locked(&mut self, id: ObjectId, locked: bool) -> bool {
        let Some(o) = self.doc.get(id) else {
            return false;
        };
        self.set_layer_visible_and_locked(id, o.visible, locked)
    }

    pub fn set_visible(&mut self, id: ObjectId, visible: bool) -> bool {
        let Some(o) = self.doc.get(id) else {
            return false;
        };
        self.set_layer_visible_and_locked(id, visible, o.locked)
    }

    /// Move an object — or its whole group, kept contiguous and in order — so
    /// that it starts at document index `to` (bottom = 0) after the move.
    pub fn move_layer(&mut self, id: ObjectId, to: usize) -> bool {
        let Some(from) = self.doc.index_of(id) else {
            return false;
        };
        let block = self.group_span(id);
        let order: Vec<ObjectId> = self.doc.objects.iter().map(|o| o.id).collect();
        let to = to.min(order.len().saturating_sub(block.len()));
        if block.len() <= 1 {
            if from == to {
                return false;
            }
            self.record(|e| {
                let object = e.doc.objects.remove(from);
                e.doc.objects.insert(to, object);
            });
            return true;
        }
        let first = order
            .iter()
            .position(|i| block.contains(i))
            .unwrap_or(from);
        if first == to {
            return false;
        }
        let members: Vec<ObjectId> = block.clone();
        let mut rest: Vec<ObjectId> = order.iter().copied().filter(|i| !block.contains(i)).collect();
        let to = to.min(rest.len());
        rest.splice(to..to, members);
        if rest == order {
            return false;
        }
        self.record(|e| {
            let mut by_id: std::collections::HashMap<ObjectId, Object> =
                e.doc.objects.drain(..).map(|o| (o.id, o)).collect();
            e.doc.objects = rest.iter().filter_map(|id| by_id.remove(id)).collect();
        });
        true
    }

    /// Z-order command for the primary selection; groups move as one block.
    pub fn reorder(&mut self, z: ZOrder) -> bool {
        let Some(id) = self.selected() else {
            return false;
        };
        let block = self.group_span(id);
        let (Some(first), Some(last)) = (
            block.first().and_then(|i| self.doc.index_of(*i)),
            block.last().and_then(|i| self.doc.index_of(*i)),
        ) else {
            return false;
        };
        let max_start = self.doc.objects.len() - block.len();
        let to = match z {
            ZOrder::Front => max_start,
            ZOrder::Back => 0,
            ZOrder::Forward => (last + 1).min(max_start),
            ZOrder::Backward => first.saturating_sub(1),
        };
        self.move_layer(id, to)
    }

    /// Resize the output canvas and update its background as one undoable edit.
    /// Insets are added to each named edge; negative values crop that edge.
    pub fn update_canvas(
        &mut self,
        top: f32,
        right: f32,
        bottom: f32,
        left: f32,
        background: u32,
    ) -> bool {
        if ![top, right, bottom, left].iter().all(|v| v.is_finite()) {
            return false;
        }
        let view = RectF::new(
            self.doc.view.x - left,
            self.doc.view.y - top,
            self.doc.view.w + left + right,
            self.doc.view.h + top + bottom,
        );
        if !view.is_finite() || view.w < 1.0 || view.h < 1.0 {
            return false;
        }
        if view == self.doc.view && background == self.doc.background {
            return false;
        }
        self.record(|e| {
            e.doc.view = view;
            e.doc.background = background;
        });
        true
    }

    /// Expand the output canvas to contain every visible annotation.
    /// Returns false when neither the view nor its background changes.
    pub fn fit_canvas_to_objects(&mut self) -> bool {
        let background = self.doc.background;
        self.fit_canvas_to_objects_with_background(background)
    }

    /// Fit visible annotations and set the canvas background as one edit.
    pub fn fit_canvas_to_objects_with_background(&mut self, background: u32) -> bool {
        let current = self.doc.view;
        let objects = self
            .doc
            .objects
            .iter()
            .filter(|object| object.visible)
            .map(|object| qs_render::bounds(&object.kind))
            .reduce(|bounds, next| bounds.union(&next));
        let target = objects.map_or(current, |bounds| current.union(&bounds));
        self.update_canvas(
            current.y - target.y,
            target.right() - current.right(),
            target.bottom() - current.bottom(),
            current.x - target.x,
            background,
        )
    }

    /// Resize only the canvas, preserving its current background.
    pub fn resize_canvas(&mut self, top: f32, right: f32, bottom: f32, left: f32) -> bool {
        self.update_canvas(top, right, bottom, left, self.doc.background)
    }

    /// Output region (crop / extended canvas), logical px.
    pub fn set_view(&mut self, view: RectF) {
        let v = view.normalized();
        if v.w < 1.0 || v.h < 1.0 || !v.is_finite() {
            return;
        }
        self.record(|e| e.doc.view = v);
    }

    /// Non-destructive strip removal in oriented display coordinates (1 undo step).
    /// Rejects strips < 4 px or >= 90% of the remaining span.
    pub fn apply_cutout(
        &mut self,
        axis: qs_doc::CutoutAxis,
        start: f32,
        end: f32,
        seam: qs_doc::CutoutSeam,
    ) -> bool {
        let eff = self.doc.effective_display_size();
        let total = match axis {
            qs_doc::CutoutAxis::Horizontal => eff.h,
            qs_doc::CutoutAxis::Vertical => eff.w,
        };
        let (s, e) = (start.min(end).max(0.0), start.max(end).min(total));
        let span = e - s;
        if !span.is_finite() || span < 4.0 || span >= total * 0.90 {
            return false;
        }
        self.record(|ed| {
            ed.doc.cutouts.push(qs_doc::Cutout {
                axis,
                start: s,
                end: e,
                seam,
            });
        });
        true
    }

    /// Smart Simplify: detect UI content blocks in `base` and insert editable
    /// rounded-rectangle blocks in a single undo step.
    pub fn simplify_from_image(&mut self, base: &image::RgbaImage) -> usize {
        let blocks = simplify::detect_ui_blocks(&self.doc, base);
        let count = blocks.len();
        if count == 0 {
            return 0;
        }
        self.record(|ed| {
            let mut ids = Vec::with_capacity(blocks.len());
            for kind in blocks {
                ids.push(ed.doc.push(kind));
            }
            ed.selection = ids;
        });
        count
    }

    /// OCR-guided Simplify UI: turns recognized text line rects (optionally
    /// clipped to a selection region) into clean proportional wireframe bars.
    pub fn simplify_from_rects(&mut self, rects: &[RectF], clip: Option<RectF>) -> usize {
        let filtered: Vec<RectF> = rects
            .iter()
            .copied()
            .filter(|r| {
                let rn = r.normalized();
                if rn.w < 4.0 || rn.h < 4.0 {
                    return false;
                }
                match clip {
                    Some(c) => c.normalized().contains(PointF::new(rn.x + rn.w * 0.5, rn.y + rn.h * 0.5)),
                    None => true,
                }
            })
            .collect();
        if filtered.is_empty() {
            return 0;
        }
        let count = filtered.len();
        self.record(|ed| {
            let mut ids = Vec::with_capacity(count);
            for r in filtered {
                let rn = r.normalized();
                ids.push(ed.doc.push(ObjectKind::Rect {
                    r: rn,
                    stroke: Stroke::new(0xD4D4D8, 1.0),
                    fill: Fill::Solid {
                        rgb: 0xE4E4E7,
                        alpha: 0.92,
                    },
                    radius: (rn.h * 0.35).clamp(2.0, 6.0),
                }));
            }
            ed.selection = ids;
        });
        count
    }

    /// Select every visible, unlocked object on the canvas (Ctrl+A).
    pub fn select_all(&mut self) -> usize {
        self.selection = self
            .doc
            .objects
            .iter()
            .filter(|o| o.visible && !o.locked)
            .map(|o| o.id)
            .collect();
        self.selection.len()
    }

    /// Place a pill-styled caption right beside a Step badge and select it for editing.
    pub fn add_step_caption(
        &mut self,
        pos: PointF,
        text: &str,
        style: TextStyle,
        fill: Fill,
    ) -> ObjectId {
        let mut out_id = ObjectId(0);
        self.record(|ed| {
            let id = ed.doc.push(ObjectKind::Text {
                pos,
                text: text.to_owned(),
                style,
                wrap_w: None,
                fill,
            });
            ed.selection = vec![id];
            out_id = id;
        });
        out_id
    }

    /// Add one redaction (`Blur`) object per rectangle in a single undo step.
    pub fn add_redactions(&mut self, rects: &[RectF], mode: qs_doc::BlurMode) -> usize {
        let valid: Vec<RectF> = rects
            .iter()
            .map(|r| r.normalized().inflate(2.0))
            .filter(|rn| rn.w >= 2.0 && rn.h >= 2.0)
            .collect();
        if valid.is_empty() {
            return 0;
        }
        let strength = self.style.blur_strength.max(12.0);
        let count = valid.len();
        self.record(|ed| {
            let mut ids = Vec::with_capacity(count);
            for r in valid {
                ids.push(ed.doc.push(ObjectKind::Blur { r, mode, strength }));
            }
            ed.selection = ids;
        });
        count
    }

    /// Paint bucket: fill the region reachable from `seed` with the current
    /// colour, as one undo step.
    pub fn fill_at(&mut self, seed: PointF) -> bool {
        let rgb = self.style.rgb;
        let tolerance = self.style.fill_tolerance;
        self.record(move |ed| {
            let id = ed.doc.push(ObjectKind::Fill {
                seed,
                rgb,
                tolerance,
            });
            ed.selection = vec![id];
        });
        true
    }

    /// Smart Move, step 1: remember what should move. Rejects degenerate boxes
    /// so a stray click cannot arm a move.
    pub fn begin_smart_move(&mut self, src: RectF) -> bool {
        let r = src.normalized();
        if r.w < 2.0 || r.h < 2.0 {
            return false;
        }
        self.pending_move = Some(r);
        true
    }

    /// The armed Smart Move source, for the canvas preview.
    pub fn pending_smart_move(&self) -> Option<RectF> {
        self.pending_move
    }

    pub fn cancel_smart_move(&mut self) {
        self.pending_move = None;
    }

    /// Smart Move, step 2: drop the armed content at `dst`, one undo step.
    pub fn commit_smart_move(&mut self, dst: PointF) -> bool {
        let Some(src) = self.pending_move.take() else {
            return false;
        };
        self.record(move |ed| {
            let id = ed.doc.push(ObjectKind::SmartMove { src, dst });
            ed.selection = vec![id];
        });
        true
    }

    pub fn set_background(&mut self, argb: u32) {
        self.record(|e| e.doc.background = argb);
    }

    fn reorient(&mut self, f: fn(Orientation) -> Orientation) {
        self.end_text_edit();
        self.record(|e| e.doc.orientation = f(e.doc.orientation));
    }

    /// Rotate the output 90° clockwise (one undo step).
    pub fn rotate_cw(&mut self) {
        self.reorient(Orientation::rotate_cw);
    }

    /// Rotate the output 90° counter-clockwise (one undo step).
    pub fn rotate_ccw(&mut self) {
        self.reorient(Orientation::rotate_ccw);
    }

    /// Mirror the output left-right as displayed (one undo step).
    pub fn flip_h(&mut self) {
        self.reorient(Orientation::flip_h);
    }

    /// Mirror the output top-bottom as displayed (one undo step).
    pub fn flip_v(&mut self) {
        self.reorient(Orientation::flip_v);
    }

    pub fn set_step_start(&mut self, n: u32) {
        self.record(|e| e.doc.step_start = n);
    }

    /// Edit every selected unlocked object as one undo step.
    pub fn update_selected(&mut self, f: impl Fn(&mut ObjectKind)) -> bool {
        let ids = self.selected_unlocked_ids();
        if ids.is_empty() {
            return false;
        }
        self.record(|e| {
            for id in &ids {
                if let Some(o) = e.doc.get_mut(*id) {
                    f(&mut o.kind);
                }
            }
        });
        for id in ids {
            self.fit_callout(id);
        }
        true
    }

    // ---- style (palette) -------------------------------------------------------------

    /// Apply a whole [`Style`] to the tool defaults and to every selected
    /// unlocked object as one undo step (Quick Styles).
    pub fn apply_style(&mut self, style: Style) -> bool {
        self.finish_effects();
        self.end_text_edit();
        let defaults_changed = self.style != style;
        self.style = style.clone();
        let ids = self.selected_unlocked_ids();
        if ids.is_empty() {
            return defaults_changed;
        }
        let mut touched = false;
        self.record(|e| {
            for id in &ids {
                if let Some(o) = e.doc.get_mut(*id) {
                    apply_style_to_kind(&mut o.kind, &style);
                    touched = true;
                }
            }
        });
        for id in ids {
            self.fit_callout(id);
        }
        defaults_changed || touched
    }

    /// Style of the primary selection, or the tool defaults when nothing is
    /// selected — the starting point for "save as preset".
    pub fn current_style(&self) -> Style {
        let mut st = self.style.clone();
        if let Some(o) = self.selected_object() {
            st.rgb = selection_rgb(&o.kind).unwrap_or(st.rgb);
            match &o.kind {
                ObjectKind::Pen { stroke, .. }
                | ObjectKind::Line { stroke, .. }
                | ObjectKind::Arrow { stroke, .. }
                | ObjectKind::Rect { stroke, .. }
                | ObjectKind::Ellipse { stroke, .. }
                | ObjectKind::Callout { stroke, .. }
                | ObjectKind::Magnify { stroke, .. } => st.width = stroke.width,
                ObjectKind::Highlighter { stroke, .. } => st.width = stroke.width / MARKER_K,
                ObjectKind::Eraser { width, .. } => st.eraser_width = *width,
                _ => {}
            }
            match &o.kind {
                ObjectKind::Rect { fill, .. } | ObjectKind::Ellipse { fill, .. } => {
                    st.fill = *fill
                }
                ObjectKind::Text {
                    fill, style: t, ..
                } => {
                    st.fill = *fill;
                    st.text = t.clone();
                }
                ObjectKind::Callout {
                    fill,
                    style: t,
                    shape,
                    ..
                } => {
                    st.fill = *fill;
                    st.text = t.clone();
                    st.callout_shape = *shape;
                }
                ObjectKind::Arrow { head, .. } => st.arrow_head = *head,
                ObjectKind::Blur { mode, strength, .. } => {
                    st.blur_mode = *mode;
                    st.blur_strength = *strength;
                }
                ObjectKind::Magnify { zoom, shape, .. } => {
                    st.magnify_zoom = *zoom;
                    st.magnify_shape = *shape;
                }
                ObjectKind::Spotlight {
                    dim_opacity, blur, ..
                } => {
                    st.spot_dim = *dim_opacity;
                    st.spot_blur = *blur;
                }
                ObjectKind::Stamp { kind, .. } => st.stamp_kind = *kind,
                _ => {}
            }
        }
        st
    }

    /// Opacity of the primary selection (1.0 when nothing is selected).
    pub fn selected_opacity(&self) -> f32 {
        self.selected_object().map(|o| o.opacity).unwrap_or(1.0)
    }

    fn apply_opacity(&mut self, opacity: f32) -> bool {
        let v = opacity.clamp(0.0, 1.0);
        let ids = self.selected_unlocked_ids();
        let changed = ids.iter().any(|id| {
            self.doc
                .get(*id)
                .is_some_and(|o| (o.opacity - v).abs() > f32::EPSILON)
        });
        if !changed {
            return false;
        }
        for id in ids {
            if let Some(o) = self.doc.get_mut(id) {
                o.opacity = v;
            }
        }
        true
    }

    /// Set the paint opacity (0..=1) of every selected unlocked object as one
    /// undo step.
    pub fn set_opacity(&mut self, opacity: f32) -> bool {
        let v = opacity.clamp(0.0, 1.0);
        let ids = self.selected_unlocked_ids();
        if ids.is_empty()
            || ids.iter().all(|id| {
                self.doc
                    .get(*id)
                    .is_some_and(|o| (o.opacity - v).abs() <= f32::EPSILON)
            })
        {
            return false;
        }
        self.record(|e| {
            for id in &ids {
                if let Some(o) = e.doc.get_mut(*id) {
                    o.opacity = v;
                }
            }
        });
        true
    }

    /// Live opacity drag frame; call [`Editor::finish_opacity`] on release so
    /// the whole gesture is a single undo step.
    pub fn preview_opacity(&mut self, opacity: f32) -> bool {
        self.begin_change();
        if self.apply_opacity(opacity) {
            self.rev = self.rev.wrapping_add(1);
            true
        } else {
            false
        }
    }

    pub fn finish_opacity(&mut self) {
        self.end_change();
    }

    pub fn set_color(&mut self, rgb: u32) {
        self.style.rgb = rgb;
        self.update_selected(|k| {
            if let Some(s) = stroke_mut(k) {
                s.rgb = rgb;
            }
            match k {
                ObjectKind::Text { style, .. } => style.rgb = rgb,
                ObjectKind::Stamp { rgb: r, .. } => *r = rgb,
                ObjectKind::Step {
                    rgb: r, text_rgb, ..
                } => {
                    *r = rgb;
                    *text_rgb = contrast(rgb);
                }
                _ => {}
            }
        });
    }

    pub fn set_stroke_width(&mut self, w: f32) {
        self.style.width = w;
        self.update_selected(|k| {
            let is_marker = matches!(k, ObjectKind::Highlighter { .. });
            if let Some(s) = stroke_mut(k) {
                s.width = if is_marker { w * MARKER_K } else { w };
            }
        });
    }

    pub fn set_fill(&mut self, fill: Fill) {
        self.style.fill = fill;
        self.update_selected(|k| {
            if let Some(f) = fill_mut(k) {
                *f = fill;
            }
        });
    }

    fn set_text_style(&mut self, f: impl Fn(&mut TextStyle)) {
        f(&mut self.style.text);
        self.update_selected(|k| {
            if let Some(s) = text_style_mut(k) {
                f(s);
            }
        });
    }

    pub fn set_text_px(&mut self, px: f32) {
        self.set_text_style(|s| s.px = px);
    }
    pub fn set_text_bold(&mut self, bold: bool) {
        self.set_text_style(|s| s.bold = bold);
    }
    pub fn set_text_align(&mut self, a: Align) {
        self.set_text_style(|s| s.align = a);
    }

    pub fn set_text_italic(&mut self, italic: bool) {
        self.set_text_style(|s| s.italic = italic);
    }

    pub fn set_text_underline(&mut self, underline: bool) {
        self.set_text_style(|s| s.underline = underline);
    }

    pub fn set_text_strike(&mut self, strike: bool) {
        self.set_text_style(|s| s.strike = strike);
    }

    pub fn set_text_family(&mut self, family: FontFamily) {
        self.set_text_style(|s| s.family = family);
    }

    /// Turn the outline on with `outline`, or off with `None`.
    pub fn set_text_outline(&mut self, outline: Option<TextOutline>) {
        self.set_text_style(|s| s.outline = outline);
    }

    /// Live text-style drag (sliders): one undo step per gesture — call
    /// [`Editor::finish_text_style`] on release.
    pub fn preview_text_style(&mut self, style: TextStyle) -> bool {
        self.begin_change();
        let mut changed = self.style.text != style;
        self.style.text = style;
        let ids = self.selected_unlocked_ids();
        for id in &ids {
            if let Some(o) = self.doc.get_mut(*id) {
                if let Some(t) = text_style_mut(&mut o.kind) {
                    if *t != style {
                        *t = style;
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.rev = self.rev.wrapping_add(1);
        }
        changed
    }

    pub fn finish_text_style(&mut self) {
        self.end_change();
    }

    pub fn set_magnify_zoom(&mut self, zoom: f32) {
        if !(1.5..=4.0).contains(&zoom) {
            return;
        }
        self.style.magnify_zoom = zoom;
        self.update_selected(|k| {
            if let ObjectKind::Magnify { zoom: z, .. } = k {
                *z = zoom;
            }
        });
    }

    pub fn set_spotlight(&mut self, dim: Option<f32>, bl: Option<f32>) {
        if let Some(d) = dim.filter(|d| (0.0..=1.0).contains(d)) {
            self.style.spot_dim = d;
        }
        if let Some(b) = bl.filter(|b| (0.0..=64.0).contains(b)) {
            self.style.spot_blur = b;
        }
        let (d, b) = (self.style.spot_dim, self.style.spot_blur);
        self.update_selected(|k| {
            if let ObjectKind::Spotlight {
                dim_opacity, blur, ..
            } = k
            {
                if dim.is_some() {
                    *dim_opacity = d;
                }
                if bl.is_some() {
                    *blur = b;
                }
            }
        });
    }

    pub fn set_eraser_width(&mut self, w: f32) {
        let w = w.clamp(1.0, 128.0);
        self.style.eraser_width = w;
        self.update_selected(|k| {
            if let ObjectKind::Eraser { width, .. } = k {
                *width = w;
            }
        });
    }

    pub fn set_stamp_kind(&mut self, kind: StampKind) {
        self.style.stamp_kind = kind;
        self.update_selected(|k| {
            if let ObjectKind::Stamp { kind: s, .. } = k {
                *s = kind;
            }
        });
    }

    pub fn set_magnify_shape(&mut self, shape: MagnifyShape) {
        self.style.magnify_shape = shape;
        self.update_selected(|k| {
            if let ObjectKind::Magnify { shape: s, .. } = k {
                *s = shape;
            }
        });
    }
    pub fn set_callout_shape(&mut self, shape: CalloutShape) {
        self.style.callout_shape = shape;
        self.update_selected(|k| {
            if let ObjectKind::Callout { shape: s, .. } = k {
                *s = shape;
            }
        });
    }

    pub fn set_arrow_head(&mut self, head: ArrowHead) {
        self.style.arrow_head = head;
        self.update_selected(|k| {
            if let ObjectKind::Arrow { head: h, .. } = k {
                *h = head;
            }
        });
    }

    pub fn set_blur_mode(&mut self, mode: BlurMode) {
        self.style.blur_mode = mode;
        self.update_selected(|k| {
            if let ObjectKind::Blur { mode: m, .. } = k {
                *m = mode;
            }
        });
    }

    pub fn set_blur_strength(&mut self, strength: f32) {
        self.style.blur_strength = strength;
        self.update_selected(|k| {
            if let ObjectKind::Blur { strength: s, .. } = k {
                *s = strength;
            }
        });
    }

    pub fn set_callout_fill_and_contrast(&mut self, fill: Fill) {
        self.style.fill = fill;
        self.update_selected(|k| {
            if let ObjectKind::Callout {
                fill: f,
                style,
                stroke,
                ..
            } = k
            {
                *f = fill;
                style.rgb = match fill {
                    Fill::Solid { rgb, .. } => contrast(rgb),
                    Fill::None => stroke.rgb,
                };
            } else if let Some(f) = fill_mut(k) {
                *f = fill;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        e
    }

    #[test]
    fn draw_rect_selects_it_returns_to_select_and_undoes() {
        let mut e = drawn_rect();
        assert_eq!(e.doc().objects.len(), 1);
        assert_eq!(rect_of(&e, 0), RectF::new(10.0, 10.0, 50.0, 30.0));
        assert_eq!(e.selected(), Some(e.doc().objects[0].id));
        assert_eq!(e.tool(), Tool::Select);
        assert!(e.undo());
        assert!(e.doc().objects.is_empty());
        assert_eq!(e.selected(), None);
        assert!(e.redo());
        assert_eq!(e.doc().objects.len(), 1);
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
        e.sticky_tools = true;
        e.set_tool(Tool::Step);
        e.pointer_down(pt(20.0, 20.0));
        e.pointer_up(pt(20.0, 20.0));
        e.pointer_down(pt(60.0, 20.0));
        e.pointer_up(pt(60.0, 20.0));
        let (a, b) = (e.doc().objects[0].id, e.doc().objects[1].id);
        assert_eq!(e.doc().step_number(b), Some(2));
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
        for (x, y) in [(10.0, 10.0), (50.0, 35.0), (120.0, 80.0)] {
            e.set_tool(Tool::Rect);
            drag(&mut e, pt(x, y), pt(x + 20.0, y + 10.0));
        }
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
        assert!(
            bounds
                .iter()
                .all(|r| (r.center().x - bounds[0].center().x).abs() < 0.01)
        );
        assert!(e.align_selected(Alignment::Bottom));
        let bounds = e
            .doc()
            .objects
            .iter()
            .map(|o| qs_render::bounds(&o.kind))
            .collect::<Vec<_>>();
        assert!(
            bounds
                .iter()
                .all(|r| (r.bottom() - bounds[0].bottom()).abs() < 0.01)
        );
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
        e.sticky_tools = true;
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
        e.set_tool(Tool::Rect);
        drag(&mut e, pt(100.0, 100.0), pt(150.0, 140.0));
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
        assert!(e.selection_grouped(), "undo keeps the group and its selection");

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
        assert_eq!(b.x - a.x, before.1.x - before.0.x, "gap inside the group stays");
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
            st.outline = Some(TextOutline { width: w, rgb: 0xFF0000 });
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
        assert_eq!(e.selection().len(), 1, "the new fill is selected");

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
