// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Integration tests for the indexer on real temp trees (plan §10.1 pairing, §10.3 goldens).

mod common;
use chrono::{TimeZone, Utc};
use common::*;
use std::collections::HashMap;
use unswiper_core::index::{index, IndexError, IndexProgress, MetadataDates};
use unswiper_core::{DateSource, Decision, MediaKind};

fn run(
    root: &std::path::Path,
) -> (
    Vec<unswiper_core::MediaItem>,
    Vec<unswiper_core::VisitGroup>,
) {
    index(root, 36.0, &HashMap::new(), &MetadataDates, |_| {}).unwrap()
}

#[test]
#[cfg_attr(
    windows,
    ignore = "known Windows path gap, see .github/workflows/rust.yml windows job"
)]
fn live_photo_pairing_and_sidecars() {
    let d = tempfile::tempdir().unwrap();
    let root = canon(d.path());
    let exif = jpeg_with_exif(Some("2026:01:02 10:00:00"), None);
    write(&root, "a/IMG_1.HEIC", &exif);
    write(&root, "a/img_1.mov", b"mov");
    write(&root, "a/IMG_1.AAE", b"aae");
    write(&root, "a/IMG_1.JPG", &exif); // same stem JPG + HEIC: two items, sidecars on HEIC
    write(&root, "a/CLIP.MP4", &mp4_with_mvhd(1_767_400_000));
    write(&root, "a/CLIP.xmp", b"x");
    write(&root, "a/orphan.aae", b"o");
    write(&root, "b/IMG_1.MOV", b"lonely mov"); // different folder: not paired
    write(&root, "a/VID.mp4", b"");
    write(&root, "a/VID.jpg", &exif); // .mp4 is never a Live Photo half
    write(&root, "a/notes.txt", b"ignored");
    let (items, _) = run(&root);
    let by_name: HashMap<_, _> = items
        .iter()
        .map(|i| {
            (
                i.path
                    .strip_prefix(&format!("{}/", root.display()))
                    .unwrap()
                    .to_string(),
                i,
            )
        })
        .collect();
    let mut names: Vec<_> = by_name.keys().cloned().collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "a/CLIP.MP4",
            "a/IMG_1.HEIC",
            "a/IMG_1.JPG",
            "a/VID.jpg",
            "a/VID.mp4",
            "b/IMG_1.MOV"
        ]
    );
    let heic = by_name["a/IMG_1.HEIC"];
    let side: Vec<_> = heic
        .sidecars
        .iter()
        .map(|s| s.path.rsplit('/').next().unwrap().to_string())
        .collect();
    assert_eq!(side, vec!["IMG_1.AAE", "img_1.mov"]);
    assert_eq!(heic.total_bytes(), heic.size_bytes + 6);
    assert!(by_name["a/IMG_1.JPG"].sidecars.is_empty());
    assert_eq!(by_name["a/CLIP.MP4"].sidecars.len(), 1);
    assert_eq!(by_name["a/CLIP.MP4"].kind, MediaKind::Video);
    assert_eq!(by_name["a/CLIP.MP4"].date_source, Some(DateSource::Capture));
    assert_eq!(
        by_name["a/CLIP.MP4"].taken_at,
        Utc.timestamp_opt(1_767_400_000, 0).unwrap()
    );
    assert_eq!(by_name["b/IMG_1.MOV"].kind, MediaKind::Video);
    assert!(by_name["b/IMG_1.MOV"].is_undated);
    assert_eq!(heic.relative_folder, "a");
    assert_eq!(heic.folder_path.as_deref(), Some("a"));
    assert!(heic.modified_at.is_some());
}

