//! Zoomable / pannable canvas for the annotation editor.
//!
//! Transforms between screen coords (egui points) and document coords
//! (logical px of the base image), plus macOS workspace backdrop, shadows, and handles.

use crate::theme::{self, accent_blue, amber_orange, bg_workspace_bot, bg_workspace_top};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, StrokeKind, Vec2};
use qs_doc::{Document, ObjectKind, Orientation, PointF, RectF, SizeF};

pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 16.0;

/// Document -> oriented output ("display") coordinates: the current view
/// origin and orientation, refreshed from the document every frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisplayMap {
    pub view: RectF,
    pub orientation: Orientation,
}

impl DisplayMap {
    pub fn of(doc: &Document) -> Self {
        Self { view: doc.view, orientation: doc.orientation }
    }

    fn view_size(&self) -> SizeF {
        SizeF::new(self.view.w, self.view.h)
    }

    pub fn to_display(&self, p: PointF) -> PointF {
        let local = PointF::new(p.x - self.view.x, p.y - self.view.y);
        self.orientation.map(local, self.view_size())
    }

    pub fn from_display(&self, p: PointF) -> PointF {
        let l = self.orientation.unmap(p, self.view_size());
        PointF::new(l.x + self.view.x, l.y + self.view.y)
    }

    pub fn display_size(&self) -> SizeF {
        self.orientation.output_size(self.view_size())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CanvasView {
    /// Center of the viewport in display (oriented output) logical coords.
    pub center: PointF,
    /// Scale factor: screen points per document logical pixel.
    pub zoom: f32,
    /// How document coords land in display coords.
    pub map: DisplayMap,
}

impl Default for CanvasView {
    fn default() -> Self {
        Self {
            center: PointF::new(0.0, 0.0),
            zoom: 1.0,
            map: DisplayMap::default(),
        }
    }
}

impl CanvasView {
    /// Adopt the document's current view / orientation, keeping the same
    /// document point under the viewport centre.
    pub fn sync(&mut self, doc: &Document) {
        let map = DisplayMap::of(doc);
        if map != self.map {
            let doc_center = self.map.from_display(self.center);
            self.map = map;
            self.center = map.to_display(doc_center);
        }
    }

    /// The oriented output in display coords (before effect padding).
    pub fn display_rect(&self) -> RectF {
        let s = self.map.display_size();
        RectF::new(0.0, 0.0, s.w, s.h)
    }

    /// Fit `display_rect` inside `screen_rect` with margin, optionally clamping max auto-zoom.
    pub fn fit_to(&mut self, display_rect: RectF, screen_rect: Rect, margin: f32) {
        let avail_w = (screen_rect.width() - 2.0 * margin).max(10.0);
        let avail_h = (screen_rect.height() - 2.0 * margin).max(10.0);
        let zw = avail_w / display_rect.w.max(1.0);
        let zh = avail_h / display_rect.h.max(1.0);
        self.zoom = zw.min(zh).clamp(MIN_ZOOM, 1.5);
        self.center = display_rect.center();
    }

    fn display_to_screen(&self, p: PointF, screen_rect: Rect) -> Pos2 {
        let screen_center = screen_rect.center();
        Pos2::new(
            screen_center.x + (p.x - self.center.x) * self.zoom,
            screen_center.y + (p.y - self.center.y) * self.zoom,
        )
    }

    fn screen_to_display(&self, pos: Pos2, screen_rect: Rect) -> PointF {
        let screen_center = screen_rect.center();
        PointF::new(
            self.center.x + (pos.x - screen_center.x) / self.zoom,
            self.center.y + (pos.y - screen_center.y) / self.zoom,
        )
    }

    /// Convert document logical point to screen point.
    pub fn doc_to_screen(&self, p: PointF, screen_rect: Rect) -> Pos2 {
        self.display_to_screen(self.map.to_display(p), screen_rect)
    }

    /// Convert screen point to document logical point.
    pub fn screen_to_doc(&self, pos: Pos2, screen_rect: Rect) -> PointF {
        self.map.from_display(self.screen_to_display(pos, screen_rect))
    }

    /// Convert document logical rect to screen rect (axis-aligned under
    /// every orientation).
    pub fn doc_rect_to_screen(&self, r: RectF, screen_rect: Rect) -> Rect {
        let a = self.doc_to_screen(PointF::new(r.x, r.y), screen_rect);
        let b = self.doc_to_screen(PointF::new(r.right(), r.bottom()), screen_rect);
        Rect::from_two_pos(a, b)
    }

    /// Convert a display-space rect to screen.
    pub fn display_rect_to_screen(&self, r: RectF, screen_rect: Rect) -> Rect {
        let a = self.display_to_screen(PointF::new(r.x, r.y), screen_rect);
        let b = self.display_to_screen(PointF::new(r.right(), r.bottom()), screen_rect);
        Rect::from_two_pos(a, b)
    }

    /// Zoom centered at `anchor_screen`.
    pub fn zoom_at(&mut self, factor: f32, anchor_screen: Pos2, screen_rect: Rect) {
        let anchor_display = self.screen_to_display(anchor_screen, screen_rect);
        let new_zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.zoom = new_zoom;
        let screen_center = screen_rect.center();
        self.center = PointF::new(
            anchor_display.x - (anchor_screen.x - screen_center.x) / new_zoom,
            anchor_display.y - (anchor_screen.y - screen_center.y) / new_zoom,
        );
    }

    /// Pan by screen delta points.
    pub fn pan(&mut self, delta_screen: Vec2) {
        self.center.x -= delta_screen.x / self.zoom;
        self.center.y -= delta_screen.y / self.zoom;
    }
}

/// Paint the calm macOS matte canvas backdrop with subtle top drop-shadow under chrome.
pub fn paint_workspace(painter: &egui::Painter, rect: Rect) {
    theme::paint_rect_v_gradient(painter, rect, bg_workspace_top(), bg_workspace_bot());

    // Subtle 4px top inner shadow beneath the inspector bar
    let top_strip = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 5.0));
    theme::paint_rect_v_gradient(
        painter,
        top_strip,
        Color32::from_rgba_unmultiplied(0, 0, 0, 55),
        Color32::TRANSPARENT,
    );
}

