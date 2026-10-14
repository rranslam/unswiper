// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Port of `VisitGrouper` (LocalTreeIndexer.swift).
//!
//! 1. Items with a capture or folder date are clustered by time gap (largest visits first).
//! 2. Undated items in a sub-folder join the visit holding most of the dated files in the same
//!    folder; if that folder has no dated files they form a visit named after the folder.
//! 3. Only undated items directly in the library root (or from pre-0.1.2 indexes) go to "Undated".

use crate::model::{MediaItem, MediaKind, VisitGroup};
use crate::time::{distant_past, Timestamp};
use chrono::Local;
use serde_json::Map;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

pub const UNDATED_ID: &str = "undated";

/// 64-bit FNV-1a over UTF-8 bytes, lowercase hex without padding (Swift `String(hash, radix: 16)`).
pub fn fnv1a(s: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:x}")
}

/// Stable across commits/re-index: FNV-1a of first item path + start time (whole seconds,
/// truncated toward zero like Swift's `Int64(timeIntervalSince1970)`).
pub fn stable_group_id(first_path: &str, start: &Timestamp) -> String {
    format!(
        "visit-{}",
        fnv1a(&format!("{first_path}|{}", start.timestamp()))
    )
}

fn by_time(a: &MediaItem, b: &MediaItem) -> Ordering {
    a.taken_at
        .cmp(&b.taken_at)
        .then_with(|| a.path.cmp(&b.path))
}

/// Splits time-sorted items wherever the gap exceeds `gap_hours`.
pub fn cluster(items: &[MediaItem], gap_hours: f64) -> Vec<Vec<MediaItem>> {
    let Some(first) = items.first() else {
        return vec![];
    };
    let gap_ms = (gap_hours * 3_600_000.0).round() as i64;
    let mut clusters = vec![];
    let mut current = vec![first.clone()];
    for item in &items[1..] {
        let last = current.last().unwrap();
        if (item.taken_at - last.taken_at).num_milliseconds() > gap_ms {
            clusters.push(std::mem::take(&mut current));
        }
        current.push(item.clone());
    }
    clusters.push(current);
    clusters
}

/// Groups items into visits (see module docs).
pub fn group(items: &[MediaItem], gap_hours: f64) -> Vec<VisitGroup> {
    let undated_all: Vec<&MediaItem> = items.iter().filter(|i| i.is_undated).collect();
    let mut dated: Vec<MediaItem> = items.iter().filter(|i| !i.is_undated).cloned().collect();
    dated.sort_by(by_time);
    let mut clusters = cluster(&dated, gap_hours);

    let mut cluster_of: HashMap<&str, usize> = HashMap::new();
    for (i, c) in clusters.iter().enumerate() {
        for item in c {
            cluster_of.insert(item.path.as_str(), i);
        }
    }
    let mut dated_by_folder: HashMap<&str, BTreeMap<usize, usize>> = HashMap::new();
    for item in &dated {
        if let (Some(folder), Some(ci)) = (
            item.folder_path.as_deref(),
            cluster_of.get(item.path.as_str()),
        ) {
            *dated_by_folder
                .entry(folder)
                .or_default()
                .entry(*ci)
                .or_default() += 1;
        }
    }

    let mut root_undated: Vec<MediaItem> = vec![];
    let mut named: BTreeMap<String, Vec<MediaItem>> = BTreeMap::new();
    let mut joined: BTreeMap<usize, Vec<MediaItem>> = BTreeMap::new();
    for item in undated_all {
        match item.folder_path.as_deref() {
            None | Some("") => root_undated.push(item.clone()),
            Some(folder) => {
                // Most dated siblings; ties go to the earliest visit.
                let best = dated_by_folder.get(folder).and_then(|counts| {
                    counts
                        .iter()
                        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                        .map(|(k, _)| *k)
                });
                match best {
                    Some(ci) => joined.entry(ci).or_default().push(item.clone()),
                    None => named
                        .entry(folder.to_string())
                        .or_default()
                        .push(item.clone()),
                }
            }
        }
    }
    for (ci, extra) in joined {
        clusters[ci].extend(extra);
    }

    let mut result: Vec<VisitGroup> = clusters.iter().map(|c| make_visit(c)).collect();
    result.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));

    let mut folders: Vec<String> = named.keys().cloned().collect();
    folders.sort_by(|a, b| natural_cmp(a, b));
    for folder in folders {
        let members = &named[&folder];
        let name = folder
            .rsplit('/')
            .find(|s| !s.is_empty())
            .unwrap_or(&folder)
            .to_string();
        result.push(simple_group(
            format!("folder-{}", fnv1a(&folder)),
            name,
            members,
            folder.clone(),
        ));
    }

    if !root_undated.is_empty() {
        let hint = most_common_folder(&root_undated);
        result.push(simple_group(
            UNDATED_ID.into(),
            "Undated".into(),
            &root_undated,
            hint,
        ));
    }
    result
}

