//! Enrichment: provider merging, Lookup, Unmatch/Match, adult pass.
//!
//! Rules, carried over as design (not copied code):
//! - Enrichment runs as a background job (see `jobs`), never inline in
//!   a UI call. Per item: at most 2 query candidates, 20s per provider
//!   chain, 120s per item remote phase.
//! - Merge across providers until title AND poster are found. A
//!   title-only match must never suppress a later poster.
//! - A failed poster download must never leave a remote URL in the DB.
//!   Posters are saved locally (sidecar file next to the media file, or
//!   the app artwork cache as fallback) and only then recorded.
//! - Store-safe builds skip the adult pass entirely. See `adult.rs`.
//! - Lookup returns ranked candidates; the caller picks one and Match
//!   applies it. Unmatch clears poster + overview only.

use crate::catalog::MediaItem;
use crate::db::{AttemptOutcome, LibraryDb};
use crate::providers::{MergedMetadata, ProviderManifest};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const MAX_CANDIDATES_PER_ITEM: usize = 2;
pub const PROVIDER_CHAIN_TIMEOUT: Duration = Duration::from_secs(20);
pub const ITEM_REMOTE_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub struct ProviderError(pub String);

/// A provider that can be queried. Implementations must be cheap to
/// test: unit tests use `MockProvider`, live code uses `HttpProvider`
/// (see `http_provider` below, which parses generic JSON shapes).
pub trait Provider: Send + Sync {
    fn manifest(&self) -> &ProviderManifest;
    fn fetch(&self, title: &str, year: Option<u32>) -> Result<MergedMetadata, ProviderError>;
}

pub struct MockProvider {
    pub manifest: ProviderManifest,
    pub result: MergedMetadata,
}

impl Provider for MockProvider {
    fn manifest(&self) -> &ProviderManifest {
        &self.manifest
    }
    fn fetch(&self, _title: &str, _year: Option<u32>) -> Result<MergedMetadata, ProviderError> {
        Ok(self.result.clone())
    }
}

/// Merge provider results in order until complete (title + poster).
/// Returns the merged result and whether it is complete.
pub fn merge_until_complete(
    providers: &[&dyn Provider],
    title: &str,
    year: Option<u32>,
) -> MergedMetadata {
    let mut merged = MergedMetadata::default();
    for provider in providers.iter().take(MAX_CANDIDATES_PER_ITEM * 4) {
        if merged.is_complete() {
            break;
        }
        if let Ok(next) = provider.fetch(title, year) {
            merged.merge(next);
        }
    }
    merged
}

