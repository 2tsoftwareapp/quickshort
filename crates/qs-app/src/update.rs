//! Self-update against GitHub Releases — unsigned build.
//!
//! Check hits `releases/latest`; a newer semver downloads and installs the
//! asset for this platform:
//!
//! - macOS: a zipped `.app` swapped over the bundle, then relaunched via
//!   `open`. Downloads go through `ureq`/plain HTTP so no quarantine xattr is
//!   set, and releases are signed with a stable self-signed identity
//!   (`packaging/macos/gen_signing_cert.sh`) so TCC grants survive the swap.
//! - Linux: APT upgrades the installed Debian package, including its editor
//!   and dependencies. Running as an AppImage replaces `$APPIMAGE` instead.
//! - Windows: the NSIS `*-Setup.exe` is launched and the app exits; the
//!   installer performs the upgrade.
//!
//! All functions block — callers run them on a worker thread.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

// pinned to the repo's current location — an old-name redirect works
// today but could be re-registered by someone else later
const REPO: &str = "2tsoftwareapp/quickshort";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

pub struct Release {
    pub version: String,
    /// asset name → download url
    assets: Vec<(String, String)>,
}

impl Release {
    fn asset(&self, pred: impl Fn(&str) -> bool) -> Option<&(String, String)> {
        self.assets.iter().find(|(n, _)| pred(n))
    }
}

/// `Some` when the latest GitHub release is newer than this binary.
pub fn check() -> Result<Option<Release>> {
    let resp = ureq::get(&format!(
        "https://api.github.com/repos/{REPO}/releases/latest"
    ))
    .header("User-Agent", "quickshort")
    .header("Accept", "application/vnd.github+json")
    .call();
    let body: Value = match resp {
        Ok(mut r) => r
            .body_mut()
            .read_json()
            .context("release reply is not JSON")?,
        // 404 = no release published yet — that's "up to date", not an error
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(e) => return Err(e).context("release check failed"),
    };

    let tag = body
        .get("tag_name")
        .and_then(|v| v.as_str())
        .context("release reply: no tag_name")?;
    let ver = tag.trim_start_matches('v');
    if !is_newer(ver, CURRENT) {
        return Ok(None);
    }

    let assets: Vec<(String, String)> = body
        .get("assets")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let n = x.get("name")?.as_str()?.to_string();
                    let u = x.get("browser_download_url")?.as_str()?.to_string();
                    Some((n, u))
                })
                .collect()
        })
        .context("release reply: no assets")?;

    // bail early when this platform's asset is missing — an update we can't
    // install must not be offered
    #[cfg(target_os = "macos")]
    let ok = assets.iter().any(|(n, _)| n.ends_with("-macos.zip"));
    #[cfg(target_os = "linux")]
    let ok = assets
        .iter()
        .any(|(n, _)| n == "quickshort_amd64.deb" || n.ends_with(".AppImage"));
    #[cfg(target_os = "windows")]
    let ok = assets
        .iter()
        .any(|(n, _)| n.ends_with("Setup.exe") || n.ends_with("setup.exe"));
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let ok = false;
    if !ok {
        bail!("release {ver} has no asset for this platform");
    }

    Ok(Some(Release {
        version: ver.to_string(),
        assets,
    }))
}

fn download(url: &str, dest: &Path) -> Result<()> {
    let mut resp = ureq::get(url)
        .header("User-Agent", "quickshort")
        .call()
        .context("download failed")?;
    let mut f = std::fs::File::create(dest)?;
    std::io::copy(
        &mut std::io::Read::take(resp.body_mut().as_reader(), 64 * 1024 * 1024),
        &mut f,
    )
    .context("download write failed")?;
    Ok(())
}

/// Ed25519 public key the release workflow signs every asset with
/// (`<asset>.sig` sidecars, base64 detached signature over the file).
/// A compromised release can't forge a matching signature — unlike the
/// `.sha256` sidecars, which travel the same channel as the payload and
/// only catch corruption.
const SIGNING_PUB_B64: &str = "f/ixPPD/EcyCrWSTGr8iTiaNT56p7+4UWffiYt6eg54=";