fn counts(members: &[MediaItem]) -> (i64, i64, i64) {
    let photos = members
        .iter()
        .filter(|i| i.kind == MediaKind::Photo)
        .count() as i64;
    let videos = members.len() as i64 - photos;
    let bytes = members.iter().map(MediaItem::total_bytes).sum();
    (photos, videos, bytes)
}

fn simple_group(id: String, title: String, members: &[MediaItem], hint: String) -> VisitGroup {
    let (photo_count, video_count, total_bytes) = counts(members);
    VisitGroup {
        id,
        title,
        start_date: distant_past(),
        end_date: distant_past(),
        item_paths: members.iter().map(|i| i.path.clone()).collect(),
        photo_count,
        video_count,
        total_bytes,
        folder_hint: hint,
        extra: Map::new(),
    }
}

/// Builds a visit from a cluster: dated items (time order) first; undated folder-siblings
/// appended at the end don't affect dates or the ID.
fn make_visit(cluster: &[MediaItem]) -> VisitGroup {
    let dated: Vec<&MediaItem> = cluster.iter().filter(|i| !i.is_undated).collect();
    let start = dated.first().expect("cluster has a dated item").taken_at;
    let end = dated.last().unwrap().taken_at;
    let folder = most_common_folder(cluster);
    let mut g = simple_group(
        stable_group_id(&dated[0].path, &start),
        title_for(&folder, &start, &end),
        cluster,
        folder,
    );
    g.start_date = start;
    g.end_date = end;
    g
}

/// Most frequent `relative_folder`; ties go to the alphabetically first name. "Library" if empty.
pub fn most_common_folder(items: &[MediaItem]) -> String {
    let mut c: BTreeMap<&str, usize> = BTreeMap::new();
    for i in items {
        *c.entry(i.relative_folder.as_str()).or_default() += 1;
    }
    c.iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(k, _)| k.to_string())
        .unwrap_or_else(|| "Library".into())
}

/// `Jan 5 · Folder` or `Jan 5–Jan 7 · Folder` (local time, en-US month names).
pub fn title_for(folder: &str, start: &Timestamp, end: &Timestamp) -> String {
    let s = start.with_timezone(&Local);
    let e = end.with_timezone(&Local);
    let start_s = s.format("%b %-d").to_string();
    if s.date_naive() == e.date_naive() {
        format!("{start_s} · {folder}")
    } else {
        format!("{start_s}–{} · {folder}", e.format("%b %-d"))
    }
}

