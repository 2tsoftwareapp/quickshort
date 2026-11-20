//! On-disk index: `<capture dir>/.qs-library.json`.
//!
//! The index is user-owned annotation state living next to user-owned files, so
//! the same discipline as the `.qsdoc` format applies: it carries a version,
//! reads older ones, and refuses to overwrite one written by a newer build.

use crate::entry::LibraryEntry;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const INDEX_VERSION: u32 = 1;
pub const INDEX_FILE: &str = ".qs-library.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryIndex {
    pub version: u32,
    pub entries: BTreeMap<String, LibraryEntry>,
}

impl Default for LibraryIndex {
    fn default() -> Self {
        Self {
            version: INDEX_VERSION,
            entries: BTreeMap::new(),
        }
    }
}

/// What the caller should do after asking for the index on disk.
#[derive(Debug, Clone)]
pub enum IndexState {
    /// Read successfully; annotations are usable and writable.
    Ready(LibraryIndex),
    /// Missing, unreadable, or corrupt: browsing still works from a directory
    /// scan, but annotations from the old file are gone. Rewriting is allowed.
    Rebuild { notice: String },
    /// Written by a newer build. The user's annotations exist but this build
    /// cannot interpret them, so it must not overwrite them.
    TooNew { found: u32, notice: String },
}

pub fn index_path(dir: &Path) -> PathBuf {
    dir.join(INDEX_FILE)
}

/// Read the index without modifying it.
pub fn load(dir: &Path) -> IndexState {
    let path = index_path(dir);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return IndexState::Rebuild {
                notice: String::new(),
            }
        }
        Err(e) => {
            return IndexState::Rebuild {
                notice: format!(
                "Library index could not be read ({}). Tags and favourites could not be recovered.",
                e
            ),
            }
        }
    };

    // Read the version before the entries: a newer layout must not be
    // interpreted field-by-field, only recognised and left alone.
    #[derive(Deserialize)]
    struct VersionOnly {
        #[serde(default)]
        version: u32,
    }
    let probe: VersionOnly = match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(e) => {
            return IndexState::Rebuild {
                notice: format!("Library index is corrupt ({}). It will be rebuilt; tags and favourites could not be recovered.", e),
            }
        }
    };
    if probe.version > INDEX_VERSION {
        return IndexState::TooNew {
            found: probe.version,
            notice: format!(
                "Library index was written by a newer quickshort (index v{}, this build reads v{INDEX_VERSION}). Tags and favourites are shown as unavailable and will not be overwritten.",
                probe.version
            ),
        };
    }

    match serde_json::from_str::<LibraryIndex>(&raw) {
        Ok(index) => IndexState::Ready(index),
        Err(e) => IndexState::Rebuild {
            notice: format!("Library index is corrupt ({}). It will be rebuilt; tags and favourites could not be recovered.", e),
        },
    }
}

/// Replace the index atomically: a crash mid-write leaves either the old file
/// or the new one, never a truncated one.
pub fn save(dir: &Path, index: &LibraryIndex) -> Result<()> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating capture folder {}", dir.display()))?;
    let final_path = index_path(dir);
    let tmp_path = dir.join(format!("{INDEX_FILE}.tmp"));
    let body = serde_json::to_vec_pretty(index).context("serialising library index")?;
    {
        let mut f = std::fs::File::create(&tmp_path)
            .with_context(|| format!("writing {}", tmp_path.display()))?;
        f.write_all(&body)
            .with_context(|| format!("writing {}", tmp_path.display()))?;
        f.sync_all().ok();
    }
    std::fs::rename(&tmp_path, &final_path)
        .with_context(|| format!("replacing {}", final_path.display()))?;
    Ok(())
}

/// Carry annotations forward onto a fresh scan.
///
/// A file whose size or mtime changed is no longer the file the metadata was
/// attached to, so its tags, favourite and provenance are dropped rather than
/// silently inherited by whatever now sits at that path.
pub fn merge(previous: Option<&LibraryIndex>, scanned: Vec<LibraryEntry>) -> LibraryIndex {
    let mut entries = BTreeMap::new();
    for mut entry in scanned {
        if let Some(old) = previous.and_then(|p| p.entries.get(&entry.file_name)) {
            if old.same_content(entry.size_bytes, entry.modified_unix) {
                entry.tags = old.tags.clone();
                entry.favorite = old.favorite;
                entry.mode = old.mode;
                entry.source_app = old.source_app.clone();
            }
        }
        entries.insert(entry.file_name.clone(), entry);
    }
    LibraryIndex {
        version: INDEX_VERSION,
        entries,
    }
}

/// The tags present across the given entries, de-duplicated case-insensitively
/// and sorted for a stable filter row.
pub fn all_tags<'a>(entries: impl IntoIterator<Item = &'a LibraryEntry>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for entry in entries {
        for tag in &entry.tags {
            if !out.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                out.push(tag.clone());
            }
        }
    }
    out.sort_by_key(|t| t.to_lowercase());
    out
}