/// Paint multi-layer macOS soft drop shadow behind the document bounds.
pub fn paint_document_shadow(painter: &egui::Painter, doc_screen_rect: Rect) {
    theme::paint_soft_shadow(painter, doc_screen_rect, 2.0, 9.0, 26.0, 128);
}

/// Paint a checkerboard pattern strictly inside `doc_rect` for transparent areas.
pub fn paint_checkerboard(painter: &egui::Painter, doc_rect: Rect) {
    let clipped = painter.with_clip_rect(doc_rect);
    let size = 10.0;
    let c1 = theme::bg_toolbar_bot();
    let c2 = theme::bg_btn_secondary_top();
    clipped.rect_filled(doc_rect, 0.0, c1);
    let mut y = doc_rect.min.y;
    let mut row = 0;
    while y < doc_rect.max.y {
        let mut x = doc_rect.min.x + if row % 2 == 1 { size } else { 0.0 };
        while x < doc_rect.max.x {
            let tile = Rect::from_min_size(
                Pos2::new(x, y),
                Vec2::new(size.min(doc_rect.max.x - x), size.min(doc_rect.max.y - y)),
            );
            clipped.rect_filled(tile, 0.0, c2);
            x += size * 2.0;
        }
        y += size;
        row += 1;
    }
}

/// Paint crisp hairline document rim so light or dark screenshots separate cleanly.
pub fn paint_document_frame(painter: &egui::Painter, doc_screen_rect: Rect) {
    painter.rect_stroke(
        doc_screen_rect,
        0.0,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 120)),
        StrokeKind::Outside,
    );
    painter.rect_stroke(
        doc_screen_rect,
        0.0,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 24)),
        StrokeKind::Inside,
    );
}

/// Crop preview: dim everything outside the pending crop rect, then stroke its
/// outline. `screen_rect` is the canvas clip area (logical px).
pub fn paint_crop_overlay(painter: &mut egui::Painter, crop_screen: Rect, screen_rect: Rect) {
    let p = painter.with_clip_rect(screen_rect);
    let dim = Color32::from_rgba_unmultiplied(0, 0, 0, 150);
    if crop_screen.min.x > screen_rect.min.x {
        let r = Rect::from_min_size(screen_rect.min, Vec2::new(crop_screen.min.x - screen_rect.min.x, screen_rect.height()));
        p.rect_filled(r, 0.0, dim);
    }
    if crop_screen.max.x < screen_rect.max.x {
        let r = Rect::from_min_max(Pos2::new(crop_screen.max.x, screen_rect.min.y), Pos2::new(screen_rect.max.x, screen_rect.max.y));
        p.rect_filled(r, 0.0, dim);
    }
    if crop_screen.min.y > screen_rect.min.y {
        let r = Rect::from_min_max(Pos2::new(screen_rect.min.x, screen_rect.min.y), Pos2::new(screen_rect.max.x, crop_screen.min.y));
        p.rect_filled(r, 0.0, dim);
    }
    if crop_screen.max.y < screen_rect.max.y {
        let r = Rect::from_min_max(Pos2::new(screen_rect.min.x, crop_screen.max.y), Pos2::new(screen_rect.max.x, screen_rect.max.y));
        p.rect_filled(r, 0.0, dim);
    }
    p.rect_stroke(
        crop_screen,
        0.0,
        Stroke::new(1.6, accent_blue()),
        StrokeKind::Outside,
    );
}

