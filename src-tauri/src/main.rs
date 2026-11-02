//! CinaVault-Nova Tauri shell — thin shell over `cinavault-core`.
//!
//! Commands do no business logic themselves; they validate the vault
//! gate, then call the core. The vault key lives only in AppState
//! (in memory, zeroized on lock). The vault header is persisted as
//! `vault.json` (salt + verification tag only) in the data directory
//! (env CINAVAULT_NOVA_DATA, else `./cinavault-nova-data`).
//! Store-safe mode is env CINAVAULT_STORE_SAFE=1 — it gates adult
//! commands at the shell, in addition to the core gate.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cinavault_core::adult::{adult_manifests, enrich_adult_item};
use cinavault_core::db::LibraryDb;
use cinavault_core::drive::{
    auth_url, download_file, list_files_with_refresh, revoke_token,
    DriveConfig, DriveFile, DriveTokens, Pkce,
};
use cinavault_core::enrichment::{
    apply_match, lookup_candidates, LookupCandidate, MockProvider,
};
use cinavault_core::enrichment::http_provider::HttpProvider;
use cinavault_core::enrichment::Provider;
use cinavault_core::playback::{discover_dlna_devices, CastDevice, PlaybackSession};
use cinavault_core::providers::{builtin_manifests, ProviderManifest};
use cinavault_core::updater::{check_github_release, SIGNING_SECRETS_NEEDED, UpdateCheck};
use cinavault_core::vault::{VaultHeader, VaultState};
use cinavault_core::vault_store::{
    lock_vault_folder, seal_db, unseal_db_to_session, VAULT_HEADER_FILE,
};
use cinavault_core::vpn::{install_wireguard, parse_profile, VpnInstallResult, VpnProfile};
use cinavault_core::{MediaItem, ScanResult};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

fn data_dir() -> PathBuf {
    std::env::var("CINAVAULT_NOVA_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("cinavault-nova-data"))
}

fn store_safe() -> bool {
    matches!(
        std::env::var("CINAVAULT_STORE_SAFE").as_deref(),
        Ok("1") | Ok("true")
    )
}

struct AppState {
    vault: Mutex<Option<VaultState>>,
    header: Mutex<Option<VaultHeader>>,
    drive_tokens: Mutex<Option<DriveTokens>>,
    drive_pkce: Mutex<Option<Pkce>>,
}

impl AppState {
    fn db_path(&self, session: bool) -> PathBuf {
        let dir = data_dir();
        if session {
            dir.join(".vault-session").join("cinavault.db")
        } else {
            dir.join("cinavault.db")
        }
    }
    fn open_db(&self) -> Result<LibraryDb, String> {
        // Prefer the unlocked session DB; fall back to plaintext DB
        // (legacy, pre-migration) only when no sealed DB exists.
        let session = self.db_path(true);
        if session.exists() {
            return LibraryDb::open(&session).map_err(|e| e.to_string());
        }
        if data_dir().join("cinavault.db.vault").exists() {
            return Err("vault is locked; unlock before accessing the library".into());
        }
        LibraryDb::open(&self.db_path(false)).map_err(|e| e.to_string())
    }
    fn require_unlocked(&self) -> Result<(), String> {
        if self.vault.lock().unwrap().is_none() {
            return Err("vault is locked; unlock before accessing the library".into());
        }
        Ok(())
    }
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
        configured: state.header.lock().unwrap().is_some()
            || data_dir().join(VAULT_HEADER_FILE).exists(),
        unlocked: state.vault.lock().unwrap().is_some(),
    }
}

fn persist_header(header: &VaultHeader) -> Result<(), String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(header).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(VAULT_HEADER_FILE), json).map_err(|e| e.to_string())
}

