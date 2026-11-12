//! Editable capture document.
//!
//! A [`Document`] is the vector description of a capture: the objects drawn
//! on top of an immutable base image. Nothing here touches pixels or windows;
//! rendering lives in `qs-render`, editing in `qs-edit`.
//!
//! Coordinates are `f32` logical pixels of the captured region, origin at the
//! top-left of the base image. `Document::scale` maps logical to the base
//! image's physical pixels (2.0 on a Retina / 200% capture).

mod geom;
pub mod qsdoc;

pub use geom::{PointF, RectF, SizeF};

use serde::{Deserialize, Serialize};

/// Current on-disk document version. Bump + add a migration in
/// [`Document::migrate`] when the schema changes incompatibly.
pub const DOC_VERSION: u32 = 2;

/// Overlay palette (kept identical to `qs-overlay`).
pub const PALETTE: [u32; 8] = [
    0xFF453A, 0xFF9F0A, 0xFFD60A, 0x30D158, 0x0A84FF, 0xBF5AF2, 0xFFFFFF, 0x1C1C1E,
];
/// Stroke widths S/M/L and highlighter widths S/M/L, logical px.
pub const SIZES: [f32; 3] = [2.0, 3.6, 6.0];
pub const MARKER_SIZES: [f32; 3] = [6.0, 9.5, 14.0];
pub const MARKER_ALPHA: f32 = 0.42;
pub const TEXT_SIZES: [f32; 6] = [12.0, 14.0, 18.0, 24.0, 32.0, 48.0];
/// Default pixelate block size, logical px.
pub const PIX_BLOCK: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    /// 0xRRGGBB
    pub rgb: u32,
    pub alpha: f32,
    pub width: f32,
}

