//! macOS OCR via Vision's VNRecognizeTextRequest — no bundled model, runs on
//! the Neural Engine, supports Vietnamese/English text.

use anyhow::{Context, Result};
use image::RgbaImage;
use objc2::rc::Retained;
use objc2::AllocAnyThread;
use objc2_foundation::{NSArray, NSData, NSDictionary, NSRange, NSString};
use objc2_vision::{
    VNImageOption, VNImageRequestHandler, VNRecognizeTextRequest, VNRecognizedTextObservation,
    VNRequest, VNRequestTextRecognitionLevel,
};

/// Text only — the lines joined by `\n`, exactly the texts `extract_lines`
/// returns for the same image so the two cannot drift.
pub fn extract(img: &RgbaImage, lang: &str) -> Result<String> {
    let lines = extract_lines(img, lang)?;
    Ok(lines
        .into_iter()
        .map(|l| l.text)
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn extract_lines(img: &RgbaImage, lang: &str) -> Result<Vec<crate::OcrLine>> {
    // Feed Vision encoded image data — a PNG roundtrip is the cheapest way to
    // build a valid NSData source without touching CoreGraphics types.
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
    let data = NSData::from_vec(png);
    let opts = NSDictionary::<VNImageOption, objc2::runtime::AnyObject>::new();
    let handler =
        VNImageRequestHandler::initWithData_options(VNImageRequestHandler::alloc(), &data, &opts);

    let req = VNRecognizeTextRequest::new();
    req.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    req.setUsesLanguageCorrection(true);
    // "auto"/"" keeps the vi+en baseline with language detection on —
    // Vision ignores recognitionLanguages while detection is enabled,
    // so an explicit pick must turn detection off.
    let explicit: Vec<_> = lang
        .split(['+', ',', ' '])
        .filter_map(crate::bcp47)
        .collect();
    if explicit.is_empty() {
        req.setAutomaticallyDetectsLanguage(true);
        let langs = NSArray::from_retained_slice(&[
            NSString::from_str("vi-VN"),
            NSString::from_str("en-US"),
        ]);
        req.setRecognitionLanguages(&langs);
    } else {
        req.setAutomaticallyDetectsLanguage(false);
        let tags: Vec<_> = explicit.iter().map(|t| NSString::from_str(t)).collect();
        req.setRecognitionLanguages(&NSArray::from_retained_slice(&tags));
    }

    let req_vn: Retained<VNRequest> = unsafe { Retained::cast_unchecked(req.clone()) };
    let reqs = NSArray::from_retained_slice(&[req_vn]);
    handler
        .performRequests_error(&reqs)
        .context("VNRecognizeTextRequest failed")?;

    let mut lines = Vec::new();
    if let Some(results) = req.results() {
        for obs in results.iter() {
            let Some(obs) = obs.downcast_ref::<VNRecognizedTextObservation>() else {
                continue;
            };
            let Some(best) = obs.topCandidates(1).iter().next() else {
                continue;
            };
            // Vision's box is normalized with a bottom-left origin; the helper
            // turns it into top-left pixels of the image we passed in.
            // `boundingBox` is marked unsafe by the bindings (it may trap on a
            // freed observation); the observation is alive for this iteration.
            let bb = unsafe { obs.boundingBox() };
            let (x, y, w, h) = crate::px_rect_from_normalized(
                bb.origin.x,
                bb.origin.y,
                bb.size.width,
                bb.size.height,
                img.width(),
                img.height(),
            );
            let confidence = best.confidence().clamp(0.0, 1.0);
            let words = word_boxes(&best, img.width(), img.height(), confidence);
            lines.push(crate::OcrLine {
                text: best.string().to_string(),
                x,
                y,
                w,
                h,
                confidence,
                words,
            });
        }
    }
    Ok(lines)
}

/// Per-token boxes via `boundingBoxForRange` — same normalized bottom-left
/// space as the line box, run through the shared conversion. `NSRange` counts
/// UTF-16 code units, so char boundaries are measured in `encode_utf16`.
/// Tokens whose range query errors are skipped, never fatal to the line.
fn word_boxes(
    best: &objc2_vision::VNRecognizedText,
    img_w: u32,
    img_h: u32,
    confidence: f32,
) -> Vec<crate::OcrWord> {
    let text = best.string().to_string();
    let mut words = Vec::new();
    // (byte_start, token) — whitespace-separated tokens in reading order.
    let mut start = 0usize;
    for (i, c) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
        if c.is_whitespace() {
            if i > start {
                words.push((start, &text[start..i]));
            }
            start = i + c.len_utf8();
        }
    }
    let mut out = Vec::with_capacity(words.len());
    let mut utf16_off = 0usize;
    let mut consumed = 0usize; // bytes of `text` already walked
    for (byte_start, token) in words {
        utf16_off += text[consumed..byte_start].encode_utf16().count();
        consumed = byte_start;
        let range = NSRange {
            location: utf16_off,
            length: token.encode_utf16().count(),
        };
        utf16_off += range.length;
        consumed += token.len();
        let Ok(rect) = (unsafe { best.boundingBoxForRange_error(range) }) else {
            continue;
        };
        let bb = unsafe { rect.boundingBox() };
        let (x, y, w, h) =
            crate::px_rect_from_normalized(bb.origin.x, bb.origin.y, bb.size.width, bb.size.height, img_w, img_h);
        out.push(crate::OcrWord {
            text: token.to_string(),
            x,
            y,
            w,
            h,
            confidence,
        });
    }
    out
}

/// BCP-47 tags Vision can recognize at the Accurate level — feeds the
/// Settings "not installed" badges.
pub fn installed() -> Vec<String> {
    let req = VNRecognizeTextRequest::new();
    req.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    match unsafe { req.supportedRecognitionLanguagesAndReturnError() } {
        Ok(langs) => langs.iter().map(|s| s.to_string()).collect(),
        Err(_) => Vec::new(),
    }
}
