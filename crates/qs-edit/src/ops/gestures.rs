//! Pointer gestures — the drag state machine behind pointer_*.

use super::*;

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

impl Editor {
    // ---- pointer ---------------------------------------------------------

    pub(super) fn text_hit_rect(kind: &ObjectKind) -> RectF {
        match kind {
            ObjectKind::Callout { body, .. } => body.normalized(),
            k => bounds(k),
        }
    }

    pub(super) fn edit_block(&self) -> Option<(PointF, text::TextLayout)> {
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

    pub(super) fn select_down(&mut self, p: PointF, shift: bool) {
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

    pub(super) fn create_down(&mut self, p: PointF) {
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
        let g = self.tool.gesture().unwrap_or(draft::Gesture::Rect);
        self.drag = Drag::Create {
            id,
            draw: Draft::begin(g, p),
        };
    }

    pub(super) fn text_down(&mut self, p: PointF) {
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

    pub(super) fn step_down(&mut self, p: PointF) {
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
        match &mut self.drag {
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
            Drag::Create { id, draw } => {
                let q = constrain(self.tool, draw.anchor(), p, shift);
                draw.update(q, 0.5);
                let (id, shape) = (*id, draw.shape());
                if let Some(o) = self.doc.get_mut(id) {
                    draft::apply(&mut o.kind, shape);
                }
            }
        }
    }

    pub fn pointer_up(&mut self, _p: PointF) {
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::Create { id, draw } => self.finish_create(id, draw.anchor()),
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

    pub(super) fn set_kind(&mut self, id: ObjectId, k: ObjectKind) {
        if let Some(o) = self.doc.get_mut(id) {
            o.kind = k;
        }
    }

    pub(super) fn finish_create(&mut self, id: ObjectId, start: PointF) {
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
                    *tail = draft::default_callout_tail(*body);
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
}
