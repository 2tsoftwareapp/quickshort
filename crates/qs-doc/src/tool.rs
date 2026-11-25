//! The drawing-tool model shared by every surface: the overlay's floating
//! dock and the editor's palette arm the same enum, so a tool's key, label,
//! icon and drag gesture are defined exactly once.

use crate::draft::Gesture;

/// Tools that arm on the canvas. The overlay only exposes a subset
/// (`Select`, `Pen`, `Arrow`, `Rect`, `Ellipse`, `Highlighter`, `Text`,
/// `Blur`, `Step`); the rest are editor-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    Select,
    Pen,
    Highlighter,
    Arrow,
    Line,
    Rect,
    Ellipse,
    Text,
    Callout,
    Step,
    Blur,
    Magnify,
    Spotlight,
    Stamp,
    Eraser,
    Crop,
    Cutout,
    SelectRegion,
    /// Paint bucket: click fills the region under the cursor.
    Fill,
    /// Move existing canvas content: drag a box, then click where it lands.
    SmartMove,
    /// Polygon shapes (mockup-locked additions): drawn inside the drag rect.
    Triangle,
    Diamond,
    Star,
    Hexagon,
}

impl Tool {
    /// Every tool, for iterating keyboard shortcuts.
    pub const ALL: &[Tool] = &[
        Tool::Select,
        Tool::SelectRegion,
        Tool::SmartMove,
        Tool::Pen,
        Tool::Highlighter,
        Tool::Eraser,
        Tool::Line,
        Tool::Arrow,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Fill,
        Tool::Text,
        Tool::Callout,
        Tool::Step,
        Tool::Stamp,
        Tool::Blur,
        Tool::Spotlight,
        Tool::Magnify,
        Tool::Cutout,
        Tool::Crop,
        Tool::Triangle,
        Tool::Diamond,
        Tool::Star,
        Tool::Hexagon,
    ];

    /// Single-key shortcut (no modifiers) that arms this tool — the one
    /// source of truth for both key dispatch and the palette subscript.
    /// The overlay shares these letters except where they collide with its
    /// own action keys (see `qs_overlay::toolbar::tool_from_key`).
    pub fn key(self) -> char {
        match self {
            Tool::Select => 'v',
            Tool::SelectRegion => 'm',
            Tool::SmartMove => 'y',
            Tool::Pen => 'p',
            Tool::Highlighter => 'h',
            Tool::Eraser => 'e',
            Tool::Line => 'l',
            Tool::Arrow => 'a',
            Tool::Rect => 'r',
            Tool::Ellipse => 'o',
            Tool::Fill => 'u',
            Tool::Text => 't',
            Tool::Callout => 'c',
            Tool::Step => 's',
            Tool::Stamp => 'g',
            Tool::Blur => 'b',
            Tool::Spotlight => 'f',
            Tool::Magnify => 'i',
            Tool::Cutout => 'x',
            Tool::Crop => 'k',
            Tool::Triangle => 'n',
            Tool::Diamond => 'd',
            Tool::Star => 'j',
            Tool::Hexagon => 'w',
        }
    }

    /// Human name shown in the tool-tag and tooltips.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::SelectRegion => "Select Region",
            Tool::SmartMove => "Smart Move",
            Tool::Pen => "Pen",
            Tool::Highlighter => "Highlight",
            Tool::Eraser => "Eraser",
            Tool::Line => "Line",
            Tool::Arrow => "Arrow",
            Tool::Rect => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::Fill => "Paint Bucket",
            Tool::Text => "Text",
            Tool::Callout => "Callout Bubble",
            Tool::Step => "Step Counter",
            Tool::Stamp => "Stamp",
            Tool::Blur => "Blur / Redact",
            Tool::Spotlight => "Spotlight",
            Tool::Magnify => "Magnify Lens",
            Tool::Cutout => "Cutout Strip",
            Tool::Crop => "Crop / Trim",
            Tool::Triangle => "Triangle",
            Tool::Diamond => "Diamond",
            Tool::Star => "Star",
            Tool::Hexagon => "Hexagon",
        }
    }

    /// Tooltip "Label (K)" — the single spelling shared by palette hovers
    /// and shortcut hints.
    pub fn tooltip(self) -> String {
        format!("{} ({})", self.label(), self.key().to_ascii_uppercase())
    }

    /// Lucide icon geometry for this tool — shared so every surface draws
    /// the same glyph. `Step` returns an empty path: its numbered badge is
    /// drawn procedurally (circle + digit), not an icon path.
    pub fn icon(self) -> &'static [qs_icons::StrokePath] {
        match self {
            Tool::Select => qs_icons::MOUSE_POINTER_2,
            Tool::Pen => qs_icons::PENCIL,
            Tool::Fill => qs_icons::PAINT_BUCKET,
            Tool::SmartMove => qs_icons::MOVE,
            Tool::Highlighter => qs_icons::HIGHLIGHTER,
            Tool::Arrow => qs_icons::ARROW_UP_RIGHT,
            Tool::Line => qs_icons::LINE,
            Tool::Rect => qs_icons::SQUARE,
            Tool::Ellipse => qs_icons::CIRCLE,
            Tool::Eraser => qs_icons::ERASER,
            Tool::Stamp => qs_icons::STAMP,
            Tool::Spotlight => qs_icons::SUN,
            Tool::Magnify => qs_icons::ZOOM_IN,
            Tool::Text => qs_icons::TYPE,
            Tool::Callout => qs_icons::MESSAGE_SQUARE,
            Tool::Step => &[],
            Tool::Blur => qs_icons::PLUS,
            Tool::Crop => qs_icons::CROP,
            Tool::Cutout => qs_icons::SCISSORS,
            Tool::SelectRegion => qs_icons::SQUARE_DASHED,
            Tool::Triangle => qs_icons::TRIANGLE,
            Tool::Diamond => qs_icons::DIAMOND,
            Tool::Star => qs_icons::STAR,
            Tool::Hexagon => qs_icons::HEXAGON,
        }
    }

    /// The drag gesture this tool drives on the canvas, if any — the shared
    /// state machine in [`crate::draft`]. Tools that act on the document
    /// itself (select, crop, fill, region pickers) return `None`.
    pub fn gesture(self) -> Option<Gesture> {
        Some(match self {
            Tool::Pen | Tool::Highlighter | Tool::Eraser => Gesture::Stroke,
            Tool::Arrow | Tool::Line => Gesture::Segment,
            Tool::Rect
            | Tool::Ellipse
            | Tool::Blur
            | Tool::Magnify
            | Tool::Spotlight
            | Tool::Stamp
            | Tool::Callout
            | Tool::Triangle
            | Tool::Diamond
            | Tool::Star
            | Tool::Hexagon => Gesture::Rect,
            Tool::Step | Tool::Text => Gesture::Point,
            _ => return None,
        })
    }
}
