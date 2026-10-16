// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Photo info (plan §17) for the Linux app: turns a library item plus the shared
//! [`unswiper_core::meta::info`] reader/formatter into rows for the right-click popover, the
//! inspector panel and the Compare strip. Display-free, so it is unit-tested headless.
//!
//! Privacy: info is read on demand and cached in memory only. Nothing here is written to
//! library.json or the quality cache, and nothing is sent anywhere.

use std::collections::HashMap;
use std::path::Path;
use unswiper_blur::cache::{QualityCache, Stamp};
use unswiper_core::meta::info::{self, FileFacts, FormatOptions, MediaInfo, Row, Sharpness};
use unswiper_core::quality::Sensitivity;
use unswiper_core::MediaItem;

fn base_name(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// File facts from the index (no extra disk access).
pub fn file_facts(item: &MediaItem) -> FileFacts {
    unswiper_blur::info::file_facts(item)
}

/// The blur result in words, from the quality cache (shared with the Mac helper's `info`).
/// `None` for videos (the formatter says "Not checked" for them) and when blur detection is off.
pub fn sharpness(
    item: &MediaItem,
    cache: &QualityCache,
    sensitivity: Sensitivity,
    blur_enabled: bool,
) -> Option<Sharpness> {
    unswiper_blur::info::sharpness_for(item, cache, sensitivity, blur_enabled)
}

/// Rows for `item`.
pub fn rows(
    item: &MediaItem,
    media: &MediaInfo,
    show_location: bool,
    sharp: Option<Sharpness>,
) -> Vec<Row> {
    info::format(
        media,
        &file_facts(item),
        &FormatOptions {
            show_location,
            sharpness: sharp,
        },
    )
}

/// Rows grouped in display order: `(group title, rows)`.
pub fn grouped(rows: &[Row], short_only: bool) -> Vec<(&'static str, Vec<&Row>)> {
    let mut out: Vec<(&'static str, Vec<&Row>)> = vec![];
    for r in rows.iter().filter(|r| !short_only || r.short) {
        let title = r.group.title();
        match out.last_mut() {
            Some((t, v)) if *t == title => v.push(r),
            _ => out.push((title, vec![r])),
        }
    }
    out
}

/// Inspector heading: the file name, plus "and N more selected".
pub fn heading(item: &MediaItem, others: usize) -> String {
    match others {
        0 => base_name(&item.path).to_string(),
        n => format!("{} (and {n} more selected)", base_name(&item.path)),
    }
}

/// Smoke-test report lines: `info: <group> | <label> | <value>`.
pub fn report(rows: &[Row]) -> String {
    rows.iter()
        .map(|r| format!("info: {} | {} | {}\n", r.group.title(), r.label, r.value))
        .collect()
}

/// In-memory cache of read info, keyed by path and invalidated when size or mtime change.
#[derive(Default)]
pub struct InfoCache {
    map: HashMap<String, (Option<Stamp>, MediaInfo)>,
}

impl InfoCache {
    pub fn get(&self, path: &str) -> Option<&MediaInfo> {
        let (stamp, info) = self.map.get(path)?;
        (*stamp == Stamp::of(Path::new(path))).then_some(info)
    }

    pub fn insert(&mut self, path: String, info: MediaInfo) {
        let stamp = Stamp::of(Path::new(&path));
        self.map.insert(path, (stamp, info));
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Reads the info of a library item (off the main thread).
pub fn read(path: &str) -> MediaInfo {
    info::read(Path::new(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use unswiper_blur::score::Score;
    use unswiper_core::meta::info::Group;
    use unswiper_core::quality;
    use unswiper_core::MediaKind;

    fn item(dir: &Path, name: &str, kind: MediaKind) -> MediaItem {
        let p = dir.join(name);
        std::fs::write(&p, b"0123456789").unwrap();
        serde_json::from_value(serde_json::json!({
            "path": p.to_str().unwrap(),
            "kind": if kind == MediaKind::Video { "video" } else { "photo" },
            "sizeBytes": 10,
            "takenAt": "2026-03-14T17:11:12Z",
            "relativeFolder": "",
            "decision": "undecided",
            "sidecars": [{"path": dir.join("IMG_1.MOV").to_str().unwrap(), "sizeBytes": 2_100_000}],
        }))
        .unwrap()
    }

    fn score(sharpness: f32) -> Score {
        Score {
            sharpness,
            global: sharpness,
            motion: 0.1,
            mean_luma: 0.5,
            confidence: 1.0,
        }
    }

    #[test]
    fn file_facts_from_the_index() {
        let d = tempfile::tempdir().unwrap();
        let it = item(d.path(), "IMG_1.HEIC", MediaKind::Photo);
        let f = file_facts(&it);
        assert_eq!(f.name, "IMG_1.HEIC");
        assert_eq!(f.size, Some(10));
        assert_eq!(f.folder.as_deref(), d.path().to_str());
        assert_eq!(f.sidecars, vec![("IMG_1.MOV".to_string(), 2_100_000)]);
        let r = rows(&it, &MediaInfo::default(), true, None);
        let size = r.iter().find(|r| r.label == "Size").unwrap();
        assert_eq!(size.value, "10 bytes + IMG_1.MOV, 2.1 MB");
    }

    #[test]
    fn sharpness_words_from_the_cache() {
        let d = tempfile::tempdir().unwrap();
        let mut it = item(d.path(), "a.jpg", MediaKind::Photo);
        let mut cache = QualityCache::default();
        let n = Sensitivity::Normal;
        assert_eq!(sharpness(&it, &cache, n, true), Some(Sharpness::NotChecked));
        assert_eq!(sharpness(&it, &cache, n, false), None, "detection off");
        let st = Stamp::of(Path::new(&it.path)).unwrap();
        cache.put(&it.path, st, score(100.0), 0);
        assert_eq!(sharpness(&it, &cache, n, true), Some(Sharpness::Sharp));
        cache.put(&it.path, st, score(20.0), 0);
        assert_eq!(sharpness(&it, &cache, n, true), Some(Sharpness::Soft));
        cache.put(&it.path, st, score(100.0), 0);
        let set: std::collections::HashSet<String> = [it.path.clone()].into_iter().collect();
        quality::apply_suggestions(std::slice::from_mut(&mut it), &set, &set, true);
        assert_eq!(
            sharpness(&it, &cache, n, true),
            Some(Sharpness::Blurry),
            "suggested wins"
        );
        let png = item(d.path(), "s.png", MediaKind::Photo);
        assert_eq!(
            sharpness(&png, &cache, n, true),
            Some(Sharpness::NotChecked)
        );
        let mov = item(d.path(), "v.mov", MediaKind::Video);
        assert_eq!(sharpness(&mov, &cache, n, true), None);
    }

    #[test]
    fn grouping_heading_and_report() {
        let d = tempfile::tempdir().unwrap();
        let it = item(d.path(), "IMG_1.JPG", MediaKind::Photo);
        let media = MediaInfo {
            make: Some("Canon".into()),
            model: Some("EOS R6".into()),
            iso: Some(100),
            gps: Some(info::Gps {
                lat: 1.0,
                lon: 2.0,
                alt_m: None,
            }),
            ..Default::default()
        };
        let r = rows(&it, &media, true, Some(Sharpness::Sharp));
        let g = grouped(&r, false);
        let titles: Vec<_> = g.iter().map(|(t, _)| *t).collect();
        assert_eq!(
            titles,
            ["Camera", "Exposure", "File", "Location", "Unswiper"]
        );
        let short = grouped(&r, true);
        assert!(short.iter().all(|(_, v)| v.iter().all(|r| r.short)));
        assert!(!short.iter().any(|(t, _)| *t == "Location"));
        assert!(rows(&it, &media, false, None)
            .iter()
            .all(|r| r.group != Group::Location));
        assert_eq!(heading(&it, 0), "IMG_1.JPG");
        assert_eq!(heading(&it, 2), "IMG_1.JPG (and 2 more selected)");
        assert!(report(&r).starts_with("info: Camera | Camera | Canon EOS R6\n"));
    }

    #[test]
    fn info_cache_invalidates_on_change() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.jpg");
        std::fs::write(&p, b"1").unwrap();
        let key = p.to_str().unwrap().to_string();
        let mut c = InfoCache::default();
        assert!(c.is_empty());
        c.insert(
            key.clone(),
            MediaInfo {
                iso: Some(1),
                ..Default::default()
            },
        );
        assert_eq!(c.get(&key).unwrap().iso, Some(1));
        assert_eq!(c.len(), 1);
        std::fs::write(&p, b"12").unwrap();
        assert!(c.get(&key).is_none(), "size changed");
        assert!(c.get("/nope").is_none());
        assert_eq!(read("/nonexistent.jpg"), MediaInfo::default());
    }
}
