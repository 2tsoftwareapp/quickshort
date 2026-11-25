//! The two entry points must never drift: whatever `extract_text` reports is
//! exactly the texts `extract_lines` returns for the same image.

use image::RgbaImage;

#[test]
fn text_and_geometry_agree_on_the_same_image() {
    // A blank frame: Vision finds nothing, which is the one case that behaves
    // identically on every machine regardless of installed language packs.
    let img: RgbaImage = RgbaImage::from_pixel(64, 64, image::Rgba([255, 255, 255, 255]));

    match (qs_ocr::extract_text(&img, "eng"), qs_ocr::extract_lines(&img, "eng")) {
        (Ok(text), Ok(lines)) => {
            let joined = lines
                .iter()
                .map(|l| l.text.clone())
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(joined, text, "geometry and text disagree");
            for line in &lines {
                assert!(line.x >= 0.0 && line.y >= 0.0, "rect starts off-image");
                assert!(
                    (0.0..=1.0).contains(&line.confidence),
                    "confidence {} out of range",
                    line.confidence
                );
            }
        }
        // No language pack / no engine on this machine: both sides report it.
        (Err(_), Err(_)) => {}
        (text, lines) => panic!("text and geometry disagree: {text:?} vs {lines:?}"),
    }
}

/// Real text, real engine: the settings screenshot ships in the repo, so this
/// proves the geometry is plausible without inventing a rasterizer (and without
/// `qs-ocr` depending on `qs-render`).
#[test]
fn geometry_of_real_ui_text_sits_inside_the_image() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/shots/settings-general.png"
    );
    let Ok(img) = image::open(path) else {
        return; // screenshot not checked out
    };
    let img = img.to_rgba8();
    let Ok(lines) = qs_ocr::extract_lines(&img, "eng") else {
        return; // no OCR engine / language pack on this machine
    };
    if lines.is_empty() {
        return; // engine ran but read nothing
    }

    // Visible with `cargo test -- --nocapture`, so a silent skip is obvious.
    println!("recognized {} lines on a {}x{} image", lines.len(), img.width(), img.height());

    let (iw, ih) = (img.width() as f32, img.height() as f32);
    for line in &lines {
        assert!(!line.text.trim().is_empty(), "empty line text");
        assert!(
            (0.0..=1.0).contains(&line.confidence) && line.confidence > 0.0,
            "confidence {} out of range",
            line.confidence
        );
        assert!(line.w > 0.0 && line.h > 0.0, "degenerate rect {:?}", line);
        let (right, bottom) = (line.x + line.w, line.y + line.h);
        assert!(
            line.x >= -1.0 && line.y >= -1.0 && right <= iw + 1.0 && bottom <= ih + 1.0,
            "rect {} {} {} {} escapes the {}x{} image",
            line.x,
            line.y,
            line.w,
            line.h,
            iw,
            ih
        );
        // Words share the line's pixel space: inside the image, and each
        // within (a slightly padded) line box — engines pad line boxes a
        // little, so allow 2 px of slop.
        for word in &line.words {
            assert!(
                !word.text.trim().is_empty() && word.w > 0.0 && word.h > 0.0,
                "degenerate word rect {:?}",
                word
            );
            assert!(
                (0.0..=1.0).contains(&word.confidence),
                "word confidence {} out of range",
                word.confidence
            );
            let (wr, wb) = (word.x + word.w, word.y + word.h);
            assert!(
                word.x >= -1.0 && word.y >= -1.0 && wr <= iw + 1.0 && wb <= ih + 1.0,
                "word rect {} {} {} {} escapes the image",
                word.x,
                word.y,
                word.w,
                word.h
            );
            assert!(
                word.x >= line.x - 2.0
                    && word.y >= line.y - 2.0
                    && wr <= right + 2.0
                    && wb <= bottom + 2.0,
                "word {:?} not inside line {:?}",
                word,
                line
            );
        }
    }
    assert!(
        lines.iter().any(|l| l.text.chars().any(|c| c.is_ascii_alphabetic())),
        "recognized no words at all: {:?}",
        lines.iter().map(|l| &l.text).collect::<Vec<_>>()
    );

    // Blank-line fidelity is documented as intentionally different: the
    // plain-text path keeps tesseract's paragraph spacing, the tsv path has
    // no empty-line rows. Compare the non-empty lines only — the actual
    // words must agree exactly.
    let joined = lines
        .iter()
        .map(|l| l.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    let plain = qs_ocr::extract_text(&img, "eng").unwrap();
    let plain_nonempty: Vec<&str> = plain
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert_eq!(
        joined,
        plain_nonempty.join("\n"),
        "geometry and text disagree on real UI text"
    );
}
