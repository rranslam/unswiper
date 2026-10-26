// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Geo grouping tests. All coordinates are made up (synthetic fixtures only).

use super::*;
use crate::model::{
    AppConfig, Decision, GroupMode, Location, LocationSource, MediaItem, MediaKind, Place,
};
use crate::time::parse_iso;
use serde_json::Map;

/// A fake namer: names a point after the nearest of a few made-up towns within 30 km.
struct Towns;
impl PlaceNamer for Towns {
    fn place(&self, lat: f64, lon: f64) -> Option<Place> {
        let towns = [
            ("Alphaville", "Oregon", "US", 44.0, -121.0),
            ("Betatown", "Region", "PT", 38.7, -9.1),
            ("Gammaburg", "Land", "DE", 50.0, 8.0),
        ];
        towns
            .iter()
            .map(|t| (haversine_km((lat, lon), (t.3, t.4)), t))
            .filter(|(d, _)| *d < 30.0)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, t)| Place {
                name: t.0.into(),
                admin1: t.1.into(),
                country: t.2.into(),
                lat: t.3,
                lon: t.4,
            })
    }
}

fn item(path: &str, iso: &str, loc: Option<(f64, f64)>) -> MediaItem {
    MediaItem {
        path: path.into(),
        kind: MediaKind::Photo,
        size_bytes: 100,
        taken_at: parse_iso(iso).unwrap(),
        relative_folder: "Lib".into(),
        decision: Decision::Undecided,
        modified_at: None,
        sidecars: vec![],
        is_undated: false,
        date_source: None,
        folder_path: Some("trip".into()),
        location: loc.and_then(|(a, b)| Location::new(a, b, LocationSource::Exif)),
        extra: Map::new(),
    }
}

/// `n` photos one minute apart from `start` (ISO, whole minutes), around `at`, jittered ~100 m.
fn burst(prefix: &str, start: &str, n: usize, at: (f64, f64)) -> Vec<MediaItem> {
    let t0 = parse_iso(start).unwrap();
    (0..n)
        .map(|k| {
            let t = t0 + chrono::Duration::minutes(k as i64);
            let j = (k as f64 - n as f64 / 2.0) * 0.0005;
            item(
                &format!("/lib/{prefix}_{k:03}.jpg"),
                &t.to_rfc3339(),
                Some((at.0 + j, at.1 - j)),
            )
        })
        .collect()
}

fn settings(mode: GroupMode) -> GroupSettings {
    let c = AppConfig {
        group_mode: Some(mode.as_str().into()),
        ..Default::default()
    };
    GroupSettings::from_config(&c)
}

const A: (f64, f64) = (44.0, -121.0);
const A2: (f64, f64) = (44.05, -121.0); // ~5.6 km north of A
const B: (f64, f64) = (38.7, -9.1);
const C: (f64, f64) = (50.0, 8.0);

#[test]
fn haversine_and_centroid() {
    assert!(haversine_km((0.0, 0.0), (0.0, 1.0)) - 111.19 < 0.05);
    assert!(haversine_km(A, A) < 1e-9);
    // Across the antimeridian: about 22 km, not 40 000.
    assert!(haversine_km((10.0, 179.9), (10.0, -179.9)) < 25.0);
    let c = centroid(&[(10.0, 179.9), (10.0, -179.9)]).unwrap();
    assert!(c.1.abs() > 179.8, "centroid stays on the antimeridian");
    let p = centroid(&[(89.9, 0.0), (89.9, 180.0)]).unwrap();
    assert!(p.0 > 89.99, "near the pole");
    assert_eq!(centroid(&[]), None);
}

#[test]
fn geohash_known_value() {
    // Reference value of the standard algorithm.
    assert_eq!(geohash(57.64911, 10.40744, 11), "u4pruydqqvj");
    assert_eq!(geohash(A.0, A.1, 7).len(), 7);
}

#[test]
fn settings_defaults_per_mode() {
    assert_eq!(settings(GroupMode::Geo).radius_km, 1.0);
    assert_eq!(settings(GroupMode::VisitGeo).radius_km, 2.0);
    assert_eq!(settings(GroupMode::Geo).min_items, 3);
    let s = GroupSettings::from_config(&AppConfig::default());
    assert_eq!(s.mode, GroupMode::Visit);
    assert!(!s.join_same_place);
    let c = AppConfig {
        group_mode: Some("nonsense".into()),
        place_radius_km: Some(-1.0),
        min_place_items: Some(0),
        ..Default::default()
    };
    let s = GroupSettings::from_config(&c);
    assert_eq!(
        (s.mode, s.radius_km, s.min_items),
        (GroupMode::Visit, 2.0, 3)
    );
}

