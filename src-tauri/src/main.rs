//! CinaVault-Nova Tauri shell.
//!
//! Thin shell over `cinavault-core`. Commands here do no business logic
//! themselves: scanning, provider listing, and vault state live in the core
//! or in the vault session held by `AppState`.
//!
//! Status in v0.1.0: commands are wired and honest about their state.
//! Vault unlock in this scaffold validates a passphrase against a header
//! held in memory for the session; persistent vault storage, database
//! sealing, and artwork sealing land with the full vault implementation
//! (see FEATURE_PARITY.md).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cinavault_core::providers::{builtin_manifests, ProviderManifest};
use cinavault_core::vault::{VaultHeader, VaultState};
use cinavault_core::{MediaItem, ScanResult};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;

struct AppState {
    vault: Mutex<Option<VaultState>>,
    header: Mutex<Option<VaultHeader>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VaultStatus {
    configured: bool,
    unlocked: bool,
}

#[tauri::command]
fn vault_status(state: tauri::State<AppState>) -> VaultStatus {
    VaultStatus {
        configured: state.header.lock().unwrap().is_some(),
        unlocked: state.vault.lock().unwrap().is_some(),
    }
}

#[tauri::command]
fn vault_setup(
    passphrase: String,
    state: tauri::State<AppState>,
) -> Result<VaultStatus, String> {
    // First-run setup: the user-selected passphrase is required.
    // No key file, no generated credential, no backdoor.
    let header = VaultHeader::create(&passphrase).map_err(|e| e.to_string())?;
    let vault = header.unlock(&passphrase).map_err(|e| e.to_string())?;
    *state.header.lock().unwrap() = Some(header);
    *state.vault.lock().unwrap() = Some(vault);
    Ok(VaultStatus {
        configured: true,
        unlocked: true,
    })
}

#[tauri::command]
fn vault_unlock(
    passphrase: String,
    state: tauri::State<AppState>,
) -> Result<VaultStatus, String> {
    let header = state.header.lock().unwrap().clone();
    let Some(header) = header else {
        return Err("no vault is configured; run setup first".into());
    };
    let vault = header.unlock(&passphrase).map_err(|e| e.to_string())?;
    *state.vault.lock().unwrap() = Some(vault);
    Ok(VaultStatus {
        configured: true,
        unlocked: true,
    })
}

#[tauri::command]
fn vault_lock(state: tauri::State<AppState>) -> VaultStatus {
    // Dropping the VaultState zeroizes the in-memory key.
    *state.vault.lock().unwrap() = None;
    VaultStatus {
        configured: state.header.lock().unwrap().is_some(),
        unlocked: false,
    }
}

#[tauri::command]
fn scan_library(folder: String, state: tauri::State<AppState>) -> Result<ScanResult, String> {
    if state.vault.lock().unwrap().is_none() {
        return Err("vault is locked; unlock before accessing the library".into());
    }
    Ok(cinavault_core::catalog::scan_folder(&PathBuf::from(folder)))
}

#[tauri::command]
fn list_providers() -> Vec<ProviderManifest> {
    builtin_manifests()
}

#[tauri::command]
fn list_library_items(scan: ScanResult) -> Vec<MediaItem> {
    // Shelves must be built from library items only.
    scan.library_items().into_iter().cloned().collect()
}

fn main() {
    tauri::Builder::default()
        .manage(AppState {
            vault: Mutex::new(None),
            header: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            vault_status,
            vault_setup,
            vault_unlock,
            vault_lock,
            scan_library,
            list_providers,
            list_library_items
        ])
        .run(tauri::generate_context!())
        .expect("error while running CinaVault-Nova");
}
