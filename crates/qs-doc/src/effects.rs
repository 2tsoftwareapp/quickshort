//! Non-destructive image effects, in oriented output coordinates.
use crate::{PointF, SizeF};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Effects {
    pub border: Option<BorderEffect>,
    pub shadow: Option<ShadowEffect>,
    pub watermark: Option<WatermarkEffect>,
    pub edge: Option<EdgeEffect>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    #[default]
    BottomRight,
}

/// Text stamped over the oriented content. `font_scale` multiplies a 24px base.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WatermarkEffect {
    pub text: String,
    pub anchor: Anchor,
    pub opacity: f32,
    pub font_scale: f32,
    pub color: u32,
}
impl Default for WatermarkEffect {
    fn default() -> Self {
        Self {
            text: String::new(),
            anchor: Anchor::BottomRight,
            opacity: 0.5,
            font_scale: 1.0,
            color: 0xff000000,
        }
    }
}

/// Deterministic torn edge on the selected sides; cuts alpha inward up to `amplitude`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EdgeEffect {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
    pub amplitude: f32,
    pub seed: u32,
}
impl Default for EdgeEffect {
    fn default() -> Self {
        Self {
            top: true,
            bottom: false,
            left: false,
            right: false,
            amplitude: 8.0,
            seed: 1,
        }
    }
}

/// Colors use the same straight 0xAARRGGBB representation as Document.background.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BorderEffect {
    pub width: f32,
    pub color: u32,
    pub radius: f32,
    pub inset: bool,
}
impl Default for BorderEffect {
    fn default() -> Self {
        Self {
            width: 4.0,
            color: 0xff000000,
            radius: 0.0,
            inset: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShadowEffect {
    /// Gaussian sigma in logical pixels; padding includes the full 3-sigma kernel.
    pub blur_radius: f32,
    pub offset: PointF,
    pub opacity: f32,
    pub color: u32,
}
impl Default for ShadowEffect {
    fn default() -> Self {
        Self {
            blur_radius: 8.0,
            offset: PointF::new(4.0, 6.0),
            opacity: 0.4,
            color: 0xff000000,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EffectLayout {
    /// Final output size and content origin, logical pixels snapped to physical pixels.
    pub size: SizeF,
    pub content_origin: PointF,
}

impl Effects {
    pub fn validate(&self) -> anyhow::Result<()> {
        let range = |v: f32, lo: f32, hi: f32| v.is_finite() && (lo..=hi).contains(&v);
        if let Some(b) = &self.border {
            anyhow::ensure!(
                range(b.width, 0.0, 256.0) && range(b.radius, 0.0, 1024.0),
                "invalid border geometry"
            );
        }
        if let Some(s) = &self.shadow {
            anyhow::ensure!(
                range(s.blur_radius, 0.0, 128.0)
                    && range(s.offset.x, -2048.0, 2048.0)
                    && range(s.offset.y, -2048.0, 2048.0)
                    && range(s.opacity, 0.0, 1.0),
                "invalid shadow geometry or opacity"
            );
        }
        if let Some(w) = &self.watermark {
            anyhow::ensure!(
                range(w.opacity, 0.0, 1.0)
                    && range(w.font_scale, 0.05, 32.0)
                    && w.text.len() <= 1024,
                "invalid watermark"
            );
        }
        if let Some(e) = &self.edge {
            anyhow::ensure!(range(e.amplitude, 0.0, 256.0), "invalid torn edge");
        }
        Ok(())
    }

    /// One shared layout for renderer, texture bounds and inverse hit testing.
    /// `content` is the oriented, already pixel-rounded view size.
    pub fn layout(&self, content: SizeF, scale: f32) -> EffectLayout {
        let border = self
            .border
            .as_ref()
            .filter(|b| !b.inset)
            .map_or(0.0, |b| (b.width * scale).ceil());
        let (mut l, mut t, mut r, mut b) = (border, border, border, border);
        if let Some(s) = &self.shadow {
            if s.opacity > 0.0 && s.color >> 24 != 0 {
                let halo = (s.blur_radius * scale * 3.0).ceil();
                let dx = (s.offset.x * scale).round();
                let dy = (s.offset.y * scale).round();
                l += (halo - dx).max(0.0);
                r += (halo + dx).max(0.0);
                t += (halo - dy).max(0.0);
                b += (halo + dy).max(0.0);
            }
        }
        EffectLayout {
            size: SizeF::new(content.w + (l + r) / scale, content.h + (t + b) / scale),
            content_origin: PointF::new(l / scale, t / scale),
        }
    }
}
