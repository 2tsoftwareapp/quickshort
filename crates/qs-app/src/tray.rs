//! Tray icon + menu plumbing: Quickshort branding, menu construction,
//! launch-at-login toggling per OS.

use anyhow::{Context, Result};
use muda::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use std::path::PathBuf;
use tray_icon::Icon;

use crate::config::Config;

pub const ID_CAPTURE: &str = "capture";
/// Saves the whole virtual screen — no region overlay.
pub const ID_CAPTURE_FULL: &str = "capture_full";
pub const ID_OPEN_DIR: &str = "open_dir";
pub const ID_SETTINGS: &str = "settings";
pub const ID_LOGIN: &str = "launch_at_login";
pub const ID_QUIT: &str = "quit";
/// Recent-capture items carry `recent:<abs path>` as their id.
pub const ID_RECENT_PREFIX: &str = "recent:";
pub const ID_UPDATE: &str = "update";
pub const ID_DONATE: &str = "donate";
pub const ID_GDRIVE: &str = "gdrive";
pub const ID_GDRIVE_DISCONNECT: &str = "gdrive_disconnect";
pub const ID_EVIDENCE_START: &str = "evidence_start";
pub const ID_EVIDENCE_NEXT: &str = "evidence_next";
pub const ID_EVIDENCE_END: &str = "evidence_end";

/// Rebuild the tray menu. `recents` = newest-first saved PNGs;
/// `pending_update` = version waiting to be installed (changes the
/// "Check for Updates…" item into a one-click updater); `evidence` = label
/// of the running evidence session.
pub fn build_menu(
    cfg: &Config,
    recents: &[PathBuf],
    pending_update: Option<&str>,
    evidence: Option<&str>,
) -> Menu {
    let menu = Menu::new();
    let capture = MenuItem::with_id(
        ID_CAPTURE,
        format!("Capture Region    {}", cfg.hotkey),
        true,
        None,
    );
    let capture_full = MenuItem::with_id(
        ID_CAPTURE_FULL,
        format!("Capture Full Screen    {}", cfg.hotkey_full),
        true,
        None,
    );
    let recent = Submenu::new("Recent Captures", !recents.is_empty());
    for p in recents {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.display().to_string());
        let item = MenuItem::with_id(
            format!("{ID_RECENT_PREFIX}{}", p.display()),
            name,
            true,
            None,
        );
        let _ = recent.append(&item);
    }
    let open = MenuItem::with_id(ID_OPEN_DIR, "Open Screenshots Folder", true, None);
    let login =
        CheckMenuItem::with_id(ID_LOGIN, "Launch at Login", true, cfg.launch_at_login, None);
    let settings = MenuItem::with_id(ID_SETTINGS, "Settings…", true, None);
    let drive = CheckMenuItem::with_id(
        ID_GDRIVE,
        "Share Links via Google Drive",
        crate::gdrive::available(),
        cfg.share_to_drive,
        None,
    );
    let drive_off = MenuItem::with_id(
        ID_GDRIVE_DISCONNECT,
        "Disconnect Google Drive",
        crate::gdrive::connected(),
        None,
    );
    let update = MenuItem::with_id(
        ID_UPDATE,
        match pending_update {
            Some(v) => format!("Update to v{v} — Restart to Install"),
            None => "Check for Updates…".to_string(),
        },
        true,
        None,
    );
    let donate = MenuItem::with_id(ID_DONATE, "Support Quickshort", true, None);
    let quit = MenuItem::with_id(ID_QUIT, "Quit Quickshort", true, None);
    let evidence_items = match evidence {
        None => vec![MenuItem::with_id(
            ID_EVIDENCE_START,
            "Start Evidence Session",
            true,
            None,
        )],
        Some(l) => vec![
            MenuItem::new(format!("Evidence: {l}"), false, None),
            MenuItem::with_id(ID_EVIDENCE_NEXT, "Next Test Case", true, None),
            MenuItem::with_id(ID_EVIDENCE_END, "End Evidence Session", true, None),
        ],
    };
    let _ = menu.append_items(&[&capture, &capture_full]);
    for i in &evidence_items {
        let _ = menu.append(i);
    }
    let _ = menu.append_items(&[
        &recent,
        &PredefinedMenuItem::separator(),
        &open,
        &settings,
        &drive,
        &drive_off,
        &login,
        &PredefinedMenuItem::separator(),
        &update,
        &donate,
        &quit,
    ]);
    menu
}

