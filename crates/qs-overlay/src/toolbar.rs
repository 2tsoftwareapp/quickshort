//! Selection toolbar: circular action buttons docked to whichever selection
//! edge has room — bottom -> top -> right -> left, falling back inside the
//! selection's bottom-right corner. Rendered straight into the softbuffer
//! surface via SDF primitives at physical resolution (crisp on HiDPI).

use crate::annotate::PALETTE;
use crate::sdf::{ellipse_sdf, fill_cov, rrect_sdf, seg_sdf, stroke_cov, Canvas};
use qs_core::{Point, Rect};

/// Terminal actions — leave the overlay with a result.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Save,
    Copy,
    Ocr,
    /// Publish the capture through an SSH tunnel and copy its link
    Share,
    /// Repeated captures of this rect while the user scrolls, stitched
    /// into one tall image
    Scroll,
    /// Screencast of this rect, piped to ffmpeg until stopped
    Record,
    /// Open the capture in the full editor, annotations kept editable
    Edit,
}

/// Drawing tools — stay in the overlay and annotate the canvas.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    /// Pick, move and delete existing annotations
    Select,
    Pen,
    Arrow,
    Rect,
    Ellipse,
    Marker,
    Text,
    Pixelate,
    /// Numbered badge ①②③ for reproduction steps
    Step,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Tool(Tool),
    Act(Action),
    Undo,
    Redo,
    /// Stroke size level 0..=2 — options row, shown while a tool is armed
    Size(u8),
    /// Palette index — options row
    Color(usize),
    /// Bold toggle — options row, only while the text tool is armed
    Bold,
    /// Font size preset index into annotate::TEXT_SIZES — text tool only
    FontSize(u8),
    /// Opens the second row with the less-used tools and actions
    More,
}

impl Tool {
    /// Letter that arms the tool — the same letters as Quickshort Editor.
    pub fn from_key(ch: char) -> Option<Self> {
        Some(match ch {
            'v' => Tool::Select,
            'p' => Tool::Pen,
            'a' => Tool::Arrow,
            'r' => Tool::Rect,
            'o' => Tool::Ellipse,
            'h' => Tool::Marker,
            't' => Tool::Text,
            'b' => Tool::Pixelate,
            'n' => Tool::Step,
            _ => return None,
        })
    }
}

/// Everyday row: core tools | history | More, Edit, Done. Done (Copy) is
/// the highlighted primary — it copies and saves in one step. Ordered for
/// bug reports: box it, point at it, number the steps, note, blur.
const MAIN: &[Item] = &[
    Item::Tool(Tool::Rect),
    Item::Tool(Tool::Arrow),
    Item::Tool(Tool::Step),
    Item::Tool(Tool::Text),
    Item::Tool(Tool::Pixelate),
    Item::Tool(Tool::Select),
    Item::Undo,
    Item::Redo,
    Item::More,
    Item::Act(Action::Edit),
    Item::Act(Action::Copy),
];
/// MAIN indexes where the tools and the history groups end — dividers
const TOOLS_END: usize = 6;
const HISTORY_END: usize = 8;

/// Second row behind More: the remaining tools, then the special captures.
const MORE: &[Item] = &[
    Item::Tool(Tool::Pen),
    Item::Tool(Tool::Ellipse),
    Item::Tool(Tool::Marker),
    Item::Act(Action::Scroll),
    Item::Act(Action::Record),
    Item::Act(Action::Ocr),
    Item::Act(Action::Share),
];

const OPTS: &[Item] = &[
    Item::Size(0),
    Item::Size(1),
    Item::Size(2),
    Item::Color(0),
    Item::Color(1),
    Item::Color(2),
    Item::Color(3),
    Item::Color(4),
    Item::Color(5),
    Item::Color(6),
    Item::Color(7),
];

/// Done is a labeled pill in the horizontal dock, so "finish" reads at a glance.
const DONE_W: f64 = 64.0;
const DONE_LABEL: &str = "Done";
const DONE_PX: f64 = 13.0;
const R: f64 = 14.0; // button half-size (28×28px >= 24px hit target)
const GAP: f64 = qs_theme::space::XS; // 4px grid between buttons
const PAD: f64 = 6.0; // dock inner padding
const EDGE_GAP: f64 = qs_theme::space::SM; // 8px between selection edge and dock
const STROKE: f64 = 1.6; // SF-Symbol-weight icon stroke, logical px
// macOS 26 HUD: round buttons in a dock whose corners are concentric with
// them (button radius + dock padding), so a one-row dock reads as a capsule.
const BTN_RAD: f64 = R;
const DOCK_RAD: f64 = R + PAD;

