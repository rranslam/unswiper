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
