//! Directory scan: turn a capture folder into fresh entries.
//!
//! This is synchronous on purpose — callers run it on a worker thread. Nothing
//! here touches a UI frame (Constitution: no blocking I/O on an overlay frame).

use crate::entry::{EntryKind, LibraryEntry};
use anyhow::{Context, Result};
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Extensions the library understands. Everything else in the folder is
/// ignored — including `.DS_Store`, partial downloads, and the index itself.
pub fn is_supported(path: &Path) -> bool {
    kind_of(path).is_some()
}

pub fn kind_of(path: &Path) -> Option<EntryKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "qsdoc" => Some(EntryKind::Document),
        "png" | "jpg" | "jpeg" | "webp" => Some(EntryKind::Image),
        _ => None,
    }
}

/// Scan `dir` for supported files.
///
/// A missing folder is not an error — it means the user has not captured
/// anything yet, and the caller shows an empty state. A folder that exists but
/// cannot be read *is* an error, so the UI can report the reason instead of
/// implying the library is empty.
pub fn read_dir(dir: &Path) -> Result<Vec<LibraryEntry>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let read = std::fs::read_dir(dir)
        .with_context(|| format!("reading capture folder {}", dir.display()))?;

    let mut out = Vec::new();
    for item in read {
        // One vanished or unreadable file must not hide the rest of the folder.
        let Ok(item) = item else { continue };
        if let Some(entry) = entry_for(&item.path()) {
            out.push(entry);
        }
    }
    Ok(out)
}

/// Read one supported file into an entry, or `None` when it is unsupported,
/// not a regular file, or vanished between listing and stat.
pub fn entry_for(path: &Path) -> Option<LibraryEntry> {
    let kind = kind_of(path)?;
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let file_name = path.file_name()?.to_str()?.to_string();
    let modified_unix = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (width, height) = dimensions(path, kind);
    Some(LibraryEntry {
        file_name,
        kind,
        size_bytes: meta.len(),
        modified_unix,
        width,
        height,
        mode: crate::entry::CaptureMode::Unknown,
        source_app: None,
        tags: Vec::new(),
        favorite: false,
    })
}

/// Pixel size, best effort: header-only for images, manifest for documents.
/// Unknown dimensions stay 0 and the UI simply omits them.
fn dimensions(path: &Path, kind: EntryKind) -> (u32, u32) {
    match kind {
        EntryKind::Image => image::image_dimensions(path).unwrap_or((0, 0)),
        EntryKind::Document => qs_doc::qsdoc::read_doc(path)
            .map(|doc| {
                let w = (doc.base_size.w * doc.scale).round();
                let h = (doc.base_size.h * doc.scale).round();
                (w.max(0.0) as u32, h.max(0.0) as u32)
            })
            .unwrap_or((0, 0)),
    }
}
