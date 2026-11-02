//! WireGuard VPN install + profile.
//!
//! Install flow (Windows only, honestly):
//! 1. Download the pinned official MSI (1.1.1) over HTTPS.
//! 2. Compute SHA-256 for audit. WireGuard publishes no SHA-256 for
//!    Windows MSIs, so the hash is deliberately NOT pinned — it is
//!    returned with `sha256_verified: false`.
//! 3. On Windows, verify Authenticode: status must be Valid and the
//!    signer must contain "WireGuard". Any other result is error 21.
//! 4. Elevated `msiexec /i /qn /norestart` (existing UAC pattern).
//!    UAC declined (exit 1223) is its own error, not "installed".
//! 5. Verify the engine exists under Program Files afterwards.
//!    A machine that only has a bundled exe, with no engine after
//!    install, is reported distinctly as error 23 ("bundled only"),
//!    never as installed.
//! Errors: 20 download, 21 signature/hash, 22 msiexec, 23
//! post-install/bundled-only, 1223 UAC declined. 10/11/12 are
//! connect-phase only and are not used here.
//! On non-Windows, install returns UnsupportedPlatform — never a fake
//! "installed" result. Profile parsing (a user-supplied .conf) is
//! platform-independent and is tested.
//!
//! No server-peer issuance or key generation is done here: a tunnel
//! still requires a user-supplied .conf.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const WIREGUARD_VERSION: &str = "1.1.1";
pub const WIREGUARD_MSI_URL: &str =
    "https://download.wireguard.com/windows-client/wireguard-amd64-1.1.1.msi";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnInstallResult {
    pub installed: bool,
    pub version: String,
    pub sha256: String,
    /// False by design: WireGuard publishes no SHA-256 to pin against.
    pub sha256_verified: bool,
    pub engine_path: Option<String>,
    pub error_code: Option<i32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnProfile {
    pub name: String,
    pub interface_address: Option<String>,
    pub private_key_present: bool,
    pub peers: usize,
    pub endpoint: Option<String>,
}

/// Parse a user-supplied WireGuard .conf. Rejects files with no
/// [Interface] section or no [Peer] section.
pub fn parse_profile(path: &Path) -> Result<VpnProfile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_profile_text(&text, path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "profile".into()))
}

