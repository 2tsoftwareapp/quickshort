//! Shared access to the Quickshort settings file across processes.

use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
};
use toml_edit::DocumentMut;

pub fn path() -> PathBuf {
    dirs::config_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("quickshort")
        .join("config.toml")
}

pub fn read<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    toml_edit::de::from_str(&fs::read_to_string(path)?).map_err(io::Error::other)
}

/// Merge the supplied fields into the latest file, preserving other owners' keys.
pub fn update(path: &Path, fields: &impl Serialize) -> io::Result<()> {
    let fields = document(fields)?;
    edit(path, |config| {
        for (key, value) in fields.iter() {
            config[key] = value.clone();
        }
    })
}

/// Write only the fields changed by an event, even if the caller's snapshot
/// predates a write from another process.
pub fn save_changes<T: Serialize>(path: &Path, before: &T, after: &T) -> io::Result<()> {
    let before = document(before)?;
    let after = document(after)?;
    edit(path, |config| {
        for (key, value) in after.iter() {
            if before
                .get(key)
                .is_none_or(|old| old.to_string() != value.to_string())
            {
                config[key] = value.clone();
            }
        }
        for (key, _) in before.iter() {
            if !after.contains_key(key) {
                config.remove(key);
            }
        }
    })
}

fn document(value: &impl Serialize) -> io::Result<DocumentMut> {
    toml_edit::ser::to_document(value).map_err(io::Error::other)
}

fn edit(path: &Path, update: impl FnOnce(&mut DocumentMut)) -> io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    // Lock a sibling file because atomic replacement changes config.toml's inode.
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path.with_extension("toml.lock"))?;
    lock.lock()?;

    let existing = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let mut config = existing.parse::<DocumentMut>().map_err(io::Error::other)?;
    update(&mut config);
    let updated = config.to_string();
    if existing == updated {
        return Ok(());
    }

    let tmp = path.with_extension("toml.tmp");
    let result = fs::write(&tmp, updated).and_then(|()| fs::rename(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn stale_settings_writes_preserve_newer_theme_and_unowned_fields() {
        let dir = std::env::temp_dir().join(format!("qs-config-merge-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(
            &path,
            "# user settings\ntheme = \"light\"\naccent = \"purple\"\n\n[[profiles]]\nname = \"Default\"\n",
        )
        .unwrap();

        let before = BTreeMap::from([("theme", "light"), ("accent", "purple")]);
        update(&path, &BTreeMap::from([("theme", "dark")])).unwrap();
        let changed = BTreeMap::from([("theme", "light"), ("accent", "green")]);
        save_changes(&path, &before, &changed).unwrap();

        let watermark = "Team \"QA\"\\lab\n";
        update(&path, &BTreeMap::from([("watermark_text", watermark)])).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        let config: DocumentMut = raw.parse().unwrap();
        assert_eq!(config["theme"].as_str(), Some("dark"));
        assert_eq!(config["accent"].as_str(), Some("green"));
        assert_eq!(config["watermark_text"].as_str(), Some(watermark));
        assert_eq!(
            config["profiles"]
                .as_array_of_tables()
                .unwrap()
                .get(0)
                .unwrap()["name"]
                .as_str(),
            Some("Default")
        );
        assert!(raw.contains("# user settings"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn update_leaves_invalid_config_in_place() {
        let dir = std::env::temp_dir().join(format!("qs-config-invalid-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let invalid = "theme = [";
        fs::write(&path, invalid).unwrap();

        assert!(update(&path, &BTreeMap::from([("theme", "dark")])).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), invalid);
        fs::remove_dir_all(dir).unwrap();
    }
}