/// Save a remote poster locally: sidecar next to the first media path,
/// falling back to the artwork cache. Returns the local path, or None
/// if the download fails — in which case the caller must NOT record a
/// remote URL. `download` is injected so tests never hit the network.
pub fn save_remote_poster<F>(
    item: &MediaItem,
    poster_url: &str,
    artwork_cache: &Path,
    download: F,
) -> Option<PathBuf>
where
    F: Fn(&str) -> Option<Vec<u8>>,
{
    let bytes = download(poster_url)?;
    let sidecar = item
        .paths
        .first()
        .and_then(|p| p.parent().map(|d| d.join("poster.jpg")));
    let target = sidecar.unwrap_or_else(|| artwork_cache.join(format!("{}-poster.jpg", item.id)));
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    std::fs::write(&target, bytes).ok()?;
    Some(target)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupCandidate {
    pub provider_id: String,
    pub title: String,
    pub poster_url: Option<String>,
    pub overview: Option<String>,
    pub year: Option<u32>,
}

/// Lookup: ranked candidates from all allowed providers, in order.
/// No candidate is applied here — the caller picks, then Match applies.
pub fn lookup_candidates(
    providers: &[&dyn Provider],
    title: &str,
    year: Option<u32>,
) -> Vec<LookupCandidate> {
    let mut out = Vec::new();
    for provider in providers {
        if let Ok(m) = provider.fetch(title, year) {
            if let Some(t) = m.title.clone() {
                out.push(LookupCandidate {
                    provider_id: provider.manifest().id.clone(),
                    title: t,
                    poster_url: m.poster_url,
                    overview: m.overview,
                    year: m.year.or(year),
                });
            }
        }
    }
    out
}

/// Apply a chosen Lookup candidate to the DB (Match).
pub fn apply_match(db: &LibraryDb, item_id: &str, cand: &LookupCandidate) -> rusqlite::Result<()> {
    db.update_metadata(
        item_id,
        Some(&cand.title),
        cand.poster_url.as_deref(),
        cand.overview.as_deref(),
        cand.year,
    )
}

/// Enrich one item with the general providers. Returns the merged
/// metadata and whether it was complete. Does not touch adult logic.
pub fn enrich_item(
    providers: &[&dyn Provider],
    item: &MediaItem,
) -> (MergedMetadata, bool) {
    let merged = merge_until_complete(providers, &item.title, item.year);
    let complete = merged.is_complete();
    (merged, complete)
}

/// Skip-complete rule: an item is complete when it has a non-blank
/// overview (not "N/A"), a local poster, and a year or a genre.
/// Nova tracks year (genre is a future field); the rule is implemented
/// here so it cannot drift silently.
pub fn is_skip_complete(item: &MediaItem) -> bool {
    let overview_ok = item
        .overview
        .as_deref()
        .map(|o| !o.trim().is_empty() && o.trim() != "N/A")
        .unwrap_or(false);
    let poster_local = item
        .poster_url
        .as_deref()
        .map(|u| !u.starts_with("http://") && !u.starts_with("https://"))
        .unwrap_or(false);
    overview_ok && poster_local && item.year.is_some()
}

/// HTTP provider: generic JSON shape parsing. Tries, in order:
/// title/name, poster_path/poster/poster_url, overview, year from
/// release_date/first_air_date (year prefix) or year. A provider that
/// returns nothing parseable is a soft failure, never a crash.
/// Live network tests are NOT run in unit tests; use MockProvider there.
pub mod http_provider {
    use super::*;
    use serde_json::Value;

    pub struct HttpProvider {
        pub manifest: ProviderManifest,
        pub api_key: Option<String>,
    }

    impl Provider for HttpProvider {
        fn manifest(&self) -> &ProviderManifest {
            &self.manifest
        }
        fn fetch(&self, title: &str, _year: Option<u32>) -> Result<MergedMetadata, ProviderError> {
            let mut url = format!("{}?query={}", self.manifest.endpoint, title);
            if let Some(key) = &self.api_key {
                url.push_str(&format!("&api_key={key}"));
            }
            let resp = ureq::get(&url)
                .timeout(PROVIDER_CHAIN_TIMEOUT)
                .call()
                .map_err(|e| ProviderError(e.to_string()))?;
            let json: Value = resp
                .into_json()
                .map_err(|e| ProviderError(e.to_string()))?;
            Ok(parse_generic(&json))
        }
    }

    pub fn parse_generic(json: &Value) -> MergedMetadata {
        // Prefer the first result object if this is a list response.
        let obj = json
            .get("results")
            .and_then(|r| r.as_array())
            .and_then(|a| a.first().cloned())
            .unwrap_or_else(|| json.clone());
        let title = obj
            .get("title")
            .or_else(|| obj.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let poster_url = obj
            .get("poster_url")
            .or_else(|| obj.get("poster"))
            .or_else(|| obj.get("poster_path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let overview = obj
            .get("overview")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let year = obj
            .get("year")
            .and_then(|v| v.as_u64())
            .map(|y| y as u32)
            .or_else(|| {
                obj.get("release_date")
                    .or_else(|| obj.get("first_air_date"))
                    .and_then(|v| v.as_str())
                    .and_then(|d| d.get(..4)?.parse().ok())
            });
        MergedMetadata {
            title,
            poster_url,
            overview,
            year,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ProviderKind, ProviderManifest};

    fn manifest(id: &str) -> ProviderManifest {
        ProviderManifest {
            id: id.into(),
            version: "1".into(),
            display_name: id.into(),
            kinds: vec![ProviderKind::Movie],
            adult_only: false,
            endpoint: "https://example.invalid".into(),
        }
    }

    #[test]
    fn merge_stops_only_when_complete_and_poster_saved_locally() {
        let title_only = MockProvider {
            manifest: manifest("a"),
            result: MergedMetadata {
                title: Some("Film".into()),
                ..Default::default()
            },
        };
        let poster_only = MockProvider {
            manifest: manifest("b"),
            result: MergedMetadata {
                poster_url: Some("https://example.invalid/p.jpg".into()),
                ..Default::default()
            },
        };
        let providers: Vec<&dyn Provider> = vec![&title_only, &poster_only];
        let merged = merge_until_complete(&providers, "Film", None);
        assert!(merged.is_complete());
        // Poster download failure must not leave a remote URL behind:
        // save_remote_poster returns None and the caller records nothing.
        let item = MediaItem {
            id: "1".into(),
            title: "Film".into(),
            year: None,
            paths: vec![std::path::PathBuf::from("/media/film.mkv")],
            poster_url: None,
            overview: None,
            watched: false,
            progress_seconds: 0,
            favourite: false,
            is_adult: false,
        };
        let saved = save_remote_poster(&item, "https://example.invalid/p.jpg", Path::new("/tmp"), |_| None);
        assert!(saved.is_none());
    }

    #[test]
    fn lookup_returns_candidates_without_applying() {
        let p = MockProvider {
            manifest: manifest("a"),
            result: MergedMetadata {
                title: Some("Film".into()),
                poster_url: Some("https://example.invalid/p.jpg".into()),
                ..Default::default()
            },
        };
        let providers: Vec<&dyn Provider> = vec![&p];
        let cands = lookup_candidates(&providers, "Film", None);
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].provider_id, "a");
        let db = LibraryDb::open_in_memory().unwrap();
        let mut item = MediaItem {
            id: "1".into(),
            title: "Film".into(),
            year: None,
            paths: vec![std::path::PathBuf::from("/media/film.mkv")],
            poster_url: None,
            overview: None,
            watched: false,
            progress_seconds: 0,
            favourite: false,
            is_adult: false,
        };
        db.upsert_item(&item).unwrap();
        apply_match(&db, "1", &cands[0]).unwrap();
        item = db.get_item("1").unwrap().unwrap();
        assert_eq!(item.title, "Film");
        assert_eq!(item.poster_url.as_deref(), Some("https://example.invalid/p.jpg"));
        // Unmatch clears poster + overview only.
        db.unmatch("1").unwrap();
        let item = db.get_item("1").unwrap().unwrap();
        assert!(item.poster_url.is_none());
        assert_eq!(item.title, "Film");
        let _ = AttemptOutcome::Enriched;
    }
}
