//! Font access and multi-line text layout.
//!
//! One bundled font family (Be Vietnam Pro Regular + Bold, OFL — fonts/OFL.txt) for every text
//! object so preview, export and the overlay agree. Layout is greedy word
//! wrap over explicit `\n` paragraphs; Be Vietnam Pro is designed for Vietnamese,
//! so no shaping engine is needed for the Latin/Vietnamese scripts we target.

use ab_glyph::{Font, FontVec, GlyphId, ScaleFont};
use qs_doc::{Align, FontFamily, TextStyle};
use std::sync::OnceLock;

pub static REGULAR_BYTES: &[u8] = include_bytes!("../fonts/BeVietnamPro-Regular.ttf");
pub static BOLD_BYTES: &[u8] = include_bytes!("../fonts/BeVietnamPro-Bold.ttf");
pub static SERIF_BYTES: &[u8] = include_bytes!("../fonts/DejaVuSerif.ttf");
pub static SERIF_BOLD_BYTES: &[u8] = include_bytes!("../fonts/DejaVuSerif-Bold.ttf");
pub static MONO_BYTES: &[u8] = include_bytes!("../fonts/DejaVuSansMono.ttf");
pub static MONO_BOLD_BYTES: &[u8] = include_bytes!("../fonts/DejaVuSansMono-Bold.ttf");

/// Horizontal slant of the synthetic italic, as dx per dy.
pub const ITALIC_SHEAR: f32 = 0.21;

/// Line advance as a multiple of the font's ascent-descent height.
pub const LINE_SPACING: f32 = 1.2;

/// Run `f` with the bundled face for `family` / `bold`. The size is applied by
/// callers through `as_scaled(px)`.
pub fn with_font<T>(
    _px: f32,
    bold: bool,
    family: FontFamily,
    f: impl FnOnce(&FontVec) -> T,
) -> T {
    type Slot = OnceLock<FontVec>;
    static SANS_REG: Slot = OnceLock::new();
    static SANS_BOLD: Slot = OnceLock::new();
    static SERIF_REG: Slot = OnceLock::new();
    static SERIF_BOLD: Slot = OnceLock::new();
    static MONO_REG: Slot = OnceLock::new();
    static MONO_BOLD: Slot = OnceLock::new();

    /// A bundled font that fails to parse is a build-time bug — panic.
    fn load(slot: &'static OnceLock<FontVec>, bytes: &'static [u8], name: &str) -> &'static FontVec {
        slot.get_or_init(|| {
            FontVec::try_from_vec(bytes.to_vec())
                .unwrap_or_else(|e| panic!("bundled {name} must parse: {e}"))
        })
    }

    let font = match (family, bold) {
        (FontFamily::Sans, false) => load(&SANS_REG, REGULAR_BYTES, "BeVietnamPro-Regular.ttf"),
        (FontFamily::Sans, true) => load(&SANS_BOLD, BOLD_BYTES, "BeVietnamPro-Bold.ttf"),
        (FontFamily::Serif, false) => load(&SERIF_REG, SERIF_BYTES, "DejaVuSerif.ttf"),
        (FontFamily::Serif, true) => load(&SERIF_BOLD, SERIF_BOLD_BYTES, "DejaVuSerif-Bold.ttf"),
        (FontFamily::Mono, false) => load(&MONO_REG, MONO_BYTES, "DejaVuSansMono.ttf"),
        (FontFamily::Mono, true) => load(&MONO_BOLD, MONO_BOLD_BYTES, "DejaVuSansMono-Bold.ttf"),
    };
    f(font)
}

/// One laid-out line. Byte offsets index the source string.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// byte range of the visible line content (no trailing '\n')
    pub start: usize,
    pub end: usize,
    /// x of the line start relative to the block (alignment offset)
    pub x: f32,
    pub width: f32,
    /// caret stops: (byte index, x relative to line start) — first is
    /// `start`, last is `end`
    pub stops: Vec<(usize, f32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub lines: Vec<Line>,
    pub px: f32,
    pub bold: bool,
    pub ascent: f32,
    pub descent: f32,
    pub line_h: f32,
    /// block size (logical px)
    pub width: f32,
    pub height: f32,
}

impl TextLayout {
    /// Line index holding byte `idx` (a caret position).
    pub fn line_of(&self, idx: usize) -> usize {
        self.lines
            .iter()
            .position(|l| idx <= l.end)
            .unwrap_or(self.lines.len().saturating_sub(1))
    }

