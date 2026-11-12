//! Headless editing engine for `qs-doc` documents.
//!
//! The UI (egui window, tests, future frontends) feeds pointer / keyboard
//! events in and reads the document + selection back. No pixels, no window.
//!
//! Undo model: whole-state snapshots (`objects`, `view`, `background`,
//! `step_start`). Drags and text sessions record one entry when they end,
//! and only if something actually changed.

pub mod hit;

pub use hit::{Handle, HandleId};

use qs_doc::{
    Align, ArrowHead, BlurMode, CalloutShape, Document, Fill, Object, ObjectId, ObjectKind, PointF,
    RectF, Stroke, TextStyle, MARKER_ALPHA, PALETTE, PIX_BLOCK, SIZES,
};
use qs_render::{bounds, text, text_block, CALLOUT_PAD};

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
    Crop,
    SelectRegion,
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
    pub step_radius: f32,
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
            step_radius: 12.0,
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
}

#[derive(Clone, Debug, PartialEq)]
enum Drag {
    None,
    Move { id: ObjectId, start: PointF, orig: ObjectKind },
    Handle { id: ObjectId, h: HandleId, orig: ObjectKind },
    Create { id: ObjectId, start: PointF },
    TextSelect,
    Crop { start: PointF },
    Region { start: PointF },
}

pub struct Editor {
    doc: Document,
    tool: Tool,
    pub style: Style,
    selected: Option<ObjectId>,
    drag: Drag,
    pre: Option<Snap>,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    edit: Option<TextEdit>,
    edit_pre: Option<Snap>,
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
    /// Region rectangle being previewed by the SelectRegion tool (transient, logical px).
    region_rect: Option<RectF>,
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

fn clamp_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn prev_boundary(s: &str, i: usize) -> usize {
    let i = clamp_boundary(s, i);
    s[..i].char_indices().next_back().map(|(b, _)| b).unwrap_or(0)
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
        Tool::Rect | Tool::Ellipse | Tool::Blur | Tool::Callout => {
            let m = dx.abs().max(dy.abs());
            PointF::new(a.x + m * dx.signum(), a.y + m * dy.signum())
        }
        _ => b,
    }
}

fn degenerate(k: &ObjectKind) -> bool {
    match k {
        ObjectKind::Arrow { a, b, .. } | ObjectKind::Line { a, b, .. } => a.dist(*b) < 3.0,
        ObjectKind::Rect { r, .. } | ObjectKind::Ellipse { r, .. } | ObjectKind::Blur { r, .. } => {
            r.w.abs() < 3.0 || r.h.abs() < 3.0
        }
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
        Rect { fill, .. } | Ellipse { fill, .. } | Text { fill, .. } | Callout { fill, .. } => Some(fill),
        _ => None,
    }
}

fn text_style_mut(k: &mut ObjectKind) -> Option<&mut TextStyle> {
    match k {
        ObjectKind::Text { style, .. } | ObjectKind::Callout { style, .. } => Some(style),
        _ => None,
    }
}

