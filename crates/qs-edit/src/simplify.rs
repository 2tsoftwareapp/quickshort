//! Smart Simplify: detects high-contrast UI content rows / blocks in the
//! captured image and emits clean, editable rounded-rectangle vector blocks
//! (SUI-style) in a single undo step.

use image::RgbaImage;
use qs_doc::{Document, Fill, ObjectKind, RectF, Stroke};

/// Scan `base` inside `doc.view` for horizontal content bands separated by
/// uniform background rows, and return clean rounded-rectangle `ObjectKind`s.
pub fn detect_ui_blocks(doc: &Document, base: &RgbaImage) -> Vec<ObjectKind> {
    let (bw, bh) = base.dimensions();
    if bw < 16 || bh < 16 {
        return Vec::new();
    }

    let vx = (doc.view.x * doc.scale).round().max(0.0) as u32;
    let vy = (doc.view.y * doc.scale).round().max(0.0) as u32;
    let vw = (doc.view.w * doc.scale).round().max(1.0) as u32;
    let vh = (doc.view.h * doc.scale).round().max(1.0) as u32;
    let x0 = vx.min(bw.saturating_sub(1));
    let y0 = vy.min(bh.saturating_sub(1));
    let x1 = (vx + vw).min(bw);
    let y1 = (vy + vh).min(bh);
    if x1 <= x0 + 8 || y1 <= y0 + 8 {
        return Vec::new();
    }

    // Measure per-row luminance variance / edge density inside the view
    let mut row_active = vec![false; (y1 - y0) as usize];
    for (idx, y) in (y0..y1).enumerate() {
        let mut edges = 0u32;
        let mut prev_l = luma(base.get_pixel(x0, y).0);
        for x in (x0 + 1)..x1 {
            let cur_l = luma(base.get_pixel(x, y).0);
            if cur_l.abs_diff(prev_l) > 18 {
                edges += 1;
            }
            prev_l = cur_l;
        }
        // A row with >= 2% high-contrast horizontal transitions is likely UI text/controls
        row_active[idx] = edges * 50 >= (x1 - x0);
    }

    // Group contiguous active rows into vertical bands (allowing 2px gaps)
    let mut bands: Vec<(u32, u32)> = Vec::new();
    let mut cur_start: Option<u32> = None;
    let mut gap = 0u32;
    for (idx, &active) in row_active.iter().enumerate() {
        let y = y0 + idx as u32;
        if active {
            if cur_start.is_none() {
                cur_start = Some(y);
            }
            gap = 0;
        } else if let Some(s) = cur_start {
            gap += 1;
            if gap > 3 {
                let end = y.saturating_sub(gap);
                if end > s + 4 {
                    bands.push((s, end));
                }
                cur_start = None;
                gap = 0;
            }
        }
    }
    if let Some(s) = cur_start {
        if y1 > s + 4 {
            bands.push((s, y1));
        }
    }

    let mut out = Vec::new();
    let pad = 4.0 * doc.scale;
    for (by0, by1) in bands.into_iter().take(24) {
        // Find horizontal extent of active pixels in this band
        let mut min_x = x1;
        let mut max_x = x0;
        let mut r_sum = 0u64;
        let mut g_sum = 0u64;
        let mut b_sum = 0u64;
        let mut cnt = 0u64;

        for y in by0..by1 {
            let mut prev_l = luma(base.get_pixel(x0, y).0);
            for x in (x0 + 1)..x1 {
                let p = base.get_pixel(x, y).0;
                let cur_l = luma(p);
                if cur_l.abs_diff(prev_l) > 18 {
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                    r_sum += p[0] as u64;
                    g_sum += p[1] as u64;
                    b_sum += p[2] as u64;
                    cnt += 1;
                }
                prev_l = cur_l;
            }
        }
        if max_x <= min_x + 8 || cnt == 0 {
            continue;
        }

        let avg_r = (r_sum / cnt) as u32;
        let avg_g = (g_sum / cnt) as u32;
        let avg_b = (b_sum / cnt) as u32;
        let rgb = (avg_r << 16) | (avg_g << 8) | avg_b;

        let lx = ((min_x as f32 - pad).max(x0 as f32)) / doc.scale;
        let ly = ((by0 as f32 - pad * 0.5).max(y0 as f32)) / doc.scale;
        let rx = ((max_x as f32 + pad).min(x1 as f32)) / doc.scale;
        let ry = ((by1 as f32 + pad * 0.5).min(y1 as f32)) / doc.scale;

        out.push(ObjectKind::Rect {
            r: RectF::new(lx, ly, (rx - lx).max(4.0), (ry - ly).max(4.0)),
            stroke: Stroke { rgb, width: 0.0, alpha: 1.0 },
            fill: Fill::Solid { rgb, alpha: 1.0 },
            radius: 4.0,
        });
    }
    out
}

fn luma(p: [u8; 4]) -> u8 {
    ((299 * p[0] as u32 + 587 * p[1] as u32 + 114 * p[2] as u32) / 1000) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn detects_contrasting_ui_bands_as_vector_blocks() {
        let mut img = RgbaImage::from_pixel(120, 80, Rgba([245, 245, 245, 255]));
        // Draw two simulated UI text/button bands
        for y in 12..24 {
            for x in (16..80).step_by(3) {
                img.put_pixel(x, y, Rgba([40, 80, 160, 255]));
            }
        }
        for y in 44..58 {
            for x in (20..100).step_by(3) {
                img.put_pixel(x, y, Rgba([30, 30, 35, 255]));
            }
        }
        let doc = Document::new(120, 80, 1.0);
        let blocks = detect_ui_blocks(&doc, &img);
        assert_eq!(blocks.len(), 2);
    }
}
