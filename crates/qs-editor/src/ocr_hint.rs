//! Smart Move hints from OCR.
//!
//! With the Smart Move tool active, hovering a recognized text line offers that
//! line as the source rectangle, so the common case ("move this label") needs a
//! click instead of a precise drag. Recognition runs on a worker thread and is
//! keyed to the base image, so editing annotations never triggers it again.

use image::RgbaImage;
use qs_doc::{Document, PointF, RectF};
use std::sync::mpsc::{self, Receiver};

/// One recognized line, already in document coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Hint {
    pub rect: RectF,
    pub text: String,
    /// Word boxes in the same document space — empty when the engine
    /// cannot report word geometry.
    pub words: Vec<(RectF, String)>,
}

/// What a result was computed for. A change means the base image or the way it
/// maps to document pixels moved, so the lines have to be recognized again.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Key {
    w: u32,
    h: u32,
    scale: f32,
    view: RectF,
}

#[derive(Default)]
pub struct OcrHints {
    lines: Vec<Hint>,
    key: Option<Key>,
    pending: Option<Key>,
    rx: Option<Receiver<(Vec<Hint>, Option<String>)>>,
    /// Set when the last attempt failed, so the tool can say why it is not
    /// offering anything instead of silently staying empty.
    last_error: Option<String>,
}

