// NexPass — Google Sign-In
//
// Two different flows live here, picked at compile time:
//
// - Android (`android_sign_in`): the OAuth client registered for this
//   app is Google's "Android" application type, which has NO client
//   secret and does NOT accept an arbitrary http://127.0.0.1 redirect
//   (that loopback-server trick is Desktop/Web-client only — Google's
//   authorization server rejects it for Android-type clients with
//   redirect_uri_mismatch). Android-type clients instead accept the
//   fixed "reversed client id" custom-scheme redirect
//   (`com.googleusercontent.apps.<id>:/oauth2redirect`), which is what
//   this flow uses, together with PKCE (there's no client secret to
//   prove who's asking, so a code_verifier/code_challenge pair stands
//   in for it — this is the standard, Google-documented pattern for
//   public/native OAuth clients). The redirect is caught via
//   `tauri-plugin-deep-link`'s Android intent-filter (see lib.rs
//   `setup()`), not a local HTTP server — this is also what fixes
//   sign-in not working on phone browsers that won't happily hand
//   control back to a raw `http://127.0.0.1:port` URL.
//
// - Everything else (`desktop_sign_in`): unchanged from before — opens
//   the system browser and catches the redirect on a temporary local
//   loopback server, using the Desktop-type client + secret.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::secrets::{FIREBASE_API_KEY, GOOGLE_CLIENT_ID};

#[derive(Serialize, Deserialize, Clone)]
pub struct FirebaseSession {
    pub id_token: String,
    pub refresh_token: String,
    pub email: String,
    pub local_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub photo_url: Option<String>,
}

pub fn sign_in_with_google(app: &AppHandle) -> Result<FirebaseSession, String> {
    #[cfg(target_os = "android")]
    {
        android_sign_in(app)
    }
    #[cfg(not(target_os = "android"))]
    {
        desktop_sign_in(app)
    }
}

// --- Shared bits used by both flows -----------------------------------

#[derive(Deserialize)]
struct GoogleTokenResponse {
    id_token: String,
}

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct FirebaseSignInResponse {
    idToken: String,
    refreshToken: String,
    email: String,
    localId: String,
    #[serde(default)]
    displayName: Option<String>,
    #[serde(default)]
    photoUrl: Option<String>,
}

fn exchange_id_token_for_firebase_session(google_id_token: &str) -> Result<FirebaseSession, String> {
    let client = reqwest::blocking::Client::new();
    let url = format!(
        "https://identitytoolkit.googleapis.com/v1/accounts:signInWithIdp?key={}",
        FIREBASE_API_KEY
    );

    let post_body = format!("id_token={google_id_token}&providerId=google.com");
    let payload = serde_json::json!({
        "postBody": post_body,
        "requestUri": "http://localhost",
        "returnSecureToken": true
    });

    let resp = client
        .post(&url)
        .json(&payload)
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!(
            "Firebase sign-in failed: {}",
            resp.text().unwrap_or_default()
        ));
    }

    let parsed: FirebaseSignInResponse = resp.json().map_err(|e| e.to_string())?;

    Ok(FirebaseSession {
        id_token: parsed.idToken,
        refresh_token: parsed.refreshToken,
        email: parsed.email,
        local_id: parsed.localId,
        display_name: parsed.displayName,
        photo_url: parsed.photoUrl,
    })
}

fn base64_url_no_pad(bytes: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.encode(bytes)
}

// --- Android flow: custom-scheme redirect + PKCE -----------------------

#[cfg(target_os = "android")]
mod android {
    use super::*;
    use std::sync::mpsc;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    struct PendingOAuth {
        verifier: String,
        tx: mpsc::Sender<Result<String, String>>,
    }

    static PENDING: OnceLock<Mutex<Option<PendingOAuth>>> = OnceLock::new();

    fn generate_code_verifier() -> String {
        use rand::RngCore;
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        base64_url_no_pad(&bytes)
    }

    fn code_challenge_s256(verifier: &str) -> String {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(verifier.as_bytes());
        base64_url_no_pad(&digest)
    }

    /// The numeric part of the OAuth client id, e.g.
    /// "625887113006-abc...".  Android-type client redirect URIs are
    /// always this fixed "reversed client id" scheme — Google
    /// recognizes it automatically for the matching client id, no
    /// separate registration needed in the Cloud Console.
    fn android_redirect_uri() -> String {
        let numeric = GOOGLE_CLIENT_ID
            .split(".apps.googleusercontent.com")
            .next()
            .unwrap_or(GOOGLE_CLIENT_ID);
        format!("com.googleusercontent.apps.{numeric}:/oauth2redirect")
    }

    pub fn sign_in(app: &AppHandle) -> Result<FirebaseSession, String> {
        let verifier = generate_code_verifier();
        let challenge = code_challenge_s256(&verifier);
        let redirect_uri = android_redirect_uri();

        let (tx, rx) = mpsc::channel::<Result<String, String>>();
        {
            let cell = PENDING.get_or_init(|| Mutex::new(None));
            *cell
                .lock()
                .map_err(|_| "internal sign-in lock error".to_string())? =
                Some(PendingOAuth { verifier: verifier.clone(), tx });
        }

        let auth_url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&code_challenge={}&code_challenge_method=S256&prompt=select_account",
            GOOGLE_CLIENT_ID,
            urlencoding::encode(&redirect_uri),
            challenge,
        );

        app.opener()
            .open_url(auth_url, None::<&str>)
            .map_err(|e| format!("could not open browser: {e}"))?;

        // handle_redirect_url() below (called from the deep-link
        // listener registered in lib.rs's setup()) delivers the code
        // through this channel the moment Android hands the redirect
        // back to us. 5 minutes is generous slack for "switched apps,
        // came back to finish" without hanging forever if the user
        // just abandons the browser tab.
        let code = rx
            .recv_timeout(Duration::from_secs(300))
            .map_err(|_| "Sign-in timed out — please try again.".to_string())?;
        let code = code?;

