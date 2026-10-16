// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Unpacking synthetic Takeout archives (no personal data): layout, sidecars, zip-slip, symlinks,
//! multi-part exports, resuming and conflicts.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use unswiper_core::index::{index, MetadataDates};
use unswiper_takeout::unpack::{PART_SUFFIX, STATE_FILE};
use unswiper_takeout::{
    find_archives, find_parts, plan, unpack, unpack_with, TakeoutDates, UnpackError, UnpackOptions,
};

const P: &str = "Takeout/Google Photos";

fn sidecar(ts: i64) -> Vec<u8> {
    format!(r#"{{"title":"x","photoTakenTime":{{"timestamp":"{ts}"}},"geoData":{{"latitude":48.85,"longitude":2.29,"altitude":0.0}}}}"#)
        .into_bytes()
}

/// A zip with the given (name, bytes) files; names are written verbatim (even unsafe ones).
fn make_zip(path: &Path, files: &[(&str, Vec<u8>)]) {
    let mut z = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in files {
        z.start_file(*name, opts).unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
}

/// A .tgz with the given entries; names go straight into the header (no sanitising by `tar`).
fn make_tgz(path: &Path, files: &[(&str, Vec<u8>)], links: &[(&str, &str)]) {
    let gz =
        flate2::write::GzEncoder::new(fs::File::create(path).unwrap(), flate2::Compression::fast());
    let mut t = tar::Builder::new(gz);
    for (name, data) in files {
        let mut h = tar::Header::new_gnu();
        h.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name.as_bytes());
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_mtime(1_561_190_459);
        h.set_entry_type(tar::EntryType::Regular);
        h.set_cksum();
        t.append(&h, &data[..]).unwrap();
    }
    for (name, target) in links {
        let mut h = tar::Header::new_gnu();
        h.as_gnu_mut().unwrap().name[..name.len()].copy_from_slice(name.as_bytes());
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_link_name(target).unwrap();
        h.set_size(0);
        h.set_cksum();
        t.append(&h, &[][..]).unwrap();
    }
    t.into_inner().unwrap().finish().unwrap();
}

fn photos_export() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            format!("{P}/Photos from 2019/IMG_0001.jpg"),
            b"jpeg one".to_vec(),
        ),
        (
            format!("{P}/Photos from 2019/IMG_0001.jpg.supplemental-metadata.json"),
            sidecar(1_561_190_459),
        ),
        (
            format!("{P}/Photos from 2019/IMG_0001-edited.jpg"),
            b"jpeg one edited".to_vec(),
        ),
        (
            format!("{P}/Photos from 2019/IMG_0001(1).jpg"),
            b"another one".to_vec(),
        ),
        (
            format!("{P}/Photos from 2019/IMG_0001.jpg.supplemental-metadata(1).json"),
            sidecar(1_561_200_000),
        ),
        (
            format!("{P}/Trip to the Coast/IMG_0001.jpg"),
            b"jpeg one".to_vec(),
        ),
        (
            format!("{P}/Trip to the Coast/metadata.json"),
            br#"{"title":"Trip to the Coast"}"#.to_vec(),
        ),
        (
            "Takeout/archive_browser.html".to_string(),
            b"<html>".to_vec(),
        ),
        ("Takeout/Google Fit/data.csv".to_string(), b"a,b".to_vec()),
    ]
}

fn refs(v: &[(String, Vec<u8>)]) -> Vec<(&str, Vec<u8>)> {
    v.iter().map(|(n, d)| (n.as_str(), d.clone())).collect()
}

#[test]
fn zip_export_unpacks_and_indexes() {
    let d = tempfile::tempdir().unwrap();
    let arch = d.path().join("takeout-20261015T000000Z-001.zip");
    make_zip(&arch, &refs(&photos_export()));
    let dest = d.path().join("Google Photos Import");
    let r = unpack(&[arch], &dest, |_| {}).unwrap();
    assert_eq!(r.extracted, 6);
    assert_eq!(
        r.duplicates_skipped, 1,
        "the album copy of the year-folder photo"
    );
    assert!(r.refused.is_empty() && r.conflicts.is_empty());
    assert_eq!(r.other_products.iter().collect::<Vec<_>>(), ["Google Fit"]);
    // Google's names are kept exactly; Takeout/Google Photos/ is stripped.
    assert!(dest.join("Photos from 2019/IMG_0001(1).jpg").is_file());
    assert!(dest.join("Trip to the Coast/metadata.json").is_file());
    assert!(!dest.join("Trip to the Coast/IMG_0001.jpg").exists());
    assert!(!dest.join("archive_browser.html").exists());
    assert!(!dest.join("Takeout").exists());
    assert!(dest.join(STATE_FILE).is_file());

    let dates = TakeoutDates::new(MetadataDates);
    let (items, _groups) = index(&dest, 36.0, &HashMap::new(), &dates, |_| {}).unwrap();
    assert_eq!(
        items.len(),
        3,
        "3 media files; JSON and state files are not media"
    );
    let ts = |suffix: &str| {
        items
            .iter()
            .find(|i| i.path.ends_with(suffix))
            .unwrap()
            .taken_at
            .timestamp()
    };
    assert_eq!(ts("Photos from 2019/IMG_0001.jpg"), 1_561_190_459);
    assert_eq!(ts("IMG_0001-edited.jpg"), 1_561_190_459);
    assert_eq!(ts("IMG_0001(1).jpg"), 1_561_200_000);
    let g = dates
        .location(&dest.join("Photos from 2019/IMG_0001.jpg"))
        .unwrap();
    assert_eq!((g.lat, g.lon), (48.85, 2.29));
}

