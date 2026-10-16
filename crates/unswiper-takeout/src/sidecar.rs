// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Google Takeout JSON sidecars: which JSON file belongs to which photo, and what's in it.
//!
//! Takeout writes one JSON file next to each photo or video. Its name follows rules that changed
//! over the years, so matching is by rule, best match first:
//!
//! | Media file | JSON file |
//! |---|---|
//! | `IMG_1234.JPG` | `IMG_1234.JPG.supplemental-metadata.json` (2024 and later) |
//! | `IMG_1234.JPG` | `IMG_1234.JPG.json` (older exports) |
//! | `IMG_1234(1).JPG` (second photo with the same name) | `IMG_1234.JPG.supplemental-metadata(1).json` / `IMG_1234.JPG(1).json` |
//! | `IMG_1234-edited.JPG` (Google Photos edit; localised suffix) | the original's JSON |
//! | long names | the JSON name is cut short: `…very_long_name.jpg.supplemental-me.json`, or even inside the media name |
//! | `IMG_1234.MP4` (Live Photo / motion video) | the still's JSON, `IMG_1234.HEIC.supplemental-metadata.json` |
//!
//! If no name rule matches, a JSON whose `title` is the media file's name is used.

use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use unswiper_core::meta::info::Gps;
use unswiper_core::meta::usable;
use unswiper_core::time::Timestamp;

/// The suffix newer exports put between the media name and `.json`.
pub const SUPPLEMENTAL: &str = ".supplemental-metadata";

/// Suffixes Google Photos adds to edited copies (`IMG-edited.jpg`), per export language.
pub const EDITED_SUFFIXES: &[&str] = &[
    "-edited",
    "-bearbeitet",
    "-modifié",
    "-modificato",
    "-editado",
    "-bewerkt",
    "-redigerad",
    "-muokattu",
    "-edytowane",
    "-upravené",
    "-編集済み",
    "-편집됨",
    "-已编辑",
];

/// A JSON name shorter than this (with `.json`) was never truncated by Takeout. Takeout cuts JSON
/// names at about 46–51 characters, so a prefix match is only trusted for names at least this long.
const TRUNCATED_MIN_CHARS: usize = 46;

/// What Unswiper uses from a Takeout JSON sidecar.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TakeoutMeta {
    /// Original file name as uploaded.
    pub title: Option<String>,
    pub description: Option<String>,
    /// When the photo was taken (`photoTakenTime`, UTC).
    pub taken: Option<Timestamp>,
    /// Google's location (`geoData`; may have been set or corrected in Google Photos).
    pub geo: Option<Gps>,
    /// The location Google read from the file's EXIF (`geoDataExif`).
    pub geo_exif: Option<Gps>,
}

fn timestamp(v: &Value) -> Option<Timestamp> {
    let t = v.get("timestamp")?;
    let secs = match t {
        Value::String(s) => s.trim().parse::<i64>().ok()?,
        Value::Number(n) => n.as_i64()?,
        _ => return None,
    };
    usable(DateTime::<Utc>::from_timestamp(secs, 0))
}

fn gps(v: Option<&Value>) -> Option<Gps> {
    let v = v?;
    let lat = v.get("latitude")?.as_f64()?;
    let lon = v.get("longitude")?.as_f64()?;
    // Takeout writes 0.0/0.0 when it has no location.
    if (lat == 0.0 && lon == 0.0)
        || !(-90.0..=90.0).contains(&lat)
        || !(-180.0..=180.0).contains(&lon)
    {
        return None;
    }
    let alt_m = v
        .get("altitude")
        .and_then(Value::as_f64)
        .filter(|a| *a != 0.0);
    Some(Gps { lat, lon, alt_m })
}

