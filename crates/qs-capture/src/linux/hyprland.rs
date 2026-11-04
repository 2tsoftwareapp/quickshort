//! Hyprland screenshot backend. Its ScreenCast portal may return only the
//! chosen monitor even with `multiple=true`; grim's per-output capture and
//! hyprctl's layout provide all screens for cross-monitor selections.

use anyhow::{bail, Context, Result};
use qs_core::CapturedFrame;
use serde::Deserialize;
use std::process::Command;

use super::wayland::mk_frame;
use crate::ScreenCapture;

pub struct HyprlandCapture;

#[derive(Deserialize)]
struct Monitor {
    name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn monitors() -> Result<Vec<Monitor>> {
    let output = Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()
        .context("run hyprctl monitors")?;
    if !output.status.success() {
        bail!(
            "hyprctl monitors failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let monitors: Vec<Monitor> =
        serde_json::from_slice(&output.stdout).context("parse Hyprland monitors")?;
    if monitors.is_empty() {
        bail!("Hyprland returned no monitors");
    }
    Ok(monitors)
}

impl ScreenCapture for HyprlandCapture {
    fn capture_all(&self) -> Result<Vec<CapturedFrame>> {
        monitors()?
            .into_iter()
            .map(|m| {
                let output = Command::new("grim")
                    .args(["-o", &m.name, "-"])
                    .output()
                    .with_context(|| format!("capture Hyprland output {}", m.name))?;
                if !output.status.success() {
                    bail!(
                        "grim -o {} failed: {}",
                        m.name,
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
                let pixels = image::load_from_memory(&output.stdout)
                    .with_context(|| format!("decode {} screenshot", m.name))?
                    .to_rgba8();
                Ok(mk_frame(
                    pixels,
                    qs_core::Rect::new(m.x, m.y, m.width, m.height),
                ))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn captures_every_hyprland_monitor_at_its_logical_position() {
        let layout = monitors().unwrap();
        let frames = HyprlandCapture.capture_all().unwrap();
        assert_eq!(frames.len(), layout.len());
        for (frame, monitor) in frames.iter().zip(layout.iter()) {
            assert_eq!(
                frame.geometry.logical_rect(),
                qs_core::Rect::new(monitor.x, monitor.y, monitor.width, monitor.height)
            );
            assert!(frame.pixels.width() > 0 && frame.pixels.height() > 0);
        }
    }
}
