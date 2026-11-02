//! Persistent library database (SQLite, via the Rust core).
//!
//! One database file per vault, `cinavault.db`. All library state lives
//! here — never in memory only. Shelves are always built from rows in
//! this database, and only from rows that still have a usable file path
//! (or a Drive id after ingestion). Rows for files that disappeared are
//! purged only when they carry no user data (unwatched, not a favourite,
//! no progress, not on the watchlist); user-data rows are kept and
//! reported as missing, so the user can restore the file or remove the
//! item deliberately.

use crate::catalog::MediaItem;
use rusqlite::{params, Connection, Result as SqlResult};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome {
    NoMatch,
    Failed,
    Timeout,
    EnrichedPosterMissing,
    Enriched,
    SkippedComplete,
}

impl AttemptOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttemptOutcome::NoMatch => "no_match",
            AttemptOutcome::Failed => "failed",
            AttemptOutcome::Timeout => "timeout",
            AttemptOutcome::EnrichedPosterMissing => "enriched_poster_missing",
            AttemptOutcome::Enriched => "enriched",
            AttemptOutcome::SkippedComplete => "skipped_complete",
        }
    }
    /// Retryable outcomes stay pending; enriched / skipped-complete clear.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            AttemptOutcome::NoMatch
                | AttemptOutcome::Failed
                | AttemptOutcome::Timeout
                | AttemptOutcome::EnrichedPosterMissing
        )
    }
}

pub struct LibraryDb {
    conn: Connection,
}

impl LibraryDb {
    pub fn open(path: &Path) -> SqlResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> SqlResult<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS media_items (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                year INTEGER,
                paths_json TEXT NOT NULL DEFAULT '[]',
                poster_url TEXT,
                overview TEXT,
                watched INTEGER NOT NULL DEFAULT 0,
                progress_seconds INTEGER NOT NULL DEFAULT 0,
                favourite INTEGER NOT NULL DEFAULT 0,
                watchlist INTEGER NOT NULL DEFAULT 0,
                is_adult INTEGER NOT NULL DEFAULT 0,
                drive_id TEXT,
                missing INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS adult_enrichment_attempts (
                item_id TEXT PRIMARY KEY,
                outcome TEXT NOT NULL,
                updated_at_unix INTEGER NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE IF NOT EXISTS watch_state (
                item_id TEXT PRIMARY KEY,
                watched INTEGER NOT NULL DEFAULT 0,
                progress_seconds INTEGER NOT NULL DEFAULT 0,
                updated_at_unix INTEGER NOT NULL DEFAULT 0
            );",
        )
    }

    pub fn upsert_item(&self, item: &MediaItem) -> SqlResult<()> {
        let paths_json = serde_json::to_string(&item.paths).unwrap_or_else(|_| "[]".into());
        self.conn.execute(
            "INSERT INTO media_items
             (id, title, year, paths_json, poster_url, overview, watched, progress_seconds, favourite, is_adult)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET title=excluded.title, year=excluded.year,
               paths_json=excluded.paths_json, poster_url=COALESCE(excluded.poster_url, media_items.poster_url),
               overview=COALESCE(excluded.overview, media_items.overview), is_adult=excluded.is_adult",
            params![
                item.id, item.title, item.year, paths_json, item.poster_url, item.overview,
                item.watched as i64, item.progress_seconds as i64, item.favourite as i64,
                item.is_adult as i64,
            ],
        )?;
        Ok(())
    }

