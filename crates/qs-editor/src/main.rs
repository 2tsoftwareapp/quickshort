//! Standalone Quickshort Editor binary.
//!
//! Usage:
//!     qs-editor [path/to/file.qsdoc | path/to/image.png]
//!
//! If no file is provided, an empty canvas is opened.

use image::RgbaImage;
use qs_doc::Document;
use std::env;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        let path = PathBuf::from(&args[1]);
        if path.exists() {
            qs_editor::open_file(&path)?;
            return Ok(());
        }
    }

    // Default: blank 800x600 canvas
    let w = 800;
    let h = 600;
    let doc = Document::new(w, h, 1.0);
    let base = RgbaImage::from_pixel(w, h, image::Rgba([240, 240, 240, 255]));
    qs_editor::run_editor(doc, base, None).map_err(|e| anyhow::anyhow!("Run error: {e:?}"))
}
