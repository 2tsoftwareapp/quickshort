//! Share links through the user's own Google Drive. Scope `drive.file`:
//! the app only ever sees files it created itself. Sign-in is Google's
//! installed-app flow — system browser, PKCE, loopback redirect — and the
//! refresh token lives in `gdrive.json` (mode 0600) beside `config.toml`.

use crate::share::SharedLink;
use anyhow::{bail, Context, Result};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const API: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD_URL: &str = "https://www.googleapis.com/upload/drive/v3/files";
const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
const FOLDER: &str = "Quickshort";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
/// `SharedLink::delete_key` prefix marking a Drive file id.
pub const KEY_PREFIX: &str = "gdrive:";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);

/// Desktop-app OAuth client, baked in at build time
/// (`QS_GDRIVE_CLIENT_ID` / `QS_GDRIVE_CLIENT_SECRET`) and overridable at
/// runtime. Google treats an installed app's secret as non-confidential;
/// PKCE is what protects the code exchange.
fn client() -> Option<(String, String)> {
    let get = |k: &str, baked: Option<&str>| {
        std::env::var(k)
            .ok()
            .or(baked.map(str::to_owned))
            .filter(|v| !v.is_empty())
    };
    let id = get("QS_GDRIVE_CLIENT_ID", option_env!("QS_GDRIVE_CLIENT_ID"))?;
    let secret =
        get("QS_GDRIVE_CLIENT_SECRET", option_env!("QS_GDRIVE_CLIENT_SECRET")).unwrap_or_default();
    Some((id, secret))
}

/// Whether this build can talk to Google Drive at all.
pub fn available() -> bool {
    client().is_some()
}

#[derive(Serialize, Deserialize, Default)]
struct Store {
    refresh_token: String,
    #[serde(default)]
    folder_id: String,
}

fn store_path() -> PathBuf {
    crate::config::Config::path().with_file_name("gdrive.json")
}

fn load() -> Option<Store> {
    let s = std::fs::read_to_string(store_path()).ok()?;
    serde_json::from_str::<Store>(&s)
        .ok()
        .filter(|s| !s.refresh_token.is_empty())
}

fn save(s: &Store) -> Result<()> {
    let p = store_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(&p)
        .with_context(|| format!("write {}", p.display()))?
        .write_all(serde_json::to_string(s)?.as_bytes())?;
    Ok(())
}

pub fn connected() -> bool {
    load().is_some()
}

/// Forget the account: revoke the grant at Google, drop the local token.
pub fn disconnect() {
    let s = load();
    let _ = std::fs::remove_file(store_path());
    if let Some(s) = s {
        std::thread::spawn(move || {
            let _ = ureq::post(REVOKE_URL).send_form([("token", s.refresh_token.as_str())]);
        });
    }
}

/// Upload `img` to Drive/Quickshort, make it readable by anyone with the
/// link, and return that link. Signs in first when not connected yet —
/// blocks until the browser round-trip finishes (worker thread only).
pub fn share(img: &RgbaImage) -> Result<SharedLink> {
    let mut store = match load() {
        Some(s) => s,
        None => connect()?,
    };
    let token = access_token(&store)?;
    let folder = ensure_folder(&token, &mut store)?;
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut png, image::ImageFormat::Png)
        .context("png encode failed")?;
    let name = format!(
        "quickshort-{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
    );
    let meta = serde_json::json!({ "name": name, "parents": [folder] });
    let file = upload(&token, &meta, &png.into_inner())?;
    let id = file["id"].as_str().context("drive: no file id")?.to_owned();
    ureq::post(format!("{API}/files/{id}/permissions"))
        .header("Authorization", &format!("Bearer {token}"))
        .send_json(serde_json::json!({ "type": "anyone", "role": "reader" }))
        .context("drive: link sharing refused (blocked by a Workspace admin?)")?;
    let url = file["webViewLink"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("https://drive.google.com/file/d/{id}/view"));
    Ok(SharedLink {
        url,
        delete_key: format!("{KEY_PREFIX}{id}"),
        expires_s: 0,
    })
}

