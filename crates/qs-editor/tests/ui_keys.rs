//! Keyboard interaction, driven through real `RawInput` events.
//!
//! The responsive UAT only renders frames; these tests press keys the way the
//! window would and assert what the UI did with them.

use eframe::egui::{self, Event, Key, Modifiers, Pos2, Rect, Vec2};

fn screen(w: f32, h: f32) -> Rect {
    Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))
}

fn key_event(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn sample_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("qs_keys_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..3 {
        image::RgbaImage::from_pixel(64, 48, image::Rgba([240, 241, 245, 255]))
            .save(dir.join(format!("shot-{i}.png")))
            .unwrap();
    }
    dir
}

/// Run one frame of the library, optionally with events, and return its action.
fn frame(
    ctx: &egui::Context,
    library: &mut qs_editor::library::Library,
    w: f32,
    h: f32,
    events: Vec<Event>,
) -> Option<qs_editor::library::LibraryAction> {
    let raw = egui::RawInput {
        screen_rect: Some(screen(w, h)),
        events,
        ..Default::default()
    };
    let mut action = None;
    let mut full = ctx.run_ui(raw, |_| {
        action = library.show(ctx);
    });
    full.textures_delta.clear();
    action
}

fn scanned(dir: &std::path::Path) -> (egui::Context, qs_editor::library::Library) {
    let ctx = egui::Context::default();
    qs_editor::theme::apply_macos_visuals(&ctx);
    let mut library = qs_editor::library::Library::default();
    library.open = true;
    library.set_dir(dir.to_path_buf());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let action = frame(&ctx, &mut library, 900.0, 700.0, Vec::new());
        assert!(action.is_none(), "a passive frame must not act");
        if !library.is_scanning() && library.len() == 3 {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "scan did not finish");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    (ctx, library)
}

#[test]
fn arrows_select_and_enter_opens() {
    let dir = sample_dir("nav");
    let (ctx, mut library) = scanned(&dir);
    assert!(library.selected_names().is_empty(), "nothing selected yet");

    frame(&ctx, &mut library, 900.0, 700.0, vec![key_event(Key::ArrowDown)]);
    let first = library.selected_names();
    assert_eq!(first.len(), 1, "down selects exactly one row");
    assert!(first[0].ends_with(".png"));

    frame(&ctx, &mut library, 900.0, 700.0, vec![key_event(Key::ArrowDown)]);
    let second = library.selected_names();
    assert_eq!(second.len(), 1);
    assert_ne!(second[0], first[0], "a second down moves the selection");

    frame(&ctx, &mut library, 900.0, 700.0, vec![key_event(Key::ArrowUp)]);
    assert_eq!(library.selected_names(), first, "up returns to the first row");

    let action = frame(&ctx, &mut library, 900.0, 700.0, vec![key_event(Key::Enter)]);
    match action {
        Some(qs_editor::library::LibraryAction::Open(path)) => {
            assert_eq!(Some(path.as_path()), dir.join(&first[0]).as_path().into())
        }
        other => panic!("expected Open for the selected row, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn escape_closes_the_library() {
    let dir = sample_dir("esc");
    let (ctx, mut library) = scanned(&dir);
    assert!(library.open);
    frame(
        &ctx,
        &mut library,
        900.0,
        700.0,
        vec![
            Event::ModifiersChanged(Modifiers::NONE),
            key_event(Key::Escape),
        ],
    );
    assert!(!library.open, "escape closes the view");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn typing_in_find_reaches_the_field() {
    let ctx = egui::Context::default();
    qs_editor::theme::apply_macos_visuals(&ctx);
    let mut state = qs_editor::find::FindState {
        open: true,
        query: String::new(),
        index: 0,
        focus_pending: true,
    };

    for events in [
        vec![Event::Text("ee".into())],
        vec![Event::Text("e".into())],
    ] {
        let raw = egui::RawInput {
            screen_rect: Some(screen(720.0, 600.0)),
            events,
            ..Default::default()
        };
        let mut full = ctx.run_ui(raw, |_| {
            let _ = qs_editor::find::render(&ctx, &mut state, 0, false);
        });
        full.textures_delta.clear();
    }
    assert!(
        !state.query.is_empty(),
        "the find field must take the keyboard without a click"
    );
    assert!(!state.focus_pending, "focus is requested once");
}

#[test]
fn command_backspace_opens_the_delete_confirmation_and_escape_cancels() {
    let dir = sample_dir("del");
    let (ctx, mut library) = scanned(&dir);

    frame(&ctx, &mut library, 900.0, 700.0, vec![key_event(Key::ArrowDown)]);
    let selected = library.selected_names();
    assert_eq!(selected.len(), 1);

    let cmd_backspace = Event::Key {
        key: Key::Backspace,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    };
    // `InputState::modifiers` only changes through ModifiersChanged, so the
    // chord needs both events in the same frame, in this order.
    frame(
        &ctx,
        &mut library,
        900.0,
        700.0,
        vec![
            Event::ModifiersChanged(Modifiers::COMMAND),
            cmd_backspace,
        ],
    );
    assert!(
        library.is_confirming_delete(),
        "cmd+backspace must ask before deleting"
    );

    frame(&ctx, &mut library, 900.0, 700.0, vec![key_event(Key::Escape)]);
    assert!(!library.is_confirming_delete(), "escape cancels the dialog");
    assert!(
        dir.join(&selected[0]).exists(),
        "cancelling keeps the file on disk"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Remove the test file from the OS trash so a test run leaves no litter.
fn clean_trash(name: &str) {
    let Ok(home) = std::env::var("HOME") else { return };
    let trash = std::path::Path::new(&home).join(".Trash");
    let Ok(entries) = std::fs::read_dir(&trash) else { return };
    for entry in entries.flatten() {
        let file = entry.file_name();
        let file = file.to_string_lossy();
        // The OS may suffix duplicates ("name 2.png").
        if file == name || file.starts_with(&format!("{name} ")) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[test]
fn confirming_the_delete_moves_the_file_to_the_trash() {
    let dir = sample_dir("trash");
    let (ctx, mut library) = scanned(&dir);

    frame(&ctx, &mut library, 900.0, 700.0, vec![key_event(Key::ArrowDown)]);
    let selected = library.selected_names();
    assert_eq!(selected.len(), 1);
    let victim = dir.join(&selected[0]);
    assert!(victim.exists());

    frame(
        &ctx,
        &mut library,
        900.0,
        700.0,
        vec![
            Event::ModifiersChanged(Modifiers::COMMAND),
            Event::Key {
                key: Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            },
        ],
    );
    assert!(library.is_confirming_delete());

    // Enter confirms; the row disappears and the file leaves the folder.
    frame(
        &ctx,
        &mut library,
        900.0,
        700.0,
        vec![Event::ModifiersChanged(Modifiers::NONE), key_event(Key::Enter)],
    );
    assert!(!library.is_confirming_delete(), "dialog closes after deleting");
    assert!(
        !victim.exists(),
        "the confirmed file must leave the capture folder"
    );
    assert_eq!(library.len(), 2, "the row is gone from the list");

    clean_trash(&selected[0]);
    let _ = std::fs::remove_dir_all(&dir);
}
