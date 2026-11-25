use anyhow::Result;
use image::RgbaImage;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// OCR languages offered on the Settings page — (tessdata code, display
/// name, lowercase subtag prefix matching the OS engines' BCP-47 tags
/// back to this code). `Config.ocr_lang` stores codes joined by "+",
/// or "auto" for engine auto-detection.
pub const LANGS: &[(&str, &str, &str)] = &[
    ("eng", "English", "en"),
    ("vie", "Vietnamese", "vi"),
    ("jpn", "Japanese", "ja"),
    ("kor", "Korean", "ko"),
    ("chi_sim", "Chinese (Simplified)", "zh-hans"),
    ("chi_tra", "Chinese (Traditional)", "zh-hant"),
    ("fra", "French", "fr"),
    ("deu", "German", "de"),
    ("spa", "Spanish", "es"),
    ("por", "Portuguese", "pt"),
    ("rus", "Russian", "ru"),
    ("tha", "Thai", "th"),
];

/// Parse an ocr_lang value ("auto" or "eng+jpn") into tessdata codes.
/// An empty vec means auto-detection.
pub fn lang_codes(ocr_lang: &str) -> Vec<String> {
    ocr_lang
        .split('+')
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "auto")
        .map(str::to_owned)
        .collect()
}

/// tessdata codes the platform engine can use today — feeds the
/// "not installed" badges on the Settings OCR page.
pub fn installed_langs() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        linux::installed()
    }
    #[cfg(target_os = "macos")]
    {
        canonicalize(macos::installed())
    }
    #[cfg(target_os = "windows")]
    {
        canonicalize(windows::installed())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Vec::new()
    }
}

/// OS language tags (BCP-47) → canonical tessdata codes from LANGS.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn canonicalize(tags: Vec<String>) -> Vec<String> {
    let mut v: Vec<String> = tags
        .iter()
        .filter_map(|t| canonical(t).map(str::to_owned))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// A tag covers a LANGS prefix on exact match or as a leading subtag
/// ("en-GB" → en, "zh-Hant-HK" → zh-hant).
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn canonical(tag: &str) -> Option<&'static str> {
    let t = tag.to_lowercase().replace('_', "-");
    LANGS
        .iter()
        .find(|e| t == e.2 || t.starts_with(&format!("{}-", e.2)))
        .map(|e| e.0)
}

/// tesseract-style codes -> BCP-47 tags the OS engines understand.
/// Shared by the Vision and WinRT backends; unknown codes are skipped
/// rather than failing the request.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn bcp47(code: &str) -> Option<&'static str> {
    Some(match code.trim() {
        "eng" | "en" | "en-US" => "en-US",
        "vie" | "vi" | "vi-VN" => "vi-VN",
        "jpn" | "ja" | "ja-JP" => "ja-JP",
        "kor" | "ko" | "ko-KR" => "ko-KR",
        "chi_sim" | "zh-Hans" | "zh-CN" => "zh-Hans",
        "chi_tra" | "zh-Hant" | "zh-TW" => "zh-Hant",
        "fra" | "fr" | "fr-FR" => "fr-FR",
        "deu" | "de" | "de-DE" => "de-DE",
        "spa" | "es" | "es-ES" => "es-ES",
        "tha" | "th" | "th-TH" => "th-TH",
        "ind" | "id" | "id-ID" => "id-ID",
        _ => return None,
    })
}

/// One recognized word and where it sits in the image.
#[derive(Clone, Debug, PartialEq)]
pub struct OcrWord {
    pub text: String,
    /// Pixel rect in the image handed to the engine, origin at the top-left.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Engine confidence in 0..=1; 0 when the engine reports none.
    pub confidence: f32,
}

/// One recognized text line and where it sits in the image.
#[derive(Clone, Debug, PartialEq)]
pub struct OcrLine {
    pub text: String,
    /// Pixel rect in the image handed to the engine, origin at the top-left.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Engine confidence in 0..=1; 0 when the engine reports none.
    pub confidence: f32,
    /// Words the line is made of, in reading order, same pixel space as the
    /// line rect. The union of word rects approximates the line box — engines
    /// pad it slightly, so exact equality is not part of the contract.
    pub words: Vec<OcrWord>,
}

