// NexPass — offline favicon cache
//
// The credential list shows a little site icon next to each entry.
// Previously the frontend hit Google's favicon service directly over
// the network every time, so icons silently vanished the moment the
// phone lost signal. This module fetches the icon once per hostname
// and keeps a local copy in the app-data directory, so the same
// command returns a usable file path offline too — only the very
// first time a given site's icon is needed does it require network.
//
// Not a security-sensitive cache — a site's favicon isn't secret, so
// this deliberately lives outside the encrypted vault and isn't wiped
// by a PIN change, only by logout/uninstall.

use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Manager};

fn cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("could not resolve app data dir: {e}"))?
        .join("favicon_cache");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Cheap, dependency-free hash for a cache filename — this only needs
/// to avoid accidental collisions between hostnames, not resist a
/// deliberate attacker, so the standard library's hasher is plenty.
fn cache_filename(hostname: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    hostname.hash(&mut hasher);
    format!("{:x}.png", hasher.finish())
}

#[derive(Serialize)]
pub struct CachedIcon {
    pub path: String,
    pub from_cache: bool,
}

/// Returns a local file path for `hostname`'s favicon. If it's not
/// already cached, downloads and saves it first; if the download
/// fails (no network, or the icon host is down) but an older cached
/// copy exists, that copy is returned anyway rather than erroring —
/// that's what keeps icons showing up while offline.
pub fn get_or_fetch(app: &AppHandle, hostname: &str) -> Result<CachedIcon, String> {
    let hostname = hostname.trim();
    if hostname.is_empty() {
        return Err("empty hostname".to_string());
    }
    let path = cache_dir(app)?.join(cache_filename(hostname));

    if path.exists() {
        return Ok(CachedIcon { path: path.to_string_lossy().to_string(), from_cache: true });
    }

    match fetch_and_save(hostname, &path) {
        Ok(()) => Ok(CachedIcon { path: path.to_string_lossy().to_string(), from_cache: false }),
        Err(e) => Err(e),
    }
}

fn fetch_and_save(hostname: &str, dest: &PathBuf) -> Result<(), String> {
    let url = format!("https://www.google.com/s2/favicons?sz=64&domain={hostname}");
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(6))
        .build()
        .map_err(|e| e.to_string())?;

    let mut resp = client
        .get(&url)
        .send()
        .map_err(|e| format!("no network: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("favicon request failed: {}", resp.status()));
    }

    let mut bytes = Vec::new();
    resp.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.is_empty() {
        return Err("empty favicon response".to_string());
    }
    fs::write(dest, &bytes).map_err(|e| e.to_string())
}
