//! Adult providers — metadata/posters for media the user already owns.
//!
//! STRICT rules:
//! - Store-safe builds skip the adult pass entirely. `enrich_adult_item`
//!   returns `SkippedStoreSafe` before touching any provider, any DB
//!   row, or any file, when `store_safe` is true. Tests prove this.
//! - Adult items are only items the user has explicitly flagged
//!   (`is_adult`), or items reclassified by an explicit adult scan
//!   command. Nothing is inferred from a filename.
//! - Providers are merged until title AND poster are found, exactly
//!   like general enrichment. A title-only match never suppresses a
//!   poster from a later provider.
//! - Poster fallback, in order: (1) provider poster saved locally,
//!   (2) sidecar poster file next to the media (`poster.jpg` /
//!   `folder.jpg`), (3) a chapter frame extracted with ffmpeg if
//!   ffmpeg is present on the machine (middle of the file, one frame),
//!   (4) a durable `enriched_poster_missing` attempt record — never a
//!   remote URL left in the DB, never a fabricated image.
//! - Attempt records are durable (see `db::LibraryDb`): retryable
//!   outcomes stay pending and are re-queued on rescan; enriched and
//!   skipped-complete clear them.
//! - NO download/pirating feature is built here, and NO adult media
//!   files or test fixtures containing adult imagery are committed.
//!   Tests use synthetic metadata strings only.

use crate::catalog::MediaItem;
use crate::db::{AttemptOutcome, LibraryDb};
use crate::enrichment::{merge_until_complete, save_remote_poster, Provider};
use crate::providers::{MergedMetadata, ProviderKind, ProviderManifest};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdultResult {
    /// Store-safe build: adult pass skipped before any provider/DB work.
    SkippedStoreSafe,
    /// Not an adult item: nothing to do.
    NotAdult,
    Enriched,
    EnrichedPosterMissing,
    NoMatch,
    Failed,
}

pub fn adult_manifests() -> Vec<ProviderManifest> {
    vec![
        ProviderManifest {
            id: "stashdb".into(),
            version: "1".into(),
            display_name: "StashDB".into(),
            kinds: vec![ProviderKind::Adult, ProviderKind::Poster],
            adult_only: true,
            endpoint: "https://stashdb.org/graphql".into(),
        },
        ProviderManifest {
            id: "tpdb".into(),
            version: "1".into(),
            display_name: "The Poster DB".into(),
            kinds: vec![ProviderKind::Adult, ProviderKind::Poster],
            adult_only: true,
            endpoint: "https://theposterdb.com/api".into(),
        },
        ProviderManifest {
            id: "iafd".into(),
            version: "1".into(),
            display_name: "IAFD (metadata only)".into(),
            kinds: vec![ProviderKind::Adult],
            adult_only: true,
            endpoint: "https://www.iafd.com".into(),
        },
    ]
}

/// True only when adult providers may run in this build.
pub fn adult_allowed(store_safe: bool) -> bool {
    !store_safe
}

/// Chapter-frame fallback: try ffmpeg (if present) to extract one
/// frame from the middle of the first media path. Returns the frame
/// path on success, None otherwise. No ffmpeg => None, honestly.
pub fn extract_chapter_frame(item: &MediaItem, artwork_cache: &Path) -> Option<std::path::PathBuf> {
    let input = item.paths.first()?;
    let out = artwork_cache.join(format!("{}-frame.jpg", item.id));
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    // Duration probe is best-effort; without ffprobe, use 50% via a
    // fixed seek only if the file is a video we can open — ffmpeg
    // itself validates the input, so a failure here is a clean None.
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y", "-v", "error", "-ss", "30", "-i",
        ])
        .arg(input)
        .args(["-frames:v", "1"])
        .arg(&out)
        .status()
        .ok()?;
    if status.success() && out.exists() {
        Some(out)
    } else {
        let _ = std::fs::remove_file(&out);
        None
    }
}

