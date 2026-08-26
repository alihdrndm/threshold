//! Connecting to Google, once, in the browser the user already trusts.
//!
//! The flow is the desktop OAuth "installed app" pattern: open the consent
//! page in the real browser, catch the redirect on a loopback socket, exchange
//! the code for tokens. PKCE (S256) means the exchange is bound to this run and
//! needs no shipped secret - which is the point, because a secret compiled into
//! a desktop binary is not a secret. The client id (and optional secret) are
//! the user's own, pasted into Settings; the app ships neither.
//!
//! The loopback listener is the wall's skeleton (bind, bounded read, one shot)
//! run in reverse: the wall never answers, this one must, with a small page
//! telling the user they can close the tab.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use base64::Engine;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use windows::core::HSTRING;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::token::{self, Tokens};
use crate::db::{self, Db};

const AUTH: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const REVOKE: &str = "https://oauth2.googleapis.com/revoke";
const SCOPES: &str = "https://www.googleapis.com/auth/calendar.events \
                      https://www.googleapis.com/auth/calendar.freebusy";

/// base64url without padding, as PKCE and OAuth state both want.
fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A random URL-safe string of `n` bytes of entropy.
fn random(n: usize) -> String {
    let mut buf = vec![0u8; n];
    getrandom::fill(&mut buf).expect("system randomness");
    b64url(&buf)
}

/// A PKCE verifier and its S256 challenge.
fn pkce_pair() -> (String, String) {
    let verifier = random(32); // 43 chars, within the 43-128 RFC range
    let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", crate::popup::urlencode(k), crate::popup::urlencode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Open a URL in the default browser.
pub fn open_url(url: &str) {
    open_in_browser(url);
}

fn open_in_browser(url: &str) {
    let verb = HSTRING::from("open");
    let target = HSTRING::from(url);
    unsafe {
        ShellExecuteW(
            None,
            &verb,
            &target,
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}

/// The `?code=...&state=...` from the one redirect request. Reads just the
/// request line, bounded, then answers with a small page.
fn catch_redirect(listener: &TcpListener) -> Result<(String, String), String> {
    listener
        .set_nonblocking(false)
        .map_err(|err| format!("listener: {err}"))?;
    let (mut stream, _) = listener
        .accept()
        .map_err(|err| format!("no redirect arrived: {err}"))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));

    let mut buf = [0u8; 4096];
    let read = stream.read(&mut buf).unwrap_or(0);
    let text = String::from_utf8_lossy(&buf[..read]);
    // First line: GET /?code=...&state=... HTTP/1.1
    let target = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("");

    let body = "<!doctype html><meta charset=utf-8><title>Threshold</title>\
        <body style='font:16px system-ui;display:grid;place-items:center;height:100vh;margin:0'>\
        <p>Connected. You can close this tab.</p>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();

    let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut code = None;
    let mut state = None;
    for pair in query.split('&') {
        match pair.split_once('=') {
            Some(("code", v)) => code = Some(decode(v)),
            Some(("state", v)) => state = Some(decode(v)),
            Some(("error", v)) => return Err(format!("Google refused: {}", decode(v))),
            _ => {}
        }
    }
    match (code, state) {
        (Some(code), Some(state)) => Ok((code, state)),
        _ => Err("the redirect carried no authorization code".into()),
    }
}

/// Minimal percent-decode for the redirect query (`%XX` and `+`).
fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    out.push(b);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
}

/// Read the user's OAuth client id and optional secret from settings.
fn client_creds(app: &AppHandle) -> Result<(String, Option<String>), String> {
    let db = app.state::<Db>();
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    let id = db::get_setting(&conn, "google_client_id")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or("Paste your Google Client ID in Settings first.")?;
    let secret = db::get_setting(&conn, "google_client_secret")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok((id, secret))
}

/// Run the whole consent flow. Blocking; call from a worker thread.
pub fn connect(app: &AppHandle) -> Result<(), String> {
    let (client_id, client_secret) = client_creds(app)?;

    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|err| format!("could not open a loopback port: {err}"))?;
    let port = listener
        .local_addr()
        .map_err(|err| format!("loopback: {err}"))?
        .port();
    let redirect = format!("http://127.0.0.1:{port}");

    let (verifier, challenge) = pkce_pair();
    let state = random(16);

    let auth_url = format!(
        "{AUTH}?{}",
        form_encode(&[
            ("client_id", &client_id),
            ("redirect_uri", &redirect),
            ("response_type", "code"),
            ("scope", SCOPES),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("state", &state),
            ("access_type", "offline"),
            ("prompt", "consent"),
        ])
    );
    open_in_browser(&auth_url);

    let (code, got_state) = catch_redirect(&listener)?;
    crate::log::line("calendar: redirect caught, exchanging the code");
    if got_state != state {
        return Err("the redirect state did not match - connection aborted".into());
    }

    // Exchange the code. client_secret is included only if the user gave one.
    let mut form = vec![
        ("code", code.as_str()),
        ("client_id", client_id.as_str()),
        ("redirect_uri", redirect.as_str()),
        ("grant_type", "authorization_code"),
        ("code_verifier", verifier.as_str()),
    ];
    if let Some(secret) = client_secret.as_deref() {
        form.push(("client_secret", secret));
    }

    let http = reqwest::blocking::Client::new();
    let resp: TokenResponse = http
        .post(TOKEN)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_encode(&form))
        .send()
        .map_err(|err| format!("token exchange failed: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Google rejected the connection: {err}"))?
        .json()
        .map_err(|err| format!("token response was not understood: {err}"))?;

    let refresh = resp
        .refresh_token
        .ok_or("Google returned no refresh token - remove Threshold from your Google account's connected apps and try again")?;
    let tokens = Tokens {
        access: resp.access_token,
        refresh,
        expires_at: chrono::Utc::now().timestamp() + resp.expires_in - 60,
    };

    {
        let db = app.state::<Db>();
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        token::save(&conn, &tokens)?;
        db::set_setting(&conn, "google_connected", "1")?;
        db::set_setting(&conn, "google_last_sync_status", "Connected")?;
    }
    let _ = app.emit("calendar-status", "connected");
    crate::log::line("calendar: connected");
    Ok(())
}

