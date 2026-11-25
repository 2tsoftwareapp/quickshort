//! Annotation operations: pointer gestures, text editing, selection, groups,
//! z-order and object-creating commands.
use qs_doc::{
    Fill, GroupId, Object, ObjectId, ObjectKind, PointF, RectF, Stroke, TextStyle, MARKER_ALPHA,
};
use qs_render::{bounds, text, text_block, CALLOUT_PAD};

use crate::history::Snap;
use crate::{
    contrast, hit, simplify, Alignment, Axis, Editor, HandleId, Motion, TextEdit, Tool, ZOrder,
    DEFAULT_CALLOUT, MARKER_K,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Drag {
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
        | Tool::Stamp
        | Tool::Triangle
        | Tool::Diamond
        | Tool::Star
        | Tool::Hexagon => {
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
        | ObjectKind::Stamp { rect: r, .. }
        | ObjectKind::Polygon { r, .. } => r.w.abs() < 3.0 || r.h.abs() < 3.0,
        ObjectKind::Eraser { pts, .. } => pts.len() < 2 || pts.iter().all(|p| p.dist(pts[0]) < 1.0),
        _ => false,
    }
}

impl Editor {
    /// Commit / clear the selection region. Cleared (or a rejected rect) does
    /// not push an undo step.
    pub fn set_region(&mut self, region: Option<RectF>) {
        self.record(|e| e.doc.region = region);
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

    /// Esc ends text editing and exits the current tool, or deselects in Select.
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
            if matches!(self.tool, Tool::Text | Tool::Callout) {
                self.tool = Tool::Select;
            }
        } else if self.tool != Tool::Select {
            self.selection.clear();
            self.cancel_smart_move();
            self.tool = Tool::Select;
        } else if !self.selection.is_empty() {
            self.selection.clear();
        } else {
            self.set_region(None);
        }
    }

    fn reset_tool(&mut self) {
        if !self.sticky_tools {
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
            self.selection.clear();
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
            Tool::Triangle | Tool::Diamond | Tool::Star | Tool::Hexagon => ObjectKind::Polygon {
                kind: match self.tool {
                    Tool::Triangle => qs_doc::PolyKind::Triangle,
                    Tool::Diamond => qs_doc::PolyKind::Diamond,
                    Tool::Star => qs_doc::PolyKind::Star,
                    _ => qs_doc::PolyKind::Hexagon,
                },
                r: r0,
                stroke,
                fill: st.fill,
            },
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
    }

    fn step_down(&mut self, p: PointF) {
        let pre = self.snap();
        self.doc.push(ObjectKind::Step {
            center: p,
            radius: self.style.step_radius,
            rgb: self.style.rgb,
            text_rgb: contrast(self.style.rgb),
        });
        self.selection.clear();
        self.push_undo(pre);
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
                        | ObjectKind::Blur { r, .. }
                        | ObjectKind::Polygon { r, .. } => *r = RectF::from_points(start, q),
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
                    self.apply_cutout(self.style.cutout_axis, start, end, self.style.cutout_seam);
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
        self.selection.clear();
        let pre = pre.unwrap_or_else(|| self.snap());
        if is_callout {
            if pre != self.snap() {
                self.mark_modified();
            }
            self.begin_text_edit_with(id, pre);
        } else if pre != self.snap() {
            self.push_undo(pre);
        }
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
        }
        if empty_text || matches!(self.tool, Tool::Text | Tool::Callout) {
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
    pub(crate) fn fit_callout(&mut self, id: ObjectId) {
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
    pub(crate) fn selected_unlocked_ids(&self) -> Vec<ObjectId> {
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
            && members
                .iter()
                .all(|id| self.doc.group_of(*id) == Some(groups[0]));
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
        let first = order.iter().position(|i| block.contains(i)).unwrap_or(from);
        if first == to {
            return false;
        }
        let members: Vec<ObjectId> = block.clone();
        let mut rest: Vec<ObjectId> = order
            .iter()
            .copied()
            .filter(|i| !block.contains(i))
            .collect();
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
                    Some(c) => c
                        .normalized()
                        .contains(PointF::new(rn.x + rn.w * 0.5, rn.y + rn.h * 0.5)),
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
}
