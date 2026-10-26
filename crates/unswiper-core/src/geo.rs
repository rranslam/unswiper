// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Geo grouping (plan §20.2): the three grouping modes.
//!
//! - [`GroupMode::Visit`] is exactly [`group::group`] (v0.5 output, byte for byte).
//! - [`GroupMode::Geo`] puts every item with one reverse-geocoded place name (name, region,
//!   country) into one group, regardless of time: no trips, no distance clustering (Rob,
//!   25 Oct 2026; superseded DBSCAN). [`dbscan`] is kept for reference and tests.
//! - [`GroupMode::VisitGeo`] splits each time visit by place with a time-ordered pass.
//!
//! Items without a location borrow a place hint from a located item taken within
//! [`HINT_WINDOW_MIN`] minutes (plan §20.11); the hint is only used for grouping and is never
//! stored. Place names come from a [`PlaceNamer`] (the `unswiper-geo` crate); core has no data.
//! Privacy: nothing here logs or prints coordinates.

use crate::group::{self, make_visit, most_common_folder, simple_group, title_for};
use crate::model::{GroupMode, MediaItem, Place, VisitGroup};
use crate::time::Timestamp;
use std::collections::{BTreeMap, HashMap};

/// Mean Earth radius in km.
pub const EARTH_RADIUS_KM: f64 = 6371.0088;
/// Items without a location take the place of a located item this close in time.
pub const HINT_WINDOW_MIN: i64 = 30;
/// `visit-geo`: a place change counts once it lasts this long (or `min_place_items` items).
pub const PLACE_CHANGE_MIN: i64 = 30;
/// `visit-geo` cross-day join: adjacent visits at the same place closer than this.
pub const JOIN_GAP_HOURS: f64 = 36.0;
pub const DEFAULT_MIN_PLACE_ITEMS: usize = 3;
/// `parentVisitId` of the date-based sets of items without a location in `geo` mode.
pub const NO_LOCATION_ID: &str = "no-location";
/// ID of the `geo` group of located items that belong to no place.
pub const OTHER_PLACES_ID: &str = "g-other";

/// Offline reverse geocoding, implemented by `unswiper-geo`.
pub trait PlaceNamer {
    fn place(&self, lat: f64, lon: f64) -> Option<Place>;
}

/// A namer without data: groups still form, titles say "Unnamed place".
pub struct NoNames;
impl PlaceNamer for NoNames {
    fn place(&self, _: f64, _: f64) -> Option<Place> {
        None
    }
}

/// Everything grouping needs, from `AppConfig` plus CLI overrides.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupSettings {
    pub mode: GroupMode,
    pub gap_hours: f64,
    pub radius_km: f64,
    pub min_items: usize,
    pub join_same_place: bool,
}

impl GroupSettings {
    /// Settings saved in a library's config (mode defaults to `visit`, radius to the mode's).
    pub fn from_config(c: &crate::model::AppConfig) -> GroupSettings {
        let mode = c.group_mode();
        GroupSettings {
            mode,
            gap_hours: c.visit_gap_hours,
            radius_km: c
                .place_radius_km
                .filter(|r| r.is_finite() && *r > 0.0)
                .unwrap_or(mode.default_radius_km()),
            min_items: c
                .min_place_items
                .filter(|n| *n >= 1)
                .map(|n| n as usize)
                .unwrap_or(DEFAULT_MIN_PLACE_ITEMS),
            join_same_place: c.join_same_place_days.unwrap_or(false),
        }
    }
}

/// Great-circle distance in km.
pub fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, lo1) = (a.0.to_radians(), a.1.to_radians());
    let (la2, lo2) = (b.0.to_radians(), b.1.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2)
        + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().min(1.0).asin()
}

fn xyz(p: (f64, f64)) -> [f64; 3] {
    let (la, lo) = (p.0.to_radians(), p.1.to_radians());
    [
        EARTH_RADIUS_KM * la.cos() * lo.cos(),
        EARTH_RADIUS_KM * la.cos() * lo.sin(),
        EARTH_RADIUS_KM * la.sin(),
    ]
}

/// Centroid on the sphere (mean of unit vectors), correct across the antimeridian.
pub fn centroid(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    if points.is_empty() {
        return None;
    }
    let mut s = [0.0; 3];
    for p in points {
        let v = xyz(*p);
        for k in 0..3 {
            s[k] += v[k];
        }
    }
    let lon = s[1].atan2(s[0]).to_degrees();
    let lat = s[2].atan2((s[0] * s[0] + s[1] * s[1]).sqrt()).to_degrees();
    Some((lat, lon))
}

