//! Where the Google tokens live.
//!
//! Refresh tokens are long-lived keys to a calendar; they must not sit in the
//! database as plain text. Windows DPAPI (`CryptProtectData`) encrypts them to
//! the logged-in user account, so the blob is useless if the file is copied to
//! another machine or read by another user, and there is no key for this app to
//! manage or leak. The protected blob is base64'd into the one settings row
//! `google_token` - the same table as everything else, just unreadable.

use base64::Engine;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use windows::Win32::Foundation::LocalFree;
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
};

use crate::db;

const KEY: &str = "google_token";

/// A Google token set as stored: what we call to read the calendar, what we
/// call to get a fresh access token, and when the access token dies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    /// Unix seconds. Refreshed a minute early so a call never races expiry.
    pub expires_at: i64,
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// Encrypt to the current user with DPAPI.
fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr() as *mut u8,
    };
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(&input, None, None, None, None, 0, &mut out)
            .map_err(|err| format!("could not protect the token: {err}"))?;
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(windows::Win32::Foundation::HLOCAL(out.pbData as *mut _)));
        Ok(bytes)
    }
}

fn unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: blob.len() as u32,
        pbData: blob.as_ptr() as *mut u8,
    };
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(&input, None, None, None, None, 0, &mut out)
            .map_err(|err| format!("could not read the stored token: {err}"))?;
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(windows::Win32::Foundation::HLOCAL(out.pbData as *mut _)));
        Ok(bytes)
    }
}

pub fn save(conn: &Connection, tokens: &Tokens) -> Result<(), String> {
    let json = serde_json::to_vec(tokens).map_err(|err| format!("could not encode token: {err}"))?;
    let blob = protect(&json)?;
    db::set_setting(conn, KEY, &b64().encode(blob))
}

pub fn load(conn: &Connection) -> Option<Tokens> {
    let raw = db::get_setting(conn, KEY)?;
    let blob = b64().decode(raw.trim()).ok()?;
    let json = unprotect(&blob).ok()?;
    serde_json::from_slice(&json).ok()
}

pub fn clear(conn: &Connection) -> Result<(), String> {
    // An empty value rather than a delete: set_setting is an upsert, and the
    // read side treats empty as absent (decode of "" fails, load returns None).
    db::set_setting(conn, KEY, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_survives_a_protect_unprotect_round_trip() {
        // DPAPI is available on the test host (Windows), so this exercises the
        // real round trip, not a mock.
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate_for_tests(&conn);
        assert!(load(&conn).is_none());

        let tokens = Tokens {
            access: "ya29.example".into(),
            refresh: "1//refresh".into(),
            expires_at: 1_700_000_000,
        };
        save(&conn, &tokens).unwrap();

        // The stored value must not contain the plaintext token.
        let stored = db::get_setting(&conn, KEY).unwrap();
        assert!(!stored.contains("ya29.example"));

        let back = load(&conn).unwrap();
        assert_eq!(back.access, "ya29.example");
        assert_eq!(back.refresh, "1//refresh");
        assert_eq!(back.expires_at, 1_700_000_000);

        clear(&conn).unwrap();
        assert!(load(&conn).is_none());
    }
}
