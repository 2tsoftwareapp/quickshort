//! Cross-platform system clipboard helper (arboard).
//!
//! Works on Linux (X11 + Wayland), macOS and Windows.

use arboard::{Clipboard, ImageData};
use image::RgbaImage;
use std::borrow::Cow;
use std::sync::{Mutex, OnceLock};

/// One process-wide clipboard handle. On X11/Wayland arboard serves the
/// contents only while a `Clipboard` lives, so a per-copy handle dropped at
/// the end of the call left nothing to paste without a clipboard manager.
fn with_clipboard<T>(f: impl FnOnce(&mut Clipboard) -> anyhow::Result<T>) -> anyhow::Result<T> {
    static CB: OnceLock<Mutex<Option<Clipboard>>> = OnceLock::new();
    let mut guard = CB
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| anyhow::anyhow!("clipboard lock poisoned"))?;
    if guard.is_none() {
        *guard = Some(Clipboard::new()?);
    }
    f(guard.as_mut().expect("clipboard initialised above"))
}

#[cfg(target_os = "linux")]
fn wl_copy(mime: &str, data: &[u8]) -> bool {
    use std::io::Write;
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return false;
    }
    let mut c = match std::process::Command::new("wl-copy")
        .args(["-t", mime])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let ok = c
        .stdin
        .take()
        .map(|mut s| s.write_all(data).is_ok())
        .unwrap_or(false);
    ok && c.wait().map(|s| s.success()).unwrap_or(false)
}

pub fn copy_image(img: &RgbaImage) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let mut png = std::io::Cursor::new(Vec::new());
        if img.write_to(&mut png, image::ImageFormat::Png).is_ok()
            && wl_copy("image/png", &png.into_inner())
        {
            return Ok(());
        }
    }
    with_clipboard(|cb| {
        let data = ImageData {
            width: img.width() as usize,
            height: img.height() as usize,
            bytes: Cow::Borrowed(img.as_raw()),
        };
        cb.set_image(data)?;
        Ok(())
    })
}

pub fn copy_text(text: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    if wl_copy("text/plain;charset=utf-8", text.as_bytes()) {
        return Ok(());
    }
    with_clipboard(|cb| {
        cb.set_text(text.to_owned())?;
        Ok(())
    })
}

pub fn get_text() -> Option<String> {
    Clipboard::new().ok()?.get_text().ok()
}