#[test]
fn tgz_export_unpacks() {
    let d = tempfile::tempdir().unwrap();
    let arch = d.path().join("takeout-20261015T000000Z-001.tgz");
    make_tgz(&arch, &refs(&photos_export()), &[]);
    let dest = d.path().join("out");
    let opts = UnpackOptions {
        skip_duplicates: false,
        ..Default::default()
    };
    let r = unpack_with(&[arch], &dest, &opts, |_| {}).unwrap();
    assert_eq!(
        r.extracted, 7,
        "album copies are kept when duplicates aren't skipped"
    );
    assert!(dest.join("Trip to the Coast/IMG_0001.jpg").is_file());
    assert_eq!(
        fs::read(dest.join("Photos from 2019/IMG_0001-edited.jpg")).unwrap(),
        b"jpeg one edited"
    );
}

fn tree(dir: &Path) -> Vec<String> {
    let mut out = vec![];
    fn walk(d: &Path, root: &Path, out: &mut Vec<String>) {
        for e in fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            out.push(p.strip_prefix(root).unwrap().to_string_lossy().into_owned());
            if e.file_type().unwrap().is_dir() {
                walk(&p, root, out);
            }
        }
    }
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn zip_slip_and_links_are_refused() {
    let d = tempfile::tempdir().unwrap();
    let outside = d.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let evil = [
        ("../evil.jpg", b"x".to_vec()),
        ("Takeout/Google Photos/../../../evil2.jpg", b"x".to_vec()),
        ("/tmp/evil3.jpg", b"x".to_vec()),
        ("Takeout\\Google Photos\\..\\..\\evil4.jpg", b"x".to_vec()),
        ("C:/evil5.jpg", b"x".to_vec()),
        ("Takeout/Google Photos/ok/IMG.jpg", b"fine".to_vec()),
    ];
    let z = d.path().join("evil.zip");
    make_zip(&z, &evil);
    let dest = d.path().join("dest");
    let r = unpack(std::slice::from_ref(&z), &dest, |_| {}).unwrap();
    assert_eq!(r.refused.len(), 5, "{:?}", r.refused);
    assert_eq!(r.extracted, 1);
    assert_eq!(tree(&dest), ["ok", "ok/IMG.jpg", STATE_FILE]);
    assert!(tree(d.path())
        .iter()
        .all(|p| !p.contains("evil") || p == "evil.zip"));

    // Symlinks in a tgz are refused, and a symlink already in dest is never followed.
    let t = d.path().join("links.tgz");
    make_tgz(
        &t,
        &[
            ("Takeout/Google Photos/sneaky/IMG.jpg", b"x".to_vec()),
            ("../evil6.jpg", b"x".to_vec()),
        ],
        &[("Takeout/Google Photos/link.jpg", "/etc/passwd")],
    );
    let dest2 = d.path().join("dest2");
    fs::create_dir(&dest2).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, dest2.join("sneaky")).unwrap();
    let res = unpack(&[t], &dest2, |_| {});
    #[cfg(unix)]
    {
        assert!(matches!(res, Err(UnpackError::Write { .. })), "{res:?}");
        assert_eq!(
            fs::read_dir(&outside).unwrap().count(),
            0,
            "nothing written through the link"
        );
    }
    #[cfg(not(unix))]
    let _ = res;
}

