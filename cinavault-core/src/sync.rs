//! Sync — watch state and collections only, end-to-end encrypted.
//!
//! Sync deliberately covers no raw media. A sync payload is a small,
//! encrypted record of what you watched and how you organised it. The
//! encryption design mirrors the app vault: the sync key is derived from
//! the user's passphrase, never stored, and the server (if any) sees only
//! ciphertext.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchState {
    pub item_id: String,
    pub watched: bool,
    pub progress_seconds: u64,
    pub updated_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCollection {
    pub name: String,
    pub item_ids: Vec<String>,
}

/// A sync payload: watch states + collections, to be sealed with the
/// vault key before leaving the device. Sealing itself lives in
/// `vault::seal_bytes`; this module only defines what may be synced.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncPayload {
    pub watch_states: Vec<WatchState>,
    pub collections: Vec<SyncCollection>,
}
