//! Cross-platform system clipboard helper (arboard).
//!
//! Works on Linux (X11 + Wayland), macOS and Windows.

use arboard::{Clipboard, ImageData};
use image::RgbaImage;
use std::borrow::Cow;

pub fn copy_image(img: &RgbaImage) -> anyhow::Result<()> {
    let mut cb = Clipboard::new()?;
    let data = ImageData {
        width: img.width() as usize,
        height: img.height() as usize,
        bytes: Cow::Borrowed(img.as_raw()),
    };
    cb.set_image(data)?;
    Ok(())
}

pub fn copy_text(text: &str) -> anyhow::Result<()> {
    let mut cb = Clipboard::new()?;
    cb.set_text(text.to_owned())?;
    Ok(())
}

pub fn get_text() -> Option<String> {
    Clipboard::new().ok()?.get_text().ok()
}
