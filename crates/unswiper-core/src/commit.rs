// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Port of `CommitService.swift`: validate → journal (pending manifest) → move → finalize,
//! and restore (also of interrupted, still-pending commits).

use crate::model::{
    CommitManifest, CommitMode, EntryState, ManifestEntry, ManifestStatus, MediaItem,
};
use crate::path_util::{canonical_path, is_inside};
use crate::store::{write_manifest, LibraryStore};
use crate::time::{stamp, Timestamp};
use crate::trash::Trasher;
use chrono::Utc;
use serde_json::Map;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommitResult {
    /// Sets (item + sidecars) fully moved.
    pub moved: usize,
    pub failed: Vec<(String, String)>,
    pub bytes: i64,
    pub dry_run: bool,
    /// Dry run only: how many items would be moved.
    pub would_move: usize,
    pub manifest_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RestoreResult {
    pub restored: usize,
    pub failed: Vec<(String, String)>,
    /// Entries of an interrupted commit whose file never left its original place.
    pub already_in_place: usize,
}

pub const MSG_NO_ROOT: &str = "No library folder; refusing to move.";
pub const MSG_SYMLINK: &str = "Is a symbolic link; not moved.";
pub const MSG_OUTSIDE: &str = "Outside the library folder; not moved.";
pub const MSG_MISSING: &str = "File is missing or not a regular file; re-index.";
pub const MSG_SIZE: &str = "File size changed since indexing; re-index.";
pub const MSG_NO_MTIME: &str = "No modification date recorded; re-index before committing.";
pub const MSG_MTIME: &str = "File modified since indexing; re-index.";

/// Returns a failure reason, or `None` if the file is safe to move.
pub fn validate(
    path: &str,
    size: i64,
    modified_at: Option<Timestamp>,
    root: Option<&Path>,
) -> Option<&'static str> {
    let Some(root) = root else {
        return Some(MSG_NO_ROOT);
    };
    let root = canonical_path(root);
    let raw = Path::new(path);
    if std::fs::symlink_metadata(raw)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Some(MSG_SYMLINK);
    }
    let file = canonical_path(raw);
    if !is_inside(&file, &root) {
        return Some(MSG_OUTSIDE);
    }
    let meta = match std::fs::metadata(&file) {
        Ok(m) if m.is_file() => m,
        _ => return Some(MSG_MISSING),
    };
    if meta.len() as i64 != size {
        return Some(MSG_SIZE);
    }
    let Some(expected) = modified_at else {
        return Some(MSG_NO_MTIME);
    };
    // library.json stores whole seconds, so allow 1 s of slack.
    match meta.modified().ok().map(Timestamp::from) {
        Some(actual) if (actual - expected).num_milliseconds().abs() < 1000 => None,
        _ => Some(MSG_MTIME),
    }
}

/// Moves `src` to `dest` without overwriting. Falls back to copy + fsync + verify + remove
/// across devices (plan §6.5).
pub fn move_file(src: &Path, dest: &Path) -> std::io::Result<()> {
    if std::fs::symlink_metadata(dest).is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "A file already exists at the destination.",
        ));
    }
    match std::fs::rename(src, dest) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(18) => copy_then_remove(src, dest),
        Err(e) => Err(e),
    }
}

fn copy_then_remove(src: &Path, dest: &Path) -> std::io::Result<()> {
    let expected = std::fs::metadata(src)?.len();
    std::fs::copy(src, dest)?;
    let f = std::fs::File::open(dest)?;
    f.sync_all()?;
    if f.metadata()?.len() != expected {
        let _ = std::fs::remove_file(dest);
        return Err(std::io::Error::other("Copy verification failed."));
    }
    if let Ok(m) = std::fs::metadata(src).and_then(|m| m.modified()) {
        let _ = f.set_modified(m);
    }
    std::fs::remove_file(src)
}

struct Options<'a> {
    mode: CommitMode,
    root: Option<&'a Path>,
    store: &'a LibraryStore,
    trasher: &'a dyn Trasher,
    now: Timestamp,
    stop_after_moves: Option<usize>,
}

/// Moves rejected items (and their sidecars) to the trash or quarantine.
///
/// Order: validate every set; write a `pending` manifest listing every planned move before
/// touching any file; update it after each move; mark it `complete`.
/// The members of one visit, in visit order (paths the index no longer has are skipped).
/// Commits act on one visit at a time, like the Mac app: pass these to [`commit_rejects`].
pub fn visit_members(items: &[MediaItem], group: &crate::VisitGroup) -> Vec<MediaItem> {
    let by_path: std::collections::HashMap<&str, &MediaItem> =
        items.iter().map(|i| (i.path.as_str(), i)).collect();
    group
        .item_paths
        .iter()
        .filter_map(|p| by_path.get(p.as_str()).map(|i| (*i).clone()))
        .collect()
}