fn text(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

impl TakeoutMeta {
    /// Parses a Takeout JSON sidecar. `None` if it isn't JSON or has none of the known fields.
    pub fn parse(bytes: &[u8]) -> Option<TakeoutMeta> {
        let v: Value = serde_json::from_slice(bytes).ok()?;
        let m = TakeoutMeta {
            title: text(v.get("title")),
            description: text(v.get("description")),
            taken: v.get("photoTakenTime").and_then(timestamp),
            geo: gps(v.get("geoData")),
            geo_exif: gps(v.get("geoDataExif")),
        };
        (m != TakeoutMeta::default()).then_some(m)
    }

    /// The location to show when the file itself has none: `geoData`, then `geoDataExif`.
    pub fn location(&self) -> Option<Gps> {
        self.geo.or(self.geo_exif)
    }
}

/// Splits `name(3)` → (`name`, Some(3)).
fn split_dup(s: &str) -> (&str, Option<u32>) {
    if let Some(open) = s
        .strip_suffix(')')
        .and_then(|r| r.rfind('(').map(|i| (r, i)))
    {
        let (r, i) = open;
        if let Ok(n) = r[i + 1..].parse::<u32>() {
            return (&r[..i], Some(n));
        }
    }
    (s, None)
}

/// `IMG(1).jpg` → (`IMG.jpg`, Some(1)), `IMG-edited.jpg` → (`IMG.jpg`, None) etc. Returns
/// (original name, duplicate number, extension-less original stem).
fn media_parts(name: &str) -> (String, Option<u32>, String) {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], Some(&name[i + 1..])),
        _ => (name, None),
    };
    let mut stem = stem;
    for suf in EDITED_SUFFIXES {
        if let Some(s) = stem.strip_suffix(suf) {
            stem = s;
            break;
        }
    }
    let (stem, dup) = split_dup(stem);
    // An edited duplicate is written `IMG(1)-edited.jpg`; the loop above handled the suffix first.
    let orig = match ext {
        Some(e) => format!("{stem}.{e}"),
        None => stem.to_string(),
    };
    (orig, dup, stem.to_string())
}

/// Picks the JSON sidecar for `media` (a file name) from the JSON file names in its folder.
/// Returns the chosen JSON name. Pure function: no file access.
pub fn match_sidecar<'a>(media: &str, jsons: &'a [String]) -> Option<&'a str> {
    let (orig, dup, stem) = media_parts(media);
    let full_new = format!("{orig}{SUPPLEMENTAL}");
    let orig_lc = orig.to_lowercase();
    let full_new_lc = full_new.to_lowercase();
    let stem_lc = stem.to_lowercase();
    let mut best: Option<(u8, usize, &str)> = None;
    for j in jsons {
        let Some(body) = j.strip_suffix(".json").or_else(|| j.strip_suffix(".JSON")) else {
            continue;
        };
        let (body, jdup) = split_dup(body);
        let body_lc = body.to_lowercase();
        let long = j.chars().count() >= TRUNCATED_MIN_CHARS;
        let score = if jdup != dup {
            // Also accept `IMG(1).jpg.json`: the duplicate number kept in the media part.
            if jdup.is_none() && dup.is_some() && media_matches_literal(media, &body_lc) {
                5
            } else {
                0
            }
        } else if body_lc == full_new_lc {
            6
        } else if body_lc == orig_lc {
            5
        } else if body_lc.len() > orig_lc.len() && full_new_lc.starts_with(&body_lc) {
            4 // suffix cut short: IMG.jpg.supplemental-me
        } else if long
            && body_lc.len() >= 20
            && (orig_lc.starts_with(&body_lc) || full_new_lc.starts_with(&body_lc))
        {
            3 // cut inside the media name (old exports)
        } else if json_stem(&body_lc) == stem_lc {
            1 // Live Photo video → the still's JSON (IMG.MP4 → IMG.HEIC.json)
        } else {
            0
        };
        if score == 0 {
            continue;
        }
        let cand = (score, body_lc.len(), j.as_str());
        if best.is_none_or(|b| (cand.0, cand.1) > (b.0, b.1)) {
            best = Some(cand);
        }
    }
    best.map(|b| b.2)
}

fn media_matches_literal(media: &str, body_lc: &str) -> bool {
    let m = media.to_lowercase();
    body_lc == m || body_lc == format!("{m}{}", SUPPLEMENTAL.to_lowercase())
}

/// `img.heic.supplemental-metadata` → `img`; `img.heic` → `img`.
fn json_stem(body_lc: &str) -> String {
    let b = body_lc
        .strip_suffix(&SUPPLEMENTAL.to_lowercase())
        .unwrap_or(body_lc);
    match b.rfind('.') {
        Some(i) if i > 0 => b[..i].to_string(),
        _ => b.to_string(),
    }
}

/// The JSON sidecars of one folder, read lazily.
#[derive(Debug, Default)]
pub struct FolderSidecars {
    dir: PathBuf,
    names: Vec<String>,
    parsed: Mutex<HashMap<String, Option<TakeoutMeta>>>,
}

