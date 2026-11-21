//! Windows OCR via the built-in WinRT Windows.Media.Ocr engine — no bundled
//! model. "auto" uses the user's profile languages; an explicit Settings pick
//! resolves to a language pack via TryCreateFromLanguage.

use anyhow::{Context, Result};
use image::RgbaImage;
use windows::core::HSTRING;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::{OcrEngine, OcrResult};
use windows::Storage::Streams::DataWriter;

/// tesseract-style codes -> BCP-47 tags the OS language packs use.
/// Mirrors macos.rs; unknown codes are skipped.
fn bcp47(code: &str) -> Option<&'static str> {
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

pub fn extract(img: &RgbaImage, lang: &str) -> Result<String> {
    let res = recognize(img, lang)?;
    Ok(res.Text()?.to_string_lossy())
}

/// Text with line geometry.
///
/// WinRT reports each word's bounding box in pixels with a top-left origin, so a
/// line is the union of its words. The engine exposes no per-word confidence, so
/// `confidence` stays 0 ("unknown") rather than being invented.
pub fn extract_lines(img: &RgbaImage, lang: &str) -> Result<Vec<crate::OcrLine>> {
    let res = recognize(img, lang)?;
    let lines = res.Lines()?;
    let mut out = Vec::new();
    for i in 0..lines.Size().unwrap_or(0) {
        let Ok(line) = lines.GetAt(i) else { continue };
        let text = line
            .Text()
            .map(|t| t.to_string_lossy())
            .unwrap_or_default();
        let (mut x0, mut y0) = (f32::MAX, f32::MAX);
        let (mut x1, mut y1) = (f32::MIN, f32::MIN);
        if let Ok(words) = line.Words() {
            for w in 0..words.Size().unwrap_or(0) {
                let Ok(word) = words.GetAt(w) else { continue };
                let Ok(r) = word.BoundingRect() else { continue };
                x0 = x0.min(r.X);
                y0 = y0.min(r.Y);
                x1 = x1.max(r.X + r.Width);
                y1 = y1.max(r.Y + r.Height);
            }
        }
        let (x, y, w, h) = if x0 <= x1 && y0 <= y1 {
            (x0, y0, x1 - x0, y1 - y0)
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };
        out.push(crate::OcrLine {
            text,
            x,
            y,
            w,
            h,
            confidence: 0.0,
        });
    }
    Ok(out)
}

/// Shared setup: bitmap + the single language pack WinRT will use.
fn recognize(img: &RgbaImage, lang: &str) -> Result<OcrResult> {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let mut bgra = img.as_raw().clone();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }

    let writer = DataWriter::new()?;
    writer.WriteBytes(&bgra)?;
    let buf = writer.DetachBuffer()?;

    let bitmap = SoftwareBitmap::Create(BitmapPixelFormat::Bgra8, w, h)?;
    bitmap.CopyToBuffer(&buf)?;

    // OcrEngine is single-language — the first listed code wins
    let tag = lang
        .split(|c| c == '+' || c == ',' || c == ' ')
        .find_map(bcp47);
    let engine = match tag {
        Some(tag) => {
            let l = Language::CreateLanguage(&HSTRING::from(tag))?;
            OcrEngine::TryCreateFromLanguage(&l).with_context(|| {
                format!("OCR language {tag} is not installed — Settings > Language & region")
            })?
        }
        None => OcrEngine::TryCreateFromUserProfileLanguages()
            .context("no OCR language pack installed")?,
    };
    Ok(engine.RecognizeAsync(&bitmap)?.join()?)
}

/// BCP-47 tags of the OCR packs installed on this machine — feeds the
/// Settings "not installed" badges.
pub fn installed() -> Vec<String> {
    let mut out = Vec::new();
    let Ok(langs) = OcrEngine::AvailableRecognizerLanguages() else {
        return out;
    };
    for i in 0..langs.Size().unwrap_or(0) {
        if let Ok(tag) = langs.GetAt(i).and_then(|l| l.LanguageTag()) {
            out.push(tag.to_string_lossy());
        }
    }
    out
}
