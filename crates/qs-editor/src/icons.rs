//! Crisp geometric vector icons (Apple SF Symbols / CleanShot X style) painted via `egui::Painter`.

use eframe::egui::{epaint::PathShape, Color32, CornerRadius, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};
use qs_edit::Tool;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionIcon {
    Undo,
    Redo,
    Copy,
    Save,
    Export,
    ZoomOut,
    ZoomIn,
    Fill,
    AlignLeft,
    AlignCenter,
    AlignRight,
    CalloutRoundRect,
    CalloutEllipse,
    CalloutPill,
    ArrowOpen,
    ArrowFilled,
    BlurPixelate,
    BlurSmooth,
    LayerFront,
    LayerBack,
    Duplicate,
    Trash,
    CheckCircle,
    Open,
    SaveAs,
    Canvas,
}

#[inline]
fn p(c: Pos2, dx: f32, dy: f32) -> Pos2 {
    Pos2::new(c.x + dx, c.y + dy)
}

pub fn draw_tool_icon(painter: &Painter, c: Pos2, tool: Tool, color: Color32) {
    let st = Stroke::new(1.45, color);
    let thin = Stroke::new(1.25, color);

    match tool {
        Tool::Select => {
            // Classic crisp macOS pointer cursor
            let pts = vec![
                p(c, -3.5, -5.8),
                p(c, -3.5, 4.2),
                p(c, -0.9, 1.8),
                p(c, 1.3, 5.5),
                p(c, 2.9, 4.7),
                p(c, 0.8, 1.1),
                p(c, 4.3, 1.1),
            ];
            let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 35);
            painter.add(PathShape::convex_polygon(pts.clone(), fill, Stroke::NONE));
            painter.add(PathShape::closed_line(pts, Stroke::new(1.35, color)));
        }
        Tool::Pen => {
            // Diagonal pencil body + nib + collar
            painter.line_segment([p(c, -4.6, 4.6), p(c, 2.6, -2.6)], Stroke::new(2.2, color));
            painter.line_segment([p(c, 2.6, -2.6), p(c, 4.8, -4.8)], Stroke::new(1.3, color));
            // Tiny tip dot and nib separation
            painter.line_segment([p(c, -5.1, 5.1), p(c, -3.6, 3.6)], Stroke::new(1.2, color));
            let eraser_col = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 150);
            painter.line_segment(
                [p(c, 1.6, -3.6), p(c, 3.6, -1.6)],
                Stroke::new(1.1, eraser_col),
            );
        }
        Tool::Highlighter => {
            // Broad angled marker + chisel tip + bottom highlight bar
            painter.line_segment([p(c, -4.2, 2.2), p(c, 1.8, -3.8)], Stroke::new(3.4, color));
            painter.line_segment([p(c, 1.8, -3.8), p(c, 4.4, -5.0)], Stroke::new(1.4, color));
            let bar = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 145);
            painter.line_segment([p(c, -5.0, 5.2), p(c, 5.0, 5.2)], Stroke::new(2.0, bar));
        }
        Tool::Arrow => {
            // Diagonal shaft bottom-left -> top-right + open wings
            let tip = p(c, 4.6, -4.6);
            painter.line_segment([p(c, -4.8, 4.8), tip], st);
            painter.line_segment([tip, p(c, -0.4, -4.6)], st);
            painter.line_segment([tip, p(c, 4.6, 0.4)], st);
        }
        Tool::Line => {
            painter.line_segment([p(c, -4.8, 4.4), p(c, 4.8, -4.4)], st);
            painter.circle_filled(p(c, -4.8, 4.4), 1.4, color);
            painter.circle_filled(p(c, 4.8, -4.4), 1.4, color);
        }
        Tool::Rect => {
            let r = Rect::from_center_size(c, Vec2::new(11.6, 9.0));
            painter.rect_stroke(
                r,
                CornerRadius::same(2),
                st,
                eframe::egui::StrokeKind::Middle,
            );
        }
        Tool::Ellipse => {
            let n = 28;
            let pts: Vec<Pos2> = (0..n)
                .map(|i| {
                    let a = (i as f32 / n as f32) * std::f32::consts::TAU;
                    p(c, a.cos() * 5.8, a.sin() * 4.3)
                })
                .collect();
            painter.add(PathShape::closed_line(pts, st));
        }
        Tool::Text => {
            // Geometric serif-hinted T (SF Symbol textbox aesthetic)
            painter.line_segment([p(c, -5.0, -4.6), p(c, 5.0, -4.6)], Stroke::new(1.6, color));
            painter.line_segment([p(c, -5.0, -4.6), p(c, -5.0, -2.8)], thin);
            painter.line_segment([p(c, 5.0, -4.6), p(c, 5.0, -2.8)], thin);
            painter.line_segment([p(c, 0.0, -4.6), p(c, 0.0, 5.0)], Stroke::new(1.6, color));
            painter.line_segment([p(c, -2.2, 5.0), p(c, 2.2, 5.0)], thin);
        }
        Tool::Callout => {
            // Rounded speech bubble with bottom-left pointer tail
            let pts = vec![
                p(c, -4.0, -5.0),
                p(c, 4.0, -5.0),
                p(c, 5.6, -3.4),
                p(c, 5.6, 1.4),
                p(c, 4.0, 3.0),
                p(c, -0.6, 3.0),
                p(c, -4.0, 5.6),
                p(c, -3.2, 3.0),
                p(c, -4.0, 3.0),
                p(c, -5.6, 1.4),
                p(c, -5.6, -3.4),
            ];
            painter.add(PathShape::closed_line(pts, st));
        }
        Tool::Step => {
            // Circle badge + crisp geometric '1'
            painter.circle_stroke(c, 5.8, st);
            painter.line_segment([p(c, -1.2, -1.5), p(c, 0.3, -2.8)], thin);
            painter.line_segment([p(c, 0.3, -2.8), p(c, 0.3, 2.8)], Stroke::new(1.4, color));
            painter.line_segment([p(c, -1.5, 2.8), p(c, 1.9, 2.8)], thin);
        }
        Tool::Blur => {
            // 2x2 mosaic squares with alternating opacity
            let hi = color;
            let lo = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 95);
            let s = 4.2;
            let gap = 1.4;
            let tl = Rect::from_min_size(p(c, -s - gap * 0.5, -s - gap * 0.5), Vec2::splat(s));
            let tr = Rect::from_min_size(p(c, gap * 0.5, -s - gap * 0.5), Vec2::splat(s));
            let bl = Rect::from_min_size(p(c, -s - gap * 0.5, gap * 0.5), Vec2::splat(s));
            let br = Rect::from_min_size(p(c, gap * 0.5, gap * 0.5), Vec2::splat(s));
            painter.rect_filled(tl, CornerRadius::same(1), hi);
            painter.rect_filled(tr, CornerRadius::same(1), lo);
            painter.rect_filled(bl, CornerRadius::same(1), lo);
            painter.rect_filled(br, CornerRadius::same(1), hi);
        }
        Tool::Crop => {
            // Snagit-style frame outline (crop = inset frame + corner tick)
            let size = 9.6;
            painter.rect_stroke(
                Rect::from_min_size(p(c, -size / 2.0, -size / 2.0), Vec2::splat(size)),
                0.0,
                st,
                StrokeKind::Outside,
            );
            painter.circle_stroke(p(c, 0.0, 0.0), 1.2, thin);
        }
        Tool::SelectRegion => {
            // Dashed-style selection region: white core + black halo
            let size = 9.6;
            let r = Rect::from_min_size(p(c, -size / 2.0, -size / 2.0), Vec2::splat(size));
            painter.rect_stroke(r, 0.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Outside);
            painter.rect_stroke(r.shrink(1.5), 0.0, Stroke::new(1.5, Color32::BLACK), StrokeKind::Inside);
        }
    }
}