pub struct Toolbar {
    /// bounding box in virtual logical coords
    pub rect: Rect,
    /// button hit rects in logical coords — main row then options row
    pub buttons: Vec<(Item, Rect)>,
    pub hover: Option<usize>,
    /// currently armed drawing tool (highlighted + shows the options row)
    pub tool: Option<Tool>,
    /// More is open — the second row shows `MORE` instead of options
    pub more: bool,
    /// selected size level / palette index for the next stroke
    pub size: u8,
    pub color: usize,
    /// bold state for the text tool
    pub bold: bool,
    /// selected index into annotate::TEXT_SIZES for the text tool
    pub font_size: u8,
    /// whether Undo/Redo have something to pop (dims the buttons otherwise)
    pub can_undo: bool,
    pub has_redo: bool,
    /// active-chip color — the configured accent
    pub accent: u32,
}

impl Toolbar {
    /// `mr` = logical rect of the monitor holding the selection.
    /// `opts` shows the second row (size + color) when a tool is armed.
    pub fn new(sel: Rect, mr: Rect, opts: bool) -> Self {
        let mut tb = Self {
            rect: Rect::default(),
            buttons: Vec::new(),
            hover: None,
            tool: None,
            more: false,
            size: 1,
            color: 0,
            bold: false,
            font_size: crate::annotate::TEXT_SIZE_DEFAULT as u8,
            can_undo: false,
            has_redo: false,
            accent: qs_theme::accent(),
        };
        tb.layout(sel, mr, opts);
        tb
    }

    /// Recompute rect + button slots — call when the options row toggles.
    pub fn layout(&mut self, sel: Rect, mr: Rect, opts: bool) {
        let d = R * 2.0;
        let n = MAIN.len() as f64;
        // the text tool swaps stroke sizes for real font sizes + a Bold
        // toggle; colors stay shared
        let opts_row: Vec<Item> = if self.more {
            MORE.to_vec()
        } else if self.tool == Some(Tool::Text) {
            (0..crate::annotate::TEXT_SIZES.len() as u8)
                .map(Item::FontSize)
                .chain([Item::Bold])
                .chain((0..PALETTE.len()).map(Item::Color))
                .collect()
        } else {
            OPTS.to_vec()
        };
        let on = opts_row.len() as f64;
        let second = opts || self.more;
        let rows = if second { 2.0 } else { 1.0 };
        // second row aligns under the main row, slightly narrower
        let row_w = |k: f64| PAD * 2.0 + k * d + (k - 1.0) * GAP;
        let on_eff = if second { on } else { 0.0 };
        let hh = PAD * 2.0 + rows * d + (rows - 1.0) * GAP * 0.5;
        let hw = (row_w(n) + DONE_W - d).max(row_w(on_eff));
        let (vw, vh) = (hh, row_w(n.max(on_eff)));
        let (sx0, sy0) = (sel.x as f64, sel.y as f64);
        let (sx1, sy1) = (sel.right() as f64, sel.bottom() as f64);
        let (mx0, my0) = (mr.x as f64, mr.y as f64);
        let (mx1, my1) = (mr.right() as f64, mr.bottom() as f64);
        let clamp = |v: f64, lo: f64, hi: f64| if lo <= hi { v.clamp(lo, hi) } else { hi };

        // (x, y, vertical, fits?)
        let candidates = [
            (
                clamp(sx1 - hw, mx0 + 2.0, mx1 - hw - 2.0),
                sy1 + EDGE_GAP,
                false,
                sy1 + EDGE_GAP + hh <= my1,
            ),
            (
                clamp(sx1 - hw, mx0 + 2.0, mx1 - hw - 2.0),
                sy0 - EDGE_GAP - hh,
                false,
                sy0 - EDGE_GAP - hh >= my0,
            ),
            (
                sx1 + EDGE_GAP,
                clamp(sy1 - vh, my0 + 2.0, my1 - vh - 2.0),
                true,
                sx1 + EDGE_GAP + vw <= mx1,
            ),
            (
                sx0 - EDGE_GAP - vw,
                clamp(sy1 - vh, my0 + 2.0, my1 - vh - 2.0),
                true,
                sx0 - EDGE_GAP - vw >= mx0,
            ),
            // inside bottom-right of the selection — always the fallback
            (
                clamp(sx1 - hw - EDGE_GAP, mx0 + 2.0, mx1 - hw - 2.0),
                clamp(sy1 - hh - EDGE_GAP, my0 + 2.0, my1 - hh - 2.0),
                false,
                true,
            ),
        ];

        let (x, y, vertical, _) = candidates.iter().find(|c| c.3).copied().unwrap();
        self.rect = Rect::new(
            x.round() as i32,
            y.round() as i32,
            (if vertical { vw } else { hw }).round() as u32,
            (if vertical { vh } else { hh }).round() as u32,
        );
        let row_step = d + GAP * 0.5;
        let place = |items: &[Item], second: bool, buttons: &mut Vec<(Item, Rect)>| {
            for (i, it) in items.iter().enumerate() {
                let off = PAD + i as f64 * (d + GAP);
                let off2 = PAD + if second { row_step } else { 0.0 };
                let r = if vertical {
                    Rect::new(
                        self.rect.x + off2.round() as i32,
                        self.rect.y + off.round() as i32,
                        d as u32,
                        d as u32,
                    )
                } else {
                    let w = if *it == Item::Act(Action::Copy) {
                        DONE_W
                    } else {
                        d
                    };
                    Rect::new(
                        self.rect.x + off.round() as i32,
                        self.rect.y + off2.round() as i32,
                        w as u32,
                        d as u32,
                    )
                };
                buttons.push((*it, r));
            }
        };
        self.buttons.clear();
        place(MAIN, false, &mut self.buttons);
        if second {
            place(&opts_row, true, &mut self.buttons);
        }
    }