/// Standard base-32 geohash (used for stable place IDs).
pub fn geohash(lat: f64, lon: f64, len: usize) -> String {
    const B32: &[u8] = b"0123456789bcdefghjkmnpqrstuvwxyz";
    let (mut la, mut lo) = ((-90.0f64, 90.0f64), (-180.0f64, 180.0f64));
    let (mut out, mut bits, mut ch, mut even) = (String::new(), 0, 0usize, true);
    while out.len() < len {
        let (r, v) = if even { (&mut lo, lon) } else { (&mut la, lat) };
        let mid = (r.0 + r.1) / 2.0;
        ch <<= 1;
        if v >= mid {
            ch |= 1;
            r.0 = mid;
        } else {
            r.1 = mid;
        }
        even = !even;
        bits += 1;
        if bits == 5 {
            out.push(B32[ch] as char);
            bits = 0;
            ch = 0;
        }
    }
    out
}

/// DBSCAN over `points` with haversine `eps_km` and `min_pts` (the point itself counts).
/// Returns a cluster number per point, `None` for noise. Clusters are numbered in input order,
/// so time-sorted input gives stable numbers. Neighbour search uses a 3-D grid of cell size
/// `eps_km` on the sphere (chord ≤ arc, so no neighbour is missed, also at the poles and the
/// antimeridian): about O(n) for photo libraries.
pub fn dbscan(points: &[(f64, f64)], eps_km: f64, min_pts: usize) -> Vec<Option<usize>> {
    let n = points.len();
    let eps = eps_km.max(1e-6);
    let cell = |v: [f64; 3]| {
        (
            (v[0] / eps).floor() as i64,
            (v[1] / eps).floor() as i64,
            (v[2] / eps).floor() as i64,
        )
    };
    let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    let cells: Vec<_> = points.iter().map(|p| cell(xyz(*p))).collect();
    for (i, c) in cells.iter().enumerate() {
        grid.entry(*c).or_default().push(i);
    }
    let neighbours = |i: usize| -> Vec<usize> {
        let (cx, cy, cz) = cells[i];
        let mut out = vec![];
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(v) = grid.get(&(cx + dx, cy + dy, cz + dz)) {
                        out.extend(
                            v.iter()
                                .copied()
                                .filter(|&j| haversine_km(points[i], points[j]) <= eps),
                        );
                    }
                }
            }
        }
        out.sort_unstable();
        out
    };
    let mut label: Vec<Option<usize>> = vec![None; n];
    let mut visited = vec![false; n];
    let mut next = 0;
    for i in 0..n {
        if visited[i] {
            continue;
        }
        visited[i] = true;
        let nb = neighbours(i);
        if nb.len() < min_pts {
            continue; // noise for now; may become a border point later
        }
        let c = next;
        next += 1;
        label[i] = Some(c);
        let mut queue = nb;
        let mut q = 0;
        while q < queue.len() {
            let j = queue[q];
            q += 1;
            if label[j].is_none() {
                label[j] = Some(c);
            }
            if visited[j] {
                continue;
            }
            visited[j] = true;
            let nb2 = neighbours(j);
            if nb2.len() >= min_pts {
                queue.extend(nb2);
            }
        }
    }
    label
}

/// Location used for grouping: the item's own, or a hint from a located item taken within
/// [`HINT_WINDOW_MIN`] minutes (nearest in time). Undated items never get or give hints.
pub fn effective_locations(items: &[MediaItem]) -> Vec<Option<(f64, f64)>> {
    let mut located: Vec<(Timestamp, (f64, f64))> = items
        .iter()
        .filter(|i| !i.is_undated)
        .filter_map(|i| i.location.map(|l| (i.taken_at, (l.lat, l.lon))))
        .collect();
    located.sort_by_key(|a| a.0);
    let window = chrono::Duration::minutes(HINT_WINDOW_MIN);
    items
        .iter()
        .map(|i| {
            if let Some(l) = i.location {
                return Some((l.lat, l.lon));
            }
            if i.is_undated || located.is_empty() {
                return None;
            }
            let k = located.partition_point(|(t, _)| *t < i.taken_at);
            [k.checked_sub(1), Some(k)]
                .into_iter()
                .flatten()
                .filter_map(|j| located.get(j))
                .map(|(t, p)| ((*t - i.taken_at).abs(), *p))
                .filter(|(d, _)| *d <= window)
                .min_by_key(|(d, _)| *d)
                .map(|(_, p)| p)
        })
        .collect()
}

