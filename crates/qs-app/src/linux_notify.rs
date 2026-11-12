//! `org.freedesktop.Notifications` over the session D-Bus — the same API
//! `notify-send` uses, without spawning a subprocess or a winit window.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, Once};

use anyhow::Context;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::Value;

const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const IFACE: &str = "org.freedesktop.Notifications";
const APP: &str = "quickshort";
/// Reuse one slot so rapid saves replace instead of stacking.
const REPLACES_ID: u32 = 0x71_71_71;

static BUS: std::sync::OnceLock<Result<Connection, String>> = std::sync::OnceLock::new();

fn session() -> anyhow::Result<&'static Connection> {
    BUS.get_or_init(|| Connection::session().map_err(|e| format!("{e:#}")))
        .as_ref()
        .map_err(|e| anyhow::anyhow!("session dbus: {e}"))
}

fn send_sync(msg: &str, ok: bool) -> anyhow::Result<()> {
    let conn = session()?;
    let proxy = Proxy::new(conn, SERVICE, PATH, IFACE)?;
    let mut hints: HashMap<&str, Value> = HashMap::new();
    hints.insert("urgency", Value::from(if ok { 0u8 } else { 2u8 }));
    hints.insert("desktop-entry", Value::from("quickshort"));
    hints.insert("category", Value::from("image-added"));
    let _: u32 = proxy
        .call(
            "Notify",
            &(
                APP,
                REPLACES_ID,
                "",
                "Quickshort",
                msg,
                Vec::<&str>::new(),
                hints,
                4000i32,
            ),
        )
        .context("Notify")?;
    Ok(())
}

/// Notification id → saved capture, for `ActionInvoked` routing. Ids are
/// server-assigned; only the last few shots can still be clicked anyway.
static SHOTS: Mutex<Vec<(u32, PathBuf)>> = Mutex::new(Vec::new());
static LISTEN: Once = Once::new();
const ACT_EDIT: &str = "edit";

/// The CleanShot-style post-capture card on Linux: thumbnail (image-path
/// hint), "Edit" button, and a body click (`default`) that opens the shot
/// in Quickshort Editor.
fn send_shot_sync(msg: &str, path: &std::path::Path) -> anyhow::Result<()> {
    let conn = session()?;
    let proxy = Proxy::new(conn, SERVICE, PATH, IFACE)?;
    let uri = format!("file://{}", path.display());
    let mut hints: HashMap<&str, Value> = HashMap::new();
    hints.insert("urgency", Value::from(0u8));
    hints.insert("desktop-entry", Value::from("quickshort"));
    hints.insert("category", Value::from("image-added"));
    hints.insert("image-path", Value::from(uri.as_str()));
    let actions = vec!["default", "Open in editor", ACT_EDIT, "Edit"];
    let id: u32 = proxy
        .call(
            "Notify",
            &(
                APP,
                REPLACES_ID,
                uri.as_str(),
                "Quickshort",
                msg,
                actions,
                hints,
                8000i32,
            ),
        )
        .context("Notify")?;
    let mut shots = SHOTS.lock().unwrap();
    shots.retain(|(i, _)| *i != id);
    shots.push((id, path.to_path_buf()));
    let n = shots.len();
    if n > 16 {
        shots.drain(..n - 16);
    }
    Ok(())
}

/// One thread for the process: `ActionInvoked(id, key)` → open the editor.
fn listen() {
    LISTEN.call_once(|| {
        std::thread::spawn(|| {
            let run = || -> anyhow::Result<()> {
                let conn = session()?;
                let proxy = Proxy::new(conn, SERVICE, PATH, IFACE)?;
                for m in proxy.receive_signal("ActionInvoked")? {
                    let Ok((id, key)) = m.body().deserialize::<(u32, String)>() else {
                        continue;
                    };
                    if key != "default" && key != ACT_EDIT {
                        continue;
                    }
                    let p = SHOTS
                        .lock()
                        .unwrap()
                        .iter()
                        .find(|(i, _)| *i == id)
                        .map(|(_, p)| p.clone());
                    if let Some(p) = p {
                        eprintln!("notification: {key} → editor {}", p.display());
                        crate::actions::open_editor(&p);
                    }
                }
                Ok(())
            };
            if let Err(e) = run() {
                eprintln!("notification actions: {e:#}");
            }
        });
    });
}

/// Saved-capture notification with thumbnail + Edit action; fire-and-forget.
pub fn send_shot(msg: &str, path: PathBuf) {
    listen();
    let msg = msg.to_string();
    IN_FLIGHT.fetch_add(1, Ordering::Relaxed);
    std::thread::spawn(move || {
        if let Err(e) = send_shot_sync(&msg, &path) {
            eprintln!("notification (freedesktop): {e:#}");
        }
        IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
    });
}

static IN_FLIGHT: AtomicU32 = AtomicU32::new(0);

/// Fire-and-forget; safe from any thread.
pub fn send(msg: &str, ok: bool) {
    let msg = msg.to_string();
    IN_FLIGHT.fetch_add(1, Ordering::Relaxed);
    std::thread::spawn(move || {
        if let Err(e) = send_sync(&msg, ok) {
            eprintln!("notification (freedesktop): {e:#}");
        }
        IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
    });
}
