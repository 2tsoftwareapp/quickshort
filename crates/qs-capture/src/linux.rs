use anyhow::Result;
use qs_core::CapturedFrame;

use crate::ScreenCapture;

mod hyprland;
pub mod live;
mod portal;
pub mod wayland;
mod wlr;
mod x11;

/// Which compositor/session family owns the display — the capture backend
/// and `record_start` both pick from this. Wayland has no global virtual
/// coordinate space exposed to clients; the mutter backend gets the real
/// layout from DisplayConfig instead. Hyprland and generic wlroots
/// (Sway, Niri, River, labwc...) capture in-process via
/// zwlr_screencopy_manager_v1 — no consent dialog — with grim (Hyprland)
/// or xdg-desktop-portal as the fallback when the protocol is missing
/// (GNOME/KDE block it).
/// QS_CAPTURE=portal|mutter|hyprland|wlr|x11 overrides the auto pick.
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

/// Is `name` owned on the session bus? Probe for portal/mutter backends —
/// the services only exist on desktops that provide them.
fn bus_has_owner(name: &str) -> bool {
    use zbus::blocking::{Connection, Proxy};
    let Ok(conn) = Connection::session() else {
        return false;
    };
    let Ok(p) = Proxy::new(&conn, "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus") else {
        return false;
    };
    p.call::<_, _, bool>("NameHasOwner", &(name,)).unwrap_or(false)
}

/// Is `bin` on PATH? Probe for tool-based backends (grim, hyprctl).
fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

pub fn session_kind() -> Result<SessionKind> {
    if let Ok(force) = std::env::var("QS_CAPTURE") {
        return match force.as_str() {
            "portal" => Ok(SessionKind::Portal),
            "mutter" => Ok(SessionKind::Mutter),
            "hyprland" => Ok(SessionKind::Hyprland),
            "wlr" => Ok(SessionKind::Portal), // routed below, forced wlr first
            "x11" => Ok(SessionKind::X11),
            other => anyhow::bail!("QS_CAPTURE={other}: expected portal|mutter|hyprland|wlr|x11"),
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

/// Ordered backend candidates for one session kind, best first —
/// capability-probed where possible (the wlr check is a real
/// registry read, not a name guess). Remaining entries are runtime
/// fallbacks `FallbackCapture` swaps in if the primary fails a grab.
fn chain(kind: SessionKind) -> Vec<Box<dyn ScreenCapture + Send>> {
    let mut v: Vec<Box<dyn ScreenCapture + Send>> = match kind {
        // Hyprland: wlr screencopy first (in-process, no grim dependency),
        // then the per-output grim+hyprctl path that also knows the
        // workspace layout even when zwlr is unavailable.
        SessionKind::Hyprland => vec![Box::new(wlr::WlrCapture), Box::new(hyprland::HyprlandCapture)],
        SessionKind::Mutter => vec![Box::new(wayland::MutterCapture), Box::new(portal::PortalCapture)],
        // generic Wayland (KDE, wlroots, others): instant wlr capture when
        // the compositor exports it, else the portal consent flow
        SessionKind::Portal => vec![Box::new(wlr::WlrCapture), Box::new(portal::PortalCapture)],
        SessionKind::X11 => vec![Box::new(x11::X11Capture)],
    };
    // cheap protocol probes drop candidates that cannot possibly work —
    // everything left gets a real chance at grab time
    v.retain(|b| b.probe());
    v
}

/// Pick the right backend for the current platform/session — capability
/// order, then lazy runtime fallback on grab failure.
pub fn backend() -> Result<Box<dyn ScreenCapture>> {
    let chain = chain(session_kind()?);
    anyhow::ensure!(!chain.is_empty(), "no capture backend for this session");
    Ok(Box::new(FallbackCapture {
        chain,
        active: std::sync::atomic::AtomicUsize::new(0),
    }))
}

/// Walks the candidate chain starting at the last backend that worked;
/// on a grab failure advances to the next candidate and retries — a
/// broken compositor stays skipped for the rest of the session.
struct FallbackCapture {
    chain: Vec<Box<dyn ScreenCapture + Send>>,
    active: std::sync::atomic::AtomicUsize,
}

impl FallbackCapture {
    fn active(&self) -> usize {
        self.active.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl ScreenCapture for FallbackCapture {
    fn capture_all(&self) -> Result<Vec<CapturedFrame>> {
        let mut i = self.active();
        loop {
            match self.chain[i].capture_all() {
                Err(e) if i + 1 < self.chain.len() => {
                    eprintln!("capture backend failed: {e:#}; trying next");
                    i += 1;
                }
                r => {
                    self.active
                        .store(i, std::sync::atomic::Ordering::Relaxed);
                    return r;
                }
            }
        }
    }

    /// Session fallback only covers *init* failure — once a session is
    /// open its grabs borrow that backend, so runtime grab errors
    /// propagate instead of silently switching mid-stream.
    fn session(&self) -> Result<Box<dyn FnMut() -> Result<Vec<CapturedFrame>> + Send + '_>> {
        let mut i = self.active();
        loop {
            match self.chain[i].session() {
                Err(e) if i + 1 < self.chain.len() => {
                    eprintln!("capture session init failed: {e:#}; trying next");
                    i += 1;
                }
                r => {
                    self.active
                        .store(i, std::sync::atomic::Ordering::Relaxed);
                    return r;
                }
            }
        }
    }
}
