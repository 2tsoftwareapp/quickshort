//! Colorize / colour replacement: only the targeted colour moves, blended by
//! strength, and the setting survives a document round-trip.

use image::{Rgba, RgbaImage};
use qs_doc::{Document, Effects};
use qs_render::render;

fn doc_with(effects: Effects, w: u32) -> Document {
    let mut d = Document::new(w, 1, 1.0);
    d.background = 0xFFFFFFFF;
    d.effects = effects;
    d
}

fn colorize(target: u32, replacement: u32, tolerance: u8, strength: f32) -> Effects {
    Effects {
        colorize: Some(qs_doc::ColorizeEffect {
            target,
            replacement,
            tolerance,
            strength,
        }),
        ..Effects::default()
    }
}

/// A 4x1 strip: target blue, a near-blue, an unrelated colour, and the
/// replacement colour itself.
fn strip() -> RgbaImage {
    let colours = [
        [0, 120, 215, 255], // target
        [0, 130, 220, 255], // within tolerance
        [200, 60, 40, 255], // unrelated
        [34, 197, 94, 255], // already the replacement
    ];
    RgbaImage::from_fn(4, 1, |x, _| Rgba(colours[x as usize]))
}

fn px(d: &Document, out: &RgbaImage, x: u32) -> [u8; 4] {
    let _ = d;
    out.get_pixel(x, 0).0
}

#[test]
fn only_the_targeted_colour_changes() {
    // (0,130,220) is 10 away from the target on one channel, so this needs a
    // tolerance above 10 to be treated as the same colour.
    let d = doc_with(colorize(0x0078D7, 0x22C55E, 12, 1.0), 4);
    let out = render(&d, &strip());
    assert_eq!(px(&d, &out, 0), [34, 197, 94, 255], "target replaced");
    assert_eq!(
        px(&d, &out, 1),
        [34, 197, 94, 255],
        "near colour inside tolerance"
    );
    assert_eq!(
        px(&d, &out, 2),
        [200, 60, 40, 255],
        "unrelated colour untouched"
    );
}

#[test]
fn tolerance_zero_matches_only_the_exact_colour() {
    let d = doc_with(colorize(0x0078D7, 0x22C55E, 0, 1.0), 4);
    let out = render(&d, &strip());
    assert_eq!(px(&d, &out, 0), [34, 197, 94, 255]);
    assert_eq!(
        px(&d, &out, 1),
        [0, 130, 220, 255],
        "a colour 10 away must not match at tolerance 0"
    );
}

#[test]
fn strength_blends_rather_than_swapping() {
    let d = doc_with(colorize(0x0078D7, 0x22C55E, 8, 0.5), 4);
    let out = render(&d, &strip());
    // Halfway between (0,120,215) and (34,197,94).
    assert_eq!(px(&d, &out, 0), [17, 159, 155, 255]);
}

#[test]
fn disabled_or_self_replacement_is_the_original() {
    let base = strip();
    for effects in [
        Effects::default(),
        colorize(0x0078D7, 0x0078D7, 32, 1.0),
        colorize(0x0078D7, 0x22C55E, 8, 0.0),
    ] {
        let d = doc_with(effects, 4);
        assert_eq!(render(&d, &base).as_raw(), base.as_raw());
    }
}

#[test]
fn colorize_survives_a_document_round_trip() {
    let d = doc_with(colorize(0x0078D7, 0x22C55E, 12, 0.75), 4);
    let back = Document::from_json(&serde_json::to_string(&d).unwrap()).unwrap();
    assert_eq!(back.effects.colorize, d.effects.colorize);
    assert_eq!(
        render(&back, &strip()).as_raw(),
        render(&d, &strip()).as_raw()
    );
}

#[test]
fn documents_written_before_the_field_exist_still_load() {
    let mut j = serde_json::to_value(Document::new(4, 4, 1.0)).unwrap();
    j["effects"] = serde_json::json!({ "color_filter": "grayscale" });
    let d = Document::from_json(&j.to_string()).unwrap();
    assert!(
        d.effects.colorize.is_none(),
        "absent colourise stays absent"
    );
}
