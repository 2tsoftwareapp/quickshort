//! Annotation operations: pointer gestures, text editing, selection, groups,
//! z-order and object-creating commands.
use qs_doc::draft::{self, Draft};
use qs_doc::{
    Fill, GroupId, Object, ObjectId, ObjectKind, PointF, RectF, Stroke, TextStyle, MARKER_ALPHA,
};
use qs_render::{bounds, text, text_block, CALLOUT_PAD};

use crate::history::Snap;
use crate::{
    contrast, hit, simplify, Alignment, Axis, Editor, HandleId, Motion, TextEdit, Tool, ZOrder,
    DEFAULT_CALLOUT, MARKER_K,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Drag {
    None,
    /// `items`: (id, kind at drag start) for every moved object.
    Move {
        items: Vec<(ObjectId, ObjectKind)>,
        start: PointF,
    },
    Marquee {
        start: PointF,
    },
    Handle {
        id: ObjectId,
        h: HandleId,
        orig: ObjectKind,
    },
    Create {
        id: ObjectId,
        draw: Draft,
    },
    TextSelect,
    Crop {
        start: PointF,
    },
    Cutout {
        start: PointF,
    },
    /// Smart Move: dragging out the region whose content will move.
    SmartMoveSrc {
        start: PointF,
    },
    Region {
        start: PointF,
    },
}

mod actions;
mod gestures;
mod selection;
mod text_edit;


fn degenerate(k: &ObjectKind) -> bool {
    match k {
        ObjectKind::Arrow { a, b, .. } | ObjectKind::Line { a, b, .. } => a.dist(*b) < 3.0,
        ObjectKind::Rect { r, .. }
        | ObjectKind::Ellipse { r, .. }
        | ObjectKind::Blur { r, .. }
        | ObjectKind::Magnify { rect: r, .. }
        | ObjectKind::Spotlight { rect: r, .. }
        | ObjectKind::Stamp { rect: r, .. }
        | ObjectKind::Polygon { r, .. } => r.w.abs() < 3.0 || r.h.abs() < 3.0,
        ObjectKind::Eraser { pts, .. } => pts.len() < 2 || pts.iter().all(|p| p.dist(pts[0]) < 1.0),
        _ => false,
    }
}
