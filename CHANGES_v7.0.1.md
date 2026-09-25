# NexPass v7.0.1 — Android-only, mandatory account login, offline icons

This is a large release. It was written and reviewed carefully, but
**could not be compiled or run here** (no Android SDK / network access
in the environment that produced it) — please build and test it
yourself before shipping, especially the sign-in flow below, before
relying on it. See "Please test before shipping" at the bottom.

## 1. Android-only — desktop code removed
NexPass now targets Android only (desktop lives in its own repo).
Removed:
- `window-vibrancy` and the tray-icon feature/code from
  `src-tauri/Cargo.toml` and `lib.rs` (Windows Mica/Acrylic, macOS
  vibrancy, the system tray menu, "minimize to tray").
- The Windows NSIS installer section from `tauri.conf.json`.
- `tauri-plugin-nexpass-installer/examples/tauri-app/` — an unrelated
  Svelte example scaffold left over from `tauri plugin new`, not part
  of NexPass itself.
- The plugin's `androidTest`/`test` template files, which had an
  actual bug: `package com.plugin.nexpass-installer` isn't valid Kotlin
  (hyphens aren't legal in package names) — they were unused
  boilerplate, so removed rather than fixed.

## 2. Fixed the signed-APK build failure
The error from your last run —
`Project directory .../gen/android/app/src/main/java/com/nexapp/*** does not exist`
— happened because `.github/workflows/android-signed-apk.yml` called
`tauri android build` directly without ever running
`tauri android init` or the two `.github/scripts/patch_*.py` steps
first. It also wrote secrets to a `.env` file that nothing in the Rust
code reads — the app expects a real `src-tauri/src/secrets.rs` module
(see `secrets.rs.example`), so that workflow would have failed a
second time on a missing `secrets` module even after the first fix.
Both are fixed now; the workflow mirrors `build-android.yml`'s
(already-correct) sequence: init → patch → write keystore.properties →
build.

## 3. Google Sign-In rewritten for Android
Your Android-type OAuth client (`625887113006-...apps.googleusercontent.com`,
now the default in `secrets.rs.example`) has **no client secret** and
Google's server does **not** accept the old desktop-style
`redirect_uri=http://127.0.0.1:<port>` loopback flow for that client
type — that's almost certainly why sign-in wasn't completing reliably
in the browser. `google_auth.rs` now has a real Android flow:
- PKCE (`code_verifier`/`code_challenge`, no client secret needed).
- Redirect URI is the fixed, Google-recognized
  `com.googleusercontent.apps.<id>:/oauth2redirect` custom scheme.
- The redirect is caught via `tauri-plugin-deep-link`'s Android
  intent-filter (wired up in `lib.rs`'s `setup()`), not a raw TCP
  listener — this is what should fix the "some browsers won't redirect
  back" problem, since it's a real OS-level intent hand-off instead of
  hoping the browser navigates to a bare `127.0.0.1` URL.
- The old loopback flow is kept, unused, behind
  `#[cfg(not(target_os = "android"))]` in case you ever build for
  desktop again from this codebase.

**Please double-check** the `plugins.deep-link` block in
`tauri.conf.json` against whatever version of `tauri-plugin-deep-link`
actually resolves for you — I matched it to the shape I'm most
confident in, but plugin config keys do drift between versions and
this is the one part of the release I'd want you to look at first if
sign-in doesn't complete.

## 4. Mandatory login, tied to PIN — no data lost
- First-run devices now see a "Sign in with Google" screen before
  anything else — there's no way to reach the PIN screen without
  signing in first.
- After sign-in, the PIN screen does the right thing automatically:
  this logic already existed in `setup_pin` (it checks the account's
  cloud vault key) and just wasn't being reached at the right time.
  - Brand-new account → sets a new PIN.
  - Account that already has a vault on another device → must enter
    *that* PIN; a wrong one is rejected with a clear message instead
    of silently starting a second, disconnected vault.
- A device that **already** has a local vault (`vault_meta.json`
  exists) skips straight to the normal PIN screen exactly as before —
  fully offline, no re-sign-in nagging. Mandatory sign-in only ever
  happens once, the very first time a device is set up.
- **Nothing about how the vault, entries, or PIN are stored on disk
  changed** — `storage.rs`, `vault.rs`, `crypto.rs`, and `sync.rs` are
  byte-for-byte the same as before. Existing installs keep their data.

## 5. Offline credential icons
Site icons used to be plain `<img>` tags pointed straight at Google's
favicon service — invisible with no signal. New `favicon_cache.rs`
module + `get_cached_favicon` command downloads each site's icon once
and caches it in the app's local storage; the frontend now asks for
icons through that cache instead of hitting the network directly, so
previously-seen icons keep showing up offline, and refresh themselves
whenever the app is back online and asks again for a site it hasn't
cached yet.

## 6. New logo
Replaced `public/assets/icon.png` and `src-tauri/icons/*` with the new
logo. Android launcher icons (`mipmap-*`) regenerate automatically the
next time you run `tauri android init` / `tauri icon`, from
`src-tauri/icons/icon.png`.

## 7. Version bump
`6.0.1` → `7.0.1` in `package.json`, `src-tauri/Cargo.toml`,
`src-tauri/tauri.conf.json`, and `updater.rs`'s `CURRENT_VERSION`.

## Not included in this pass
- A broader visual/UX redesign — this release is scoped to auth +
  offline icons + cleanup, since those touch the riskiest code paths
  (sign-in, PIN, data storage). A follow-up UI pass is a much safer
  separate step once this one is confirmed working.
- `Cargo.lock` and `package-lock.json` are **not** included in this
  zip — they'll regenerate automatically on `cargo build` / `npm
  install`. Deleting them isn't a data-loss risk, just a lockfile.

## Please test before shipping
This could not be built or run in the environment that produced it —
please, before relying on it:
1. `npm install && npx tauri android init`, then check `tauri.conf.json`'s
   `plugins.deep-link` block matches what your installed
   `tauri-plugin-deep-link` version expects.
2. Full sign-in flow on a real device: fresh install → sign in → set
   PIN → close app → reopen → PIN-only unlock, offline.
3. Sign in with an account that already has a vault on another
   device — confirm it asks for that PIN and rejects a wrong one.
4. Turn on airplane mode after first login — confirm the vault opens
   and previously-viewed credential icons still render.
5. Existing users: install this over a real existing NexPass data
   directory and confirm all prior credentials are still there
   untouched.
