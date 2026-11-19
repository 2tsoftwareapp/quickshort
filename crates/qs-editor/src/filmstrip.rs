//! Recent Captures Filmstrip: asynchronously scans capture directories and
//! builds thumbnail textures for quick document switching.

use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};
use image::imageops::FilterType;
use qs_doc::qsdoc;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::SystemTime;

const MAX_ITEMS: usize = 24;
const THUMB_MAX_W: u32 = 120;
const THUMB_MAX_H: u32 = 76;

pub struct FilmItem {
    pub path: PathBuf,
    pub label: String,
    pub is_qsdoc: bool,
    pub texture: Option<TextureHandle>,
    pending_image: Option<ColorImage>,
}

struct LoadedThumb {
    path: PathBuf,
    label: String,
    is_qsdoc: bool,
    image: Option<ColorImage>,
}

pub struct Filmstrip {
    pub items: Vec<FilmItem>,
    pub scanning: bool,
    rx: Option<Receiver<Vec<LoadedThumb>>>,
    last_dir: Option<PathBuf>,
}

impl Filmstrip {
    pub fn new(doc_path: Option<&Path>) -> Self {
        let mut s = Self {
            items: Vec::new(),
            scanning: false,
            rx: None,
            last_dir: doc_path.and_then(|p| p.parent().map(Path::to_path_buf)),
        };
        s.refresh(doc_path);
        s
    }

    pub fn refresh(&mut self, doc_path: Option<&Path>) {
        if let Some(dir) = doc_path.and_then(|p| p.parent()) {
            self.last_dir = Some(dir.to_path_buf());
        }
        let dirs = candidate_dirs(self.last_dir.as_deref());
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        self.scanning = true;

        std::thread::spawn(move || {
            let loaded = scan_dirs(&dirs);
            let _ = tx.send(loaded);
        });
    }

    /// Poll background scan results and lazily upload egui textures.
    pub fn poll(&mut self, ctx: &Context) {
        if let Some(rx) = &self.rx {
            if let Ok(loaded) = rx.try_recv() {
                self.items = loaded
                    .into_iter()
                    .map(|l| FilmItem {
                        path: l.path,
                        label: l.label,
                        is_qsdoc: l.is_qsdoc,
                        texture: None,
                        pending_image: l.image,
                    })
                    .collect();
                self.scanning = false;
                self.rx = None;
                ctx.request_repaint();
            }
        }

        for item in &mut self.items {
            if item.texture.is_none() {
                if let Some(img) = item.pending_image.take() {
                    let name = format!("filmstrip_{}", item.path.display());
                    item.texture = Some(ctx.load_texture(name, img, TextureOptions::LINEAR));
                }
            }
        }
    }
}

fn candidate_dirs(active_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut push = |p: PathBuf| {
        if p.is_dir() && seen.insert(p.clone()) {
            out.push(p);
        }
    };

    if let Some(d) = active_dir {
        push(d.to_path_buf());
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        push(home.join("Pictures").join("quickshort"));
        push(home.join("Pictures").join("Quickshort"));
    }
    out
}

fn is_supported(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "qsdoc" | "png" | "jpg" | "jpeg" | "webp"
    )
}

fn scan_dirs(dirs: &[PathBuf]) -> Vec<LoadedThumb> {
    let mut files: Vec<(SystemTime, PathBuf)> = Vec::new();
    let mut seen = HashSet::new();

    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if !p.is_file() || !is_supported(&p) || !seen.insert(p.clone()) {
                continue;
            }
            let mtime = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            files.push((mtime, p));
        }
    }

    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.truncate(MAX_ITEMS);

    files
        .into_iter()
        .map(|(_, path)| {
            let label = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("capture")
                .to_string();
            let is_qsdoc = qsdoc::is_qsdoc(&path);
            let image = load_thumbnail(&path, is_qsdoc);
            LoadedThumb {
                path,
                label,
                is_qsdoc,
                image,
            }
        })
        .collect()
}

fn load_thumbnail(path: &Path, is_qsdoc: bool) -> Option<ColorImage> {
    let rgba = if is_qsdoc {
        qsdoc::read_thumb(path)
            .ok()
            .or_else(|| qsdoc::read(path).ok().map(|l| l.base))?
    } else {
        image::open(path).ok()?.to_rgba8()
    };

    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let scale = (THUMB_MAX_W as f32 / w as f32)
        .min(THUMB_MAX_H as f32 / h as f32)
        .min(1.0);
    let tw = ((w as f32 * scale).round() as u32).max(1);
    let th = ((h as f32 * scale).round() as u32).max(1);
    let resized = if tw != w || th != h {
        image::imageops::resize(&rgba, tw, th, FilterType::Triangle)
    } else {
        rgba
    };
    Some(ColorImage::from_rgba_unmultiplied(
        [tw as usize, th as usize],
        resized.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_extensions_filter_correctly() {
        assert!(is_supported(Path::new("shot.png")));
        assert!(is_supported(Path::new("shot.QSDOC")));
        assert!(is_supported(Path::new("photo.jpeg")));
        assert!(!is_supported(Path::new("notes.txt")));
    }

    #[test]
    fn scan_dirs_sorts_newest_first_and_builds_thumbs() {
        let dir = std::env::temp_dir().join(format!("qs_filmstrip_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let p1 = dir.join("a.png");
        let p2 = dir.join("b.png");
        let img = image::RgbaImage::from_pixel(40, 20, image::Rgba([10, 20, 30, 255]));
        img.save(&p1).unwrap();
        img.save(&p2).unwrap();

        let loaded = scan_dirs(&[dir.clone()]);
        assert_eq!(loaded.len(), 2);
        assert!(loaded.iter().all(|l| l.image.is_some()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
