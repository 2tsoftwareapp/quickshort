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
