//! Tool & selection state, groups, align/distribute, layers and z-order.

use super::*;

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

    pub(super) fn reset_tool(&mut self) {
        if !self.sticky_tools {
            self.tool = Tool::Select;
        }
    }


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
    pub(super) fn expand_groups(&self, ids: &[ObjectId]) -> Vec<ObjectId> {
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
    pub(super) fn selection_units(&self) -> Vec<(Vec<ObjectId>, RectF)> {
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

    pub(super) fn set_layer_visible_and_locked(&mut self, id: ObjectId, visible: bool, locked: bool) -> bool {
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
}
