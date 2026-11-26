//! Standalone Quickshort Editor binary.
//!
//! Usage:
//!     qs-editor [path/to/file.qsdoc | path/to/image.png]
//!
//! If no file is provided, an empty canvas is opened.

use image::RgbaImage;
use qs_doc::Document;
use std::env;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

enum DocLock {
    Acquired(std::fs::File),
    AlreadyOpen,
    Failed,
}

fn document_lock(path: &Path) -> DocLock {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    let hash = hasher.finish();
    let dir = std::env::temp_dir().join("quickshort");
    if std::fs::create_dir_all(&dir).is_err() {
        return DocLock::Failed;
    }
    let lock_path = dir.join(format!("editor_{:016x}.lock", hash));
    let Ok(f) = std::fs::File::create(lock_path) else {
        return DocLock::Failed;
    };
    match f.try_lock() {
        Ok(()) => DocLock::Acquired(f),
        Err(std::fs::TryLockError::WouldBlock) => DocLock::AlreadyOpen,
        Err(_) => DocLock::Failed,
    }
}

fn main() -> anyhow::Result<()> {
    qs_core::init_log();
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        let path = PathBuf::from(&args[1]);
        if path.exists() {
            let lock = document_lock(&path);
            if matches!(lock, DocLock::AlreadyOpen) {
                log::warn!("qs-editor: document is already open: {}", path.display());
                return Ok(());
            }
            let _guard = match lock {
                DocLock::Acquired(f) => Some(f),
                _ => None,
            };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_lock_prevents_duplicate_editors_on_same_file() {
        let temp_file = std::env::temp_dir().join(format!("qs_lock_test_{}.png", std::process::id()));
        std::fs::write(&temp_file, b"test").unwrap();
        let guard1 = match document_lock(&temp_file) {
            DocLock::Acquired(f) => f,
            _ => panic!("first lock must be acquired"),
        };
        assert!(matches!(document_lock(&temp_file), DocLock::AlreadyOpen));
        drop(guard1);
        assert!(matches!(document_lock(&temp_file), DocLock::Acquired(_)));
        let _ = std::fs::remove_file(&temp_file);
    }
}
