// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Cross-check against the Swift app (plan §10.1 ID table, §10.5 library.json bytes).
//!
//! `fixtures/crosscheck/swift/` is written on a Mac by `CrossCheckFixtureTests.swift`
//! (see `fixtures/crosscheck/README.md`). When it's missing, the real comparison is skipped;
//! the same comparison code is still exercised against Rust-generated output.

mod common;
use chrono::{DateTime, Utc};
use common::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use unswiper_core::group::{fnv1a, stable_group_id};
use unswiper_core::index::{index, MetadataDates};
use unswiper_core::{apple_json, AppConfig, LibrarySnapshot, MediaItem, VisitGroup};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    gap_hours: f64,
    files: Vec<SpecFile>,
}

#[derive(Deserialize)]
struct SpecFile {
    path: String,
    exif: Option<String>,
    tiff: Option<String>,
    bytes: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct TItem {
    path: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    date_source: Option<String>,
    taken_at: String,
    is_undated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    folder_path: Option<String>,
    relative_folder: String,
    sidecars: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct TGroup {
    id: String,
    title: String,
    start_date: String,
    end_date: String,
    item_paths: Vec<String>,
    folder_hint: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct THash {
    input: String,
    hash: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Table {
    root: String,
    time_zone: String,
    locale: String,
    gap_hours: f64,
    items: Vec<TItem>,
    groups: Vec<TGroup>,
    fnv: Vec<THash>,
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/crosscheck")
}

fn spec() -> Spec {
    serde_json::from_slice(&std::fs::read(fixtures().join("spec.json")).unwrap()).unwrap()
}

/// Builds the spec tree under a fresh temp folder and indexes it with the Rust core.
fn rust_index(spec: &Spec) -> (tempfile::TempDir, PathBuf, Vec<MediaItem>, Vec<VisitGroup>) {
    let d = tempfile::tempdir().unwrap();
    let root = canon(d.path()).join("library");
    std::fs::create_dir(&root).unwrap();
    for f in &spec.files {
        if f.exif.is_some() || f.tiff.is_some() {
            write(
                &root,
                &f.path,
                &jpeg_with_exif(f.exif.as_deref(), f.tiff.as_deref()),
            );
        } else {
            write(&root, &f.path, &vec![0x5A; f.bytes.unwrap_or(16)]);
        }
    }
    let (items, groups) = index(
        &root,
        spec.gap_hours,
        &HashMap::new(),
        &MetadataDates,
        |_| {},
    )
    .unwrap();
    (d, root, items, groups)
}

fn rel(root: &str, p: &str) -> String {
    p.strip_prefix(&format!("{root}/")).unwrap_or(p).to_string()
}

fn iso(t: &DateTime<Utc>) -> String {
    unswiper_core::time::format_iso(t)
}

fn table_from(root: &Path, items: &[MediaItem], groups: &[VisitGroup], gap: f64) -> Table {
    let r = root.to_string_lossy().into_owned();
    Table {
        root: r.clone(),
        time_zone: std::env::var("TZ").unwrap_or_default(),
        locale: "en_US".into(),
        gap_hours: gap,
        items: items
            .iter()
            .map(|i| TItem {
                path: rel(&r, &i.path),
                kind: serde_json::to_value(i.kind)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .into(),
                date_source: i
                    .date_source
                    .map(|d| serde_json::to_value(d).unwrap().as_str().unwrap().into()),
                taken_at: iso(&i.taken_at),
                is_undated: i.is_undated,
                folder_path: i.folder_path.clone(),
                relative_folder: i.relative_folder.clone(),
                sidecars: i.sidecars.iter().map(|s| rel(&r, &s.path)).collect(),
            })
            .collect(),
        groups: groups
            .iter()
            .map(|g| TGroup {
                id: g.id.clone(),
                title: g.title.clone(),
                start_date: iso(&g.start_date),
                end_date: iso(&g.end_date),
                item_paths: g.item_paths.iter().map(|p| rel(&r, p)).collect(),
                folder_hint: g.folder_hint.clone(),
            })
            .collect(),
        fnv: ["", "a", "foobar", "Porto/old"]
            .iter()
            .map(|s| THash {
                input: s.to_string(),
                hash: fnv1a(s),
            })
            .collect(),
    }
}

/// Compares the oracle in `dir` (library.json + table.json) with the Rust core.
/// Returns a list of mismatches (empty = match).
fn compare(dir: &Path) -> Vec<String> {
    let mut bad = vec![];
    // 1. library.json: decode + re-encode must give identical bytes.
    let lib = std::fs::read_to_string(dir.join("library.json")).unwrap();
    let snap: LibrarySnapshot = serde_json::from_str(&lib).unwrap();
    if snap.to_json() != lib {
        let a: Vec<&str> = lib.lines().collect();
        let enc = snap.to_json();
        let b: Vec<&str> = enc.lines().collect();
        let first = a
            .iter()
            .zip(&b)
            .position(|(x, y)| x != y)
            .unwrap_or(a.len().min(b.len()));
        bad.push(format!(
            "library.json re-encode differs at line {}: swift {:?} vs rust {:?}",
            first + 1,
            a.get(first),
            b.get(first)
        ));
    }

    let table: Table =
        serde_json::from_slice(&std::fs::read(dir.join("table.json")).unwrap()).unwrap();
    // 2. FNV-1a samples.
    for h in &table.fnv {
        if fnv1a(&h.input) != h.hash {
            bad.push(format!(
                "fnv1a({:?}) swift {} rust {}",
                h.input,
                h.hash,
                fnv1a(&h.input)
            ));
        }
    }
    // 3. Swift's own visit IDs follow the documented formula.
    for g in &table.groups {
        if g.id.starts_with("visit-") {
            let start = unswiper_core::time::parse_iso(&g.start_date).unwrap();
            let want = stable_group_id(&format!("{}/{}", table.root, g.item_paths[0]), &start);
            if want != g.id {
                bad.push(format!("visit id formula: swift {} expected {want}", g.id));
            }
        }
    }

    // 4. Same tree indexed by Rust (dates are local time: needs the same zone).
    let tz = std::env::var("TZ").unwrap_or_default();
    if tz != table.time_zone {
        eprintln!(
            "crosscheck: TZ={tz:?} but oracle used {:?}; skipping date/grouping comparison",
            table.time_zone
        );
        return bad;
    }
    let (_d, root, items, groups) = rust_index(&spec());
    let mine = table_from(&root, &items, &groups, table.gap_hours);
    let theirs: BTreeMap<&str, &TItem> = table.items.iter().map(|i| (i.path.as_str(), i)).collect();
    let ours: BTreeMap<&str, &TItem> = mine.items.iter().map(|i| (i.path.as_str(), i)).collect();
    if theirs.keys().collect::<Vec<_>>() != ours.keys().collect::<Vec<_>>() {
        bad.push(format!(
            "item paths differ: swift {:?} rust {:?}",
            theirs.keys(),
            ours.keys()
        ));
    }
    for (p, t) in &theirs {
        let Some(o) = ours.get(p) else { continue };
        let mut o2 = (*o).clone_for_compare();
        let mut t2 = (*t).clone_for_compare();
        // The root folder's own name differs between the two temp trees.
        if t.folder_path.as_deref() == Some("") {
            o2.relative_folder.clear();
            t2.relative_folder.clear();
        }
        o2.sidecars.sort_by_key(|s| s.to_lowercase());
        t2.sidecars.sort_by_key(|s| s.to_lowercase());
        if o2 != t2 {
            bad.push(format!("item {p}: swift {t2:?} rust {o2:?}"));
        }
    }
    let key = |g: &TGroup| g.item_paths.iter().cloned().collect::<BTreeSet<_>>();
    let ours_g: BTreeMap<BTreeSet<String>, &TGroup> =
        mine.groups.iter().map(|g| (key(g), g)).collect();
    for g in &table.groups {
        match ours_g.get(&key(g)) {
            None => bad.push(format!(
                "swift group {} {:?} has no Rust equivalent",
                g.id, g.item_paths
            )),
            Some(o) => {
                let same_id = if g.id.starts_with("visit-") {
                    o.id.starts_with("visit-")
                } else {
                    o.id == g.id
                };
                let english = table.locale.starts_with("en");
                if !same_id
                    || o.item_paths[0] != g.item_paths[0]
                    || o.start_date != g.start_date
                    || o.end_date != g.end_date
                    || (english && o.title != g.title)
                    || (o.folder_hint != g.folder_hint && g.id != "undated")
                {
                    bad.push(format!("group differs: swift {g:?} rust {o:?}"));
                }
            }
        }
    }
    if ours_g.len() != table.groups.len() {
        bad.push(format!(
            "group count: swift {} rust {}",
            table.groups.len(),
            ours_g.len()
        ));
    }
    bad
}

impl TItem {
    fn clone_for_compare(&self) -> TItem {
        TItem {
            path: self.path.clone(),
            kind: self.kind.clone(),
            date_source: self.date_source.clone(),
            taken_at: self.taken_at.clone(),
            is_undated: self.is_undated,
            folder_path: self.folder_path.clone(),
            relative_folder: self.relative_folder.clone(),
            sidecars: self.sidecars.clone(),
        }
    }
}

#[test]
fn swift_oracle_matches_when_present() {
    let dir = fixtures().join("swift");
    if !dir.join("table.json").exists() {
        eprintln!(
            "crosscheck: {} not found; run CrossCheckFixtureTests on a Mac (see README). Skipping.",
            dir.display()
        );
        return;
    }
    let bad = compare(&dir);
    assert!(bad.is_empty(), "Swift/Rust mismatches:\n{}", bad.join("\n"));
}

/// Exercises the comparison against a Rust-written oracle, so the harness itself is tested
/// before any Swift output exists.
#[test]
fn harness_self_check() {
    let spec = spec();
    let (_d, root, items, groups) = rust_index(&spec);
    let out = tempfile::tempdir().unwrap();
    let mut snap = LibrarySnapshot::empty();
    snap.config = AppConfig {
        root_path: Some(root.to_string_lossy().into_owned()),
        ..Default::default()
    };
    snap.items = items.clone();
    snap.groups = groups.clone();
    std::fs::write(out.path().join("library.json"), snap.to_json()).unwrap();
    let table = table_from(&root, &items, &groups, spec.gap_hours);
    std::fs::write(
        out.path().join("table.json"),
        apple_json::to_string(&serde_json::to_value(&table).unwrap()),
    )
    .unwrap();
    assert_eq!(compare(out.path()), Vec::<String>::new());

    // Expected shape of the spec (pins the Undated rule on this corpus).
    let titles: BTreeSet<_> = groups
        .iter()
        .filter(|g| !g.id.starts_with("visit-"))
        .map(|g| g.title.clone())
        .collect();
    assert_eq!(
        titles,
        ["Undated", "old", "Porto"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
    let undated = groups.iter().find(|g| g.id == "undated").unwrap();
    assert_eq!(undated.item_paths.len(), 2); // root.jpg, clip.mp4
    assert_eq!(
        groups.len(),
        10,
        "{:#?}",
        groups
            .iter()
            .map(|g| (&g.title, &g.item_paths))
            .collect::<Vec<_>>()
    );

    // A tampered oracle is reported.
    let mut bad = table;
    bad.fnv[0].hash = "0".into();
    bad.groups[0].id = "visit-0".into();
    std::fs::write(
        out.path().join("table.json"),
        serde_json::to_string(&bad).unwrap(),
    )
    .unwrap();
    std::fs::write(
        out.path().join("library.json"),
        snap.to_json().replacen("  ", "   ", 1),
    )
    .unwrap();
    assert!(compare(out.path()).len() >= 3);
}
