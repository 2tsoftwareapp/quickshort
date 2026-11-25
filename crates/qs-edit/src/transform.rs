//! Canvas-level geometry: view/crop, canvas resize, rotation, flip, cutouts.
use qs_doc::{Orientation, RectF};

use crate::{trim, Editor};

impl Editor {
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

    /// Trim a uniform border off the output view, as one undo step. Wraps
    /// [`trim::calculate_trim_rect`]; returns false when there is no safe
    /// border to trim.
    pub fn trim_to_content(&mut self, base: &image::RgbaImage, tolerance: u8) -> bool {
        let Some(rect) = trim::calculate_trim_rect(&self.doc, base, tolerance) else {
            return false;
        };
        self.set_view(rect);
        true
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
}
