//! Deletion: the operating system's trash is the only removal path.
//!
//! Nothing here falls back to `remove_file` — a library delete must stay
//! recoverable by the user's own file manager.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct TrashOutcome {
    pub path: PathBuf,
    /// `None` on success; a human-readable reason otherwise.
    pub error: Option<String>,
}

/// Move each path to the OS trash, reporting per path so one failure does not
/// hide the rest.
pub fn move_all(paths: &[PathBuf]) -> Vec<TrashOutcome> {
    paths
        .iter()
        .map(|path| TrashOutcome {
            path: path.clone(),
            error: trash::delete(path).err().map(|e| e.to_string()),
        })
        .collect()
}
