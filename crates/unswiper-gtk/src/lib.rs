// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Unswiper for Linux. The GUI logic lives in display-free modules ([`state`], [`help`],
//! [`thumbs`], [`format`]) so it can be unit-tested headless; [`ui`] is the thin relm4 layer.

pub mod blur;
pub mod format;
pub mod help;
pub mod icons;
pub mod info;
pub mod player;
pub mod state;
pub mod thumbs;
pub mod ui;
pub mod video;

/// Application ID (Linux only; the macOS bundle ID is decided separately).
pub const APP_ID: &str = "io.github.rranslam.Unswiper";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const WEBSITE: &str = "https://github.com/rranslam/unswiper";
pub const ISSUES: &str = "https://github.com/rranslam/unswiper/issues";
