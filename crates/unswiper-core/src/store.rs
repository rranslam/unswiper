// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Port of `LibraryStore.swift`: `library.json` (schema v1), commit manifests, backups.
//! Writes are atomic (temp file + fsync + rename).

use crate::model::{CommitManifest, LibrarySnapshot};
use crate::time::{stamp, Timestamp};
use chrono::Utc;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{0}")]
    SaveBlocked(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug)]
pub struct LoadResult {
    pub snapshot: LibrarySnapshot,
    /// Set when library.json existed but could not be read; describes what happened.
    pub problem: Option<String>,
    /// Where the unreadable file was copied to.
    pub backup_path: Option<PathBuf>,
}

/// Per-platform default support folder:
/// Linux `$XDG_DATA_HOME/unswiper` (or `~/.local/share/unswiper`),
/// macOS `~/Library/Application Support/Unswiper` (same as the Swift app).
pub fn default_support_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        return home.join("Library/Application Support/Unswiper");
    }
    match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(x) => PathBuf::from(x).join("unswiper"),
        None => home.join(".local/share/unswiper"),
    }
}

/// Writes `data` to `path` atomically.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(
        ".{}.tmp-{}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4().simple()
    ));
    let res = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
        Ok(())
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

#[derive(Debug, Clone)]
pub struct LibraryStore {
    dir: PathBuf,
    /// Set when library.json could not be read *and* not backed up; `save` refuses to overwrite it.
    pub save_blocked_reason: Option<String>,
}

impl LibraryStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        LibraryStore {
            dir: dir.into(),
            save_blocked_reason: None,
        }
    }

    pub fn support_dir(&self) -> &Path {
        &self.dir
    }
    pub fn snapshot_path(&self) -> PathBuf {
        self.dir.join("library.json")
    }
    pub fn quarantine_root(&self) -> PathBuf {
        self.dir.join("Quarantine")
    }
    pub fn commits_dir(&self) -> PathBuf {
        self.dir.join("commits")
    }

    /// All unrestored manifests (`*.json` in `commits/`), oldest first by name.
    pub fn manifest_paths(&self) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(self.commits_dir())
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|e| e == "json"))
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
        v
    }

    /// Most recent manifest that has not been restored yet.
    pub fn latest_manifest_path(&self) -> Option<PathBuf> {
        self.manifest_paths().pop()
    }

    /// Manifests left `pending` by an interrupted commit.
    pub fn pending_manifest_paths(&self) -> Vec<PathBuf> {
        self.manifest_paths()
            .into_iter()
            .filter(|p| read_manifest(p).map(|m| m.is_pending()).unwrap_or(false))
            .collect()
    }

    /// Renames a manifest to `<name>.restored` so it is no longer offered for restore.
    pub fn mark_restored(&self, manifest: &Path) -> std::io::Result<PathBuf> {
        let mut to = manifest.as_os_str().to_owned();
        to.push(".restored");
        let to = PathBuf::from(to);
        std::fs::rename(manifest, &to)?;
        Ok(to)
    }

    /// Loads library.json. Missing = fresh start. Unreadable, undecodable or newer-format files
    /// are copied to `library.json.corrupt-<stamp>` and reported, never silently replaced.
    pub fn load(&mut self) -> LoadResult {
        self.save_blocked_reason = None;
        let path = self.snapshot_path();
        if !path.exists() {
            return LoadResult {
                snapshot: LibrarySnapshot::empty(),
                problem: None,
                backup_path: None,
            };
        }
        let reason = match std::fs::read(&path) {
            Err(e) => format!("it could not be read ({e})"),
            Ok(data) => match serde_json::from_slice::<LibrarySnapshot>(&data) {
                Err(e) => format!("it is damaged ({e})"),
                Ok(snap) => {
                    let v = snap.schema_version.unwrap_or(1);
                    if v <= LibrarySnapshot::CURRENT_SCHEMA_VERSION {
                        return LoadResult {
                            snapshot: snap,
                            problem: None,
                            backup_path: None,
                        };
                    }
                    format!("it was saved by a newer version of Unswiper (format {v})")
                }
            },
        };
        match self.backup_snapshot(Utc::now()) {
            Ok(backup) => LoadResult {
                snapshot: LibrarySnapshot::empty(),
                problem: Some(format!(
                    "Your saved library couldn't be opened because {reason}. A copy was saved as {} in {}. Unswiper started with an empty library; re-index your folder to continue.",
                    backup.file_name().unwrap_or_default().to_string_lossy(),
                    self.dir.display()
                )),
                backup_path: Some(backup),
            },
            Err(e) => {
                self.save_blocked_reason = Some(format!("library.json couldn't be backed up ({e}); not overwriting it."));
                LoadResult {
                    snapshot: LibrarySnapshot::empty(),
                    problem: Some(format!(
                        "Your saved library couldn't be opened because {reason}, and a backup copy couldn't be made. Unswiper won't overwrite it. Quit and copy {} somewhere safe, then remove it.",
                        path.display()
                    )),
                    backup_path: None,
                }
            }
        }
    }

    /// Copies library.json to `library.json.corrupt-<stamp>[-n]`.
    pub fn backup_snapshot(&self, now: Timestamp) -> std::io::Result<PathBuf> {
        let base = format!("library.json.corrupt-{}", stamp(&now));
        let mut backup = self.dir.join(&base);
        let mut n = 1;
        while backup.exists() {
            n += 1;
            backup = self.dir.join(format!("{base}-{n}"));
        }
        std::fs::copy(self.snapshot_path(), &backup)?;
        Ok(backup)
    }

    /// Saves (schemaVersion forced to 1) in Swift's byte layout.
    pub fn save(&self, snapshot: &LibrarySnapshot) -> Result<(), StoreError> {
        if let Some(r) = &self.save_blocked_reason {
            return Err(StoreError::SaveBlocked(r.clone()));
        }
        let mut s = snapshot.clone();
        s.schema_version = Some(LibrarySnapshot::CURRENT_SCHEMA_VERSION);
        write_atomic(&self.snapshot_path(), s.to_json().as_bytes())?;
        Ok(())
    }
}

/// Writes the same manifest to each path (atomic each).
pub fn write_manifest(m: &CommitManifest, paths: &[PathBuf]) -> std::io::Result<()> {
    let data = m.to_json();
    for p in paths {
        write_atomic(p, data.as_bytes())?;
    }
    Ok(())
}

pub fn read_manifest(path: &Path) -> Result<CommitManifest, StoreError> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dir_respects_xdg() {
        // Only checks shape; env is process-global so we don't mutate it here.
        let d = default_support_dir();
        assert!(d.ends_with("unswiper") || d.ends_with("Unswiper"));
    }

    #[test]
    fn write_atomic_failure_cleans_up() {
        let d = tempfile::tempdir().unwrap();
        // Target is an existing directory: rename fails.
        std::fs::create_dir(d.path().join("x")).unwrap();
        std::fs::write(d.path().join("x/f"), b"").unwrap();
        assert!(write_atomic(&d.path().join("x"), b"data").is_err());
        let leftovers: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty());
    }
}
