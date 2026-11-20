//! Capture library: an index of the files in the capture folder, the metadata
//! the user attaches to them, and the queries the Library view runs.
//!
//! Layering: this crate sits beside `qs-doc` (it reads documents to learn their
//! size) and below the UI crates. It has no `egui`/`eframe` dependency, so both
//! `qs-app` and `qs-editor` can use it without dragging in a windowing stack.

pub mod entry;
pub mod index;
pub mod query;
pub mod scan;
pub mod trash;

pub use entry::{CaptureMode, EntryKind, LibraryEntry, Provenance};
pub use index::{IndexState, LibraryIndex, INDEX_FILE, INDEX_VERSION};
pub use query::{format_date, DateFilter, FilterState, Sort};
pub use scan::{is_supported, kind_of};
pub use trash::TrashOutcome;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// The documented default capture folder (`~/Pictures/quickshort`), used when
/// the app config does not name one.
pub fn default_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| PathBuf::from(home).join("Pictures").join("quickshort"))
}

/// Record one just-written capture without rescanning the folder.
///
/// Used on the capture path, where a full directory walk would be wasted work.
/// An index written by a newer build is left alone rather than overwritten.
pub fn record_capture(dir: &Path, path: &Path, provenance: Provenance) -> Result<()> {
    let mut entry =
        scan::entry_for(path).with_context(|| format!("reading capture {}", path.display()))?;
    entry.mode = provenance.mode;
    entry.source_app = provenance.source_app;

    let mut idx = match index::load(dir) {
        IndexState::Ready(idx) => idx,
        IndexState::Rebuild { .. } => LibraryIndex::default(),
        IndexState::TooNew { found, .. } => anyhow::bail!(
            "library index v{found} was written by a newer quickshort; refusing to overwrite it"
        ),
    };
    idx.entries.insert(entry.file_name.clone(), entry);
    index::save(dir, &idx)
}
