// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Offline reverse geocoding for Unswiper (plan §20.3, option A).
//!
//! Wraps the `reverse_geocoder` crate (MIT OR Apache-2.0), which bundles the GeoNames
//! `cities1000` table (CC BY 4.0, <https://www.geonames.org/>). No network access, ever.
//! Callers use [`Geocoder`] through `unswiper_core::geo::PlaceNamer`, so the data source can be
//! swapped for option B (own GeoNames table with admin-1 names) without touching them.

use reverse_geocoder::ReverseGeocoder;
use std::sync::OnceLock;
use unswiper_core::geo::{PlaceNamer, EARTH_RADIUS_KM};
use unswiper_core::Place;

/// Points farther than this from the nearest town get no name (open sea, ice caps).
pub const MAX_DISTANCE_KM: f64 = 100.0;

/// Attribution to show wherever place names appear (About, Help).
pub const ATTRIBUTION: &str =
    "Place names: GeoNames (www.geonames.org), CC BY 4.0, via reverse_geocoder.";

fn data() -> &'static ReverseGeocoder {
    static G: OnceLock<ReverseGeocoder> = OnceLock::new();
    G.get_or_init(ReverseGeocoder::new)
}

/// The bundled offline geocoder. Cheap to create; the table loads once, on first use.
#[derive(Debug, Default, Clone, Copy)]
pub struct Geocoder;

impl Geocoder {
    pub fn new() -> Geocoder {
        Geocoder
    }

    /// Nearest town within [`MAX_DISTANCE_KM`], or `None`. Invalid input gives `None`.
    pub fn lookup(&self, lat: f64, lon: f64) -> Option<Place> {
        if !lat.is_finite() || !lon.is_finite() || lat.abs() > 90.0 || lon.abs() > 180.0 {
            return None;
        }
        let r = data().search((lat, lon));
        // `distance` is the squared chord length on the unit sphere.
        let chord = r.distance.max(0.0).sqrt().min(2.0);
        let km = 2.0 * EARTH_RADIUS_KM * (chord / 2.0).asin();
        if km > MAX_DISTANCE_KM {
            return None;
        }
        Some(Place {
            name: r.record.name.clone(),
            admin1: r.record.admin1.clone(),
            country: r.record.cc.clone(),
            lat: r.record.lat,
            lon: r.record.lon,
        })
    }
}

impl PlaceNamer for Geocoder {
    fn place(&self, lat: f64, lon: f64) -> Option<Place> {
        self.lookup(lat, lon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(lat: f64, lon: f64) -> Option<String> {
        Geocoder.lookup(lat, lon).map(|p| p.label())
    }

    #[test]
    fn world_cities() {
        assert_eq!(label(38.7223, -9.1393).as_deref(), Some("Lisbon, PT"));
        assert_eq!(label(44.0582, -121.3153).as_deref(), Some("Bend, OR"));
        assert_eq!(label(48.8566, 2.3522).as_deref(), Some("Paris, FR"));
        let tokyo = Geocoder.lookup(35.6895, 139.6917).unwrap();
        assert_eq!(tokyo.country, "JP");
    }

    #[test]
    fn open_sea_and_poles_have_no_name() {
        assert_eq!(label(0.0, -30.0), None); // mid-Atlantic
        assert_eq!(label(-89.9, 0.0), None);
        assert_eq!(label(89.9, 0.0), None);
        assert_eq!(label(f64::NAN, 0.0), None);
        assert_eq!(label(91.0, 0.0), None);
    }

    #[test]
    fn near_the_antimeridian_and_far_north() {
        // Taveuni, Fiji, straddles 180°: either side finds a Fijian town.
        assert_eq!(Geocoder.lookup(-16.8, 179.99).unwrap().country, "FJ");
        assert_eq!(Geocoder.lookup(-16.8, -179.99).unwrap().country, "FJ");
        // Longyearbyen, Svalbard.
        assert_eq!(Geocoder.lookup(78.2232, 15.6267).unwrap().country, "SJ");
    }

    #[test]
    fn works_as_a_place_namer() {
        let n: &dyn PlaceNamer = &Geocoder::new();
        assert_eq!(n.place(38.7223, -9.1393).unwrap().name, "Lisbon");
    }
}
