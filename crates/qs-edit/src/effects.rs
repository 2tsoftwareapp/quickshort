//! Effects and style application: image-effects draft/commit and every
//! palette setter that pushes style onto the current selection.
use qs_doc::{
    Align, ArrowHead, BlurMode, CalloutShape, Fill, FontFamily, MagnifyShape, ObjectKind,
    StampKind, Stroke, TextOutline, TextStyle,
};

use crate::{contrast, Editor, Style, MARKER_K};

fn stroke_mut(k: &mut ObjectKind) -> Option<&mut Stroke> {
    use ObjectKind::*;
    match k {
        Pen { stroke, .. }
        | Highlighter { stroke, .. }
        | Arrow { stroke, .. }
        | Line { stroke, .. }
        | Rect { stroke, .. }
        | Ellipse { stroke, .. }
        | Callout { stroke, .. }
        | Polygon { stroke, .. } => Some(stroke),
        _ => None,
    }
}

fn fill_mut(k: &mut ObjectKind) -> Option<&mut Fill> {
    use ObjectKind::*;
    match k {
        Rect { fill, .. }
        | Ellipse { fill, .. }
        | Text { fill, .. }
        | Callout { fill, .. }
        | Polygon { fill, .. } => Some(fill),
        _ => None,
    }
}

fn text_style_mut(k: &mut ObjectKind) -> Option<&mut TextStyle> {
    match k {
        ObjectKind::Text { style, .. } | ObjectKind::Callout { style, .. } => Some(style),
        _ => None,
    }
}

/// Colour a selection swatch should show for `k`.
fn selection_rgb(k: &ObjectKind) -> Option<u32> {
    use ObjectKind::*;
    match k {
        Pen { stroke, .. }
        | Highlighter { stroke, .. }
        | Line { stroke, .. }
        | Arrow { stroke, .. }
        | Rect { stroke, .. }
        | Ellipse { stroke, .. }
        | Callout { stroke, .. }
        | Magnify { stroke, .. }
        | Polygon { stroke, .. } => Some(stroke.rgb),
        Text { style, .. } => Some(style.rgb),
        Step { rgb, .. } | Stamp { rgb, .. } => Some(*rgb),
        _ => None,
    }
}

/// Overwrite every style knob `k` exposes with the values in `st`, leaving
/// geometry alone (Quick Styles / "paste style").
fn apply_style_to_kind(k: &mut ObjectKind, st: &Style) {
    use ObjectKind::*;
    let is_marker = matches!(k, Highlighter { .. });
    if let Some(s) = stroke_mut(k) {
        s.rgb = st.rgb;
        s.width = if is_marker {
            st.width * MARKER_K
        } else {
            st.width
        };
    }
    if let Some(f) = fill_mut(k) {
        *f = st.fill;
    }
    if let Some(t) = text_style_mut(k) {
        *t = st.text;
        t.rgb = st.rgb;
    }
    match k {
        Arrow { head, .. } => *head = st.arrow_head,
        Callout { shape, .. } => *shape = st.callout_shape,
        Step { rgb, text_rgb, .. } => {
            *rgb = st.rgb;
            *text_rgb = contrast(st.rgb);
        }
        Blur { mode, strength, .. } => {
            *mode = st.blur_mode;
            *strength = st.blur_strength;
        }
        Magnify { zoom, shape, .. } => {
            *zoom = st.magnify_zoom;
            *shape = st.magnify_shape;
        }
        Spotlight {
            dim_opacity, blur, ..
        } => {
            *dim_opacity = st.spot_dim;
            *blur = st.spot_blur;
        }
        Stamp { kind, rgb, .. } => {
            *kind = st.stamp_kind;
            *rgb = st.rgb;
        }
        Eraser { width, .. } => *width = st.eraser_width,
        _ => {}
    }
}