#[test]
#[cfg_attr(
    windows,
    ignore = "known Windows path gap, see .github/workflows/rust.yml windows job"
)]
fn skips_hidden_rejected_trash_symlinks_and_packages() {
    let d = tempfile::tempdir().unwrap();
    let root = canon(d.path());
    write(&root, "keep.jpg", b"");
    write(&root, ".hidden.jpg", b"");
    write(&root, ".cache/x.jpg", b"");
    write(&root, "_Rejected/Unswiper/x.jpg", b"");
    write(&root, "sub/_rejected/y.jpg", b"");
    write(&root, "Photos.photoslibrary/originals/z.jpg", b"");
    // Symlinks need privileges on Windows; that CI job covers the rest of this test.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("keep.jpg"), root.join("link.jpg")).unwrap();
        std::os::unix::fs::symlink(root.join("sub"), root.join("linkdir")).unwrap();
    }
    let (items, groups) = run(&root);
    assert_eq!(items.len(), 1);
    assert!(items[0].path.ends_with("/keep.jpg"));
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].id, "undated");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "known Windows path gap, see .github/workflows/rust.yml windows job"
)]
fn dates_capture_tiff_folder_and_undated_rule() {
    let d = tempfile::tempdir().unwrap();
    let root = canon(d.path());
    write(
        &root,
        "x/dto.jpg",
        &jpeg_with_exif(Some("2026:05:01 09:00:00"), Some("2020:01:01 00:00:00")),
    );
    write(
        &root,
        "x/tiff.jpg",
        &jpeg_with_exif(None, Some("2026:05:01 11:00:00")),
    );
    write(&root, "2026/01/01/nodate.jpg", b"");
    write(&root, "2026/02/nodate.jpg", b"");
    write(&root, "2025-07-04 Beach/nodate.jpg", b"");
    write(
        &root,
        "Lisbon/dated.jpg",
        &jpeg_with_exif(Some("2026:03:03 12:00:00"), None),
    );
    write(&root, "Lisbon/nodate.jpg", b"");
    write(&root, "Porto/nodate.jpg", b"");
    write(&root, "rootnodate.jpg", b"");
    write(&root, "x/old.mp4", &mp4_with_mvhd(0));
    let (items, groups) = run(&root);
    let get = |suffix: &str| {
        items
            .iter()
            .find(|i| i.path.ends_with(suffix))
            .unwrap()
            .clone()
    };
    let local = |s: &str| {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
            .unwrap()
            .and_local_timezone(chrono::Local)
            .unwrap()
            .with_timezone(&Utc)
    };

    assert_eq!(get("x/dto.jpg").taken_at, local("2026-05-01 09:00:00"));
    assert_eq!(get("x/tiff.jpg").taken_at, local("2026-05-01 11:00:00"));
    let f = get("2026/01/01/nodate.jpg");
    assert_eq!(
        (f.date_source, f.is_undated, f.taken_at),
        (
            Some(DateSource::Folder),
            false,
            local("2026-01-01 12:00:00")
        )
    );
    assert_eq!(
        get("2026/02/nodate.jpg").taken_at,
        local("2026-02-01 12:00:00")
    );
    assert_eq!(
        get("Beach/nodate.jpg").taken_at,
        local("2025-07-04 12:00:00")
    );
    assert_eq!(
        get("Lisbon/nodate.jpg").date_source,
        Some(DateSource::Unknown)
    );
    assert!(get("x/old.mp4").is_undated);

    let group_of = |suffix: &str| {
        groups
            .iter()
            .find(|g| g.item_paths.iter().any(|p| p.ends_with(suffix)))
            .unwrap()
    };
    assert_eq!(
        group_of("Lisbon/nodate.jpg").id,
        group_of("Lisbon/dated.jpg").id
    );
    assert_eq!(group_of("Porto/nodate.jpg").title, "Porto");
    assert_eq!(
        group_of("Porto/nodate.jpg").id,
        format!("folder-{}", unswiper_core::group::fnv1a("Porto"))
    );
    let undated = group_of("rootnodate.jpg");
    assert_eq!(undated.id, "undated");
    assert_eq!(undated.item_paths.len(), 1);
    assert_eq!(groups.last().unwrap().id, "undated");
    // x/old.mp4 has no usable date and x has no dated... x has dated siblings → joins them.
    assert_eq!(group_of("x/old.mp4").id, group_of("x/dto.jpg").id);
}

#[test]
#[cfg_attr(
    windows,
    ignore = "known Windows path gap, see .github/workflows/rust.yml windows job"
)]
fn decisions_kept_progress_reported_and_errors() {
    let d = tempfile::tempdir().unwrap();
    let root = canon(d.path());
    for i in 0..120 {
        write(&root, &format!("f/{i:03}.jpg"), b"");
    }
    let keep = format!("{}/f/007.jpg", root.display());
    let decisions = HashMap::from([(keep.clone(), Decision::Keep)]);
    let mut ticks: Vec<IndexProgress> = vec![];
    let (items, _) = index(&root, 36.0, &decisions, &MetadataDates, |p| ticks.push(p)).unwrap();
    assert_eq!(
        items.iter().find(|i| i.path == keep).unwrap().decision,
        Decision::Keep
    );
    assert!(ticks.len() >= 3);
    assert_eq!(ticks.last().unwrap().media_found, 120);

    let err = index(
        &root.join("missing"),
        36.0,
        &HashMap::new(),
        &MetadataDates,
        |_| {},
    )
    .unwrap_err();
    assert!(matches!(err, IndexError::UnreadableRoot(_)));
    assert_eq!(err.to_string(), "Could not read the selected folder.");
}

#[test]
#[cfg_attr(
    windows,
    ignore = "known Windows path gap, see .github/workflows/rust.yml windows job"
)]
fn relative_folder_path_cases() {
    use unswiper_core::index::relative_folder_path;
    let d = tempfile::tempdir().unwrap();
    let root = canon(d.path());
    std::fs::create_dir_all(root.join("a/b")).unwrap();
    assert_eq!(relative_folder_path(&root, &root), "");
    assert_eq!(relative_folder_path(&root.join("a/b"), &root), "a/b");
    assert_eq!(
        relative_folder_path(std::path::Path::new("/elsewhere"), &root),
        ""
    );
}