impl Stroke {
    pub fn new(rgb: u32, width: f32) -> Self {
        Self {
            rgb,
            alpha: 1.0,
            width,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "type")]
pub enum Fill {
    #[default]
    None,
    Solid {
        rgb: u32,
        alpha: f32,
    },
}

impl Fill {
    pub fn is_none(&self) -> bool {
        matches!(self, Fill::None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub px: f32,
    pub bold: bool,
    pub rgb: u32,
    #[serde(default)]
    pub align: Align,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            px: 14.0,
            bold: false,
            rgb: PALETTE[0],
            align: Align::Left,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CalloutShape {
    #[default]
    RoundRect,
    Ellipse,
    Pill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ArrowHead {
    /// two open wings (overlay parity)
    #[default]
    Open,
    /// solid triangle
    Filled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BlurMode {
    #[default]
    Pixelate,
    Gaussian,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ObjectKind {
    Pen {
        pts: Vec<PointF>,
        stroke: Stroke,
    },
    Highlighter {
        pts: Vec<PointF>,
        stroke: Stroke,
    },
    Arrow {
        a: PointF,
        b: PointF,
        stroke: Stroke,
        #[serde(default)]
        head: ArrowHead,
    },
    Line {
        a: PointF,
        b: PointF,
        stroke: Stroke,
    },
    Rect {
        r: RectF,
        stroke: Stroke,
        #[serde(default)]
        fill: Fill,
        #[serde(default = "default_radius")]
        radius: f32,
    },
    Ellipse {
        r: RectF,
        stroke: Stroke,
        #[serde(default)]
        fill: Fill,
    },
    /// `pos` is the top-left of the text block. `wrap_w`: wrap lines at
    /// this logical width (None = only explicit newlines).
    Text {
        pos: PointF,
        text: String,
        style: TextStyle,
        #[serde(default)]
        wrap_w: Option<f32>,
        #[serde(default)]
        fill: Fill,
    },
    /// Speech bubble: a body with a tail pointing at `tail`, text inside.
    Callout {
        shape: CalloutShape,
        body: RectF,
        tail: PointF,
        stroke: Stroke,
        fill: Fill,
        text: String,
        style: TextStyle,
    },
    /// Numbered badge. The number is the object's order among all steps
    /// (auto-renumbered) plus `Document::step_start - 1`.
    Step {
        center: PointF,
        radius: f32,
        rgb: u32,
        text_rgb: u32,
    },
    /// Non-destructive blur/pixelate of whatever lies under `r`.
    /// `strength` = pixelate block size or gaussian radius, logical px.
    Blur {
        r: RectF,
        mode: BlurMode,
        strength: f32,
    },
}

fn default_radius() -> f32 {
    1.5
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: ObjectId,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    pub kind: ObjectKind,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Source {
    #[default]
    Region,
    Full,
    Scroll,
    Imported,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Meta {
    /// unix seconds
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// recognized text, for library search
    #[serde(default)]
    pub ocr_text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub version: u32,
    /// physical px per logical px of the base image
    pub scale: f32,
    /// logical size of the base image
    pub base_size: SizeF,
    /// Output region in logical coords. Defaults to the whole base image;
    /// smaller = crop, larger = extended canvas filled with `background`.
    pub view: RectF,
    /// User-drawn selection region (overlay only, not an object).
    #[serde(default)]
    pub region: Option<RectF>,
    /// 0xAARRGGBB fill for canvas outside the base image
    #[serde(default = "white")]
    pub background: u32,
    /// z-order: last = top
    pub objects: Vec<Object>,
    pub next_id: u64,
    #[serde(default = "one")]
    pub step_start: u32,
    #[serde(default)]
    pub meta: Meta,
}

fn white() -> u32 {
    0xFFFF_FFFF
}
fn one() -> u32 {
    1
}

impl Document {
    /// New document over a base image of `px_w`×`px_h` physical pixels.
    pub fn new(px_w: u32, px_h: u32, scale: f32) -> Self {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let base_size = SizeF::new(px_w as f32 / scale, px_h as f32 / scale);
        Self {
            version: DOC_VERSION,
            scale,
            base_size,
            view: RectF::new(0.0, 0.0, base_size.w, base_size.h),
            region: None,
            background: white(),
            objects: Vec::new(),
            next_id: 1,
            step_start: 1,
            meta: Meta::default(),
        }
    }

    pub fn alloc_id(&mut self) -> ObjectId {
        let id = ObjectId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Append an object on top, assigning a fresh id. Returns the id.
    pub fn push(&mut self, kind: ObjectKind) -> ObjectId {
        let id = self.alloc_id();
        self.objects.push(Object {
            id,
            visible: true,
            locked: false,
            kind,
        });
        id
    }

    pub fn index_of(&self, id: ObjectId) -> Option<usize> {
        self.objects.iter().position(|o| o.id == id)
    }

    pub fn get(&self, id: ObjectId) -> Option<&Object> {
        self.objects.iter().find(|o| o.id == id)
    }

    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut Object> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    pub fn remove(&mut self, id: ObjectId) -> Option<Object> {
        let i = self.index_of(id)?;
        Some(self.objects.remove(i))
    }

    /// Display number of a step object (1-based order among steps).
    pub fn step_number(&self, id: ObjectId) -> Option<u32> {
        let mut n = self.step_start;
        for o in &self.objects {
            if let ObjectKind::Step { .. } = o.kind {
                if o.id == id {
                    return Some(n);
                }
                n += 1;
            }
        }
        None
    }

    /// Structural checks a loaded document must pass. Errors are labelled,
    /// never panics.
    pub fn validate(&self) -> anyhow::Result<()> {
        use anyhow::bail;
        if !(self.scale.is_finite() && self.scale > 0.0) {
            bail!("invalid scale {}", self.scale);
        }
        if !self.base_size.is_finite() || !self.view.is_finite() {
            bail!("non-finite canvas geometry");
        }
        let mut seen = std::collections::HashSet::new();
        for o in &self.objects {
            if !seen.insert(o.id) {
                bail!("duplicate object id {}", o.id.0);
            }
            if o.id.0 >= self.next_id {
                bail!("object id {} >= next_id {}", o.id.0, self.next_id);
            }
            if !o.kind.is_finite() {
                bail!("object {} has non-finite geometry", o.id.0);
            }
        }
        Ok(())
    }

    /// Upgrade an older document in memory.
    pub fn migrate(mut self) -> anyhow::Result<Self> {
        if self.version > DOC_VERSION {
            anyhow::bail!(
                "document version {} is newer than this build supports ({DOC_VERSION})",
                self.version
            );
        }
        // v1 is the first version — nothing to migrate yet.
        self.version = DOC_VERSION;
        Ok(self)
    }

    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(s: &str) -> anyhow::Result<Self> {
        let d: Document = serde_json::from_str(s)?;
        let d = d.migrate()?;
        d.validate()?;
        Ok(d)
    }
}

impl ObjectKind {
    pub fn is_finite(&self) -> bool {
        use ObjectKind::*;
        match self {
            Pen { pts, stroke } | Highlighter { pts, stroke } => {
                stroke.width.is_finite() && pts.iter().all(|p| p.is_finite())
            }
            Arrow { a, b, stroke, .. } | Line { a, b, stroke } => {
                a.is_finite() && b.is_finite() && stroke.width.is_finite()
            }
            Rect { r, stroke, radius, .. } => {
                r.is_finite() && stroke.width.is_finite() && radius.is_finite()
            }
            Ellipse { r, stroke, .. } => r.is_finite() && stroke.width.is_finite(),
            Text {
                pos, style, wrap_w, ..
            } => pos.is_finite() && style.px.is_finite() && wrap_w.is_none_or(f32::is_finite),
            Callout {
                body, tail, stroke, style, ..
            } => {
                body.is_finite()
                    && tail.is_finite()
                    && stroke.width.is_finite()
                    && style.px.is_finite()
            }
            Step { center, radius, .. } => center.is_finite() && radius.is_finite(),
            Blur { r, strength, .. } => r.is_finite() && strength.is_finite(),
        }
    }

    /// Short human label, for object lists.
    pub fn label(&self) -> &'static str {
        use ObjectKind::*;
        match self {
            Pen { .. } => "Pen",
            Highlighter { .. } => "Highlighter",
            Arrow { .. } => "Arrow",
            Line { .. } => "Line",
            Rect { .. } => "Rectangle",
            Ellipse { .. } => "Ellipse",
            Text { .. } => "Text",
            Callout { .. } => "Callout",
            Step { .. } => "Step",
            Blur { .. } => "Blur",
        }
    }

    /// Move by (dx, dy) logical px.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        use ObjectKind::*;
        match self {
            Pen { pts, .. } | Highlighter { pts, .. } => {
                for p in pts {
                    p.x += dx;
                    p.y += dy;
                }
            }
            Arrow { a, b, .. } | Line { a, b, .. } => {
                *a = a.offset(dx, dy);
                *b = b.offset(dx, dy);
            }
            Rect { r, .. } | Ellipse { r, .. } | Blur { r, .. } => *r = r.offset(dx, dy),
            Text { pos, .. } => *pos = pos.offset(dx, dy),
            Callout { body, tail, .. } => {
                *body = body.offset(dx, dy);
                *tail = tail.offset(dx, dy);
            }
            Step { center, .. } => *center = center.offset(dx, dy),
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            ObjectKind::Text { text, .. } | ObjectKind::Callout { text, .. } => Some(text),
            _ => None,
        }
    }

    pub fn text_mut(&mut self) -> Option<&mut String> {
        match self {
            ObjectKind::Text { text, .. } | ObjectKind::Callout { text, .. } => Some(text),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Document {
        let mut d = Document::new(800, 600, 2.0);
        let s = Stroke::new(PALETTE[0], 3.6);
        d.push(ObjectKind::Pen {
            pts: vec![PointF::new(1.0, 2.0), PointF::new(3.0, 4.0)],
            stroke: s,
        });
        d.push(ObjectKind::Highlighter {
            pts: vec![PointF::new(1.0, 2.0), PointF::new(30.0, 4.0)],
            stroke: Stroke {
                alpha: MARKER_ALPHA,
                ..s
            },
        });
        d.push(ObjectKind::Arrow {
            a: PointF::new(0.0, 0.0),
            b: PointF::new(50.0, 50.0),
            stroke: s,
            head: ArrowHead::Filled,
        });
        d.push(ObjectKind::Line {
            a: PointF::new(0.0, 0.0),
            b: PointF::new(5.0, 50.0),
            stroke: s,
        });
        d.push(ObjectKind::Rect {
            r: RectF::new(10.0, 10.0, 40.0, 20.0),
            stroke: s,
            fill: Fill::Solid {
                rgb: 0xFFFFFF,
                alpha: 0.5,
            },
            radius: 4.0,
        });
        d.push(ObjectKind::Ellipse {
            r: RectF::new(10.0, 10.0, 40.0, 20.0),
            stroke: s,
            fill: Fill::None,
        });
        d.push(ObjectKind::Text {
            pos: PointF::new(5.0, 5.0),
            text: "Tiếng Việt\nhai dòng".into(),
            style: TextStyle::default(),
            wrap_w: Some(120.0),
            fill: Fill::None,
        });
        d.push(ObjectKind::Callout {
            shape: CalloutShape::Pill,
            body: RectF::new(100.0, 100.0, 120.0, 50.0),
            tail: PointF::new(60.0, 200.0),
            stroke: s,
            fill: Fill::Solid {
                rgb: 0xFFFFFF,
                alpha: 1.0,
            },
            text: "Click here".into(),
            style: TextStyle::default(),
        });
        d.push(ObjectKind::Step {
            center: PointF::new(20.0, 20.0),
            radius: 12.0,
            rgb: PALETTE[0],
            text_rgb: 0xFFFFFF,
        });
        d.push(ObjectKind::Blur {
            r: RectF::new(0.0, 0.0, 30.0, 30.0),
            mode: BlurMode::Gaussian,
            strength: 6.0,
        });
        d.meta.tags = vec!["invoice".into()];
        d
    }

    #[test]
    fn json_round_trip_all_kinds() {
        let d = sample();
        let back = Document::from_json(&d.to_json().unwrap()).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn ids_unique_and_monotonic() {
        let mut d = sample();
        let a = d.push(ObjectKind::Step {
            center: PointF::new(0.0, 0.0),
            radius: 10.0,
            rgb: 0,
            text_rgb: 0,
        });
        d.remove(a);
        let b = d.alloc_id();
        assert!(b > a, "ids are never reused");
        d.validate().unwrap();
    }

    #[test]
    fn rejects_nan_and_duplicate_ids() {
        let mut d = sample();
        if let ObjectKind::Step { radius, .. } = &mut d.objects[8].kind {
            *radius = f32::NAN;
        }
        // NaN serializes as null -> parse error or validation error, never panic
        let j = d.to_json().unwrap();
        assert!(Document::from_json(&j).is_err());

        let mut d = sample();
        d.objects[1].id = d.objects[0].id;
        assert!(Document::from_json(&d.to_json().unwrap()).is_err());
    }

    #[test]
    fn newer_version_rejected() {
        let mut d = sample();
        d.version = DOC_VERSION + 1;
        let e = Document::from_json(&d.to_json().unwrap()).unwrap_err();
        assert!(e.to_string().contains("newer"));
    }

    #[test]
    fn unknown_fields_ignored_and_defaults_fill_in() {
        let j = r#"{"version":1,"scale":1.0,"base_size":{"w":10,"h":10},
            "view":{"x":0,"y":0,"w":10,"h":10},"objects":[
            {"id":1,"kind":{"type":"Rect","r":{"x":0,"y":0,"w":5,"h":5},
              "stroke":{"rgb":0,"alpha":1,"width":2},"future_field":7}}],
            "next_id":2,"some_new_thing":true}"#;
        let d = Document::from_json(j).unwrap();
        assert!(d.objects[0].visible);
        assert_eq!(d.step_start, 1);
        match &d.objects[0].kind {
            ObjectKind::Rect { fill, radius, .. } => {
                assert!(fill.is_none());
                assert_eq!(*radius, 1.5);
            }
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn steps_renumber_by_order() {
        let mut d = Document::new(100, 100, 1.0);
        let st = |x| ObjectKind::Step {
            center: PointF::new(x, 0.0),
            radius: 10.0,
            rgb: 0,
            text_rgb: 0,
        };
        let a = d.push(st(0.0));
        let b = d.push(st(1.0));
        let c = d.push(st(2.0));
        assert_eq!(d.step_number(c), Some(3));
        d.remove(b);
        assert_eq!(d.step_number(a), Some(1));
        assert_eq!(d.step_number(c), Some(2));
    }

    #[test]
    fn translate_moves_every_point() {
        let mut k = ObjectKind::Callout {
            shape: CalloutShape::RoundRect,
            body: RectF::new(0.0, 0.0, 10.0, 10.0),
            tail: PointF::new(20.0, 20.0),
            stroke: Stroke::new(0, 1.0),
            fill: Fill::None,
            text: String::new(),
            style: TextStyle::default(),
        };
        k.translate(5.0, -5.0);
        match k {
            ObjectKind::Callout { body, tail, .. } => {
                assert_eq!(body, RectF::new(5.0, -5.0, 10.0, 10.0));
                assert_eq!(tail, PointF::new(25.0, 15.0));
            }
            _ => unreachable!(),
        }
    }
}
