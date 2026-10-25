// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

use std::process::Command;

fn cli(args: &[&str]) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_unswiper-cli"))
        .args(args)
        .output()
        .unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).into(),
        String::from_utf8_lossy(&o.stderr).into(),
    )
}

#[test]
fn version_usage_and_errors() {
    assert!(cli(&["--version"]).1.starts_with("unswiper-cli "));
    let (ok, _, err) = cli(&[]);
    assert!(!ok && err.contains("usage:"));
    assert!(!cli(&["commit", "x.json"]).0);
    assert!(!cli(&["groups", "/nonexistent.json"]).0);
    assert!(!cli(&["index", "/nonexistent-dir"]).0);
}

#[test]
fn index_groups_dry_run_restore() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("lib");
    std::fs::create_dir_all(root.join("2026-01-01 Lisbon")).unwrap();
    std::fs::write(root.join("2026-01-01 Lisbon/a.jpg"), b"aa").unwrap();
    std::fs::write(root.join("b.jpg"), b"b").unwrap();
    let (ok, out, _) = cli(&["index", root.to_str().unwrap(), "--gap-hours", "12"]);
    assert!(ok);
    assert!(out.starts_with("2 items in 2 visits"), "{out}");
    assert!(out.contains("undated"));
    assert!(!cli(&["index", root.to_str().unwrap(), "--bogus"]).0);

    let (ok, json, _) = cli(&["index", root.to_str().unwrap(), "--json"]);
    assert!(ok);
    let lib = d.path().join("library.json");
    std::fs::write(&lib, json.replace("\"undecided\"", "\"reject\"")).unwrap();
    let (ok, out, _) = cli(&["groups", lib.to_str().unwrap()]);
    assert!(ok && out.contains("Lisbon"));
    let (ok, out, _) = cli(&["commit", "--dry-run", lib.to_str().unwrap()]);
    assert!(ok);
    assert_eq!(out.trim(), "would move 2 items (3 bytes)");
    // --visit limits the dry run to one visit.
    let lisbon = out_groups_id(&cli(&["groups", lib.to_str().unwrap()]).1, "Lisbon");
    let (ok, out, _) = cli(&[
        "commit",
        "--dry-run",
        "--visit",
        &lisbon,
        lib.to_str().unwrap(),
    ]);
    assert!(ok);
    assert_eq!(out.trim(), "would move 1 items (2 bytes)");
    let (ok, _, err) = cli(&[
        "commit",
        "--dry-run",
        "--visit",
        "nope",
        lib.to_str().unwrap(),
    ]);
    assert!(!ok && err.contains("no visit with id nope"), "{err}");
    assert!(!cli(&["commit", "--dry-run", "--visit"]).0);

    let m = d.path().join("m.json");
    std::fs::write(&m, r#"{"createdAt":"2026-10-12T10:00:00Z","entries":[{"originalPath":"/nonexistent/a.jpg","resultPath":"/nonexistent/q/a.jpg","sizeBytes":3}],"id":"x","mode":"quarantine"}"#).unwrap();
    let (ok, out, err) = cli(&["restore", m.to_str().unwrap()]);
    assert!(!ok && out.contains("failed 1") && err.contains("Moved file not found"));
    std::fs::write(
        &m,
        r#"{"createdAt":"2026-10-12T10:00:00Z","entries":[],"id":"x","mode":"quarantine"}"#,
    )
    .unwrap();
    assert!(cli(&["restore", m.to_str().unwrap()]).0);
}

fn out_groups_id(out: &str, title: &str) -> String {
    let line = out.lines().find(|l| l.contains(title)).expect(title);
    line.split_whitespace().next().unwrap().to_string()
}