impl Editor {
    /// Live preview across slider frames; finish at the end of the gesture.
    pub fn preview_effects(&mut self, effects: qs_doc::Effects) -> bool {
        if effects.validate().is_err() || effects == self.doc.effects {
            return false;
        }
        self.end_text_edit();
        if self.effects_pre.is_none() {
            self.effects_pre = Some(self.snap());
        }
        self.doc.effects = effects;
        self.rev = self.rev.wrapping_add(1);
        true
    }

    pub fn finish_effects(&mut self) {
        if let Some(pre) = self.effects_pre.take() {
            if pre != self.snap() {
                self.push_undo(pre);
            }
        }
    }

    /// Apply an entire effects draft as one history entry.
    pub fn set_effects(&mut self, effects: qs_doc::Effects) -> bool {
        if effects.validate().is_err() || effects == self.doc.effects {
            return false;
        }
        self.end_text_edit();
        self.record(|s| s.doc.effects = effects);
        true
    }

    // ---- style (palette) -------------------------------------------------------------

    /// Apply a whole [`Style`] to the tool defaults and to every selected
    /// unlocked object as one undo step (Quick Styles).
    pub fn apply_style(&mut self, style: Style) -> bool {
        self.finish_effects();
        self.end_text_edit();
        let defaults_changed = self.style != style;
        self.style = style.clone();
        let ids = self.selected_unlocked_ids();
        if ids.is_empty() {
            return defaults_changed;
        }
        let mut touched = false;
        self.record(|e| {
            for id in &ids {
                if let Some(o) = e.doc.get_mut(*id) {
                    apply_style_to_kind(&mut o.kind, &style);
                    touched = true;
                }
            }
        });
        for id in ids {
            self.fit_callout(id);
        }
        defaults_changed || touched
    }

    /// Style of the primary selection, or the tool defaults when nothing is
    /// selected — the starting point for "save as preset".
    pub fn current_style(&self) -> Style {
        let mut st = self.style.clone();
        if let Some(o) = self.selected_object() {
            st.rgb = selection_rgb(&o.kind).unwrap_or(st.rgb);
            match &o.kind {
                ObjectKind::Pen { stroke, .. }
                | ObjectKind::Line { stroke, .. }
                | ObjectKind::Arrow { stroke, .. }
                | ObjectKind::Rect { stroke, .. }
                | ObjectKind::Ellipse { stroke, .. }
                | ObjectKind::Callout { stroke, .. }
                | ObjectKind::Magnify { stroke, .. } => st.width = stroke.width,
                ObjectKind::Highlighter { stroke, .. } => st.width = stroke.width / MARKER_K,
                ObjectKind::Eraser { width, .. } => st.eraser_width = *width,
                _ => {}
            }
            match &o.kind {
                ObjectKind::Rect { fill, .. } | ObjectKind::Ellipse { fill, .. } => st.fill = *fill,
                ObjectKind::Text { fill, style: t, .. } => {
                    st.fill = *fill;
                    st.text = *t;
                }
                ObjectKind::Callout {
                    fill,
                    style: t,
                    shape,
                    ..
                } => {
                    st.fill = *fill;
                    st.text = *t;
                    st.callout_shape = *shape;
                }
                ObjectKind::Arrow { head, .. } => st.arrow_head = *head,
                ObjectKind::Blur { mode, strength, .. } => {
                    st.blur_mode = *mode;
                    st.blur_strength = *strength;
                }
                ObjectKind::Magnify { zoom, shape, .. } => {
                    st.magnify_zoom = *zoom;
                    st.magnify_shape = *shape;
                }
                ObjectKind::Spotlight {
                    dim_opacity, blur, ..
                } => {
                    st.spot_dim = *dim_opacity;
                    st.spot_blur = *blur;
                }
                ObjectKind::Stamp { kind, .. } => st.stamp_kind = *kind,
                _ => {}
            }
        }
        st
    }

    /// Opacity of the primary selection (1.0 when nothing is selected).
    pub fn selected_opacity(&self) -> f32 {
        self.selected_object().map(|o| o.opacity).unwrap_or(1.0)
    }