fn by_time(a: &MediaItem, b: &MediaItem) -> std::cmp::Ordering {
    a.taken_at
        .cmp(&b.taken_at)
        .then_with(|| a.path.cmp(&b.path))
}

fn label_of(place: &Option<Place>) -> String {
    place
        .as_ref()
        .map(Place::label)
        .unwrap_or_else(|| "Unnamed place".into())
}

/// Groups `items` in the chosen mode.
pub fn group_with(
    items: &[MediaItem],
    s: &GroupSettings,
    namer: &dyn PlaceNamer,
) -> Vec<VisitGroup> {
    match s.mode {
        GroupMode::Visit => group::group(items, s.gap_hours),
        GroupMode::Geo => group_geo(items, s, namer),
        GroupMode::VisitGeo => group_visit_geo(items, s, namer),
    }
}

/// A group of `members` (any order) at `place`, dated by its dated members.
fn place_group(
    id: String,
    members: &mut [MediaItem],
    place: Option<Place>,
    parent: Option<String>,
) -> VisitGroup {
    members.sort_by(|a, b| a.is_undated.cmp(&b.is_undated).then_with(|| by_time(a, b)));
    let folder = most_common_folder(members);
    let label = label_of(&place);
    let mut g = simple_group(id, label.clone(), members, folder);
    let dated: Vec<&MediaItem> = members.iter().filter(|i| !i.is_undated).collect();
    if let (Some(f), Some(l)) = (dated.first(), dated.last()) {
        g.start_date = f.taken_at;
        g.end_date = l.taken_at;
        g.title = title_for(&label, &f.taken_at, &l.taken_at);
    }
    g.place = place;
    g.parent_visit_id = parent;
    g
}

/// Undated members go to the part holding most of their folder siblings, else the first part.
fn attach_undated(parts: &mut [Vec<MediaItem>], undated: Vec<MediaItem>) {
    if parts.is_empty() {
        return;
    }
    for u in undated {
        let best = parts
            .iter()
            .enumerate()
            .map(|(k, p)| {
                let n = p
                    .iter()
                    .filter(|i| i.folder_path.is_some() && i.folder_path == u.folder_path)
                    .count();
                (n, std::cmp::Reverse(k))
            })
            .max()
            .map(|(_, std::cmp::Reverse(k))| k)
            .unwrap_or(0);
        parts[best].push(u);
    }
}

/// (name, admin1, country) of a place: equal keys share a `geo` group.
type PlaceKey = (String, String, String);

/// The key of a named place. Two towns with one name in different states or countries stay apart.
fn place_key(p: &Place) -> PlaceKey {
    (p.name.clone(), p.admin1.clone(), p.country.clone())
}

/// A stable group ID from a place key: `g-walla-walla-washington-us`.
fn place_id(k: &(String, String, String)) -> String {
    let mut id = String::from("g");
    for part in [&k.0, &k.1, &k.2] {
        let mut slug = String::new();
        for c in part.chars().flat_map(char::to_lowercase) {
            if c.is_alphanumeric() {
                slug.push(c);
            } else if !slug.ends_with('-') {
                slug.push('-');
            }
        }
        let slug = slug.trim_matches('-');
        if !slug.is_empty() {
            id.push('-');
            id.push_str(slug);
        }
    }
    id
}

