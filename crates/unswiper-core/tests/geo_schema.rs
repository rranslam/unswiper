// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! library.json schema v2 (plan §20.5): optional geo fields and v1 migration.

use unswiper_core::store::LibraryStore;
use unswiper_core::{GroupMode, LibrarySnapshot, Location, LocationSource, Place};

const V1: &str = include_str!("fixtures/compat/library-v1.json");

#[test]
fn v1_file_migrates_to_v2_without_other_changes() {
    let snap: LibrarySnapshot = serde_json::from_str(V1).unwrap();
    assert_eq!(snap.schema_version, Some(1));
    assert_eq!(snap.config.group_mode(), GroupMode::Visit);
    assert!(snap.items.iter().all(|i| i.location.is_none()));
    assert!(snap.groups.iter().all(|g| g.place.is_none()));

    let d = tempfile::tempdir().unwrap();
    let mut st = LibraryStore::new(d.path());
    std::fs::write(st.snapshot_path(), V1).unwrap();
    let r = st.load();
    assert!(r.problem.is_none(), "a v1 file loads");
    st.save(&r.snapshot).unwrap();
    let saved = std::fs::read_to_string(st.snapshot_path()).unwrap();
    // Only the version number changes: no geo keys are added for a library without locations.
    assert_eq!(
        saved,
        V1.replace("\"schemaVersion\" : 1", "\"schemaVersion\" : 2")
    );
    for k in ["groupMode", "location", "place", "parentVisitId"] {
        assert!(!saved.contains(k), "{k}");
    }
}

#[test]
fn v2_fields_round_trip_byte_for_byte() {
    let mut snap: LibrarySnapshot = serde_json::from_str(V1).unwrap();
    snap.schema_version = Some(2);
    snap.config.group_mode = Some("visit-geo".into());
    snap.config.place_radius_km = Some(1.5);
    snap.config.min_place_items = Some(4);
    snap.config.join_same_place_days = Some(true);
    snap.items[0].location = Location::new(12.34567, -65.4321, LocationSource::Exif);
    snap.groups[0].place = Some(Place {
        name: "Testville".into(),
        admin1: "Nowhere".into(),
        country: "ZZ".into(),
        lat: 12.34567,
        lon: -65.4321,
    });
    snap.groups[0].parent_visit_id = Some("visit-abc".into());
    let json = snap.to_json();
    let back: LibrarySnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snap);
    assert_eq!(back.to_json(), json);
    assert_eq!(back.config.group_mode(), GroupMode::VisitGeo);
    assert!(json.contains("\"groupMode\" : \"visit-geo\""));
    assert!(json.contains("\"source\" : \"exif\""));
}

#[test]
fn unknown_group_mode_survives_and_reads_as_visit() {
    let json = V1.replace(
        "\"commitMode\"",
        "\"groupMode\" : \"by-moon-phase\",\n    \"commitMode\"",
    );
    let snap: LibrarySnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(snap.config.group_mode(), GroupMode::Visit);
    assert_eq!(snap.config.group_mode.as_deref(), Some("by-moon-phase"));
    let out = snap.to_json();
    assert!(out.contains("by-moon-phase"));
}

#[test]
fn newer_than_v2_is_still_refused() {
    let d = tempfile::tempdir().unwrap();
    let mut st = LibraryStore::new(d.path());
    std::fs::write(
        st.snapshot_path(),
        V1.replace("\"schemaVersion\" : 1", "\"schemaVersion\" : 3"),
    )
    .unwrap();
    assert!(st.load().problem.unwrap().contains("format 3"));
}

/// Golden v2 file shared with the Mac tests (`LibraryStoreTests.testV2GoldenRoundTripsExactly`):
/// both apps must read it and write it back byte for byte.
#[test]
fn v2_golden_round_trips_exactly() {
    const V2: &str = include_str!("fixtures/compat/library-v2.json");
    let snap: LibrarySnapshot = serde_json::from_str(V2).unwrap();
    assert_eq!(snap.schema_version, Some(2));
    assert_eq!(snap.config.group_mode(), GroupMode::VisitGeo);
    assert_eq!(snap.items[0].location.unwrap().source, LocationSource::Exif);
    assert_eq!(snap.groups[0].place.as_ref().unwrap().label(), "Lisbon, PT");
    assert_eq!(
        snap.groups[0].parent_visit_id.as_deref(),
        Some("visit-1a2b")
    );
    assert_eq!(snap.to_json(), V2);
}
