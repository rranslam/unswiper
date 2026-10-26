// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Data model, field-for-field compatible with `Models.swift` (library.json schema v1), plus the
//! optional schema v2 geo fields (plan §20.5). Every v2 field is optional and left out when empty,
//! so a library without locations serialises exactly as v1 apart from `schemaVersion`.
//! Unknown JSON fields are kept (`extra`) so a newer app's data survives a round trip.

use crate::time::{serde_opt_ts, serde_ts, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Photo,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    #[default]
    Undecided,
    Keep,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommitMode {
    #[serde(rename = "trash")]
    Trash,
    #[serde(rename = "quarantine")]
    Quarantine,
    #[serde(rename = "dry_run")]
    DryRun,
}

impl CommitMode {
    pub fn label(self) -> &'static str {
        match self {
            CommitMode::Trash => "Move to Trash",
            CommitMode::Quarantine => "Quarantine folder",
            CommitMode::DryRun => "Dry run (decisions only)",
        }
    }
}

/// Where an item's grouping date came from (plan §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateSource {
    /// EXIF / TIFF / QuickTime capture date.
    Capture,
    /// The photo's GPS timestamp (UTC), for photos without an EXIF date (schema 2, plan §20.14).
    Gps,
    /// Parsed from the file name (`IMG_20240612_133241`, `PXL_…`; schema 2, plan §20.14).
    #[serde(rename = "filename")]
    FileName,
    /// Parsed from the folder path (`2026/01/01`, `2025-07-04 Beach`).
    Folder,
    /// No date of its own; grouped via dated siblings or the folder name.
    Unknown,
}

/// Where an item's location came from (plan §20.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LocationSource {
    /// EXIF GPS of a photo.
    Exif,
    /// QuickTime / MP4 location of a video.
    Quicktime,
    /// Google Takeout JSON `geoData` / `geoDataExif`.
    Takeout,
}

/// A capture location, rounded to 5 decimals (about 1 m). `0,0` is never stored.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub lat: f64,
    pub lon: f64,
    pub source: LocationSource,
}

impl Location {
    /// A rounded location, or `None` for `0,0`, NaN or out-of-range values.
    pub fn new(lat: f64, lon: f64, source: LocationSource) -> Option<Location> {
        let ok = lat.is_finite()
            && lon.is_finite()
            && (-90.0..=90.0).contains(&lat)
            && (-180.0..=180.0).contains(&lon)
            && !(lat == 0.0 && lon == 0.0);
        let r = |v: f64| (v * 1e5).round() / 1e5;
        ok.then(|| Location {
            lat: r(lat),
            lon: r(lon),
            source,
        })
    }
}

/// A place name from the offline reverse geocoder (plan §20.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub name: String,
    #[serde(default)]
    pub admin1: String,
    #[serde(default)]
    pub country: String,
    pub lat: f64,
    pub lon: f64,
}

impl Place {
    /// "Bend, OR" (US state abbreviation) or "Lisbon, PT" (country code).
    pub fn label(&self) -> String {
        let suffix = if self.country == "US" {
            crate::geo::us_state_code(&self.admin1).unwrap_or("US")
        } else {
            self.country.as_str()
        };
        if suffix.is_empty() {
            self.name.clone()
        } else {
            format!("{}, {suffix}", self.name)
        }
    }
}

/// How visits are built (plan §20.2). Stored as text in `AppConfig.groupMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GroupMode {
    /// Time-gap visits exactly as in v0.5 (default).
    #[default]
    Visit,
    /// One group per place name, regardless of time (Rob, 25 Oct 2026).
    Geo,
    /// Each time visit split by place.
    VisitGeo,
}

