// In-app update check + download.
//
// IMPORTANT — what this can and can't do: Android will never let a
// sideloaded (non-Play-Store) app silently replace itself. This module
// gets you all the way to "here's the new APK, please confirm" — the
// final install tap is a native Android system prompt this code cannot
// skip, by OS design (same restriction every non-Play-Store app faces).
//
// Bump CURRENT_VERSION here (and APP_VERSION in App.tsx, and the version
// fields in Cargo.toml / tauri.conf.json / package.json) on every release.
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const CURRENT_VERSION: &str = "5.0.5";
const HTTP_TIMEOUT_SECS: u64 = 15;

// Replace with wherever you end up hosting update.json (a GitHub raw
// file, Firebase Storage, Vercel, etc. all work — it just needs to be a
// plain public URL that returns the JSON below).
const MANIFEST_URL: &str = "https://raw.githubusercontent.com/nexerisltd/NexPass-Update/main/update.json";

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateManifest {
    pub version: String,
    pub changelog: String,
    pub download_url: String,
}

#[derive(Serialize, Clone)]
pub struct UpdateInfo {
    pub current_version: String,
    #[serde(flatten)]
    pub manifest: UpdateManifest,
}

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build()
        .map_err(|e| e.to_string())
}

// Simple "X.Y.Z" comparator — good enough as long as releases always use
// plain numeric versions (no "-beta" suffixes etc).
fn is_newer(remote: &str, local: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> { v.split('.').filter_map(|p| p.parse().ok()).collect() };
    let (r, l) = (parse(remote), parse(local));
    for i in 0..r.len().max(l.len()) {
        let rv = r.get(i).copied().unwrap_or(0);
        let lv = l.get(i).copied().unwrap_or(0);
        if rv != lv {
            return rv > lv;
        }
    }
    false
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ReleaseNote {
    pub version: String,
    pub date: String,
    pub notes: String,
}

// Same hosting note as MANIFEST_URL above — replace with your actual
// releases.json URL once it's hosted.
const RELEASES_URL: &str = "https://raw.githubusercontent.com/nexerisltd/NexPass-Update/main/releases.json";

pub fn fetch_release_notes() -> Result<Vec<ReleaseNote>, String> {
    let client = http_client()?;
    client
        .get(RELEASES_URL)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json::<Vec<ReleaseNote>>()
        .map_err(|e| e.to_string())
}

pub fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    let client = http_client()?;
    let manifest: UpdateManifest = client
        .get(MANIFEST_URL)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;

    if is_newer(&manifest.version, CURRENT_VERSION) {
        Ok(Some(UpdateInfo { current_version: CURRENT_VERSION.to_string(), manifest }))
    } else {
        Ok(None)
    }
}

#[derive(Serialize, Clone)]
struct DownloadProgress {
    downloaded: u64,
    total: u64,
}

// Downloads the update APK into the app's cache dir (plain, unencrypted —
// it's just an installer file, no vault data), emitting
// "update-download-progress" events as it goes so the UI can show a real
// progress bar instead of a static "Downloading..." message. Returns the
// local file path — the frontend hands that to the opener plugin once
// the person taps "Install", which fires Android's native
// package-installer prompt (and, if needed, its "allow installs from
// this source" permission screen — both are OS-level and unavoidable
// for a sideloaded app).
pub fn download_update(app: &AppHandle, url: &str) -> Result<String, String> {
    let client = http_client()?;
    let mut response = client.get(url).send().map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?;
    let total = response.content_length().unwrap_or(0);

    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;
    let apk_path = cache_dir.join("nexpass-update.apk");

    let mut file = std::fs::File::create(&apk_path).map_err(|e| e.to_string())?;
    let mut buffer = [0u8; 8192];
    let mut downloaded: u64 = 0;
    loop {
        let n = response.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        downloaded += n as u64;
        let _ = app.emit("update-download-progress", DownloadProgress { downloaded, total });
    }

    Ok(apk_path.to_string_lossy().to_string())
}

// Lets the person free up storage by deleting an already-downloaded
// update APK from the app's cache. Returns true if a file was actually
// removed, false if there was nothing to delete.
pub fn delete_downloaded_apk(app: &AppHandle) -> Result<bool, String> {
    let cache_dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
    let apk_path = cache_dir.join("nexpass-update.apk");
    if apk_path.exists() {
        std::fs::remove_file(&apk_path).map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}