    /// Index of the button under `p`, or `None`.
    pub fn hit(&self, p: Point) -> Option<usize> {
        self.buttons
            .iter()
            .position(|(_, r)| p.x >= r.x && p.x < r.right() && p.y >= r.y && p.y < r.bottom())
    }

    /// `p` inside the toolbar box (incl. padding) — clicks there should not
    /// start a new selection.
    pub fn contains(&self, p: Point) -> bool {
        let r = &self.rect;
        p.x >= r.x && p.x < r.right() && p.y >= r.y && p.y < r.bottom()
    }

    /// Paint unified Liquid Glass dock + buttons into the physical surface.
    pub fn paint(&self, buf: &mut [u32], sw: usize, sh: usize, vrect: Rect, scale: f64) {
        let mut cv = Canvas {
            buf,
            w: sw,
            h: sh,
            scale,
        };
        let dock_x = self.rect.x as f64 - vrect.x as f64;
        let dock_y = self.rect.y as f64 - vrect.y as f64;
        let dock_w = self.rect.w as f64;
        let dock_h = self.rect.h as f64;
        paint_dock(&mut cv, dock_x, dock_y, dock_w, dock_h);

        // Subtle vertical dividers between Tools | History | Actions (horizontal layout)
        if dock_w >= dock_h && self.buttons.len() >= MAIN.len() {
            for split_idx in [TOOLS_END, HISTORY_END] {
                let prev = self.buttons[split_idx - 1].1;
                let next = self.buttons[split_idx].1;
                let lx = (prev.right() as f64 + next.x as f64) * 0.5 - vrect.x as f64;
                let ly0 = prev.y as f64 + 5.0 - vrect.y as f64;
                let ly1 = prev.bottom() as f64 - 5.0 - vrect.y as f64;
                let px = (lx * scale).round() as i32;
                for py in (ly0 * scale) as i32..=(ly1 * scale) as i32 {
                    cv.blend(px, py, qs_theme::pal().border, 0.85);
                }
            }
        }

        for (i, (item, r)) in self.buttons.iter().enumerate() {
            let cx = (r.x as f64 + r.w as f64 / 2.0 - vrect.x as f64) * scale;
            let cy = (r.y as f64 + r.h as f64 / 2.0 - vrect.y as f64) * scale;
            let is_primary = matches!(item, Item::Act(Action::Copy));
            let active = matches!(item, Item::Tool(t) if self.tool == Some(*t))
                || matches!(item, Item::Size(s) if self.size == *s)
                || matches!(item, Item::Color(c) if self.color == *c)
                || matches!(item, Item::FontSize(s) if self.font_size == *s)
                || matches!(item, Item::Bold) && self.bold
                || matches!(item, Item::More)
                    && (self.more || self.tool.is_some_and(|t| MORE.contains(&Item::Tool(t))))
                || is_primary;
            let enabled = match item {
                Item::Undo => self.can_undo,
                Item::Redo => self.has_redo,
                _ => true,
            };
            if is_primary && r.w > r.h {
                paint_done(&mut cv, *r, vrect, self.hover == Some(i), self.accent);
                continue;
            }
            paint_button(
                &mut cv,
                cx,
                cy,
                R * scale,
                self.hover == Some(i),
                active,
                enabled,
                *item,
                self.accent,
            );
        }
    }
}

