// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! `unswiper-blur info`: photo info rows for the Mac app (plan §17.4/§17.9), from the same Rust
//! reader and formatter the Linux app uses, so both apps show identical wording.
//!
//! ```text
//! unswiper-blur info [--library library.json] [--sensitivity S] [--blur-off] [--hide-location] FILE...
//! unswiper-blur info --format CASES.json
//! ```
//!
//! Prints JSON: `[{"path", "rows": [{group, label, value, short}], "strip"}]`. Read-only: it never
//! changes photos, library.json or the score cache, and the location goes only to stdout.

use crate::cache::{QualityCache, Stamp};
use crate::decide::{is_camera_format, sharpness_word, Reason, Thresholds};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use unswiper_core::meta::info::{self, FileFacts, FormatOptions, MediaInfo, Row, Sharpness};
use unswiper_core::quality::{self, Sensitivity};
use unswiper_core::{LibrarySnapshot, MediaItem, MediaKind};

/// The blur result in words for one library item, from the score cache. `None` for videos (the
/// formatter says "Not checked") and when blur detection is off. A suggestion in library.json
/// wins over the score (it covers bursts and shake).
pub fn sharpness_for(
    item: &MediaItem,
    cache: &QualityCache,
    sensitivity: Sensitivity,
    blur_enabled: bool,
) -> Option<Sharpness> {
    if item.kind == MediaKind::Video || !blur_enabled {
        return None;
    }
    if !is_camera_format(&item.path) {
        return Some(Sharpness::NotChecked);
    }
    let score = Stamp::of(Path::new(&item.path)).and_then(|st| cache.get(&item.path, st));
    Some(match score {
        Some(sc) => sharpness_word(
            &sc,
            quality::is_suggested(item).then_some(Reason::Soft),
            Thresholds::for_sensitivity(sensitivity),
        ),
        None => Sharpness::NotChecked,
    })
}

