//! `.qsdoc` container: a zip holding
//!
//! ```text
//! manifest.json   {"format":"qsdoc","version":1,"app":"..."}
//! doc.json        Document (no pixels)
//! base.png        the untouched capture, physical px
//! thumb.png       <= 320 px long side, for library/shelf
//! ```
//!
//! Writes are atomic (temp file + rename). Reads are bounded so a hostile
//! file cannot exhaust memory: only the four known entries are read, each
//! with a size cap, and the base image dimensions are capped.

use crate::{Document, DOC_VERSION};
use anyhow::{bail, Context, Result};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const EXT: &str = "qsdoc";
const MAX_JSON: u64 = 32 << 20;
const MAX_PNG: u64 = 512 << 20;
const MAX_DIM: u32 = 16384;
pub const THUMB_MAX: u32 = 320;

#[derive(Serialize, Deserialize)]
struct Manifest {
    format: String,
    version: u32,
    #[serde(default)]
    app: String,
}

/// Fully loaded document + base pixels.
pub struct Loaded {
    pub doc: Document,
    pub base: RgbaImage,
}

/// `foo.png` -> `foo.qsdoc` (same dir, same stem).
pub fn sibling_path(img: &Path) -> PathBuf {
    img.with_extension(EXT)
}

pub fn is_qsdoc(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(EXT))
}

fn encode_png(img: &RgbaImage) -> Result<Vec<u8>> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;
    let mut out = Vec::new();
    PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Adaptive)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )?;
    Ok(out)
}

pub fn make_thumb(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let long = w.max(h).max(1);
    if long <= THUMB_MAX {
        return img.clone();
    }
    let k = THUMB_MAX as f32 / long as f32;
    let tw = ((w as f32 * k).round() as u32).max(1);
    let th = ((h as f32 * k).round() as u32).max(1);
    image::imageops::resize(img, tw, th, image::imageops::FilterType::Triangle)
}

/// Write atomically. `thumb` is usually the rendered (annotated) image's
/// thumbnail; pass None to thumbnail the bare base.
pub fn write(path: &Path, doc: &Document, base: &RgbaImage, thumb: Option<&RgbaImage>) -> Result<()> {
    doc.validate().context("refusing to save an invalid document")?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = path.with_extension(format!("{EXT}.tmp"));
    let res = (|| -> Result<()> {
        let f = std::fs::File::create(&tmp).with_context(|| format!("create {}", tmp.display()))?;
        let mut z = zip::ZipWriter::new(f);
        let deflate = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        // PNG is already compressed
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .large_file(true);
        let manifest = Manifest {
            format: "qsdoc".into(),
            version: DOC_VERSION,
            app: format!("quickshort {}", env!("CARGO_PKG_VERSION")),
        };
        z.start_file("manifest.json", deflate)?;
        z.write_all(serde_json::to_string(&manifest)?.as_bytes())?;
        z.start_file("doc.json", deflate)?;
        z.write_all(doc.to_json()?.as_bytes())?;
        z.start_file("base.png", stored)?;
        z.write_all(&encode_png(base)?)?;
        let t = match thumb {
            Some(t) => make_thumb(t),
            None => make_thumb(base),
        };
        z.start_file("thumb.png", stored)?;
        z.write_all(&encode_png(&t)?)?;
        let f = z.finish()?;
        f.sync_all()?;
        Ok(())
    })();
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, path).with_context(|| format!("rename to {}", path.display()))?;
    Ok(())
}