/// The labeled primary: accent pill with "Done".
fn paint_done(cv: &mut Canvas, r: Rect, vrect: Rect, hovered: bool, accent: u32) {
    let s = cv.scale;
    let p = qs_theme::pal();
    let (hw, hh) = (r.w as f64 * 0.5, r.h as f64 * 0.5);
    let cx = r.x as f64 + hw - vrect.x as f64;
    let cy = r.y as f64 + hh - vrect.y as f64;
    let (x0, y0) = (((cx - hw - 1.0) * s) as i32, ((cy - hh - 1.0) * s) as i32);
    let (x1, y1) = (((cx + hw + 1.0) * s) as i32, ((cy + hh + 1.0) * s) as i32);
    for py in y0..=y1 {
        for px in x0..=x1 {
            let (lx, ly) = ((px as f64 + 0.5) / s, (py as f64 + 0.5) / s);
            let a = fill_cov(rrect_sdf(lx, ly, cx, cy, hw, hh, BTN_RAD), s);
            if a > 0.0 {
                cv.blend(px, py, accent, a);
                if hovered {
                    cv.blend(px, py, p.on_accent, a * 0.12);
                }
            }
        }
    }
    label(cv, r, vrect, DONE_LABEL, DONE_PX, true, p.on_accent);
}

/// Centered text inside logical rect `r`.
fn label(cv: &mut Canvas, r: Rect, vrect: Rect, text: &str, px: f64, bold: bool, rgb: u32) {
    let scale = cv.scale;
    let (w, h) = crate::annotate::text_extent(text, px, bold);
    let pos = Point::new(
        (r.x as f64 + (r.w as f64 - w) * 0.5).round() as i32,
        (r.y as f64 + (r.h as f64 - h) * 0.5).round() as i32,
    );
    crate::annotate::Annotation::Text {
        pos,
        text: text.to_string(),
        px,
        bold,
        rgb,
    }
    .render_phys(cv, vrect, scale);
}

/// First-run style hint shown before a selection exists, centered near the
/// top of the monitor `vrect`.
pub fn paint_hint(buf: &mut [u32], sw: usize, sh: usize, vrect: Rect, scale: f64, text: &str) {
    let px = 14.0;
    let (w, h) = crate::annotate::text_extent(text, px, false);
    let (bw, bh) = (w + 32.0, h + 16.0);
    let r = Rect::new(
        (vrect.x as f64 + (vrect.w as f64 - bw) * 0.5).round() as i32,
        vrect.y + 40,
        bw.round() as u32,
        bh.round() as u32,
    );
    let mut cv = Canvas {
        buf,
        w: sw,
        h: sh,
        scale,
    };
    paint_dock(
        &mut cv,
        (r.x - vrect.x) as f64,
        (r.y - vrect.y) as f64,
        r.w as f64,
        r.h as f64,
    );
    label(&mut cv, r, vrect, text, px, false, qs_theme::pal().fg);
}

