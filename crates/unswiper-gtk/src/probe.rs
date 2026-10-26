// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Responsiveness probe for stress tests (`UNSWIPER_PROBE=1`). Measures how long the GTK main
//! loop goes without running a 5 ms timer and prints `probe: …` lines to stderr. Off by default;
//! it never reads or prints file names.

use std::cell::Cell;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

static START: OnceLock<Instant> = OnceLock::new();

thread_local! {
    static MAX_STALL: Cell<u128> = const { Cell::new(0) };
}

/// True when `UNSWIPER_PROBE` is set.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("UNSWIPER_PROBE").is_some())
}

fn secs() -> f64 {
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// Starts the main-loop timer (call once, on the main thread).
pub fn start() {
    if !enabled() {
        return;
    }
    secs();
    let last = Cell::new(Instant::now());
    relm4::gtk::glib::timeout_add_local(Duration::from_millis(5), move || {
        let gap = last.get().elapsed().as_millis();
        last.set(Instant::now());
        MAX_STALL.with(|m| m.set(m.get().max(gap)));
        if gap >= 200 {
            eprintln!("probe: t={:.2}s stall_ms={gap}", secs());
        }
        relm4::gtk::glib::ControlFlow::Continue
    });
}

/// Prints a milestone.
pub fn mark(what: &str) {
    if enabled() {
        eprintln!("probe: t={:.2}s {what}", secs());
    }
}

/// Reports one `update` that took long.
pub fn update(name: &str, took: Duration) {
    if enabled() && took.as_millis() >= 50 {
        eprintln!(
            "probe: t={:.2}s update={name} ms={}",
            secs(),
            took.as_millis()
        );
    }
}

/// Prints the worst stall seen.
pub fn finish() {
    if enabled() {
        eprintln!(
            "probe: t={:.2}s max_stall_ms={}",
            secs(),
            MAX_STALL.with(|m| m.get())
        );
    }
}
