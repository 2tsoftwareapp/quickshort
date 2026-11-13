//! Non-destructive image effects, in oriented output coordinates.
use crate::{PointF, SizeF};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Effects {
    pub border: Option<BorderEffect>,
    pub shadow: Option<ShadowEffect>,
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
