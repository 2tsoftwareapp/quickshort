//! Standalone annotation editor application window using egui / eframe.

pub mod app;
pub mod canvas;
pub mod clipboard;
pub mod fonts;
pub mod icons;
pub mod recovery;
pub mod theme;
pub mod toolbar;

pub use app::EditorApp;

use image::RgbaImage;
use qs_doc::Document;
use std::path::{Path, PathBuf};

/// Launch the standalone editor window.
pub fn run_editor(
    doc: Document,
    base: RgbaImage,
    doc_path: Option<PathBuf>,
) -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Quickshort Editor")
            .with_inner_size([1100.0, 750.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Quickshort Editor",
        native_options,
        Box::new(|cc| Ok(Box::new(EditorApp::new(doc, base, doc_path, cc)))),
    )
}

/// Open an existing file (.qsdoc or image) in the editor window.
pub fn open_file(path: impl AsRef<Path>) -> anyhow::Result<()> {
    let p = path.as_ref().to_path_buf();
    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(format!("Quickshort Editor — {}", p.display()))
            .with_inner_size([1100.0, 750.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Quickshort Editor",
        native_options,
        Box::new(move |cc| {
            let app = EditorApp::open_file(&p, cc)?;
            Ok(Box::new(app))
        }),
    ).map_err(|e| anyhow::anyhow!("Editor run error: {e:?}"))
}
