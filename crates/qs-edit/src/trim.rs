//! Detect uniform borders without changing the source image.
use image::RgbaImage;
use qs_doc::{Document, RectF};

/// Find the content bounds inside the current view. Pixel data stays external
/// to the vector document. Reject ambiguous corners and trims over 60%.
pub fn calculate_trim_rect(doc: &Document, base: &RgbaImage, tolerance: u8) -> Option<RectF> {
    let s = doc.scale;
    if !s.is_finite() || s <= 0.0 || base.width() == 0 || base.height() == 0 { return None; }
    let v = doc.view.normalized();
    let x0 = (v.x * s).max(0.0).ceil() as u32;
    let y0 = (v.y * s).max(0.0).ceil() as u32;
    let x1 = (v.right() * s).max(0.0).floor().min(base.width() as f32) as u32;
    let y1 = (v.bottom() * s).max(0.0).floor().min(base.height() as f32) as u32;
    if x0 >= x1 || y0 >= y1 { return None; }
    let sample = base.get_pixel(x0, y0);
    let similar = |p: &image::Rgba<u8>| {
        if p[3] == 0 && sample[3] == 0 { return true; }
        p.0.iter().zip(sample.0.iter()).all(|(a,b)| a.abs_diff(*b) <= tolerance)
    };
    if ![(x1-1,y0),(x0,y1-1),(x1-1,y1-1)].iter().all(|&(x,y)| similar(base.get_pixel(x,y))) { return None; }
    let (mut left, mut top, mut right, mut bottom) = (x1, y1, x0, y0);
    for y in y0..y1 { for x in x0..x1 {
        if !similar(base.get_pixel(x, y)) {
            left = left.min(x); top = top.min(y); right = right.max(x+1); bottom = bottom.max(y+1);
        }
    } }
    if left >= right || top >= bottom { return None; }
    let area = (right-left) as f64 * (bottom-top) as f64;
    let original = (x1-x0) as f64 * (y1-y0) as f64;
    if area < original * 0.4 { return None; }
    let r = RectF::new(left as f32/s, top as f32/s, (right-left) as f32/s, (bottom-top) as f32/s);
    if r == doc.view { None } else { Some(r) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn trims_white_border_with_tolerance_and_scale() {
        let doc = Document::new(100, 100, 2.0);
        let mut base = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        for y in 10..90 { for x in 10..90 { base.put_pixel(x, y, Rgba([50, 80, 120, 255])); } }
        base.put_pixel(2, 2, Rgba([250, 250, 250, 255]));
        assert_eq!(calculate_trim_rect(&doc, &base, 6), Some(RectF::new(5.0, 5.0, 40.0, 40.0)));
    }

    #[test]
    fn uniform_or_excessive_trim_is_rejected() {
        let doc = Document::new(100, 100, 1.0);
        let mut base = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        assert_eq!(calculate_trim_rect(&doc, &base, 6), None);
        base.put_pixel(50, 50, Rgba([0, 0, 0, 255]));
        assert_eq!(calculate_trim_rect(&doc, &base, 6), None);
    }

    #[test]
    fn transparent_rgb_is_ignored_and_existing_crop_is_respected() {
        let mut doc = Document::new(100, 100, 1.0);
        let mut base = RgbaImage::from_pixel(100, 100, Rgba([123, 56, 78, 0]));
        for y in 10..90 { for x in 10..90 { base.put_pixel(x, y, Rgba([0, 0, 0, 255])); } }
        assert_eq!(calculate_trim_rect(&doc, &base, 6), Some(RectF::new(10.0, 10.0, 80.0, 80.0)));
        doc.view = RectF::new(20.0, 20.0, 60.0, 60.0);
        assert_eq!(calculate_trim_rect(&doc, &base, 6), None);
    }
}