/// Paint a committed selection region outline: white core + black halo.
pub fn paint_region(painter: &egui::Painter, region_screen: Rect) {
    painter.rect_stroke(
        region_screen,
        0.0,
        Stroke::new(1.5, Color32::WHITE),
        StrokeKind::Outside,
    );
    painter.rect_stroke(
        region_screen.shrink(1.5),
        0.0,
        Stroke::new(1.5, Color32::BLACK),
        StrokeKind::Inside,
    );
}

/// Paint selection frame + resize / endpoint handles on the canvas.
pub fn paint_handles(
    painter: &egui::Painter,
    handles: &[qs_edit::Handle],
    selected_kind: Option<&ObjectKind>,
    view: &CanvasView,
    screen_rect: Rect,
) {
    // Draw subtle selection bounding outline & callout tail tether
    if let Some(kind) = selected_kind {
        match kind {
            ObjectKind::Callout { body, tail, .. } => {
                let br = view.doc_rect_to_screen(body.normalized(), screen_rect);
                painter.rect_stroke(
                    br,
                    2.0,
                    Stroke::new(1.0, theme::accent_alpha(160)),
                    StrokeKind::Outside,
                );
                let bc = br.center();
                let tc = view.doc_to_screen(*tail, screen_rect);
                painter.line_segment(
                    [bc, tc],
                    Stroke::new(1.0, theme::amber_alpha(140)),
                );
            }
            ObjectKind::Rect { .. }
            | ObjectKind::Ellipse { .. }
            | ObjectKind::Text { .. }
            | ObjectKind::Blur { .. } => {
                let b = qs_render::bounds(kind);
                let sr = view.doc_rect_to_screen(b, screen_rect);
                painter.rect_stroke(
                    sr,
                    0.0,
                    Stroke::new(1.0, theme::accent_alpha(130)),
                    StrokeKind::Outside,
                );
            }
            _ => {}
        }
    }

    let handle_radius = 5.0;
    let outline = Stroke::new(1.75, accent_blue());
    let tail_outline = Stroke::new(1.75, Color32::WHITE);

    for h in handles {
        let p = view.doc_to_screen(h.pos, screen_rect);
        // Subtle handle shadow
        painter.circle_filled(
            p + Vec2::new(0.0, 1.2),
            handle_radius + 1.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, 110),
        );
        match h.id {
            qs_edit::HandleId::Tail => {
                painter.circle(p, handle_radius + 0.5, amber_orange(), tail_outline);
            }
            _ => {
                painter.circle(p, handle_radius, Color32::WHITE, outline);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Rect {
        Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0))
    }

    fn close(a: PointF, b: PointF) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }

    #[test]
    fn screen_round_trip_and_handles_follow_rotation() {
        let mut d = Document::new(400, 200, 1.0);
        d.view = RectF::new(50.0, 20.0, 300.0, 160.0);
        d.orientation = Orientation::Rotate90;
        let mut v = CanvasView::default();
        v.sync(&d);
        v.fit_to(v.display_rect(), screen(), 40.0);
        let p = PointF::new(60.0, 30.0); // 10,10 inside the view's top-left
        let s = v.doc_to_screen(p, screen());
        assert!(close(v.screen_to_doc(s, screen()), p));
        // clockwise: view's top-left corner shows at the displayed top-right
        let tl = v.doc_to_screen(PointF::new(50.0, 20.0), screen());
        let shown = v.display_rect_to_screen(v.display_rect(), screen());
        assert!((tl.x - shown.max.x).abs() < 1e-3 && (tl.y - shown.min.y).abs() < 1e-3);
        assert!((shown.width() / shown.height() - 160.0 / 300.0).abs() < 1e-3, "axes swap");
        let r = v.doc_rect_to_screen(RectF::new(60.0, 30.0, 20.0, 10.0), screen());
        assert!((r.width() - 10.0 * v.zoom).abs() < 1e-3 && (r.height() - 20.0 * v.zoom).abs() < 1e-3);
    }

    #[test]
    fn sync_keeps_the_same_document_point_centred() {
        let mut d = Document::new(400, 200, 1.0);
        let mut v = CanvasView::default();
        v.sync(&d);
        v.center = PointF::new(100.0, 50.0);
        let before = v.screen_to_doc(screen().center(), screen());
        d.orientation = Orientation::Rotate270;
        d.view = RectF::new(30.0, 0.0, 300.0, 200.0);
        v.sync(&d);
        assert!(close(v.screen_to_doc(screen().center(), screen()), before));
    }
}
