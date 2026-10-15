// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Live folder watching (plan §10.4a): incremental re-index and the real watcher back ends.

mod common;
use common::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;
use unswiper_core::index::{index, MetadataDates};
use unswiper_core::watch::*;
use unswiper_core::Decision;

fn live(root: &Path) -> LiveIndex {
    LiveIndex::new(root, 36.0, &HashMap::new(), &MetadataDates).unwrap()
}

fn batch(paths: &[&Path]) -> WatchBatch {
    WatchBatch::Paths(paths.iter().map(|p| p.to_path_buf()).collect())
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// The incremental result must always equal a fresh full index (modulo decisions).
fn assert_matches_full(li: &LiveIndex, root: &Path) {
    let (mut items, groups) = index(root, 36.0, &HashMap::new(), &MetadataDates, |_| {}).unwrap();
    let mut mine = li.items();
    for i in mine.iter_mut().chain(items.iter_mut()) {
        i.decision = Decision::Undecided;
    }
    assert_eq!(mine, items);
    let ids = |g: &[unswiper_core::VisitGroup]| {
        let mut v: Vec<_> = g
            .iter()
            .map(|g| (g.id.clone(), g.item_paths.clone()))
            .collect();
        v.sort();
        v
    };
    assert_eq!(ids(li.groups()), ids(&groups));
}

fn root() -> (tempfile::TempDir, PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let r = canon(d.path()).join("lib");
    std::fs::create_dir(&r).unwrap();
    (d, r)
}

#[test]
fn add_modify_delete_files() {
    let (_d, r) = root();
    let a = write(&r, "x/a.jpg", b"a");
    let mut li = live(&r);
    li.set_decision(&s(&a), Decision::Keep);

    let b = write(&r, "x/b.jpg", b"b");
    let d = li.apply(&batch(&[&b]), &MetadataDates);
    assert_eq!(d.added, vec![s(&b)]);
    assert!(d.removed.is_empty() && d.updated.is_empty() && !d.rescanned);

    std::fs::write(&a, b"changed").unwrap();
    let d = li.apply(&batch(&[&a]), &MetadataDates);
    assert_eq!(d.updated, vec![s(&a)]);
    assert_eq!(
        li.item(&s(&a)).unwrap().decision,
        Decision::Keep,
        "decision survives an edit"
    );
    assert_eq!(li.item(&s(&a)).unwrap().size_bytes, 7);

    std::fs::remove_file(&b).unwrap();
    let d = li.apply(&batch(&[&b]), &MetadataDates);
    assert_eq!(d.removed, vec![s(&b)]);
    assert_matches_full(&li, &r);

    // No-op batch
    assert!(li.apply(&batch(&[&a]), &MetadataDates).is_empty());
    assert!(li
        .apply(&WatchBatch::Paths(vec![]), &MetadataDates)
        .is_empty());
}

#[test]
fn rename_and_move_back_keeps_decision() {
    let (_d, r) = root();
    let a = write(&r, "x/a.jpg", b"a");
    let mut li = live(&r);
    li.set_decision(&s(&a), Decision::Reject);
    let moved = r.join("x/renamed.jpg");
    std::fs::rename(&a, &moved).unwrap();
    let d = li.apply(&batch(&[&a, &moved]), &MetadataDates);
    assert_eq!(
        (d.added.clone(), d.removed.clone()),
        (vec![s(&moved)], vec![s(&a)])
    );
    assert_eq!(li.item(&s(&moved)).unwrap().decision, Decision::Undecided);
    std::fs::rename(&moved, &a).unwrap();
    li.apply(&batch(&[&a, &moved]), &MetadataDates);
    assert_eq!(li.item(&s(&a)).unwrap().decision, Decision::Reject);
    assert!(!li.set_decision("/nope", Decision::Keep));
}

#[test]
fn sync_item_copies_decision_and_extra() {
    let (_d, r) = root();
    let a = write(&r, "x/a.jpg", b"a");
    let mut li = live(&r);
    let mut src = li.item(&s(&a)).unwrap().clone();
    src.decision = Decision::Reject;
    src.extra
        .insert("decisionSource".into(), serde_json::json!("auto-blur"));
    assert!(li.sync_item(&src));
    let got = li.item(&s(&a)).unwrap();
    assert_eq!(got.decision, Decision::Reject);
    assert_eq!(got.extra["decisionSource"], "auto-blur");
    src.path = "/nope".into();
    assert!(!li.sync_item(&src));
}

#[test]
fn live_photo_pair_forms_and_breaks() {
    let (_d, r) = root();
    let heic = write(&r, "t/IMG_1.HEIC", b"heic");
    let mut li = live(&r);
    li.set_decision(&s(&heic), Decision::Keep);
    let mov = write(&r, "t/IMG_1.MOV", b"mov");
    let d = li.apply(&batch(&[&mov]), &MetadataDates);
    assert_eq!(d.updated, vec![s(&heic)]);
    assert!(d.added.is_empty());
    let item = li.item(&s(&heic)).unwrap();
    assert_eq!(item.sidecars.len(), 1);
    assert_eq!(item.decision, Decision::Keep);
    assert!(li.item(&s(&mov)).is_none());

    // Removing the still turns the MOV into its own video item.
    std::fs::remove_file(&heic).unwrap();
    let d = li.apply(&batch(&[&heic]), &MetadataDates);
    assert_eq!(d.removed, vec![s(&heic)]);
    assert_eq!(d.added, vec![s(&mov)]);
    assert_matches_full(&li, &r);

    // Sidecar edit marks the anchor updated.
    let heic = write(&r, "t/IMG_1.HEIC", b"heic");
    li.apply(&batch(&[&heic]), &MetadataDates);
    std::fs::write(&mov, b"longer mov").unwrap();
    let d = li.apply(&batch(&[&mov]), &MetadataDates);
    assert_eq!(d.updated, vec![s(&heic)]);
    assert_matches_full(&li, &r);
}

#[test]
fn undated_file_joins_new_dated_sibling() {
    let (_d, r) = root();
    write(&r, "Lisbon/nodate.jpg", b"n");
    let mut li = live(&r);
    assert_eq!(li.groups()[0].title, "Lisbon");
    let dated = write(
        &r,
        "Lisbon/dated.jpg",
        &jpeg_with_exif(Some("2026:03:03 12:00:00"), None),
    );
    li.apply(&batch(&[&dated]), &MetadataDates);
    assert_eq!(li.groups().len(), 1);
    assert_eq!(li.groups()[0].item_paths.len(), 2);
    assert!(li.groups()[0].id.starts_with("visit-"));
    assert_matches_full(&li, &r);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "known Windows path gap, see .github/workflows/rust.yml windows job"
)]
fn folder_create_rename_delete() {
    let (_d, r) = root();
    write(&r, "keep.jpg", b"k");
    let mut li = live(&r);
    // A whole folder appears (e.g. copied in): only the folder path is reported.
    write(&r, "trip/2026-01-01/a.jpg", b"a");
    write(&r, "trip/2026-01-01/b.jpg", b"b");
    write(&r, "trip/.hidden/c.jpg", b"c");
    let d = li.apply(&batch(&[&r.join("trip")]), &MetadataDates);
    assert_eq!(d.added.len(), 2);
    // Rename the folder.
    std::fs::rename(r.join("trip"), r.join("voyage")).unwrap();
    let d = li.apply(
        &batch(&[&r.join("trip"), &r.join("voyage")]),
        &MetadataDates,
    );
    assert_eq!((d.added.len(), d.removed.len()), (2, 2));
    assert_matches_full(&li, &r);
    // Delete it.
    std::fs::remove_dir_all(r.join("voyage")).unwrap();
    let d = li.apply(&batch(&[&r.join("voyage")]), &MetadataDates);
    assert_eq!(d.removed.len(), 2);
    assert_matches_full(&li, &r);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "known Windows path gap, see .github/workflows/rust.yml windows job"
)]
fn ignored_paths_and_suppression() {
    let (_d, r) = root();
    let a = write(&r, "a.jpg", b"a");
    let mut li = live(&r);
    let hidden = write(&r, ".x.jpg", b"");
    let rejected = write(&r, "_Rejected/Unswiper/z.jpg", b"");
    let txt = write(&r, "notes.txt", b"");
    let outside = write(r.parent().unwrap(), "other/o.jpg", b"");
    assert!(li
        .apply(
            &batch(&[&hidden, &rejected, &txt, &outside, &r]),
            &MetadataDates
        )
        .is_empty());

    li.suppress([a.clone()], Duration::from_secs(60));
    std::fs::remove_file(&a).unwrap();
    assert!(li.apply(&batch(&[&a]), &MetadataDates).is_empty());
    let d = li.remove_paths(&[s(&a), "/not/indexed".into()]);
    assert_eq!(d.removed, vec![s(&a)]);
    assert!(li.items().is_empty());
    assert!(li.groups().is_empty());
    // Expired suppression no longer hides events.
    li.suppress([a.clone()], Duration::ZERO);
    write(&r, "a.jpg", b"a");
    assert_eq!(li.apply(&batch(&[&a]), &MetadataDates).added.len(), 1);
}

