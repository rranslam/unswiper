// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Port of `LocalTreeIndexer.index`: walk, filter, pair Live Photos and sidecars, date, group.

use crate::group;
use crate::meta::{exif::exif_date, folder_date, quicktime::video_date};
use crate::model::{DateSource, Decision, MediaItem, MediaKind, SidecarFile, VisitGroup};
use crate::path_util::canonical_path;
use crate::time::{distant_past, Timestamp};
use serde_json::Map;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

pub const PHOTO_EXTS: &[&str] = &[
    "jpg", "jpeg", "png", "heic", "heif", "webp", "gif", "tif", "tiff", "bmp", "raw", "dng",
];
pub const VIDEO_EXTS: &[&str] = &[
    "mp4", "mov", "m4v", "avi", "mkv", "mts", "m2ts", "3gp", "webm",
];
pub const SIDECAR_EXTS: &[&str] = &["aae", "xmp"];
/// Stills that can be the photo half of a Live Photo, in anchor preference order.
pub const LIVE_STILL_EXTS: &[&str] = &["heic", "heif", "jpg", "jpeg"];
/// macOS package directories are skipped like `.skipsPackageDescendants` does.
const PACKAGE_EXTS: &[&str] = &[
    "app",
    "bundle",
    "framework",
    "photoslibrary",
    "plugin",
    "pkg",
    "photolibrary",
    "aplibrary",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexProgress {
    pub files_seen: usize,
    pub media_found: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("Could not read the selected folder.")]
    UnreadableRoot(#[source] std::io::Error),
}

/// Pluggable date readers so tests and the CLI can stub them.
pub trait DateReader {
    fn photo_date(&self, path: &Path) -> Option<Timestamp>;
    fn video_date(&self, path: &Path) -> Option<Timestamp>;
}

/// Real metadata readers (EXIF/TIFF, QuickTime/MP4).
pub struct MetadataDates;

impl DateReader for MetadataDates {
    fn photo_date(&self, path: &Path) -> Option<Timestamp> {
        exif_date(path)
    }
    fn video_date(&self, path: &Path) -> Option<Timestamp> {
        video_date(path)
    }
}

#[derive(Debug, Clone)]
struct Entry {
    path: PathBuf,
    ext: String,
    size: i64,
    modified_at: Option<Timestamp>,
}

fn ext_of(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn is_media(ext: &str) -> bool {
    PHOTO_EXTS.contains(&ext) || VIDEO_EXTS.contains(&ext) || SIDECAR_EXTS.contains(&ext)
}

fn walk(
    dir: &Path,
    seen: &mut usize,
    out: &mut Vec<Entry>,
    progress: &mut dyn FnMut(IndexProgress),
) -> std::io::Result<()> {
    let mut children: Vec<_> = std::fs::read_dir(dir)?.filter_map(Result::ok).collect();
    children.sort_by_key(|e| e.file_name());
    for child in children {
        let name = child.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        *seen += 1;
        if *seen % 50 == 0 {
            progress(IndexProgress {
                files_seen: *seen,
                media_found: out.len(),
            });
        }
        let Ok(meta) = std::fs::symlink_metadata(child.path()) else {
            continue;
        };
        let path = child.path();
        if meta.is_dir() {
            if PACKAGE_EXTS.contains(&ext_of(&path).as_str()) {
                continue;
            }
            // Unreadable sub-folders are skipped, like FileManager's enumerator.
            let _ = walk(&path, seen, out, progress);
            continue;
        }
        if !meta.is_file() {
            continue; // symlinks, sockets, devices
        }
        let ext = ext_of(&path);
        if !is_media(&ext) {
            continue;
        }
        let lower = path_str(&path).to_lowercase();
        if lower.contains("/_rejected/") || lower.contains("/.trash/") {
            continue;
        }
        out.push(Entry {
            path,
            ext,
            size: meta.len() as i64,
            modified_at: meta.modified().ok().map(Timestamp::from),
        });
    }
    Ok(())
}

/// Parent folder relative to the root, "" for the root itself (both canonicalised).
pub fn relative_folder_path(dir: &Path, root: &Path) -> String {
    let d = canonical_path(dir);
    let r = canonical_path(root);
    match d.strip_prefix(&r) {
        Ok(rel) => path_str(rel).trim_matches('/').to_string(),
        Err(_) => String::new(),
    }
}

fn anchor_rank(ext: &str) -> usize {
    LIVE_STILL_EXTS
        .iter()
        .position(|e| *e == ext)
        .unwrap_or(LIVE_STILL_EXTS.len())
}

/// Indexes `root`. Returns items sorted by date and the visit groups.
pub fn index(
    root: &Path,
    visit_gap_hours: f64,
    existing_decisions: &HashMap<String, Decision>,
    dates: &dyn DateReader,
    mut on_progress: impl FnMut(IndexProgress),
) -> Result<(Vec<MediaItem>, Vec<VisitGroup>), IndexError> {
    let mut entries = vec![];
    let mut seen = 0usize;
    std::fs::read_dir(root).map_err(IndexError::UnreadableRoot)?;
    walk(root, &mut seen, &mut entries, &mut on_progress).map_err(IndexError::UnreadableRoot)?;

    // Pair: same directory + base name, case-insensitive.
    let mut sets: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for e in entries {
        let key = path_str(&e.path.with_extension("")).to_lowercase();
        sets.entry(key).or_default().push(e);
    }

    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut items = vec![];
    for set in sets.into_values() {
        let photos: Vec<&Entry> = set
            .iter()
            .filter(|e| PHOTO_EXTS.contains(&e.ext.as_str()))
            .collect();
        let videos: Vec<&Entry> = set
            .iter()
            .filter(|e| VIDEO_EXTS.contains(&e.ext.as_str()))
            .collect();
        let mut attached: Vec<&Entry> = set
            .iter()
            .filter(|e| SIDECAR_EXTS.contains(&e.ext.as_str()))
            .collect();
        let mut standalone: Vec<&Entry> = photos.iter().chain(videos.iter()).copied().collect();
        if photos
            .iter()
            .any(|e| LIVE_STILL_EXTS.contains(&e.ext.as_str()))
        {
            attached.extend(videos.iter().filter(|e| e.ext == "mov"));
            standalone = photos
                .iter()
                .copied()
                .chain(videos.iter().copied().filter(|e| e.ext != "mov"))
                .collect();
        }
        // Anchor: a Live Photo still (HEIC before JPEG), else the first primary.
        let Some(anchor) = standalone
            .iter()
            .min_by_key(|e| anchor_rank(&e.ext))
            .map(|e| e.path.clone())
        else {
            continue; // orphan sidecars are ignored
        };

        for e in standalone {
            let kind = if PHOTO_EXTS.contains(&e.ext.as_str()) {
                MediaKind::Photo
            } else {
                MediaKind::Video
            };
            let captured = match kind {
                MediaKind::Photo => dates.photo_date(&e.path),
                MediaKind::Video => dates.video_date(&e.path),
            };
            let folder_path = relative_folder_path(e.path.parent().unwrap_or(root), root);
            let (taken_at, source) = if let Some(c) = captured {
                (c, DateSource::Capture)
            } else if let Some(m) = folder_date::parse(&folder_path) {
                (m.date(), DateSource::Folder)
            } else {
                (distant_past(), DateSource::Unknown)
            };
            let rel_folder = folder_path
                .split('/')
                .find(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| root_name.clone());
            let path = path_str(&e.path);
            let sidecars = if e.path == anchor {
                attached
                    .iter()
                    .map(|s| SidecarFile {
                        path: path_str(&s.path),
                        size_bytes: s.size,
                        modified_at: s.modified_at,
                        extra: Map::new(),
                    })
                    .collect()
            } else {
                vec![]
            };
            items.push(MediaItem {
                decision: existing_decisions.get(&path).copied().unwrap_or_default(),
                path,
                kind,
                size_bytes: e.size,
                taken_at,
                relative_folder: rel_folder,
                modified_at: e.modified_at,
                sidecars,
                is_undated: source == DateSource::Unknown,
                date_source: Some(source),
                folder_path: Some(folder_path),
                extra: Map::new(),
            });
        }
    }

    on_progress(IndexProgress {
        files_seen: seen,
        media_found: items.len(),
    });
    items.sort_by(|a, b| {
        a.taken_at
            .cmp(&b.taken_at)
            .then_with(|| a.path.cmp(&b.path))
    });
    let groups = group::group(&items, visit_gap_hours);
    Ok((items, groups))
}