#[test]
fn multi_part_exports() {
    let d = tempfile::tempdir().unwrap();
    let all = photos_export();
    let (a, b) = all.split_at(4);
    let p1 = d.path().join("takeout-20261015T000000Z-001.zip");
    let p2 = d.path().join("takeout-20261015T000000Z-002.zip");
    make_zip(&p1, &refs(a));
    make_zip(&p2, &refs(b));
    fs::write(
        d.path().join("takeout-20250101T000000Z-001.zip"),
        b"other export",
    )
    .unwrap();
    fs::write(
        d.path().join("takeout-20261015T000000Z-003.tgz"),
        b"other format",
    )
    .unwrap();
    assert_eq!(find_parts(&p2), vec![p1.clone(), p2.clone()]);
    let single = d.path().join("photos.zip");
    assert_eq!(find_parts(&single), vec![single.clone()]);
    let dest = d.path().join("dest");
    let r = unpack(&find_parts(&p1), &dest, |_| {}).unwrap();
    assert_eq!(r.extracted, 6);
    // The sidecar of IMG_0001(1).jpg is in part 2, the photo in part 1: matching still works.
    let dates = TakeoutDates::new(MetadataDates);
    let (items, _) = index(&dest, 36.0, &HashMap::new(), &dates, |_| {}).unwrap();
    let dup = items
        .iter()
        .find(|i| i.path.ends_with("IMG_0001(1).jpg"))
        .unwrap();
    assert_eq!(dup.taken_at.timestamp(), 1_561_200_000);
}

#[test]
fn resume_and_conflicts() {
    let d = tempfile::tempdir().unwrap();
    let arch = d.path().join("t.zip");
    make_zip(&arch, &refs(&photos_export()));
    let dest = d.path().join("dest");
    let year = dest.join("Photos from 2019");
    fs::create_dir_all(&year).unwrap();
    // An interrupted earlier run: one finished file, one half-written part.
    fs::write(year.join("IMG_0001.jpg"), b"jpeg one").unwrap();
    fs::write(
        year.join(format!("IMG_0001-edited.jpg{PART_SUFFIX}")),
        b"jpeg o",
    )
    .unwrap();
    // A different file of the user's with the same name as an export file.
    fs::write(year.join("IMG_0001(1).jpg"), b"the user's own, longer file").unwrap();
    let mut ticks = vec![];
    let r = unpack(std::slice::from_ref(&arch), &dest, |p| ticks.push(p)).unwrap();
    assert_eq!(r.already_there, 1);
    assert_eq!(r.extracted, 4);
    assert_eq!(r.duplicates_skipped, 1);
    assert_eq!(r.conflicts, ["Photos from 2019/IMG_0001(1).jpg"]);
    assert_eq!(
        fs::read(year.join("IMG_0001(1).jpg")).unwrap(),
        b"the user's own, longer file"
    );
    assert_eq!(
        fs::read(year.join("IMG_0001-edited.jpg")).unwrap(),
        b"jpeg one edited"
    );
    assert!(tree(&dest).iter().all(|p| !p.ends_with(PART_SUFFIX)));
    assert_eq!(ticks.last().unwrap().archives, 1);
    // Running again skips the finished archive entirely.
    let r2 = unpack(&[arch], &dest, |_| {}).unwrap();
    assert_eq!((r2.archives_skipped, r2.extracted), (1, 0));
}

#[test]
fn not_a_photos_export() {
    let d = tempfile::tempdir().unwrap();
    let z = d.path().join("mail.zip");
    make_zip(&z, &[("Takeout/Mail/All.mbox", b"From x".to_vec())]);
    let err = unpack(&[z], &d.path().join("dest"), |_| {}).unwrap_err();
    assert!(
        matches!(&err, UnpackError::NoPhotos(s) if s == "Mail"),
        "{err}"
    );
    let bad: PathBuf = d.path().join("photos.rar");
    fs::write(&bad, b"x").unwrap();
    assert!(matches!(
        unpack(&[bad], &d.path().join("d2"), |_| {}),
        Err(UnpackError::UnknownFormat(_))
    ));
    let broken = d.path().join("broken.zip");
    fs::write(&broken, b"PK not really").unwrap();
    assert!(matches!(
        unpack(&[broken], &d.path().join("d3"), |_| {}),
        Err(UnpackError::Archive { .. })
    ));
}