    fn apply_opacity(&mut self, opacity: f32) -> bool {
        let v = opacity.clamp(0.0, 1.0);
        let ids = self.selected_unlocked_ids();
        let changed = ids.iter().any(|id| {
            self.doc
                .get(*id)
                .is_some_and(|o| (o.opacity - v).abs() > f32::EPSILON)
        });
        if !changed {
            return false;
        }
        for id in ids {
            if let Some(o) = self.doc.get_mut(id) {
                o.opacity = v;
            }
        }
        true
    }

    /// Set the paint opacity (0..=1) of every selected unlocked object as one
    /// undo step.
    pub fn set_opacity(&mut self, opacity: f32) -> bool {
        let v = opacity.clamp(0.0, 1.0);
        let ids = self.selected_unlocked_ids();
        if ids.is_empty()
            || ids.iter().all(|id| {
                self.doc
                    .get(*id)
                    .is_some_and(|o| (o.opacity - v).abs() <= f32::EPSILON)
            })
        {
            return false;
        }
        self.record(|e| {
            for id in &ids {
                if let Some(o) = e.doc.get_mut(*id) {
                    o.opacity = v;
                }
            }
        });
        true
    }

    /// Live opacity drag frame; call [`Editor::finish_opacity`] on release so
    /// the whole gesture is a single undo step.
    pub fn preview_opacity(&mut self, opacity: f32) -> bool {
        self.begin_change();
        if self.apply_opacity(opacity) {
            self.rev = self.rev.wrapping_add(1);
            true
        } else {
            false
        }
    }

    pub fn finish_opacity(&mut self) {
        self.end_change();
    }

    pub fn set_color(&mut self, rgb: u32) {
        self.style.rgb = rgb;
        self.update_selected(|k| {
            if let Some(s) = stroke_mut(k) {
                s.rgb = rgb;
            }
            match k {
                ObjectKind::Text { style, .. } => style.rgb = rgb,
                ObjectKind::Stamp { rgb: r, .. } => *r = rgb,
                ObjectKind::Step {
                    rgb: r, text_rgb, ..
                } => {
                    *r = rgb;
                    *text_rgb = contrast(rgb);
                }
                _ => {}
            }
        });
    }

    pub fn set_stroke_width(&mut self, w: f32) {
        self.style.width = w;
        self.update_selected(|k| {
            let is_marker = matches!(k, ObjectKind::Highlighter { .. });
            if let Some(s) = stroke_mut(k) {
                s.width = if is_marker { w * MARKER_K } else { w };
            }
        });
    }

    pub fn set_fill(&mut self, fill: Fill) {
        self.style.fill = fill;
        self.update_selected(|k| {
            if let Some(f) = fill_mut(k) {
                *f = fill;
            }
        });
    }

    fn set_text_style(&mut self, f: impl Fn(&mut TextStyle)) {
        f(&mut self.style.text);
        self.update_selected(|k| {
            if let Some(s) = text_style_mut(k) {
                f(s);
            }
        });
    }

    pub fn set_text_px(&mut self, px: f32) {
        self.set_text_style(|s| s.px = px);
    }
    pub fn set_text_bold(&mut self, bold: bool) {
        self.set_text_style(|s| s.bold = bold);
    }
    pub fn set_text_align(&mut self, a: Align) {
        self.set_text_style(|s| s.align = a);
    }

    pub fn set_text_italic(&mut self, italic: bool) {
        self.set_text_style(|s| s.italic = italic);
    }

    pub fn set_text_underline(&mut self, underline: bool) {
        self.set_text_style(|s| s.underline = underline);
    }

    pub fn set_text_strike(&mut self, strike: bool) {
        self.set_text_style(|s| s.strike = strike);
    }

    pub fn set_text_family(&mut self, family: FontFamily) {
        self.set_text_style(|s| s.family = family);
    }

    /// Turn the outline on with `outline`, or off with `None`.
    pub fn set_text_outline(&mut self, outline: Option<TextOutline>) {
        self.set_text_style(|s| s.outline = outline);
    }