/// Base64 detached ed25519 signature for `asset_name`, from its `.sig`
/// asset. `None` when the release ships no signature for it.
fn expected_sig(rel: &Release, asset_name: &str) -> Result<Option<String>> {
    let sig_name = format!("{asset_name}.sig");
    let Some((_, url)) = rel.asset(|n| n == sig_name) else {
        return Ok(None);
    };
    let body: String = ureq::get(url)
        .call()
        .and_then(|mut r| r.body_mut().read_to_string())
        .context("signature asset fetch failed")?;
    Ok(Some(body.trim().to_string()))
}

fn verify_sig_with(pub_b64: &str, file: &Path, sig_b64: &str) -> Result<()> {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;
    let pub_raw = b64.decode(pub_b64).context("bad embedded public key")?;
    let key = ed25519_dalek::VerifyingKey::from_bytes(
        pub_raw.as_slice().try_into().context("bad embedded public key")?,
    )
    .context("bad embedded public key")?;
    let sig_raw = b64.decode(sig_b64.trim()).context("bad signature encoding")?;
    let sig_arr: [u8; 64] = sig_raw
        .as_slice()
        .try_into()
        .context("bad signature length")?;
    let sig = ed25519_dalek::Signature::from_bytes(&sig_arr);
    let data = std::fs::read(file)?;
    use ed25519_dalek::Verifier;
    key.verify(&data, &sig)
        .context("signature verification failed")
}

/// Check `file` against the release's ed25519 `.sig` asset. A release
/// without one is refused: the `.sha256` sidecars ship through the same
/// channel as the payload, so whoever can swap the payload can swap them.
fn verify(rel: &Release, file: &Path, asset_name: &str) -> Result<()> {
    let sig = expected_sig(rel, asset_name)?
        .with_context(|| format!("release has no signature for {asset_name}"))?;
    verify_sig_with(SIGNING_PUB_B64, file, &sig)
}

/// Download the release, swap it over the running `.app`, relaunch.
/// Returns the new app path; the caller should exit its event loop.
#[cfg(target_os = "macos")]
pub fn install(rel: &Release) -> Result<PathBuf> {
    let app = current_bundle().context("not running from a .app bundle")?;
    let tmp = std::env::temp_dir().join("quickshort-update");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;

    let (name, url) = rel
        .asset(|n| n.ends_with("-macos.zip"))
        .context("release has no *-macos.zip asset")?;
    let zip = tmp.join(name);
    download(url, &zip)?;
    verify(rel, &zip, name)?;

    if !std::process::Command::new("/usr/bin/ditto")
        .args(["-xk"])
        .arg(&zip)
        .arg(&tmp)
        .status()
        .context("unzip failed")?
        .success()
    {
        bail!("unzip failed");
    }

    let new_app = tmp.join("Quickshort.app");
    for bin in ["quickshort", "qs-editor"] {
        if !new_app.join("Contents/MacOS").join(bin).is_file() {
            bail!("update archive is missing Quickshort.app/Contents/MacOS/{bin}");
        }
    }

    // swap: move the old bundle aside, drop the new one in place
    let backup = app.with_extension(format!("old-{}", unix_ts()?));
    std::fs::rename(&app, &backup).context("can't move the old app aside")?;
    match std::fs::rename(&new_app, &app) {
        Ok(()) => {
            let _ = std::fs::remove_dir_all(&backup);
        }
        Err(e) => {
            let _ = std::fs::rename(&backup, &app); // roll back
            return Err(e).context("can't install the new app");
        }
    }
    Ok(app)
}

/// Upgrade the Debian package or replace the whole AppImage.
#[cfg(target_os = "linux")]
pub fn install(rel: &Release) -> Result<PathBuf> {
    let tmp = tempfile::Builder::new()
        .prefix("quickshort-update-")
        .tempdir()?;

    // AppImage: the whole app is one file — download and replace it
    if let Ok(appimage) = std::env::var("APPIMAGE") {
        let (name, url) = rel
            .asset(|n| n.ends_with(".AppImage"))
            .context("release has no AppImage asset")?;
        let f = tmp.path().join(name);
        download(url, &f)?;
        verify(rel, &f, name)?;
        set_exec(&f)?;
        // stage next to the target so rename stays on one filesystem
        let staged = PathBuf::from(&appimage).with_extension("new");
        std::fs::copy(&f, &staged).context("can't stage AppImage")?;
        std::fs::rename(&staged, &appimage).context("can't replace AppImage")?;
        return Ok(PathBuf::from(appimage));
    }

    let exe = std::env::current_exe().context("can't locate current exe")?;
    if exe != Path::new("/usr/bin/quickshort") {
        bail!("Debian package upgrades require /usr/bin/quickshort; install the .deb manually");
    }
    let (name, url) = rel
        .asset(|n| n == "quickshort_amd64.deb")
        .context("release has no linux .deb asset")?;
    let deb = tmp.path().join(name);
    download(url, &deb)?;
    verify(rel, &deb, name)?;
    let status = std::process::Command::new("/usr/bin/pkexec")
        .args(["/usr/bin/apt-get", "--yes", "--no-remove", "install"])
        .arg(&deb)
        .status()
        .context("could not start the Debian package upgrade; install the .deb manually")?;
    if !status.success() {
        bail!("Debian package upgrade failed ({status}); install the .deb manually");
    }
    Ok(exe)
}