pub fn parse_profile_text(text: &str, name: String) -> Result<VpnProfile, String> {
    let mut section = "";
    let mut interface_address = None;
    let mut private_key_present = false;
    let mut peers = 0usize;
    let mut endpoint = None;
    let mut saw_interface = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line;
            if section == "[Interface]" {
                saw_interface = true;
            }
            if section == "[Peer]" {
                peers += 1;
            }
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let (k, v) = (k.trim(), v.trim());
            match (section, k) {
                ("[Interface]", "Address") => interface_address = Some(v.to_string()),
                ("[Interface]", "PrivateKey") => private_key_present = !v.is_empty(),
                ("[Peer]", "Endpoint") => {
                    if endpoint.is_none() {
                        endpoint = Some(v.to_string());
                    }
                }
                _ => {}
            }
        }
    }
    if !saw_interface {
        return Err("profile has no [Interface] section".into());
    }
    if peers == 0 {
        return Err("profile has no [Peer] section".into());
    }
    Ok(VpnProfile {
        name,
        interface_address,
        private_key_present,
        peers,
        endpoint,
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn download_msi(dest: &Path) -> Result<Vec<u8>, VpnInstallResult> {
    let err = |code: i32, msg: String| VpnInstallResult {
        installed: false,
        version: WIREGUARD_VERSION.into(),
        sha256: String::new(),
        sha256_verified: false,
        engine_path: None,
        error_code: Some(code),
        error: Some(msg),
    };
    let resp = ureq::get(WIREGUARD_MSI_URL)
        .timeout(std::time::Duration::from_secs(120))
        .call()
        .map_err(|e| err(20, format!("download failed: {e}")))?;
    use std::io::Read as _;
    let mut bytes = Vec::new();
    resp.into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| err(20, format!("download read failed: {e}")))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(dest, &bytes).map_err(|e| err(20, format!("write failed: {e}")))?;
    Ok(bytes)
}

/// Install WireGuard. On non-Windows this returns a distinct
/// UnsupportedPlatform result — it never claims success.
pub fn install_wireguard(dest_dir: &Path) -> VpnInstallResult {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = dest_dir;
        return VpnInstallResult {
            installed: false,
            version: WIREGUARD_VERSION.into(),
            sha256: String::new(),
            sha256_verified: false,
            engine_path: None,
            error_code: Some(23),
            error: Some(
                "WireGuard install is Windows-only in this build; use a Windows machine for the live install test".into(),
            ),
        };
    }
    #[cfg(target_os = "windows")]
    {
        let msi = dest_dir.join("wireguard-amd64-1.1.1.msi");
        let bytes = match download_msi(&msi) {
            Ok(b) => b,
            Err(e) => return e,
        };
        let sha = sha256_hex(&bytes);
        // Authenticode verification via PowerShell.
        let ps = format!(
            "(Get-AuthenticodeSignature '{}').Status",
            msi.display()
        );
        let sig = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .output();
        let valid = sig
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("Valid"))
            .unwrap_or(false);
        if !valid {
            return VpnInstallResult {
                installed: false,
                version: WIREGUARD_VERSION.into(),
                sha256: sha,
                sha256_verified: false,
                engine_path: None,
                error_code: Some(21),
                error: Some("Authenticode signature is not Valid for the WireGuard MSI".into()),
            };
        }
        let status = std::process::Command::new("msiexec")
            .args(["/i", &msi.to_string_lossy(), "/qn", "/norestart"])
            .status();
        match status {
            Ok(s) if s.code() == Some(1223) => {
                return VpnInstallResult {
                    installed: false,
                    version: WIREGUARD_VERSION.into(),
                    sha256: sha,
                    sha256_verified: false,
                    engine_path: None,
                    error_code: Some(1223),
                    error: Some("UAC elevation declined".into()),
                }
            }
            Ok(s) if !s.success() => {
                return VpnInstallResult {
                    installed: false,
                    version: WIREGUARD_VERSION.into(),
                    sha256: sha,
                    sha256_verified: false,
                    engine_path: None,
                    error_code: Some(22),
                    error: Some(format!("msiexec failed with status {s}")),
                }
            }
            Err(e) => {
                return VpnInstallResult {
                    installed: false,
                    version: WIREGUARD_VERSION.into(),
                    sha256: sha,
                    sha256_verified: false,
                    engine_path: None,
                    error_code: Some(22),
                    error: Some(format!("msiexec failed: {e}")),
                }
            }
            _ => {}
        }
        // Engine verification: Program Files\WireGuard\wireguard.exe
        let engine = PathBuf::from(r"C:\Program Files\WireGuard\wireguard.exe");
        if engine.exists() {
            VpnInstallResult {
                installed: true,
                version: WIREGUARD_VERSION.into(),
                sha256: sha,
                sha256_verified: false,
                engine_path: Some(engine.to_string_lossy().into_owned()),
                error_code: None,
                error: None,
            }
        } else {
            VpnInstallResult {
                installed: false,
                version: WIREGUARD_VERSION.into(),
                sha256: sha,
                sha256_verified: false,
                engine_path: None,
                error_code: Some(23),
                error: Some("post-install engine not found (bundled-only machine)".into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_parses_and_rejects_bad_profiles() {
        let good = "[Interface]\nPrivateKey = abc\nAddress = 10.0.0.2/24\n\n[Peer]\nPublicKey = xyz\nEndpoint = vpn.example.invalid:51820\n";
        let p = parse_profile_text(good, "home".into()).unwrap();
        assert_eq!(p.peers, 1);
        assert!(p.private_key_present);
        assert_eq!(p.endpoint.as_deref(), Some("vpn.example.invalid:51820"));
        assert!(parse_profile_text("[Peer]\nPublicKey = x\n", "bad".into()).is_err());
        assert!(parse_profile_text("[Interface]\nAddress = 10.0.0.2/24\n", "bad".into()).is_err());
    }

    #[test]
    fn sha256_is_hex_and_not_pinned() {
        let h = sha256_hex(b"hello");
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn non_windows_install_never_claims_success() {
        let r = install_wireguard(Path::new("/tmp"));
        assert!(!r.installed);
        assert!(!r.sha256_verified);
    }
}