/// Vision reports a rect normalized to 0..=1 with the origin at the
/// **bottom-left**; every other surface in this workspace is top-left pixels.
/// The rect is clamped to the image so a caller can draw it as-is.
#[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
pub(crate) fn px_rect_from_normalized(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    img_w: u32,
    img_h: u32,
) -> (f32, f32, f32, f32) {
    let iw = img_w.max(1) as f64;
    let ih = img_h.max(1) as f64;
    let left = (x.clamp(0.0, 1.0) * iw).min(iw);
    let px_w = (w.clamp(0.0, 1.0) * iw).min(iw - left);
    let px_h = (h.clamp(0.0, 1.0) * ih).min(ih);
    // Flip the vertical axis first: Vision's `y` is the line's bottom edge.
    let top = (ih - (y.clamp(0.0, 1.0) * ih) - px_h).max(0.0);
    (left as f32, top as f32, px_w as f32, px_h as f32)
}

/// Extract text together with the rectangle each line occupies.
///
/// An engine that finds nothing returns an empty list; a platform whose engine
/// cannot report geometry returns an error. Collapsing those two would make the
/// UI say "no text here" when the truth is "this build cannot tell you".
pub fn extract_lines(img: &RgbaImage, lang: &str) -> Result<Vec<OcrLine>> {
    #[cfg(target_os = "macos")]
    {
        macos::extract_lines(img, lang)
    }
    #[cfg(target_os = "windows")]
    {
        windows::extract_lines(img, lang)
    }
    #[cfg(target_os = "linux")]
    {
        linux::extract_lines(img, lang)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (img, lang);
        anyhow::bail!("ocr line geometry is not implemented for this platform")
    }
}

/// Axis-aligned union of `(x, y, w, h)` pixel rects — how tesseract's
/// tsv words and WinRT's OcrWord bounding rects become a line box.
/// Shared so both engines (and the unit tests that simulate their raw
/// output) exercise the same math.
#[cfg_attr(not(any(test, target_os = "linux", target_os = "windows")), allow(dead_code))]
pub(crate) fn union_rects(rects: &[(f32, f32, f32, f32)]) -> (f32, f32, f32, f32) {
    let (mut x0, mut y0) = (f32::MAX, f32::MAX);
    let (mut x1, mut y1) = (f32::MIN, f32::MIN);
    for &(x, y, w, h) in rects {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + w);
        y1 = y1.max(y + h);
    }
    if x0 <= x1 && y0 <= y1 {
        (x0, y0, x1 - x0, y1 - y0)
    } else {
        (0.0, 0.0, 0.0, 0.0)
    }
}

/// Parse tesseract's `tsv` output into line-level geometry.
///
/// Columns: level, page, block, par, line, word, left, top, width, height,
/// conf, text. Only `level == 5` rows are words; a line is their union, its text
/// is the words joined by a space, and its confidence is their mean. The Linux
/// backend runs `tidy` over each line afterwards, because tesseract splits CJK
/// into space-separated glyphs.
#[cfg_attr(not(any(test, target_os = "linux")), allow(dead_code))]
pub(crate) fn parse_tesseract_tsv(tsv: &str) -> Vec<OcrLine> {
    let mut groups: Vec<((u32, u32, u32, u32), OcrLine)> = Vec::new();

    for row in tsv.lines() {
        // `splitn` keeps the text column intact even if it contains a tab.
        let cols: Vec<&str> = row.splitn(12, '\t').collect();
        if cols.len() < 12 {
            continue;
        }
        let Ok(level) = cols[0].trim().parse::<u32>() else {
            continue; // the header row
        };
        if level != 5 {
            continue; // page / block / paragraph / line summaries
        }
        let text = cols[11].trim();
        if text.is_empty() {
            continue;
        }
        let num = |i: usize| cols[i].trim().parse::<f32>().unwrap_or(0.0);
        let key = (
            cols[1].trim().parse().unwrap_or(0),
            cols[2].trim().parse().unwrap_or(0),
            cols[3].trim().parse().unwrap_or(0),
            cols[4].trim().parse().unwrap_or(0),
        );
        let (x, y, w, h) = (num(6), num(7), num(8), num(9));
        // Tesseract reports confidence as a percentage, and -1 for "no idea".
        let conf = match cols[10].trim().parse::<f32>() {
            Ok(c) if c >= 0.0 => (c / 100.0).clamp(0.0, 1.0),
            _ => 0.0,
        };
        let word = OcrWord {
            text: text.to_string(),
            x,
            y,
            w,
            h,
            confidence: conf,
        };

        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, line)) => {
                line.words.push(word);
                line.text.push(' ');
                line.text.push_str(text);
            }
            None => groups.push((
                key,
                OcrLine {
                    text: text.to_string(),
                    x,
                    y,
                    w,
                    h,
                    confidence: 0.0,
                    words: vec![word],
                },
            )),
        }
    }

    groups
        .into_iter()
        .map(|(_, mut line)| {
            let rects: Vec<(f32, f32, f32, f32)> =
                line.words.iter().map(|w| (w.x, w.y, w.w, w.h)).collect();
            (line.x, line.y, line.w, line.h) = union_rects(&rects);
            let n = line.words.len() as f32;
            line.confidence = if n > 0.0 {
                line.words.iter().map(|w| w.confidence).sum::<f32>() / n
            } else {
                0.0
            };
            line
        })
        .collect()
}