impl Editor {
    pub fn new(doc: Document) -> Self {
        Self {
            doc,
            tool: Tool::Select,
            style: Style::default(),
            selected: None,
            drag: Drag::None,
            pre: None,
            undo: Vec::new(),
            redo: Vec::new(),
            edit: None,
            edit_pre: None,
            hit_tol: 5.0,
            sticky_tools: false,
            rev: 0,
            crop_rect: None,
            region_rect: None,
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
    pub fn selected(&self) -> Option<ObjectId> {
        self.selected
    }
    pub fn selected_object(&self) -> Option<&Object> {
        self.selected.and_then(|id| self.doc.get(id))
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

    /// Handles of the selected (unlocked) object.
    pub fn selection_handles(&self) -> Vec<Handle> {
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
        }
    }

    fn restore(&mut self, s: Snap) {
        self.doc.objects = s.objects;
        self.doc.view = s.view;
        self.doc.region = s.region;
        self.doc.background = s.background;
        self.doc.step_start = s.step_start;
        if self.selected.is_some_and(|id| self.doc.get(id).is_none()) {
            self.selected = None;
        }
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
        if self.edit.is_some() {
            f(self);
            return;
        }
        let pre = self.snap();
        f(self);
        if pre != self.snap() {
            self.push_undo(pre);
        }
    }

    pub fn undo(&mut self) -> bool {
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
        self.end_text_edit();
        self.tool = t;
        if t != Tool::Select {
            self.selected = None;
        }
    }

    pub fn select(&mut self, id: Option<ObjectId>) {
        self.end_text_edit();
        self.selected = id.filter(|i| self.doc.get(*i).is_some());
    }

    /// Esc: leave text edit, else deselect, else back to Select / clear region.
    pub fn escape(&mut self) {
        if matches!(self.drag, Drag::Crop { .. }) {
            self.drag = Drag::None;
            self.crop_rect = None;
        }
        if matches!(self.drag, Drag::Region { .. }) {
            self.drag = Drag::None;
            self.region_rect = None;
        }
        if self.edit.is_some() {
            self.end_text_edit();
        } else if self.selected.is_some() {
            self.selected = None;
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
        self.drag = Drag::None;
        if let Some(te) = self.edit {
            let inside = self
                .doc
                .get(te.id)
                .is_some_and(|o| Self::text_hit_rect(&o.kind).inflate(self.hit_tol).contains(p));
            if inside {
                if let Some((origin, lay)) = self.edit_block() {
                    let i = lay.index_at(p.x - origin.x, p.y - origin.y);
                    self.edit = Some(TextEdit { id: te.id, caret: i, anchor: i });
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
        if self.tool == Tool::SelectRegion {
            self.drag = Drag::Region { start: p };
            return;
        }
        match self.tool {
            Tool::Select => self.select_down(p),
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
                self.edit = Some(TextEdit { id, caret: i, anchor: i });
            }
        }
    }

    fn select_down(&mut self, p: PointF) {
        if let Some(o) = self.selected_object() {
            if !o.locked {
                let (id, orig) = (o.id, o.kind.clone());
                let slack = self.hit_tol * 1.8;
                if let Some(hd) = hit::handles(&orig).into_iter().find(|hd| hd.pos.dist(p) <= slack) {
                    self.begin_change();
                    self.drag = Drag::Handle { id, h: hd.id, orig };
                    return;
                }
            }
        }
        self.selected = self.pick(p);
        if let Some(o) = self.selected_object() {
            if !o.locked {
                let (id, orig) = (o.id, o.kind.clone());
                self.begin_change();
                self.drag = Drag::Move { id, start: p, orig };
            }
        }
    }

    fn create_down(&mut self, p: PointF) {
        let st = self.style.clone();
        let stroke = Stroke::new(st.rgb, st.width);
        let r0 = RectF::new(p.x, p.y, 0.0, 0.0);
        let kind = match self.tool {
            Tool::Pen => ObjectKind::Pen { pts: vec![p], stroke },
            Tool::Highlighter => ObjectKind::Highlighter {
                pts: vec![p],
                stroke: Stroke { rgb: st.rgb, alpha: MARKER_ALPHA, width: st.width * MARKER_K },
            },
            Tool::Arrow => ObjectKind::Arrow { a: p, b: p, stroke, head: st.arrow_head },
            Tool::Line => ObjectKind::Line { a: p, b: p, stroke },
            Tool::Rect => ObjectKind::Rect { r: r0, stroke, fill: st.fill, radius: 1.5 },
            Tool::Ellipse => ObjectKind::Ellipse { r: r0, stroke, fill: st.fill },
            Tool::Blur => ObjectKind::Blur { r: r0, mode: st.blur_mode, strength: st.blur_strength },
            Tool::Callout => {
                let fill = if st.fill.is_none() {
                    Fill::Solid { rgb: 0xFFFFFF, alpha: 1.0 }
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
                    self.edit = Some(TextEdit { id, caret: i, anchor: i });
                }
            }
            return;
        }
        let style = TextStyle { rgb: self.style.rgb, ..self.style.text };
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
        self.selected = Some(id);
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
            Drag::Region { start } => {
                let start = *start;
                self.region_rect = Some(RectF::from_points(start, p));
            }
            Drag::Move { id, start, orig } => {
                let id = *id;
                let (dx, dy) = (p.x - start.x, p.y - start.y);
                let mut k = orig.clone();
                k.translate(dx, dy);
                self.set_kind(id, k);
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
                        ObjectKind::Pen { pts, .. } | ObjectKind::Highlighter { pts, .. } => {
                            if pts.last().is_none_or(|l| l.dist(p) >= 0.5) {
                                pts.push(p);
                            }
                        }
                        ObjectKind::Arrow { b, .. } | ObjectKind::Line { b, .. } => *b = q,
                        ObjectKind::Rect { r, .. }
                        | ObjectKind::Ellipse { r, .. }
                        | ObjectKind::Blur { r, .. } => *r = RectF::from_points(start, q),
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
            Drag::Region { .. } => {
                if let Some(r) = self.region_rect.take() {
                    if r.w >= 1.0 && r.h >= 1.0 {
                        self.set_region(Some(r));
                    }
                }
                self.reset_tool();
            }
            Drag::Move { .. } | Drag::Handle { .. } => self.end_change(),
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
        let Some(o) = self.doc.get_mut(id) else { return };
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
            self.selected = None;
            return;
        }
        let freehand = self.tool.keeps_tool();
        self.selected = if freehand { None } else { Some(id) };
        let pre = pre.unwrap_or_else(|| self.snap());
        if is_callout {
            self.begin_text_edit_with(id, pre);
        } else if pre != self.snap() {
            self.push_undo(pre);
        }
        self.reset_tool();
    }

    // ---- text editing --------------------------------------------------------

    /// Start editing the text of `id` (caret at the end).
    pub fn begin_text_edit(&mut self, id: ObjectId) -> bool {
        let pre = self.snap();
        self.begin_text_edit_with(id, pre)
    }

    fn begin_text_edit_with(&mut self, id: ObjectId, pre: Snap) -> bool {
        if self.edit.is_some() {
            self.end_text_edit();
        }
        let Some(o) = self.doc.get(id) else { return false };
        if o.locked {
            return false;
        }
        let Some(t) = o.kind.text() else { return false };
        let len = t.len();
        self.selected = Some(id);
        self.edit = Some(TextEdit { id, caret: len, anchor: len });
        self.edit_pre = Some(pre);
        true
    }

    pub fn end_text_edit(&mut self) {
        let Some(te) = self.edit.take() else { return };
        let pre = self.edit_pre.take();
        let empty_text = self.doc.get(te.id).is_some_and(|o| {
            matches!(o.kind, ObjectKind::Text { .. }) && o.kind.text().is_some_and(|t| t.trim().is_empty())
        });
        if empty_text {
            self.doc.remove(te.id);
            if self.selected == Some(te.id) {
                self.selected = None;
            }
        }
        if let Some(pre) = pre {
            if pre != self.snap() {
                self.push_undo(pre);
            }
        }
    }

    fn edit_text(&mut self, f: impl FnOnce(&mut String, &mut TextEdit)) -> bool {
        let Some(mut te) = self.edit else { return false };
        let Some(t) = self.doc.get_mut(te.id).and_then(|o| o.kind.text_mut()) else {
            return false;
        };
        te.caret = clamp_boundary(t, te.caret);
        te.anchor = clamp_boundary(t, te.anchor);
        f(t, &mut te);
        self.edit = Some(te);
        self.fit_callout(te.id);
        true
    }

    /// Grow a callout so its wrapped text fits (never shrinks).
    fn fit_callout(&mut self, id: ObjectId) {
        let Some(o) = self.doc.get_mut(id) else { return };
        if let ObjectKind::Callout { body, text: t, style, .. } = &mut o.kind {
            let b = body.normalized();
            let lay = text::layout(t, style, Some((b.w - 2.0 * CALLOUT_PAD).max(8.0)));
            let need = lay.height + 2.0 * CALLOUT_PAD;
            if need > b.h {
                *body = RectF::new(b.x, b.y, b.w, need);
            }
        }
    }

    pub fn text_insert(&mut self, s: &str) {
        let clean: String = s.chars().filter(|c| *c == '\n' || !c.is_control()).collect();
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
        let len = self.doc.get(te.id).and_then(|o| o.kind.text()).map_or(0, str::len);
        self.edit = Some(TextEdit { caret: len, anchor: 0, ..te });
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
        self.edit = Some(TextEdit { caret: new, anchor: if extend { te.anchor } else { new }, ..te });
    }

    fn motion_target(&self, te: TextEdit, m: Motion) -> usize {
        let Some(o) = self.doc.get(te.id) else { return te.caret };
        let Some(t) = o.kind.text() else { return te.caret };
        let i = clamp_boundary(t, te.caret);
        match m {
            Motion::Left => prev_boundary(t, i),
            Motion::Right => next_boundary(t, i),
            Motion::DocStart => 0,
            Motion::DocEnd => t.len(),
            Motion::Home | Motion::End | Motion::Up | Motion::Down => {
                let Some((_, lay)) = text_block(&o.kind) else { return i };
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

    fn selected_unlocked(&self) -> Option<ObjectId> {
        let o = self.selected_object()?;
        (!o.locked).then_some(o.id)
    }

    pub fn delete_selected(&mut self) -> bool {
        self.end_text_edit();
        let Some(id) = self.selected_unlocked() else { return false };
        self.record(|e| {
            e.doc.remove(id);
        });
        self.selected = None;
        true
    }

    pub fn duplicate_selected(&mut self) -> Option<ObjectId> {
        self.end_text_edit();
        let id = self.selected?;
        let src = self.doc.get(id)?.kind.clone();
        let mut new = None;
        self.record(|e| {
            let mut k = src;
            k.translate(12.0, 12.0);
            new = Some(e.doc.push(k));
        });
        self.selected = new;
        new
    }

    pub fn reorder(&mut self, z: ZOrder) -> bool {
        let Some(id) = self.selected else { return false };
        let Some(i) = self.doc.index_of(id) else { return false };
        let n = self.doc.objects.len();
        let j = match z {
            ZOrder::Front => n - 1,
            ZOrder::Back => 0,
            ZOrder::Forward => (i + 1).min(n - 1),
            ZOrder::Backward => i.saturating_sub(1),
        };
        if i == j {
            return false;
        }
        self.record(|e| {
            let o = e.doc.objects.remove(i);
            e.doc.objects.insert(j, o);
        });
        true
    }

    pub fn nudge(&mut self, dx: f32, dy: f32) -> bool {
        self.update_selected(|k| k.translate(dx, dy))
    }

    pub fn set_locked(&mut self, id: ObjectId, locked: bool) {
        self.record(|e| {
            if let Some(o) = e.doc.get_mut(id) {
                o.locked = locked;
            }
        });
    }

    pub fn set_visible(&mut self, id: ObjectId, visible: bool) {
        self.record(|e| {
            if let Some(o) = e.doc.get_mut(id) {
                o.visible = visible;
            }
        });
    }

    /// Output region (crop / extended canvas), logical px.
    pub fn set_view(&mut self, view: RectF) {
        let v = view.normalized();
        if v.w < 1.0 || v.h < 1.0 || !v.is_finite() {
            return;
        }
        self.record(|e| e.doc.view = v);
    }

    pub fn set_background(&mut self, argb: u32) {
        self.record(|e| e.doc.background = argb);
    }

    pub fn set_step_start(&mut self, n: u32) {
        self.record(|e| e.doc.step_start = n);
    }

    /// Edit the selected object as one undo step.
    pub fn update_selected(&mut self, f: impl FnOnce(&mut ObjectKind)) -> bool {
        let Some(id) = self.selected_unlocked() else { return false };
        self.record(|e| {
            if let Some(o) = e.doc.get_mut(id) {
                f(&mut o.kind);
            }
        });
        self.fit_callout(id);
        true
    }

    // ---- style (palette) -------------------------------------------------------------

    pub fn set_color(&mut self, rgb: u32) {
        self.style.rgb = rgb;
        self.update_selected(|k| {
            if let Some(s) = stroke_mut(k) {
                s.rgb = rgb;
            }
            match k {
                ObjectKind::Text { style, .. } => style.rgb = rgb,
                ObjectKind::Step { rgb: r, text_rgb, .. } => {
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
            if let ObjectKind::Callout { fill: f, style, stroke, .. } = k {
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
        let se = e.selection_handles().into_iter().find(|h| h.id == HandleId::SE).unwrap();
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
        assert!(e.doc().objects.is_empty(), "whole text session is one undo step");
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
        let tail = e.selection_handles().into_iter().find(|h| h.id == HandleId::Tail).unwrap();
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
        let east = e.selection_handles().into_iter().find(|h| h.id == HandleId::E).unwrap();
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
        e.set_fill(Fill::Solid { rgb: 0x0000FF, alpha: 0.5 });
        match &e.doc().objects[0].kind {
            ObjectKind::Rect { stroke, fill, .. } => {
                assert_eq!((stroke.rgb, stroke.width), (0x00FF00, 6.0));
                assert_eq!(*fill, Fill::Solid { rgb: 0x0000FF, alpha: 0.5 });
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
        e.set_fill(Fill::Solid { rgb: 0xFFFF00, alpha: 1.0 });
        e.set_tool(Tool::Ellipse);
        drag(&mut e, pt(10.0, 10.0), pt(80.0, 60.0));
        match &e.doc().objects[0].kind {
            ObjectKind::Ellipse { stroke, fill, .. } => {
                assert_eq!(stroke.rgb, 0x112233);
                assert_eq!(*fill, Fill::Solid { rgb: 0xFFFF00, alpha: 1.0 });
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
        assert!(hit::hit(&c, pt(50.0, 25.0), 3.0), "callout interior is hittable");
        assert!(hit::hit(&c, pt(50.0, 118.0), 3.0), "tail tip");
        assert!(!hit::hit(&c, pt(200.0, 200.0), 3.0));
    }

    #[test]
    fn shape_arrow_blur_setters_update_selection_and_undo() {
        let mut e = editor();
        e.set_tool(Tool::Arrow);
        drag(&mut e, pt(10.0, 10.0), pt(80.0, 10.0));
        e.set_arrow_head(ArrowHead::Filled);
        assert!(matches!(e.doc().objects[0].kind, ObjectKind::Arrow { head: ArrowHead::Filled, .. }));
        assert!(e.undo());
        assert!(matches!(e.doc().objects[0].kind, ObjectKind::Arrow { head: ArrowHead::Open, .. }));

        e.set_tool(Tool::Callout);
        drag(&mut e, pt(20.0, 40.0), pt(120.0, 90.0));
        e.end_text_edit();
        e.select(Some(e.doc().objects[1].id));
        e.set_callout_shape(CalloutShape::Pill);
        assert!(matches!(e.doc().objects[1].kind, ObjectKind::Callout { shape: CalloutShape::Pill, .. }));
        assert!(e.undo());
        assert!(matches!(e.doc().objects[1].kind, ObjectKind::Callout { shape: CalloutShape::RoundRect, .. }));

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
}