/// Download the NSIS installer and run it; the caller then exits and the
/// installer performs the upgrade.
#[cfg(target_os = "windows")]
pub fn install(rel: &Release) -> Result<PathBuf> {
    let (name, url) = rel
        .asset(|n| n.ends_with("Setup.exe") || n.ends_with("setup.exe"))
        .context("release has no windows installer asset")?;
    let setup = std::env::temp_dir().join(name);
    download(url, &setup)?;
    verify(rel, &setup, name)?;
    std::process::Command::new(&setup)
        .spawn()
        .context("can't launch installer")?;
    Ok(std::env::current_exe()?)
}

/// Relaunch the freshly installed app; the current process must exit.
/// Windows relaunches itself from the NSIS installer — nothing to do.
#[cfg(target_os = "macos")]
pub fn relaunch(app: &Path) -> Result<()> {
    std::process::Command::new("open")
        .arg("-n")
        .arg(app)
        .spawn()
        .context("relaunch failed")?;
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn relaunch(exe: &Path) -> Result<()> {
    std::process::Command::new(exe)
        .spawn()
        .context("relaunch failed")?;
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn relaunch(_exe: &Path) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_exec(p: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).context("chmod failed")
}

/// `/Applications/Quickshort.app` (or wherever the bundle lives), walking up
/// from the executable. `None` when running an unbundled binary (dev builds).
#[cfg(target_os = "macos")]
fn current_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(|p| p.to_path_buf())
}

#[cfg(target_os = "macos")]
fn unix_ts() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs())
}

/// Loose semver compare: "1.10.0" > "1.9.9"; non-numeric suffixes ignored.
fn is_newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> [u64; 3] {
        let mut out = [0u64; 3];
        for (i, seg) in v.split('.').take(3).enumerate() {
            let digits: String = seg.chars().take_while(|c| c.is_ascii_digit()).collect();
            out[i] = digits.parse().unwrap_or(0);
        }
        out
    };
    parse(a) > parse(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_compare() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(is_newer("1.0.0-beta", "0.9.9"));
    }

    #[test]
    fn signature_roundtrip() {
        use base64::Engine;
        use ed25519_dalek::Signer;
        let b64 = base64::engine::general_purpose::STANDARD;
        let sk = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let pub_b64 = b64.encode(sk.verifying_key().to_bytes());
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("asset.bin");
        std::fs::write(&f, b"release payload").unwrap();
        let sig_b64 = b64.encode(sk.sign(b"release payload").to_bytes());
        verify_sig_with(&pub_b64, &f, &sig_b64).unwrap();
        std::fs::write(&f, b"tampered payload").unwrap();
        assert!(verify_sig_with(&pub_b64, &f, &sig_b64).is_err());
    }

    #[test]
    fn release_without_signature_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("quickshort_amd64.deb");
        std::fs::write(&f, b"payload").unwrap();
        let rel = Release {
            version: "9.9.9".into(),
            assets: vec![
                ("quickshort_amd64.deb".into(), "https://invalid/deb".into()),
                ("linux.sha256".into(), "https://invalid/sha".into()),
            ],
        };
        let err = verify(&rel, &f, "quickshort_amd64.deb").unwrap_err();
        assert!(err.to_string().contains("no signature"), "{err:#}");
    }

    /// Live check — `cargo test -p qs-app -- --ignored`
    #[test]
    #[ignore]
    fn live_release_check() {
        let r = check().unwrap();
        log::info!("latest release: {r:?}", r = r.map(|r| r.version));
    }
}