// ---- DBSCAN ----

#[test]
fn dbscan_two_clusters_and_noise() {
    let mut pts = vec![];
    for k in 0..5 {
        pts.push((A.0 + k as f64 * 0.001, A.1)); // ~110 m apart
    }
    for k in 0..4 {
        pts.push((B.0, B.1 + k as f64 * 0.001));
    }
    pts.push(C); // lone point
    let l = dbscan(&pts, 1.0, 3);
    assert!(l[..5].iter().all(|x| *x == Some(0)));
    assert!(l[5..9].iter().all(|x| *x == Some(1)));
    assert_eq!(l[9], None);
}

#[test]
fn dbscan_min_points_and_chain() {
    // Two points are not a place with min 3.
    assert_eq!(dbscan(&[A, (A.0 + 0.001, A.1)], 1.0, 3), vec![None, None]);
    // A chain of points 0.8 km apart is one place (density-connected), any shape.
    let chain: Vec<_> = (0..10).map(|k| (A.0 + k as f64 * 0.0072, A.1)).collect();
    assert!(dbscan(&chain, 1.0, 3).iter().all(|x| *x == Some(0)));
    // Border point: within eps of a core point but not core itself.
    let pts = [
        A,
        (A.0 + 0.001, A.1),
        (A.0 + 0.002, A.1),
        (A.0 + 0.0105, A.1),
    ];
    let l = dbscan(&pts, 1.0, 3);
    assert_eq!(l, vec![Some(0), Some(0), Some(0), Some(0)]);
    assert!(dbscan(&[], 1.0, 3).is_empty());
}

#[test]
fn dbscan_across_antimeridian_and_pole() {
    let am = [(0.0, 179.999), (0.0, -179.999), (0.001, 179.9995)];
    assert!(dbscan(&am, 1.0, 3).iter().all(|x| *x == Some(0)));
    let pole = [(89.999, 0.0), (89.999, 120.0), (89.999, -120.0)];
    assert!(dbscan(&pole, 1.0, 3).iter().all(|x| *x == Some(0)));
}

// ---- geo mode ----

#[test]
fn geo_groups_by_place_regardless_of_time() {
    let mut items = burst("a1", "2019-06-01T10:00:00Z", 5, A);
    items.extend(burst("b1", "2020-03-01T10:00:00Z", 4, B));
    items.extend(burst("a2", "2023-03-01T10:00:00Z", 6, A)); // A again, years later
    let g = group_with(&items, &settings(GroupMode::Geo), &Towns);
    // A's newest item (2023) is newer than B's (2020), so A comes first.
    assert_eq!(
        g.len(),
        3,
        "{:?}",
        g.iter().map(|x| &x.title).collect::<Vec<_>>()
    );
    assert_eq!(g[0].place.as_ref().unwrap().name, "Alphaville");
    assert_eq!(g[1].place.as_ref().unwrap().name, "Alphaville");
    assert_eq!(g[2].place.as_ref().unwrap().name, "Betatown");
    // Two trips to A, oldest first, sharing a parent place ID.
    assert!(g[0].start_date < g[1].start_date);
    assert_eq!(g[0].parent_visit_id, g[1].parent_visit_id);
    let parent = g[0].parent_visit_id.clone().unwrap();
    assert!(parent.starts_with("g-") && parent.len() == 9);
    assert_eq!(g[0].id, format!("{parent}-20190601"));
    assert_eq!(g[1].id, format!("{parent}-20230301"));
    assert!(g[0].title.ends_with(" · Alphaville, OR"), "{}", g[0].title);
    assert!(g[2].title.ends_with(" · Betatown, PT"));
    assert_eq!(g.iter().map(|x| x.item_count()).sum::<usize>(), 15);
}

