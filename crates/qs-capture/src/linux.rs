use anyhow::Result;

use crate::ScreenCapture;

mod hyprland;
pub mod live;
mod portal;
pub mod wayland;
mod x11;

/// Which compositor/session family owns the display — the capture backend
/// and `record_start` both pick from this. Wayland has no global virtual
/// coordinate space exposed to clients; the mutter backend gets the real
/// layout from DisplayConfig instead. Non-GNOME compositors (KDE, wlroots)
/// go through xdg-desktop-portal, except Hyprland's per-output grim capture.
/// QS_CAPTURE=portal|mutter|x11 overrides
/// the auto pick.
#[derive(Clone, Copy)]
pub enum SessionKind {
    Mutter,
    Portal,
    Hyprland,
    X11,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyprland_with_unset_session_type_uses_native_capture() {
        assert!(matches!(
            auto_session_kind(None, true, "Hyprland"),
            SessionKind::Hyprland
        ));
        assert!(matches!(
            auto_session_kind(Some("wayland"), true, "Hyprland"),
            SessionKind::Hyprland
        ));
        assert!(matches!(
            auto_session_kind(None, true, "KDE"),
            SessionKind::Portal
        ));
    }

    #[test]
    fn xwayland_display_does_not_override_wayland_socket() {
        assert!(matches!(
            auto_session_kind(Some("x11"), true, "Hyprland"),
            SessionKind::X11
        ));
        assert!(matches!(
            auto_session_kind(None, false, "Hyprland"),
            SessionKind::X11
        ));
    }
}

pub fn session_kind() -> Result<SessionKind> {
    if let Ok(force) = std::env::var("QS_CAPTURE") {
        return match force.as_str() {
            "portal" => Ok(SessionKind::Portal),
            "mutter" => Ok(SessionKind::Mutter),
            "x11" => Ok(SessionKind::X11),
            other => anyhow::bail!("QS_CAPTURE={other}: expected portal|mutter|x11"),
        };
    }
    Ok(auto_session_kind(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        &std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
    ))
}

fn auto_session_kind(session: Option<&str>, wayland_display: bool, desktop: &str) -> SessionKind {
    if session == Some("wayland") || (session.is_none() && wayland_display) {
        let desktops = desktop.to_ascii_uppercase();
        if desktops.split(':').any(|d| d.trim() == "GNOME") {
            SessionKind::Mutter
        } else if desktops.split(':').any(|d| d.trim() == "HYPRLAND") {
            SessionKind::Hyprland
        } else {
            SessionKind::Portal
        }
    } else {
        SessionKind::X11
    }
}

/// Pick the right backend for the current platform/session.
pub fn backend() -> Result<Box<dyn ScreenCapture>> {
    match session_kind()? {
        SessionKind::Mutter => Ok(Box::new(wayland::MutterCapture)),
        SessionKind::Portal => Ok(Box::new(portal::PortalCapture)),
        SessionKind::Hyprland => Ok(Box::new(hyprland::HyprlandCapture)),
        SessionKind::X11 => Ok(Box::new(x11::X11Capture)),
    }
}