impl OcrHints {
    /// Start recognizing when the base image or its mapping changed. Cheap to
    /// call every frame: it returns immediately unless work is needed.
    pub fn ensure(&mut self, doc: &Document, base: &RgbaImage, lang: &str) {
        if self.rx.is_some() {
            return;
        }
        let key = Key {
            w: base.width(),
            h: base.height(),
            scale: doc.scale,
            view: doc.view,
        };
        if self.key == Some(key) {
            return;
        }

        let img = base.clone();
        let lang = lang.to_string();
        let (scale, view) = (doc.scale, doc.view);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let sent = match qs_ocr::extract_lines(&img, &lang) {
                Ok(lines) => lines
                    .into_iter()
                    .filter(|l| l.w >= 1.0 && l.h >= 1.0 && !l.text.trim().is_empty())
                    .map(|l| Hint {
                        rect: px_to_doc(l.x, l.y, l.w, l.h, view, scale),
                        words: l
                            .words
                            .into_iter()
                            .map(|w| {
                                (
                                    px_to_doc(w.x, w.y, w.w, w.h, view, scale),
                                    w.text,
                                )
                            })
                            .collect(),
                        text: l.text,
                    })
                    .collect(),
                // The engine is a capability, not a guarantee: keep the error so
                // the UI can explain the missing hints, and carry on.
                Err(e) => {
                    let _ = tx.send((Vec::new(), Some(e.to_string())));
                    return;
                }
            };
            let _ = tx.send((sent, None));
        });
        self.rx = Some(rx);
        self.pending = Some(key);
    }

    /// Pick up a finished recognition. Returns true when the line list changed.
    pub fn poll(&mut self) -> bool {
        let Some(rx) = &self.rx else {
            return false;
        };
        match rx.try_recv() {
            Ok((lines, error)) => {
                self.lines = lines;
                self.last_error = error;
                self.key = self.pending.take();
                self.rx = None;
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.key = self.pending.take();
                self.rx = None;
                self.last_error = Some("OCR worker stopped unexpectedly".to_string());
                true
            }
        }
    }

    /// True while a recognition is in flight, so the caller can keep repainting.
    pub fn is_scanning(&self) -> bool {
        self.rx.is_some()
    }

    /// The recognized lines, in engine order (reading order).
    pub fn lines(&self) -> &[Hint] {
        &self.lines
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Forget the lines; the next `ensure` recognizes again.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.key = None;
        self.last_error = None;
    }

    /// Indices of the lines whose text contains `query`, case-insensitively,
    /// in reading order.
    ///
    /// An empty query matches nothing: an empty find bar that highlighted every
    /// line in the capture would be noise, not feedback.
    pub fn find(&self, query: &str) -> Vec<usize> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, h)| h.text.to_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect()
    }

    /// Word-level rects for the `find` hits, aligned with `find(query)`'s
    /// output: every word whose span intersects a needle occurrence inside
    /// the line. Falls back to the line rect when the engine reported no
    /// word geometry or a word's text can't be mapped back (CJK lines
    /// lose their separator spaces under `tidy`).
    pub fn find_rects(&self, query: &str) -> Vec<Vec<RectF>> {
        let needle = query.trim().to_lowercase();
        self.find(query)
            .iter()
            .map(|&i| {
                let h = &self.lines[i];
                if h.words.is_empty() {
                    return vec![h.rect];
                }
                let hay = h.text.to_lowercase();
                // Each word's span inside the line text, found by scanning
                // forward — robust to separators `tidy` removed.
                let mut spans = Vec::with_capacity(h.words.len());
                let mut off = 0usize;
                for (_, t) in &h.words {
                    let tl = t.to_lowercase();
                    match hay.get(off..).and_then(|s| s.find(&tl)) {
                        Some(rel) => {
                            let ws = off + rel;
                            spans.push(ws..ws + tl.len());
                            off = ws + tl.len();
                        }
                        None => return vec![h.rect],
                    }
                }
                let rects: Vec<RectF> = h
                    .words
                    .iter()
                    .zip(&spans)
                    .filter(|(_, span)| {
                        hay.match_indices(&needle)
                            .any(|(ms, _)| span.start < ms + needle.len() && span.end > ms)
                    })
                    .map(|((r, _), _)| *r)
                    .collect();
                if rects.is_empty() {
                    vec![h.rect]
                } else {
                    rects
                }
            })
            .collect()
    }

    /// Every line whose centre falls inside `region`, one per line, in engine
    /// (reading) order.
    ///
    /// Centre-based on purpose: a line half outside the selection is either
    /// wanted or not, and splitting it by area gives no better answer while
    /// making the result depend on how the box was dragged.
    pub fn text_in(&self, region: RectF) -> String {
        let r = region.normalized();
        self.lines
            .iter()
            .filter(|h| {
                let centre = PointF::new(h.rect.x + h.rect.w / 2.0, h.rect.y + h.rect.h / 2.0);
                r.contains(centre)
            })
            .map(|h| h.text.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The line under `p`, preferring the tightest box: a heading recognized
    /// inside a larger block should win over the block.
    pub fn hit(&self, p: PointF) -> Option<&Hint> {
        let tightest = |tol: f32| {
            self.lines
                .iter()
                .filter(|h| h.rect.normalized().inflate(tol).contains(p))
                .min_by(|a, b| area(&a.rect).total_cmp(&area(&b.rect)))
        };
        // A couple of pixels of slack, because clicking the very edge of a
        // glyph box is easy to miss and the user meant it anyway.
        tightest(0.0).or_else(|| tightest(2.0))
    }

    /// Detect lines likely containing sensitive PII / credentials (emails,
    /// IPv4 addresses, API keys / tokens, phone-like digit runs) for 1-click
    /// Auto-Redact.
    pub fn sensitive_rects(&self) -> Vec<RectF> {
        self.lines
            .iter()
            .filter(|h| is_sensitive_text(&h.text))
            .map(|h| h.rect)
            .collect()
    }
}

fn is_sensitive_text(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    let lower = t.to_lowercase();
    // Explicit secret / token prefixes
    for prefix in [
        "sk-", "pk_", "ghp_", "gho_", "ghu_", "ghs_", "xoxb-", "xoxp-", "akia", "eyj",
        "bearer ", "token=", "apikey=", "api_key=", "password=", "secret=",
    ] {
        if lower.contains(prefix) {
            return true;
        }
    }
    for tok in t.split_whitespace() {
        let tok = tok.trim_matches(|c: char| ",;:\"'()[]{}<>".contains(c));
        // Email address: local@domain.tld
        if let Some((local, domain)) = tok.split_once('@') {
            if !local.is_empty() && domain.contains('.') && domain.len() >= 4 {
                return true;
            }
        }
        // IPv4 address: a.b.c.d
        let parts: Vec<&str> = tok.split('.').collect();
        if parts.len() == 4 && parts.iter().all(|p| !p.is_empty() && p.parse::<u8>().is_ok()) {
            return true;
        }
        // Long high-entropy secret/key token (>= 24 hex/alnum chars with digits)
        if tok.len() >= 24
            && tok.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && tok.chars().any(|c| c.is_ascii_digit())
            && tok.chars().any(|c| c.is_ascii_alphabetic())
        {
            return true;
        }
    }
    // Phone number pattern: >= 9 digits in a short line
    let digits = t.chars().filter(|c| c.is_ascii_digit()).count();
    if (9..=15).contains(&digits) && t.len() <= 24 {
        let non_phone = t
            .chars()
            .filter(|c| !c.is_ascii_digit() && !" +-().".contains(*c))
            .count();
        if non_phone == 0 {
            return true;
        }
    }
    false
}

fn area(r: &RectF) -> f32 {
    r.w.abs() * r.h.abs()
}

/// Rect in base-image pixels to document coordinates.
///
/// Rendering maps document pixels as `px = (x - view.x) * scale`, so this is
/// that mapping inverted; it is the only place the conversion lives.
pub(crate) fn px_to_doc(x: f32, y: f32, w: f32, h: f32, view: RectF, scale: f32) -> RectF {
    let s = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    RectF::new(x / s + view.x, y / s + view.y, w / s, h / s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_rects_map_into_document_space() {
        // scale 2, view offset (10, 20): pixel (40, 60) is document (30, 50).
        let r = px_to_doc(40.0, 60.0, 20.0, 10.0, RectF::new(10.0, 20.0, 500.0, 400.0), 2.0);
        assert_eq!(r, RectF::new(30.0, 50.0, 10.0, 5.0));
    }

    #[test]
    fn a_broken_scale_falls_back_to_one_to_one() {
        let r = px_to_doc(10.0, 10.0, 5.0, 5.0, RectF::new(0.0, 0.0, 100.0, 100.0), 0.0);
        assert_eq!(r, RectF::new(10.0, 10.0, 5.0, 5.0));
    }

    fn hint(x: f32, y: f32, w: f32, h: f32, text: &str) -> Hint {
        Hint {
            rect: RectF::new(x, y, w, h),
            text: text.to_string(),
            words: Vec::new(),
        }
    }

    #[test]
    fn hit_prefers_the_tightest_box_under_the_pointer() {
        let hints = OcrHints {
            lines: vec![
                hint(0.0, 0.0, 200.0, 60.0, "block"),
                hint(10.0, 10.0, 80.0, 20.0, "heading"),
            ],
            ..OcrHints::default()
        };
        let picked = hints.hit(PointF::new(20.0, 15.0)).unwrap();
        assert_eq!(picked.text, "heading");
    }

    #[test]
    fn hit_is_none_outside_every_line() {
        let hints = OcrHints {
            lines: vec![hint(0.0, 0.0, 40.0, 10.0, "x")],
            ..OcrHints::default()
        };
        assert!(hints.hit(PointF::new(200.0, 200.0)).is_none());
    }

    #[test]
    fn hit_allows_two_pixels_of_slop() {
        let hints = OcrHints {
            lines: vec![hint(10.0, 10.0, 20.0, 10.0, "x")],
            ..OcrHints::default()
        };
        assert!(hints.hit(PointF::new(9.0, 10.5)).is_some(), "just outside");
        assert!(hints.hit(PointF::new(4.0, 10.5)).is_none(), "far outside");
    }

    #[test]
    fn polling_without_work_is_a_no_op() {
        let mut hints = OcrHints::default();
        assert!(!hints.poll());
        assert!(hints.is_empty());
    }
    #[test]
    fn text_in_keeps_only_the_lines_inside_the_region() {
        let hints = OcrHints {
            lines: vec![
                hint(0.0, 0.0, 100.0, 20.0, "first"),
                hint(0.0, 40.0, 100.0, 20.0, "second"),
                hint(0.0, 200.0, 100.0, 20.0, "outside"),
            ],
            ..OcrHints::default()
        };
        let picked = hints.text_in(RectF::new(0.0, 0.0, 120.0, 100.0));
        assert_eq!(picked, "first\nsecond", "reading order, outside line dropped");
    }

    #[test]
    fn text_in_a_region_with_nothing_is_empty_not_an_error() {
        let hints = OcrHints {
            lines: vec![hint(0.0, 0.0, 10.0, 10.0, "x")],
            ..OcrHints::default()
        };
        assert!(hints.text_in(RectF::new(500.0, 500.0, 20.0, 20.0)).is_empty());
    }

    #[test]
    fn text_in_uses_the_line_centre_not_any_overlap() {
        // The line starts inside the region but its centre is outside it.
        let hints = OcrHints {
            lines: vec![hint(90.0, 0.0, 100.0, 20.0, "mostly outside")],
            ..OcrHints::default()
        };
        assert!(hints.text_in(RectF::new(0.0, 0.0, 100.0, 50.0)).is_empty());
    }
    #[test]
    fn find_rects_picks_only_the_matched_words() {
        let mut h = hint(0.0, 0.0, 300.0, 20.0, "General Settings Page");
        h.words = vec![
            (RectF::new(0.0, 0.0, 80.0, 20.0), "General".to_string()),
            (RectF::new(90.0, 0.0, 100.0, 20.0), "Settings".to_string()),
            (RectF::new(200.0, 0.0, 60.0, 20.0), "Page".to_string()),
        ];
        let hints = OcrHints {
            lines: vec![h],
            ..OcrHints::default()
        };
        let groups = hints.find_rects("settings");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], vec![RectF::new(90.0, 0.0, 100.0, 20.0)]);
        // a needle spanning a space covers both words
        let groups = hints.find_rects("settings page");
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn find_rects_falls_back_to_the_line_when_words_cant_be_mapped() {
        let mut h = hint(0.0, 0.0, 100.0, 20.0, "你好世界");
        // CJK line text lost its separator spaces under tidy — a stored
        // word that no longer appears in the text falls back to the line.
        h.words = vec![
            (RectF::new(0.0, 0.0, 50.0, 20.0), "你好".to_string()),
            (RectF::new(50.0, 0.0, 50.0, 20.0), "missing".to_string()),
        ];
        let hints = OcrHints {
            lines: vec![h],
            ..OcrHints::default()
        };
        assert_eq!(
            hints.find_rects("世界"),
            vec![vec![RectF::new(0.0, 0.0, 100.0, 20.0)]]
        );
        // and a line with no word geometry at all
        let hints = OcrHints {
            lines: vec![hint(5.0, 5.0, 50.0, 10.0, "abc")],
            ..OcrHints::default()
        };
        assert_eq!(
            hints.find_rects("abc"),
            vec![vec![RectF::new(5.0, 5.0, 50.0, 10.0)]]
        );
    }

    #[test]
    fn find_matches_case_insensitively_in_reading_order() {
        let hints = OcrHints {
            lines: vec![
                hint(0.0, 0.0, 10.0, 10.0, "General Settings"),
                hint(0.0, 20.0, 10.0, 10.0, "Shortcuts"),
                hint(0.0, 40.0, 10.0, 10.0, "SETTINGS again"),
            ],
            ..OcrHints::default()
        };
        assert_eq!(hints.find("settings"), vec![0, 2]);
        assert_eq!(hints.find("SHORT"), vec![1]);
        assert!(hints.find("missing").is_empty());
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        let hints = OcrHints {
            lines: vec![hint(0.0, 0.0, 10.0, 10.0, "x")],
            ..OcrHints::default()
        };
        assert!(hints.find("").is_empty());
        assert!(hints.find("   ").is_empty());
    }
}