#[test]
fn geo_no_location_falls_back_to_date_sets_and_noise_to_other_places() {
    let mut items = burst("a", "2021-01-01T10:00:00Z", 4, A);
    // Unlocated photos far in time from any located one: date sets.
    items.push(item("/lib/n1.jpg", "2022-05-01T10:00:00Z", None));
    items.push(item("/lib/n2.jpg", "2022-05-01T11:00:00Z", None));
    items.push(item("/lib/n3.jpg", "2024-05-01T11:00:00Z", None));
    // A lone located photo nowhere near a town: Other places.
    items.push(item(
        "/lib/lone.jpg",
        "2021-07-01T10:00:00Z",
        Some((-40.0, -140.0)),
    ));
    let g = group_with(&items, &settings(GroupMode::Geo), &Towns);
    assert_eq!(g[0].place.as_ref().unwrap().name, "Alphaville");
    let other = g.iter().find(|x| x.id == OTHER_PLACES_ID).unwrap();
    assert_eq!(other.title, "Other places");
    assert_eq!(other.item_paths, vec!["/lib/lone.jpg".to_string()]);
    let none: Vec<_> = g
        .iter()
        .filter(|x| x.parent_visit_id.as_deref() == Some(NO_LOCATION_ID))
        .collect();
    assert_eq!(none.len(), 2, "two date sets (2022 and 2024)");
    assert!(none.iter().all(|x| x.id.starts_with("visit-")));
    // Places, then Other places, then No location.
    let pos = |id: &str| g.iter().position(|x| x.id == id).unwrap();
    assert!(pos(OTHER_PLACES_ID) > 0 && pos(OTHER_PLACES_ID) < pos(&none[0].id));
}

#[test]
fn geo_noise_in_the_same_town_forms_a_place_only_when_close() {
    // Two loose photos 1.1 km apart in one town: noise for DBSCAN (min 3), but one place.
    let near = (A.0 + 0.01, A.1);
    let items = vec![
        item("/lib/x1.jpg", "2021-01-01T10:00:00Z", Some(A)),
        item("/lib/x2.jpg", "2021-01-02T10:00:00Z", Some(near)),
    ];
    let g = group_with(&items, &settings(GroupMode::Geo), &Towns);
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].place.as_ref().unwrap().name, "Alphaville");
    // M3: 5.6 km apart is too far for loose photos (town name alone used to chain them).
    let items = vec![
        item("/lib/x1.jpg", "2021-01-01T10:00:00Z", Some(A)),
        item("/lib/x2.jpg", "2021-01-02T10:00:00Z", Some(A2)),
    ];
    let g = group_with(&items, &settings(GroupMode::Geo), &Towns);
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].id, OTHER_PLACES_ID);
}

#[test]
fn geo_merges_places_of_one_town_within_5_km() {
    // Two dense spots 2.2 km apart in one town: DBSCAN (1 km) finds two clusters, one place.
    let a_east = (A.0, A.1 + 0.028);
    let mut items = burst("a", "2021-01-01T10:00:00Z", 4, A);
    items.extend(burst("b", "2021-01-01T12:00:00Z", 4, a_east));
    let g = group_with(&items, &settings(GroupMode::Geo), &Towns);
    let parents: std::collections::BTreeSet<_> =
        g.iter().filter_map(|x| x.parent_visit_id.clone()).collect();
    assert_eq!(parents.len(), 1, "{g:#?}");
    assert_eq!(g.iter().map(|x| x.item_count()).sum::<usize>(), 8);
    // Same town name but 5.6 km apart: two places.
    let mut items = burst("a", "2021-01-01T10:00:00Z", 4, A);
    items.extend(burst("b", "2021-01-01T12:00:00Z", 4, A2));
    let g = group_with(&items, &settings(GroupMode::Geo), &Towns);
    let parents: std::collections::BTreeSet<_> =
        g.iter().filter_map(|x| x.parent_visit_id.clone()).collect();
    assert_eq!(parents.len(), 2);
}

#[test]
fn unlocated_items_take_a_hint_from_neighbours_in_time() {
    let mut items = burst("a", "2021-01-01T10:00:00Z", 4, A);
    // A video without GPS 10 minutes after the burst: same place.
    let mut v = item("/lib/clip.3gp", "2021-01-01T10:13:00Z", None);
    v.kind = MediaKind::Video;
    items.push(v);
    // Two hours later: too far in time for a hint.
    items.push(item("/lib/late.jpg", "2021-01-01T12:30:00Z", None));
    let eff = effective_locations(&items);
    assert!(eff[4].is_some());
    assert!(eff[5].is_none());
    let g = group_with(&items, &settings(GroupMode::Geo), &Towns);
    assert!(g[0].item_paths.contains(&"/lib/clip.3gp".to_string()));
    // The hint is never stored.
    assert!(items[4].location.is_none());
}

#[test]
fn geo_ids_are_stable_when_photos_are_added() {
    let base = burst("a", "2021-01-01T10:00:00Z", 5, A);
    let g1 = group_with(&base, &settings(GroupMode::Geo), &Towns);
    let mut more = base.clone();
    more.extend(burst("a_more", "2021-01-01T10:30:00Z", 2, A));
    let g2 = group_with(&more, &settings(GroupMode::Geo), &Towns);
    assert_eq!(g1[0].id, g2[0].id);
    assert_eq!(g2[0].item_count(), 7);
}