/// Delete a file this app uploaded — the Drive side of "Revoke".
pub fn delete(file_id: &str) -> Result<()> {
    let store = load().context("Google Drive is not connected")?;
    let token = access_token(&store)?;
    match ureq::delete(format!("{API}/files/{file_id}"))
        .header("Authorization", &format!("Bearer {token}"))
        .call()
    {
        Ok(_) | Err(ureq::Error::StatusCode(404)) => Ok(()),
        Err(e) => Err(e).context("drive delete failed"),
    }
}

fn connect() -> Result<Store> {
    let (id, secret) = client().context("Google Drive is not set up in this build")?;
    let listener = TcpListener::bind("127.0.0.1:0").context("loopback listener")?;
    let redirect = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let verifier = b64url(&random::<32>()?);
    let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
    let state = b64url(&random::<16>()?);
    let url = format!(
        "{AUTH_URL}?client_id={}&redirect_uri={}&response_type=code&scope={}\
         &code_challenge={challenge}&code_challenge_method=S256&state={state}\
         &access_type=offline&prompt=consent",
        enc(&id),
        enc(&redirect),
        enc(SCOPE)
    );
    crate::actions::open_path(std::path::Path::new(&url));
    let code = wait_for_code(&listener, &state)?;
    let mut resp = ureq::post(TOKEN_URL)
        .send_form([
            ("code", code.as_str()),
            ("client_id", id.as_str()),
            ("client_secret", secret.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier.as_str()),
        ])
        .context("google token exchange failed")?;
    let v: serde_json::Value = resp.body_mut().read_json()?;
    let store = Store {
        refresh_token: v["refresh_token"]
            .as_str()
            .context("google returned no refresh token")?
            .to_owned(),
        folder_id: String::new(),
    };
    save(&store)?;
    Ok(store)
}

/// Serve the loopback redirect until Google sends `?code=` (or an error),
/// ignoring stray requests such as `/favicon.ico`.
fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    loop {
        let mut stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    bail!("Google sign-in timed out");
                }
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => return Err(e).context("loopback accept"),
        };
        stream.set_nonblocking(false)?;
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line)?;
        let target = line.split_whitespace().nth(1).unwrap_or("");
        let q = parse_query(target.split_once('?').map(|(_, q)| q).unwrap_or(""));
        let get = |k: &str| q.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
        let (ok, body) = match (get("code"), get("error")) {
            (Some(_), _) if get("state").as_deref() != Some(state) => {
                (Err(anyhow::anyhow!("sign-in state mismatch")), "Sign-in failed.")
            }
            (Some(c), _) => (Ok(c), "Quickshort is connected to Google Drive. You can close this tab."),
            (None, Some(e)) => (Err(anyhow::anyhow!("Google sign-in: {e}")), "Sign-in was cancelled."),
            (None, None) => {
                let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
                continue;
            }
        };
        let html = format!("<!doctype html><meta charset=utf-8><title>Quickshort</title><p style=\"font:16px sans-serif;margin:3em\">{body}</p>");
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
            html.len()
        );
        return ok;
    }
}

fn access_token(store: &Store) -> Result<String> {
    let (id, secret) = client().context("Google Drive is not set up in this build")?;
    let resp = ureq::post(TOKEN_URL).send_form([
        ("client_id", id.as_str()),
        ("client_secret", secret.as_str()),
        ("refresh_token", store.refresh_token.as_str()),
        ("grant_type", "refresh_token"),
    ]);
    match resp {
        Ok(mut r) => {
            let v: serde_json::Value = r.body_mut().read_json()?;
            v["access_token"]
                .as_str()
                .map(str::to_owned)
                .context("google returned no access token")
        }
        // revoked / expired grant — forget it so the next share signs in again
        Err(ureq::Error::StatusCode(400 | 401)) => {
            let _ = std::fs::remove_file(store_path());
            bail!("Google Drive access expired — share again to reconnect")
        }
        Err(e) => Err(e).context("google token refresh failed"),
    }
}

