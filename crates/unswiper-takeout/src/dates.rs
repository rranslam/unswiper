// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Dates and locations for an unpacked Takeout folder: the file's own metadata first, then the
//! Takeout JSON sidecar.

use crate::sidecar::{FolderSidecars, TakeoutMeta};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use unswiper_core::index::DateReader;
use unswiper_core::meta::info::{self, Gps};
use unswiper_core::time::Timestamp;
use unswiper_core::{Location, LocationSource, MediaKind};

/// A [`DateReader`] for the normal indexer that falls back to Takeout's `photoTakenTime` when a
/// photo or video has no EXIF/QuickTime date (screenshots, edited copies, messenger images, files
/// whose metadata Google stripped). Items dated this way count as having a capture date.
pub struct TakeoutDates<R: DateReader> {
    inner: R,
    folders: Mutex<HashMap<PathBuf, Arc<FolderSidecars>>>,
    from_sidecar: AtomicUsize,
}

impl<R: DateReader> TakeoutDates<R> {
    pub fn new(inner: R) -> Self {
        TakeoutDates {
            inner,
            folders: Mutex::default(),
            from_sidecar: AtomicUsize::new(0),
        }
    }

    /// The Takeout sidecar of `path`, if there is one.
    pub fn meta(&self, path: &Path) -> Option<TakeoutMeta> {
        let dir = path.parent()?;
        let name = path.file_name()?.to_string_lossy();
        let folder = {
            let mut map = self.folders.lock().unwrap_or_else(|e| e.into_inner());
            map.entry(dir.to_path_buf())
                .or_insert_with(|| Arc::new(FolderSidecars::load(dir)))
                .clone()
        };
        folder.for_media(&name)
    }

    fn fallback(&self, path: &Path) -> Option<Timestamp> {
        let t = self.meta(path)?.taken?;
        self.from_sidecar.fetch_add(1, Ordering::Relaxed);
        Some(t)
    }

    /// How many dates so far came from Takeout sidecars.
    pub fn sidecar_dates(&self) -> usize {
        self.from_sidecar.load(Ordering::Relaxed)
    }

    /// Where `path` was taken: the file's own GPS first, then Takeout's `geoData`/`geoDataExif`.
    pub fn location(&self, path: &Path) -> Option<Gps> {
        info::read(path).gps.or_else(|| self.meta(path)?.location())
    }
}

/// Google's date and place for one media file of a Takeout library (from its JSON sidecar).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SidecarDate {
    /// Absolute path of the photo or video.
    pub path: PathBuf,
    /// `photoTakenTime`, seconds since 1970 (UTC).
    pub taken: Option<i64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

