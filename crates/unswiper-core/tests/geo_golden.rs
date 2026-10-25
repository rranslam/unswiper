// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Golden test (plan §20.8): `visit` mode gives exactly the v0.5 visits. The fixture is the
//! library.json the Swift app wrote for the synthetic crosscheck tree (no personal media).

use std::collections::BTreeSet;
use unswiper_core::geo::{group_with, GroupSettings, NoNames};
use unswiper_core::{group, GroupMode, LibrarySnapshot, Location, LocationSource};

const SWIFT: &str = include_str!("fixtures/crosscheck/swift/library.json");

/// (id, sorted members): independent of the local time zone used for titles.
fn shape(groups: &[unswiper_core::VisitGroup]) -> BTreeSet<(String, Vec<String>)> {
    groups
        .iter()
        .map(|g| {
            let mut p = g.item_paths.clone();
            p.sort();
            (g.id.clone(), p)
        })
        .collect()
}

#[test]
fn visit_mode_equals_v05_grouping() {
    let snap: LibrarySnapshot = serde_json::from_str(SWIFT).unwrap();
    let s = GroupSettings::from_config(&snap.config);
    assert_eq!(s.mode, GroupMode::Visit, "no groupMode saved = visit");
    let ours = group_with(&snap.items, &s, &NoNames);
    // Same visits as the v0.5 (Swift-written) library …
    assert_eq!(shape(&ours), shape(&snap.groups));
    // … and byte-identical to the unchanged v0.5 grouper.
    let v05 = group::group(&snap.items, snap.config.visit_gap_hours);
    assert_eq!(
        serde_json::to_string(&ours).unwrap(),
        serde_json::to_string(&v05).unwrap()
    );

    // Locations on every item don't change visit mode.
    let mut located = snap.clone();
    for (k, i) in located.items.iter_mut().enumerate() {
        i.location = Location::new(10.0 + k as f64, 20.0, LocationSource::Exif);
    }
    let again = group_with(&located.items, &s, &NoNames);
    assert_eq!(
        serde_json::to_string(&again).unwrap(),
        serde_json::to_string(&v05).unwrap()
    );
}
