//! Toolbar and action icons — Lucide geometry via `qs_icons` (vendored SVGs,
//! ISC licence) painted with `egui::Painter`. One geometry source shared with
//! the overlay's SDF renderer, so the same tool looks identical everywhere.

use eframe::egui::{epaint::PathShape, Color32, Painter, Pos2, Stroke};
use qs_doc::ObjectKind;
use qs_edit::Tool;
use qs_icons::StrokePath;

/// The tool whose icon represents a document object in the Layers list.
/// Every `ObjectKind` was created by a tool, so the mapping is total.
pub fn kind_tool(kind: &ObjectKind) -> Tool {
    match kind {
        ObjectKind::Pen { .. } => Tool::Pen,
        ObjectKind::Highlighter { .. } => Tool::Highlighter,
        ObjectKind::Arrow { .. } => Tool::Arrow,
        ObjectKind::Line { .. } => Tool::Line,
        ObjectKind::Rect { .. } => Tool::Rect,
        ObjectKind::Ellipse { .. } => Tool::Ellipse,
        ObjectKind::Text { .. } => Tool::Text,
        ObjectKind::Callout { .. } => Tool::Callout,
        ObjectKind::Step { .. } => Tool::Step,
        ObjectKind::Blur { .. } => Tool::Blur,
        ObjectKind::Magnify { .. } => Tool::Magnify,
        ObjectKind::Stamp { .. } => Tool::Stamp,
        ObjectKind::Eraser { .. } => Tool::Eraser,
        ObjectKind::Spotlight { .. } => Tool::Spotlight,
        ObjectKind::Fill { .. } => Tool::Fill,
        ObjectKind::SmartMove { .. } => Tool::SmartMove,
        ObjectKind::Polygon { kind, .. } => match kind {
            qs_doc::PolyKind::Triangle => Tool::Triangle,
            qs_doc::PolyKind::Diamond => Tool::Diamond,
            qs_doc::PolyKind::Star => Tool::Star,
            qs_doc::PolyKind::Hexagon => Tool::Hexagon,
        },
    }
}

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
    /// App menu: three bars.
    Menu,
    /// Layer visibility: an eye, with a slash when hidden.
    Eye,
    EyeOff,
    /// Layer lock: a padlock, open when unlocked.
    Lock,
    Unlock,
    /// Window close: crisp ×.
    Close,
    /// Document tab glyph (panel with a top strip) for the docbar name.
    DocTab,
    /// Corner brackets + text lines — Auto-Redact in Quick Actions.
    ScanLine,
    /// Four-point star — Simplify UI in Quick Actions.
    Sparkles,
}

/// Logical box each icon is drawn into, centred on `c`.
const BOX: f32 = 12.5;
const STROKE_W: f32 = 1.5;

/// Paint Lucide `paths` centred on `c` — shared by the tool icons, the
/// action icons and the transform/star glyphs elsewhere in the editor.
pub fn draw_paths(painter: &Painter, c: Pos2, paths: &[StrokePath], color: Color32) {
    let k = BOX / qs_icons::VIEWBOX;
    let st = Stroke::new(STROKE_W, color);
    let fill = |p: &(f32, f32)| Pos2::new(c.x + (p.0 - 12.0) * k, c.y + (p.1 - 12.0) * k);
    for sp in paths {
        let pts: Vec<Pos2> = sp.pts.iter().map(fill).collect();
        if sp.fill {
            painter.add(PathShape::convex_polygon(pts, color, Stroke::NONE));
        } else if sp.closed {
            painter.add(PathShape::closed_line(pts, st));
        } else {
            painter.add(PathShape::line(pts, st));
        }
    }
}

#[inline]
fn p(c: Pos2, dx: f32, dy: f32) -> Pos2 {
    Pos2::new(c.x + dx, c.y + dy)
}

