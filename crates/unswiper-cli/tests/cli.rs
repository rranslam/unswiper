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