/// The app's own `Quickshort` folder, created on first use and re-created
/// if the user trashed it.
fn ensure_folder(token: &str, store: &mut Store) -> Result<String> {
    let auth = format!("Bearer {token}");
    if !store.folder_id.is_empty() {
        let alive = ureq::get(format!("{API}/files/{}?fields=trashed", store.folder_id))
            .header("Authorization", &auth)
            .call()
            .ok()
            .and_then(|mut r| r.body_mut().read_json::<serde_json::Value>().ok())
            .is_some_and(|v| v["trashed"] == serde_json::Value::Bool(false));
        if alive {
            return Ok(store.folder_id.clone());
        }
    }
    let mut r = ureq::post(format!("{API}/files?fields=id"))
        .header("Authorization", &auth)
        .send_json(serde_json::json!({ "name": FOLDER, "mimeType": FOLDER_MIME }))
        .context("drive: create folder failed")?;
    let v: serde_json::Value = r.body_mut().read_json()?;
    store.folder_id = v["id"].as_str().context("drive: no folder id")?.to_owned();
    save(store)?;
    Ok(store.folder_id.clone())
}

fn upload(token: &str, meta: &serde_json::Value, png: &[u8]) -> Result<serde_json::Value> {
    let boundary = format!("qs{}", b64url(&random::<12>()?).replace(['-', '_'], "x"));
    let mut body = format!(
        "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n\
         --{boundary}\r\nContent-Type: image/png\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(png);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let mut r = ureq::post(format!(
        "{UPLOAD_URL}?uploadType=multipart&fields=id,webViewLink"
    ))
    .header("Authorization", &format!("Bearer {token}"))
    .content_type(&format!("multipart/related; boundary={boundary}"))
    .send(&body[..])
    .context("drive upload failed")?;
    Ok(r.body_mut().read_json()?)
}

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|e| anyhow::anyhow!("os entropy: {e}"))?;
    Ok(b)
}

/// Unpadded base64url (RFC 4648 §5) — PKCE verifier/challenge and state.
fn b64url(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..=c.len() {
            s.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    s
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn parse_query(q: &str) -> Vec<(String, String)> {
    let dec = |s: &str| {
        let b = s.as_bytes();
        let mut out = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'+' => out.push(b' '),
                b'%' if i + 2 < b.len() => {
                    match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                        Ok(v) => {
                            out.push(v);
                            i += 2;
                        }
                        Err(_) => out.push(b'%'),
                    }
                }
                c => out.push(c),
            }
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    };
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (dec(k), dec(v))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_s256_is_unpadded_base64url_sha256() {
        let v = "dBjftJeZ4CVP-mJ92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            b64url(&Sha256::digest(v.as_bytes())),
            "ngF5GsXcbwljx6u133FFr3Xht9xooA_DuaX_3QwODtc"
        );
    }

    #[test]
    fn parses_redirect_query() {
        let q = parse_query("state=abc&code=4%2F0Ab-x_y&scope=https%3A%2F%2Fx");
        assert_eq!(q[1], ("code".into(), "4/0Ab-x_y".into()));
        assert_eq!(q[2].1, "https://x");
        assert_eq!(enc("http://127.0.0.1:9004"), "http%3A%2F%2F127.0.0.1%3A9004");
    }

    #[test]
    fn loopback_returns_code_and_skips_favicon() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let t = std::thread::spawn(move || {
            for path in ["/favicon.ico", "/?state=s1&code=4%2Fabc"] {
                let mut c = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
                write!(c, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
                let mut r = String::new();
                let _ = std::io::Read::read_to_string(&mut c, &mut r);
            }
        });
        assert_eq!(wait_for_code(&l, "s1").unwrap(), "4/abc");
        t.join().unwrap();
    }
}