/// `geo` (Rob, 25 Oct 2026, supersedes DBSCAN): one group per reverse-geocoded place name, no
/// matter the date, no trips, no distance clustering. Items without a name (more than 100 km
/// from any town) go to "Other places"; items without a location (after the neighbour hint)
/// go to one "No location" group. Items in a group are in time order; places newest first.
fn group_geo(items: &[MediaItem], _s: &GroupSettings, namer: &dyn PlaceNamer) -> Vec<VisitGroup> {
    let mut sorted: Vec<MediaItem> = items.to_vec();
    sorted.sort_by(by_time);
    let eff = effective_locations(&sorted);
    let mut places: BTreeMap<(String, String, String), (Place, Vec<MediaItem>)> = BTreeMap::new();
    let mut other = vec![];
    let mut unlocated = vec![];
    for (i, e) in sorted.into_iter().zip(eff) {
        match e.map(|p| namer.place(p.0, p.1)) {
            Some(Some(pl)) => places
                .entry(place_key(&pl))
                .or_insert_with(|| (pl, vec![]))
                .1
                .push(i),
            Some(None) => other.push(i),
            None => unlocated.push(i),
        }
    }
    // Labels shared by two keys ("Springfield, PT" twice) get the region too.
    let mut label_count: HashMap<String, usize> = HashMap::new();
    for (pl, _) in places.values() {
        *label_count.entry(pl.label()).or_default() += 1;
    }
    let mut named: Vec<(Option<Timestamp>, VisitGroup)> = places
        .into_iter()
        .map(|(k, (pl, mut members))| {
            let newest = members
                .iter()
                .filter(|i| !i.is_undated)
                .map(|i| i.taken_at)
                .max();
            let mut g = place_group(place_id(&k), &mut members, Some(pl.clone()), None);
            if label_count[&pl.label()] > 1 {
                let label = format!("{}, {}, {}", pl.name, pl.admin1, pl.country);
                g.title = if members.iter().any(|i| !i.is_undated) {
                    title_for(&label, &g.start_date, &g.end_date)
                } else {
                    label
                };
            }
            (newest, g)
        })
        .collect();
    // Newest place first (ties by ID for stability).
    named.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    let mut result: Vec<VisitGroup> = named.into_iter().map(|(_, g)| g).collect();
    if !other.is_empty() {
        let mut g = place_group(OTHER_PLACES_ID.into(), &mut other, None, None);
        g.title = "Other places".into();
        result.push(g);
    }
    if !unlocated.is_empty() {
        let mut g = place_group(NO_LOCATION_ID.into(), &mut unlocated, None, None);
        g.title = "No location".into();
        result.push(g);
    }
    result
}

/// `visit-geo`: a detour with this many items that never returns to the current place is a
/// place of its own, even when it is shorter than `min_items` items or [`PLACE_CHANGE_MIN`].
pub const MOVE_ON_ITEMS: usize = 2;

/// `visit-geo` split of one visit's dated, located members (time order) by place name (M3b,
/// same names as `geo`): indices per place. A change of place needs `min_items` items or
/// [`PLACE_CHANGE_MIN`] minutes at another name; shorter detours (stray fixes) stay in the
/// current place, unless [`MOVE_ON_ITEMS`] of them move on and never come back. Parts with the
/// same name merge (left and came back).
fn split_by_place<K: PartialEq>(pts: &[(Timestamp, K)], min_items: usize) -> Vec<Vec<usize>> {
    let mut parts: Vec<Vec<usize>> = vec![];
    let mut cur: Vec<usize> = vec![];
    let mut pending: Vec<usize> = vec![];
    let change = chrono::Duration::minutes(PLACE_CHANGE_MIN);
    for k in 0..pts.len() {
        if cur.first().is_none_or(|&c| pts[c].1 == pts[k].1) {
            cur.append(&mut pending);
            cur.push(k);
            continue;
        }
        if pending.first().is_some_and(|&p| pts[p].1 != pts[k].1) {
            if pending.len() >= MOVE_ON_ITEMS {
                // Moved on without coming back: the detour was a place of its own.
                parts.push(std::mem::take(&mut cur));
                cur = std::mem::take(&mut pending);
                if pts[cur[0]].1 == pts[k].1 {
                    cur.push(k);
                    continue;
                }
            } else {
                // A different detour: the earlier one was a stray.
                cur.append(&mut pending);
            }
        }
        pending.push(k);
        let span = pts[*pending.last().unwrap()].0 - pts[pending[0]].0;
        if pending.len() >= min_items || span >= change {
            parts.push(std::mem::take(&mut cur));
            cur = std::mem::take(&mut pending);
        }
    }
    // The visit ended at another place: 2+ items there are a place of their own.
    if pending.len() >= MOVE_ON_ITEMS && !cur.is_empty() {
        parts.push(std::mem::take(&mut cur));
    }
    cur.append(&mut pending);
    if !cur.is_empty() {
        parts.push(cur);
    }
    // Merge parts with the same place (left and came back); a part's place is its first item's.
    let mut merged: Vec<Vec<usize>> = vec![];
    for p in parts {
        match merged.iter_mut().find(|m| pts[m[0]].1 == pts[p[0]].1) {
            Some(m) => m.extend(p),
            None => merged.push(p),
        }
    }
    for p in &mut merged {
        p.sort_unstable();
    }
    merged.sort_by_key(|p| p[0]);
    merged
}

