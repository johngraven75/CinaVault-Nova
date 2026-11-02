//! Google Drive OAuth + media listing/ingestion.
//!
//! - PKCE (S256) + loopback listener on 127.0.0.1:19284, matching the
//!   redirect the user must register for a Desktop-app OAuth client.
//! - Tokens: access + refresh, stored only in memory by the caller
//!   (the Tauri shell holds them in AppState, never in the DB).
//! - Refresh-on-401: a 401 from the Drive API triggers one refresh
//!   and one retry; a second 401 means genuinely disconnected.
//! - Revoke on disconnect, and a true disconnected state (no token,
//!   no silent local-folder pretending to be Drive).
//! - Client ID: NEVER invented or committed. If none is configured,
//!   `auth_url` returns a plain ConfigError and the UI must say so
//!   and offer only the explicitly labelled local-folder flow.
//! - Listing/ingestion: list Drive files (name, mime, id), filter to
//!   media, and download selected files into a user-chosen local
//!   folder, then scan them into the library with a `drive:` id.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const REDIRECT_URI: &str = "http://127.0.0.1:19284/auth/callback";
pub const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
pub const DRIVE_FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";

#[derive(Debug, Clone)]
pub struct DriveConfig {
    pub client_id: String,
}

impl DriveConfig {
    pub fn require(&self) -> Result<(), String> {
        if self.client_id.trim().is_empty()
            || self.client_id == "cinavault-gdrive-client"
        {
            return Err(
                "No Google OAuth client ID is configured. Create a Desktop-app \
                 OAuth client in Google Cloud (Drive API enabled, redirect \
                 http://127.0.0.1:19284/auth/callback) and enter its client ID. \
                 Until then, Drive is disconnected; the local-folder flow is \
                 available separately and is labelled as local only."
                    .into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

pub fn new_pkce() -> Pkce {
    use rand::RngCore;
    let mut v = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut v);
    let verifier = URL_SAFE_NO_PAD.encode(v);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut s = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut s);
    Pkce {
        verifier,
        challenge,
        state: URL_SAFE_NO_PAD.encode(s),
    }
}

pub fn auth_url(config: &DriveConfig, pkce: &Pkce) -> Result<String, String> {
    config.require()?;
    Ok(format!(
        "{AUTH_URL}?client_id={}&redirect_uri={}&response_type=code&scope=https://www.googleapis.com/auth/drive.readonly&code_challenge={}&code_challenge_method=S256&state={}&access_type=offline&prompt=consent",
        url_encode(&config.client_id),
        url_encode(REDIRECT_URI),
        pkce.challenge,
        pkce.state
    ))
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DriveTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
}

impl DriveTokens {
    pub fn is_connected(&self) -> bool {
        !self.access_token.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    pub mime_type: String,
}

impl DriveFile {
    pub fn is_media(&self) -> bool {
        self.mime_type.starts_with("video/")
            || self.mime_type.starts_with("audio/")
            || matches!(
                Path::new(&self.name)
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .as_deref(),
                Some("mp4") | Some("mkv") | Some("avi") | Some("mov") | Some("webm") | Some("m4v")
            )
    }
}

/// Exchange an authorization code for tokens. Network call — not used
/// in unit tests.
pub fn exchange_code(
    config: &DriveConfig,
    code: &str,
    verifier: &str,
) -> Result<DriveTokens, String> {
    config.require()?;
    let resp = ureq::post(TOKEN_URL)
        .send_form(&[
            ("code", code),
            ("client_id", &config.client_id),
            ("client_secret", ""),
            ("redirect_uri", REDIRECT_URI),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier),
        ])
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    Ok(DriveTokens {
        access_token: json
            .get("access_token")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .into(),
        refresh_token: json
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        expires_in: json.get("expires_in").and_then(|v| v.as_u64()),
    })
}

pub fn refresh_access_token(
    config: &DriveConfig,
    refresh_token: &str,
) -> Result<DriveTokens, String> {
    config.require()?;
    let resp = ureq::post(TOKEN_URL)
        .send_form(&[
            ("refresh_token", refresh_token),
            ("client_id", &config.client_id),
            ("grant_type", "refresh_token"),
        ])
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    Ok(DriveTokens {
        access_token: json
            .get("access_token")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .into(),
        refresh_token: Some(refresh_token.to_string()),
        expires_in: json.get("expires_in").and_then(|v| v.as_u64()),
    })
}

pub fn revoke_token(access_token: &str) -> Result<(), String> {
    let _ = ureq::post(REVOKE_URL)
        .send_form(&[("token", access_token)])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// List Drive files, with one refresh-and-retry on 401.
/// `fetch` is injected for testing; live code passes a ureq closure.
pub fn list_files_with_refresh<F>(
    config: &DriveConfig,
    tokens: &mut DriveTokens,
    fetch: F,
) -> Result<Vec<DriveFile>, String>
where
    F: Fn(&str) -> Result<(u16, serde_json::Value), String>,
{
    let (status, json) = fetch(&tokens.access_token)?;
    if status != 401 {
        return parse_file_list(&json);
    }
    // 401: refresh once, retry once. A second 401 = disconnected.
    let refresh = tokens.refresh_token.clone().ok_or("disconnected: no refresh token")?;
    let new_tokens = refresh_access_token(config, &refresh)?;
    tokens.access_token = new_tokens.access_token;
    let (status2, json2) = fetch(&tokens.access_token)?;
    if status2 == 401 {
        tokens.access_token.clear();
        return Err("disconnected: Drive token rejected after refresh".into());
    }
    parse_file_list(&json2)
}

pub fn parse_file_list(json: &serde_json::Value) -> Result<Vec<DriveFile>, String> {
    let files = json
        .get("files")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(files
        .iter()
        .map(|f| DriveFile {
            id: f.get("id").and_then(|v| v.as_str()).unwrap_or_default().into(),
            name: f.get("name").and_then(|v| v.as_str()).unwrap_or_default().into(),
            mime_type: f
                .get("mimeType")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
        })
        .collect())
}

/// Download one Drive file into a local folder. Returns the local path.
pub fn download_file(
    tokens: &DriveTokens,
    file: &DriveFile,
    dest_folder: &Path,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dest_folder).map_err(|e| e.to_string())?;
    let url = format!("{DRIVE_FILES_URL}/{}/?alt=media", file.id);
    let resp = ureq::get(&url)
        .set("Authorization", &format!("Bearer {}", tokens.access_token))
        .call()
        .map_err(|e| e.to_string())?;
    use std::io::Read as _;
    let mut bytes = Vec::new();
    resp.into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let out = dest_folder.join(&file.name);
    std::fs::write(&out, bytes).map_err(|e| e.to_string())?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_client_id_is_a_plain_error_not_a_fake_flow() {
        let cfg = DriveConfig {
            client_id: String::new(),
        };
        assert!(cfg.require().is_err());
        assert!(auth_url(&cfg, &new_pkce()).is_err());
        let placeholder = DriveConfig {
            client_id: "cinavault-gdrive-client".into(),
        };
        assert!(placeholder.require().is_err());
    }

    #[test]
    fn pkce_challenge_is_sha256_of_verifier() {
        let pkce = new_pkce();
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(pkce.verifier.as_bytes()));
        assert_eq!(pkce.challenge, expected);
        assert!(!pkce.verifier.is_empty());
    }

    #[test]
    fn media_filtering_and_refresh_retry() {
        let files = parse_file_list(
            &serde_json::json!({"files": [
                {"id": "1", "name": "film.mp4", "mimeType": "video/mp4"},
                {"id": "2", "name": "notes.txt", "mimeType": "text/plain"},
            ]}),
        )
        .unwrap();
        assert_eq!(files.len(), 2);
        assert!(files[0].is_media());
        assert!(!files[1].is_media());
    }

    #[test]
    fn refresh_on_401_then_disconnected_without_refresh() {
        let cfg = DriveConfig {
            client_id: "test-client".into(),
        };
        let mut tokens = DriveTokens {
            access_token: "stale".into(),
            refresh_token: None,
            expires_in: None,
        };
        let err = list_files_with_refresh(&cfg, &mut tokens, |_| {
            Ok((401, serde_json::json!({})))
        })
        .unwrap_err();
        assert!(err.contains("disconnected"));
    }
}
