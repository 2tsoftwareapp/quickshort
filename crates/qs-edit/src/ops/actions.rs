//! Object-creating commands — simplify, captions, redactions,
//! fill and Smart Move.

use super::*;

impl Editor {
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
