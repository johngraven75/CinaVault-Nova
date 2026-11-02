//! Media catalog: folder scanning into library items.
//!
//! Design rules carried over from the Premium fixes, by design:
//! - An item with no usable file path is not a library item and must not
//!   appear on any shelf. A blank primary path still counts only if some
//!   copy of the item has a usable path.
//! - Shelves are built from library items only. Nothing that is not in the
//!   library may appear as a "suggested" movie.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One library item. `paths` holds every known copy; at least one entry
/// must be usable for the item to be shown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: String,
    pub title: String,
    pub year: Option<u32>,
    pub paths: Vec<PathBuf>,
    pub poster_url: Option<String>,
    pub overview: Option<String>,
    pub watched: bool,
    pub progress_seconds: u64,
    pub favourite: bool,
}

impl MediaItem {
    pub fn has_usable_file_path(&self) -> bool {
        self.paths
            .iter()
            .any(|p| !p.as_os_str().is_empty() && p.is_absolute())
    }
}

/// Result of one scan pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub scanned_paths: usize,
    pub items: Vec<MediaItem>,
}

impl ScanResult {
    /// Library-only view: drops every item with no usable file path.
    /// Every shelf in every UI must be built from this, never from the
    /// raw scan list.
    pub fn library_items(&self) -> Vec<&MediaItem> {
        self.items
            .iter()
            .filter(|item| item.has_usable_file_path())
            .collect()
    }
}

/// Scan a folder tree for media files. This is a scaffolded scan: it walks
/// the tree and records candidate media files as items. Full metadata
/// enrichment runs as a separate background job and must never block the
/// library UI.
pub fn scan_folder(root: &Path) -> ScanResult {
    let mut items = Vec::new();
    let mut scanned = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            scanned += 1;
            if is_media_extension(&path) {
                let stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Unknown".to_string());
                items.push(MediaItem {
                    id: format!("{}-{}", scanned, stem),
                    title: stem,
                    year: None,
                    paths: vec![path],
                    poster_url: None,
                    overview: None,
                    watched: false,
                    progress_seconds: 0,
                    favourite: false,
                });
            }
        }
    }
    ScanResult {
        scanned_paths: scanned,
        items,
    }
}

fn is_media_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .as_deref(),
        Some("mp4") | Some("mkv") | Some("avi") | Some("mov") | Some("webm") | Some("m4v")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pathless_items_are_not_library_items() {
        let result = ScanResult {
            scanned_paths: 2,
            items: vec![
                MediaItem {
                    id: "1".into(),
                    title: "No file".into(),
                    year: None,
                    paths: vec![],
                    poster_url: None,
                    overview: None,
                    watched: false,
                    progress_seconds: 0,
                    favourite: false,
                },
                MediaItem {
                    id: "2".into(),
                    title: "Has file".into(),
                    year: None,
                    paths: vec![PathBuf::from("/media/film.mkv")],
                    poster_url: None,
                    overview: None,
                    watched: false,
                    progress_seconds: 0,
                    favourite: false,
                },
            ],
        };
        let library = result.library_items();
        assert_eq!(library.len(), 1);
        assert_eq!(library[0].id, "2");
    }
}
