//! CinaVault-Nova shared core.
//!
//! One core, thin platform shells. This crate owns the concepts that must
//! behave identically on every platform: catalog, provider plugins, playback
//! pipeline, app-vault encryption, and sync. It owns no UI and no Tauri code.
//!
//! Status in v0.1.0: catalog scanning, provider manifests, and vault
//! seal/unseal are implemented at the core level. Playback and sync are
//! designed types with stub pipelines — see FEATURE_PARITY.md. Do not claim
//! them as shipped.

pub mod catalog;
pub mod playback;
pub mod providers;
pub mod sync;
pub mod vault;

pub use catalog::{MediaItem, ScanResult};
pub use providers::{ProviderKind, ProviderManifest};
pub use sync::{SyncCollection, WatchState};
pub use vault::{VaultHeader, VaultState};