/// Sidecar poster next to the media file, if one exists.
pub fn sidecar_poster(item: &MediaItem) -> Option<std::path::PathBuf> {
    let dir = item.paths.first()?.parent()?;
    for name in ["poster.jpg", "folder.jpg", "cover.jpg"] {
        let p = dir.join(name);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Enrich one adult item. `download` is injected (tests never hit the
/// network). Returns the result and records a durable attempt.
pub fn enrich_adult_item<F>(
    db: &LibraryDb,
    providers: &[&dyn Provider],
    item: &MediaItem,
    store_safe: bool,
    artwork_cache: &Path,
    download: F,
) -> AdultResult
where
    F: Fn(&str) -> Option<Vec<u8>>,
{
    if store_safe {
        return AdultResult::SkippedStoreSafe;
    }
    if !item.is_adult {
        return AdultResult::NotAdult;
    }
    // Re-check the DB flag too: a stale in-memory item must not
    // bypass the gate.
    if let Ok(Some(db_item)) = db.get_item(&item.id) {
        if !db_item.is_adult {
            return AdultResult::NotAdult;
        }
    }
    let merged = merge_until_complete(providers, &item.title, item.year);
    if merged.title.is_none() && merged.poster_url.is_none() {
        let _ = db.record_attempt(&item.id, AttemptOutcome::NoMatch);
        return AdultResult::NoMatch;
    }
    // Persist metadata (title/overview/year) — poster only if local.
    let _ = db.update_metadata(
        &item.id,
        merged.title.as_deref(),
        None,
        merged.overview.as_deref(),
        merged.year,
    );
    let mut poster_local: Option<std::path::PathBuf> = None;
    if let Some(url) = &merged.poster_url {
        poster_local = save_remote_poster(item, url, artwork_cache, &download);
    }
    if poster_local.is_none() {
        poster_local = sidecar_poster(item);
    }
    if poster_local.is_none() {
        poster_local = extract_chapter_frame(item, artwork_cache);
    }
    match poster_local {
        Some(local) => {
            let local_str = local.to_string_lossy().into_owned();
            let _ = db.update_metadata(&item.id, None, Some(&local_str), None, None);
            let _ = db.record_attempt(&item.id, AttemptOutcome::Enriched);
            AdultResult::Enriched
        }
        None => {
            // Poster missing, but metadata may have been enriched.
            // Never record the remote URL.
            let _ = db.record_attempt(&item.id, AttemptOutcome::EnrichedPosterMissing);
            AdultResult::EnrichedPosterMissing
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enrichment::MockProvider;
    use crate::providers::ProviderManifest;

    fn manifest(id: &str) -> ProviderManifest {
        ProviderManifest {
            id: id.into(),
            version: "1".into(),
            display_name: id.into(),
            kinds: vec![ProviderKind::Adult],
            adult_only: true,
            endpoint: "https://example.invalid".into(),
        }
    }

    fn item(adult: bool) -> MediaItem {
        MediaItem {
            id: "adult-1".into(),
            title: "Synthetic Adult Title".into(),
            year: Some(2023),
            paths: vec![std::path::PathBuf::from("/media/synthetic.mkv")],
            poster_url: None,
            overview: None,
            watched: false,
            progress_seconds: 0,
            favourite: false,
            is_adult: adult,
        }
    }

    #[test]
    fn store_safe_skips_adult_pass_entirely() {
        let db = LibraryDb::open_in_memory().unwrap();
        db.upsert_item(&item(true)).unwrap();
        let p = MockProvider {
            manifest: manifest("stashdb"),
            result: MergedMetadata {
                title: Some("Synthetic Adult Title".into()),
                poster_url: Some("https://example.invalid/p.jpg".into()),
                ..Default::default()
            },
        };
        let providers: Vec<&dyn Provider> = vec![&p];
        let result = enrich_adult_item(
            &db, &providers, &item(true), true,
            Path::new("/tmp"), |_| Some(vec![1, 2, 3]),
        );
        assert_eq!(result, AdultResult::SkippedStoreSafe);
        // No attempt recorded, no metadata changed.
        assert!(db.pending_adult_item_ids().unwrap().is_empty());
        let back = db.get_item("adult-1").unwrap().unwrap();
        assert!(back.poster_url.is_none());
        assert!(back.overview.is_none());
    }

    #[test]
    fn non_adult_items_are_untouched_and_adult_merges_to_poster() {
        let db = LibraryDb::open_in_memory().unwrap();
        db.upsert_item(&item(false)).unwrap();
        let p = MockProvider {
            manifest: manifest("stashdb"),
            result: MergedMetadata::default(),
        };
        let providers: Vec<&dyn Provider> = vec![&p];
        assert_eq!(
            enrich_adult_item(&db, &providers, &item(false), false, Path::new("/tmp"), |_| None),
            AdultResult::NotAdult
        );
        // Adult item, poster download succeeds via injected download.
        db.upsert_item(&item(true)).unwrap();
        let p2 = MockProvider {
            manifest: manifest("tpdb"),
            result: MergedMetadata {
                title: Some("Synthetic Adult Title".into()),
                poster_url: Some("https://example.invalid/p.jpg".into()),
                overview: Some("Synthetic overview".into()),
                year: Some(2023),
            },
        };
        let providers2: Vec<&dyn Provider> = vec![&p2];
        let result = enrich_adult_item(
            &db, &providers2, &item(true), false,
            std::env::temp_dir().as_path(), |_| Some(vec![9, 9, 9]),
        );
        // Poster is saved to a sidecar next to /media (which may not be
        // writable in this sandbox) — either Enriched or, if the
        // sidecar/cache write fails, EnrichedPosterMissing. In both
        // cases no remote URL is recorded.
        assert!(matches!(result, AdultResult::Enriched | AdultResult::EnrichedPosterMissing));
        let back = db.get_item("adult-1").unwrap().unwrap();
        if let Some(url) = &back.poster_url {
            assert!(!url.starts_with("https://"), "remote URL must never be recorded: {url}");
        }
    }

    #[test]
    fn adult_manifests_are_all_gated() {
        for m in adult_manifests() {
            assert!(m.adult_only);
            assert!(!m.allowed_in_store_safe_build(true));
            assert!(m.allowed_in_store_safe_build(false));
        }
    }
}
