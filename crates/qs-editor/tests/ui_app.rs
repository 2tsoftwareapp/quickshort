//! App-level dialog chords, driven without an eframe frame.
//!
//! `EditorApp::handle_dialog_keys` is what `update` calls first, so pressing
//! keys here is the same path the window takes.

use eframe::egui::{self, Event, Key, Modifiers, Pos2, Rect, Vec2};
use qs_doc::Document;
use qs_editor::EditorApp;

fn screen(w: f32, h: f32) -> Rect {
    Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))
}

fn app() -> EditorApp {
    let base = image::RgbaImage::from_pixel(64, 48, image::Rgba([240, 241, 245, 255]));
    EditorApp::for_tests(Document::new(64, 48, 1.0), base)
}

/// One frame with a chord: modifiers change first, then the key.
fn press(app: &mut EditorApp, ctx: &egui::Context, key: Key, modifiers: Modifiers) {
    let raw = egui::RawInput {
        screen_rect: Some(screen(900.0, 700.0)),
        events: vec![
            Event::ModifiersChanged(modifiers),
            Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
        ],
        ..Default::default()
    };
    let mut full = ctx.run_ui(raw, |_| {
        app.handle_dialog_keys(ctx);
    });
    full.textures_delta.clear();
}

#[test]
fn chords_open_the_expected_dialog_and_close_the_previous_one() {
    let mut app = app();
    let ctx = egui::Context::default();
    qs_editor::theme::apply_macos_visuals(&ctx);
    assert_eq!(app.open_dialogs(), (false, false, false, false, false));

    press(&mut app, &ctx, Key::C, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(
        app.open_dialogs(),
        (true, false, false, false, false),
        "cmd+shift+C opens Canvas"
    );

    press(&mut app, &ctx, Key::E, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(
        app.open_dialogs(),
        (false, true, false, false, false),
        "Export replaces Canvas instead of stacking"
    );

    // Option chords reach Flatten; cmd+shift+F is swallowed by macOS.
    press(&mut app, &ctx, Key::F, Modifiers::COMMAND | Modifiers::ALT);
    assert_eq!(app.open_dialogs(), (false, false, true, false, false));

    press(&mut app, &ctx, Key::A, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(app.open_dialogs(), (false, false, false, true, false));

    press(&mut app, &ctx, Key::Comma, Modifiers::COMMAND);
    assert_eq!(app.open_dialogs(), (false, false, false, false, true));
}

#[test]
fn escape_closes_the_top_dialog() {
    let mut app = app();
    let ctx = egui::Context::default();
    qs_editor::theme::apply_macos_visuals(&ctx);

    press(&mut app, &ctx, Key::C, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(app.open_dialogs().0);

    press(&mut app, &ctx, Key::Escape, Modifiers::NONE);
    assert_eq!(
        app.open_dialogs(),
        (false, false, false, false, false),
        "escape dismisses Canvas"
    );
}

#[test]
fn cmd_shift_t_toggles_the_filmstrip() {
    let mut app = app();
    let ctx = egui::Context::default();
    qs_editor::theme::apply_macos_visuals(&ctx);
    assert!(!app.filmstrip_visible());

    press(&mut app, &ctx, Key::T, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(app.filmstrip_visible(), "tray opens");

    press(&mut app, &ctx, Key::T, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(!app.filmstrip_visible(), "tray closes again");
}

#[test]
fn combine_builds_a_sheet_from_two_captures() {
    let dir = std::env::temp_dir().join(format!("qs_combine_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut paths = Vec::new();
    for i in 0..2u32 {
        let p = dir.join(format!("shot-{i}.png"));
        image::RgbaImage::from_pixel(80 + i, 60, image::Rgba([250, 251, 253, 255]))
            .save(&p)
            .unwrap();
        paths.push(p);
    }

    let mut app = app();
    let before = app.document().objects.len();
    app.set_tray_selection(paths.clone());
    app.create_combined_sheet().expect("combine should succeed");

    let doc = app.document();
    assert!(
        doc.objects.len() > before,
        "the sheet carries the captures as objects"
    );
    assert!(
        doc.objects
            .iter()
            .any(|o| matches!(o.kind, qs_doc::ObjectKind::Step { .. })),
        "numbered step badges are added"
    );
    assert!(
        doc.base_size.w > 80.0,
        "the canvas is the combined width, not one capture"
    );
    assert!(app.has_unsaved_edits(), "the new sheet is unsaved");

    let _ = std::fs::remove_dir_all(&dir);
}