pub fn draw_action_icon(painter: &Painter, c: Pos2, icon: ActionIcon, color: Color32) {
    let st = Stroke::new(1.45, color);
    let thin = Stroke::new(1.25, color);

    match icon {
        ActionIcon::Undo | ActionIcon::Redo => {
            let m = if icon == ActionIcon::Undo { 1.0 } else { -1.0 };
            let tip = p(c, -4.6 * m, -1.4);
            painter.line_segment([tip, p(c, tip.x - c.x + 2.8 * m, -4.0)], st);
            painter.line_segment([tip, p(c, tip.x - c.x + 2.8 * m, 1.2)], st);
            painter.line_segment([tip, p(c, 1.8 * m, -1.4)], st);
            // U-turn arc
            let (ccx, ccy, rr) = (1.8, 1.7, 3.1);
            let mut pts = Vec::with_capacity(11);
            for i in 0..=10 {
                let a = (-90.0 + 180.0 * i as f32 / 10.0).to_radians();
                pts.push(p(c, m * (ccx + rr * a.cos()), ccy + rr * a.sin()));
            }
            pts.push(p(c, m * (ccx - 2.4), ccy + rr));
            painter.add(PathShape::line(pts, st));
        }
        ActionIcon::Copy => {
            // Overlapping front/back docs (SF Symbol doc.on.doc)
            let back = Rect::from_center_size(p(c, -1.8, -1.8), Vec2::new(7.4, 8.6));
            let front = Rect::from_center_size(p(c, 1.8, 1.8), Vec2::new(7.4, 8.6));
            painter.rect_stroke(
                back,
                CornerRadius::same(1),
                thin,
                eframe::egui::StrokeKind::Middle,
            );
            painter.rect_filled(
                front.shrink(0.5),
                CornerRadius::same(1),
                Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 30),
            );
            painter.rect_stroke(
                front,
                CornerRadius::same(1),
                st,
                eframe::egui::StrokeKind::Middle,
            );
        }
        ActionIcon::Save => {
            // Clean disk / document save silhouette
            let pts = vec![
                p(c, -5.0, -5.0),
                p(c, 2.8, -5.0),
                p(c, 5.0, -2.8),
                p(c, 5.0, 5.0),
                p(c, -5.0, 5.0),
            ];
            painter.add(PathShape::closed_line(pts, st));
            // Top slot & bottom label
            let top_slot = Rect::from_min_max(p(c, -2.4, -5.0), p(c, 1.6, -2.0));
            painter.rect_stroke(
                top_slot,
                CornerRadius::ZERO,
                thin,
                eframe::egui::StrokeKind::Middle,
            );
            let bot_lbl = Rect::from_min_max(p(c, -2.6, 0.8), p(c, 2.6, 5.0));
            painter.rect_stroke(
                bot_lbl,
                CornerRadius::ZERO,
                thin,
                eframe::egui::StrokeKind::Middle,
            );
        }
        ActionIcon::Export => {
            // Open tray + upward arrow (SF Symbol square.and.arrow.up)
            painter.line_segment([p(c, -4.6, -0.5), p(c, -4.6, 4.8)], st);
            painter.line_segment([p(c, -4.6, 4.8), p(c, 4.6, 4.8)], st);
            painter.line_segment([p(c, 4.6, 4.8), p(c, 4.6, -0.5)], st);
            painter.line_segment([p(c, 0.0, 1.8), p(c, 0.0, -5.4)], st);
            painter.line_segment([p(c, 0.0, -5.4), p(c, -2.8, -2.6)], st);
            painter.line_segment([p(c, 0.0, -5.4), p(c, 2.8, -2.6)], st);
        }
        ActionIcon::Open => {
            // Folder silhouette (SF Symbol folder)
            let pts = vec![
                p(c, -5.2, -3.2),
                p(c, -1.7, -3.2),
                p(c, -0.5, -1.7),
                p(c, 5.2, -1.7),
                p(c, 5.2, 4.2),
                p(c, -5.2, 4.2),
            ];
            painter.add(PathShape::closed_line(pts, st));
        }
        ActionIcon::SaveAs => {
            // Save disk shrunk into the corner, plus a "new" badge
            let (dx, dy, k) = (-1.5, -1.5, 0.68);
            let q = |x: f32, y: f32| p(c, x * k + dx, y * k + dy);
            let pts = vec![
                q(-5.0, -5.0),
                q(2.8, -5.0),
                q(5.0, -2.8),
                q(5.0, 5.0),
                q(-5.0, 5.0),
            ];
            painter.add(PathShape::closed_line(pts, thin));
            painter.rect_stroke(
                Rect::from_min_max(q(-2.6, 1.0), q(2.6, 5.0)),
                CornerRadius::ZERO,
                thin,
                eframe::egui::StrokeKind::Middle,
            );
            painter.line_segment([p(c, 3.3, 3.6), p(c, 6.3, 3.6)], st);
            painter.line_segment([p(c, 4.8, 2.1), p(c, 4.8, 5.1)], st);
        }
        ActionIcon::Canvas => {
            painter.rect_stroke(
                Rect::from_center_size(c, Vec2::splat(9.0)),
                CornerRadius::ZERO,
                thin,
                StrokeKind::Middle,
            );
            painter.line_segment([p(c, -6.5, -2.0), p(c, 6.5, -2.0)], thin);
            painter.line_segment([p(c, -6.5, 2.0), p(c, 6.5, 2.0)], thin);
        }
        ActionIcon::ZoomOut => {
            painter.line_segment([p(c, -4.0, 0.0), p(c, 4.0, 0.0)], Stroke::new(1.5, color));
        }
        ActionIcon::ZoomIn => {
            painter.line_segment([p(c, -4.0, 0.0), p(c, 4.0, 0.0)], Stroke::new(1.5, color));
            painter.line_segment([p(c, 0.0, -4.0), p(c, 0.0, 4.0)], Stroke::new(1.5, color));
        }
        ActionIcon::Fill => {
            // Diagonal half-filled rounded square
            let r = Rect::from_center_size(c, Vec2::splat(10.0));
            let tri = vec![r.left_top(), r.right_top(), r.left_bottom()];
            painter.add(PathShape::convex_polygon(tri, color, Stroke::NONE));
            painter.rect_stroke(
                r,
                CornerRadius::same(2),
                st,
                eframe::egui::StrokeKind::Middle,
            );
        }
        ActionIcon::AlignLeft => {
            painter.line_segment([p(c, -5.0, -3.5), p(c, 5.0, -3.5)], st);
            painter.line_segment([p(c, -5.0, 0.0), p(c, 1.8, 0.0)], st);
            painter.line_segment([p(c, -5.0, 3.5), p(c, 3.6, 3.5)], st);
        }
        ActionIcon::AlignCenter => {
            painter.line_segment([p(c, -5.0, -3.5), p(c, 5.0, -3.5)], st);
            painter.line_segment([p(c, -2.8, 0.0), p(c, 2.8, 0.0)], st);
            painter.line_segment([p(c, -4.0, 3.5), p(c, 4.0, 3.5)], st);
        }
        ActionIcon::AlignRight => {
            painter.line_segment([p(c, -5.0, -3.5), p(c, 5.0, -3.5)], st);
            painter.line_segment([p(c, -1.8, 0.0), p(c, 5.0, 0.0)], st);
            painter.line_segment([p(c, -3.6, 3.5), p(c, 5.0, 3.5)], st);
        }
        ActionIcon::CalloutRoundRect => {
            let r = Rect::from_center_size(c, Vec2::new(11.0, 7.6));
            painter.rect_stroke(
                r,
                CornerRadius::same(2),
                st,
                eframe::egui::StrokeKind::Middle,
            );
        }
        ActionIcon::CalloutEllipse => {
            let n = 20;
            let pts: Vec<Pos2> = (0..n)
                .map(|i| {
                    let a = (i as f32 / n as f32) * std::f32::consts::TAU;
                    p(c, a.cos() * 5.6, a.sin() * 3.8)
                })
                .collect();
            painter.add(PathShape::closed_line(pts, st));
        }
        ActionIcon::CalloutPill => {
            let r = Rect::from_center_size(c, Vec2::new(11.6, 6.8));
            painter.rect_stroke(
                r,
                CornerRadius::same(4),
                st,
                eframe::egui::StrokeKind::Middle,
            );
        }
        ActionIcon::ArrowOpen => {
            painter.line_segment([p(c, -5.2, 0.0), p(c, 4.5, 0.0)], st);
            painter.line_segment([p(c, 4.5, 0.0), p(c, 1.0, -3.2)], st);
            painter.line_segment([p(c, 4.5, 0.0), p(c, 1.0, 3.2)], st);
        }
        ActionIcon::ArrowFilled => {
            painter.line_segment([p(c, -5.2, 0.0), p(c, 1.5, 0.0)], st);
            let tri = vec![p(c, 5.2, 0.0), p(c, 0.5, -3.4), p(c, 0.5, 3.4)];
            painter.add(PathShape::convex_polygon(tri, color, Stroke::NONE));
        }
        ActionIcon::BlurPixelate => {
            let s = 3.6;
            let g = 1.2;
            let a = color;
            let b = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 90);
            painter.rect_filled(
                Rect::from_min_size(p(c, -s - g * 0.5, -s - g * 0.5), Vec2::splat(s)),
                CornerRadius::same(1),
                a,
            );
            painter.rect_filled(
                Rect::from_min_size(p(c, g * 0.5, -s - g * 0.5), Vec2::splat(s)),
                CornerRadius::same(1),
                b,
            );
            painter.rect_filled(
                Rect::from_min_size(p(c, -s - g * 0.5, g * 0.5), Vec2::splat(s)),
                CornerRadius::same(1),
                b,
            );
            painter.rect_filled(
                Rect::from_min_size(p(c, g * 0.5, g * 0.5), Vec2::splat(s)),
                CornerRadius::same(1),
                a,
            );
        }
        ActionIcon::BlurSmooth => {
            painter.circle_stroke(c, 5.0, thin);
            painter.circle_filled(
                c,
                2.5,
                Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 150),
            );
        }
        ActionIcon::LayerFront => {
            let back = Rect::from_center_size(p(c, 1.8, 1.8), Vec2::splat(7.0));
            let front = Rect::from_center_size(p(c, -1.8, -1.8), Vec2::splat(7.0));
            painter.rect_stroke(
                back,
                CornerRadius::same(1),
                thin,
                eframe::egui::StrokeKind::Middle,
            );
            painter.rect_filled(front, CornerRadius::same(1), color);
        }
        ActionIcon::LayerBack => {
            let back = Rect::from_center_size(p(c, 1.8, 1.8), Vec2::splat(7.0));
            let front = Rect::from_center_size(p(c, -1.8, -1.8), Vec2::splat(7.0));
            painter.rect_filled(
                back,
                CornerRadius::same(1),
                Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 180),
            );
            painter.rect_stroke(
                front,
                CornerRadius::same(1),
                st,
                eframe::egui::StrokeKind::Middle,
            );
        }
        ActionIcon::Duplicate => {
            let back = Rect::from_center_size(p(c, -1.8, -1.8), Vec2::splat(7.2));
            let front = Rect::from_center_size(p(c, 1.8, 1.8), Vec2::splat(7.2));
            painter.rect_stroke(
                back,
                CornerRadius::same(1),
                thin,
                eframe::egui::StrokeKind::Middle,
            );
            painter.rect_stroke(
                front,
                CornerRadius::same(1),
                st,
                eframe::egui::StrokeKind::Middle,
            );
            painter.line_segment([p(c, 1.8, -0.3), p(c, 1.8, 3.9)], thin);
            painter.line_segment([p(c, -0.3, 1.8), p(c, 3.9, 1.8)], thin);
        }
        ActionIcon::Trash => {
            painter.line_segment([p(c, -4.6, -3.2), p(c, 4.6, -3.2)], st);
            painter.line_segment([p(c, -1.6, -5.0), p(c, 1.6, -5.0)], st);
            let body = vec![
                p(c, -3.6, -3.0),
                p(c, -2.8, 5.0),
                p(c, 2.8, 5.0),
                p(c, 3.6, -3.0),
            ];
            painter.add(PathShape::line(body, st));
            painter.line_segment([p(c, -1.1, -0.8), p(c, -0.9, 3.0)], thin);
            painter.line_segment([p(c, 1.1, -0.8), p(c, 0.9, 3.0)], thin);
        }
        ActionIcon::CheckCircle => {
            painter.circle_filled(c, 7.5, color);
            let chk = Stroke::new(1.7, crate::theme::on_success());
            painter.line_segment([p(c, -3.2, 0.2), p(c, -0.8, 2.6)], chk);
            painter.line_segment([p(c, -0.8, 2.6), p(c, 3.6, -2.4)], chk);
        }
    }
}
