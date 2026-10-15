// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Commit validation, journal, quarantine/trash moves and restore on temp trees (plan §10.1, §10.2, §10.4).

mod common;
use chrono::{Duration, TimeZone, Utc};
use common::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use unswiper_core::commit::*;
use unswiper_core::index::{index, MetadataDates};
use unswiper_core::store::{read_manifest, LibraryStore};
use unswiper_core::trash::{FreedesktopTrash, Trasher};
use unswiper_core::{CommitMode, Decision, EntryState, ManifestStatus, MediaItem};

struct Env {
    _d: tempfile::TempDir,
    root: PathBuf,
    store: LibraryStore,
    trash: FreedesktopTrash,
}

fn env() -> Env {
    let d = tempfile::tempdir().unwrap();
    let base = canon(d.path());
    let root = base.join("lib");
    std::fs::create_dir(&root).unwrap();
    Env {
        store: LibraryStore::new(base.join("support")),
        trash: FreedesktopTrash {
            trash_dir: base.join("Trash"),
        },
        root,
        _d: d,
    }
}

fn indexed(e: &Env) -> Vec<MediaItem> {
    index(&e.root, 36.0, &HashMap::new(), &MetadataDates, |_| {})
        .unwrap()
        .0
}

fn reject_all(items: &mut [MediaItem]) {
    for i in items {
        i.decision = Decision::Reject;
    }
}

fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut v = vec![];
    fn rec(dir: &Path, root: &Path, v: &mut Vec<(String, Vec<u8>)>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().unwrap() != "_Rejected" {
                    rec(&p, root, v);
                }
            } else {
                v.push((
                    p.strip_prefix(root).unwrap().to_string_lossy().into_owned(),
                    std::fs::read(&p).unwrap(),
                ));
            }
        }
    }
    rec(root, root, &mut v);
    v.sort();
    v
}

#[test]
fn validate_messages() {
    let e = env();
    let f = write(&e.root, "a.jpg", b"1234");
    let m = std::fs::metadata(&f).unwrap().modified().unwrap();
    let mt = Some(chrono::DateTime::<Utc>::from(m));
    let p = f.to_string_lossy().into_owned();
    assert_eq!(validate(&p, 4, mt, Some(&e.root)), None);
    assert_eq!(validate(&p, 4, mt, None), Some(MSG_NO_ROOT));
    assert_eq!(validate(&p, 5, mt, Some(&e.root)), Some(MSG_SIZE));
    assert_eq!(validate(&p, 4, None, Some(&e.root)), Some(MSG_NO_MTIME));
    assert_eq!(
        validate(
            &p,
            4,
            mt.map(|t| t + Duration::milliseconds(900)),
            Some(&e.root)
        ),
        None
    );
    assert_eq!(
        validate(
            &p,
            4,
            mt.map(|t| t - Duration::milliseconds(1100)),
            Some(&e.root)
        ),
        Some(MSG_MTIME)
    );
    let missing = e.root.join("missing.jpg").to_string_lossy().into_owned();
    assert_eq!(validate(&missing, 4, mt, Some(&e.root)), Some(MSG_MISSING));
    std::fs::create_dir(e.root.join("dir.jpg")).unwrap();
    assert_eq!(
        validate(
            &e.root.join("dir.jpg").to_string_lossy(),
            4,
            mt,
            Some(&e.root)
        ),
        Some(MSG_MISSING)
    );
    // Outside, ../ escape, prefix trick, symlink
    let outside = write(e.root.parent().unwrap(), "lib2/x.jpg", b"1234");
    assert_eq!(
        validate(&outside.to_string_lossy(), 4, mt, Some(&e.root)),
        Some(MSG_OUTSIDE)
    );
    let escape = format!("{}/../lib2/x.jpg", e.root.display());
    assert_eq!(validate(&escape, 4, mt, Some(&e.root)), Some(MSG_OUTSIDE));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&f, e.root.join("link.jpg")).unwrap();
        assert_eq!(
            validate(
                &e.root.join("link.jpg").to_string_lossy(),
                4,
                mt,
                Some(&e.root)
            ),
            Some(MSG_SYMLINK)
        );
    }
    assert_eq!(
        validate(&e.root.to_string_lossy(), 4, mt, Some(&e.root)),
        Some(MSG_OUTSIDE)
    );
}