fn action_paths(icon: ActionIcon) -> &'static [StrokePath] {
    match icon {
        ActionIcon::Undo => qs_icons::UNDO_2,
        ActionIcon::Redo => qs_icons::REDO_2,
        ActionIcon::Copy => qs_icons::COPY,
        ActionIcon::Save => qs_icons::SAVE,
        ActionIcon::Export => qs_icons::IMAGE_DOWN,
        ActionIcon::ZoomOut => qs_icons::ZOOM_OUT,
        ActionIcon::ZoomIn => qs_icons::ZOOM_IN,
        ActionIcon::Fill => qs_icons::PAINT_BUCKET,
        ActionIcon::AlignLeft => qs_icons::ALIGN_START_VERTICAL,
        ActionIcon::AlignCenter => qs_icons::ALIGN_CENTER_VERTICAL,
        ActionIcon::AlignRight => qs_icons::ALIGN_END_VERTICAL,
        ActionIcon::CalloutRoundRect => qs_icons::SQUARE,
        ActionIcon::CalloutEllipse => qs_icons::CIRCLE,
        ActionIcon::CalloutPill => qs_icons::PILL,
        ActionIcon::ArrowOpen => qs_icons::ARROW_UP_RIGHT,
        ActionIcon::ArrowFilled => qs_icons::ARROW_FILLED,
        ActionIcon::BlurPixelate => qs_icons::GRID_2X2,
        ActionIcon::BlurSmooth => qs_icons::BLEND,
        ActionIcon::LayerFront => qs_icons::BRING_TO_FRONT,
        ActionIcon::LayerBack => qs_icons::SEND_TO_BACK,
        ActionIcon::Duplicate => qs_icons::COPY_PLUS,
        ActionIcon::Trash => qs_icons::TRASH,
        ActionIcon::CheckCircle => qs_icons::CIRCLE_CHECK,
        ActionIcon::Open => qs_icons::FOLDER_OPEN,
        ActionIcon::SaveAs => qs_icons::SAVE_ALL,
        ActionIcon::Canvas => qs_icons::PROPORTIONS,
        ActionIcon::Menu => qs_icons::MENU,
        ActionIcon::Eye => qs_icons::EYE,
        ActionIcon::EyeOff => qs_icons::EYE_OFF,
        ActionIcon::Lock => qs_icons::LOCK,
        ActionIcon::Unlock => qs_icons::LOCK_OPEN,
        ActionIcon::Close => qs_icons::X,
        ActionIcon::DocTab => qs_icons::PANEL_TOP,
        ActionIcon::ScanLine => qs_icons::SCAN_LINE,
        ActionIcon::Sparkles => qs_icons::SPARKLES,
    }
}

/// Same as `draw_tool_icon` but scales the logical box — palette glyphs read
/// bigger than layer-row glyphs (17px vs 13px in the mockup).
pub fn draw_tool_icon_sz(painter: &Painter, c: Pos2, tool: Tool, color: Color32, box_px: f32) {
    if tool == Tool::Step {
        // Circle badge + geometric '1', scaled to the caller's box.
        let s = box_px / 12.5;
        let st = Stroke::new(1.3, color);
        let thin = Stroke::new(1.1, color);
        painter.circle_stroke(c, 5.5 * s, st);
        painter.line_segment(
            [p(c, -1.2 * s, -1.5 * s), p(c, 0.3 * s, -2.8 * s)],
            thin,
        );
        painter.line_segment(
            [p(c, 0.3 * s, -2.8 * s), p(c, 0.3 * s, 2.8 * s)],
            Stroke::new(1.2, color),
        );
        painter.line_segment(
            [p(c, -1.5 * s, 2.8 * s), p(c, 1.9 * s, 2.8 * s)],
            thin,
        );
        return;
    }
    let k = box_px / qs_icons::VIEWBOX;
    let st = Stroke::new(STROKE_W, color);
    let fill = |p: &(f32, f32)| Pos2::new(c.x + (p.0 - 12.0) * k, c.y + (p.1 - 12.0) * k);
    for sp in tool.icon() {
        let pts: Vec<Pos2> = sp.pts.iter().map(fill).collect();
        if sp.fill {
            painter.add(PathShape::convex_polygon(pts, color, Stroke::NONE));
        } else if sp.closed {
            painter.add(PathShape::closed_line(pts, st));
        } else {
            painter.add(PathShape::line(pts, st));
        }
    }
}

/// Scaled variant of `draw_action_icon` for chrome that wants a larger glyph.
pub fn draw_action_icon_sz(
    painter: &Painter,
    c: Pos2,
    icon: ActionIcon,
    color: Color32,
    box_px: f32,
) {
    let k = box_px / qs_icons::VIEWBOX;
    let st = Stroke::new(STROKE_W, color);
    let fill = |p: &(f32, f32)| Pos2::new(c.x + (p.0 - 12.0) * k, c.y + (p.1 - 12.0) * k);
    for sp in action_paths(icon) {
        let pts: Vec<Pos2> = sp.pts.iter().map(fill).collect();
        if sp.fill {
            painter.add(PathShape::convex_polygon(pts, color, Stroke::NONE));
        } else if sp.closed {
            painter.add(PathShape::closed_line(pts, st));
        } else {
            painter.add(PathShape::line(pts, st));
        }
    }
}

pub fn draw_tool_icon(painter: &Painter, c: Pos2, tool: Tool, color: Color32) {
    if tool == Tool::Step {
        // Circle badge + crisp geometric '1' — composed, not a Lucide icon.
        let st = Stroke::new(1.3, color);
        let thin = Stroke::new(1.1, color);
        painter.circle_stroke(c, 5.5, st);
        painter.line_segment([p(c, -1.2, -1.5), p(c, 0.3, -2.8)], thin);
        painter.line_segment([p(c, 0.3, -2.8), p(c, 0.3, 2.8)], Stroke::new(1.2, color));
        painter.line_segment([p(c, -1.5, 2.8), p(c, 1.9, 2.8)], thin);
        return;
    }
    draw_paths(painter, c, tool.icon(), color);
}

pub fn draw_action_icon(painter: &Painter, c: Pos2, icon: ActionIcon, color: Color32) {
    draw_paths(painter, c, action_paths(icon), color);
}