#[test]
#[cfg_attr(
    windows,
    ignore = "known Windows path gap, see .github/workflows/rust.yml windows job"
)]
fn sidecar_order_matches_swift_sidecars_then_mov() {
    let d = tempfile::tempdir().unwrap();
    let root = canon(d.path());
    write(&root, "IMG_9.HEIC", b"h");
    write(&root, "IMG_9.MOV", b"m");
    write(&root, "IMG_9.xmp", b"x");
    let (items, _) = run(&root);
    let side: Vec<_> = items[0]
        .sidecars
        .iter()
        .map(|s| s.path.rsplit('/').next().unwrap().to_string())
        .collect();
    assert_eq!(side, vec!["IMG_9.xmp", "IMG_9.MOV"]);
}

/// JPEG with only a GPS timestamp (no DateTimeOriginal/DateTime), like many phone exports.
fn jpeg_with_gps_time(day: &str, hms: [u32; 3]) -> Vec<u8> {
    use exif::experimental::Writer;
    use exif::{Field, In, Rational, Tag, Value};
    let r = |num| Rational { num, denom: 1 };
    let fields = [
        Field {
            tag: Tag::GPSDateStamp,
            ifd_num: In::PRIMARY,
            value: Value::Ascii(vec![day.as_bytes().to_vec()]),
        },
        Field {
            tag: Tag::GPSTimeStamp,
            ifd_num: In::PRIMARY,
            value: Value::Rational(hms.iter().map(|x| r(*x)).collect()),
        },
    ];
    let mut w = Writer::new();
    for f in &fields {
        w.push_field(f);
    }
    let mut tiff = std::io::Cursor::new(vec![]);
    w.write(&mut tiff, false).unwrap();
    let tiff = tiff.into_inner();
    let mut j = vec![0xFF, 0xD8, 0xFF, 0xE1];
    j.extend_from_slice(&((tiff.len() + 8) as u16).to_be_bytes());
    j.extend_from_slice(b"Exif\0\0");
    j.extend_from_slice(&tiff);
    j.extend_from_slice(&[0xFF, 0xD9]);
    j
}

/// Plan §20.14: capture date, then GPS time, then the file name, then the folder.
#[test]
fn date_fallbacks_gps_then_file_name_then_folder() {
    let d = tempfile::tempdir().unwrap();
    let root = canon(d.path());
    write(
        &root,
        "Trip/a.jpg",
        &jpeg_with_gps_time("2024:06:12", [13, 32, 41]),
    );
    write(&root, "Trip/IMG_20240613_101500.jpg", b"x");
    // A GPS time wins over the name.
    write(
        &root,
        "Trip/IMG_20200101_000000.jpg",
        &jpeg_with_gps_time("2024:06:14", [8, 0, 0]),
    );
    // A capture date wins over everything.
    write(
        &root,
        "2023-01-01 Old/PXL_20240615_120000000.jpg",
        &jpeg_with_exif(Some("2024:06:16 09:00:00"), None),
    );
    write(&root, "2023-01-01 Old/PXL_20240617_120000000.jpg", b"x");
    write(&root, "2023-01-01 Old/IMG_1234.jpg", b"x");
    let (items, _) = run(&root);
    let by = |n: &str| items.iter().find(|i| i.path.ends_with(n)).unwrap();
    let iso = |n: &str| unswiper_core::time::format_iso(&by(n).taken_at);
    assert_eq!(by("a.jpg").date_source, Some(DateSource::Gps));
    assert_eq!(iso("a.jpg"), "2024-06-12T13:32:41Z");
    assert_eq!(
        by("IMG_20240613_101500.jpg").date_source,
        Some(DateSource::FileName)
    );
    assert!(!by("IMG_20240613_101500.jpg").is_undated);
    assert_eq!(
        by("IMG_20200101_000000.jpg").date_source,
        Some(DateSource::Gps)
    );
    assert_eq!(
        by("PXL_20240615_120000000.jpg").date_source,
        Some(DateSource::Capture)
    );
    assert_eq!(
        by("PXL_20240617_120000000.jpg").date_source,
        Some(DateSource::FileName)
    );
    assert_eq!(
        iso("PXL_20240617_120000000.jpg"),
        "2024-06-17T12:00:00Z",
        "Pixel names are UTC"
    );
    assert_eq!(by("IMG_1234.jpg").date_source, Some(DateSource::Folder));
    let json = serde_json::to_string(&by("IMG_20240613_101500.jpg").date_source).unwrap();
    assert_eq!(json, "\"filename\"");
}
