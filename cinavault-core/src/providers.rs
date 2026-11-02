//! Metadata provider plugins.
//!
//! Providers are versioned manifests, not hardcoded code paths. A provider
//! that dies, changes its API, or returns nothing must degrade gracefully:
//! the enrichment job merges providers until title AND poster are found,
//! and a missing provider never blocks the library.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Movie,
    Series,
    Adult,
    Poster,
}

/// A versioned provider plugin manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderManifest {
    /// Stable provider id, e.g. "tmdb", "cinemeta", "stashdb".
    pub id: String,
    /// Manifest version, e.g. "1". Bumping it is a provider release.
    pub version: String,
    pub display_name: String,
    pub kinds: Vec<ProviderKind>,
    /// If true, this provider is never used in store-safe builds.
    pub adult_only: bool,
    /// Base URL template or endpoint descriptor for the provider.
    pub endpoint: String,
}

impl ProviderManifest {
    /// True if this provider may run in the given build mode.
    pub fn allowed_in_store_safe_build(&self, store_safe: bool) -> bool {
        !(store_safe && self.adult_only)
    }
}

/// Merge rule for enrichment: keep merging provider results until both a
/// title and a poster are present. A title-only match must not suppress a
/// later provider that could supply a poster.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MergedMetadata {
    pub title: Option<String>,
    pub poster_url: Option<String>,
    pub overview: Option<String>,
    pub year: Option<u32>,
}

impl MergedMetadata {
    pub fn is_complete(&self) -> bool {
        self.title.is_some() && self.poster_url.is_some()
    }

    pub fn merge(&mut self, other: MergedMetadata) {
        if self.title.is_none() {
            self.title = other.title;
        }
        if self.poster_url.is_none() {
            self.poster_url = other.poster_url;
        }
        if self.overview.is_none() {
            self.overview = other.overview;
        }
        if self.year.is_none() {
            self.year = other.year;
        }
    }
}

/// Built-in manifests shipped in v0.1.0. These are descriptors only:
/// no network provider code is included in this scaffold.
pub fn builtin_manifests() -> Vec<ProviderManifest> {
    vec![
        ProviderManifest {
            id: "tmdb".into(),
            version: "1".into(),
            display_name: "TMDB".into(),
            kinds: vec![ProviderKind::Movie, ProviderKind::Series, ProviderKind::Poster],
            adult_only: false,
            endpoint: "https://api.themoviedb.org/3".into(),
        },
        ProviderManifest {
            id: "cinemeta".into(),
            version: "1".into(),
            display_name: "Cinemeta".into(),
            kinds: vec![ProviderKind::Movie, ProviderKind::Series, ProviderKind::Poster],
            adult_only: false,
            endpoint: "https://v3-cinemeta.strem.io".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_only_match_does_not_block_poster() {
        let mut merged = MergedMetadata {
            title: Some("Example".into()),
            ..Default::default()
        };
        assert!(!merged.is_complete());
        merged.merge(MergedMetadata {
            poster_url: Some("https://example.invalid/poster.jpg".into()),
            ..Default::default()
        });
        assert!(merged.is_complete());
    }

    #[test]
    fn adult_providers_are_store_safe_gated() {
        let adult = ProviderManifest {
            id: "adult-example".into(),
            version: "1".into(),
            display_name: "Adult Example".into(),
            kinds: vec![ProviderKind::Adult],
            adult_only: true,
            endpoint: String::new(),
        };
        assert!(!adult.allowed_in_store_safe_build(true));
        assert!(adult.allowed_in_store_safe_build(false));
    }
}
