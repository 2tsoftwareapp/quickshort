//! Multi-image Combine Templates: arranges 2+ captures into a clean,
//! numbered step-by-step guide with editable Step annotations.

use image::{imageops, DynamicImage, Rgba, RgbaImage};
use qs_doc::{qsdoc, Document, ObjectKind, PointF, PALETTE};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CombineLayout {
    #[default]
    SideBySide,
    BeforeAfter,
    VerticalSteps,
    Grid2x2,
}

impl CombineLayout {
    pub fn label(self) -> &'static str {
        match self {
            Self::SideBySide => "Side-by-side",
            Self::BeforeAfter => "Before / After comparison",
            Self::VerticalSteps => "Vertical steps",
            Self::Grid2x2 => "2×2 grid",
        }
    }
}

fn load_flattened(path: &Path) -> anyhow::Result<RgbaImage> {
    if qsdoc::is_qsdoc(path) {
        let loaded = qsdoc::read(path)?;
        Ok(qs_render::render(&loaded.doc, &loaded.base))
    } else {
        Ok(image::open(path)?.to_rgba8())
    }
}

/// Build a combined document from `paths` using `layout`, placing an editable
/// numbered `Step` badge at the top-left of each image panel.
pub fn build_combined_document(
    paths: &[PathBuf],
    layout: CombineLayout,
    backdrop_argb: u32,
) -> anyhow::Result<(Document, RgbaImage)> {
    anyhow::ensure!(paths.len() >= 2, "select at least 2 images to combine");

    let mut imgs = Vec::with_capacity(paths.len());
    for p in paths {
        imgs.push(load_flattened(p)?);
    }

    let pad: u32 = 28;
    let gap: u32 = 20;
    let max_cell_w: u32 = 960;

    // Normalize oversized images so the combined sheet stays crisp and manageable
    let imgs: Vec<RgbaImage> = imgs
        .into_iter()
        .map(|im| {
            let (w, h) = im.dimensions();
            if w > max_cell_w {
                let nh = ((h as f32) * (max_cell_w as f32 / w as f32)).round().max(1.0) as u32;
                DynamicImage::ImageRgba8(im)
                    .resize_exact(max_cell_w, nh, imageops::FilterType::Lanczos3)
                    .to_rgba8()
            } else {
                im
            }
        })
        .collect();

    let mut placements: Vec<(u32, u32)> = Vec::with_capacity(imgs.len());
    let (total_w, total_h) = match layout {
        CombineLayout::SideBySide | CombineLayout::BeforeAfter => {
            let mut x = pad;
            let mut max_h = 0u32;
            for im in &imgs {
                placements.push((x, pad));
                x += im.width() + gap;
                max_h = max_h.max(im.height());
            }
            (x - gap + pad, max_h + pad * 2)
        }
        CombineLayout::VerticalSteps => {
            let mut y = pad;
            let mut max_w = 0u32;
            for im in &imgs {
                placements.push((pad, y));
                y += im.height() + gap;
                max_w = max_w.max(im.width());
            }
            (max_w + pad * 2, y - gap + pad)
        }
        CombineLayout::Grid2x2 => {
            let cols = 2usize;
            let rows = imgs.len().div_ceil(cols);
            let mut col_w = [0u32; 2];
            let mut row_h = vec![0u32; rows];
            for (i, im) in imgs.iter().enumerate() {
                col_w[i % cols] = col_w[i % cols].max(im.width());
                row_h[i / cols] = row_h[i / cols].max(im.height());
            }
            let mut y = pad;
            for (r, &rh) in row_h.iter().enumerate() {
                let mut x = pad;
                for (c, &cw) in col_w.iter().enumerate().take(cols) {
                    let idx = r * cols + c;
                    if idx < imgs.len() {
                        placements.push((x, y));
                    }
                    x += cw + gap;
                }
                y += rh + gap;
            }
            (col_w[0] + col_w[1] + gap + pad * 2, y - gap + pad)
        }
    };

    let bg = Rgba([
        ((backdrop_argb >> 16) & 0xff) as u8,
        ((backdrop_argb >> 8) & 0xff) as u8,
        (backdrop_argb & 0xff) as u8,
        255,
    ]);
    let mut canvas = RgbaImage::from_pixel(total_w.max(1), total_h.max(1), bg);
    for (im, &(px, py)) in imgs.iter().zip(placements.iter()) {
        imageops::overlay(&mut canvas, im, px as i64, py as i64);
    }

    let mut doc = Document::new(canvas.width(), canvas.height(), 1.0);
    doc.background = backdrop_argb;
    let step_rgb = PALETTE[4]; // accent blue
    let text_rgb = PALETTE[6]; // white
    if layout == CombineLayout::BeforeAfter {
        for (i, &(px, py)) in placements.iter().enumerate() {
            let (tag, fill_rgb) = match i {
                0 => ("BEFORE", 0x52525B), // allow-hex: document data, baked into the export
                1 => ("AFTER", 0x16A34A),  // allow-hex: document data, baked into the export
                _ => ("STEP", step_rgb),
            };
            doc.push(ObjectKind::Text {
                pos: PointF::new(px as f32 + 14.0, py as f32 + 12.0),
                text: tag.to_owned(),
                style: qs_doc::TextStyle {
                    px: 13.0,
                    bold: true,
                    rgb: text_rgb,
                    ..Default::default()
                },
                wrap_w: None,
                fill: qs_doc::Fill::Solid {
                    rgb: fill_rgb,
                    alpha: 0.94,
                },
            });
        }
    } else {
        for &(px, py) in &placements {
            doc.push(ObjectKind::Step {
                center: PointF::new(px as f32 + 18.0, py as f32 + 18.0),
                radius: 13.0,
                rgb: step_rgb,
                text_rgb,
            });
        }
    }

    Ok((doc, canvas))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_layouts_produce_expected_dimensions_and_steps() {
        let dir = std::env::temp_dir().join(format!("qs_tpl_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let p1 = dir.join("1.png");
        let p2 = dir.join("2.png");
        RgbaImage::from_pixel(100, 60, Rgba([255, 0, 0, 255])).save(&p1).unwrap();
        RgbaImage::from_pixel(120, 80, Rgba([0, 255, 0, 255])).save(&p2).unwrap();

        let paths = [p1, p2];
        let (doc_h, img_h) = build_combined_document(&paths, CombineLayout::SideBySide, 0xFF1E_1E24).unwrap();
        assert_eq!(img_h.dimensions(), (28 + 100 + 20 + 120 + 28, 28 + 80 + 28));
        assert_eq!(doc_h.objects.len(), 2);

        let (doc_v, img_v) = build_combined_document(&paths, CombineLayout::VerticalSteps, 0xFF1E_1E24).unwrap();
        assert_eq!(img_v.dimensions(), (28 + 120 + 28, 28 + 60 + 20 + 80 + 28));
        assert_eq!(doc_v.objects.len(), 2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
