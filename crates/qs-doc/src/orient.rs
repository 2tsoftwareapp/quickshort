//! Output orientation: the eight symmetries of a rectangle (dihedral D4).
//!
//! Objects and the base image stay in document coordinates; the oriented
//! output is derived from them. Every variant is "mirror horizontally
//! (optional) then rotate clockwise by `turns` quarter turns".

use crate::{PointF, SizeF};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Orientation {
    #[default]
    Normal,
    /// 90° clockwise
    Rotate90,
    Rotate180,
    /// 270° clockwise (= 90° counter-clockwise)
    Rotate270,
    FlipHorizontal,
    FlipVertical,
    /// mirror across the main diagonal: (x, y) -> (y, x)
    Transpose,
    /// mirror across the anti-diagonal
    Transverse,
}

impl Orientation {
    /// `(clockwise quarter turns, mirrored first)`.
    pub fn parts(self) -> (u8, bool) {
        use Orientation::*;
        match self {
            Normal => (0, false),
            Rotate90 => (1, false),
            Rotate180 => (2, false),
            Rotate270 => (3, false),
            FlipHorizontal => (0, true),
            Transverse => (1, true),
            FlipVertical => (2, true),
            Transpose => (3, true),
        }
    }

    pub fn from_parts(turns: u8, flip: bool) -> Self {
        use Orientation::*;
        match (turns % 4, flip) {
            (0, false) => Normal,
            (1, false) => Rotate90,
            (2, false) => Rotate180,
            (3, false) => Rotate270,
            (0, true) => FlipHorizontal,
            (1, true) => Transverse,
            (2, true) => FlipVertical,
            _ => Transpose,
        }
    }

    /// Rotate the current output 90° clockwise.
    pub fn rotate_cw(self) -> Self {
        let (k, f) = self.parts();
        Self::from_parts(k + 1, f)
    }

    /// Rotate the current output 90° counter-clockwise.
    pub fn rotate_ccw(self) -> Self {
        let (k, f) = self.parts();
        Self::from_parts(k + 3, f)
    }

    /// Mirror the current output left-right.
    pub fn flip_h(self) -> Self {
        let (k, f) = self.parts();
        Self::from_parts(4 - k, !f)
    }

    /// Mirror the current output top-bottom.
    pub fn flip_v(self) -> Self {
        let (k, f) = self.parts();
        Self::from_parts(6 - k, !f)
    }

    pub fn inverse(self) -> Self {
        let (k, f) = self.parts();
        if f {
            self
        } else {
            Self::from_parts(4 - k, false)
        }
    }

    /// Width and height trade places (90° / 270° and the diagonal mirrors).
    pub fn swaps_axes(self) -> bool {
        self.parts().0 % 2 == 1
    }

    /// Size of the oriented output of an `s`-sized input.
    pub fn output_size(self, s: SizeF) -> SizeF {
        if self.swaps_axes() {
            SizeF::new(s.h, s.w)
        } else {
            s
        }
    }

    /// Input point (in an `s`-sized box, origin top-left) -> output point.
    pub fn map(self, p: PointF, s: SizeF) -> PointF {
        let (k, f) = self.parts();
        let (mut x, mut y, mut w, mut h) = (p.x, p.y, s.w, s.h);
        if f {
            x = w - x;
        }
        for _ in 0..k {
            (x, y) = (h - y, x);
            (w, h) = (h, w);
        }
        PointF::new(x, y)
    }

    /// Output point -> input point; `s` is the *input* size.
    pub fn unmap(self, p: PointF, s: SizeF) -> PointF {
        self.inverse().map(p, self.output_size(s))
    }

    /// Output-space displacement -> input-space displacement.
    pub fn unmap_vector(self, dx: f32, dy: f32) -> (f32, f32) {
        let s = SizeF::new(0.0, 0.0);
        let a = self.unmap(PointF::new(0.0, 0.0), s);
        let b = self.unmap(PointF::new(dx, dy), s);
        (b.x - a.x, b.y - a.y)
    }

    /// Per-edge amounts `[top, right, bottom, left]` given for the oriented
    /// output -> the same amounts on the input's edges.
    pub fn insets_to_input(self, out: [f32; 4]) -> [f32; 4] {
        let normals = [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)];
        let mut input = [0.0; 4];
        for (amount, (nx, ny)) in out.into_iter().zip(normals) {
            let v = self.unmap_vector(nx, ny);
            let edge = normals
                .iter()
                .position(|n| (n.0 - v.0).abs() < 0.5 && (n.1 - v.1).abs() < 0.5)
                .unwrap_or(0);
            input[edge] = amount;
        }
        input
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Orientation::*;

