//! Smart Move hints against a real screenshot: the rectangle offered on hover
//! must be the one the engine recognized, mapped into document space.

use qs_doc::{Document, PointF};
use qs_editor::ocr_hint::OcrHints;
use std::time::{Duration, Instant};

#[test]
fn hints_land_on_the_recognized_lines() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/shots/settings-general.png"
    );
    let Ok(base) = image::open(path) else {
        return; // screenshot not checked out
    };
    let base = base.to_rgba8();
    let doc = Document::new(base.width(), base.height(), 1.0);

    let mut hints = OcrHints::default();
    hints.ensure(&doc, &base, "eng");

    let deadline = Instant::now() + Duration::from_secs(60);
    while !hints.poll() {
        assert!(Instant::now() < deadline, "OCR worker never reported back");
        std::thread::sleep(Duration::from_millis(25));
    }

    if hints.is_empty() {
        return; // no engine / language pack on this machine
    }
    println!("{} hint lines", hints.len());

    let (w, h) = (base.width() as f32, base.height() as f32);
    for line in hints.lines() {
        let centre = PointF::new(line.rect.x + line.rect.w / 2.0, line.rect.y + line.rect.h / 2.0);
        let picked = hints.hit(centre).expect("centre of a line must hit it");
        assert!(
            (picked.rect.x - line.rect.x).abs() <= 1.0
                && (picked.rect.y - line.rect.y).abs() <= 1.0
                && (picked.rect.w - line.rect.w).abs() <= 1.0
                && (picked.rect.h - line.rect.h).abs() <= 1.0,
            "hover offered {:?} but the line is {:?}",
            picked.rect,
            line.rect
        );
        assert!(
            line.rect.x >= -1.0
                && line.rect.y >= -1.0
                && line.rect.x + line.rect.w <= w + 1.0
                && line.rect.y + line.rect.h <= h + 1.0,
            "hint {:?} escapes the {}x{} document",
            line.rect,
            w,
            h
        );
    }
}

#[test]
fn a_document_scale_maps_pixels_into_logical_units() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/shots/settings-general.png"
    );
    let Ok(base) = image::open(path) else { return };
    let base = base.to_rgba8();

    // Retina capture: 2 physical px per point. `Document::new` takes physical
    // pixels and keeps the logical size as px/scale, so the document is
    // 820x600 while the base image stays 1640x1200.
    let doc = Document::new(base.width(), base.height(), 2.0);
    assert_eq!(doc.base_size.w, base.width() as f32 / 2.0);
    let mut hints = OcrHints::default();
    hints.ensure(&doc, &base, "eng");
    let deadline = Instant::now() + Duration::from_secs(60);
    while !hints.poll() {
        assert!(Instant::now() < deadline, "OCR worker never reported back");
        std::thread::sleep(Duration::from_millis(25));
    }
    if hints.is_empty() {
        return;
    }
    let limit = doc.base_size.w + 1.0;
    for line in hints.lines() {
        assert!(
            line.rect.x + line.rect.w <= limit,
            "line {:?} not scaled into a {}pt-wide document",
            line.rect,
            limit
        );
    }
}

#[test]
fn text_in_a_region_matches_the_engine_for_the_whole_capture() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/shots/settings-general.png"
    );
    let Ok(base) = image::open(path) else { return };
    let base = base.to_rgba8();
    let doc = Document::new(base.width(), base.height(), 1.0);

    let mut hints = OcrHints::default();
    hints.ensure(&doc, &base, "eng");
    let deadline = Instant::now() + Duration::from_secs(60);
    while !hints.poll() {
        assert!(Instant::now() < deadline, "OCR worker never reported back");
        std::thread::sleep(Duration::from_millis(25));
    }
    if hints.is_empty() {
        return;
    }

    // The whole image selects everything, in the engine's reading order.
    // Blank-line fidelity is documented as intentionally different: the
    // plain-text path keeps tesseract's paragraph spacing while the
    // geometry path has no empty-line rows — compare non-empty lines only.
    let all = qs_doc::RectF::new(0.0, 0.0, base.width() as f32, base.height() as f32);
    let text = hints.text_in(all);
    assert!(!text.is_empty());
    let plain = qs_ocr::extract_text(&base, "eng").unwrap();
    let expected: Vec<&str> = plain
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert_eq!(
        text,
        expected.join("\n"),
        "the region selection must reproduce the engine text exactly"
    );

    // And a single line's own box selects it first — engine boxes overlap
    // (icon glyph + text), so a neighbour's centre may fall inside too;
    // `text_in` selects by centre, so only the leading line is guaranteed.
    let first = hints.lines()[0].rect;
    let picked = hints.text_in(qs_doc::RectF::new(
        first.x,
        first.y,
        first.w,
        first.h,
    ));
    assert!(
        picked.starts_with(hints.lines()[0].text.trim()),
        "the first line's box must select its own text, got {picked:?}"
    );
}

#[test]
fn find_finds_real_text_in_the_capture() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/shots/settings-general.png"
    );
    let Ok(base) = image::open(path) else { return };
    let base = base.to_rgba8();
    let doc = Document::new(base.width(), base.height(), 1.0);

    let mut hints = OcrHints::default();
    hints.ensure(&doc, &base, "eng");
    let deadline = Instant::now() + Duration::from_secs(60);
    while !hints.poll() {
        assert!(Instant::now() < deadline, "OCR worker never reported back");
        std::thread::sleep(Duration::from_millis(25));
    }
    if hints.is_empty() {
        return;
    }

    let hits = hints.find("settings");
    assert!(!hits.is_empty(), "a settings window should mention settings");
    for i in &hits {
        assert!(
            hints.lines()[*i].text.to_lowercase().contains("settings"),
            "match {:?} does not contain the query",
            hints.lines()[*i].text
        );
    }

    assert!(hints.find("zzz-not-in-this-image").is_empty());
    assert!(hints.find("").is_empty(), "an empty query highlights nothing");
}