impl FolderSidecars {
    /// Lists the `.json` files in `dir` (none if it can't be read).
    pub fn load(dir: &Path) -> FolderSidecars {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.to_lowercase().ends_with(".json"))
            .collect();
        names.sort();
        FolderSidecars {
            dir: dir.to_path_buf(),
            names,
            parsed: Mutex::default(),
        }
    }

    fn read(&self, name: &str) -> Option<TakeoutMeta> {
        let mut cache = self.parsed.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .entry(name.to_string())
            .or_insert_with(|| {
                // Sidecars are small; refuse anything large (not a Takeout sidecar).
                let p = self.dir.join(name);
                let len = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(u64::MAX);
                if len > 1 << 20 {
                    return None;
                }
                std::fs::read(&p).ok().and_then(|b| TakeoutMeta::parse(&b))
            })
            .clone()
    }

    /// The sidecar of the media file called `media` in this folder.
    pub fn for_media(&self, media: &str) -> Option<TakeoutMeta> {
        if let Some(j) = match_sidecar(media, &self.names) {
            if let Some(m) = self.read(j) {
                return Some(m);
            }
        }
        // Last resort: a sidecar whose title is this file's name.
        let (_, dup, _) = media_parts(media);
        if dup.is_some() {
            return None; // several files share that title; don't guess
        }
        self.names.iter().filter_map(|n| self.read(n)).find(|m| {
            m.title
                .as_deref()
                .is_some_and(|t| t.eq_ignore_ascii_case(media))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn new_and_old_names() {
        let j = names(&[
            "IMG_1234.JPG.supplemental-metadata.json",
            "IMG_9999.JPG.json",
        ]);
        assert_eq!(
            match_sidecar("IMG_1234.JPG", &j),
            Some("IMG_1234.JPG.supplemental-metadata.json")
        );
        assert_eq!(match_sidecar("IMG_9999.JPG", &j), Some("IMG_9999.JPG.json"));
        assert_eq!(match_sidecar("IMG_0000.JPG", &j), None);
        // Case differences between the file and its JSON are tolerated.
        assert_eq!(match_sidecar("img_9999.jpg", &j), Some("IMG_9999.JPG.json"));
    }

    #[test]
    fn duplicates_need_the_same_number() {
        let j = names(&[
            "IMG.jpg.supplemental-metadata.json",
            "IMG.jpg.supplemental-metadata(1).json",
            "IMG.jpg(2).json",
        ]);
        assert_eq!(
            match_sidecar("IMG.jpg", &j),
            Some("IMG.jpg.supplemental-metadata.json")
        );
        assert_eq!(
            match_sidecar("IMG(1).jpg", &j),
            Some("IMG.jpg.supplemental-metadata(1).json")
        );
        assert_eq!(match_sidecar("IMG(2).jpg", &j), Some("IMG.jpg(2).json"));
        assert_eq!(match_sidecar("IMG(3).jpg", &j), None);
        // Some exports keep the number in the media part.
        let j2 = names(&["IMG(1).jpg.json"]);
        assert_eq!(match_sidecar("IMG(1).jpg", &j2), Some("IMG(1).jpg.json"));
    }

    #[test]
    fn edited_copies_use_the_original() {
        let j = names(&["IMG_1.jpg.supplemental-metadata.json", "P2.jpg.json"]);
        assert_eq!(
            match_sidecar("IMG_1-edited.jpg", &j),
            Some("IMG_1.jpg.supplemental-metadata.json")
        );
        assert_eq!(
            match_sidecar("IMG_1-bearbeitet.jpg", &j),
            Some("IMG_1.jpg.supplemental-metadata.json")
        );
        assert_eq!(match_sidecar("P2-modifié.jpg", &j), Some("P2.jpg.json"));
        let jd = names(&["IMG.jpg.supplemental-metadata(1).json"]);
        assert_eq!(
            match_sidecar("IMG(1)-edited.jpg", &jd),
            Some("IMG.jpg.supplemental-metadata(1).json")
        );
    }

    #[test]
    fn truncated_names() {
        // Suffix cut short.
        let j = names(&["PXL_20230101_120000123.MP.jpg.supplemental-me.json"]);
        assert_eq!(
            match_sidecar("PXL_20230101_120000123.MP.jpg", &j),
            Some(j[0].as_str())
        );
        // Cut inside the media name (old exports, 46 characters + .json).
        let long = "a_really_long_file_name_from_some_camera_app_2019.jpg";
        let cut = format!("{}.json", &long[..46]);
        let j = names(&[&cut]);
        assert_eq!(match_sidecar(long, &j), Some(cut.as_str()));
        // ...with a duplicate number.
        let cutd = format!("{}(1).json", &long[..46]);
        let longd = long.replace(".jpg", "(1).jpg");
        let j = names(&[&cut, &cutd]);
        assert_eq!(match_sidecar(&longd, &j), Some(cutd.as_str()));
        // A short JSON name is never treated as truncated.
        let j = names(&["IMG.json"]);
        assert_eq!(match_sidecar("IMG_2.jpg", &j), None);
    }

    #[test]
    fn best_match_wins() {
        let j = names(&[
            "IMG_5.HEIC.supplemental-metadata.json",
            "IMG_5.MP4.supplemental-metadata.json",
        ]);
        assert_eq!(
            match_sidecar("IMG_5.MP4", &j),
            Some("IMG_5.MP4.supplemental-metadata.json")
        );
        // Live Photo video without its own JSON uses the still's.
        let j = names(&["IMG_6.HEIC.supplemental-metadata.json"]);
        assert_eq!(
            match_sidecar("IMG_6.MP4", &j),
            Some("IMG_6.HEIC.supplemental-metadata.json")
        );
        let j = names(&["IMG_6.HEIC.json"]);
        assert_eq!(match_sidecar("IMG_6.MOV", &j), Some("IMG_6.HEIC.json"));
    }

    #[test]
    fn parses_takeout_json() {
        let json = br#"{
          "title": "IMG_1234.JPG",
          "description": "  ",
          "photoTakenTime": { "timestamp": "1561190459", "formatted": "22 Jun 2019, 08:00:59 UTC" },
          "creationTime": { "timestamp": "1700000000" },
          "geoData": { "latitude": 0.0, "longitude": 0.0, "altitude": 0.0 },
          "geoDataExif": { "latitude": 48.8584, "longitude": 2.2945, "altitude": 35.5 }
        }"#;
        let m = TakeoutMeta::parse(json).unwrap();
        assert_eq!(m.title.as_deref(), Some("IMG_1234.JPG"));
        assert_eq!(m.description, None);
        assert_eq!(m.taken.unwrap().timestamp(), 1_561_190_459);
        assert_eq!(m.geo, None);
        assert_eq!(m.location().unwrap().lat, 48.8584);
        assert_eq!(m.location().unwrap().alt_m, Some(35.5));
        // Numeric timestamps, zero dates and junk.
        let m = TakeoutMeta::parse(br#"{"photoTakenTime":{"timestamp":1561190459}}"#).unwrap();
        assert_eq!(m.taken.unwrap().timestamp(), 1_561_190_459);
        assert_eq!(
            TakeoutMeta::parse(br#"{"photoTakenTime":{"timestamp":"0"}}"#),
            None
        );
        assert_eq!(TakeoutMeta::parse(b"not json"), None);
        assert_eq!(TakeoutMeta::parse(br#"{"albumData":{}}"#), None);
    }

    #[test]
    fn folder_lookup_and_title_fallback() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("IMG_1.jpg.supplemental-metadata.json"),
            br#"{"title":"IMG_1.jpg","photoTakenTime":{"timestamp":"1561190459"}}"#,
        )
        .unwrap();
        std::fs::write(
            d.path().join("odd-name.json"),
            br#"{"title":"Screenshot 2020.png","photoTakenTime":{"timestamp":"1600000000"}}"#,
        )
        .unwrap();
        let f = FolderSidecars::load(d.path());
        assert_eq!(
            f.for_media("IMG_1.jpg").unwrap().taken.unwrap().timestamp(),
            1_561_190_459
        );
        assert_eq!(
            f.for_media("IMG_1-edited.jpg")
                .unwrap()
                .taken
                .unwrap()
                .timestamp(),
            1_561_190_459
        );
        assert_eq!(
            f.for_media("Screenshot 2020.png")
                .unwrap()
                .taken
                .unwrap()
                .timestamp(),
            1_600_000_000
        );
        assert_eq!(f.for_media("other.jpg"), None);
    }
}
