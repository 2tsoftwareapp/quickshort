use image::{Rgba, RgbaImage};
use qs_doc::{
    Anchor, ColorFilter, Cutout, CutoutAxis, CutoutSeam, Document, PageCurlEffect,
    ReflectionEffect,
};
use qs_render::{output_size, render};

#[test]
fn cutout_horizontal_splices_pixels_and_shrinks_output() {
    let base = RgbaImage::from_fn(40, 40, |_, y| {
        if y < 10 {
            Rgba([255, 0, 0, 255])
        } else if y < 20 {
            Rgba([0, 255, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        }
    });
    let mut doc = Document::new(40, 40, 1.0);
    doc.cutouts.push(Cutout {
        axis: CutoutAxis::Horizontal,
        start: 10.0,
        end: 20.0,
        seam: CutoutSeam::Clean,
    });
    assert_eq!(output_size(&doc), (40, 30));

    let img = render(&doc, &base);
    assert_eq!(img.dimensions(), (40, 30));
    assert_eq!(*img.get_pixel(5, 5), Rgba([255, 0, 0, 255]));
    assert_eq!(*img.get_pixel(5, 25), Rgba([0, 0, 255, 255]));
}

#[test]
fn color_filter_grayscale_and_invert_preserve_alpha() {
    let base = RgbaImage::from_pixel(10, 10, Rgba([100, 150, 200, 210]));
    let mut doc = Document::new(10, 10, 1.0);
    doc.background = 0;
    doc.effects.color_filter = Some(ColorFilter::Grayscale);
    let gray = render(&doc, &base);
    let p = gray.get_pixel(5, 5);
    assert_eq!(p[0], p[1]);
    assert_eq!(p[1], p[2]);
    assert_eq!(p[3], 210);

    doc.effects.color_filter = Some(ColorFilter::Invert);
    let inv = render(&doc, &base);
    let ip = inv.get_pixel(5, 5);
    assert_eq!(ip[3], 210);
    assert!(ip[0].abs_diff(155) <= 1 && ip[1].abs_diff(105) <= 1 && ip[2].abs_diff(55) <= 1);
}

#[test]
fn page_curl_cuts_corner_and_shades_fold() {
    let base = RgbaImage::from_pixel(64, 64, Rgba([40, 120, 200, 255]));
    let mut doc = Document::new(64, 64, 1.0);
    doc.effects.page_curl = Some(PageCurlEffect {
        corner: Anchor::BottomRight,
        size: 24.0,
        shadow: 0.5,
    });
    let out = render(&doc, &base);
    assert_eq!(out.get_pixel(63, 63)[3], 0);
    assert_eq!(*out.get_pixel(10, 10), Rgba([40, 120, 200, 255]));
}

#[test]
fn reflection_extends_height_and_fades_below_image() {
    let base = RgbaImage::from_pixel(30, 20, Rgba([200, 80, 40, 255]));
    let mut doc = Document::new(30, 20, 1.0);
    doc.effects.reflection = Some(ReflectionEffect {
        height: 10.0,
        opacity: 0.5,
        gap: 2.0,
    });
    assert_eq!(output_size(&doc), (30, 32));
    let out = render(&doc, &base);
    assert_eq!(out.dimensions(), (30, 32));
    // Gap row is transparent
    assert_eq!(out.get_pixel(15, 20)[3], 0);
    // Reflection starts at y=22 and fades toward y=31
    assert!(out.get_pixel(15, 22)[3] > out.get_pixel(15, 31)[3]);
    assert!(out.get_pixel(15, 22)[3] > 0);
}
