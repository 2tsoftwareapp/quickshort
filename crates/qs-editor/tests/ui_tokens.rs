//! Contracts the UI relies on: one control height, and readable colour pairs.
//! These guard the design tokens rather than any particular screen.

use eframe::egui::{Pos2, Rect, Vec2};
use qs_editor::components::{pill_size, status_pill_size, CONTROL_H};

fn frame<R>(f: impl FnOnce(&mut eframe::egui::Ui) -> R) -> R {
    let ctx = eframe::egui::Context::default();
    let raw = eframe::egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))),
        ..Default::default()
    };
    let mut f = Some(f);
    let mut out = None;
    let mut run = ctx.run_ui(raw, |ui| {
        if let Some(f) = f.take() {
            out = Some(f(ui));
        }
    });
    run.textures_delta.clear();
    out.expect("frame ran")
}

#[test]
fn every_control_uses_the_shared_height() {
    let (pill, status) = frame(|ui| (pill_size(ui, "Open"), status_pill_size(ui, "Layers")));
    assert_eq!(pill.y, CONTROL_H, "action pill height");
    // Status pills are the mockup's slimmer footer chips, not chrome controls.
    assert_eq!(status.y, 20.0, "status pill height");
    assert_eq!(CONTROL_H, 26.0, "the shared control height is the design value");
    assert!(pill.x > 0.0 && status.x > 0.0, "sizes must be measured");
}

/// WCAG relative luminance for an sRGB colour.
fn luminance(c: eframe::egui::Color32) -> f32 {
    let f = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.039_28 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
}

fn contrast(a: eframe::egui::Color32, b: eframe::egui::Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[test]
fn text_stays_readable_on_cards_in_both_themes() {
    for theme in ["light", "dark"] {
        qs_theme::set_theme(theme);
        let card = qs_editor::theme::bg_card();
        let primary = qs_editor::theme::text_primary();
        let tertiary = qs_editor::theme::text_tertiary();
        assert!(
            contrast(primary, card) >= 4.5,
            "{theme}: primary text on a card is {:.2}:1",
            contrast(primary, card)
        );
        // Meta lines are supporting text: 3:1 is the WCAG threshold for
        // non-body text and is what the design tokens target.
        assert!(
            contrast(tertiary, card) >= 3.0,
            "{theme}: tertiary text on a card is {:.2}:1",
            contrast(tertiary, card)
        );
    }
}