fn group_visit_geo(
    items: &[MediaItem],
    s: &GroupSettings,
    namer: &dyn PlaceNamer,
) -> Vec<VisitGroup> {
    let by_path: HashMap<&str, &MediaItem> = items.iter().map(|i| (i.path.as_str(), i)).collect();
    let mut sorted: Vec<MediaItem> = items.to_vec();
    sorted.sort_by(by_time);
    let eff: HashMap<String, (f64, f64)> = sorted
        .iter()
        .zip(effective_locations(&sorted))
        .filter_map(|(i, e)| e.map(|p| (i.path.clone(), p)))
        .collect();
    let members_of = |g: &VisitGroup| -> Vec<MediaItem> {
        g.item_paths
            .iter()
            .filter_map(|p| by_path.get(p.as_str()).map(|i| (*i).clone()))
            .collect()
    };
    let visit_centre = |m: &[MediaItem]| {
        centroid(
            &m.iter()
                .filter_map(|i| eff.get(&i.path).copied())
                .collect::<Vec<_>>(),
        )
    };

    // One geocoder lookup per located item; the same names as `geo` (M3b).
    let names: HashMap<String, Option<Place>> = eff
        .iter()
        .map(|(path, p)| (path.clone(), namer.place(p.0, p.1)))
        .collect();
    let mut visits = group::group(items, s.gap_hours);
    if s.join_same_place {
        visits = join_same_place(visits, &members_of, &visit_centre, s.radius_km);
    }

    let mut result = vec![];
    for v in visits {
        if !v.id.starts_with("visit-") {
            result.push(v); // folder and Undated groups stay as they are
            continue;
        }
        let mut members = members_of(&v);
        members.sort_by(by_time);
        let (dated, undated): (Vec<MediaItem>, Vec<MediaItem>) =
            members.into_iter().partition(|i| !i.is_undated);
        let located: Vec<(Timestamp, Option<PlaceKey>)> = dated
            .iter()
            .filter_map(|i| {
                eff.get(&i.path).map(|_| {
                    (
                        i.taken_at,
                        names.get(&i.path).and_then(|n| n.as_ref().map(place_key)),
                    )
                })
            })
            .collect();
        if located.is_empty() {
            result.push(v);
            continue;
        }
        let located_items: Vec<&MediaItem> =
            dated.iter().filter(|i| eff.contains_key(&i.path)).collect();
        let split = split_by_place(&located, s.min_items.max(1));
        let mut parts: Vec<Vec<MediaItem>> = split
            .iter()
            .map(|p| p.iter().map(|&k| located_items[k].clone()).collect())
            .collect();
        // Dated items without any location: the part with the member nearest in time.
        for i in dated.iter().filter(|i| !eff.contains_key(&i.path)) {
            let best = parts
                .iter()
                .enumerate()
                .map(|(k, p)| {
                    let d = p
                        .iter()
                        .map(|m| (m.taken_at - i.taken_at).abs())
                        .min()
                        .unwrap_or(chrono::Duration::MAX);
                    (d, k)
                })
                .min()
                .map(|(_, k)| k)
                .unwrap_or(0);
            parts[best].push(i.clone());
        }
        attach_undated(&mut parts, undated);
        // A part's place: the most common name among its members (ties: the earliest).
        let name = |p: &[MediaItem]| -> Option<Place> {
            let mut counts: Vec<(Place, usize)> = vec![];
            for pl in p
                .iter()
                .filter_map(|i| names.get(&i.path).cloned().flatten())
            {
                match counts
                    .iter_mut()
                    .find(|(c, _)| place_key(c) == place_key(&pl))
                {
                    Some((_, n)) => *n += 1,
                    None => counts.push((pl, 1)),
                }
            }
            let best = counts.iter().map(|(_, n)| *n).max()?;
            counts
                .into_iter()
                .find(|(_, n)| *n == best)
                .map(|(pl, _)| pl)
        };
        if parts.len() == 1 {
            let mut g = v.clone();
            let place = name(&parts[0]);
            g.title = title_for(&label_of(&place), &g.start_date, &g.end_date);
            g.place = place;
            result.push(g);
            continue;
        }
        for (n, mut part) in parts.into_iter().enumerate() {
            let place = name(&part);
            let mut g = place_group(
                format!("{}-p{}", v.id, n + 1),
                &mut part,
                place,
                Some(v.id.clone()),
            );
            g.folder_hint = most_common_folder(&part);
            result.push(g);
        }
    }
    result
}

