// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Headless smoke test: starts the real app on a display (CI runs it under `xvfb-run`),
//! indexes a tiny library, opens the first visit and quits on its own.
//! Skipped when there is no display.

use std::process::Command;
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_unswiper");

#[test]
fn version_and_help_selftest() {
    let out = Command::new(BIN).arg("--version").output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("unswiper "));
    let out = Command::new(BIN).arg("--help-selftest").output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new(BIN).arg("--bogus").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn launches_indexes_and_quits() {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("no display; skipping (run under xvfb-run)");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let lib = d.path().join("lib");
    std::fs::create_dir(&lib).unwrap();
    // A minimal valid 1x1 PNG, so the thumbnail path runs too.
    let png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52, 0, 0,
        0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 13, 0x49, 0x44, 0x41,
        0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0, 0x05, 0x00, 0x01, 0xFF, 0x89,
        0x99, 0x3D, 0x1D, 0, 0, 0, 0, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    for n in ["a.png", "b.png"] {
        std::fs::write(lib.join(n), png).unwrap();
    }
    let support = d.path().join("support");
    let start = Instant::now();
    let out = Command::new(BIN)
        .args([
            "--library-dir",
            support.to_str().unwrap(),
            "--open",
            lib.to_str().unwrap(),
            "--open-first-visit",
            "--quit-after",
            "4",
        ])
        .env("XDG_CACHE_HOME", d.path().join("cache"))
        .env("GTK_A11Y", "none")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(start.elapsed() < Duration::from_secs(60));
    let json =
        std::fs::read_to_string(support.join("library.json")).expect("library.json saved on quit");
    assert!(json.contains("a.png") && json.contains("b.png"), "{json}");
}

#[test]
fn viewer_plays_a_video() {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("no display; skipping (run under xvfb-run)");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let lib = d.path().join("lib");
    std::fs::create_dir(&lib).unwrap();
    let Some(_clip) = unswiper_gtk::video::make_test_clip(&lib) else {
        eprintln!("no GStreamer encoder; skipping");
        return;
    };
    let support = d.path().join("support");
    let out = Command::new(BIN)
        .args([
            "--library-dir",
            support.to_str().unwrap(),
            "--open",
            lib.to_str().unwrap(),
            "--open-first-visit",
            "--present-viewer",
            "--quit-after",
            "8",
        ])
        .env("XDG_CACHE_HOME", d.path().join("cache"))
        .env("GTK_A11Y", "none")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let ms: u64 = stdout
        .lines()
        .find_map(|l| l.strip_prefix("viewer: prepared duration_ms="))
        .unwrap_or_else(|| panic!("viewer never prepared: {stdout}"))
        .trim()
        .parse()
        .unwrap();
    assert!(ms > 0, "{stdout}");
}

/// A JPEG whose EXIF names a camera, exposure and a location (no pixels needed for the info).
fn exif_jpeg() -> Vec<u8> {
    use exif::experimental::Writer;
    use exif::{Field, In, Rational, Tag, Value};
    let f = |tag, value| Field {
        tag,
        ifd_num: In::PRIMARY,
        value,
    };
    let r = |num, denom| Rational { num, denom };
    let fields = [
        f(Tag::Make, Value::Ascii(vec![b"Canon".to_vec()])),
        f(Tag::Model, Value::Ascii(vec![b"Canon EOS R6".to_vec()])),
        f(Tag::ExposureTime, Value::Rational(vec![r(1, 250)])),
        f(Tag::FNumber, Value::Rational(vec![r(28, 10)])),
        f(Tag::PhotographicSensitivity, Value::Short(vec![400])),
        f(
            Tag::DateTimeOriginal,
            Value::Ascii(vec![b"2026:03:14 10:11:12".to_vec()]),
        ),
        f(
            Tag::OffsetTimeOriginal,
            Value::Ascii(vec![b"-07:00".to_vec()]),
        ),
        f(Tag::GPSLatitudeRef, Value::Ascii(vec![b"N".to_vec()])),
        f(
            Tag::GPSLatitude,
            Value::Rational(vec![r(37, 1), r(30, 1), r(0, 1)]),
        ),
        f(Tag::GPSLongitudeRef, Value::Ascii(vec![b"W".to_vec()])),
        f(
            Tag::GPSLongitude,
            Value::Rational(vec![r(122, 1), r(15, 1), r(0, 1)]),
        ),
    ];
    let mut w = Writer::new();
    for x in &fields {
        w.push_field(x);
    }
    let mut tiff = std::io::Cursor::new(vec![]);
    w.write(&mut tiff, false).unwrap();
    let tiff = tiff.into_inner();
    let mut j = vec![0xFF, 0xD8, 0xFF, 0xE1];
    j.extend_from_slice(&((tiff.len() + 8) as u16).to_be_bytes());
    j.extend_from_slice(b"Exif\0\0");
    j.extend_from_slice(&tiff);
    j.extend_from_slice(&[
        0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x30, 0x00, 0x40, 0x01, 0x01,
    ]);
    j.extend_from_slice(&[0x11, 0x00, 0xFF, 0xD9]);
    j
}