fn load_header() -> Result<VaultHeader, String> {
    let json = std::fs::read_to_string(data_dir().join(VAULT_HEADER_FILE))
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

#[tauri::command]
fn vault_setup(passphrase: String, state: tauri::State<AppState>) -> Result<VaultStatus, String> {
    let header = VaultHeader::create(&passphrase).map_err(|e| e.to_string())?;
    let vault = header.unlock(&passphrase).map_err(|e| e.to_string())?;
    persist_header(&header)?;
    // Create the DB file so the vault folder is complete.
    let _ = LibraryDb::open(&state.db_path(false)).map_err(|e| e.to_string())?;
    *state.header.lock().unwrap() = Some(header);
    *state.vault.lock().unwrap() = Some(vault);
    Ok(VaultStatus { configured: true, unlocked: true })
}

#[tauri::command]
fn vault_unlock(passphrase: String, state: tauri::State<AppState>) -> Result<VaultStatus, String> {
    let header = match state.header.lock().unwrap().clone() {
        Some(h) => h,
        None => load_header()?,
    };
    let vault = header.unlock(&passphrase).map_err(|e| e.to_string())?;
    // If a sealed DB exists, unseal it to the session copy.
    let dir = data_dir();
    if dir.join("cinavault.db.vault").exists() {
        unseal_db_to_session(&vault, &dir).map_err(|e| e.to_string())?;
    }
    *state.header.lock().unwrap() = Some(header);
    *state.vault.lock().unwrap() = Some(vault);
    Ok(VaultStatus { configured: true, unlocked: true })
}

#[tauri::command]
fn vault_lock(state: tauri::State<AppState>) -> Result<VaultStatus, String> {
    if let Some(vault) = state.vault.lock().unwrap().as_ref() {
        // Seal DB + artwork before dropping the key. If sealing fails,
        // keep the vault unlocked rather than losing data.
        lock_vault_folder(vault, &data_dir()).map_err(|e| e.to_string())?;
    }
    *state.vault.lock().unwrap() = None;
    Ok(VaultStatus {
        configured: state.header.lock().unwrap().is_some()
            || data_dir().join(VAULT_HEADER_FILE).exists(),
        unlocked: false,
    })
}

#[tauri::command]
fn vault_change_passphrase(
    old_passphrase: String,
    new_passphrase: String,
    state: tauri::State<AppState>,
) -> Result<VaultStatus, String> {
    state.require_unlocked()?;
    let header = state.header.lock().unwrap().clone().ok_or("no vault configured")?;
    let old = header.unlock(&old_passphrase).map_err(|e| e.to_string())?;
    let dir = data_dir();
    // Re-seal via vault_store if a sealed DB exists, else just replace
    // the header (DB will be sealed on lock with the new key).
    if dir.join("cinavault.db.vault").exists() {
        let new_header =
            cinavault_core::vault_store::change_passphrase(&old, &dir, &new_passphrase)
                .map_err(|e| e.to_string())?;
        persist_header(&new_header)?;
        *state.header.lock().unwrap() = Some(new_header);
    } else {
        let new_header = VaultHeader::create(&new_passphrase).map_err(|e| e.to_string())?;
        persist_header(&new_header)?;
        *state.header.lock().unwrap() = Some(new_header);
    }
    // Force re-unlock with the new passphrase on next unlock.
    *state.vault.lock().unwrap() = None;
    Ok(VaultStatus { configured: true, unlocked: false })
}

#[tauri::command]
fn vault_export_backup(dest_folder: String, state: tauri::State<AppState>) -> Result<String, String> {
    state.require_unlocked()?;
    let dir = data_dir();
    let dest = PathBuf::from(&dest_folder);
    std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    // Ensure a sealed DB exists for the backup.
    if let Some(vault) = state.vault.lock().unwrap().as_ref() {
        if !dir.join("cinavault.db.vault").exists() && dir.join("cinavault.db").exists() {
            seal_db(vault, &dir).map_err(|e| e.to_string())?;
        }
    }
    std::fs::copy(dir.join(VAULT_HEADER_FILE), dest.join(VAULT_HEADER_FILE))
        .map_err(|e| e.to_string())?;
    std::fs::copy(dir.join("cinavault.db.vault"), dest.join("cinavault.db.vault"))
        .map_err(|e| e.to_string())?;
    Ok("backup exported: header + sealed DB only; it still requires your passphrase and contains no plaintext or artwork".into())
}

#[tauri::command]
fn vault_import_backup(backup_folder: String, passphrase: String, state: tauri::State<AppState>) -> Result<VaultStatus, String> {
    let src = PathBuf::from(&backup_folder);
    let header_json = std::fs::read_to_string(src.join(VAULT_HEADER_FILE)).map_err(|e| e.to_string())?;
    let header: VaultHeader = serde_json::from_str(&header_json).map_err(|e| e.to_string())?;
    // Verify the passphrase BEFORE overwriting anything.
    let _vault = header.unlock(&passphrase).map_err(|e| e.to_string())?;
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::copy(src.join(VAULT_HEADER_FILE), dir.join(VAULT_HEADER_FILE)).map_err(|e| e.to_string())?;
    std::fs::copy(src.join("cinavault.db.vault"), dir.join("cinavault.db.vault")).map_err(|e| e.to_string())?;
    *state.header.lock().unwrap() = Some(header);
    Ok(VaultStatus { configured: true, unlocked: false })
}

#[tauri::command]
fn vault_migrate(confirm: String, backup_folder: String, state: tauri::State<AppState>) -> Result<String, String> {
    if confirm != "MIGRATE" {
        return Err("migration requires confirm = \"MIGRATE\" and a backup folder outside the vault".into());
    }
    state.require_unlocked()?;
    // Backup first: header + sealed DB to the backup folder.
    vault_export_backup(backup_folder, state.clone())?;
    // Then seal DB + artwork and delete plaintexts (lock does this).
    vault_lock(state)?;
    Ok("migrated: legacy plaintext library sealed; reversible only via the backup + passphrase".into())
}

#[tauri::command]
fn scan_library(folder: String, state: tauri::State<AppState>) -> Result<ScanResult, String> {
    state.require_unlocked()?;
    let result = cinavault_core::catalog::scan_folder(&PathBuf::from(&folder));
    let db = state.open_db()?;
    for item in result.library_items() {
        db.upsert_item(item).map_err(|e| e.to_string())?;
    }
    Ok(result)
}

#[tauri::command]
fn list_library(state: tauri::State<AppState>) -> Result<Vec<MediaItem>, String> {
    state.require_unlocked()?;
    state.open_db()?.list_library().map_err(|e| e.to_string())
}

#[tauri::command]
fn search_library(query: String, state: tauri::State<AppState>) -> Result<Vec<MediaItem>, String> {
    state.require_unlocked()?;
    state.open_db()?.search(&query).map_err(|e| e.to_string())
}

#[tauri::command]
fn continue_watching(state: tauri::State<AppState>) -> Result<Vec<MediaItem>, String> {
    state.require_unlocked()?;
    state.open_db()?.continue_watching().map_err(|e| e.to_string())
}

#[tauri::command]
fn purge_missing(state: tauri::State<AppState>) -> Result<(Vec<String>, Vec<String>), String> {
    state.require_unlocked()?;
    state.open_db()?.purge_missing_files().map_err(|e| e.to_string())
}

#[tauri::command]
fn item_unmatch(item_id: String, state: tauri::State<AppState>) -> Result<(), String> {
    state.require_unlocked()?;
    state.open_db()?.unmatch(&item_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn item_lookup(title: String, year: Option<u32>, state: tauri::State<AppState>) -> Result<Vec<LookupCandidate>, String> {
    state.require_unlocked()?;
    let manifests = builtin_manifests();
    let providers: Vec<HttpProvider> = manifests
        .iter()
        .map(|m| HttpProvider { manifest: m.clone(), api_key: None })
        .collect();
    let refs: Vec<&dyn Provider> = providers.iter().map(|p| p as &dyn Provider).collect();
    Ok(lookup_candidates(&refs, &title, year))
}

#[tauri::command]
fn item_match(item_id: String, candidate: LookupCandidate, state: tauri::State<AppState>) -> Result<(), String> {
    state.require_unlocked()?;
    let db = state.open_db()?;
    apply_match(&db, &item_id, &candidate).map_err(|e| e.to_string())
}

#[tauri::command]
fn item_mark_adult(item_id: String, is_adult: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.require_unlocked()?;
    state.open_db()?.mark_adult(&item_id, is_adult).map_err(|e| e.to_string())
}

#[tauri::command]
fn enrich_adult(item_id: String, state: tauri::State<AppState>) -> Result<String, String> {
    state.require_unlocked()?;
    if store_safe() {
        return Ok("store-safe build: adult pass skipped".into());
    }
    let db = state.open_db()?;
    let item = db.get_item(&item_id).map_err(|e| e.to_string())?.ok_or("item not found")?;
    let manifests = adult_manifests();
    let providers: Vec<HttpProvider> = manifests
        .iter()
        .map(|m| HttpProvider { manifest: m.clone(), api_key: None })
        .collect();
    let refs: Vec<&dyn Provider> = providers.iter().map(|p| p as &dyn Provider).collect();
    let cache = data_dir().join("artwork");
    let result = enrich_adult_item(&db, &refs, &item, store_safe(), &cache, |url| {
        ureq::get(url).call().ok().and_then(|r| {
            use std::io::Read as _;
            let mut b = Vec::new();
            r.into_reader().read_to_end(&mut b).ok()?;
            Some(b)
        })
    });
    Ok(format!("{result:?}"))
}

#[tauri::command]
fn list_providers() -> Vec<ProviderManifest> {
    let mut all = builtin_manifests();
    all.extend(adult_manifests());
    all
}

#[tauri::command]
fn vpn_install(state: tauri::State<AppState>) -> Result<VpnInstallResult, String> {
    state.require_unlocked()?;
    Ok(install_wireguard(&data_dir().join("vpn")))
}

#[tauri::command]
fn vpn_profile(path: String, state: tauri::State<AppState>) -> Result<VpnProfile, String> {
    state.require_unlocked()?;
    parse_profile(&PathBuf::from(path))
}

#[tauri::command]
fn drive_auth_url(state: tauri::State<AppState>) -> Result<String, String> {
    state.require_unlocked()?;
    let config = DriveConfig {
        client_id: std::env::var("CINAVAULT_DRIVE_CLIENT_ID").unwrap_or_default(),
    };
    let pkce = cinavault_core::drive::new_pkce();
    let url = auth_url(&config, &pkce)?;
    *state.drive_pkce.lock().unwrap() = Some(pkce);
    Ok(url)
}

#[tauri::command]
fn drive_exchange(code: String, state: tauri::State<AppState>) -> Result<bool, String> {
    state.require_unlocked()?;
    let config = DriveConfig {
        client_id: std::env::var("CINAVAULT_DRIVE_CLIENT_ID").unwrap_or_default(),
    };
    let pkce = state.drive_pkce.lock().unwrap().clone().ok_or("start auth first")?;
    let tokens = cinavault_core::drive::exchange_code(&config, &code, &pkce.verifier)?;
    let connected = tokens.is_connected();
    *state.drive_tokens.lock().unwrap() = Some(tokens);
    Ok(connected)
}

#[tauri::command]
fn drive_disconnect(state: tauri::State<AppState>) -> Result<bool, String> {
    if let Some(tokens) = state.drive_tokens.lock().unwrap().take() {
        let _ = revoke_token(&tokens.access_token);
    }
    *state.drive_pkce.lock().unwrap() = None;
    Ok(true)
}

#[tauri::command]
fn drive_list(state: tauri::State<AppState>) -> Result<Vec<DriveFile>, String> {
    state.require_unlocked()?;
    let config = DriveConfig {
        client_id: std::env::var("CINAVAULT_DRIVE_CLIENT_ID").unwrap_or_default(),
    };
    let mut tokens = state.drive_tokens.lock().unwrap().clone().ok_or("Drive is disconnected; configure a client ID and sign in first")?;
    let files = list_files_with_refresh(&config, &mut tokens, |token| {
        let resp = ureq::get("https://www.googleapis.com/drive/v3/files?pageSize=100&fields=files(id,name,mimeType)")
            .set("Authorization", &format!("Bearer {token}"))
            .call()
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
        Ok((status, json))
    })?;
    *state.drive_tokens.lock().unwrap() = Some(tokens);
    Ok(files.into_iter().filter(|f| f.is_media()).collect())
}

#[tauri::command]
fn drive_ingest(file_id: String, file_name: String, dest_folder: String, state: tauri::State<AppState>) -> Result<String, String> {
    state.require_unlocked()?;
    let tokens = state.drive_tokens.lock().unwrap().clone().ok_or("Drive is disconnected")?;
    let file = DriveFile { id: file_id, name: file_name, mime_type: String::new() };
    let local = download_file(&tokens, &file, &PathBuf::from(&dest_folder))?;
    let db = state.open_db()?;
    let item = MediaItem {
        id: format!("drive:{}", file.id),
        title: file.name.clone(),
        year: None,
        paths: vec![local],
        poster_url: None,
        overview: None,
        watched: false,
        progress_seconds: 0,
        favourite: false,
        is_adult: false,
    };
    db.upsert_item(&item).map_err(|e| e.to_string())?;
    Ok(format!("ingested {}", file.name))
}

#[tauri::command]
fn playback_start(item_id: String, source: String, state: tauri::State<AppState>) -> Result<PlaybackSession, String> {
    state.require_unlocked()?;
    Ok(PlaybackSession::new(item_id, &source))
}

#[tauri::command]
fn cast_discover_dlna(state: tauri::State<AppState>) -> Result<Vec<CastDevice>, String> {
    state.require_unlocked()?;
    Ok(discover_dlna_devices(Duration::from_secs(3)))
}

#[tauri::command]
fn update_check(state: tauri::State<AppState>) -> Result<UpdateCheck, String> {
    state.require_unlocked()?;
    check_github_release("0.1.0")
}

#[tauri::command]
fn signing_requirements() -> Vec<String> {
    SIGNING_SECRETS_NEEDED.iter().map(|s| s.to_string()).collect()
}

fn main() {
    tauri::Builder::default()
        .manage(AppState {
            vault: Mutex::new(None),
            header: Mutex::new(None),
            drive_tokens: Mutex::new(None),
            drive_pkce: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            vault_status, vault_setup, vault_unlock, vault_lock,
            vault_change_passphrase, vault_export_backup, vault_import_backup, vault_migrate,
            scan_library, list_library, search_library, continue_watching, purge_missing,
            item_unmatch, item_lookup, item_match, item_mark_adult, enrich_adult, list_providers,
            vpn_install, vpn_profile,
            drive_auth_url, drive_exchange, drive_disconnect, drive_list, drive_ingest,
            playback_start, cast_discover_dlna, update_check, signing_requirements
        ])
        .run(tauri::generate_context!())
        .expect("error while running CinaVault-Nova");
}

// Keep MockProvider referenced for tests / future mock providers.
#[allow(dead_code)]
fn _mock_provider_unused() {
    let _ = MockProvider {
        manifest: ProviderManifest {
            id: String::new(),
            version: String::new(),
            display_name: String::new(),
            kinds: vec![],
            adult_only: false,
            endpoint: String::new(),
        },
        result: Default::default(),
    };
}