#[test]
fn dry_run_and_nothing_to_do() {
    let e = env();
    write(&e.root, "a.jpg", b"12");
    let mut items = indexed(&e);
    let r = commit_rejects(
        &items,
        CommitMode::Quarantine,
        Some(&e.root),
        &e.store,
        &e.trash,
    )
    .unwrap();
    assert_eq!(r, CommitResult::default());
    reject_all(&mut items);
    let before = snapshot(&e.root);
    let r = commit_rejects(
        &items,
        CommitMode::DryRun,
        Some(&e.root),
        &e.store,
        &e.trash,
    )
    .unwrap();
    assert!(r.dry_run);
    assert_eq!((r.would_move, r.bytes, r.moved), (1, 2, 0));
    assert_eq!(snapshot(&e.root), before);
    assert!(e.store.manifest_paths().is_empty());
}

#[test]
fn quarantine_commit_then_restore_round_trip() {
    let e = env();
    write(&e.root, "a/IMG_1.HEIC", b"heic");
    write(&e.root, "a/IMG_1.MOV", b"mov");
    write(&e.root, "b/IMG_1.HEIC", b"heic2"); // same name → collision-renamed in quarantine
    write(&e.root, "keep.jpg", b"k");
    let before = snapshot(&e.root);
    let mut items = indexed(&e);
    for i in items.iter_mut().filter(|i| !i.path.ends_with("keep.jpg")) {
        i.decision = Decision::Reject;
    }
    let now = Utc.with_ymd_and_hms(2026, 10, 13, 12, 0, 0).unwrap();
    let r = commit_rejects_at(
        &items,
        CommitMode::Quarantine,
        Some(&e.root),
        &e.store,
        &e.trash,
        now,
        None,
    )
    .unwrap();
    assert_eq!(r.moved, 2);
    assert!(r.failed.is_empty(), "{:?}", r.failed);
    assert_eq!(r.bytes, 12);
    let mp = r.manifest_path.unwrap();
    assert!(mp.ends_with("2026-10-13T12-00-00Z.json"));
    let m = read_manifest(&mp).unwrap();
    assert_eq!(m.status, Some(ManifestStatus::Complete));
    assert!(m
        .entries
        .iter()
        .all(|x| x.effective_state() == EntryState::Moved && x.moved_at.is_some()));
    let qdir = e.root.join("_Rejected/Unswiper/2026-10-13T12-00-00Z");
    assert!(qdir.join("manifest.json").exists());
    assert_eq!(std::fs::read_dir(&qdir).unwrap().count(), 4);
    assert!(!e.root.join("a/IMG_1.MOV").exists());

    let rr = restore(&m);
    assert_eq!((rr.restored, rr.already_in_place), (3, 0));
    assert!(rr.failed.is_empty());
    assert_eq!(snapshot(&e.root), before);
    // Second restore: sources gone.
    let rr = restore(&m);
    assert_eq!(rr.failed.len(), 3);
    assert!(rr.failed[0].1.starts_with("Moved file not found at"));
}

#[test]
fn trash_commit_and_restore_removes_trashinfo() {
    let e = env();
    write(&e.root, "x/a.jpg", b"aaa");
    let before = snapshot(&e.root);
    let mut items = indexed(&e);
    reject_all(&mut items);
    let r = commit_rejects(&items, CommitMode::Trash, Some(&e.root), &e.store, &e.trash).unwrap();
    assert_eq!(r.moved, 1);
    let m = read_manifest(&r.manifest_path.unwrap()).unwrap();
    let rp = PathBuf::from(&m.entries[0].result_path);
    assert!(rp.starts_with(e.trash.trash_dir.join("files")));
    let info = e.trash.info_for(&rp).unwrap();
    assert!(info.exists());
    let rr = restore(&m);
    assert_eq!(rr.restored, 1);
    assert!(!info.exists());
    assert_eq!(snapshot(&e.root), before);
    // Trash emptied
    let rr = restore(&m);
    assert_eq!(rr.failed[0].1, MSG_TRASH_GONE);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "known Windows path gap, see .github/workflows/rust.yml windows job"
)]
fn changed_files_block_their_whole_set() {
    let e = env();
    write(&e.root, "a/IMG_1.HEIC", b"heic");
    let mov = write(&e.root, "a/IMG_1.MOV", b"mov");
    write(&e.root, "b/ok.jpg", b"ok");
    let gone = write(&e.root, "b/gone.jpg", b"g");
    let mut items = indexed(&e);
    reject_all(&mut items);
    std::fs::write(&mov, b"changed sidecar").unwrap();
    std::fs::remove_file(&gone).unwrap();
    let r = commit_rejects(
        &items,
        CommitMode::Quarantine,
        Some(&e.root),
        &e.store,
        &e.trash,
    )
    .unwrap();
    assert_eq!(r.moved, 1);
    let mut fails: Vec<_> = r
        .failed
        .iter()
        .map(|f| (f.0.rsplit('/').next().unwrap().to_string(), f.1.clone()))
        .collect();
    fails.sort();
    assert_eq!(
        fails,
        vec![
            ("IMG_1.HEIC".into(), MSG_SIZE.into()),
            ("gone.jpg".into(), MSG_MISSING.into())
        ]
    );
    assert!(e.root.join("a/IMG_1.HEIC").exists());
}