impl GroupMode {
    /// `visit`, `geo` or `visit-geo`; anything else is `None`.
    pub fn parse(s: &str) -> Option<GroupMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "visit" => Some(GroupMode::Visit),
            "geo" => Some(GroupMode::Geo),
            "visit-geo" | "visit_geo" | "visitgeo" => Some(GroupMode::VisitGeo),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            GroupMode::Visit => "visit",
            GroupMode::Geo => "geo",
            GroupMode::VisitGeo => "visit-geo",
        }
    }
    /// Default `placeRadiusKm` for this mode: 1 km for `geo`, 2 km otherwise.
    pub fn default_radius_km(self) -> f64 {
        match self {
            GroupMode::Geo => 1.0,
            _ => 2.0,
        }
    }
}

/// A file that travels with a primary item (Live Photo MOV, .AAE/.XMP sidecar).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarFile {
    pub path: String,
    pub size_bytes: i64,
    #[serde(
        default,
        with = "serde_opt_ts",
        skip_serializing_if = "Option::is_none"
    )]
    pub modified_at: Option<Timestamp>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    pub path: String,
    pub kind: MediaKind,
    pub size_bytes: i64,
    #[serde(with = "serde_ts")]
    pub taken_at: Timestamp,
    pub relative_folder: String,
    pub decision: Decision,
    #[serde(
        default,
        with = "serde_opt_ts",
        skip_serializing_if = "Option::is_none"
    )]
    pub modified_at: Option<Timestamp>,
    #[serde(default)]
    pub sidecars: Vec<SidecarFile>,
    #[serde(default)]
    pub is_undated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_source: Option<DateSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
    /// Schema v2: capture location, when the file (or its Takeout sidecar) has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl MediaItem {
    pub fn total_bytes(&self) -> i64 {
        self.sidecars
            .iter()
            .fold(self.size_bytes, |a, s| a + s.size_bytes)
    }
    pub fn file_count(&self) -> usize {
        1 + self.sidecars.len()
    }
    pub fn file_name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManifestStatus {
    Pending,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryState {
    Pending,
    Moved,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub original_path: String,
    /// Planned path for quarantine; Trash location (may stay empty) for trash.
    pub result_path: String,
    pub size_bytes: i64,
    #[serde(
        default,
        with = "serde_opt_ts",
        skip_serializing_if = "Option::is_none"
    )]
    pub moved_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<EntryState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl ManifestEntry {
    /// Manifests from before v0.1.2 have no state: treated as moved.
    pub fn effective_state(&self) -> EntryState {
        self.state.unwrap_or(EntryState::Moved)
    }
}

