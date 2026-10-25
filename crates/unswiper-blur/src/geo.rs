// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! `unswiper-blur geo`: offline place names for the Mac app (plan §20.3).
//!
//! Reads one JSON request on stdin, `{"cmd":"geo","points":[[lat,lon],...]}`, and writes
//! `{"places":[{"name":…,"admin1":…,"country":…,"lat":…,"lon":…,"label":…} | null, …]}` with
//! one entry per point, in order (`null`: no town within 100 km or an invalid point). Same
//! names as the Linux app because it is the same `unswiper-geo` code. A new subcommand, so the
//! existing `scan`, `info` and `takeout` protocol is unchanged. Coordinates are never logged.

use serde::{Deserialize, Serialize};
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
}
