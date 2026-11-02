//! CinaVault-Nova shared core.
//!
//! One core, thin platform shells. This crate owns the concepts that must
//! behave identically on every platform: catalog, persistent database,
//! provider plugins (including adult providers behind a strict
//! store-safe gate), background enrichment jobs, playback/casting,
//! app-vault encryption, VPN, Drive, sync, and update checks.
//! It owns no UI and no Tauri code.

pub mod adult;
pub mod catalog;
pub mod db;
pub mod drive;
pub mod enrichment;
pub mod jobs;
pub mod playback;
pub mod providers;
pub mod sync;
pub mod updater;
pub mod vault;
pub mod vault_store;
pub mod vpn;

pub use catalog::{MediaItem, ScanResult};
pub use providers::{ProviderKind, ProviderManifest};
pub use sync::{SyncCollection, WatchState};
pub use vault::{VaultHeader, VaultState};