fn big(n: usize, seed: u8) -> Vec<u8> {
    (0..n)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

#[test]
fn dry_run_reports_sizes_and_space() {
    let d = tempfile::tempdir().unwrap();
    let a = d.path().join("takeout-1-001.zip");
    let b = d.path().join("takeout-1-002.tgz");
    make_zip(
        &a,
        &[
            (&format!("{P}/Photos from 2020/A.jpg"), big(1000, 1)),
            (
                &format!("{P}/Photos from 2020/A.jpg.supplemental-metadata.json"),
                sidecar(1_600_000_000),
            ),
            ("Takeout/Google Fit/x.csv", big(500, 2)),
        ],
    );
    make_tgz(
        &b,
        &[(&format!("{P}/Photos from 2020/B.mp4"), big(3000, 3))],
        &[],
    );
    let dest = d.path().join("dest");
    let opts = UnpackOptions::default();
    let p = plan(&[a.clone(), b.clone()], &dest, &opts, true).unwrap();
    let json = sidecar(1_600_000_000).len() as u64;
    assert_eq!((p.items, p.new_items), (2, 2));
    assert_eq!(p.photos_bytes, 4000 + json, "Google Fit is not counted");
    assert_eq!(p.new_bytes, 4000 + json);
    assert!(p.archives.iter().all(|x| x.exact));
    assert!(p.same_disk);
    assert_eq!(p.needed_keep, 4000 + json);
    // Deleting archive 1 frees its size before archive 2 is written.
    let a_size = fs::metadata(&a).unwrap().len();
    assert_eq!(p.needed_delete, (1000 + json).max(4000 + json - a_size));
    assert!(p.free_bytes.is_some());
    assert!(!dest.exists(), "a dry run writes nothing");
    // Without exact, the .tgz is estimated from its file size.
    let q = plan(&[a, b.clone()], &dest, &opts, false).unwrap();
    assert!(!q.archives[1].exact);
    assert_eq!(q.archives[1].new_bytes, fs::metadata(&b).unwrap().len());
}

#[test]
fn not_enough_space_is_refused_before_writing() {
    let d = tempfile::tempdir().unwrap();
    let a = d.path().join("t.zip");
    make_zip(&a, &refs(&photos_export()));
    let dest = d.path().join("dest");
    let opts = UnpackOptions {
        reserve_bytes: u64::MAX / 2,
        ..Default::default()
    };
    let err = unpack_with(&[a], &dest, &opts, |_| {}).unwrap_err();
    assert!(matches!(err, UnpackError::NotEnoughSpace { .. }), "{err}");
    assert!(!dest.exists());
}

#[test]
fn folder_import_with_archive_deletion_and_progress() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("downloads");
    fs::create_dir(&src).unwrap();
    let all = photos_export();
    let (x, y) = all.split_at(4);
    make_zip(&src.join("takeout-1-001.zip"), &refs(x));
    make_tgz(&src.join("takeout-1-002.tgz"), &refs(y), &[]);
    make_zip(
        &src.join("mail.zip"),
        &[("Takeout/Mail/All.mbox", b"From x".to_vec())],
    );
    fs::write(src.join("notes.txt"), b"not an archive").unwrap();
    let archives = find_archives(&src).unwrap();
    assert_eq!(archives.len(), 3);
    let dest = d.path().join("dest");
    let opts = UnpackOptions {
        delete_archives: true,
        ..Default::default()
    };
    let mut ticks = vec![];
    let r = unpack_with(&archives, &dest, &opts, |t| ticks.push(t)).unwrap();
    assert_eq!(r.extracted, 6);
    assert_eq!(r.archives_deleted.len(), 2);
    // Only the photo archives are deleted; the Mail export and other files stay.
    let mut left: Vec<_> = fs::read_dir(&src)
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    left.sort();
    assert_eq!(left, ["mail.zip", "notes.txt"]);
    let last = ticks.last().unwrap();
    assert_eq!((last.archive, last.archives), (3, 3));
    assert_eq!(last.bytes, r.bytes_written);
    assert!(last.bytes_total >= last.bytes, "{last:?}");
    assert!(matches!(
        find_archives(&dest),
        Err(UnpackError::NoArchives(_))
    ));
}

#[test]
fn incremental_reimport_only_adds_new_items() {
    let d = tempfile::tempdir().unwrap();
    let dest = d.path().join("dest");
    let first = d.path().join("takeout-20260101T000000Z-001.zip");
    make_zip(&first, &refs(&photos_export()));
    unpack(&[first], &dest, |_| {}).unwrap();
    // Rob reviews and deletes a reject locally.
    fs::remove_file(dest.join("Photos from 2019/IMG_0001(1).jpg")).unwrap();
    // Months later a scheduled Takeout re-exports everything plus one new photo.
    let mut next = photos_export();
    next.push((format!("{P}/Photos from 2026/NEW.jpg"), big(2000, 9)));
    next.push((
        format!("{P}/Photos from 2026/NEW.jpg.supplemental-metadata.json"),
        sidecar(1_780_000_000),
    ));
    let second = d.path().join("takeout-20260701T000000Z-001.zip");
    make_zip(&second, &refs(&next));
    let p = plan(
        std::slice::from_ref(&second),
        &dest,
        &UnpackOptions::default(),
        true,
    )
    .unwrap();
    assert_eq!(p.new_items, 1);
    assert_eq!(p.new_bytes, 2000 + sidecar(1_780_000_000).len() as u64);
    let r = unpack(&[second], &dest, |_| {}).unwrap();
    assert_eq!(r.extracted, 2, "only the new photo and its sidecar");
    assert_eq!(
        r.previously_imported, 1,
        "the deleted reject is not brought back"
    );
    assert!(!dest.join("Photos from 2019/IMG_0001(1).jpg").exists());
    assert!(dest.join("Photos from 2026/NEW.jpg").is_file());
}