/// A saved library with two made-up places (synthetic; 5 photos each, years apart).
fn geo_library(dir: &std::path::Path) -> std::path::PathBuf {
    let mut items = vec![];
    for (k, (day, lat, lon)) in [
        ("2019-06-01", 38.7223, -9.1393),
        ("2023-03-01", 44.0582, -121.3153),
    ]
    .iter()
    .enumerate()
    {
        for n in 0..5 {
            items.push(serde_json::json!({
                "path": format!("/lib/p{k}_{n}.jpg"),
                "kind": "photo",
                "sizeBytes": 10,
                "takenAt": format!("{day}T10:0{n}:00Z"),
                "relativeFolder": "lib",
                "decision": "undecided",
                "sidecars": [],
                "isUndated": false,
                "location": {"lat": lat + n as f64 * 0.0005, "lon": lon, "source": "exif"}
            }));
        }
    }
    let lib = serde_json::json!({
        "schemaVersion": 2,
        "config": {"keepTargetDefault": 10, "visitGapHours": 36.0, "commitMode": "trash"},
        "items": items,
        "groups": []
    });
    let p = dir.join("library.json");
    std::fs::write(&p, serde_json::to_string_pretty(&lib).unwrap()).unwrap();
    p
}

#[test]
fn group_mode_override_is_not_saved() {
    let d = tempfile::tempdir().unwrap();
    let lib = geo_library(d.path());
    let l = lib.to_str().unwrap();
    let before = std::fs::read(&lib).unwrap();
    // Saved mode: visit (none saved) → two time visits, no places.
    let (ok, out, _) = cli(&["groups", l, "--json"]);
    assert!(ok);
    assert!(!out.contains("\"place\""), "{out}");
    // Override for this run: geo → two places with names.
    let (ok, out, _) = cli(&["groups", l, "--json", "--group-mode", "geo"]);
    assert!(ok);
    let g: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(g.as_array().unwrap().len(), 2);
    assert_eq!(g[0]["place"]["name"], "Bend", "newest place first");
    assert_eq!(g[1]["place"]["name"], "Lisbon");
    // The file is untouched and still reads as visit.
    assert_eq!(std::fs::read(&lib).unwrap(), before);
    assert_eq!(cli(&["config", "get", "group-mode", l]).1.trim(), "visit");
    assert!(!cli(&["groups", l, "--group-mode", "sideways"]).0);
    assert!(!cli(&["groups", l, "--place-radius-km", "-2"]).0);
}

#[test]
fn config_set_group_mode_persists() {
    let d = tempfile::tempdir().unwrap();
    let lib = geo_library(d.path());
    let l = lib.to_str().unwrap();
    let (ok, out, _) = cli(&["config", "set", "group-mode", "visit-geo", l]);
    assert!(ok, "{out}");
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&lib).unwrap()).unwrap();
    assert_eq!(saved["config"]["groupMode"], "visit-geo");
    assert_eq!(saved["items"].as_array().unwrap().len(), 10, "items kept");
    assert_eq!(
        cli(&["config", "get", "group-mode", l]).1.trim(),
        "visit-geo"
    );
    // A visit-geo run titles each visit with its place.
    let (ok, out, _) = cli(&["groups", l]);
    assert!(
        ok && out.contains("Lisbon, PT") && out.contains("Bend, OR"),
        "{out}"
    );
    // A CLI flag still wins for one run.
    let (_, out, _) = cli(&["groups", l, "--group-mode", "visit"]);
    assert!(!out.contains("Lisbon, PT"));
    assert!(!cli(&["config", "set", "group-mode", "bogus", l]).0);
    // An unknown saved value reads as visit.
    let txt = std::fs::read_to_string(&lib)
        .unwrap()
        .replace("\"visit-geo\"", "\"bogus\"");
    std::fs::write(&lib, txt).unwrap();
    assert_eq!(cli(&["config", "get", "group-mode", l]).1.trim(), "visit");
}

#[test]
fn geo_debug_command() {
    let (ok, out, _) = cli(&["geo", "38.7223", "-9.1393"]);
    assert!(ok);
    assert_eq!(out.trim(), "Lisbon, PT");
    assert!(cli(&["geo", "0", "-30"]).1.contains("no place"));
    assert!(!cli(&["geo", "x"]).0);
}

#[test]
fn index_accepts_grouping_flags() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("lib");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.jpg"), b"aa").unwrap();
    let r = root.to_str().unwrap();
    let (ok, _, err) = cli(&["index", r, "--group-mode", "geo"]);
    assert!(ok, "{err}");
    let (ok, _, err) = cli(&["index", r, "--place-radius-km", "3"]);
    assert!(ok && err.contains("ignored in visit mode"));
}