#[test]
fn geo_undated_located_items_join_their_folder_trip() {
    let mut items = burst("a", "2021-01-01T10:00:00Z", 4, A);
    let mut u = item("/lib/u.jpg", "0001-01-01T00:00:00Z", Some(A));
    u.is_undated = true;
    items.push(u);
    let g = group_with(&items, &settings(GroupMode::Geo), &Towns);
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].item_count(), 5);
    assert_eq!(*g[0].item_paths.last().unwrap(), "/lib/u.jpg");
    assert_eq!(g[0].start_date, parse_iso("2021-01-01T10:00:00Z").unwrap());
}

// ---- visit mode ----

#[test]
fn visit_mode_is_exactly_v05_grouping_even_with_locations() {
    let mut items = burst("a", "2021-01-01T10:00:00Z", 4, A);
    items.extend(burst("b", "2021-01-01T12:00:00Z", 4, B));
    items.push(item("/lib/n.jpg", "2021-05-01T10:00:00Z", None));
    let mut u = item("/lib/u.jpg", "0001-01-01T00:00:00Z", None);
    u.is_undated = true;
    u.folder_path = Some(String::new());
    items.push(u);
    let s = GroupSettings::from_config(&AppConfig::default());
    let ours = group_with(&items, &s, &Towns);
    let v05 = group::group(&items, 36.0);
    assert_eq!(ours, v05);
    let json = serde_json::to_string(&ours).unwrap();
    assert!(!json.contains("place") && !json.contains("parentVisitId"));
    // Locations don't change visit output at all.
    let stripped: Vec<_> = items
        .iter()
        .cloned()
        .map(|mut i| {
            i.location = None;
            i
        })
        .collect();
    assert_eq!(group_with(&stripped, &s, &Towns), ours);
}

// ---- visit-geo mode ----

#[test]
fn visit_geo_splits_a_visit_by_place() {
    let mut items = burst("a", "2021-01-01T10:00:00Z", 5, A);
    items.extend(burst("a2", "2021-01-01T13:00:00Z", 5, A2)); // same day, 5.6 km away
    let s = settings(GroupMode::VisitGeo);
    let visits = group::group(&items, 36.0);
    assert_eq!(visits.len(), 1);
    let g = group_with(&items, &s, &Towns);
    assert_eq!(g.len(), 2);
    assert_eq!(g[0].id, format!("{}-p1", visits[0].id));
    assert_eq!(g[1].id, format!("{}-p2", visits[0].id));
    assert_eq!(g[0].parent_visit_id.as_deref(), Some(visits[0].id.as_str()));
    assert_eq!(g[0].item_count() + g[1].item_count(), 10);
    assert!(g[0].start_date < g[1].start_date);
}

#[test]
fn visit_geo_absorbs_stray_fixes_and_short_detours() {
    let mut items = burst("a", "2021-01-01T10:00:00Z", 5, A);
    // One stray fix 5.6 km away, then back at A.
    items.push(item("/lib/stray.jpg", "2021-01-01T10:06:00Z", Some(A2)));
    items.extend(burst("a_back", "2021-01-01T10:07:00Z", 3, A));
    let g = group_with(&items, &settings(GroupMode::VisitGeo), &Towns);
    assert_eq!(g.len(), 1, "a single stray doesn't split the visit");
    assert_eq!(g[0].item_count(), 9);
    // One visit at one place keeps the v0.5 ID and gets its place name.
    assert_eq!(g[0].id, group::group(&items, 36.0)[0].id);
    assert!(g[0].title.ends_with(" · Alphaville, OR"));
    assert!(g[0].parent_visit_id.is_none());
}

#[test]
fn visit_geo_change_counts_after_30_minutes_even_with_few_items() {
    let mut items = burst("a", "2021-01-01T10:00:00Z", 5, A);
    items.push(item("/lib/x1.jpg", "2021-01-01T11:00:00Z", Some(A2)));
    items.push(item("/lib/x2.jpg", "2021-01-01T11:31:00Z", Some(A2)));
    let g = group_with(&items, &settings(GroupMode::VisitGeo), &Towns);
    assert_eq!(g.len(), 2);
    assert_eq!(g[1].item_count(), 2);
}