    pub fn list_library(&self) -> SqlResult<Vec<MediaItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, year, paths_json, poster_url, overview, watched,
                    progress_seconds, favourite, is_adult FROM media_items
             WHERE missing = 0 ORDER BY title",
        )?;
        let rows = stmt.query_map([], |row| {
            let paths_json: String = row.get(3)?;
            let paths: Vec<PathBuf> = serde_json::from_str(&paths_json).unwrap_or_default();
            Ok(MediaItem {
                id: row.get(0)?,
                title: row.get(1)?,
                year: row.get(2)?,
                paths,
                poster_url: row.get(4)?,
                overview: row.get(5)?,
                watched: row.get::<_, i64>(6)? != 0,
                progress_seconds: row.get::<_, i64>(7)? as u64,
                favourite: row.get::<_, i64>(8)? != 0,
                is_adult: row.get::<_, i64>(9)? != 0,
            })
        })?;
        rows.collect()
    }

    /// Library-only shelves. Trending is deliberately *not* a global
    /// recommendation list: it is library items sorted by recency of
    /// progress, so nothing outside the library can appear on it.
    pub fn continue_watching(&self) -> SqlResult<Vec<MediaItem>> {
        Ok(self
            .list_library()?
            .into_iter()
            .filter(|i| i.progress_seconds > 0 && !i.watched)
            .collect())
    }

    pub fn get_item(&self, id: &str) -> SqlResult<Option<MediaItem>> {
        Ok(self.list_library()?.into_iter().find(|i| i.id == id))
    }

    pub fn update_metadata(
        &self,
        id: &str,
        title: Option<&str>,
        poster_url: Option<&str>,
        overview: Option<&str>,
        year: Option<u32>,
    ) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE media_items SET title=COALESCE(?2, title), poster_url=COALESCE(?3, poster_url),
             overview=COALESCE(?4, overview), year=COALESCE(?5, year) WHERE id=?1",
            params![id, title, poster_url, overview, year],
        )?;
        Ok(())
    }

    pub fn unmatch(&self, id: &str) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE media_items SET poster_url=NULL, overview=NULL WHERE id=?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn set_watch(&self, id: &str, watched: bool, progress: u64) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE media_items SET watched=?2, progress_seconds=?3 WHERE id=?1",
            params![id, watched as i64, progress as i64],
        )?;
        self.conn.execute(
            "INSERT INTO watch_state (item_id, watched, progress_seconds, updated_at_unix)
             VALUES (?1, ?2, ?3, strftime('%s','now'))
             ON CONFLICT(item_id) DO UPDATE SET watched=excluded.watched,
               progress_seconds=excluded.progress_seconds, updated_at_unix=excluded.updated_at_unix",
            params![id, watched as i64, progress as i64],
        )?;
        Ok(())
    }

    pub fn mark_adult(&self, id: &str, is_adult: bool) -> SqlResult<()> {
        self.conn.execute(
            "UPDATE media_items SET is_adult=?2 WHERE id=?1",
            params![id, is_adult as i64],
        )?;
        Ok(())
    }

    pub fn search(&self, query: &str) -> SqlResult<Vec<MediaItem>> {
        let q = query.to_lowercase();
        Ok(self
            .list_library()?
            .into_iter()
            .filter(|i| i.title.to_lowercase().contains(&q))
            .collect())
    }

    /// Purge rows whose files are all gone *and* which carry no user
    /// data. Returns (purged_ids, kept_missing_ids).
    pub fn purge_missing_files(&self) -> SqlResult<(Vec<String>, Vec<String>)> {
        let mut purged = Vec::new();
        let mut kept = Vec::new();
        let items = self.list_library()?;
        for item in items {
            let any_exists = item.paths.iter().any(|p| p.exists());
            if any_exists || item.paths.is_empty() && item.id.starts_with("drive:") {
                continue;
            }
            // Drive-ingested items have no local path by design.
            if item.paths.is_empty() {
                continue;
            }
            if !item.watched && !item.favourite && item.progress_seconds == 0 {
                // watchlist check
                let watchlist: i64 = self.conn.query_row(
                    "SELECT watchlist FROM media_items WHERE id=?1",
                    params![item.id],
                    |r| r.get(0),
                )?;
                if watchlist == 0 {
                    self.conn.execute(
                        "DELETE FROM media_items WHERE id=?1",
                        params![item.id],
                    )?;
                    purged.push(item.id.clone());
                    continue;
                }
            }
            self.conn.execute(
                "UPDATE media_items SET missing=1 WHERE id=?1",
                params![item.id],
            )?;
            kept.push(item.id);
        }
        Ok((purged, kept))
    }

    pub fn record_attempt(&self, item_id: &str, outcome: AttemptOutcome) -> SqlResult<()> {
        if outcome.is_retryable() {
            self.conn.execute(
                "INSERT INTO adult_enrichment_attempts (item_id, outcome, updated_at_unix, attempts)
                 VALUES (?1, ?2, strftime('%s','now'), 1)
                 ON CONFLICT(item_id) DO UPDATE SET outcome=excluded.outcome,
                   updated_at_unix=excluded.updated_at_unix, attempts=attempts+1",
                params![item_id, outcome.as_str()],
            )?;
        } else {
            self.conn.execute(
                "DELETE FROM adult_enrichment_attempts WHERE item_id=?1",
                params![item_id],
            )?;
        }
        Ok(())
    }

    /// Pending adult items, oldest attempt first, deterministic tiebreak.
    pub fn pending_adult_item_ids(&self) -> SqlResult<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT item_id FROM adult_enrichment_attempts
             ORDER BY updated_at_unix ASC, item_id ASC",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    pub fn attempt_count(&self, item_id: &str) -> SqlResult<i64> {
        Ok(self.conn.query_row(
            "SELECT attempts FROM adult_enrichment_attempts WHERE item_id=?1",
            params![item_id],
            |r| r.get(0),
        ).unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::MediaItem;
    use std::path::PathBuf;

    fn item(id: &str) -> MediaItem {
        MediaItem {
            id: id.into(),
            title: format!("Film {id}"),
            year: Some(2024),
            paths: vec![PathBuf::from("/media/film.mkv")],
            poster_url: None,
            overview: None,
            watched: false,
            progress_seconds: 0,
            favourite: false,
            is_adult: false,
        }
    }

    #[test]
    fn upsert_list_search_and_purge() {
        let db = LibraryDb::open_in_memory().unwrap();
        db.upsert_item(&item("a")).unwrap();
        let mut b = item("b");
        b.progress_seconds = 120;
        db.upsert_item(&b).unwrap();
        assert_eq!(db.list_library().unwrap().len(), 2);
        assert_eq!(db.search("film a").unwrap().len(), 1);
        assert_eq!(db.continue_watching().unwrap().len(), 1);
        db.set_watch("b", false, 300).unwrap();
        // Both rows point at a non-existent file; 'a' has no user data
        // and is purged, 'b' has progress and is kept as missing.
        let (purged, kept) = db.purge_missing_files().unwrap();
        assert_eq!(purged, vec!["a".to_string()]);
        assert_eq!(kept, vec!["b".to_string()]);
    }

    #[test]
    fn attempts_retryable_then_cleared() {
        let db = LibraryDb::open_in_memory().unwrap();
        db.record_attempt("x", AttemptOutcome::NoMatch).unwrap();
        assert_eq!(db.pending_adult_item_ids().unwrap(), vec!["x".to_string()]);
        db.record_attempt("x", AttemptOutcome::Enriched).unwrap();
        assert!(db.pending_adult_item_ids().unwrap().is_empty());
    }
}
