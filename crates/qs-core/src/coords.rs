//! Typed logical/physical coordinates and the per-monitor scale bridge.
//!
//! Windows, cursors and monitor layout live in *logical* units of one
//! virtual desktop; captured buffers are *physical* pixels. `MonitorScale`
//! carries the mapping for one monitor —
//! `physical = (logical - logical_origin) * scale_factor + physical_origin` —
//! so call sites never multiply a raw scale factor by hand. Conversions are
//! edge-scaled (both edges transformed, then differenced) which keeps a
//! rect's physical size consistent with its endpoints instead of drifting
//! by a rounding pixel.

use crate::{Point, Rect};

/// Point in logical units — virtual-desktop coords shared across monitors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LogicalPoint {
    pub x: i32,
    pub y: i32,
}

impl LogicalPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn to_physical(self, m: &MonitorScale) -> PhysicalPoint {
        PhysicalPoint {
            x: (self.x - m.logical_origin.x) as f64 * m.scale_factor + m.physical_origin.x,
            y: (self.y - m.logical_origin.y) as f64 * m.scale_factor + m.physical_origin.y,
        }
    }
}

/// Rect in logical units. Convert to/from [`crate::Rect`] freely — the
/// newtype only marks which coordinate space the value means.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LogicalRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl LogicalRect {
    pub const fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    pub fn origin(self) -> LogicalPoint {
        LogicalPoint::new(self.x, self.y)
    }

    pub fn to_physical(self, m: &MonitorScale) -> PhysicalRect {
        let x0 = self.origin().to_physical(m);
        let x1 = LogicalPoint::new(self.x + self.w as i32, self.y + self.h as i32).to_physical(m);
        PhysicalRect {
            x: x0.x.round() as i32,
            y: x0.y.round() as i32,
            w: (x1.x - x0.x).round().max(0.0) as u32,
            h: (x1.y - x0.y).round().max(0.0) as u32,
        }
    }
}

/// Point in physical pixels — f64 because input devices report subpixel
/// positions.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhysicalPoint {
    pub x: f64,
    pub y: f64,
}

impl PhysicalPoint {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn to_logical(self, m: &MonitorScale) -> LogicalPoint {
        LogicalPoint {
            x: ((self.x - m.physical_origin.x) / m.scale_factor + m.logical_origin.x as f64).round()
                as i32,
            y: ((self.y - m.physical_origin.y) / m.scale_factor + m.logical_origin.y as f64).round()
                as i32,
        }
    }
}

/// Rect in physical pixels — buffer coordinates, integer pixel grid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl PhysicalRect {
    pub const fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    pub fn to_logical(self, m: &MonitorScale) -> LogicalRect {
        let x0 = PhysicalPoint::new(self.x as f64, self.y as f64).to_logical(m);
        let x1 = PhysicalPoint::new(self.x as f64 + self.w as f64, self.y as f64 + self.h as f64)
            .to_logical(m);
        LogicalRect {
            x: x0.x,
            y: x0.y,
            w: (x1.x - x0.x).max(0) as u32,
            h: (x1.y - x0.y).max(0) as u32,
        }
    }
}

/// One monitor's logical↔physical mapping. `logical_origin` is the
/// monitor's top-left in virtual logical space; `physical_origin` is where
/// that logical origin lands in physical space — `(0,0)` when converting
/// into a monitor-local pixel buffer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonitorScale {
    pub scale_factor: f64,
    pub logical_origin: LogicalPoint,
    pub physical_origin: PhysicalPoint,
}

impl MonitorScale {
    /// Uniform scale with both origins at zero — virtual-space conversions
    /// (e.g. a winit monitor rect physical→logical, or a composite canvas).
    pub fn new(scale_factor: f64) -> Self {
        Self::at(scale_factor, LogicalPoint::default())
    }

    /// Scale anchored at a virtual logical origin; physical origin `(0,0)`
    /// — i.e. conversion into a monitor-local buffer or canvas.
    pub fn at(scale_factor: f64, logical_origin: LogicalPoint) -> Self {
        Self {
            scale_factor,
            logical_origin,
            physical_origin: PhysicalPoint::default(),
        }
    }

    /// A bare logical length in physical units (sizes, radii, margins).
    pub fn len(&self, logical: f64) -> f64 {
        logical * self.scale_factor
    }

    /// A bare physical length in logical units.
    pub fn unlen(&self, physical: f64) -> f64 {
        physical / self.scale_factor
    }
}

impl From<Rect> for LogicalRect {
    fn from(r: Rect) -> Self {
        Self::new(r.x, r.y, r.w, r.h)
    }
}

impl From<LogicalRect> for Rect {
    fn from(r: LogicalRect) -> Self {
        Rect::new(r.x, r.y, r.w, r.h)
    }
}

impl From<Point> for LogicalPoint {
    fn from(p: Point) -> Self {
        Self::new(p.x, p.y)
    }
}

impl From<LogicalPoint> for Point {
    fn from(p: LogicalPoint) -> Self {
        Point::new(p.x, p.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_is_edge_scaled() {
        // logical rect at (10,20) 100x50 on a 2x monitor with origin (5,0)
        let m = MonitorScale::at(2.0, LogicalPoint::new(5, 0));
        let p = LogicalRect::new(10, 20, 100, 50).to_physical(&m);
        assert_eq!(p, PhysicalRect::new(10, 40, 200, 100));
        // inverse lands back on the same logical rect
        assert_eq!(p.to_logical(&m), LogicalRect::new(10, 20, 100, 50));
    }

    #[test]
    fn fractional_scale_rounds() {
        let m = MonitorScale::new(1.5);
        let p = LogicalRect::new(0, 0, 101, 33).to_physical(&m);
        assert_eq!(p, PhysicalRect::new(0, 0, 152, 50));
        assert_eq!(PhysicalPoint::new(7.4, 2.6).to_logical(&m), LogicalPoint::new(5, 2));
    }
}
