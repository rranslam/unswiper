// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! `unswiper-blur geo`: offline place names for the Mac app (plan §20.3).
//!
//! Reads one JSON request on stdin, `{"cmd":"geo","points":[[lat,lon],...]}`, and writes
//! `{"places":[{"name":…,"admin1":…,"country":…,"lat":…,"lon":…,"label":…} | null, …]}` with
//! one entry per point, in order (`null`: no town within 100 km or an invalid point). Same
//! names as the Linux app because it is the same `unswiper-geo` code. A new subcommand, so the
//! existing `scan`, `info` and `takeout` protocol is unchanged. Coordinates are never logged.
//!
//! `unswiper-blur group --library library.json [--mode visit|geo|visit-geo]` (plan §20.6, M2):
//! the Mac app's grouping in the place modes. Reads the library the app just saved, reads the
//! GPS of items that have no `location` yet (EXIF for photos, QuickTime/MP4 for videos), groups
//! with the same `unswiper_core::geo::group_with` as Linux, and prints
//! `{"groups":[…],"locations":{"<path>":{"lat":…,"lon":…,"source":…}}}` in the app's JSON
//! layout. It never writes library.json; the app applies the answer and saves.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use unswiper_core::geo::{group_with, GroupSettings};
use unswiper_core::model::{GroupMode, Location};
use unswiper_core::LibrarySnapshot;
use unswiper_geo::Geocoder;

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    cmd: Option<String>,
    points: Vec<(f64, f64)>,
}

#[derive(Serialize)]
struct Answer {
    name: String,
    admin1: String,
    country: String,
    lat: f64,
    lon: f64,
    label: String,
}

#[derive(Serialize)]
struct Response {
    places: Vec<Option<Answer>>,
    attribution: &'static str,
}

/// Answers one request (the JSON text on stdin).
pub fn run(input: &str) -> Result<String, String> {
    let req: Request =
        serde_json::from_str(input.trim()).map_err(|e| format!("geo: bad request ({e})"))?;
    if let Some(c) = req.cmd.as_deref() {
        if c != "geo" {
            return Err(format!("geo: unknown cmd {c:?}"));
        }
    }
    let g = Geocoder::new();
    let places = req
        .points
        .iter()
        .map(|&(lat, lon)| {
            g.lookup(lat, lon).map(|p| Answer {
                label: p.label(),
                name: p.name,
                admin1: p.admin1,
                country: p.country,
                lat: p.lat,
                lon: p.lon,
            })
        })
        .collect();
    serde_json::to_string(&Response {
        places,
        attribution: unswiper_geo::ATTRIBUTION,
    })
    .map_err(|e| e.to_string())
}

pub const GROUP_USAGE: &str =
    "usage: unswiper-blur group --library library.json [--mode visit|geo|visit-geo]";

/// Parses `group` arguments: (library, mode override).
pub fn parse_group(args: &[String]) -> Result<(String, Option<GroupMode>), String> {
    let mut library = None;
    let mut mode = None;
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--library" => library = it.next().cloned(),
            "--mode" => {
                let v = it.next().ok_or(GROUP_USAGE)?;
                mode = Some(GroupMode::parse(v).ok_or(format!("group: unknown mode {v:?}"))?);
            }
            _ => return Err(GROUP_USAGE.into()),
        }
    }
    Ok((library.ok_or(GROUP_USAGE)?, mode))
}

#[derive(Serialize)]
struct GroupAnswer {
    groups: Vec<unswiper_core::VisitGroup>,
    /// Locations read now for items that had none (path → location).
    locations: BTreeMap<String, Location>,
}

/// Groups the library at `library` with its saved settings (or `mode`) and real place names.
pub fn group(library: &Path, mode: Option<GroupMode>) -> Result<String, String> {
    let data = std::fs::read(library).map_err(|e| format!("group: can't read library ({e})"))?;
    let mut snap: LibrarySnapshot =
        serde_json::from_slice(&data).map_err(|e| format!("group: bad library ({e})"))?;
    let mut locations = BTreeMap::new();
    for it in snap.items.iter_mut().filter(|i| i.location.is_none()) {
        if let Some(l) = unswiper_core::index::file_location(Path::new(&it.path), it.kind) {
            it.location = Some(l);
            locations.insert(it.path.clone(), l);
        }
    }
    let mut settings = GroupSettings::from_config(&snap.config);
    if let Some(m) = mode {
        settings.mode = m;
        if snap.config.place_radius_km.is_none() {
            settings.radius_km = m.default_radius_km();
        }
    }
    let groups = group_with(&snap.items, &settings, &Geocoder::new());
    let v = serde_json::to_value(GroupAnswer { groups, locations }).map_err(|e| e.to_string())?;
    Ok(unswiper_core::apple_json::to_string(&v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_each_point_in_order() {
        let out = run(r#"{"cmd":"geo","points":[[38.7223,-9.1393],[0,-30],[44.0582,-121.3153]]}"#)
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let p = v["places"].as_array().unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p[0]["label"], "Lisbon, PT");
        assert!(p[1].is_null());
        assert_eq!(p[2]["name"], "Bend");
        assert_eq!(p[2]["country"], "US");
        assert!(v["attribution"].as_str().unwrap().contains("GeoNames"));
    }

    #[test]
    fn cmd_is_optional_but_checked() {
        assert!(run(r#"{"points":[]}"#).unwrap().contains("\"places\":[]"));
        assert!(run(r#"{"cmd":"scan","points":[]}"#).is_err());
        assert!(run("not json").is_err());
    }

    #[test]
    fn group_uses_saved_mode_and_reports_new_locations() {
        let d = tempfile::tempdir().unwrap();
        let lib = d.path().join("library.json");
        let mut snap = LibrarySnapshot::empty();
        snap.config.group_mode = Some("geo".into());
        for (i, n) in ["a.jpg", "b.jpg", "c.jpg"].iter().enumerate() {
            let p = d.path().join(n);
            std::fs::write(&p, b"x").unwrap();
            let mut it: unswiper_core::MediaItem = serde_json::from_value(serde_json::json!({
                "path": p.to_string_lossy(), "kind": "photo", "sizeBytes": 1,
                "takenAt": format!("2026-06-0{}T10:00:00Z", i + 1),
                "relativeFolder": "", "decision": "undecided"
            }))
            .unwrap();
            // Made-up coordinates (synthetic test data).
            it.location =
                Location::new(38.7223, -9.1393, unswiper_core::model::LocationSource::Exif);
            snap.items.push(it);
        }
        std::fs::write(&lib, snap.to_json()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&group(&lib, None).unwrap()).unwrap();
        let g = v["groups"].as_array().unwrap();
        assert!(g.iter().all(|g| g["parentVisitId"].is_string()));
        assert_eq!(g[0]["place"]["name"], "Lisbon");
        assert!(v["locations"].as_object().unwrap().is_empty());
        let v: serde_json::Value =
            serde_json::from_str(&group(&lib, Some(GroupMode::Visit)).unwrap()).unwrap();
        assert!(v["groups"][0]["parentVisitId"].is_null());
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            parse_group(&a(&["group", "--library", "x.json", "--mode", "visit-geo"])).unwrap(),
            ("x.json".to_string(), Some(GroupMode::VisitGeo))
        );
        assert!(parse_group(&a(&["group"])).is_err());
        assert!(parse_group(&a(&["group", "--library", "x", "--mode", "nope"])).is_err());
    }
}
