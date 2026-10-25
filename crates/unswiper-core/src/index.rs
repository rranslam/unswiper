// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Port of `LocalTreeIndexer.index`: walk, filter, pair Live Photos and sidecars, date, group.

use crate::group;
use crate::meta::{exif::exif_date, folder_date, quicktime::video_date};
use crate::model::{
    DateSource, Decision, Location, LocationSource, MediaItem, MediaKind, SidecarFile, VisitGroup,
};
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
    /// Capture location (plan §20.1). Readers without location support return `None`.
    fn location(&self, _path: &Path, _kind: MediaKind) -> Option<Location> {
        None
    }
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
    fn location(&self, path: &Path, kind: MediaKind) -> Option<Location> {
        file_location(path, kind)
    }
}

/// The file's own GPS: EXIF for photos, QuickTime/MP4 for videos. `0,0` counts as none.
pub fn file_location(path: &Path, kind: MediaKind) -> Option<Location> {
    let g = crate::meta::info::read(path).gps?;
    let source = match kind {
        MediaKind::Photo => LocationSource::Exif,
        MediaKind::Video => LocationSource::Quicktime,
    };
    Location::new(g.lat, g.lon, source)
}

#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub(crate) path: PathBuf,
    pub(crate) ext: String,
    pub(crate) size: i64,
    pub(crate) modified_at: Option<Timestamp>,
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

/// Pairing key: same directory + base name, case-insensitive.
pub(crate) fn set_key(path: &Path) -> String {
    path_str(&path.with_extension("")).to_lowercase()
}

/// `true` for paths the indexer never looks at: hidden components below `root`,
/// `_Rejected` / `.trash` folders, macOS packages.
pub(crate) fn is_excluded(path: &Path, root: &Path) -> bool {
    let lower = path_str(path).to_lowercase();
    if lower.contains("/_rejected/") || lower.contains("/.trash/") || lower.ends_with("/_rejected")
    {
        return true;
    }
    let rel = path.strip_prefix(root).unwrap_or(path);
    let comps: Vec<_> = rel.components().collect();
    comps.iter().enumerate().any(|(i, c)| {
        let n = c.as_os_str().to_string_lossy();
        n.starts_with('.')
            || (i + 1 < comps.len() && PACKAGE_EXTS.contains(&ext_of(Path::new(&*n)).as_str()))
    })
}

/// A media entry for one existing regular file, or `None` (missing, symlink, not media, excluded).
pub(crate) fn entry_for(path: &Path, root: &Path) -> Option<Entry> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || is_excluded(path, root) {
        return None;
    }
    let ext = ext_of(path);
    if !is_media(&ext) {
        return None;
    }
    Some(Entry {
        path: path.to_path_buf(),
        ext,
        size: meta.len() as i64,
        modified_at: meta.modified().ok().map(Timestamp::from),
    })
}

pub(crate) fn walk(
    dir: &Path,
    root: &Path,
    seen: &mut usize,
    out: &mut Vec<Entry>,
    progress: &mut dyn FnMut(IndexProgress),
) -> std::io::Result<()> {
    let mut children: Vec<_> = std::fs::read_dir(dir)?.filter_map(Result::ok).collect();
    children.sort_by_key(|e| e.file_name());
    for child in children {
        if child.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        *seen += 1;
        if seen.is_multiple_of(50) {
            progress(IndexProgress {
                files_seen: *seen,
                media_found: out.len(),
            });
        }
        let path = child.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            if PACKAGE_EXTS.contains(&ext_of(&path).as_str()) {
                continue;
            }
            // Unreadable sub-folders are skipped, like FileManager's enumerator.
            let _ = walk(&path, root, seen, out, progress);
        } else if let Some(e) = entry_for(&path, root) {
            out.push(e);
        }
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

/// Groups entries into pairing sets keyed by [`set_key`].
pub(crate) fn into_sets(entries: Vec<Entry>) -> BTreeMap<String, Vec<Entry>> {
    let mut sets: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for e in entries {
        sets.entry(set_key(&e.path)).or_default().push(e);
    }
    sets
}

/// Builds the items of one pairing set (Live Photo still + .mov, sidecars on the anchor).
pub(crate) fn items_for_set(
    set: &[Entry],
    root: &Path,
    existing_decisions: &dyn Fn(&str) -> Option<Decision>,
    dates: &dyn DateReader,
) -> Vec<MediaItem> {
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
    // Swift order: .aae/.xmp sidecars first, then the Live Photo .mov (stored in library.json).
    attached.sort_by(|a, b| a.path.cmp(&b.path));
    let mut standalone: Vec<&Entry> = photos.iter().chain(videos.iter()).copied().collect();
    if photos
        .iter()
        .any(|e| LIVE_STILL_EXTS.contains(&e.ext.as_str()))
    {
        let mut movs: Vec<&Entry> = videos.iter().copied().filter(|e| e.ext == "mov").collect();
        movs.sort_by(|a, b| a.path.cmp(&b.path));
        attached.extend(movs);
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
        return vec![]; // orphan sidecars are ignored
    };
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut items = vec![];
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
            decision: existing_decisions(&path).unwrap_or_default(),
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
            location: dates.location(&e.path, kind),
            extra: Map::new(),
        });
    }
    items
}

/// Sort order used for `items` everywhere: date, then path.
pub(crate) fn sort_items(items: &mut [MediaItem]) {
    items.sort_by(|a, b| {
        a.taken_at
            .cmp(&b.taken_at)
            .then_with(|| a.path.cmp(&b.path))
    });
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
    walk(root, root, &mut seen, &mut entries, &mut on_progress)
        .map_err(IndexError::UnreadableRoot)?;

    let lookup = |p: &str| existing_decisions.get(p).copied();
    let mut items: Vec<MediaItem> = into_sets(entries)
        .values()
        .flat_map(|set| items_for_set(set, root, &lookup, dates))
        .collect();

    on_progress(IndexProgress {
        files_seen: seen,
        media_found: items.len(),
    });
    sort_items(&mut items);
    let groups = group::group(&items, visit_gap_hours);
    Ok((items, groups))
}