/// Paint the unified Apple Liquid Glass floating dock behind all toolbar items.
fn paint_dock(cv: &mut Canvas, x: f64, y: f64, w: f64, h: f64) {
    let s = cv.scale;
    let p = qs_theme::pal();
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let hw = w * 0.5;
    let hh = h * 0.5;
    let sh_pad = 12.0;
    let x0 = ((x - sh_pad) * s).floor() as i32;
    let y0 = ((y - sh_pad) * s).floor() as i32;
    let x1 = ((x + w + sh_pad) * s).ceil() as i32;
    let y1 = ((y + h + sh_pad) * s).ceil() as i32;
    let sh_alpha = if p.dark { 0.42 } else { 0.16 };

    for py in y0..=y1 {
        for px in x0..=x1 {
            let lx = (px as f64 + 0.5) / s;
            let ly = (py as f64 + 0.5) / s;
            // Smooth ambient + key drop shadow
            let sd = rrect_sdf(lx, ly, cx, cy + 3.0, hw, hh, DOCK_RAD);
            if sd > 0.0 && sd < sh_pad {
                let t = 1.0 - sd / sh_pad;
                cv.blend(px, py, p.shadow, sh_alpha * t * t);
            }
            // Dock surface + 1px hairline border
            let d = rrect_sdf(lx, ly, cx, cy, hw, hh, DOCK_RAD);
            let cov = fill_cov(d, s);
            if cov > 0.0 {
                cv.blend(px, py, p.popover, cov * 0.96);
            }
            let bc = stroke_cov(d, 1.0, s);
            if bc > 0.0 {
                cv.blend(px, py, p.popover_border, bc);
            }
            // Subtle top specular highlight
            if ly >= y + 0.5 && ly <= y + 1.5 && lx >= x + DOCK_RAD && lx <= x + w - DOCK_RAD {
                let spec_a = if p.dark { 0.10 } else { 0.72 };
                cv.blend(px, py, p.on_accent, spec_a);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_button(
    cv: &mut Canvas,
    cx: f64,
    cy: f64,
    rad: f64,
    hovered: bool,
    active: bool,
    enabled: bool,
    item: Item,
    accent: u32,
) {
    let scale = cv.scale;
    let p = qs_theme::pal();
    let pad = (STROKE * scale).ceil() as i32 + 2;
    let r = rad.ceil() as i32 + pad;
    let half = rad / scale;
    let has_fill = active || hovered;
    let bg = if active {
        accent
    } else if hovered {
        p.control
    } else {
        p.popover
    };
    let fg_col = if active { p.on_accent } else { p.fg };
    let fg_a = if enabled { 1.0 } else { 0.32 };
    for y in (cy as i32 - r)..=(cy as i32 + r) {
        for x in (cx as i32 - r)..=(cx as i32 + r) {
            let px = x as f64 + 0.5 - cx;
            let py = y as f64 + 0.5 - cy;
            let (lx, ly) = (px / scale, py / scale);
            if has_fill {
                let d = rrect_sdf(lx, ly, 0.0, 0.0, half, half, BTN_RAD);
                let a = fill_cov(d, scale);
                if a > 0.0 {
                    cv.blend(x, y, bg, a);
                }
            }
            draw_icon(cv, x, y, lx, ly, scale, bg, fg_col, fg_a, item);
        }
    }
}

/// Exact box-filter area average of an `n×n` coverage mask over the pixel
/// footprint `[u - half, u + half] × [v - half, v + half]`.
fn sample_mask_box(m: &[f32], n: usize, u: f64, v: f64, half: f64) -> f64 {
    let nf = n as f64;
    let u0 = u - half;
    let u1 = u + half;
    let v0 = v - half;
    let v1 = v + half;
    let area = (u1 - u0) * (v1 - v0);
    if area <= 0.0 || u1 <= 0.0 || v1 <= 0.0 || u0 >= nf || v0 >= nf {
        return 0.0;
    }
    let ix0 = (u0.floor() as i32).max(0);
    let ix1 = (u1.ceil() as i32).min(n as i32);
    let iy0 = (v0.floor() as i32).max(0);
    let iy1 = (v1.ceil() as i32).min(n as i32);
    let mut sum = 0.0;
    for iy in iy0..iy1 {
        let wy = (v1.min((iy + 1) as f64) - v0.max(iy as f64)).max(0.0);
        if wy <= 0.0 {
            continue;
        }
        let row = iy as usize * n;
        for ix in ix0..ix1 {
            let wx = (u1.min((ix + 1) as f64) - u0.max(ix as f64)).max(0.0);
            sum += m[row + ix as usize] as f64 * (wx * wy);
        }
    }
    (sum / area).clamp(0.0, 1.0)
}

/// Icon stroke/fill for one toolbar item at logical point (lx, ly).
#[allow(clippy::too_many_arguments)]
fn draw_icon(
    cv: &mut Canvas,
    x: i32,
    y: i32,
    lx: f64,
    ly: f64,
    scale: f64,
    bg: u32,
    fg_col: u32,
    fg_a: f64,
    item: Item,
) {
    let stroke = |d: f64, w: f64| stroke_cov(d, w, scale);
    match item {
        Item::Tool(Tool::Select) => {
            // pointer arrow: outline of the classic up-left cursor
            let pts = [
                (-4.2, -5.6),
                (-4.2, 4.0),
                (-1.4, 1.4),
                (0.8, 5.0),
                (2.9, 4.2),
                (0.7, -0.2),
                (4.6, -0.2),
                (-4.2, -5.6),
            ];
            let mut d = f64::INFINITY;
            for s in pts.windows(2) {
                d = d.min(seg_sdf(lx, ly, s[0].0, s[0].1, s[1].0, s[1].1));
            }
            let a = stroke(d, 1.6) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Tool(Tool::Pen) => {
            // pencil: body + pointed nib
            let d =
                seg_sdf(lx, ly, -4.6, 4.6, 1.4, -1.4).min(seg_sdf(lx, ly, 1.4, -1.4, 4.6, -4.4));
            let a = stroke(d, 2.4) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Tool(Tool::Arrow) => {
            // ↗ shaft + two wings
            let d = seg_sdf(lx, ly, -5.2, 5.2, 3.6, -3.6)
                .min(seg_sdf(lx, ly, 3.6, -3.6, -0.6, -3.6))
                .min(seg_sdf(lx, ly, 3.6, -3.6, 3.6, 0.6));
            let a = stroke(d, STROKE) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Tool(Tool::Rect) => {
            let a = stroke(rrect_sdf(lx, ly, 0.0, 0.0, 4.8, 3.8, 0.9), STROKE) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Tool(Tool::Ellipse) => {
            let a = stroke(ellipse_sdf(lx, ly, 0.0, 0.0, 5.0, 3.8), STROKE) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Tool(Tool::Marker) => {
            // highlighter: wide body + thin nib
            let d = seg_sdf(lx, ly, -4.8, 4.4, 1.6, -2.0);
            let a = stroke(d, 3.4) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
            let d2 = seg_sdf(lx, ly, 1.6, -2.0, 4.8, -5.2);
            let a = stroke(d2, STROKE) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Tool(Tool::Step) => {
            // circle with a "1" inside
            let d = lx.hypot(ly) - 7.0;
            let ring = stroke_cov(d, STROKE, scale);
            let m = crate::annotate::label_mask("1", true);
            let n = crate::annotate::ICON_MASK_N;
            let u = (lx * 1.7 + 8.0) / 16.0 * n as f64;
            let v = (ly * 1.7 + 8.0) / 16.0 * n as f64;
            let half = (0.5 * 1.7 / scale) / 16.0 * n as f64;
            let g = sample_mask_box(m, n, u, v, half);
            let a = ring.max(g) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Tool(Tool::Text) | Item::Bold | Item::FontSize(_) => {
            // real font glyphs — area-averaged from the 56×56 coverage mask
            // so thin strokes never drop out on 1x (1080p) displays.
            let (lbl, b) = match item {
                Item::Bold => ("B", true),
                Item::FontSize(i) => (crate::annotate::TEXT_SIZES[i as usize].1, false),
                _ => ("T", false),
            };
            let m = crate::annotate::label_mask(lbl, b);
            let n = crate::annotate::ICON_MASK_N;
            let u = (lx + 8.0) / 16.0 * n as f64;
            let v = (ly + 8.0) / 16.0 * n as f64;
            let half = (0.5 / scale) / 16.0 * n as f64;
            let cov = sample_mask_box(m, n, u, v, half);
            // Gentle gamma lift keeps 1px strokes crisp against both light and
            // accent-blue pills without blooming.
            let a = if cov > 0.0 { cov.powf(0.85) * fg_a } else { 0.0 };
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Tool(Tool::Pixelate) => {
            // 2x2 checkerboard of filled squares
            let d = rrect_sdf(lx, ly, -2.9, -2.9, 1.7, 1.7, 0.4)
                .min(rrect_sdf(lx, ly, 2.9, -2.9, 1.7, 1.7, 0.4))
                .min(rrect_sdf(lx, ly, -2.9, 2.9, 1.7, 1.7, 0.4))
                .min(rrect_sdf(lx, ly, 2.9, 2.9, 1.7, 1.7, 0.4));
            let a = fill_cov(d, scale) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Undo | Item::Redo => {
            // ↩/↪ hook: chevron tip + shaft + U-turn arc returning left,
            // mirrored for redo
            let m = if item == Item::Undo { 1.0 } else { -1.0 };
            let tip = (-4.6 * m, -0.6);
            let mut d = seg_sdf(lx, ly, tip.0, tip.1, tip.0 + 3.0 * m, tip.1 - 3.0)
                .min(seg_sdf(lx, ly, tip.0, tip.1, tip.0 + 3.0 * m, tip.1 + 3.0))
                .min(seg_sdf(lx, ly, tip.0, tip.1, 1.8 * m, tip.1));
            let (ccx, ccy, rr) = (1.8, 2.6, 3.2);
            let mut prev = (m * ccx, ccy - rr);
            for i in 1..=12 {
                let a = (-90.0 + 180.0 * i as f64 / 12.0).to_radians();
                let p = (m * (ccx + rr * a.cos()), ccy + rr * a.sin());
                d = d.min(seg_sdf(lx, ly, prev.0, prev.1, p.0, p.1));
                prev = p;
            }
            d = d.min(seg_sdf(lx, ly, prev.0, prev.1, prev.0 - 2.0 * m, prev.1));
            let a = stroke(d, STROKE) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::More => {
            // ⋯ three dots
            let d = (lx + 4.5)
                .hypot(ly)
                .min(lx.hypot(ly))
                .min((lx - 4.5).hypot(ly))
                - 1.5;
            let a = fill_cov(d, scale) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Size(s) => {
            // filled disc, radius grows with the level
            let r = [1.8, 3.0, 4.6][s as usize];
            let d = lx.hypot(ly) - r;
            let a = fill_cov(d, scale) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Color(c) => {
            // filled swatch disc in the palette color
            let d = lx.hypot(ly) - 6.0;
            let a = fill_cov(d, scale) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, PALETTE[c], a);
            }
            // hairline ring so white/black swatches stay visible on either bar
            let ring = stroke(d, 1.0) * fg_a * 0.35;
            if ring > 0.0 {
                cv.blend(x, y, fg_col, ring);
            }
        }
        Item::Act(Action::Copy) => {
            let d_back = rrect_sdf(lx, ly, -3.5, -3.5, 4.5, 4.5, 1.4);
            let d_front = rrect_sdf(lx, ly, 3.5, 3.5, 4.5, 4.5, 1.4);
            let fill = fill_cov(d_front, scale);
            if fill > 0.0 {
                cv.blend(x, y, bg, fill); // occlude back square
            }
            let a = stroke(d_back, STROKE).max(stroke(d_front, STROKE)) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Act(Action::Ocr) => {
            // paragraph: left-aligned lines, last one shorter
            let d = seg_sdf(lx, ly, -5.75, -4.5, 4.5, -4.5)
                .min(seg_sdf(lx, ly, -5.75, 0.0, 5.75, 0.0))
                .min(seg_sdf(lx, ly, -5.75, 4.5, 1.5, 4.5));
            let a = stroke(d, STROKE) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Act(Action::Share) => {
            // share: open-top box + arrow rising out of it
            let d = seg_sdf(lx, ly, -5.0, -1.0, -5.0, 5.5)
                .min(seg_sdf(lx, ly, -5.0, 5.5, 5.0, 5.5))
                .min(seg_sdf(lx, ly, 5.0, 5.5, 5.0, -1.0))
                .min(seg_sdf(lx, ly, 0.0, -6.5, 0.0, 0.5))
                .min(seg_sdf(lx, ly, 0.0, -6.5, -3.0, -3.5))
                .min(seg_sdf(lx, ly, 0.0, -6.5, 3.0, -3.5));
            let a = stroke(d, STROKE) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Act(Action::Scroll) => {
            // tall page outline + down chevron inside
            let d_page = rrect_sdf(lx, ly, 0.0, 0.0, 4.0, 5.5, 1.2);
            let d_arrow = seg_sdf(lx, ly, 0.0, -2.5, 0.0, 2.5)
                .min(seg_sdf(lx, ly, -2.2, 0.3, 0.0, 2.5))
                .min(seg_sdf(lx, ly, 2.2, 0.3, 0.0, 2.5));
            let a = stroke(d_page, STROKE).max(stroke(d_arrow, STROKE)) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Act(Action::Record) => {
            // video camera: body rect + lens wedge on the right
            let d_body = rrect_sdf(lx, ly, -1.2, 0.0, 4.8, 3.6, 1.4);
            let d_lens = seg_sdf(lx, ly, 3.8, -2.4, 6.4, -3.6)
                .min(seg_sdf(lx, ly, 6.4, -3.6, 6.4, 3.6))
                .min(seg_sdf(lx, ly, 6.4, 3.6, 3.8, 2.4));
            let a = stroke(d_body.min(d_lens), STROKE) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Act(Action::Edit) => {
            // "open in editor": page frame with a pencil leaving its
            // top-right corner — the frame is cut where the pencil crosses
            let d_frame = rrect_sdf(lx, ly, -1.0, 1.0, 4.6, 4.6, 1.4);
            let d_pen = seg_sdf(lx, ly, -2.4, 2.4, 4.4, -4.4);
            let halo = fill_cov(d_pen - 2.6, scale);
            let a = (stroke(d_frame, STROKE) * (1.0 - halo)).max(stroke(d_pen, 2.2)) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
        Item::Act(Action::Save) => {
            // tray: U shape open at top; arrow dips into it
            let d = seg_sdf(lx, ly, -5.75, -0.5, -5.75, 5.5)
                .min(seg_sdf(lx, ly, -5.75, 5.5, 5.75, 5.5))
                .min(seg_sdf(lx, ly, 5.75, 5.5, 5.75, -0.5))
                .min(seg_sdf(lx, ly, 0.0, -6.5, 0.0, 1.5))
                .min(seg_sdf(lx, ly, -3.2, -1.5, 0.0, 1.5))
                .min(seg_sdf(lx, ly, 3.2, -1.5, 0.0, 1.5));
            let a = stroke(d, STROKE) * fg_a;
            if a > 0.0 {
                cv.blend(x, y, fg_col, a);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel() -> (Rect, Rect) {
        (Rect::new(200, 200, 400, 300), Rect::new(0, 0, 1920, 1080))
    }

    #[test]
    fn main_row_ends_with_more_edit_done() {
        let (s, m) = sel();
        let tb = Toolbar::new(s, m, false);
        let items: Vec<Item> = tb.buttons.iter().map(|b| b.0).collect();
        assert_eq!(items, MAIN);
        assert_eq!(items[TOOLS_END], Item::Undo);
        assert_eq!(items[HISTORY_END], Item::More);
        assert_eq!(items.last(), Some(&Item::Act(Action::Copy)));
    }

    #[test]
    fn more_swaps_the_second_row() {
        let (s, m) = sel();
        let mut tb = Toolbar::new(s, m, false);
        tb.tool = Some(Tool::Arrow);
        tb.layout(s, m, true);
        assert!(tb.buttons.iter().any(|b| matches!(b.0, Item::Color(_))));
        tb.more = true;
        tb.layout(s, m, true);
        let second: Vec<Item> = tb.buttons[MAIN.len()..].iter().map(|b| b.0).collect();
        assert_eq!(second, MORE);
        // every action and tool stays reachable from one of the two rows
        for a in [
            Action::Save,
            Action::Edit,
            Action::Copy,
            Action::Scroll,
            Action::Record,
            Action::Ocr,
            Action::Share,
        ] {
            if a == Action::Save {
                continue; // keyboard only — Done already saves
            }
            assert!(
                MAIN.contains(&Item::Act(a)) || MORE.contains(&Item::Act(a)),
                "{a:?}"
            );
        }
    }

    #[test]
    fn main_row_leads_with_bug_report_tools() {
        assert_eq!(MAIN[0], Item::Tool(Tool::Rect));
        assert!(MAIN[..TOOLS_END].contains(&Item::Tool(Tool::Step)));
        assert!(MORE.contains(&Item::Tool(Tool::Marker)));
    }

    #[test]
    fn done_is_a_wide_pill_inside_the_dock() {
        let (s, m) = sel();
        let tb = Toolbar::new(s, m, false);
        let (item, r) = *tb.buttons.last().unwrap();
        assert_eq!(item, Item::Act(Action::Copy));
        assert_eq!(r.w, DONE_W as u32);
        assert!(r.right() <= tb.rect.right());
        assert!(tb.hit(Point::new(r.right() - 2, r.y + 4)).is_some());
    }

    #[test]
    fn tool_letters_match_the_editor() {
        assert_eq!(Tool::from_key('a'), Some(Tool::Arrow));
        assert_eq!(Tool::from_key('h'), Some(Tool::Marker));
        assert_eq!(Tool::from_key('b'), Some(Tool::Pixelate));
        assert_eq!(Tool::from_key('n'), Some(Tool::Step));
        assert_eq!(Tool::from_key('s'), None);
        assert_eq!(Tool::from_key('c'), None);
    }
}
