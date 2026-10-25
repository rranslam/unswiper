// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! library.json schema v1 compatibility with the Swift app (plan §10.5) and LibraryStore behaviour.

use chrono::{TimeZone, Utc};
use unswiper_core::store::{read_manifest, LibraryStore, StoreError};
use unswiper_core::{CommitMode, DateSource, Decision, EntryState, LibrarySnapshot};

const V1: &str = include_str!("fixtures/compat/library-v1.json");
const PRE: &str = include_str!("fixtures/compat/library-pre-0.1.2.json");

#[test]
fn v1_round_trips_byte_for_byte() {
    let snap: LibrarySnapshot = serde_json::from_str(V1).unwrap();
    assert_eq!(snap.schema_version, Some(1));
    assert_eq!(snap.config.commit_mode, CommitMode::Quarantine);
    assert_eq!(snap.config.bookmark_data.as_deref(), Some(&b"bookmark"[..]));
    assert_eq!(snap.items[0].decision, Decision::Reject);
    assert_eq!(snap.items[0].sidecars.len(), 1);
    assert_eq!(snap.items[1].date_source, Some(DateSource::Unknown));
    assert_eq!(snap.to_json(), V1);
}

#[test]
fn pre_012_library_decodes_with_defaults() {
    let snap: LibrarySnapshot = serde_json::from_str(PRE).unwrap();
    assert_eq!(snap.schema_version, None);
    let i = &snap.items[0];
    assert!(
        i.sidecars.is_empty()
            && !i.is_undated
            && i.modified_at.is_none()
            && i.date_source.is_none()
            && i.folder_path.is_none()
    );
    assert_eq!(snap.config.keep_targets, None);
    assert_eq!(snap.last_indexed_at, None);
}

#[test]
fn unknown_fields_survive() {
    let mut v: serde_json::Value = serde_json::from_str(V1).unwrap();
    v["futureTop"] = serde_json::json!({"x": 1});
    v["items"][0]["futureItem"] = serde_json::json!("y");
    v["config"]["futureConfig"] = serde_json::json!(true);
    v["groups"][0]["futureGroup"] = serde_json::json!([1]);
    v["items"][0]["sidecars"][0]["futureSidecar"] = serde_json::json!(2);
    let snap: LibrarySnapshot = serde_json::from_value(v.clone()).unwrap();
    let back: serde_json::Value = serde_json::from_str(&snap.to_json()).unwrap();
    assert_eq!(back, v);
}

#[test]
fn old_manifest_reads_as_complete_and_moved() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("m.json");
    std::fs::write(
        &p,
        include_str!("fixtures/compat/manifest-quarantine-v0.1.1.json"),
    )
    .unwrap();
    let m = read_manifest(&p).unwrap();
    assert!(!m.is_pending());
    assert_eq!(m.entries[0].effective_state(), EntryState::Moved);
    assert_eq!(m.mode, CommitMode::Quarantine);
}

#[test]
fn load_missing_save_and_reload() {
    let d = tempfile::tempdir().unwrap();
    let mut st = LibraryStore::new(d.path().join("support"));
    let r = st.load();
    assert!(r.problem.is_none());
    assert_eq!(r.snapshot, LibrarySnapshot::empty());
    let mut snap: LibrarySnapshot = serde_json::from_str(PRE).unwrap();
    snap.last_indexed_at = Some(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
    st.save(&snap).unwrap();
    let r = st.load();
    assert!(r.problem.is_none());
    assert_eq!(
        r.snapshot.schema_version,
        Some(LibrarySnapshot::CURRENT_SCHEMA_VERSION)
    );
    assert_eq!(r.snapshot.items, snap.items);
    assert_eq!(st.support_dir(), d.path().join("support"));
}

#[test]
fn damaged_and_newer_files_are_backed_up() {
    let d = tempfile::tempdir().unwrap();
    let mut st = LibraryStore::new(d.path());
    std::fs::write(st.snapshot_path(), "{ not json").unwrap();
    let r = st.load();
    let backup = r.backup_path.unwrap();
    assert!(r.problem.unwrap().contains("it is damaged"));
    assert!(backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("library.json.corrupt-"));
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), "{ not json");

    std::fs::write(
        st.snapshot_path(),
        V1.replace("\"schemaVersion\" : 1", "\"schemaVersion\" : 9"),
    )
    .unwrap();
    let r = st.load();
    assert!(r
        .problem
        .unwrap()
        .contains("newer version of Unswiper (format 9)"));
    // Same-second backups get -2, -3 …
    let now = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    let a = st.backup_snapshot(now).unwrap();
    let b = st.backup_snapshot(now).unwrap();
    assert!(b
        .to_string_lossy()
        .ends_with(&format!("{}-2", a.file_name().unwrap().to_string_lossy())));
}

#[test]
fn unreadable_and_unbackupable_blocks_save() {
    let d = tempfile::tempdir().unwrap();
    let mut st = LibraryStore::new(d.path());
    // library.json is a directory: can't be read, can't be copied.
    std::fs::create_dir(st.snapshot_path()).unwrap();
    let r = st.load();
    assert!(r.backup_path.is_none());
    assert!(r.problem.unwrap().contains("won't overwrite it"));
    let err = st.save(&LibrarySnapshot::empty()).unwrap_err();
    assert!(matches!(err, StoreError::SaveBlocked(_)));
    assert!(err.to_string().contains("not overwriting"));
}

#[test]
fn manifests_listing_pending_latest_and_restored() {
    use unswiper_core::store::write_manifest;
    let d = tempfile::tempdir().unwrap();
    let st = LibraryStore::new(d.path());
    assert!(st.manifest_paths().is_empty());
    assert!(st.latest_manifest_path().is_none());
    let mut m: unswiper_core::CommitManifest = serde_json::from_str(include_str!(
        "fixtures/compat/manifest-quarantine-v0.1.1.json"
    ))
    .unwrap();
    let c = st.commits_dir();
    write_manifest(&m, &[c.join("2026-10-12T10-00-00Z.json")]).unwrap();
    m.status = Some(unswiper_core::ManifestStatus::Pending);
    write_manifest(&m, &[c.join("2026-10-13T10-00-00Z.json")]).unwrap();
    std::fs::write(c.join("notes.txt"), "x").unwrap();
    std::fs::write(c.join("2026-10-14T00-00-00Z.json"), "garbage").unwrap();
    assert_eq!(st.manifest_paths().len(), 3);
    assert!(st
        .latest_manifest_path()
        .unwrap()
        .ends_with("2026-10-14T00-00-00Z.json"));
    let pending = st.pending_manifest_paths();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].ends_with("2026-10-13T10-00-00Z.json"));
    let r = st.mark_restored(&pending[0]).unwrap();
    assert!(r.to_string_lossy().ends_with(".json.restored"));
    assert_eq!(st.manifest_paths().len(), 2);
    assert!(read_manifest(&c.join("2026-10-14T00-00-00Z.json")).is_err());
    assert!(read_manifest(&c.join("nope.json")).is_err());
    assert!(st.quarantine_root().ends_with("Quarantine"));
}