    const ALL: [Orientation; 8] = [
        Normal, Rotate90, Rotate180, Rotate270, FlipHorizontal, FlipVertical, Transpose, Transverse,
    ];
    const S: SizeF = SizeF::new(40.0, 10.0);

    fn close(a: PointF, b: PointF) -> bool {
        (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
    }

    #[test]
    fn named_variants_map_corners_as_expected() {
        let p = PointF::new(1.0, 2.0);
        let want = [
            (Normal, PointF::new(1.0, 2.0)),
            (Rotate90, PointF::new(8.0, 1.0)),
            (Rotate180, PointF::new(39.0, 8.0)),
            (Rotate270, PointF::new(2.0, 39.0)),
            (FlipHorizontal, PointF::new(39.0, 2.0)),
            (FlipVertical, PointF::new(1.0, 8.0)),
            (Transpose, PointF::new(2.0, 1.0)),
            (Transverse, PointF::new(8.0, 39.0)),
        ];
        for (o, w) in want {
            assert!(close(o.map(p, S), w), "{o:?}: {:?} != {w:?}", o.map(p, S));
        }
    }

    #[test]
    fn parts_round_trip_and_cover_all_eight() {
        let mut seen = std::collections::HashSet::new();
        for o in ALL {
            let (t, f) = o.parts();
            assert_eq!(Orientation::from_parts(t, f), o);
            seen.insert((t, f));
        }
        assert_eq!(seen.len(), 8);
    }

    #[test]
    fn ops_compose_in_output_space() {
        let p = PointF::new(3.0, 7.0);
        for o in ALL {
            let out = o.output_size(S);
            let q = o.map(p, S);
            // each op applied after `o` equals the op on the already-oriented point
            let cw = o.rotate_cw().map(p, S);
            assert!(close(cw, PointF::new(out.h - q.y, q.x)), "{o:?} cw");
            let ccw = o.rotate_ccw().map(p, S);
            assert!(close(ccw, PointF::new(q.y, out.w - q.x)), "{o:?} ccw");
            let fh = o.flip_h().map(p, S);
            assert!(close(fh, PointF::new(out.w - q.x, q.y)), "{o:?} fh");
            let fv = o.flip_v().map(p, S);
            assert!(close(fv, PointF::new(q.x, out.h - q.y)), "{o:?} fv");
        }
    }

    #[test]
    fn rotate_and_flip_reach_diagonal_mirrors() {
        assert_eq!(Normal.rotate_cw().flip_h(), Transpose);
        assert_eq!(Normal.rotate_cw().flip_v(), Transverse);
        assert_eq!(Normal.flip_h().rotate_cw(), Transverse);
    }

    #[test]
    fn four_turns_and_double_flips_are_identity() {
        for o in ALL {
            assert_eq!(o.rotate_cw().rotate_cw().rotate_cw().rotate_cw(), o);
            assert_eq!(o.rotate_cw().rotate_ccw(), o);
            assert_eq!(o.flip_h().flip_h(), o);
            assert_eq!(o.flip_v().flip_v(), o);
        }
    }

    #[test]
    fn unmap_inverts_map_and_sizes_swap() {
        let p = PointF::new(5.5, 9.25);
        for o in ALL {
            assert!(close(o.unmap(o.map(p, S), S), p), "{o:?}");
            let out = o.output_size(S);
            assert_eq!(o.swaps_axes(), out == SizeF::new(10.0, 40.0), "{o:?}");
            assert!(out == S || out == SizeF::new(10.0, 40.0));
        }
    }

    #[test]
    fn serde_default_and_names() {
        assert_eq!(serde_json::to_string(&Rotate90).unwrap(), "\"Rotate90\"");
        assert_eq!(Orientation::default(), Normal);
    }

    #[test]
    fn displayed_edges_and_vectors_map_back_to_input() {
        // clockwise: input left becomes the displayed top, input top the right
        assert_eq!(Rotate90.insets_to_input([1.0, 2.0, 3.0, 4.0]), [2.0, 3.0, 4.0, 1.0]);
        assert_eq!(FlipHorizontal.insets_to_input([1.0, 2.0, 3.0, 4.0]), [1.0, 4.0, 3.0, 2.0]);
        assert_eq!(Normal.insets_to_input([1.0, 2.0, 3.0, 4.0]), [1.0, 2.0, 3.0, 4.0]);
        let (dx, dy) = Rotate90.unmap_vector(1.0, 0.0);
        assert!((dx - 0.0).abs() < 1e-6 && (dy + 1.0).abs() < 1e-6, "right on screen = up in input");
        for o in ALL {
            let mut ins = o.insets_to_input([1.0, 2.0, 3.0, 4.0]);
            ins.sort_by(f32::total_cmp);
            assert_eq!(ins, [1.0, 2.0, 3.0, 4.0], "{o:?} is a permutation");
        }
    }
}