#[test]
fn rescan_and_root_removal() {
    let (_d, r) = root();
    let a = write(&r, "a.jpg", b"a");
    let mut li = live(&r);
    li.set_decision(&s(&a), Decision::Keep);
    write(&r, "sub/b.jpg", b"b");
    let d = li.apply(&WatchBatch::Rescan, &MetadataDates);
    assert!(d.rescanned);
    assert_eq!(d.added.len(), 1);
    assert_eq!(li.item(&s(&a)).unwrap().decision, Decision::Keep);
    std::fs::remove_dir_all(&r).unwrap();
    let d = li.apply(&batch(&[&r]), &MetadataDates);
    assert!(d.rescanned);
    assert_eq!(d.removed.len(), 2);
    // Root comes back: decisions remembered.
    write(&r, "a.jpg", b"a");
    li.apply(&WatchBatch::Rescan, &MetadataDates);
    assert_eq!(li.item(&s(&a)).unwrap().decision, Decision::Keep);
    assert!(LiveIndex::new(&r.join("missing"), 36.0, &HashMap::new(), &MetadataDates).is_err());
}

fn collect(
    rx: &mpsc::Receiver<WatchBatch>,
    until: impl Fn(&[WatchBatch]) -> bool,
) -> Vec<WatchBatch> {
    let mut v = vec![];
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline && !until(&v) {
        if let Ok(b) = rx.recv_timeout(Duration::from_millis(200)) {
            v.push(b);
        }
    }
    v
}

