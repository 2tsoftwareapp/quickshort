//! One capture file in the library, plus the metadata the user attaches to it.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Image,
    Document,
}

impl EntryKind {
    pub fn label(self) -> &'static str {
        match self {
            EntryKind::Image => "Image",
            EntryKind::Document => "Document",
        }
    }
}

/// How the capture was produced.
///
/// `Unknown` covers files that predate provenance recording and flows that
/// cannot report a mode — it is never guessed from a file name.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaptureMode {
    Region,
    Fullscreen,
    Scrolling,
    Recording,
    #[default]
    Unknown,
}

impl CaptureMode {
    pub fn label(self) -> &'static str {
        match self {
            CaptureMode::Region => "Region",
            CaptureMode::Fullscreen => "Fullscreen",
            CaptureMode::Scrolling => "Scrolling",
            CaptureMode::Recording => "Recording",
            CaptureMode::Unknown => "Unknown",
        }
    }
}

/// What produced a capture. Assembled on the thread that owns the capture
/// trigger — AppKit cannot be asked from a worker — and carried unchanged into
/// the index write.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Provenance {
    pub mode: CaptureMode,
    /// Frontmost application name, when the platform reports one.
    pub source_app: Option<String>,
}

/// One file in the capture folder.
///
/// `file_name` is the identity: it is the key in the index and the only thing
/// tags are attached to, so a renamed file is a new entry rather than an entry
/// inheriting another image's metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub file_name: String,
    pub kind: EntryKind,
    pub size_bytes: u64,
    pub modified_unix: i64,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub mode: CaptureMode,
    #[serde(default)]
    pub source_app: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
}

impl LibraryEntry {
    pub fn path(&self, dir: &Path) -> PathBuf {
        dir.join(&self.file_name)
    }

    /// Filesystem facts that decide whether the bytes are still the ones the
    /// metadata was attached to.
    pub fn same_content(&self, size_bytes: u64, modified_unix: i64) -> bool {
        self.size_bytes == size_bytes && self.modified_unix == modified_unix
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
    }

    /// Add a tag, preserving the casing the user typed first. Tags are unique
    /// case-insensitively; empty/whitespace-only tags are rejected.
    pub fn add_tag(&mut self, tag: &str) -> bool {
        let trimmed = tag.trim();
        if trimmed.is_empty() || self.has_tag(trimmed) {
            return false;
        }
        self.tags.push(trimmed.to_string());
        true
    }

    pub fn remove_tag(&mut self, tag: &str) -> bool {
        let before = self.tags.len();
        self.tags.retain(|t| !t.eq_ignore_ascii_case(tag));
        self.tags.len() != before
    }

    /// Search matches the file name and the tags, case-insensitively.
    pub fn matches(&self, needle: &str) -> bool {
        if needle.is_empty() {
            return true;
        }
        let needle = needle.to_lowercase();
        self.file_name.to_lowercase().contains(&needle)
            || self.tags.iter().any(|t| t.to_lowercase().contains(&needle))
    }
}