#[test]
fn all_invalid_writes_no_manifest() {
    let e = env();
    write(&e.root, "a.jpg", b"a");
    let mut items = indexed(&e);
    reject_all(&mut items);
    let r = commit_rejects(&items, CommitMode::Quarantine, None, &e.store, &e.trash).unwrap();
    assert_eq!(r.failed[0].1, MSG_NO_ROOT);
    assert!(r.manifest_path.is_none());
    assert!(e.store.manifest_paths().is_empty());
}

struct FailingTrash;
impl Trasher for FailingTrash {
    fn trash(&self, _: &Path) -> std::io::Result<Option<PathBuf>> {
        Err(std::io::Error::other("trash unavailable"))
    }
}

#[test]
fn every_move_failing_removes_manifest() {
    let e = env();
    write(&e.root, "a.jpg", b"a");
    let mut items = indexed(&e);
    reject_all(&mut items);
    let r = commit_rejects(
        &items,
        CommitMode::Trash,
        Some(&e.root),
        &e.store,
        &FailingTrash,
    )
    .unwrap();
    assert_eq!(r.moved, 0);
    assert_eq!(r.failed[0].1, "trash unavailable");
    assert!(r.manifest_path.is_none());
    assert!(e.store.manifest_paths().is_empty());
}

struct UnknownLocationTrash;
impl Trasher for UnknownLocationTrash {
    fn trash(&self, p: &Path) -> std::io::Result<Option<PathBuf>> {
        std::fs::remove_file(p)?;
        Ok(None)
    }
}

#[test]
fn trash_without_location_reports_on_restore() {
    let e = env();
    write(&e.root, "a.jpg", b"a");
    let mut items = indexed(&e);
    reject_all(&mut items);
    let r = commit_rejects(
        &items,
        CommitMode::Trash,
        Some(&e.root),
        &e.store,
        &UnknownLocationTrash,
    )
    .unwrap();
    let m = read_manifest(&r.manifest_path.unwrap()).unwrap();
    assert_eq!(restore(&m).failed[0].1, MSG_TRASH_UNKNOWN);
    let mut q = m.clone();
    q.mode = CommitMode::Quarantine;
    assert_eq!(restore(&q).failed[0].1, MSG_NO_RECORD);
}

#[test]
fn interrupted_commit_is_pending_and_restorable() {
    let e = env();
    for n in ["a", "b", "c"] {
        write(&e.root, &format!("{n}.jpg"), n.as_bytes());
    }
    let before = snapshot(&e.root);
    let mut items = indexed(&e);
    reject_all(&mut items);
    let r = commit_rejects_at(
        &items,
        CommitMode::Quarantine,
        Some(&e.root),
        &e.store,
        &e.trash,
        Utc::now(),
        Some(2),
    )
    .unwrap();
    let mp = r.manifest_path.unwrap();
    assert_eq!(e.store.pending_manifest_paths(), vec![mp.clone()]);
    let m = read_manifest(&mp).unwrap();
    let states: Vec<_> = m.entries.iter().map(|x| x.effective_state()).collect();
    assert_eq!(
        states,
        vec![EntryState::Moved, EntryState::Moved, EntryState::Pending]
    );
    let rr = restore(&m);
    assert_eq!((rr.restored, rr.already_in_place), (2, 1));
    assert_eq!(snapshot(&e.root), before);
}