fn run_info(lib: &std::path::Path, support: &std::path::Path, cache: &std::path::Path) -> String {
    let out = Command::new(BIN)
        .args([
            "--library-dir",
            support.to_str().unwrap(),
            "--open",
            lib.to_str().unwrap(),
            "--open-first-visit",
            "--present-info",
            "--quit-after",
            "6",
        ])
        .env("XDG_CACHE_HOME", cache)
        .env("GTK_A11Y", "none")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn photo_info_panel_and_popover() {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("no display; skipping (run under xvfb-run)");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let lib = d.path().join("lib");
    std::fs::create_dir(&lib).unwrap();
    std::fs::write(lib.join("IMG_0001.JPG"), exif_jpeg()).unwrap();
    let support = d.path().join("support");
    let cache = d.path().join("cache");
    let stdout = run_info(&lib, &support, &cache);
    for want in [
        "info: Camera | Camera | Canon EOS R6",
        "info: Exposure | Shutter | 1/250 s",
        "info: Exposure | Aperture | f/2.8",
        "info: Exposure | ISO | ISO 400",
        "info: Date | Taken | Sat 14 Mar 2026, 10:11:12 (UTC-07:00)",
        "info: Image | Dimensions | 0.1 MP · 64 × 48",
        "info: File | Format | JPEG",
        "info: Location | Coordinates | 37.50000, -122.25000",
        "info: panel open",
    ] {
        assert!(stdout.contains(want), "missing {want:?} in:\n{stdout}");
    }
    let n: usize = stdout
        .lines()
        .find_map(|l| l.strip_prefix("info: popover rows="))
        .unwrap_or_else(|| panic!("no popover: {stdout}"))
        .parse()
        .unwrap();
    assert!(n >= 6, "{stdout}");
    // Privacy: the location is never written to library.json.
    let json = std::fs::read_to_string(support.join("library.json")).unwrap();
    assert!(!json.contains("37.5") && !json.contains("122.25"), "{json}");

    // With "Show Location in Photo Info" off, no Location rows.
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    v["config"]["infoShowLocation"] = serde_json::Value::Bool(false);
    std::fs::write(support.join("library.json"), v.to_string()).unwrap();
    let stdout = run_info(&lib, &support, &cache);
    assert!(
        stdout.contains("info: Camera | Camera | Canon EOS R6"),
        "{stdout}"
    );
    assert!(!stdout.contains("info: Location"), "{stdout}");
}

/// The Google Photos import assistant end to end on a synthetic Takeout export: check, unpack
/// (deleting the archive), open and index the library with the dates from Google's JSON files.
#[test]
fn takeout_assistant_imports_and_indexes() {
    use std::io::Write;
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("no display; skipping (run under xvfb-run)");
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let downloads = d.path().join("downloads");
    std::fs::create_dir(&downloads).unwrap();
    let png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52, 0, 0,
        0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 13, 0x49, 0x44, 0x41,
        0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0, 0x05, 0x00, 0x01, 0xFF, 0x89,
        0x99, 0x3D, 0x1D, 0, 0, 0, 0, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    let p = "Takeout/Google Photos";
    let files: Vec<(String, Vec<u8>)> = vec![
        (
            format!("{p}/Photos from 2019/Screenshot_1.png"),
            png.to_vec(),
        ),
        (
            format!("{p}/Photos from 2019/Screenshot_1.png.supplemental-metadata.json"),
            br#"{"title":"Screenshot_1.png","photoTakenTime":{"timestamp":"1561190459"}}"#.to_vec(),
        ),
        (format!("{p}/Trip/Screenshot_1.png"), png.to_vec()),
    ];
    let archive = downloads.join("takeout-20261019T000000Z-001.zip");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    for (n, data) in &files {
        z.start_file(n.as_str(), zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
    let support = d.path().join("support");
    std::fs::create_dir(&support).unwrap();
    // Delete archives after unpacking (remembered setting, as if ticked earlier).
    std::fs::write(
        support.join("takeout-setup.json"),
        br#"{"step":5,"deleteArchives":true}"#,
    )
    .unwrap();
    let lib = d.path().join("GooglePhotosLibrary");
    let out = Command::new(BIN)
        .args([
            "--library-dir",
            support.to_str().unwrap(),
            "--present-takeout",
            downloads.to_str().unwrap(),
            lib.to_str().unwrap(),
            "--quit-after",
            "8",
        ])
        .env("XDG_CACHE_HOME", d.path().join("cache"))
        .env("GTK_A11Y", "none")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("takeout: plan archives=1 items=2 new_items=1 fits=true"),
        "{stdout}"
    );
    assert!(
        stdout.contains("takeout: unpacked extracted=2 duplicates_skipped=1 deleted=1"),
        "{stdout}"
    );
    assert!(
        stdout.contains("takeout: indexed items=1 visits=1"),
        "{stdout}"
    );
    assert!(!archive.exists(), "archive deleted after unpacking");
    assert!(lib.join("Photos from 2019/Screenshot_1.png").is_file());
    let json = std::fs::read_to_string(support.join("library.json")).unwrap();
    assert!(
        json.contains("2019-06-22T08:00:59"),
        "date from the sidecar: {json}"
    );
    let setup = std::fs::read_to_string(support.join("takeout-setup.json")).unwrap();
    assert!(setup.contains("GooglePhotosLibrary"), "{setup}");
}
