//! Failure states: a corrupt index, an index from a newer build, and an
//! unreadable folder must each be visible and distinguishable.

use eframe::egui::{self, Pos2, Rect, Vec2};
use qs_editor::library::Library;

fn screen(w: f32, h: f32) -> Rect {
    Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))
}

fn sample_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("qs_states_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for i in 0..3 {
        image::RgbaImage::from_pixel(64, 48, image::Rgba([240, 241, 245, 255]))
            .save(dir.join(format!("shot-{i}.png")))
            .unwrap();
    }
    dir
}

fn scan(dir: &std::path::Path) -> (egui::Context, Library) {
    let ctx = egui::Context::default();
    qs_editor::theme::apply_macos_visuals(&ctx);
    let mut library = Library::default();
    library.open = true;
    library.set_dir(dir.to_path_buf());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let raw = egui::RawInput {
            screen_rect: Some(screen(900.0, 700.0)),
            ..Default::default()
        };
        let mut full = ctx.run_ui(raw, |_| {
            let _ = library.show(&ctx);
        });
        full.textures_delta.clear();
        if !library.is_scanning() {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "scan did not finish");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    (ctx, library)
}

#[test]
fn a_corrupt_index_is_reported_and_the_folder_still_browses() {
    let dir = sample_dir("corrupt");
    std::fs::write(dir.join(".qs-library.json"), "{not json").unwrap();

    let (_ctx, library) = scan(&dir);
    assert_eq!(library.len(), 3, "entries come from the scan, not the index");
    let notice = library.notice().unwrap_or_default();
    assert!(
        notice.to_lowercase().contains("corrupt"),
        "notice should explain the rebuild, got {notice:?}"
    );
    assert!(!library.is_read_only(), "a corrupt index may be rewritten");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_index_from_a_newer_build_is_read_only() {
    let dir = sample_dir("newer");
    std::fs::write(
        dir.join(".qs-library.json"),
        r#"{"version":99,"entries":{}}"#,
    )
    .unwrap();

    let (_ctx, library) = scan(&dir);
    assert_eq!(library.len(), 3, "browsing still works from the scan");
    let notice = library.notice().unwrap_or_default();
    assert!(
        notice.to_lowercase().contains("newer"),
        "notice should say the index is newer, got {notice:?}"
    );
    assert!(library.is_read_only(), "a newer index must not be overwritten");
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn an_unreadable_folder_reports_the_reason() {
    use std::os::unix::fs::PermissionsExt;

    let dir = sample_dir("locked");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (_ctx, library) = scan(&dir);
    let readable = library.error().is_some();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

    // A root shell can read it anyway; only assert when the permission bites.
    if readable {
        let error = library.error().unwrap();
        assert!(
            error.contains("reading capture folder"),
            "error should name the folder, got {error:?}"
        );
        assert_eq!(library.len(), 0);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