#[test]
fn pending_trash_entry_without_path_and_target_exists() {
    let e = env();
    let a = write(&e.root, "a.jpg", b"a");
    let mut items = indexed(&e);
    reject_all(&mut items);
    let r = commit_rejects(
        &items,
        CommitMode::Quarantine,
        Some(&e.root),
        &e.store,
        &e.trash,
    )
    .unwrap();
    let mut m = read_manifest(&r.manifest_path.unwrap()).unwrap();
    std::fs::write(&a, b"new file in the way").unwrap();
    assert_eq!(restore(&m).failed[0].1, MSG_TARGET_EXISTS);
    std::fs::remove_file(&a).unwrap();
    m.entries[0].state = Some(EntryState::Pending);
    m.entries[0].result_path.clear();
    assert_eq!(restore(&m).failed[0].1, MSG_TRASH_UNKNOWN);
    m.entries[0].state = Some(EntryState::Failed);
    assert_eq!(restore(&m), RestoreResult::default());
}

#[test]
fn restore_recreates_missing_parent_folders() {
    let e = env();
    write(&e.root, "deep/er/a.jpg", b"a");
    let mut items = indexed(&e);
    reject_all(&mut items);
    let r = commit_rejects(
        &items,
        CommitMode::Quarantine,
        Some(&e.root),
        &e.store,
        &e.trash,
    )
    .unwrap();
    std::fs::remove_dir_all(e.root.join("deep")).unwrap();
    let m = read_manifest(&r.manifest_path.unwrap()).unwrap();
    assert_eq!(restore(&m).restored, 1);
    assert!(e.root.join("deep/er/a.jpg").exists());
}

#[test]
fn move_file_refuses_overwrite() {
    let e = env();
    let a = write(&e.root, "a", b"a");
    let b = write(&e.root, "b", b"b");
    assert!(move_file(&a, &b).is_err());
    assert_eq!(std::fs::read(&b).unwrap(), b"b");
    assert!(move_file(&e.root.join("nope"), &e.root.join("c")).is_err());
}

mod props {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]
        #[test]
        fn commit_then_restore_is_identity(
            files in prop::collection::btree_map(
                ("(d[0-2]/){0,3}", "[a-cA-C é]{1,4}", prop::sample::select(vec!["jpg", "JPG", "heic", "mov", "aae", "mp4"])),
                prop::collection::vec(any::<u8>(), 0..16), 1..12),
            mask in any::<u32>(),
            trash in any::<bool>(),
        ) {
            let e = env();
            for ((dir, stem, ext), data) in &files {
                let rel = format!("{dir}{}.{ext}", stem.trim());
                if stem.trim().is_empty() { continue; }
                write(&e.root, &rel, data);
            }
            let before = snapshot(&e.root);
            let mut items = indexed(&e);
            for (n, i) in items.iter_mut().enumerate() {
                if mask & (1 << (n % 32)) != 0 { i.decision = Decision::Reject; }
            }
            let mode = if trash { CommitMode::Trash } else { CommitMode::Quarantine };
            let dry = commit_rejects(&items, CommitMode::DryRun, Some(&e.root), &e.store, &e.trash).unwrap();
            prop_assert!(dry.dry_run);
            prop_assert_eq!(&snapshot(&e.root), &before);
            let r = commit_rejects(&items, mode, Some(&e.root), &e.store, &e.trash).unwrap();
            prop_assert!(r.failed.is_empty(), "{:?}", r.failed);
            if let Some(mp) = r.manifest_path {
                let rr = restore(&read_manifest(&mp).unwrap());
                prop_assert!(rr.failed.is_empty(), "{:?}", rr.failed);
            }
            prop_assert_eq!(snapshot(&e.root), before);
        }
    }
}

#[test]
fn visit_members_follow_group_order_and_skip_missing() {
    let e = env();
    std::fs::write(e.root.join("a.jpg"), b"a").unwrap();
    std::fs::write(e.root.join("b.jpg"), b"b").unwrap();
    let items = indexed(&e);
    let mut g = unswiper_core::group::group(&items, 36.0).remove(0);
    g.item_paths.reverse();
    g.item_paths.push("/gone.jpg".into());
    let m = visit_members(&items, &g);
    assert_eq!(m.len(), 2);
    assert_eq!(m[0].path, g.item_paths[0]);
}