fn read_entry<R: Read + std::io::Seek>(
    z: &mut zip::ZipArchive<R>,
    name: &str,
    cap: u64,
) -> Result<Vec<u8>> {
    let e = z.by_name(name).with_context(|| format!("missing {name}"))?;
    if e.size() > cap {
        bail!("{name} too large ({} bytes)", e.size());
    }
    let mut buf = Vec::with_capacity(e.size().min(cap) as usize);
    // `take` guards against a lying header
    e.take(cap + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > cap {
        bail!("{name} exceeds {cap} bytes");
    }
    Ok(buf)
}

fn decode_png(bytes: &[u8]) -> Result<RgbaImage> {
    let mut r = image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut lim = image::Limits::default();
    lim.max_image_width = Some(MAX_DIM);
    lim.max_image_height = Some(MAX_DIM);
    lim.max_alloc = Some(MAX_PNG);
    r.limits(lim);
    Ok(r.decode()?.to_rgba8())
}

fn open(path: &Path) -> Result<zip::ZipArchive<std::fs::File>> {
    let f = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let z = zip::ZipArchive::new(f).context("not a qsdoc (zip) file")?;
    Ok(z)
}

pub fn read(path: &Path) -> Result<Loaded> {
    let mut z = open(path)?;
    let m: Manifest = serde_json::from_slice(&read_entry(&mut z, "manifest.json", 1 << 16)?)
        .context("bad manifest.json")?;
    if m.format != "qsdoc" {
        bail!("unknown format {:?}", m.format);
    }
    if m.version > DOC_VERSION {
        bail!(
            "file was written by a newer quickshort (format v{}, this build reads v{DOC_VERSION})",
            m.version
        );
    }
    let json = String::from_utf8(read_entry(&mut z, "doc.json", MAX_JSON)?)?;
    let doc = Document::from_json(&json).context("bad doc.json")?;
    let base = decode_png(&read_entry(&mut z, "base.png", MAX_PNG)?).context("bad base.png")?;
    let exp_w = (doc.base_size.w * doc.scale).round() as i64;
    let exp_h = (doc.base_size.h * doc.scale).round() as i64;
    if (exp_w - base.width() as i64).abs() > 1 || (exp_h - base.height() as i64).abs() > 1 {
        bail!(
            "base.png is {}x{} but doc expects {exp_w}x{exp_h}",
            base.width(),
            base.height()
        );
    }
    Ok(Loaded { doc, base })
}

/// Thumbnail only (cheap, for library grids).
pub fn read_thumb(path: &Path) -> Result<RgbaImage> {
    let mut z = open(path)?;
    decode_png(&read_entry(&mut z, "thumb.png", 16 << 20)?)
}

/// Document only, no pixels (library indexing).
pub fn read_doc(path: &Path) -> Result<Document> {
    let mut z = open(path)?;
    let json = String::from_utf8(read_entry(&mut z, "doc.json", MAX_JSON)?)?;
    Document::from_json(&json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ObjectKind, PointF, Stroke};

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("qsdoc-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample() -> (Document, RgbaImage) {
        let base = RgbaImage::from_fn(64, 40, |x, y| image::Rgba([x as u8 * 3, y as u8 * 5, 90, 255]));
        let mut d = Document::new(64, 40, 2.0);
        d.push(ObjectKind::Arrow {
            a: PointF::new(1.0, 1.0),
            b: PointF::new(20.0, 10.0),
            stroke: Stroke::new(0xFF0000, 2.0),
            head: Default::default(),
        });
        (d, base)
    }

    #[test]
    fn round_trip_byte_exact_pixels() {
        let dir = tmpdir("rt");
        let p = dir.join("a.qsdoc");
        let (d, base) = sample();
        write(&p, &d, &base, None).unwrap();
        let l = read(&p).unwrap();
        assert_eq!(l.doc, d);
        assert_eq!(l.base.as_raw(), base.as_raw());
        assert!(read_thumb(&p).unwrap().width() <= THUMB_MAX);
        assert_eq!(read_doc(&p).unwrap(), d);
        assert!(!p.with_extension("qsdoc.tmp").exists());
    }

    #[test]
    fn overwrite_keeps_file_valid() {
        let dir = tmpdir("ow");
        let p = dir.join("a.qsdoc");
        let (mut d, base) = sample();
        write(&p, &d, &base, None).unwrap();
        d.meta.title = "second".into();
        write(&p, &d, &base, None).unwrap();
        assert_eq!(read(&p).unwrap().doc.meta.title, "second");
    }

    #[test]
    fn rejects_garbage_and_mismatched_base() {
        let dir = tmpdir("bad");
        let p = dir.join("junk.qsdoc");
        std::fs::write(&p, b"not a zip").unwrap();
        assert!(read(&p).is_err());

        let (d, _) = sample();
        let wrong = RgbaImage::new(10, 10);
        let p2 = dir.join("mismatch.qsdoc");
        write(&p2, &d, &wrong, None).unwrap();
        let e = read(&p2).err().unwrap().to_string();
        assert!(e.contains("expects"), "{e}");
    }

    #[test]
    fn rejects_newer_format() {
        let dir = tmpdir("new");
        let p = dir.join("n.qsdoc");
        let f = std::fs::File::create(&p).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let o = zip::write::SimpleFileOptions::default();
        z.start_file("manifest.json", o).unwrap();
        z.write_all(br#"{"format":"qsdoc","version":99}"#).unwrap();
        z.finish().unwrap();
        let e = read(&p).err().unwrap().to_string();
        assert!(e.contains("newer"), "{e}");
    }

    #[test]
    fn sibling_and_ext() {
        assert_eq!(
            sibling_path(Path::new("/x/quickshort-1.png")),
            PathBuf::from("/x/quickshort-1.qsdoc")
        );
        assert!(is_qsdoc(Path::new("a.QSDOC")));
        assert!(!is_qsdoc(Path::new("a.png")));
    }
}