pub fn commit_rejects(
    items: &[MediaItem],
    mode: CommitMode,
    library_root: Option<&Path>,
    store: &LibraryStore,
    trasher: &dyn Trasher,
) -> std::io::Result<CommitResult> {
    commit_rejects_at(items, mode, library_root, store, trasher, Utc::now(), None)
}

/// [`commit_rejects`] with an explicit clock and a crash-simulation hook
/// (`stop_after_moves`: return after N moves without finalizing the manifest).
pub fn commit_rejects_at(
    items: &[MediaItem],
    mode: CommitMode,
    library_root: Option<&Path>,
    store: &LibraryStore,
    trasher: &dyn Trasher,
    now: Timestamp,
    stop_after_moves: Option<usize>,
) -> std::io::Result<CommitResult> {
    run(
        items,
        Options {
            mode,
            root: library_root,
            store,
            trasher,
            now,
            stop_after_moves,
        },
    )
}

fn run(items: &[MediaItem], o: Options) -> std::io::Result<CommitResult> {
    let rejects: Vec<&MediaItem> = items
        .iter()
        .filter(|i| i.decision == crate::model::Decision::Reject)
        .collect();
    let dry = o.mode == CommitMode::DryRun;
    if rejects.is_empty() {
        return Ok(CommitResult {
            dry_run: dry,
            ..Default::default()
        });
    }
    if dry {
        return Ok(CommitResult {
            bytes: rejects.iter().map(|i| i.total_bytes()).sum(),
            dry_run: true,
            would_move: rejects.len(),
            ..Default::default()
        });
    }

    let id = stamp(&o.now);
    let dest_dir = match o.root {
        Some(r) => r.join("_Rejected/Unswiper"),
        None => o.store.quarantine_root(),
    }
    .join(&id);

    // 1. Validate every set before moving anything.
    let mut failed: Vec<(String, String)> = vec![];
    let mut entries: Vec<ManifestEntry> = vec![];
    let mut sets: Vec<std::ops::Range<usize>> = vec![];
    let mut planned_names: HashSet<String> = HashSet::new();
    for item in rejects {
        let files: Vec<(&str, i64, Option<Timestamp>)> =
            std::iter::once((item.path.as_str(), item.size_bytes, item.modified_at))
                .chain(
                    item.sidecars
                        .iter()
                        .map(|s| (s.path.as_str(), s.size_bytes, s.modified_at)),
                )
                .collect();
        if let Some(problem) = files.iter().find_map(|f| validate(f.0, f.1, f.2, o.root)) {
            failed.push((item.path.clone(), problem.to_string()));
            continue;
        }
        let start = entries.len();
        for (path, size, _) in files {
            let mut planned = String::new();
            if o.mode == CommitMode::Quarantine {
                let mut name = Path::new(path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                if planned_names.contains(&name.to_lowercase()) {
                    name = format!(
                        "{}-{name}",
                        uuid::Uuid::new_v4().simple().to_string()[..8].to_uppercase()
                    );
                }
                planned_names.insert(name.to_lowercase());
                planned = dest_dir.join(&name).to_string_lossy().into_owned();
            }
            entries.push(ManifestEntry {
                original_path: path.to_string(),
                result_path: planned,
                size_bytes: size,
                moved_at: None,
                state: Some(EntryState::Pending),
                error: None,
                extra: Map::new(),
            });
        }
        sets.push(start..entries.len());
    }
    if entries.is_empty() {
        return Ok(CommitResult {
            failed,
            ..Default::default()
        });
    }

    // 2. Journal first.
    let mut manifest = CommitManifest {
        status: Some(ManifestStatus::Pending),
        id: id.clone(),
        created_at: o.now,
        mode: o.mode,
        library_root: o.root.map(|r| r.to_string_lossy().into_owned()),
        entries,
        extra: Map::new(),
    };
    let manifest_path = o.store.commits_dir().join(format!("{id}.json"));
    write_manifest(&manifest, std::slice::from_ref(&manifest_path))?;
    if o.mode == CommitMode::Quarantine {
        std::fs::create_dir_all(&dest_dir)?;
    }

    // 3. Move, recording each result.
    let (mut moved, mut bytes, mut moves_done) = (0usize, 0i64, 0usize);
    for set in sets {
        let mut set_failed = false;
        for i in set {
            if o.stop_after_moves.is_some_and(|n| moves_done >= n) {
                return Ok(CommitResult {
                    moved,
                    failed,
                    bytes,
                    manifest_path: Some(manifest_path),
                    ..Default::default()
                });
            }
            let src = PathBuf::from(&manifest.entries[i].original_path);
            let res = if o.mode == CommitMode::Quarantine {
                move_file(&src, Path::new(&manifest.entries[i].result_path))
            } else {
                o.trasher.trash(&src).map(|r| {
                    manifest.entries[i].result_path = r
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default();
                })
            };
            let e = &mut manifest.entries[i];
            match res {
                Ok(()) => {
                    e.state = Some(EntryState::Moved);
                    e.moved_at = Some(Utc::now());
                    bytes += e.size_bytes;
                    moves_done += 1;
                }
                Err(err) => {
                    e.state = Some(EntryState::Failed);
                    e.error = Some(err.to_string());
                    failed.push((e.original_path.clone(), err.to_string()));
                    set_failed = true;
                }
            }
            if let Err(err) = write_manifest(&manifest, std::slice::from_ref(&manifest_path)) {
                failed.push((
                    "manifest.json".into(),
                    format!("Could not update manifest: {err}"),
                ));
            }
        }
        if !set_failed {
            moved += 1;
        }
    }

    // 4. Finalize.
    manifest.status = Some(ManifestStatus::Complete);
    if !manifest
        .entries
        .iter()
        .any(|e| e.effective_state() == EntryState::Moved)
    {
        let _ = std::fs::remove_file(&manifest_path);
        if o.mode == CommitMode::Quarantine {
            let _ = std::fs::remove_dir_all(&dest_dir);
        }
        return Ok(CommitResult {
            moved,
            failed,
            bytes,
            ..Default::default()
        });
    }
    let mut paths = vec![manifest_path.clone()];
    if o.mode == CommitMode::Quarantine {
        paths.push(dest_dir.join("manifest.json"));
    }
    if let Err(err) = write_manifest(&manifest, &paths) {
        failed.push((
            "manifest.json".into(),
            format!("Could not write manifest: {err}"),
        ));
    }
    Ok(CommitResult {
        moved,
        failed,
        bytes,
        manifest_path: Some(manifest_path),
        ..Default::default()
    })
}

pub const MSG_TRASH_UNKNOWN: &str =
    "Couldn't locate this file in the Trash. Open the Trash and use Restore / Put Back.";
pub const MSG_TRASH_GONE: &str = "Couldn't locate this file in the Trash (was the Trash emptied?).";
pub const MSG_NO_RECORD: &str = "No record of where this file was moved.";
pub const MSG_TARGET_EXISTS: &str = "A file already exists at the original path.";

/// Removes the freedesktop `.trashinfo` for a file restored out of `…/Trash/files/`.
fn remove_trashinfo(src: &Path) {
    let (Some(parent), Some(name)) = (src.parent(), src.file_name()) else {
        return;
    };
    if parent.file_name().is_some_and(|n| n == "files") {
        if let Some(trash) = parent.parent() {
            let mut n = name.to_owned();
            n.push(".trashinfo");
            let _ = std::fs::remove_file(trash.join("info").join(n));
        }
    }
}

/// Moves every moved file in the manifest back to its original path.
/// Works for complete manifests and for `pending` ones left by an interrupted commit.
pub fn restore(manifest: &CommitManifest) -> RestoreResult {
    let mut r = RestoreResult::default();
    let trash = manifest.mode == CommitMode::Trash;
    for e in &manifest.entries {
        let dest = Path::new(&e.original_path);
        match e.effective_state() {
            EntryState::Failed => continue,
            EntryState::Pending => {
                if dest.exists() {
                    r.already_in_place += 1;
                    continue;
                }
                if e.result_path.is_empty() {
                    r.failed
                        .push((e.original_path.clone(), MSG_TRASH_UNKNOWN.into()));
                    continue;
                }
            }
            EntryState::Moved => {}
        }
        if e.result_path.is_empty() {
            let msg = if trash {
                MSG_TRASH_UNKNOWN
            } else {
                MSG_NO_RECORD
            };
            r.failed.push((e.original_path.clone(), msg.into()));
            continue;
        }
        let src = Path::new(&e.result_path);
        if !src.exists() {
            let msg = if trash {
                MSG_TRASH_GONE.to_string()
            } else {
                format!("Moved file not found at {}.", e.result_path)
            };
            r.failed.push((e.original_path.clone(), msg));
            continue;
        }
        if dest.exists() {
            r.failed
                .push((e.original_path.clone(), MSG_TARGET_EXISTS.into()));
            continue;
        }
        let res = dest
            .parent()
            .map(std::fs::create_dir_all)
            .unwrap_or(Ok(()))
            .and_then(|_| move_file(src, dest));
        match res {
            Ok(()) => {
                if trash {
                    remove_trashinfo(src);
                }
                r.restored += 1;
            }
            Err(err) => r.failed.push((e.original_path.clone(), err.to_string())),
        }
    }
    r
}