/// Approximation of Finder's `localizedStandardCompare`: case-insensitive, digit runs compared
/// numerically.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n1 = String::new();
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    n1.push(c);
                    x.next();
                }
                let mut n2 = String::new();
                while let Some(c) = y.peek().copied().filter(char::is_ascii_digit) {
                    n2.push(c);
                    y.next();
                }
                let t1 = n1.trim_start_matches('0');
                let t2 = n2.trim_start_matches('0');
                let o = t1.len().cmp(&t2.len()).then_with(|| t1.cmp(t2));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                let o = c.to_lowercase().cmp(d.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DateSource, Decision};
    use chrono::{Duration, TimeZone, Utc};

    pub(crate) fn it(path: &str, t: Option<Timestamp>, folder: &str, bytes: i64) -> MediaItem {
        MediaItem {
            path: path.into(),
            kind: if path.ends_with(".mov") {
                MediaKind::Video
            } else {
                MediaKind::Photo
            },
            size_bytes: bytes,
            taken_at: t.unwrap_or_else(distant_past),
            relative_folder: folder
                .split('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("root")
                .into(),
            decision: Decision::Undecided,
            modified_at: None,
            sidecars: vec![],
            is_undated: t.is_none(),
            date_source: Some(if t.is_some() {
                DateSource::Capture
            } else {
                DateSource::Unknown
            }),
            folder_path: Some(folder.into()),
            extra: Map::new(),
        }
    }

    fn t0() -> Timestamp {
        Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
    }

    #[test]
    fn fnv_known_vectors() {
        assert_eq!(fnv1a(""), "cbf29ce484222325");
        assert_eq!(fnv1a("a"), "af63dc4c8601ec8c");
        assert_eq!(fnv1a("foobar"), "85944171f73967e8");
        // Leading zeros are not padded (Swift radix-16).
        assert_eq!(
            stable_group_id("/p/a.jpg", &Utc.timestamp_opt(0, 0).unwrap()),
            format!("visit-{}", fnv1a("/p/a.jpg|0"))
        );
        assert_eq!(
            stable_group_id("/p/a.jpg", &Utc.timestamp_opt(-1, 500_000_000).unwrap()),
            format!("visit-{}", fnv1a("/p/a.jpg|-1"))
        );
    }

    #[test]
    fn empty_and_single() {
        assert!(group(&[], 36.0).is_empty());
        let g = group(&[it("/r/a/1.jpg", Some(t0()), "a", 5)], 36.0);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].id, stable_group_id("/r/a/1.jpg", &t0()));
        assert_eq!(g[0].photo_count, 1);
        assert_eq!(g[0].folder_hint, "a");
    }

    #[test]
    fn gap_boundary() {
        let a = it("/r/a/1.jpg", Some(t0()), "a", 1);
        let b = it("/r/a/2.jpg", Some(t0() + Duration::hours(36)), "a", 1);
        assert_eq!(group(&[a.clone(), b], 36.0).len(), 1);
        let c = it(
            "/r/a/2.jpg",
            Some(t0() + Duration::hours(36) + Duration::seconds(1)),
            "a",
            1,
        );
        assert_eq!(group(&[a.clone(), c.clone()], 36.0).len(), 2);
        assert_eq!(group(&[a, c], 48.0).len(), 1);
    }

    #[test]
    fn ordering_by_bytes_and_undated_last() {
        let items = vec![
            it("/r/a/1.jpg", Some(t0()), "a", 1),
            it("/r/b/2.jpg", Some(t0() + Duration::days(10)), "b", 100),
            it("/r/x.jpg", None, "", 1000),
        ];
        let g = group(&items, 36.0);
        assert_eq!(g.len(), 3);
        assert_eq!(g[0].item_paths, vec!["/r/b/2.jpg"]);
        assert_eq!(g[1].item_paths, vec!["/r/a/1.jpg"]);
        assert_eq!(g[2].id, UNDATED_ID);
        assert_eq!(g[2].title, "Undated");
        assert_eq!(g[2].start_date, distant_past());
    }

    #[test]
    fn undated_joins_sibling_visit_or_folder_group() {
        let items = vec![
            it("/r/Lisbon/1.jpg", Some(t0()), "Lisbon", 10),
            it(
                "/r/Lisbon/2.jpg",
                Some(t0() + Duration::hours(1)),
                "Lisbon",
                10,
            ),
            it(
                "/r/Lisbon/3.jpg",
                Some(t0() + Duration::days(30)),
                "Lisbon",
                10,
            ),
            it("/r/Lisbon/nodate.jpg", None, "Lisbon", 10),
            it("/r/Porto/old/x.jpg", None, "Porto/old", 7),
            it("/r/Porto/old/y.mov", None, "Porto/old", 7),
            it("/r/Album 10/z.jpg", None, "Album 10", 1),
            it("/r/Album 9/z.jpg", None, "Album 9", 1),
        ];
        let g = group(&items, 36.0);
        let lisbon = g
            .iter()
            .find(|g| g.item_paths.contains(&"/r/Lisbon/nodate.jpg".to_string()))
            .unwrap();
        assert_eq!(lisbon.item_paths.len(), 3);
        assert_eq!(lisbon.id, stable_group_id("/r/Lisbon/1.jpg", &t0()));
        assert_eq!(lisbon.end_date, t0() + Duration::hours(1));
        let tail: Vec<&str> = g[g.len() - 3..].iter().map(|g| g.title.as_str()).collect();
        assert_eq!(tail, vec!["Album 9", "Album 10", "old"]);
        let porto = &g[g.len() - 1];
        assert_eq!(porto.id, format!("folder-{}", fnv1a("Porto/old")));
        assert_eq!(
            (porto.photo_count, porto.video_count, porto.total_bytes),
            (1, 1, 14)
        );
        assert_eq!(porto.folder_hint, "Porto/old");
        assert!(g.iter().all(|g| g.id != UNDATED_ID));
    }

    #[test]
    fn sibling_tie_goes_to_earliest_visit() {
        let items = vec![
            it("/r/L/1.jpg", Some(t0()), "L", 1),
            it("/r/L/2.jpg", Some(t0() + Duration::days(9)), "L", 1),
            it("/r/L/n.jpg", None, "L", 1),
        ];
        let g = group(&items, 36.0);
        let host = g
            .iter()
            .find(|g| g.item_paths.contains(&"/r/L/n.jpg".to_string()))
            .unwrap();
        assert!(host.item_paths.contains(&"/r/L/1.jpg".to_string()));
    }

    #[test]
    fn legacy_items_without_folder_go_undated() {
        let mut a = it("/r/a/1.jpg", None, "a", 1);
        a.folder_path = None;
        let g = group(&[a], 36.0);
        assert_eq!(g[0].id, UNDATED_ID);
    }

    #[test]
    fn titles() {
        let s = Utc
            .with_ymd_and_hms(2026, 1, 5, 12, 0, 0)
            .unwrap()
            .with_timezone(&Local);
        let start = s.with_timezone(&Utc);
        let want = format!("{} · Trip", s.format("%b %-d"));
        assert_eq!(title_for("Trip", &start, &start), want);
        let end = start + Duration::days(2);
        let t = title_for("Trip", &start, &end);
        assert!(t.contains('–') && t.ends_with(" · Trip"), "{t}");
    }

    #[test]
    fn most_common_folder_ties_and_empty() {
        assert_eq!(most_common_folder(&[]), "Library");
        let items = vec![it("/1", Some(t0()), "b", 1), it("/2", Some(t0()), "a", 1)];
        assert_eq!(most_common_folder(&items), "a");
        let items = vec![
            it("/1", Some(t0()), "b", 1),
            it("/2", Some(t0()), "b", 1),
            it("/3", Some(t0()), "a", 1),
        ];
        assert_eq!(most_common_folder(&items), "b");
    }

    #[test]
    fn natural() {
        assert_eq!(natural_cmp("a2", "a10"), Ordering::Less);
        assert_eq!(natural_cmp("B", "a"), Ordering::Greater);
        assert_eq!(natural_cmp("a", "ab"), Ordering::Less);
        assert_eq!(natural_cmp("ab", "a"), Ordering::Greater);
        assert_eq!(natural_cmp("a01", "a1"), Ordering::Less);
        assert_eq!(natural_cmp("x", "x"), Ordering::Equal);
    }

    mod props {
        use super::*;
        use proptest::prelude::*;

        fn items_strategy() -> impl Strategy<Value = Vec<MediaItem>> {
            prop::collection::vec((0i64..2_000_000, 1i64..1000), 0..40).prop_map(|v| {
                v.into_iter()
                    .enumerate()
                    .map(|(i, (secs, b))| {
                        it(
                            &format!("/r/f/{i}.jpg"),
                            Some(t0() + Duration::seconds(secs * 60)),
                            "f",
                            b,
                        )
                    })
                    .collect()
            })
        }

        proptest! {
            #[test]
            fn invariants(items in items_strategy(), gap in 1.0f64..100.0, seed in any::<u64>()) {
                let groups = group(&items, gap);
                let mut seen = std::collections::HashSet::new();
                for g in &groups {
                    for p in &g.item_paths { prop_assert!(seen.insert(p.clone())); }
                }
                prop_assert_eq!(seen.len(), items.len());
                let ids: std::collections::HashSet<_> = groups.iter().map(|g| g.id.clone()).collect();
                prop_assert_eq!(ids.len(), groups.len());
                let by_path: HashMap<&str, &MediaItem> = items.iter().map(|i| (i.path.as_str(), i)).collect();
                let gap_ms = (gap * 3_600_000.0).round() as i64;
                for g in &groups {
                    let ts: Vec<_> = g.item_paths.iter().map(|p| by_path[p.as_str()].taken_at).collect();
                    for w in ts.windows(2) { prop_assert!((w[1] - w[0]).num_milliseconds() <= gap_ms); }
                }
                let mut starts: Vec<_> = groups.iter().map(|g| (g.start_date, g.end_date)).collect();
                starts.sort();
                for w in starts.windows(2) { prop_assert!((w[1].0 - w[0].1).num_milliseconds() > gap_ms); }
                // Input order doesn't matter.
                let mut shuffled = items.clone();
                let n = shuffled.len();
                if n > 1 { for i in 0..n { shuffled.swap(i, (seed as usize).wrapping_add(i * 7) % n); } }
                prop_assert_eq!(group(&shuffled, gap), groups);
            }
        }
    }
}
