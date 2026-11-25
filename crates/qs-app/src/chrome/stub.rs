//! Chrome stub for platforms without a backend yet — `new` bails so the
//! shared `land` hands the `Land` back and the host falls back to the
//! quick-access card.

use super::{ChromeCmd, ChromeEv, PlatformChrome, Stack};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

/// Never constructed — `new` bails before creating a window.
pub struct ChromeWin {
    _uninhabited: core::convert::Infallible,
}



/// Shared chrome contract — signatures enforced by
/// `PlatformChrome` in `chrome.rs`; helpers stay inherent.
impl PlatformChrome for ChromeWin {
    /// Lets the shared `land` path fail like any other backend —
    /// `Chrome::land` hands the `Land` back and the host opens the card.
    fn new(
        _el: &ActiveEventLoop,
        _mon: &winit::monitor::MonitorHandle,
        _corner: super::Corner,
        _font: std::rc::Rc<fontdue::Font>,
        _accent: u32,
    ) -> anyhow::Result<Self> {
        anyhow::bail!("no chrome backend on this platform")
    }

    fn id(&self) -> WindowId {
        unreachable!()
    }

    fn set_dock_corner(&mut self, _corner: super::Corner) {
        unreachable!()
    }

    fn mon_key(&self) -> &str {
        unreachable!()
    }

    fn set_sink(&self, _sink: std::rc::Rc<dyn Fn(ChromeEv)>) {}

    fn land(&mut self, _l: super::Land) {
        unreachable!()
    }

    fn stack(&self) -> &Stack {
        unreachable!()
    }

    fn stack_mut(&mut self) -> &mut Stack {
        unreachable!()
    }

    fn event(&mut self, _ev: ChromeEv) -> Option<ChromeCmd> {
        None
    }

    fn redraw(&mut self) {}

    fn window_event(&mut self, _ev: &WindowEvent) -> bool {
        false
    }

    fn pump(&mut self) -> Option<std::time::Instant> {
        None
    }
}