#[test]
fn visit_geo_splits_when_two_items_move_on_and_never_return() {
    // M3: two photos at the end of a visit, 5.6 km away within 30 minutes, used to count as
    // strays, so the visit was not split.
    let mut items = burst("a", "2021-01-01T10:00:00Z", 5, A);
    items.push(item("/lib/x1.jpg", "2021-01-01T10:10:00Z", Some(A2)));
    items.push(item("/lib/x2.jpg", "2021-01-01T10:12:00Z", Some(A2)));
    let g = group_with(&items, &settings(GroupMode::VisitGeo), &Towns);
    assert_eq!(g.len(), 2);
    assert_eq!(g[1].item_count(), 2);
    // Two photos at B, then moving on to C: B is a place of its own.
    let mut items = burst("a", "2021-01-01T10:00:00Z", 5, A);
    items.push(item("/lib/b1.jpg", "2021-01-01T10:10:00Z", Some(A2)));
    items.push(item("/lib/b2.jpg", "2021-01-01T10:11:00Z", Some(A2)));
    items.extend(burst("c", "2021-01-01T10:20:00Z", 5, (A.0 - 0.1, A.1)));
    let g = group_with(&items, &settings(GroupMode::VisitGeo), &Towns);
    assert_eq!(
        g.iter().map(|x| x.item_count()).collect::<Vec<_>>(),
        vec![5, 2, 5]
    );
}

#[test]
fn visit_geo_merges_when_you_come_back() {
    let mut items = burst("a", "2021-01-01T08:00:00Z", 4, A);
    items.extend(burst("a2", "2021-01-01T10:00:00Z", 4, A2));
    items.extend(burst("a_again", "2021-01-01T14:00:00Z", 4, A));
    let g = group_with(&items, &settings(GroupMode::VisitGeo), &Towns);
    assert_eq!(g.len(), 2);
    assert_eq!(g[0].item_count(), 8, "both stays at A are one place");
}

#[test]
fn visit_geo_unlocated_and_undated_items() {
    let mut items = burst("a", "2021-01-01T08:00:00Z", 4, A);
    items.extend(burst("a2", "2021-01-01T12:00:00Z", 4, A2));
    // No location and no hint (an hour away from both), nearer in time to the A2 stay.
    items.push(item("/lib/n.jpg", "2021-01-01T11:00:00Z", None));
    let mut u = item("/lib/u.jpg", "0001-01-01T00:00:00Z", None);
    u.is_undated = true;
    items.push(u);
    let g = group_with(&items, &settings(GroupMode::VisitGeo), &Towns);
    assert_eq!(g.len(), 2);
    assert!(g[1].item_paths.contains(&"/lib/n.jpg".to_string()));
    let total: usize = g.iter().map(|x| x.item_count()).sum();
    assert_eq!(total, 10);
    // A visit with no locations at all stays exactly as in v0.5.
    let plain = vec![
        item("/lib/p1.jpg", "2020-01-01T08:00:00Z", None),
        item("/lib/p2.jpg", "2020-01-01T09:00:00Z", None),
    ];
    assert_eq!(
        group_with(&plain, &settings(GroupMode::VisitGeo), &Towns),
        group::group(&plain, 36.0)
    );
}

#[test]
fn visit_geo_cross_day_join_is_optional() {
    // Two visits 30 h apart at the same place, with a 24 h visit gap.
    let mut items = burst("d1", "2021-01-01T10:00:00Z", 4, A);
    items.extend(burst("d2", "2021-01-02T16:00:00Z", 4, A));
    let mut s = settings(GroupMode::VisitGeo);
    s.gap_hours = 24.0;
    assert_eq!(group_with(&items, &s, &Towns).len(), 2);
    s.join_same_place = true;
    let g = group_with(&items, &s, &Towns);
    assert_eq!(g.len(), 1);
    assert_eq!(g[0].item_count(), 8);
    // Different places are never joined.
    let mut far = burst("d1", "2021-01-01T10:00:00Z", 4, A);
    far.extend(burst("d2", "2021-01-02T16:00:00Z", 4, B));
    assert_eq!(group_with(&far, &s, &Towns).len(), 2);
}

#[test]
fn zero_zero_is_ignored() {
    let items = vec![item("/lib/z.jpg", "2021-01-01T10:00:00Z", Some((0.0, 0.0)))];
    assert!(items[0].location.is_none());
    let g = group_with(&items, &settings(GroupMode::Geo), &Towns);
    assert_eq!(g[0].parent_visit_id.as_deref(), Some(NO_LOCATION_ID));
}

#[test]
fn no_names_gives_unnamed_places() {
    let items = burst("a", "2021-01-01T10:00:00Z", 4, A);
    let g = group_with(&items, &settings(GroupMode::Geo), &NoNames);
    assert!(g[0].place.is_none());
    assert!(g[0].title.ends_with(" · Unnamed place"));
}

#[test]
fn us_states() {
    assert_eq!(us_state_code("Oregon"), Some("OR"));
    assert_eq!(us_state_code("Narnia"), None);
}