    /// Caret (x, top y) relative to the block's top-left.
    pub fn caret(&self, idx: usize) -> (f32, f32) {
        let li = self.line_of(idx);
        let Some(l) = self.lines.get(li) else {
            return (0.0, 0.0);
        };
        let x = l
            .stops
            .iter()
            .find(|(b, _)| *b >= idx)
            .or(l.stops.last())
            .map(|(_, x)| *x)
            .unwrap_or(0.0);
        (l.x + x, li as f32 * self.line_h)
    }

    /// Nearest caret byte index for a point relative to the block.
    pub fn index_at(&self, x: f32, y: f32) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        let li = ((y / self.line_h).floor().max(0.0) as usize).min(self.lines.len() - 1);
        self.index_on_line(li, x)
    }

    pub fn index_on_line(&self, li: usize, x: f32) -> usize {
        let l = &self.lines[li];
        let rx = x - l.x;
        l.stops
            .iter()
            .min_by(|a, b| (a.1 - rx).abs().total_cmp(&(b.1 - rx).abs()))
            .map(|s| s.0)
            .unwrap_or(l.start)
    }
}

struct Measurer<'f> {
    s: ab_glyph::PxScaleFont<&'f FontVec>,
}

impl Measurer<'_> {
    fn adv(&self, prev: Option<GlyphId>, c: char) -> (GlyphId, f32) {
        let id = self.s.glyph_id(c);
        let k = prev.map(|p| self.s.kern(p, id)).unwrap_or(0.0);
        (id, k + self.s.h_advance(id))
    }
}

/// Lay out `text` with `style`; wrap at `wrap_w` logical px when given.
pub fn layout(text: &str, style: &TextStyle, wrap_w: Option<f32>) -> TextLayout {
    with_font(style.px, style.bold, style.family, |f| {
        let m = Measurer {
            s: f.as_scaled(style.px),
        };
        let ascent = m.s.ascent();
        let descent = m.s.descent();
        let line_h = (ascent - descent) * LINE_SPACING;
        let wrap = wrap_w.filter(|w| *w > 1.0);
        let mut lines = Vec::new();
        let mut para_start = 0usize;
        for para in text.split('\n') {
            wrap_para(&m, para_start, para, wrap, &mut lines);
            para_start += para.len() + 1;
        }
        let maxw = lines.iter().map(|l: &Line| l.width).fold(0.0f32, f32::max);
        let width = wrap.unwrap_or(maxw);
        for l in &mut lines {
            l.x = match style.align {
                Align::Left => 0.0,
                Align::Center => (width - l.width) / 2.0,
                Align::Right => width - l.width,
            };
        }
        let n = lines.len().max(1) as f32;
        TextLayout {
            height: (n - 1.0) * line_h + (ascent - descent),
            lines,
            px: style.px,
            bold: style.bold,
            ascent,
            descent,
            line_h,
            width,
        }
    })
}

fn wrap_para(m: &Measurer, off: usize, para: &str, wrap: Option<f32>, out: &mut Vec<Line>) {
    let chars: Vec<(usize, char)> = para.char_indices().map(|(i, c)| (off + i, c)).collect();
    let n = chars.len();
    let mut start = 0usize;
    loop {
        let sb = chars.get(start).map(|c| c.0).unwrap_or(off + para.len());
        let mut stops = vec![(sb, 0.0f32)];
        let mut x = 0.0f32;
        let mut prev = None;
        let mut last_ws: Option<usize> = None;
        let mut cut = None;
        let mut i = start;
        while i < n {
            let (b, c) = chars[i];
            let (id, adv) = m.adv(prev, c);
            if let Some(w) = wrap {
                if x + adv > w && i > start && !c.is_whitespace() {
                    cut = Some(last_ws.map(|k| k + 1).unwrap_or(i));
                    break;
                }
            }
            x += adv;
            stops.push((b + c.len_utf8(), x));
            if c.is_whitespace() {
                last_ws = Some(i);
            }
            prev = Some(id);
            i += 1;
        }
        let end_i = cut.unwrap_or(n);
        stops.truncate(end_i - start + 1);
        let end = stops.last().map(|s| s.0).unwrap_or(sb);
        // visible width: up to the last non-whitespace char
        let mut vis = 0.0;
        for (k, (_, c)) in chars[start..end_i].iter().enumerate() {
            if !c.is_whitespace() {
                vis = stops[k + 1].1;
            }
        }
        out.push(Line {
            start: sb,
            end,
            x: 0.0,
            width: vis,
            stops,
        });
        match cut {
            Some(c) if c > start => start = c,
            _ => break,
        }
    }
}

/// Plain advance width of a single line (no wrapping).
pub fn measure(text: &str, px: f32, bold: bool) -> (f32, f32) {
    let l = layout(
        text,
        &TextStyle {
            px,
            bold,
            ..Default::default()
        },
        None,
    );
    (l.width, l.height)
}
#[cfg(test)]
mod tests {
    use super::*;