/// Resize the same Quickshort logo used by the editor and the app bundle.
pub fn tray_icon(size: u32) -> Icon {
    let logo = image::load_from_memory(include_bytes!("../../../assets/icon.png"))
        .expect("embedded Quickshort app icon must be a valid PNG")
        .resize_exact(size, size, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    Icon::from_rgba(logo.into_raw(), size, size).expect("valid Quickshort tray icon")
}

// ---------- Linux: the tray lives on a dedicated gtk thread ----------
//
// tray-icon/muda call gtk, which is single-threaded but not bound to
// "the" main thread — so a dedicated thread runs gtk::init() +
// gtk::main() and owns the TrayIcon, while the winit thread talks to it
// through a channel. `MainContext::invoke` wakes gtk::main() exactly
// when a message arrives, so no polling is needed.
#[cfg(target_os = "linux")]
pub mod linux_tray {
    use super::*;
    use gtk::glib;
    use std::cell::RefCell;
    use std::sync::{mpsc, OnceLock};

    pub enum TrayMsg {
        Menu {
            cfg: Config,
            recents: Vec<PathBuf>,
            update: Option<String>,
            evidence: Option<String>,
        },
        Quit,
    }

    thread_local! {
        static TRAY: RefCell<Option<tray_icon::TrayIcon>> = const { RefCell::new(None) };
        static RX: RefCell<Option<mpsc::Receiver<TrayMsg>>> = const { RefCell::new(None) };
    }
    static CTX: OnceLock<glib::MainContext> = OnceLock::new();

    /// Handle held by the host thread — the TrayIcon itself is pinned
    /// inside the tray thread's thread-local.
    pub struct TrayHandle {
        tx: mpsc::Sender<TrayMsg>,
    }

    pub fn spawn(cfg: &Config, recents: &[PathBuf]) -> Result<TrayHandle> {
        let (tx, rx) = mpsc::channel::<TrayMsg>();
        let (init_tx, init_rx) = mpsc::channel::<Result<()>>();
        let (cfg, recents) = (cfg.clone(), recents.to_vec());
        std::thread::spawn(move || {
            if let Err(e) = gtk::init() {
                let _ = init_tx.send(Err(anyhow::anyhow!("gtk::init: {e}")));
                return;
            }
            let built = tray_icon::TrayIcon::builder()
                .with_tooltip("Quickshort — region screenshot")
                .with_menu(Box::new(build_menu(&cfg, &recents, None, None)))
                .with_icon(tray_icon(44))
                .build();
            match built {
                Ok(t) => TRAY.with(|s| *s.borrow_mut() = Some(t)),
                Err(e) => {
                    let _ = init_tx.send(Err(anyhow::anyhow!("tray icon: {e}")));
                    return;
                }
            }
            RX.with(|s| *s.borrow_mut() = Some(rx));
            crate::chrome::gtk_layer::spawn_channel();
            crate::chrome::gtk_album::spawn_channel();
            // gtk::main iterates the default context — publish it so other
            // threads can wake the loop via invoke()
            let _ = CTX.set(glib::MainContext::default());
            let _ = init_tx.send(Ok(()));
            gtk::main();
        });
        init_rx.recv().context("tray thread died")??;
        Ok(TrayHandle { tx })
    }

    /// Runs on the tray thread via MainContext::invoke.
    fn drain() {
        crate::chrome::gtk_layer::drain_pending();
        crate::chrome::gtk_album::drain_pending();
        RX.with(|rx| {
            let rx = rx.borrow();
            let Some(rx) = rx.as_ref() else { return };
            while let Ok(m) = rx.try_recv() {
                match m {
                    TrayMsg::Menu {
                        cfg,
                        recents,
                        update,
                        evidence,
                    } => TRAY.with(|t| {
                        if let Some(t) = t.borrow().as_ref() {
                            t.set_menu(Some(Box::new(build_menu(
                                &cfg,
                                &recents,
                                update.as_deref(),
                                evidence.as_deref(),
                            ))));
                        }
                    }),
                    TrayMsg::Quit => {
                        TRAY.with(|t| *t.borrow_mut() = None);
                        gtk::main_quit();
                    }
                }
            }
        });
    }

    pub fn wake() {
        if let Some(ctx) = CTX.get() {
            ctx.invoke(drain);
        }
    }

    /// Run `f` on the tray GTK thread (after `spawn`).
    pub fn gtk_invoke<F: FnOnce() + Send + 'static>(f: F) {
        if let Some(ctx) = CTX.get() {
            ctx.invoke(f);
        } else {
            log::info!("chrome: gtk_invoke before tray init");
        }
    }

    impl TrayHandle {
        pub fn set_menu(
            &self,
            cfg: Config,
            recents: Vec<PathBuf>,
            update: Option<String>,
            evidence: Option<String>,
        ) {
            let _ = self.tx.send(TrayMsg::Menu {
                cfg,
                recents,
                update,
                evidence,
            });
            wake();
        }
    }

    impl Drop for TrayHandle {
        fn drop(&mut self) {
            let _ = self.tx.send(TrayMsg::Quit);
            wake();
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux_tray::TrayHandle;

/// Toggle "launch at login" for the current executable.
pub fn set_launch_at_login(enable: bool) -> Result<()> {
    let exe = std::env::current_exe().context("current_exe")?;
    #[cfg(target_os = "macos")]
    {
        // LaunchAgent — works for an unbundled binary; a future .app can
        // switch to SMAppService without touching callers.
        let dir = dirs::home_dir()
            .context("no home dir")?
            .join("Library/LaunchAgents");
        let plist = dir.join("ai.quickshort.plist");
        if enable {
            std::fs::create_dir_all(&dir)?;
            std::fs::write(
                &plist,
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                     <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\"\
                     \x20\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                     <plist version=\"1.0\"><dict>\n\
                     <key>Label</key><string>ai.quickshort</string>\n\
                     <key>ProgramArguments</key><array><string>{}</string></array>\n\
                     <key>RunAtLoad</key><true/>\n\
                     </dict></plist>\n",
                    exe.display()
                ),
            )?;
        } else {
            let _ = std::fs::remove_file(&plist);
        }
    }
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let run = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(
                "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                winreg::enums::KEY_SET_VALUE,
            )
            .context("open Run key")?;
        if enable {
            run.set_value("quickshort", &exe.display().to_string())?;
        } else {
            let _ = run.delete_value("quickshort");
        }
    }
    #[cfg(target_os = "linux")]
    {
        let dir = dirs::config_dir()
            .context("no config dir")?
            .join("autostart");
        let desk = dir.join("quickshort.desktop");
        if enable {
            std::fs::create_dir_all(&dir)?;
            std::fs::write(
                &desk,
                format!(
                    "[Desktop Entry]\nType=Application\nName=Quickshort\n\
                     Exec={}\nX-GNOME-Autostart-enabled=true\n",
                    exe.display()
                ),
            )?;
        } else {
            let _ = std::fs::remove_file(&desk);
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (enable, exe);
        anyhow::bail!("launch at login unsupported on this platform");
    }
    Ok(())
}