fn base_name(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// File facts from the index (sizes of Live Photo motion files included).
pub fn file_facts(item: &MediaItem) -> FileFacts {
    FileFacts {
        name: base_name(&item.path).to_string(),
        size: u64::try_from(item.size_bytes).ok(),
        folder: Path::new(&item.path)
            .parent()
            .map(|p| p.display().to_string()),
        sidecars: item
            .sidecars
            .iter()
            .map(|s| {
                (
                    base_name(&s.path).to_string(),
                    u64::try_from(s.size_bytes).unwrap_or(0),
                )
            })
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Options {
    pub files: Vec<PathBuf>,
    pub library: Option<PathBuf>,
    pub sensitivity: Option<Sensitivity>,
    pub blur_off: bool,
    pub hide_location: bool,
    /// `--format CASES.json`: format golden-table cases instead of reading files (tests).
    pub format: Option<PathBuf>,
}

pub const USAGE: &str =
    "  unswiper-blur info [options] FILE...      photo info rows as JSON (read-only)
      --library FILE         library.json: sidecars, blur suggestions, scores next to it
      --sensitivity S        low | normal | high (default: the library's setting)
      --blur-off             no Sharpness row (blur detection is off)
      --hide-location        leave out the Location rows
      --format CASES.json    format golden-table cases (tests)";

pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut i = 1;
    let value = |i: usize, what: &str| {
        args.get(i + 1)
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .ok_or(format!("{what} needs a value"))
    };
    while i < args.len() {
        match args[i].as_str() {
            "--library" => {
                o.library = Some(value(i, "--library")?.into());
                i += 1;
            }
            "--sensitivity" => {
                o.sensitivity = Some(
                    Sensitivity::parse(&value(i, "--sensitivity")?)
                        .ok_or("--sensitivity needs low, normal or high")?,
                );
                i += 1;
            }
            "--format" => {
                o.format = Some(value(i, "--format")?.into());
                i += 1;
            }
            "--blur-off" => o.blur_off = true,
            "--hide-location" => o.hide_location = true,
            a if a.starts_with("--") => return Err(format!("unknown option {a}")),
            a => o.files.push(PathBuf::from(a)),
        }
        i += 1;
    }
    if o.files.is_empty() && o.format.is_none() {
        return Err("info needs at least one file".into());
    }
    Ok(o)
}

/// One answer per file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    pub path: String,
    pub rows: Vec<Row>,
    pub strip: String,
}

#[derive(Deserialize)]
struct Case {
    info: MediaInfo,
    file: FileFacts,
    options: FormatOptions,
}

pub fn run(o: &Options) -> Result<String, String> {
    if let Some(f) = &o.format {
        let data = std::fs::read(f).map_err(|e| format!("{}: {e}", f.display()))?;
        let cases: Vec<Case> =
            serde_json::from_slice(&data).map_err(|e| format!("{}: {e}", f.display()))?;
        let out: Vec<Vec<Row>> = cases
            .iter()
            .map(|c| info::format(&c.info, &c.file, &c.options))
            .collect();
        return Ok(serde_json::to_string_pretty(&out).unwrap_or_default());
    }
    let (items, cache, lib_sens, lib_blur) = match &o.library {
        Some(lib) => {
            let data = std::fs::read(lib).map_err(|e| format!("{}: {e}", lib.display()))?;
            let snap: LibrarySnapshot =
                serde_json::from_slice(&data).map_err(|e| format!("{}: {e}", lib.display()))?;
            let dir = lib.parent().unwrap_or(Path::new("."));
            let cache = QualityCache::load(&QualityCache::path_in(dir));
            (
                snap.items,
                cache,
                quality::sensitivity(&snap.config),
                quality::enabled(&snap.config),
            )
        }
        None => (vec![], QualityCache::default(), Sensitivity::Normal, true),
    };
    let sens = o.sensitivity.unwrap_or(lib_sens);
    let blur_on = lib_blur && !o.blur_off;
    let answers: Vec<Answer> = o
        .files
        .iter()
        .map(|f| {
            let path = f.to_string_lossy().into_owned();
            let media = info::read(f);
            let item = items.iter().find(|i| i.path == path);
            let facts = item.map(file_facts).unwrap_or_else(|| FileFacts::of(f));
            let sharp = match item {
                Some(it) => sharpness_for(it, &cache, sens, blur_on),
                None if media.is_video || !blur_on => None,
                None => Some(Sharpness::NotChecked),
            };
            let opts = FormatOptions {
                show_location: !o.hide_location,
                sharpness: sharp,
            };
            Answer {
                rows: info::format(&media, &facts, &opts),
                strip: info::strip(&media, sharp),
                path,
            }
        })
        .collect();
    Ok(serde_json::to_string_pretty(&answers).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::Score;
    use unswiper_core::meta::info::Group;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
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

    fn item(dir: &Path, name: &str, kind: &str) -> MediaItem {
        let p = dir.join(name);
        std::fs::write(&p, b"0123456789").unwrap();
        serde_json::from_value(serde_json::json!({
            "path": p.to_str().unwrap(), "kind": kind, "sizeBytes": 10,
            "takenAt": "2026-03-14T17:11:12Z", "relativeFolder": "", "decision": "undecided",
            "sidecars": [{"path": dir.join("IMG_1.MOV").to_str().unwrap(), "sizeBytes": 2_100_000}],
        }))
        .unwrap()
    }

    #[test]
    fn parse_options() {
        let o = parse(&a(&[
            "info",
            "--library",
            "/l.json",
            "--sensitivity",
            "high",
            "--blur-off",
            "--hide-location",
            "/a.jpg",
            "/b.mov",
        ]))
        .unwrap();
        assert_eq!(o.library, Some(PathBuf::from("/l.json")));
        assert_eq!(o.sensitivity, Some(Sensitivity::High));
        assert!(o.blur_off && o.hide_location);
        assert_eq!(o.files.len(), 2);
        assert!(parse(&a(&["info", "--format", "/c.json"])).is_ok());
        for bad in [
            &["info"][..],
            &["info", "--library"],
            &["info", "--sensitivity", "max", "/a"],
            &["info", "--format", "--blur-off"],
            &["info", "--bogus", "/a"],
        ] {
            assert!(parse(&a(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn sharpness_for_items() {
        let d = tempfile::tempdir().unwrap();
        let mut it = item(d.path(), "a.jpg", "photo");
        let mut cache = QualityCache::default();
        let n = Sensitivity::Normal;
        assert_eq!(
            sharpness_for(&it, &cache, n, true),
            Some(Sharpness::NotChecked)
        );
        assert_eq!(sharpness_for(&it, &cache, n, false), None);
        let st = Stamp::of(Path::new(&it.path)).unwrap();
        cache.put(&it.path, st, score(100.0), 0);
        assert_eq!(sharpness_for(&it, &cache, n, true), Some(Sharpness::Sharp));
        cache.put(&it.path, st, score(20.0), 0);
        assert_eq!(sharpness_for(&it, &cache, n, true), Some(Sharpness::Soft));
        cache.put(&it.path, st, score(100.0), 0);
        let set: std::collections::HashSet<String> = [it.path.clone()].into_iter().collect();
        quality::apply_suggestions(std::slice::from_mut(&mut it), &set, &set, true);
        assert_eq!(sharpness_for(&it, &cache, n, true), Some(Sharpness::Blurry));
        let png = item(d.path(), "s.png", "photo");
        assert_eq!(
            sharpness_for(&png, &cache, n, true),
            Some(Sharpness::NotChecked)
        );
        let mov = item(d.path(), "v.mov", "video");
        assert_eq!(sharpness_for(&mov, &cache, n, true), None);
        let f = file_facts(&it);
        assert_eq!(f.sidecars, vec![("IMG_1.MOV".to_string(), 2_100_000)]);
    }

    #[test]
    fn info_for_files_with_and_without_library() {
        let d = tempfile::tempdir().unwrap();
        let it = item(d.path(), "IMG_1.JPG", "photo");
        let loose = d.path().join("loose.png");
        std::fs::write(&loose, b"x").unwrap();
        // Without a library: file facts from disk, Sharpness "Not checked".
        let o = parse(&a(&["info", loose.to_str().unwrap()])).unwrap();
        let v: Vec<Answer> = serde_json::from_str(&run(&o).unwrap()).unwrap();
        assert_eq!(v[0].path, loose.to_str().unwrap());
        assert!(v[0]
            .rows
            .iter()
            .any(|r| r.label == "Size" && r.value == "1 bytes"));
        assert_eq!(v[0].rows.last().unwrap().value, "Not checked");
        assert_eq!(v[0].strip, "");
        // With a library: sidecars from the index, blur off from the library config.
        let mut snap = LibrarySnapshot::empty();
        snap.items = vec![it.clone()];
        quality::set_enabled(&mut snap.config, false);
        let lib = d.path().join("library.json");
        std::fs::write(&lib, serde_json::to_vec(&snap).unwrap()).unwrap();
        let o = parse(&a(&["info", "--library", lib.to_str().unwrap(), &it.path])).unwrap();
        let v: Vec<Answer> = serde_json::from_str(&run(&o).unwrap()).unwrap();
        let size = v[0].rows.iter().find(|r| r.label == "Size").unwrap();
        assert_eq!(size.value, "10 bytes + IMG_1.MOV, 2.1 MB");
        assert!(v[0].rows.iter().all(|r| r.group != Group::Unswiper));
        let o = Options {
            files: vec![d.path().join("missing.jpg")],
            library: Some(d.path().join("nope.json")),
            ..Default::default()
        };
        assert!(run(&o).is_err());
    }

    #[test]
    fn hide_location_and_golden_format() {
        let golden = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../unswiper-core/testdata/info-format-golden.json");
        let o = parse(&a(&["info", "--format", golden.to_str().unwrap()])).unwrap();
        let got: Vec<Vec<Row>> = serde_json::from_str(&run(&o).unwrap()).unwrap();
        let want: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(&golden).unwrap()).unwrap();
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(&want) {
            let w: Vec<Row> = serde_json::from_value(w["rows"].clone()).unwrap();
            assert_eq!(g, &w);
        }
        let bad = Options {
            format: Some("/nonexistent.json".into()),
            ..Default::default()
        };
        assert!(run(&bad).is_err());
        // --hide-location on a file with GPS: covered by the formatter; here just the flag.
        let o = parse(&a(&["info", "--hide-location", "/x.jpg"])).unwrap();
        let v: Vec<Answer> = serde_json::from_str(&run(&o).unwrap()).unwrap();
        assert!(v[0].rows.iter().all(|r| r.group != Group::Location));
    }
}
