// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! `unswiper-blur scan --progress` (the Mac app's progress bar): progress lines on stderr,
//! the same JSON on stdout, and nothing scored twice.

use std::process::Command;
use unswiper_blur::synth;

#[test]
fn scan_progress_lines_and_cached_rerun() {
    let d = tempfile::tempdir().unwrap();
    let photos = d.path().join("photos");
    std::fs::create_dir(&photos).unwrap();
    let s = synth::scene(5, 300, 200);
    for i in 0..5 {
        s.to_rgb8()
            .save(photos.join(format!("IMG_{i}.jpg")))
            .unwrap();
    }
    let cache = d.path().join("c.json");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_unswiper-blur"))
            .args(["scan", photos.to_str().unwrap(), "--json", "--progress"])
            .args(["--write-cache", cache.to_str().unwrap()])
            .output()
            .unwrap()
    };
    let o = run();
    assert!(o.status.success());
    let err = String::from_utf8_lossy(&o.stderr);
    let lines: Vec<&str> = err.lines().filter(|l| l.starts_with("progress ")).collect();
    assert_eq!(lines.first(), Some(&"progress 0 5"), "{err}");
    assert_eq!(lines.last(), Some(&"progress 5 5"), "{err}");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(rows.len(), 5);
    // Everything cached: nothing left to score.
    let o = run();
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.lines().all(|l| l == "progress 0 0"), "{err}");
}