/// Extract text from an image using the platform-native OCR engine.
/// Returns recognized text with lines separated by '\n'.
/// `lang` uses tesseract's `eng+vie` syntax; "auto"/"" lets the engine
/// detect. Windows.Media.Ocr is single-language — the first code wins.
pub fn extract_text(img: &RgbaImage, lang: &str) -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        macos::extract(img, lang)
    }
    #[cfg(target_os = "windows")]
    {
        windows::extract(img, lang)
    }
    #[cfg(target_os = "linux")]
    {
        linux::extract(img, lang).map(|t| tidy(&t))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = img;
        anyhow::bail!("ocr not implemented for this platform")
    }
}

/// Tesseract tokenizes CJK into space-separated "words", but real CJK text has
/// no spaces — drop a space only when BOTH neighbours are non-ASCII, so Latin
/// lines and mixed words ('café bar') stay intact.
///
/// Available on every platform so the Linux path's text handling is testable
/// where it cannot be run.
#[cfg_attr(not(any(test, target_os = "linux")), allow(dead_code))]
fn tidy(text: &str) -> String {
    text.lines()
        .map(|l| {
            let c: Vec<char> = l.chars().collect();
            let mut out = String::with_capacity(l.len());
            for (i, ch) in c.iter().enumerate() {
                let squash = *ch == ' '
                    && i > 0
                    && !c[i - 1].is_ascii()
                    && i + 1 < c.len()
                    && !c[i + 1].is_ascii();
                if !squash {
                    out.push(*ch);
                }
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::{parse_tesseract_tsv, px_rect_from_normalized, tidy};

    #[test]
    fn a_line_at_the_top_left_maps_to_pixel_zero() {
        // Vision: y is the bottom edge, so a line hugging the top has y ~ 0.9.
        let (x, y, w, h) = px_rect_from_normalized(0.0, 0.9, 0.5, 0.1, 100, 50);
        assert_eq!((x, y, w, h), (0.0, 0.0, 50.0, 5.0));
    }

    #[test]
    fn a_line_at_the_bottom_sits_at_the_image_height() {
        let (x, y, w, h) = px_rect_from_normalized(0.0, 0.0, 0.5, 0.1, 100, 50);
        assert_eq!((x, y, w, h), (0.0, 45.0, 50.0, 5.0));
    }

    #[test]
    fn a_full_image_rect_covers_everything() {
        let (x, y, w, h) = px_rect_from_normalized(0.0, 0.0, 1.0, 1.0, 100, 50);
        assert_eq!((x, y, w, h), (0.0, 0.0, 100.0, 50.0));
    }

    #[test]
    fn a_zero_size_observation_is_kept_not_dropped() {
        let (x, y, w, h) = px_rect_from_normalized(0.5, 0.5, 0.0, 0.0, 100, 50);
        assert_eq!((x, y, w, h), (50.0, 25.0, 0.0, 0.0));
    }

    #[test]
    fn a_rect_overrunning_the_edge_is_clamped_inside() {
        let (x, y, w, h) = px_rect_from_normalized(0.9, 0.05, 0.5, 0.5, 100, 50);
        assert_eq!(x, 90.0);
        assert_eq!(w, 10.0, "width is reduced so the rect stays on the image");
        assert_eq!(h, 25.0);
        assert!(
            x + w <= 100.0 && y >= 0.0 && y + h <= 50.0,
            "rect {} {} {} {} escapes the image",
            x,
            y,
            w,
            h
        );
    }

    #[test]
    fn a_degenerate_image_size_does_not_divide_by_zero() {
        let (x, y, w, h) = px_rect_from_normalized(0.0, 0.0, 1.0, 1.0, 0, 0);
        assert!(x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite());
    }

    // The same three words at 2000x1000 (a Retina @2x frame), expressed the
    // way each backend reports them, must land on identical pixel rects.

    #[test]
    fn vision_normalized_and_native_pixels_agree_on_a_2000x1000_frame() {
        // "Settings" line near the top: top-left px rect (200, 80, 400, 48)
        // → Vision reports origin.y = 1 - (y + h) / ih, bottom-left.
        let (x, y, w, h) = px_rect_from_normalized(
            200.0 / 2000.0,
            1.0 - (80.0 + 48.0) / 1000.0,
            400.0 / 2000.0,
            48.0 / 1000.0,
            2000,
            1000,
        );
        assert_eq!((x, y, w, h), (200.0, 80.0, 400.0, 48.0));

        // Same words as tesseract tsv — union must be the identical rect.
        let tsv = concat!(
            "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n",
            "5\t1\t1\t1\t1\t1\t200\t80\t180\t48\t96\tSettings\n",
            "5\t1\t1\t1\t1\t2\t392\t80\t208\t48\t96\tpage\n"
        );
        let lines = parse_tesseract_tsv(tsv);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            (lines[0].x, lines[0].y, lines[0].w, lines[0].h),
            (200.0, 80.0, 400.0, 48.0)
        );
        assert_eq!(lines[0].words.len(), 2);
        assert_eq!(
            (lines[0].words[0].x, lines[0].words[0].y, lines[0].words[0].w, lines[0].words[0].h),
            (200.0, 80.0, 180.0, 48.0)
        );

        // Same words as WinRT BoundingRect rows — union_rects is what
        // windows.rs feeds them through.
        assert_eq!(
            super::union_rects(&[(200.0, 80.0, 180.0, 48.0), (392.0, 80.0, 208.0, 48.0)]),
            (200.0, 80.0, 400.0, 48.0)
        );
    }

    #[test]
    fn vision_word_boxes_and_line_boxes_share_the_same_pixel_space() {
        // A word and a line reported for the same glyph run land on the same
        // rect — word_boxes on macOS routes both through this one function.
        let line = px_rect_from_normalized(0.1, 0.872, 0.2, 0.048, 2000, 1000);
        let word = px_rect_from_normalized(0.1, 0.872, 0.09, 0.048, 2000, 1000);
        assert_eq!(line, (200.0, 80.0, 400.0, 48.0));
        assert_eq!(word, (200.0, 80.0, 180.0, 48.0));
    }
    #[test]
    fn tesseract_tsv_groups_words_into_lines() {
        let tsv = concat!(
            "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext\n",
            "1\t1\t0\t0\t0\t0\t0\t0\t400\t160\t-1\t\n",
            "5\t1\t1\t1\t1\t1\t24\t24\t120\t40\t90\tHELLO\n",
            "5\t1\t1\t1\t1\t2\t150\t24\t130\t40\t100\tWORLD\n",
            "5\t1\t1\t1\t2\t1\t24\t96\t200\t40\t80\tSECOND\n"
        );
        let lines = parse_tesseract_tsv(tsv);
        assert_eq!(lines.len(), 2, "header and summaries are not lines");
        assert_eq!(lines[0].text, "HELLO WORLD");
        assert_eq!((lines[0].x, lines[0].y), (24.0, 24.0));
        assert_eq!((lines[0].w, lines[0].h), (256.0, 40.0), "union of both words");
        assert!((lines[0].confidence - 0.95).abs() < 0.001, "mean of 90 and 100");
        assert_eq!(lines[1].text, "SECOND");
        assert!((lines[1].confidence - 0.8).abs() < 0.001);
    }

    #[test]
    fn tesseract_tsv_ignores_empty_words_and_bad_confidence() {
        let tsv = concat!(
            "5\t1\t1\t1\t1\t1\t10\t10\t20\t10\t-1\tHI\n",
            "5\t1\t1\t1\t1\t2\t40\t10\t20\t10\t-1\t   \n"
        );
        let lines = parse_tesseract_tsv(tsv);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "HI", "a blank word is not appended");
        assert_eq!(lines[0].confidence, 0.0, "-1 means 'no confidence'");
    }

    #[test]
    fn tesseract_tsv_of_nothing_is_empty() {
        assert!(parse_tesseract_tsv("").is_empty());
        assert!(parse_tesseract_tsv("garbage").is_empty());
    }
    #[test]
    fn tidy_drops_only_the_artificial_cjk_spaces() {
        assert_eq!(tidy("你 好 世 界"), "你好世界");
        assert_eq!(tidy("café bar"), "café bar", "latin words keep their space");
        assert_eq!(
            tidy("Hello 世界"),
            "Hello 世界",
            "a space between ascii and cjk is real"
        );
    }
}