fn paths_in(batches: &[WatchBatch]) -> Vec<PathBuf> {
    batches
        .iter()
        .flat_map(|b| match b {
            WatchBatch::Paths(p) => p.clone(),
            WatchBatch::Rescan => vec![],
        })
        .collect()
}

#[test]
fn native_watcher_burst_is_batched() {
    let (_d, r) = root();
    let mut li = live(&r);
    let (tx, rx) = mpsc::channel();
    let w = FolderWatcher::start(
        &r,
        Duration::from_millis(300),
        Duration::from_secs(10),
        move |b| {
            let _ = tx.send(b);
        },
    )
    .unwrap();
    assert_eq!(w.backend(), Backend::Native);
    std::thread::sleep(Duration::from_millis(100));
    for i in 0..500 {
        write(&r, &format!("burst/{i:04}.jpg"), b"x");
    }
    let mut batches = 0;
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while li.items().len() < 500 && std::time::Instant::now() < deadline {
        if let Ok(b) = rx.recv_timeout(Duration::from_millis(200)) {
            batches += 1;
            li.apply(&b, &MetadataDates);
        }
    }
    assert!(batches <= 3, "burst produced {batches} batches");
    assert_eq!(li.items().len(), 500);
    assert_matches_full(&li, &r);
    drop(w);
}

#[test]
fn polling_watcher_sees_changes() {
    let (_d, r) = root();
    let (tx, rx) = mpsc::channel();
    let w = FolderWatcher::start_polling(
        &r,
        Duration::from_millis(100),
        Duration::from_millis(150),
        Duration::from_secs(2),
        move |b| {
            let _ = tx.send(b);
        },
    )
    .unwrap();
    assert_eq!(w.backend(), Backend::Polling);
    std::thread::sleep(Duration::from_millis(300));
    let a = write(&r, "p/a.jpg", b"a");
    let batches = collect(&rx, |v| {
        paths_in(v).iter().any(|p| p == &a || p == &r.join("p"))
    });
    assert!(!batches.is_empty());
}

#[test]
fn start_on_missing_folder_fails() {
    let r = PathBuf::from("/nonexistent/unswiper-watch");
    assert!(FolderWatcher::start(&r, DEFAULT_QUIET, DEFAULT_MAX_WAIT, |_| {}).is_err());
}

#[test]
fn from_items_wraps_existing_index() {
    let (_d, r) = root();
    let a = write(&r, "x/a.jpg", b"a");
    let (items, groups) = index(&r, 36.0, &HashMap::new(), &MetadataDates, |_| {}).unwrap();
    let mut li = LiveIndex::from_items(&r, 36.0, items.clone());
    assert_eq!(li.root(), r.as_path());
    assert_eq!(li.items(), items);
    assert_eq!(li.groups(), groups.as_slice());
    let b = write(&r, "x/b.jpg", b"b");
    assert_eq!(li.apply(&batch(&[&b]), &MetadataDates).added, vec![s(&b)]);
    assert!(li.item(&s(&a)).is_some());
}
