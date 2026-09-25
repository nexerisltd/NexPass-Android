# NexPass

v7.0.1 · NexApp · Developed by Arabi Islam, MR. ARX

A secure Android credential vault built with Tauri (Rust) + React.
Local-only encrypted vault with mandatory Google sign-in + PIN unlock,
optional Firestore cloud sync, and offline-friendly credential icons.
(Desktop lives in its own separate repo now.)

## Before you build this

1. **Secrets**: copy `src-tauri/src/secrets.rs.example` to
   `src-tauri/src/secrets.rs` (git-ignored, never commit it) and fill
   in your Firebase Web API key. The Android OAuth client ID is
   already filled in. If you build via CI instead, set the
   `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` (can be blank — Android
   clients don't have one; only the old desktop flow reads it), and
   `FIREBASE_API_KEY` GitHub Secrets — see `.github/workflows/`.
2. **Google Cloud Console**: the OAuth client must be an **Android**
   application type, registered with this app's package name
   (`com.nexapp.nexpass`) and your release keystore's SHA-1
   fingerprint. See `src-tauri/src/google_auth.rs` for why this
   matters (Android-type clients use a fixed custom-scheme redirect +
   PKCE, not the old loopback-server flow).
3. **Icon**: `src-tauri/icons/icon.png` and `public/assets/icon.png`
   already have the new logo. To regenerate every platform-specific
   size after changing it, run:
   ```
   npm install -g @tauri-apps/cli
   tauri icon src-tauri/icons/icon.png
   ```
4. **Prerequisites**: Node.js 18+, Rust (via `rustup`), and the
   Android SDK/NDK — see https://v2.tauri.app/start/prerequisites/.

## Getting it running

```bash
npm install
npx tauri android init
npx tauri android dev
```

First run on a fresh device: you'll be asked to sign in with Google
before anything else, then set a 6-digit PIN. After that, NexPass only
ever asks for the PIN — fully offline-capable — until you log out.

## What's here vs. what's next

- ✅ Mandatory Google sign-in on first setup, PIN tied to the account
  (a second device signing into the same account is required to enter
  that account's existing PIN, not create a new disconnected vault)
- ✅ Crypto module (`src-tauri/src/crypto.rs`): Argon2id key derivation + AES-256-GCM encrypt/decrypt
- ✅ Storage module (`src-tauri/src/storage.rs`) + SQLite vault (`vault.rs`)
- ✅ Firestore sync (`src-tauri/src/sync.rs`): batched push/pull, smart-sync change detection
- ✅ Offline-cached credential icons (`favicon_cache.rs`)
- ✅ Biometric unlock, in-app update check/download/install, data export/import
- ⬜ Sidebar categories beyond the current grid — only flat category filtering exists
- ⬜ Browser autofill extension — later version

See `CHANGES_v7.0.1.md` for the full list of what changed in this
release and what to test before shipping it.

## Where the vault data lives

Everything is under the OS app-data directory (Android:
`/data/data/com.nexapp.nexpass/files/`, not directly accessible without
root): `vault_meta.json` (PIN salt + verification hash only — never the
PIN or the encryption key), `vault.sqlite3` (all fields AES-256-GCM
encrypted, keyed by the PIN-derived key), `google_session.json`,
`sync_meta.json`, `favicon_cache/`, `settings.json`, `profile.json`.
