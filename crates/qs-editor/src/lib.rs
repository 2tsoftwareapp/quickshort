//! Standalone annotation editor application window using egui / eframe.

pub mod app;
pub mod canvas;
pub mod clipboard;
pub mod components;
pub mod filmstrip;
pub mod filmstrip_keys;
pub mod find;
pub mod fonts;
pub mod icons;
pub mod image_panel;
mod input;
pub mod library;
pub mod ocr_hint;
pub mod prefs;
pub mod recovery;
mod state;
pub mod styles;
pub mod templates;
pub mod theme;
pub mod toolbar;

pub use app::EditorApp;

use image::RgbaImage;
use qs_doc::Document;
use std::path::{Path, PathBuf};

fn editor_viewport(title: impl Into<String>) -> eframe::egui::ViewportBuilder {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon.png"))
        .expect("embedded Quickshort app icon must be a valid PNG");
    eframe::egui::ViewportBuilder::default()
        .with_title(title)
        .with_icon(icon)
        .with_inner_size([1180.0, 720.0])
        .with_min_inner_size([640.0, 480.0])
        // The docbar doubles as the title row beneath the macOS traffic lights.
        .with_fullsize_content_view(true)
        .with_title_shown(false)
        .with_titlebar_shown(false)
}

/// Launch the standalone editor window.
pub fn run_editor(
    doc: Document,
    base: RgbaImage,
    doc_path: Option<PathBuf>,
) -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: editor_viewport("Quickshort Editor"),
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
        viewport: editor_viewport(format!("Quickshort Editor — {}", p.display())),
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

#[cfg(test)]
mod tests {
    #[test]
    fn editor_viewport_uses_the_quickshort_app_icon() {
        let viewport = super::editor_viewport("Quickshort Editor");
        let icon = viewport.icon.unwrap();
        let source = image::load_from_memory(include_bytes!("../../../assets/icon.png"))
            .unwrap()
            .into_rgba8();
        assert_eq!((icon.width, icon.height), source.dimensions());
        assert_eq!(&icon.rgba, source.as_raw());
    }
}