type MembersOf<'a> = dyn Fn(&VisitGroup) -> Vec<MediaItem> + 'a;
type CentreOf<'a> = dyn Fn(&[MediaItem]) -> Option<(f64, f64)> + 'a;

/// Joins time-adjacent visits whose place centres are within `radius_km` and whose gap is under
/// [`JOIN_GAP_HOURS`] (a weekend at one campsite). Joined visits get the ID of the merged set.
fn join_same_place(
    visits: Vec<VisitGroup>,
    members_of: &MembersOf,
    centre: &CentreOf,
    radius_km: f64,
) -> Vec<VisitGroup> {
    let (mut timed, rest): (Vec<VisitGroup>, Vec<VisitGroup>) =
        visits.into_iter().partition(|v| v.id.starts_with("visit-"));
    timed.sort_by_key(|a| a.start_date);
    let mut out: Vec<Vec<MediaItem>> = vec![];
    for v in timed {
        let m = members_of(&v);
        if let Some(last) = out.last_mut() {
            let gap_h = last
                .iter()
                .filter(|i| !i.is_undated)
                .map(|i| i.taken_at)
                .max()
                .map(|e| (v.start_date - e).num_seconds() as f64 / 3600.0);
            let close = match (centre(last), centre(&m)) {
                (Some(a), Some(b)) => haversine_km(a, b) <= radius_km,
                _ => false,
            };
            if close && gap_h.is_some_and(|g| g < JOIN_GAP_HOURS) {
                last.extend(m);
                continue;
            }
        }
        out.push(m);
    }
    let mut result: Vec<VisitGroup> = out
        .into_iter()
        .map(|mut m| {
            m.sort_by(|a, b| a.is_undated.cmp(&b.is_undated).then_with(|| by_time(a, b)));
            make_visit(&m)
        })
        .collect();
    result.sort_by_key(|g| std::cmp::Reverse(g.total_bytes));
    result.extend(rest);
    result
}

/// Two-letter code of a US state (GeoNames admin1 name), for "Bend, OR" titles.
pub fn us_state_code(admin1: &str) -> Option<&'static str> {
    const S: &[(&str, &str)] = &[
        ("Alabama", "AL"),
        ("Alaska", "AK"),
        ("Arizona", "AZ"),
        ("Arkansas", "AR"),
        ("California", "CA"),
        ("Colorado", "CO"),
        ("Connecticut", "CT"),
        ("Delaware", "DE"),
        ("District of Columbia", "DC"),
        ("Washington, D.C.", "DC"),
        ("Florida", "FL"),
        ("Georgia", "GA"),
        ("Hawaii", "HI"),
        ("Idaho", "ID"),
        ("Illinois", "IL"),
        ("Indiana", "IN"),
        ("Iowa", "IA"),
        ("Kansas", "KS"),
        ("Kentucky", "KY"),
        ("Louisiana", "LA"),
        ("Maine", "ME"),
        ("Maryland", "MD"),
        ("Massachusetts", "MA"),
        ("Michigan", "MI"),
        ("Minnesota", "MN"),
        ("Mississippi", "MS"),
        ("Missouri", "MO"),
        ("Montana", "MT"),
        ("Nebraska", "NE"),
        ("Nevada", "NV"),
        ("New Hampshire", "NH"),
        ("New Jersey", "NJ"),
        ("New Mexico", "NM"),
        ("New York", "NY"),
        ("North Carolina", "NC"),
        ("North Dakota", "ND"),
        ("Ohio", "OH"),
        ("Oklahoma", "OK"),
        ("Oregon", "OR"),
        ("Pennsylvania", "PA"),
        ("Rhode Island", "RI"),
        ("South Carolina", "SC"),
        ("South Dakota", "SD"),
        ("Tennessee", "TN"),
        ("Texas", "TX"),
        ("Utah", "UT"),
        ("Vermont", "VT"),
        ("Virginia", "VA"),
        ("Washington", "WA"),
        ("West Virginia", "WV"),
        ("Wisconsin", "WI"),
        ("Wyoming", "WY"),
    ];
    S.iter().find(|(n, _)| *n == admin1).map(|(_, c)| *c)
}

#[cfg(test)]
mod tests;
