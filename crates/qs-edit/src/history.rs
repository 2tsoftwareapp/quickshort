//! Undo/redo history: whole-state snapshots capped at `MAX_UNDO` entries.
use qs_doc::{Object, Orientation, RectF};

use crate::Editor;

const MAX_UNDO: usize = 200;

#[derive(Clone, PartialEq)]
pub(crate) struct Snap {
    objects: Vec<Object>,
    view: RectF,
    region: Option<RectF>,
    background: u32,
    step_start: u32,
    orientation: Orientation,
    effects: qs_doc::Effects,
    cutouts: Vec<qs_doc::Cutout>,
}

impl Editor {
    /// Reports the current document as modified without touching its content,
    /// so an editor comparing revisions treats it as unsaved. Used when edits
    /// are recovered from a crashed session.
    pub fn mark_modified(&mut self) {
        self.rev = self.rev.wrapping_add(1);
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    // ---- snapshots / undo ----------------------------------------------

    pub(crate) fn snap(&self) -> Snap {
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

    pub(crate) fn push_undo(&mut self, pre: Snap) {
        self.rev = self.rev.wrapping_add(1);
        self.undo.push(pre);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub(crate) fn begin_change(&mut self) {
        self.finish_effects();
        if self.pre.is_none() {
            self.pre = Some(self.snap());
        }
    }

    pub(crate) fn end_change(&mut self) {
        if let Some(pre) = self.pre.take() {
            if pre != self.snap() {
                self.push_undo(pre);
            }
        }
    }

    /// Run `f` as one undo step (skipped when it changes nothing). Inside a
    /// text session the change folds into that session's step instead.
    pub(crate) fn record(&mut self, f: impl FnOnce(&mut Self)) {
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
}