    /// Live text-style drag (sliders): one undo step per gesture — call
    /// [`Editor::finish_text_style`] on release.
    pub fn preview_text_style(&mut self, style: TextStyle) -> bool {
        self.begin_change();
        let mut changed = self.style.text != style;
        self.style.text = style;
        let ids = self.selected_unlocked_ids();
        for id in &ids {
            if let Some(o) = self.doc.get_mut(*id) {
                if let Some(t) = text_style_mut(&mut o.kind) {
                    if *t != style {
                        *t = style;
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.rev = self.rev.wrapping_add(1);
        }
        changed
    }

    pub fn finish_text_style(&mut self) {
        self.end_change();
    }

    pub fn set_magnify_zoom(&mut self, zoom: f32) {
        if !(1.5..=4.0).contains(&zoom) {
            return;
        }
        self.style.magnify_zoom = zoom;
        self.update_selected(|k| {
            if let ObjectKind::Magnify { zoom: z, .. } = k {
                *z = zoom;
            }
        });
    }

    pub fn set_spotlight(&mut self, dim: Option<f32>, bl: Option<f32>) {
        if let Some(d) = dim.filter(|d| (0.0..=1.0).contains(d)) {
            self.style.spot_dim = d;
        }
        if let Some(b) = bl.filter(|b| (0.0..=64.0).contains(b)) {
            self.style.spot_blur = b;
        }
        let (d, b) = (self.style.spot_dim, self.style.spot_blur);
        self.update_selected(|k| {
            if let ObjectKind::Spotlight {
                dim_opacity, blur, ..
            } = k
            {
                if dim.is_some() {
                    *dim_opacity = d;
                }
                if bl.is_some() {
                    *blur = b;
                }
            }
        });
    }

    pub fn set_eraser_width(&mut self, w: f32) {
        let w = w.clamp(1.0, 128.0);
        self.style.eraser_width = w;
        self.update_selected(|k| {
            if let ObjectKind::Eraser { width, .. } = k {
                *width = w;
            }
        });
    }

    pub fn set_stamp_kind(&mut self, kind: StampKind) {
        self.style.stamp_kind = kind;
        self.update_selected(|k| {
            if let ObjectKind::Stamp { kind: s, .. } = k {
                *s = kind;
            }
        });
    }

    pub fn set_magnify_shape(&mut self, shape: MagnifyShape) {
        self.style.magnify_shape = shape;
        self.update_selected(|k| {
            if let ObjectKind::Magnify { shape: s, .. } = k {
                *s = shape;
            }
        });
    }
    pub fn set_callout_shape(&mut self, shape: CalloutShape) {
        self.style.callout_shape = shape;
        self.update_selected(|k| {
            if let ObjectKind::Callout { shape: s, .. } = k {
                *s = shape;
            }
        });
    }

    pub fn set_arrow_head(&mut self, head: ArrowHead) {
        self.style.arrow_head = head;
        self.update_selected(|k| {
            if let ObjectKind::Arrow { head: h, .. } = k {
                *h = head;
            }
        });
    }

    pub fn set_blur_mode(&mut self, mode: BlurMode) {
        self.style.blur_mode = mode;
        self.update_selected(|k| {
            if let ObjectKind::Blur { mode: m, .. } = k {
                *m = mode;
            }
        });
    }

    pub fn set_blur_strength(&mut self, strength: f32) {
        self.style.blur_strength = strength;
        self.update_selected(|k| {
            if let ObjectKind::Blur { strength: s, .. } = k {
                *s = strength;
            }
        });
    }

    pub fn set_callout_fill_and_contrast(&mut self, fill: Fill) {
        self.style.fill = fill;
        self.update_selected(|k| {
            if let ObjectKind::Callout {
                fill: f,
                style,
                stroke,
                ..
            } = k
            {
                *f = fill;
                style.rgb = match fill {
                    Fill::Solid { rgb, .. } => contrast(rgb),
                    Fill::None => stroke.rgb,
                };
            } else if let Some(f) = fill_mut(k) {
                *f = fill;
            }
        });
    }
}