/// Record of one commit (journal). Written `pending` before any file moves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitManifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ManifestStatus>,
    pub id: String,
    #[serde(with = "serde_ts")]
    pub created_at: Timestamp,
    pub mode: CommitMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_root: Option<String>,
    pub entries: Vec<ManifestEntry>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl CommitManifest {
    pub fn is_pending(&self) -> bool {
        self.status == Some(ManifestStatus::Pending)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisitGroup {
    pub id: String,
    pub title: String,
    #[serde(with = "serde_ts")]
    pub start_date: Timestamp,
    #[serde(with = "serde_ts")]
    pub end_date: Timestamp,
    pub item_paths: Vec<String>,
    pub photo_count: i64,
    pub video_count: i64,
    pub total_bytes: i64,
    pub folder_hint: String,
    /// Schema v2: the place of a geo group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<Place>,
    /// Schema v2: the visit (`visit-geo`) or place (`geo`) this group belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_visit_id: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// (kept, rejected, left) for a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub kept: usize,
    pub rejected: usize,
    pub left: usize,
}

impl VisitGroup {
    pub fn item_count(&self) -> usize {
        self.item_paths.len()
    }

    pub fn progress(&self, items: &[MediaItem]) -> Progress {
        let set: std::collections::HashSet<&str> =
            self.item_paths.iter().map(String::as_str).collect();
        let mut p = Progress {
            kept: 0,
            rejected: 0,
            left: 0,
        };
        for i in items.iter().filter(|i| set.contains(i.path.as_str())) {
            match i.decision {
                Decision::Keep => p.kept += 1,
                Decision::Reject => p.rejected += 1,
                Decision::Undecided => p.left += 1,
            }
        }
        p
    }
}

mod serde_b64 {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(d: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match d {
            Some(d) => s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(d)),
            None => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        match Option::<String>::deserialize(d)? {
            None => Ok(None),
            Some(s) => base64::engine::general_purpose::STANDARD
                .decode(s)
                .map(Some)
                .map_err(serde::de::Error::custom),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_path: Option<String>,
    pub keep_target_default: i64,
    pub visit_gap_hours: f64,
    pub commit_mode: CommitMode,
    /// macOS security-scoped bookmark (opaque; kept so the Mac app keeps working).
    #[serde(default, with = "serde_b64", skip_serializing_if = "Option::is_none")]
    pub bookmark_data: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_targets: Option<BTreeMap<String, i64>>,
    /// Schema v2: `visit` | `geo` | `visit-geo`, kept as written so an unknown value from a newer
    /// app survives a round trip. Missing or unknown reads as `visit` ([`AppConfig::group_mode`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place_radius_km: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_place_items: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_same_place_days: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl AppConfig {
    /// The saved grouping mode; missing or unknown values read as [`GroupMode::Visit`].
    pub fn group_mode(&self) -> GroupMode {
        self.group_mode
            .as_deref()
            .and_then(GroupMode::parse)
            .unwrap_or_default()
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            root_path: None,
            keep_target_default: 10,
            visit_gap_hours: 36.0,
            commit_mode: CommitMode::Trash,
            bookmark_data: None,
            keep_targets: None,
            group_mode: None,
            place_radius_km: None,
            min_place_items: None,
            join_same_place_days: None,
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    /// Missing in files written before v0.1.2 (treated as 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<i64>,
    pub config: AppConfig,
    pub items: Vec<MediaItem>,
    pub groups: Vec<VisitGroup>,
    #[serde(
        default,
        with = "serde_opt_ts",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_indexed_at: Option<Timestamp>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl LibrarySnapshot {
    pub const CURRENT_SCHEMA_VERSION: i64 = 2;

    pub fn empty() -> Self {
        LibrarySnapshot {
            schema_version: Some(Self::CURRENT_SCHEMA_VERSION),
            config: AppConfig::default(),
            items: vec![],
            groups: vec![],
            last_indexed_at: None,
            extra: Map::new(),
        }
    }

    /// Serialises in Swift's exact layout (pretty, sorted keys, `\/`).
    pub fn to_json(&self) -> String {
        crate::apple_json::to_string(&serde_json::to_value(self).expect("snapshot serialises"))
    }
}

impl CommitManifest {
    pub fn to_json(&self) -> String {
        crate::apple_json::to_string(&serde_json::to_value(self).expect("manifest serialises"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::parse_iso;

    fn item(path: &str, d: Decision) -> MediaItem {
        MediaItem {
            path: path.into(),
            kind: MediaKind::Photo,
            size_bytes: 10,
            taken_at: parse_iso("2026-01-01T00:00:00Z").unwrap(),
            relative_folder: "x".into(),
            decision: d,
            modified_at: None,
            sidecars: vec![SidecarFile {
                path: format!("{path}.aae"),
                size_bytes: 5,
                modified_at: None,
                extra: Map::new(),
            }],
            is_undated: false,
            date_source: None,
            folder_path: None,
            location: None,
            extra: Map::new(),
        }
    }

    #[test]
    fn item_helpers() {
        let i = item("/a/b/IMG_1.HEIC", Decision::Keep);
        assert_eq!(i.total_bytes(), 15);
        assert_eq!(i.file_count(), 2);
        assert_eq!(i.file_name(), "IMG_1.HEIC");
    }

    #[test]
    fn group_progress() {
        let items = vec![
            item("/a", Decision::Keep),
            item("/b", Decision::Reject),
            item("/c", Decision::Undecided),
            item("/d", Decision::Keep),
        ];
        let g = VisitGroup {
            id: "g".into(),
            title: "t".into(),
            start_date: crate::time::distant_past(),
            end_date: crate::time::distant_past(),
            item_paths: vec!["/a".into(), "/b".into(), "/c".into()],
            photo_count: 3,
            video_count: 0,
            total_bytes: 0,
            folder_hint: "".into(),
            place: None,
            parent_visit_id: None,
            extra: Map::new(),
        };
        assert_eq!(g.item_count(), 3);
        assert_eq!(
            g.progress(&items),
            Progress {
                kept: 1,
                rejected: 1,
                left: 1
            }
        );
    }

    #[test]
    fn mode_labels_and_names() {
        assert_eq!(CommitMode::Trash.label(), "Move to Trash");
        assert_eq!(CommitMode::Quarantine.label(), "Quarantine folder");
        assert_eq!(CommitMode::DryRun.label(), "Dry run (decisions only)");
        assert_eq!(
            serde_json::to_string(&CommitMode::DryRun).unwrap(),
            "\"dry_run\""
        );
    }

    #[test]
    fn effective_state_defaults_to_moved() {
        let e: ManifestEntry =
            serde_json::from_str(r#"{"originalPath":"/a","resultPath":"/b","sizeBytes":1}"#)
                .unwrap();
        assert_eq!(e.effective_state(), EntryState::Moved);
    }

    #[test]
    fn bookmark_base64_round_trip_and_error() {
        let c = AppConfig {
            bookmark_data: Some(vec![1, 2, 3, 255]),
            ..Default::default()
        };
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["bookmarkData"], "AQID/w==");
        let back: AppConfig = serde_json::from_value(v).unwrap();
        assert_eq!(back, c);
        let bad = serde_json::from_str::<AppConfig>(
            r#"{"keepTargetDefault":1,"visitGapHours":1,"commitMode":"trash","bookmarkData":"!!"}"#,
        );
        assert!(bad.is_err());
        let bad_date = serde_json::from_str::<SidecarFile>(
            r#"{"path":"/a","sizeBytes":1,"modifiedAt":"yesterday"}"#,
        );
        assert!(bad_date.is_err());
    }

    #[test]
    fn unknown_or_missing_group_mode_reads_as_visit() {
        let mut c = AppConfig::default();
        assert_eq!(c.group_mode(), GroupMode::Visit);
        c.group_mode = Some("spiral".into());
        assert_eq!(c.group_mode(), GroupMode::Visit);
        // The unknown value itself is kept for a round trip.
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["groupMode"], "spiral");
        c.group_mode = Some("visit-geo".into());
        assert_eq!(c.group_mode(), GroupMode::VisitGeo);
        c.group_mode = Some("GEO".into());
        assert_eq!(c.group_mode(), GroupMode::Geo);
    }

    #[test]
    fn v1_config_has_no_v2_keys() {
        let v = serde_json::to_value(AppConfig::default()).unwrap();
        for k in [
            "groupMode",
            "placeRadiusKm",
            "minPlaceItems",
            "joinSamePlaceDays",
        ] {
            assert!(v.get(k).is_none(), "{k}");
        }
    }

    #[test]
    fn location_rounds_and_rejects_zero() {
        let l = Location::new(45.123456789, -122.987654321, LocationSource::Exif).unwrap();
        assert_eq!((l.lat, l.lon), (45.12346, -122.98765));
        assert!(Location::new(0.0, 0.0, LocationSource::Takeout).is_none());
        assert!(Location::new(91.0, 0.0, LocationSource::Exif).is_none());
        assert!(Location::new(f64::NAN, 1.0, LocationSource::Exif).is_none());
        let j = serde_json::to_value(l).unwrap();
        assert_eq!(j["source"], "exif");
    }

    #[test]
    fn place_labels() {
        let p = |n: &str, a: &str, c: &str| Place {
            name: n.into(),
            admin1: a.into(),
            country: c.into(),
            lat: 0.0,
            lon: 0.0,
        };
        assert_eq!(p("Bend", "Oregon", "US").label(), "Bend, OR");
        assert_eq!(p("Lisbon", "Lisbon", "PT").label(), "Lisbon, PT");
        assert_eq!(p("Somewhere", "", "").label(), "Somewhere");
    }
}