        let tokens = exchange_code_for_google_tokens(&code, &redirect_uri, &verifier)?;
        exchange_id_token_for_firebase_session(&tokens.id_token)
    }

    /// Called from the `tauri-plugin-deep-link` `on_open_url` listener
    /// (see lib.rs) whenever the OS hands NexPass a
    /// `com.googleusercontent.apps.<id>:/oauth2redirect?...` URL. A
    /// no-op if no sign-in is currently in progress (e.g. the link was
    /// opened some other way, or sign-in already timed out).
    pub fn handle_redirect_url(url: &str) {
        let pending = {
            let cell = PENDING.get_or_init(|| Mutex::new(None));
            cell.lock().ok().and_then(|mut g| g.take())
        };
        let Some(pending) = pending else { return };
        let _ = pending.tx.send(extract_code(url));
    }

    fn extract_code(url: &str) -> Result<String, String> {
        let query = url.splitn(2, '?').nth(1).unwrap_or("");
        let mut code = None;
        let mut error = None;
        for pair in query.split('&') {
            let mut it = pair.splitn(2, '=');
            let key = it.next().unwrap_or("");
            let val = it.next().unwrap_or("");
            match key {
                "code" => {
                    code = Some(urlencoding::decode(val).map(|c| c.into_owned()).unwrap_or_else(|_| val.to_string()))
                }
                "error" => error = Some(val.to_string()),
                _ => {}
            }
        }
        if let Some(c) = code {
            return Ok(c);
        }
        if let Some(e) = error {
            return Err(format!("Google sign-in was cancelled ({e})"));
        }
        Err("Redirect didn't contain an authorization code".to_string())
    }

    fn exchange_code_for_google_tokens(
        code: &str,
        redirect_uri: &str,
        verifier: &str,
    ) -> Result<GoogleTokenResponse, String> {
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("code", code),
                ("client_id", GOOGLE_CLIENT_ID),
                ("redirect_uri", redirect_uri),
                ("grant_type", "authorization_code"),
                ("code_verifier", verifier),
            ])
            .send()
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!(
                "Google token exchange failed: {}",
                resp.text().unwrap_or_default()
            ));
        }

        resp.json::<GoogleTokenResponse>().map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "android")]
pub use android::handle_redirect_url;

#[cfg(target_os = "android")]
fn android_sign_in(app: &AppHandle) -> Result<FirebaseSession, String> {
    android::sign_in(app)
}

// Registered even when nothing is pending — kept as a real (not
// `#[cfg]`'d away) no-op on non-Android targets so lib.rs can call it
// unconditionally without sprinkling cfg-gates through the setup code.
#[cfg(not(target_os = "android"))]
pub fn handle_redirect_url(_url: &str) {}

// --- Desktop flow: system browser + loopback redirect -------------------
// Google blocks OAuth login inside embedded webviews, so this opens
// the user's default browser instead, catches the redirect on a
// temporary local server, then exchanges tokens with Google + Firebase.

#[cfg(not(target_os = "android"))]
mod desktop {
    use super::*;
    use crate::secrets::GOOGLE_CLIENT_SECRET;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};

    pub fn sign_in(app: &AppHandle) -> Result<FirebaseSession, String> {
        let listener = TcpListener::bind("127.0.0.1:8721")
            .map_err(|e| format!("could not bind to port 8721 — is another instance already running? ({e})"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let redirect_uri = format!("http://127.0.0.1:{port}");

        let auth_url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&access_type=offline&prompt=consent",
            GOOGLE_CLIENT_ID, redirect_uri
        );

        app.opener()
            .open_url(auth_url, None::<&str>)
            .map_err(|e| format!("could not open browser: {e}"))?;

        let code = wait_for_redirect_code(&listener)?;
        let google_tokens = exchange_code_for_google_tokens(&code, &redirect_uri)?;
        exchange_id_token_for_firebase_session(&google_tokens.id_token)
    }

    fn wait_for_redirect_code(listener: &TcpListener) -> Result<String, String> {
        let (stream, _) = listener.accept().map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .map_err(|e| e.to_string())?;

        let raw_code = request_line
            .split_whitespace()
            .nth(1)
            .and_then(|path| path.split("code=").nth(1))
            .and_then(|rest| rest.split('&').next())
            .ok_or_else(|| "no authorization code received".to_string())?;

        let code = urlencoding::decode(raw_code)
            .map_err(|e| format!("failed to decode auth code: {e}"))?
            .into_owned();

        respond_with_close_page(stream);
        Ok(code)
    }

    fn respond_with_close_page(mut stream: TcpStream) {
        let body = "<html><body style=\"font-family:sans-serif;text-align:center;margin-top:80px;\">\
            <h2>Signed in to NexPass</h2><p>You can close this tab and return to the app.</p>\
            </body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write_all(response.as_bytes());
    }

    fn exchange_code_for_google_tokens(
        code: &str,
        redirect_uri: &str,
    ) -> Result<GoogleTokenResponse, String> {
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("code", code),
                ("client_id", GOOGLE_CLIENT_ID),
                ("client_secret", GOOGLE_CLIENT_SECRET),
                ("redirect_uri", redirect_uri),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!(
                "Google token exchange failed: {}",
                resp.text().unwrap_or_default()
            ));
        }

        resp.json::<GoogleTokenResponse>().map_err(|e| e.to_string())
    }
}

#[cfg(not(target_os = "android"))]
fn desktop_sign_in(app: &AppHandle) -> Result<FirebaseSession, String> {
    desktop::sign_in(app)
}
