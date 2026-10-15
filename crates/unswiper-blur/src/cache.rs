// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! `quality-cache.json`: scores keyed by path, valid while size, mtime and the algorithm
//! version match (plan §15.3). Rebuildable; losing it only costs a rescan.

use crate::score::Score;
use crate::ALGO_VERSION;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "quality-cache.json";

/// A file's identity for cache purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stamp {
    pub file_size: u64,
    /// Modification time, Unix seconds.
    pub mtime: i64,
}

impl Stamp {
    pub fn of(path: &Path) -> Option<Stamp> {
        let m = std::fs::metadata(path).ok()?;
        let mtime = m
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs() as i64;
        Some(Stamp {
            file_size: m.len(),
            mtime,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    #[serde(flatten)]
    pub score: Score,
    #[serde(flatten)]
    pub stamp: Stamp,
    /// When it was scored, Unix seconds.
    pub scored_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityCache {
    pub v: u32,
    pub algo: u32,
    pub entries: BTreeMap<String, Entry>,
}

impl Default for QualityCache {
    fn default() -> Self {
        QualityCache {
            v: 1,
            algo: ALGO_VERSION,
            entries: BTreeMap::new(),
        }
    }
}

impl QualityCache {
    /// `<support dir>/quality-cache.json`.
    pub fn path_in(dir: &Path) -> PathBuf {
        dir.join(FILE_NAME)
    }

    /// Loads a cache; a missing, damaged or other-algorithm file gives an empty cache.
    pub fn load(path: &Path) -> QualityCache {
        std::fs::read(path)
            .ok()
            .and_then(|d| serde_json::from_slice::<QualityCache>(&d).ok())
            .filter(|c| c.algo == ALGO_VERSION && c.v == 1)
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let data = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        unswiper_core::store::write_atomic(path, &data)
    }

    /// The cached score if the file hasn't changed.
    pub fn get(&self, path: &str, stamp: Stamp) -> Option<Score> {
        self.entries
            .get(path)
            .filter(|e| e.stamp == stamp)
            .map(|e| e.score)
    }

    pub fn put(&mut self, path: &str, stamp: Stamp, score: Score, now: i64) {
        self.entries.insert(
            path.to_string(),
            Entry {
                score,
                stamp,
                scored_at: now,
            },
        );
    }

    /// Drops entries for paths not in `keep` (files that were removed or committed).
    pub fn retain_paths(&mut self, keep: &std::collections::HashSet<String>) {
        self.entries.retain(|p, _| keep.contains(p));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(s: f32) -> Score {
        Score {
            sharpness: s,
            global: 1.0,
            motion: 0.1,
            mean_luma: 100.0,
            confidence: 1.0,
        }
    }

    #[test]
    fn round_trip_and_invalidation() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.jpg");
        std::fs::write(&f, b"1234").unwrap();
        let st = Stamp::of(&f).unwrap();
        assert_eq!(st.file_size, 4);
        let mut c = QualityCache::default();
        c.put("/a.jpg", st, score(42.0), 7);
        assert_eq!(c.get("/a.jpg", st), Some(score(42.0)));
        let changed = Stamp { file_size: 5, ..st };
        assert_eq!(c.get("/a.jpg", changed), None);
        assert_eq!(c.get("/b.jpg", st), None);
        let p = QualityCache::path_in(d.path());
        c.save(&p).unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains("\"sharpness\"") && text.contains("\"fileSize\""));
        assert_eq!(QualityCache::load(&p), c);
        c.retain_paths(&Default::default());
        assert!(c.entries.is_empty());
        assert_eq!(Stamp::of(&d.path().join("missing")), None);
    }

    #[test]
    fn bad_or_old_files_start_empty() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(FILE_NAME);
        assert_eq!(QualityCache::load(&p), QualityCache::default());
        std::fs::write(&p, b"{nope").unwrap();
        assert_eq!(QualityCache::load(&p), QualityCache::default());
        let mut old = QualityCache {
            algo: ALGO_VERSION + 1,
            ..Default::default()
        };
        old.put(
            "/x",
            Stamp {
                file_size: 1,
                mtime: 1,
            },
            score(1.0),
            1,
        );
        old.save(&p).unwrap();
        assert!(QualityCache::load(&p).entries.is_empty());
    }
}
