// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! The `takeout` command line and its JSON output (what the Mac app reads). Synthetic data only.
//! `tests/fixtures/helper/` holds sample output the Swift tests decode; this test fails if the
//! live output gains or loses keys, so the two sides can't drift apart.

use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::Path;
use unswiper_takeout::cli::{duration, human, plan_lines, run};
use unswiper_takeout::{plan, UnpackOptions};

const P: &str = "Takeout/Google Photos";

fn make_zip(path: &Path, files: &[(&str, &[u8])]) {
    let mut z = zip::ZipWriter::new(fs::File::create(path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    for (name, data) in files {
        z.start_file(*name, opts).unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap();
}

const SIDECAR: &[u8] = br#"{"title":"IMG_1.jpg","photoTakenTime":{"timestamp":"1561190459"},"geoData":{"latitude":48.85,"longitude":2.29,"altitude":0.0}}"#;

fn export(dir: &Path) {
    make_zip(
        &dir.join("takeout-20261015T000000Z-001.zip"),
        &[
            (&format!("{P}/Photos from 2019/IMG_1.jpg"), b"one"),
            (
                &format!("{P}/Photos from 2019/IMG_1.jpg.supplemental-metadata.json"),
                SIDECAR,
            ),
            (&format!("{P}/Trip/IMG_1.jpg"), b"one"),
        ],
    );
    make_zip(
        &dir.join("takeout-20261015T000000Z-002.zip"),
        &[(&format!("{P}/Photos from 2019/IMG_2.jpg"), b"two!")],
    );
}

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn call(v: &[&str]) -> (Result<(), String>, String, String) {
    let (mut out, mut err) = (vec![], vec![]);
    let r = run(&args(v), &mut out, &mut err);
    (
        r,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn lines(s: &str) -> Vec<Value> {
    s.lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// All key paths of a JSON value (`plan.archives[].items`, …).
fn keys(v: &Value, prefix: &str, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                let p = format!("{prefix}.{k}");
                out.insert(p.clone());
                keys(x, &p, out);
            }
        }
        Value::Array(a) => {
            for x in a {
                keys(x, &format!("{prefix}[]"), out);
            }
        }
        _ => {}
    }
}

fn same_keys(live: &Value, fixture_name: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/helper")
        .join(fixture_name);
    if std::env::var_os("UNSWIPER_WRITE_HELPER_FIXTURES").is_some() {
        fs::write(&path, serde_json::to_string_pretty(live).unwrap() + "\n").unwrap();
    }
    let fixture: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let (mut a, mut b) = (BTreeSet::new(), BTreeSet::new());
    keys(live, "", &mut a);
    keys(&fixture, "", &mut b);
    assert_eq!(
        a, b,
        "{fixture_name} is out of date (UNSWIPER_WRITE_HELPER_FIXTURES=1)"
    );
}

#[test]
fn dry_run_json_and_words() {
    let d = tempfile::tempdir().unwrap();
    export(d.path());
    let lib = d.path().join("GooglePhotosLibrary");
    let src = d.path().to_str().unwrap();
    let (r, out, _) = call(&[
        "import",
        src,
        "--to",
        lib.to_str().unwrap(),
        "--dry-run",
        "--json",
    ]);
    r.unwrap();
    let ev = lines(&out);
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0]["event"], "plan");
    assert_eq!(ev[0]["fits"], true);
    assert_eq!(ev[0]["plan"]["items"], 3);
    assert_eq!(
        ev[0]["plan"]["new_items"], 2,
        "the album copy is a duplicate"
    );
    assert_eq!(ev[0]["plan"]["archives"].as_array().unwrap().len(), 2);
    assert!(ev[0]["plan"]["seconds_max"].as_u64().unwrap() >= 1);
    same_keys(&ev[0], "plan.json");
    assert!(!lib.exists(), "a dry run writes nothing");

    let (r, out, _) = call(&["import", src, "--to", lib.to_str().unwrap(), "--dry-run"]);
    r.unwrap();
    assert!(out.contains("2 archives"), "{out}");
    assert!(out.contains("It fits"), "{out}");

    // --keep-duplicates counts the album copy as new.
    let (_, out, _) = call(&[
        "import",
        src,
        "--to",
        lib.to_str().unwrap(),
        "--dry-run",
        "--json",
        "--keep-duplicates",
    ]);
    assert_eq!(lines(&out)[0]["plan"]["new_items"], 3);
}

#[test]
fn progress_json_import_then_dates() {
    let d = tempfile::tempdir().unwrap();
    let src = d.path().join("downloads");
    fs::create_dir(&src).unwrap();
    export(&src);
    let lib = d.path().join("lib");
    let (r, out, err) = call(&[
        "import",
        src.to_str().unwrap(),
        "--to",
        lib.to_str().unwrap(),
        "--progress-json",
        "--delete-archives",
    ]);
    r.unwrap();
    assert!(err.is_empty(), "JSON mode writes nothing to stderr: {err}");
    let ev = lines(&out);
    assert_eq!(ev.first().unwrap()["event"], "plan");
    let progress: Vec<_> = ev.iter().filter(|e| e["event"] == "progress").collect();
    assert!(progress.len() >= 2);
    same_keys(progress[0], "progress.json");
    let done = ev.last().unwrap();
    assert_eq!(done["event"], "done");
    assert_eq!(done["report"]["extracted"], 3);
    assert_eq!(done["report"]["duplicates_skipped"], 1);
    assert_eq!(
        done["report"]["archives_deleted"].as_array().unwrap().len(),
        2
    );
    same_keys(done, "done.json");
    assert!(unswiper_takeout::is_takeout_library(&lib));

    let (r, out, _) = call(&["dates", lib.to_str().unwrap()]);
    r.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    let a = v.as_array().unwrap();
    assert_eq!(a.len(), 1, "only IMG_1 has a sidecar: {v}");
    assert!(a[0]["path"]
        .as_str()
        .unwrap()
        .ends_with("Photos from 2019/IMG_1.jpg"));
    assert_eq!(a[0]["taken"], 1_561_190_459);
    assert_eq!(a[0]["lat"], 48.85);
    same_keys(&a[0], "dates-entry.json");
}

#[test]
fn errors_are_json_events() {
    let d = tempfile::tempdir().unwrap();
    let empty = d.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let (r, out, _) = call(&[
        "import",
        empty.to_str().unwrap(),
        "--to",
        d.path().join("lib").to_str().unwrap(),
        "--progress-json",
    ]);
    assert!(r.is_err());
    let ev = lines(&out);
    assert_eq!(ev[0]["event"], "error");
    assert_eq!(ev[0]["kind"], "no_archives");
    same_keys(&ev[0], "error.json");
    let (r, _, _) = call(&["import", "x"]);
    assert!(r.unwrap_err().contains("--to"));
    assert!(call(&["bogus"]).0.is_err());
}

#[test]
fn words() {
    assert_eq!(human(999), "999 bytes");
    assert_eq!(human(1_500_000_000), "1.5 GB");
    assert_eq!(duration(30), "under a minute");
    assert_eq!(duration(61), "about 2 min");
    assert_eq!(duration(7_800), "about 2 h 10 min");
    let d = tempfile::tempdir().unwrap();
    export(d.path());
    let archives = unswiper_takeout::find_archives(d.path()).unwrap();
    let opts = UnpackOptions {
        reserve_bytes: u64::MAX / 4,
        ..Default::default()
    };
    let p = plan(&archives, &d.path().join("lib"), &opts, false).unwrap();
    let l = plan_lines(&p, &opts).join("\n");
    assert!(l.contains("NOT enough"), "{l}");
}

/// `tests/fixtures/helper/takeout-20261015T000000Z-001.zip`: a tiny synthetic export the Swift
/// end-to-end test unpacks with the real helper. Checked here so it can't silently change.
#[test]
fn helper_fixture_zip() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/helper/takeout-20261015T000000Z-001.zip");
    if std::env::var_os("UNSWIPER_WRITE_HELPER_FIXTURES").is_some() {
        make_zip(
            &fixture,
            &[
                (&format!("{P}/Photos from 2019/IMG_1.jpg"), b"one"),
                (
                    &format!("{P}/Photos from 2019/IMG_1.jpg.supplemental-metadata.json"),
                    SIDECAR,
                ),
                (&format!("{P}/Trip/IMG_1.jpg"), b"one"),
                ("Takeout/archive_browser.html", b"<html></html>"),
            ],
        );
    }
    let d = tempfile::tempdir().unwrap();
    let lib = d.path().join("lib");
    let r = unswiper_takeout::unpack(&[fixture], &lib, |_| {}).unwrap();
    assert_eq!((r.extracted, r.duplicates_skipped), (2, 1));
    let dates = unswiper_takeout::library_dates(&lib);
    assert_eq!(dates.len(), 1);
    assert_eq!(dates[0].taken, Some(1_561_190_459));
}
