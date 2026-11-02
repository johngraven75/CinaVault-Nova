//! Signed-installer readiness and delta auto-update design.
//!
//! No secrets are invented or stored here. This module documents
//! exactly what is needed, and implements the *check* half of
//! auto-update: compare the installed version against the latest
//! GitHub release. Applying an update (and delta updates) is a Tauri
//! updater concern: it requires a signed updater artifact and a
//! signed update manifest, which do not exist until the signing
//! secrets below exist. Until then, the UI must say updates are
//! manual (download the new installer), not pretend delta updates work.

use serde::{Deserialize, Serialize};

/// Signing secrets needed — names only, never values.
/// - Windows: AZURE_TENANT_ID, AZURE_CLIENT_ID, AZURE_CLIENT_SECRET,
///   ARTIFACT_SIGNING_ENDPOINT, ARTIFACT_SIGNING_ACCOUNT,
///   ARTIFACT_SIGNING_CERTIFICATE_PROFILE (Azure Trusted Signing), or
///   a local code-signing certificate.
/// - macOS (future): APPLE_CERTIFICATE, APPLE_ID, APPLE_PASSWORD,
///   APPLE_TEAM_ID (notarization).
/// - Updater signing: TAURI_SIGNING_PRIVATE_KEY (and its public key
///   embedded in tauri.conf.json) — without this, the Tauri updater
///   will refuse every update, by design.
pub const SIGNING_SECRETS_NEEDED: &[&str] = &[
    "AZURE_TENANT_ID",
    "AZURE_CLIENT_ID",
    "AZURE_CLIENT_SECRET",
    "ARTIFACT_SIGNING_ENDPOINT",
    "ARTIFACT_SIGNING_ACCOUNT",
    "ARTIFACT_SIGNING_CERTIFICATE_PROFILE",
    "TAURI_SIGNING_PRIVATE_KEY",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheck {
    pub installed_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release_notes: Option<String>,
    /// False until signing + updater manifest exist.
    pub delta_update_available: bool,
}

pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| {
        s.trim_start_matches('v')
            .split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    parse(a).cmp(&parse(b))
}

/// Check GitHub releases for johngraven75/CinaVault-Nova.
/// Network call — not used in unit tests.
pub fn check_github_release(installed_version: &str) -> Result<UpdateCheck, String> {
    let resp = ureq::get("https://api.github.com/repos/johngraven75/CinaVault-Nova/releases/latest")
        .set("User-Agent", "CinaVault-Nova")
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    let latest = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    Ok(UpdateCheck {
        installed_version: installed_version.into(),
        latest_version: Some(latest.clone()),
        update_available: compare_versions(&latest, installed_version)
            == std::cmp::Ordering::Greater,
        release_notes: json
            .get("body")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        delta_update_available: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare_orders_correctly() {
        assert_eq!(
            compare_versions("v0.2.0", "0.1.0"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(compare_versions("0.1.0", "0.1.0"), std::cmp::Ordering::Equal);
        assert_eq!(
            compare_versions("0.1.0", "0.2.0"),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn signing_list_names_no_values() {
        assert!(SIGNING_SECRETS_NEEDED.contains(&"TAURI_SIGNING_PRIVATE_KEY"));
        for name in SIGNING_SECRETS_NEEDED {
            assert!(!name.contains('='));
        }
    }
}
