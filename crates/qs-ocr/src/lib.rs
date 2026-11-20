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
}

/// Vision reports a rect normalized to 0..=1 with the origin at the
/// **bottom-left**; every other surface in this workspace is top-left pixels.
/// The rect is clamped to the image so a caller can draw it as-is.
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

/// Tesseract tokenizes CJK into space-separated "words", but real CJK
/// text has no spaces — drop a space only when BOTH neighbours are
/// non-ASCII, so Latin lines and mixed words ('café bar') stay intact.
#[cfg(target_os = "linux")]
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
    use super::px_rect_from_normalized;

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
}