/// A fresh access token from the stored refresh token. Saves the new access
/// token and expiry. On `invalid_grant` the connection is dead: clear it.
pub fn refresh(app: &AppHandle, current: &Tokens) -> Result<Tokens, String> {
    let (client_id, client_secret) = client_creds(app)?;
    let mut form = vec![
        ("client_id", client_id.as_str()),
        ("grant_type", "refresh_token"),
        ("refresh_token", current.refresh.as_str()),
    ];
    if let Some(secret) = client_secret.as_deref() {
        form.push(("client_secret", secret));
    }

    let http = reqwest::blocking::Client::new();
    let resp = http
        .post(TOKEN)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(form_encode(&form))
        .send()
        .map_err(|err| format!("token refresh failed: {err}"))?;

    if !resp.status().is_success() {
        let body = resp.text().unwrap_or_default();
        if body.contains("invalid_grant") {
            disconnect(app);
            // Say why, where the Connect button is. The common cause is not
            // the user: a Cloud project whose consent screen is still in
            // "Testing" gets refresh tokens that Google expires after 7 days.
            let why = "Google expired the connection. If the Cloud project's                        OAuth consent screen is in Testing, its tokens last 7 days -                        set the user type to Internal (or publish the app), then reconnect.";
            if let Ok(conn) = app.state::<Db>().0.lock() {
                let _ = db::set_setting(&conn, "google_last_sync_status", why);
            }
            crate::log::line(&format!("calendar: refresh rejected: {}", body.trim()));
            return Err("Google disconnected - reconnect in Settings.".into());
        }
        return Err(format!("token refresh failed: {body}"));
    }

    let parsed: TokenResponse = resp
        .json()
        .map_err(|err| format!("refresh response was not understood: {err}"))?;
    let tokens = Tokens {
        access: parsed.access_token,
        // A refresh response usually omits the refresh token; keep the old one.
        refresh: parsed.refresh_token.unwrap_or_else(|| current.refresh.clone()),
        expires_at: chrono::Utc::now().timestamp() + parsed.expires_in - 60,
    };
    let db = app.state::<Db>();
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    token::save(&conn, &tokens)?;
    Ok(tokens)
}

/// Forget the connection. Revoking is best effort - the local clear is what
/// matters, so a network failure here still disconnects.
fn clear_local(app: &AppHandle) -> Option<Tokens> {
    let db = app.state::<Db>();
    let conn = db.0.lock().ok()?;
    let t = token::load(&conn);
    let _ = token::clear(&conn);
    let _ = db::set_setting(&conn, "google_connected", "");
    let _ = db::set_setting(&conn, "google_sync_token", "");
    let _ = db::set_setting(&conn, "google_last_sync_status", "Disconnected");
    t
}

pub fn disconnect(app: &AppHandle) {
    let stored = clear_local(app);
    if let Some(tokens) = stored {
        let _ = reqwest::blocking::Client::new()
            .post(REVOKE)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_encode(&[("token", tokens.refresh.as_str())]))
            .send();
    }
    let _ = app.emit("calendar-status", "disconnected");
    crate::log::line("calendar: disconnected");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_matches_the_rfc_7636_vector() {
        // RFC 7636 Appendix B: this exact verifier hashes to this challenge.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = b64url(&Sha256::digest(verifier.as_bytes()));
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn a_generated_pair_is_well_formed() {
        let (verifier, challenge) = pkce_pair();
        assert!((43..=128).contains(&verifier.len()));
        assert_eq!(challenge, b64url(&Sha256::digest(verifier.as_bytes())));
        assert!(!challenge.contains('=') && !challenge.contains('+') && !challenge.contains('/'));
    }

    #[test]
    fn the_redirect_query_is_decoded() {
        assert_eq!(decode("a%2Fb+c"), "a/b c");
        assert_eq!(decode("4%2F0AX"), "4/0AX");
    }
}
