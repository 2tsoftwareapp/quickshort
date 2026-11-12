//! Crash-recovery copies of the document being edited.
//!
//! A recovery copy lives in the user's state directory (`$XDG_STATE_HOME`)
//! and only exists while the document holds edits that are not on disk yet.
//! It is rewritten during editing and removed as soon as the document is
//! saved or the edits are discarded, so a leftover copy after a restart means
//! the previous session died before it could reach the real file.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Minimum delay between two autosave writes.
pub const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(20);

/// Recovery copies older than this are deleted on startup.
pub const ORPHAN_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);

const UNTITLED_PREFIX: &str = "untitled-";

/// `$XDG_STATE_HOME/quickshort/editor`, the directory holding recovery copies.
pub fn state_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state"))
        })?;
    Some(base.join("quickshort").join("editor"))
}

/// Recovery-copy path for `doc_path` inside `dir`.
///
/// A saved document is keyed by the hash of its absolute path, so the same
/// file reopened always finds the same slot. A document that was never saved
/// has no stable name and gets a process-scoped slot instead; those are the
/// ones [`orphans_in`] reports after a crash.
pub fn slot_in(dir: &Path, doc_path: Option<&Path>) -> PathBuf {
    match doc_path {
        Some(p) => {
            let abs = fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
            let mut h = DefaultHasher::new();
            abs.to_string_lossy().hash(&mut h);
            dir.join(format!("{:016x}.qsdoc", h.finish()))
        }
        None => dir.join(format!("{UNTITLED_PREFIX}{}.qsdoc", std::process::id())),
    }
}

/// [`slot_in`] for the real state directory, `None` when the OS gives us no
/// place to write.
pub fn slot_path(doc_path: Option<&Path>) -> Option<PathBuf> {
    state_dir().map(|d| slot_in(&d, doc_path))
}

/// Removes the recovery copy; a missing file is not an error.
pub fn clear(slot: &Path) {
    let _ = fs::remove_file(slot);
    let _ = fs::remove_file(slot.with_extension("tmp"));
}

fn modified(p: &Path) -> Option<SystemTime> {
    fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// True when the recovery copy holds edits the saved file does not have.
///
/// An unsaved document has no file to compare against, so any copy counts.
pub fn is_newer(slot: &Path, doc: Option<&Path>) -> bool {
    let Some(copy_m) = modified(slot) else {
        return false;
    };
    match doc.and_then(modified) {
        Some(doc_m) => copy_m > doc_m,
        None => true,
    }
}

/// Recovery copies of never-saved documents left behind by earlier sessions.
pub fn orphans_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|e| e.to_str()) == Some("qsdoc")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(UNTITLED_PREFIX))
        })
        .collect()
}

/// Deletes orphaned recovery copies older than `max_age`; returns how many
/// went away. Nothing older than that is worth recovering, and leaving them
/// behind grows the state directory forever.
pub fn prune_orphans(max_age: Duration) -> usize {
    let Some(dir) = state_dir() else {
        return 0;
    };
    let now = SystemTime::now();
    orphans_in(&dir)
        .into_iter()
        .filter(|p| {
            now.duration_since(modified(p).unwrap_or(now))
                .map(|age| age > max_age)
                .unwrap_or(false)
        })
        .inspect(|p| clear(p))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qs-recovery-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn same_document_always_maps_to_the_same_slot() {
        let dir = scratch("stable");
        let doc = dir.join("capture.qsdoc");
        fs::write(&doc, b"x").unwrap();
        assert_eq!(slot_in(&dir, Some(&doc)), slot_in(&dir, Some(&doc)));
        // a relative and an absolute spelling of the same file must agree
        let cwd = std::env::current_dir().unwrap();
        if let Ok(rel) = doc.strip_prefix(&cwd) {
            assert_eq!(slot_in(&dir, Some(&doc)), slot_in(&dir, Some(rel)));
        }
    }

    #[test]
    fn different_documents_get_different_slots() {
        let dir = scratch("distinct");
        let a = dir.join("a.qsdoc");
        let b = dir.join("b.qsdoc");
        assert_ne!(slot_in(&dir, Some(&a)), slot_in(&dir, Some(&b)));
    }

    #[test]
    fn untitled_slots_are_per_process() {
        let dir = scratch("untitled");
        let slot = slot_in(&dir, None);
        let name = slot.file_name().unwrap().to_str().unwrap().to_string();
        assert!(name.starts_with(UNTITLED_PREFIX));
        assert!(name.contains(&std::process::id().to_string()));
    }

    #[test]
    fn copy_without_a_saved_file_counts_as_newer() {
        let dir = scratch("newer-none");
        let slot = dir.join("s.qsdoc");
        fs::write(&slot, b"x").unwrap();
        assert!(is_newer(&slot, None));
    }

    #[test]
    fn copy_is_only_newer_when_it_beat_the_file() {
        let dir = scratch("newer-cmp");
        let slot = dir.join("s.qsdoc");
        let doc = dir.join("d.qsdoc");
        fs::write(&doc, b"x").unwrap();
        std::thread::sleep(Duration::from_millis(20));
        fs::write(&slot, b"y").unwrap();
        assert!(is_newer(&slot, Some(&doc)));
        std::thread::sleep(Duration::from_millis(20));
        fs::write(&doc, b"z").unwrap();
        assert!(!is_newer(&slot, Some(&doc)));
    }

    #[test]
    fn missing_copy_is_never_recovered() {
        let dir = scratch("missing");
        assert!(!is_newer(&dir.join("nope.qsdoc"), None));
    }

    #[test]
    fn orphans_list_only_untitled_copies() {
        let dir = scratch("orphans");
        fs::write(slot_in(&dir, None), b"x").unwrap();
        fs::write(dir.join("0123456789abcdef.qsdoc"), b"y").unwrap();
        fs::write(dir.join("notes.txt"), b"z").unwrap();
        let found = orphans_in(&dir);
        assert_eq!(found.len(), 1);
        assert!(found[0]
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with(UNTITLED_PREFIX));
    }

    #[test]
    fn clear_is_idempotent() {
        let dir = scratch("clear");
        let slot = dir.join("s.qsdoc");
        fs::write(&slot, b"x").unwrap();
        clear(&slot);
        clear(&slot);
        assert!(!slot.exists());
    }
}
