//! Unified macOS-style header toolbar, tool palette and contextual
//! properties bar — split by tier: `top` (app actions), `tools` (the
//! tool palette) and `properties` (per-tool options).

mod properties;
mod tools;
mod top;

pub use properties::render_properties_bar;
pub use tools::render_tool_palette;
pub use top::{render_top_bar, TopBarResult, ZoomCmd};

use eframe::egui::Color32;

/// Shared control height so the width capsule matches chips and buttons.
pub(crate) const CONTROL_H: f32 = crate::components::CONTROL_H;
pub(crate) const SPACE_XS: f32 = qs_theme::space::XS as f32;
pub(crate) const SPACE_SM: f32 = qs_theme::space::SM as f32;

pub fn rgb_to_color32(rgb: u32) -> Color32 {
    Color32::from_rgb(
        ((rgb >> 16) & 255) as u8,
        ((rgb >> 8) & 255) as u8,
        (rgb & 255) as u8,
    )
}

// ---------------------------------------------------------------------------
// Tier 1: Unified Header Toolbar (46px)
// ---------------------------------------------------------------------------