    fn st(px: f32) -> TextStyle {
        TextStyle {
            px,
            ..Default::default()
        }
    }

    #[test]
    fn newlines_make_lines() {
        let l = layout("ab\ncd\n", &st(20.0), None);
        assert_eq!(l.lines.len(), 3);
        assert_eq!((l.lines[0].start, l.lines[0].end), (0, 2));
        assert_eq!((l.lines[1].start, l.lines[1].end), (3, 5));
        assert_eq!((l.lines[2].start, l.lines[2].end), (6, 6));
        assert!(l.height > 2.0 * l.line_h);
    }

    #[test]
    fn wraps_at_spaces_within_width() {
        let s = st(20.0);
        let one = layout("hello world again", &s, None);
        assert_eq!(one.lines.len(), 1);
        let w = one.width * 0.55;
        let l = layout("hello world again", &s, Some(w));
        assert!(l.lines.len() >= 2, "{:?}", l.lines);
        for ln in &l.lines {
            // a trailing space may overhang; visible glyphs must fit
            assert!(ln.width <= w + s.px, "line too wide {} > {w}", ln.width);
        }
        // every byte is covered exactly once, in order
        let mut pos = 0;
        for ln in &l.lines {
            assert_eq!(ln.start, pos);
            pos = ln.end;
        }
        assert_eq!(pos, "hello world again".len());
    }

    #[test]
    fn long_word_breaks_by_char() {
        let l = layout("abcdefghijklmnopqrstuvwxyz", &st(20.0), Some(60.0));
        assert!(l.lines.len() > 2);
        for ln in &l.lines {
            assert!(ln.width <= 60.0 + 20.0);
        }
    }

    #[test]
    fn caret_round_trip_vietnamese() {
        let t = "Tiếng Việt\nắ ặ ề ỗ";
        let l = layout(t, &st(24.0), None);
        for (i, _) in t.char_indices().chain([(t.len(), ' ')]) {
            let (x, y) = l.caret(i);
            let back = l.index_at(x, y + 1.0);
            assert_eq!(back, i, "caret {i} -> ({x},{y}) -> {back}");
            assert!(t.is_char_boundary(back));
        }
    }

    #[test]
    fn align_offsets() {
        let mut s = st(20.0);
        s.align = Align::Right;
        let l = layout("a\nwide line", &s, None);
        let r0 = l.lines[0].x + l.lines[0].width;
        let r1 = l.lines[1].x + l.lines[1].width;
        assert!((r0 - r1).abs() < 0.01);
        s.align = Align::Center;
        let l = layout("a\nwide line", &s, None);
        assert!(l.lines[0].x > 0.0);
    }

    #[test]
    fn bold_is_wider() {
        assert!(measure("Quick", 28.0, true).0 > measure("Quick", 28.0, false).0);
    }

    #[test]
    fn italics_keep_the_same_advances() {
        let upright = layout("Quick brown", &st(22.0), None);
        let mut s = st(22.0);
        s.italic = true;
        let slanted = layout("Quick brown", &s, None);
        assert_eq!(slanted.width, upright.width, "shear happens at draw time");
        assert_eq!(slanted.height, upright.height);
    }

    #[test]
    fn families_use_different_metrics_and_mono_is_uniform() {
        let mut sans = st(20.0);
        sans.family = FontFamily::Sans;
        let mut serif = st(20.0);
        serif.family = FontFamily::Serif;
        let mut mono = st(20.0);
        mono.family = FontFamily::Mono;

        let w_sans = layout("Hamburgefonstiv", &sans, None).width;
        let w_serif = layout("Hamburgefonstiv", &serif, None).width;
        let w_mono = layout("Hamburgefonstiv", &mono, None).width;
        assert!((w_sans - w_serif).abs() > 0.5, "sans and serif differ");
        assert!((w_mono - w_sans).abs() > 0.5, "mono differs from sans");

        let narrow = layout("iiii", &mono, None).width;
        let wide = layout("MMMM", &mono, None).width;
        assert!(
            (narrow - wide).abs() < 0.5,
            "mono advances are uniform: {narrow} vs {wide}"
        );
    }

    #[test]
    fn vietnamese_diacritics_layout_in_every_family() {
        for family in FontFamily::ALL {
            let mut s = st(24.0);
            s.family = family;
            let l = layout("Tiếng Việt: ắ ặ ề ỗ ỹ", &s, None);
            assert_eq!(l.lines.len(), 1);
            assert!(l.width > 40.0, "{} metrics look empty", family.label());
        }
    }
}
