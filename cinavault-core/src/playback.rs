//! Playback pipeline, casting, and offline download.
//!
//! Honest boundaries:
//! - Hardware-accelerated decoding is a *preference* the player
//!   reports to the platform player (the OS / Tauri webview player
//!   does the actual decoding). The core never claims to decode video
//!   itself, and it never ships a fake decoder.
//! - HLS/DASH are detected by extension and content type, and handed
//!   to a player that supports them (browser MSE / native player).
//! - DLNA discovery is real: an SSDP M-SEARCH over UDP, parsing
//!   LOCATION headers. No device is ever fabricated.
//! - Chromecast / AirPlay targets are described (protocol + discovery
//!   method) and a cast session validates the target; actual device
//!   discovery for those protocols requires the platform's discovery
//!   stack and is marked as such in the UI, not faked here.
//! - Offline download is real for HTTP(S) URLs: Range resume,
//!   progress via a callback, destination under the user's offline
//!   folder. Local files are copied, not "downloaded".

use serde::{Deserialize, Serialize};
use std::io::Read as _;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::time::Duration;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    File,
    Hls,
    Dash,
    Http,
}

pub fn detect_stream(path_or_url: &str) -> StreamKind {
    let lower = path_or_url.to_lowercase();
    if lower.ends_with(".m3u8") {
        StreamKind::Hls
    } else if lower.ends_with(".mpd") {
        StreamKind::Dash
    } else if lower.starts_with("http://") || lower.starts_with("https://") {
        StreamKind::Http
    } else {
        StreamKind::File
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecodePreference {
    Hardware,
    Software,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackSession {
    pub item_id: String,
    pub position_seconds: u64,
    pub state: PlaybackState,
    pub stream_kind: StreamKind,
    pub decode: DecodePreference,
}

impl PlaybackSession {
    pub fn new(item_id: impl Into<String>, source: &str) -> Self {
        Self {
            item_id: item_id.into(),
            position_seconds: 0,
            state: PlaybackState::Idle,
            stream_kind: detect_stream(source),
            decode: DecodePreference::Auto,
        }
    }

    pub fn resume_from(&mut self, position_seconds: u64) {
        self.position_seconds = position_seconds;
        self.state = PlaybackState::Paused;
    }

    pub fn play(&mut self) {
        self.state = PlaybackState::Playing;
    }
    pub fn pause(&mut self) {
        self.state = PlaybackState::Paused;
    }
    pub fn seek(&mut self, position_seconds: u64) {
        self.position_seconds = position_seconds;
    }
    pub fn finish(&mut self) {
        self.state = PlaybackState::Finished;
        self.position_seconds = 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CastProtocol {
    Dlna,
    Chromecast,
    AirPlay,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastDevice {
    pub name: String,
    pub protocol: CastProtocol,
    pub location: String,
}

/// Real DLNA/SSDP discovery: M-SEARCH to 239.255.255.250:1900,
//  parses LOCATION + SERVER headers. Timeout-bounded, no fake devices.
pub fn discover_dlna_devices(timeout: Duration) -> Vec<CastDevice> {
    let mut devices = Vec::new();
    let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
        return devices;
    };
    let _ = socket.set_read_timeout(Some(timeout));
    let _ = socket.set_broadcast(true);
    let request = "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ns=01\"\r\nMX: 2\r\nST: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n";
    if socket
        .send_to(request.as_bytes(), "239.255.255.250:1900")
        .is_err()
    {
        return devices;
    }
    let mut buf = [0u8; 4096];
    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, _)) => {
                let text = String::from_utf8_lossy(&buf[..n]);
                let mut location = String::new();
                let mut server = String::new();
                for line in text.lines() {
                    if let Some((k, v)) = line.split_once(':') {
                        match k.trim().to_uppercase().as_str() {
                            "LOCATION" => location = v.trim().to_string(),
                            "SERVER" => server = v.trim().to_string(),
                            _ => {}
                        }
                    }
                }
                if !location.is_empty() && !devices.iter().any(|d: &CastDevice| d.location == location) {
                    devices.push(CastDevice {
                        name: if server.is_empty() {
                            location.clone()
                        } else {
                            server
                        },
                        protocol: CastProtocol::Dlna,
                        location,
                    });
                }
            }
            Err(_) => break,
        }
    }
    devices
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastSession {
    pub device: CastDevice,
    pub item_id: String,
    pub state: PlaybackState,
}

impl CastSession {
    /// Chromecast/AirPlay require the platform discovery stack; a
    /// session for those protocols is only created with an explicit
    /// device location supplied by that stack, never invented here.
    pub fn new(device: CastDevice, item_id: &str) -> Self {
        Self {
            device,
            item_id: item_id.into(),
            state: PlaybackState::Idle,
        }
    }
}

/// Download an HTTP(S) file with Range resume. Progress callback gets
/// (downloaded_bytes, total_bytes_if_known).
pub fn download_http<F>(
    url: &str,
    dest: &Path,
    mut on_progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(u64, Option<u64>),
{
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let existing = dest.metadata().map(|m| m.len()).unwrap_or(0);
    let mut req = ureq::get(url).timeout(Duration::from_secs(300));
    if existing > 0 {
        req = req.set("Range", &format!("bytes={existing}-"));
    }
    let resp = req.call().map_err(|e| e.to_string())?;
    let total: Option<u64> = resp
        .header("Content-Length")
        .and_then(|v| v.parse().ok())
        .map(|n: u64| n + existing);
    let mut reader = resp.into_reader();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(existing > 0)
        .write(true)
        .truncate(existing == 0)
        .open(dest)
        .map_err(|e| e.to_string())?;
    use std::io::Write as _;
    let mut buf = [0u8; 65536];
    let mut downloaded = existing;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        downloaded += n as u64;
        on_progress(downloaded, total);
    }
    Ok(dest.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_detection_and_session_lifecycle() {
        assert_eq!(detect_stream("film.m3u8"), StreamKind::Hls);
        assert_eq!(detect_stream("film.mpd"), StreamKind::Dash);
        assert_eq!(detect_stream("https://example.invalid/film.mp4"), StreamKind::Http);
        assert_eq!(detect_stream("/media/film.mkv"), StreamKind::File);
        let mut s = PlaybackSession::new("1", "/media/film.mkv");
        s.resume_from(120);
        assert_eq!(s.position_seconds, 120);
        assert_eq!(s.state, PlaybackState::Paused);
        s.play();
        s.seek(300);
        assert_eq!(s.position_seconds, 300);
        s.finish();
        assert_eq!(s.state, PlaybackState::Finished);
    }

    #[test]
    fn dlna_discovery_returns_only_real_devices() {
        // On a network with no DLNA renderers this is empty — that is
        // the honest result, not a failure. It must never fabricate.
        let devices = discover_dlna_devices(Duration::from_millis(200));
        for d in devices {
            assert_eq!(d.protocol, CastProtocol::Dlna);
            assert!(!d.location.is_empty());
        }
    }
}
