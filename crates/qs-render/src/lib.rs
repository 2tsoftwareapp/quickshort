//! Headless renderer for `qs-doc` documents.
//!
//! Same output for the editor preview, the PNG/clipboard export and tests:
//! tiny-skia paths for shapes, the bundled Be Vietnam Pro font for text.

mod bounds;
mod draw;
pub mod shapes;
pub mod text;

pub use bounds::{bounds, text_block};
pub use draw::{callout_layout, gaussian, orient, output_size, pixelate, render, CALLOUT_PAD};
