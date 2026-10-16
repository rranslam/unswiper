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

impl<R: DateReader> DateReader for TakeoutDates<R> {
    fn photo_date(&self, path: &Path) -> Option<Timestamp> {
        self.inner.photo_date(path).or_else(|| self.fallback(path))
    }
    fn video_date(&self, path: &Path) -> Option<Timestamp> {
        self.inner.video_date(path).or_else(|| self.fallback(path))
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
}