/// Sidecar dates and places of every photo and video under `root` that has a Takeout sidecar.
/// For the Mac app, whose own indexer uses them when a file has no capture date. Skips hidden
/// folders and `_rejected`, like the indexers.
pub fn library_dates(root: &Path) -> Vec<SidecarDate> {
    use unswiper_core::index::{PHOTO_EXTS, VIDEO_EXTS};
    let mut out = vec![];
    let mut dirs = vec![root.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut media = vec![];
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            match e.file_type() {
                Ok(t) if t.is_dir() => {
                    if !name.eq_ignore_ascii_case("_rejected") {
                        dirs.push(e.path());
                    }
                }
                Ok(t) if t.is_file() => {
                    let ext = Path::new(&name)
                        .extension()
                        .map(|x| x.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    if PHOTO_EXTS.contains(&ext.as_str()) || VIDEO_EXTS.contains(&ext.as_str()) {
                        media.push(name);
                    }
                }
                _ => {}
            }
        }
        if media.is_empty() {
            continue;
        }
        let folder = FolderSidecars::load(&dir);
        media.sort();
        for name in media {
            if let Some(m) = folder.for_media(&name) {
                let gps = m.location();
                if m.taken.is_none() && gps.is_none() {
                    continue;
                }
                out.push(SidecarDate {
                    path: dir.join(&name),
                    taken: m.taken.map(|t| t.timestamp()),
                    lat: gps.as_ref().map(|g| g.lat),
                    lon: gps.as_ref().map(|g| g.lon),
                });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

impl<R: DateReader> DateReader for TakeoutDates<R> {
    fn photo_date(&self, path: &Path) -> Option<Timestamp> {
        self.inner.photo_date(path).or_else(|| self.fallback(path))
    }
    fn video_date(&self, path: &Path) -> Option<Timestamp> {
        self.inner.video_date(path).or_else(|| self.fallback(path))
    }
    /// The file's own location first, then Takeout's `geoData`/`geoDataExif` (`0,0` is none).
    fn location(&self, path: &Path, kind: MediaKind) -> Option<Location> {
        self.inner.location(path, kind).or_else(|| {
            let g = self.meta(path)?.location()?;
            Location::new(g.lat, g.lon, LocationSource::Takeout)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::collections::HashMap;
    use unswiper_core::index::{index, MetadataDates};
    use unswiper_core::model::DateSource;

    /// Fake reader: only files whose name contains "exif" have a date (2020-01-01).
    struct Fake;
    impl DateReader for Fake {
        fn photo_date(&self, p: &Path) -> Option<Timestamp> {
            p.to_string_lossy()
                .contains("exif")
                .then(|| Utc.with_ymd_and_hms(2020, 1, 1, 12, 0, 0).unwrap())
        }
        fn video_date(&self, p: &Path) -> Option<Timestamp> {
            self.photo_date(p)
        }
    }

    fn sidecar(dir: &Path, name: &str, ts: i64, geo: Option<(f64, f64)>) {
        let geo = geo
            .map(|(a, b)| {
                format!(r#","geoData":{{"latitude":{a},"longitude":{b},"altitude":0.0}}"#)
            })
            .unwrap_or_default();
        std::fs::write(
            dir.join(name),
            format!(r#"{{"title":"x","photoTakenTime":{{"timestamp":"{ts}"}}{geo}}}"#),
        )
        .unwrap();
    }

    #[test]
    fn exif_first_then_sidecar() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        for f in [
            "exif_a.jpg",
            "plain.jpg",
            "plain-edited.jpg",
            "clip.mp4",
            "none.jpg",
        ] {
            std::fs::write(p.join(f), b"not really an image").unwrap();
        }
        sidecar(
            p,
            "exif_a.jpg.supplemental-metadata.json",
            1_561_190_459,
            None,
        );
        sidecar(
            p,
            "plain.jpg.supplemental-metadata.json",
            1_561_190_459,
            Some((48.85, 2.29)),
        );
        sidecar(p, "clip.mp4.json", 1_600_000_000, None);
        let r = TakeoutDates::new(Fake);
        assert_eq!(
            r.photo_date(&p.join("exif_a.jpg")).unwrap().timestamp(),
            1_577_880_000
        );
        assert_eq!(
            r.photo_date(&p.join("plain.jpg")).unwrap().timestamp(),
            1_561_190_459
        );
        assert_eq!(
            r.photo_date(&p.join("plain-edited.jpg"))
                .unwrap()
                .timestamp(),
            1_561_190_459
        );
        assert_eq!(
            r.video_date(&p.join("clip.mp4")).unwrap().timestamp(),
            1_600_000_000
        );
        assert_eq!(r.photo_date(&p.join("none.jpg")), None);
        assert_eq!(r.sidecar_dates(), 3);
        let g = r.location(&p.join("plain.jpg")).unwrap();
        assert_eq!((g.lat, g.lon), (48.85, 2.29));
        assert_eq!(r.location(&p.join("clip.mp4")), None);
    }

    #[test]
    fn feeds_the_normal_indexer() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let album = root.join("Photos from 2019");
        std::fs::create_dir(&album).unwrap();
        std::fs::write(album.join("IMG_1.jpg"), b"x").unwrap();
        std::fs::write(album.join("IMG_2.jpg"), b"x").unwrap();
        std::fs::write(
            album.join("metadata.json"),
            br#"{"title":"Photos from 2019"}"#,
        )
        .unwrap();
        sidecar(
            &album,
            "IMG_1.jpg.supplemental-metadata.json",
            1_561_190_459,
            None,
        );
        let dates = TakeoutDates::new(MetadataDates);
        let (items, groups) = index(root, 36.0, &HashMap::new(), &dates, |_| {}).unwrap();
        assert_eq!(items.len(), 2, "JSON files are not media");
        let one = items
            .iter()
            .find(|i| i.path.ends_with("IMG_1.jpg"))
            .unwrap();
        assert_eq!(one.taken_at.timestamp(), 1_561_190_459);
        assert_eq!(one.date_source, Some(DateSource::Capture));
        assert!(!one.is_undated);
        assert_eq!(one.relative_folder, "Photos from 2019");
        let two = items
            .iter()
            .find(|i| i.path.ends_with("IMG_2.jpg"))
            .unwrap();
        assert_ne!(two.date_source, Some(DateSource::Capture));
        assert!(!groups.is_empty());
        assert_eq!(dates.sidecar_dates(), 1);
    }

    #[test]
    fn index_location_falls_back_to_geo_data_and_ignores_zero() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        for f in ["geo.jpg", "zero.jpg", "bare.jpg"] {
            std::fs::write(p.join(f), b"not really an image").unwrap();
        }
        sidecar(p, "geo.jpg.json", 1_600_000_000, Some((12.5, -65.25)));
        sidecar(p, "zero.jpg.json", 1_600_000_000, Some((0.0, 0.0)));
        let r = TakeoutDates::new(unswiper_core::index::MetadataDates);
        let l = DateReader::location(&r, &p.join("geo.jpg"), MediaKind::Photo).unwrap();
        assert_eq!(
            (l.lat, l.lon, l.source),
            (12.5, -65.25, LocationSource::Takeout)
        );
        assert_eq!(
            DateReader::location(&r, &p.join("zero.jpg"), MediaKind::Photo),
            None
        );
        assert_eq!(
            DateReader::location(&r, &p.join("bare.jpg"), MediaKind::Photo),
            None
        );
    }
}
