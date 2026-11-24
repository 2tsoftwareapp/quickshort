//! Icon geometry shared by every surface: vendored Lucide SVGs (ISC —
//! `icons/LICENSE`) flattened at build time to stroked/filled polylines on
//! Lucide's 24×24 grid. Each renderer (egui painter, SDF softbuffer) scales
//! these coordinates and applies its own stroke width + antialiasing, so the
//! same icon looks identical in the overlay, the editor and app chrome.
//!
//! SVG source: Lucide revision 586bf5cf9bcf4129a63c0ce574602b23ca24f324.
//! The two flip icons retain their names and geometry from Lucide 0.577.0.
//!
//! Icons without a Lucide equivalent live here as hand-authored polylines.

/// One flattened sub-path of an icon.
///
/// - `pts`: polyline vertices in the 24×24 viewBox
/// - `closed`: the sub-path closes back to its start (`Z`)
/// - `fill`: paint as filled shape; otherwise paint `pts` as a stroke
pub struct StrokePath {
    pub pts: &'static [(f32, f32)],
    pub closed: bool,
    pub fill: bool,
}

/// Lucide's grid in logical units; `stroke-width=2` on it at render scale.
pub const VIEWBOX: f32 = 24.0;

/// Even-odd point-in-polygon test on icon-space coords — renderers combine
/// this with distance-to-segments for signed fill coverage.
pub fn inside(pts: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut c = false;
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            c = !c;
        }
        j = i;
    }
    c
}

include!(concat!(env!("OUT_DIR"), "/icons_gen.rs"));

/// Line tool — no Lucide equivalent: a diagonal segment with square dots at
/// both endpoints (distinguishes it from Arrow).
pub static LINE: &[StrokePath] = &[
    StrokePath {
        pts: &[(4.0, 20.0), (20.0, 4.0)],
        closed: false,
        fill: false,
    },
    StrokePath {
        pts: &[(2.6, 18.6), (5.4, 18.6), (5.4, 21.4), (2.6, 21.4)],
        closed: true,
        fill: true,
    },
    StrokePath {
        pts: &[(18.6, 2.6), (21.4, 2.6), (21.4, 5.4), (18.6, 5.4)],
        closed: true,
        fill: true,
    },
];

/// Filled arrowhead + thin shaft — the "closed arrow" end-style option
/// (`ARROW_UP_RIGHT` is the open-winged variant).
pub static ARROW_FILLED: &[StrokePath] = &[
    StrokePath {
        pts: &[(5.0, 19.0), (15.0, 9.0)],
        closed: false,
        fill: false,
    },
    StrokePath {
        pts: &[(9.0, 7.0), (20.0, 4.0), (17.0, 15.0)],
        closed: true,
        fill: true,
    },
];
