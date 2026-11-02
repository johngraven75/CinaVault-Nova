//! Playback pipeline — stub in v0.1.0.
//!
//! The pipeline contract is defined here so every platform shell implements
//! the same behaviour. Hardware-accelerated decoding, HLS/DASH, casting
//! (Chromecast / AirPlay / DLNA), offline download, and resume are
//! Planned (see FEATURE_PARITY.md) — this module does not claim them.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackState {
    Idle,
    Loading,
    Playing,
    Paused,
    Finished,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackSession {
    pub item_id: String,
    pub position_seconds: u64,
    pub state: PlaybackState,
}

impl PlaybackSession {
    pub fn new(item_id: impl Into<String>) -> Self {
        Self {
            item_id: item_id.into(),
            position_seconds: 0,
            state: PlaybackState::Idle,
        }
    }

    /// Resume position for this item. Real resume persistence is wired
    /// through `sync::WatchState` once the database layer lands.
    pub fn resume_from(&mut self, position_seconds: u64) {
        self.position_seconds = position_seconds;
        self.state = PlaybackState::Paused;
    }
}
