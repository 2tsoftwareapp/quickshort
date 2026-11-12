//! Zoomable / pannable canvas for the annotation editor.
//!
//! Transforms between screen coords (egui points) and document coords
//! (logical px of the base image), plus macOS workspace backdrop, shadows, and handles.

use crate::theme::{self, accent_blue, amber_orange, bg_workspace_bot, bg_workspace_top};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, StrokeKind, Vec2};
use qs_doc::{ObjectKind, PointF, RectF};

pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 16.0;

#[derive(Clone, Copy, Debug)]
pub struct CanvasView {
    /// Center of the viewport in document logical coords.
    pub center: PointF,
    /// Scale factor: screen points per document logical pixel.
    pub zoom: f32,
}

impl Default for CanvasView {
    fn default() -> Self {
        Self {
            center: PointF::new(0.0, 0.0),
            zoom: 1.0,
        }
    }
}

impl CanvasView {
    /// Fit `doc_rect` inside `screen_rect` with margin, optionally clamping max auto-zoom.
    pub fn fit_to(&mut self, doc_rect: RectF, screen_rect: Rect, margin: f32) {
        let avail_w = (screen_rect.width() - 2.0 * margin).max(10.0);
        let avail_h = (screen_rect.height() - 2.0 * margin).max(10.0);
        let zw = avail_w / doc_rect.w.max(1.0);
        let zh = avail_h / doc_rect.h.max(1.0);
        self.zoom = zw.min(zh).clamp(MIN_ZOOM, 1.5);
        self.center = doc_rect.center();
    }

    /// Convert document logical point to screen point.
    pub fn doc_to_screen(&self, p: PointF, screen_rect: Rect) -> Pos2 {
        let screen_center = screen_rect.center();
        Pos2::new(
            screen_center.x + (p.x - self.center.x) * self.zoom,
            screen_center.y + (p.y - self.center.y) * self.zoom,
        )
    }

    /// Convert screen point to document logical point.
    pub fn screen_to_doc(&self, pos: Pos2, screen_rect: Rect) -> PointF {
        let screen_center = screen_rect.center();
        PointF::new(
            self.center.x + (pos.x - screen_center.x) / self.zoom,
            self.center.y + (pos.y - screen_center.y) / self.zoom,
        )
    }

    /// Convert document logical rect to screen rect.
    pub fn doc_rect_to_screen(&self, r: RectF, screen_rect: Rect) -> Rect {
        let a = self.doc_to_screen(PointF::new(r.x, r.y), screen_rect);
        let b = self.doc_to_screen(PointF::new(r.right(), r.bottom()), screen_rect);
        Rect::from_two_pos(a, b)
    }

    /// Zoom centered at `anchor_screen`.
    pub fn zoom_at(&mut self, factor: f32, anchor_screen: Pos2, screen_rect: Rect) {
        let anchor_doc = self.screen_to_doc(anchor_screen, screen_rect);
        let new_zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.zoom = new_zoom;
        let screen_center = screen_rect.center();
        self.center = PointF::new(
            anchor_doc.x - (anchor_screen.x - screen_center.x) / new_zoom,
            anchor_doc.y - (anchor_screen.y - screen_center.y) / new_zoom,
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
