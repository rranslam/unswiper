// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Shared core for Unswiper (Linux GTK app, CLI, and later the macOS app through UniFFI).
//!
//! This is a port of the Swift logic in `apps/macos/Sources/Unswiper` (v0.1.2):
//! the library model and `library.json` schema v1, the tree indexer with Live Photo
//! pairing, capture/folder dates and the Undated folder-context rule, visit grouping
//! (identical FNV-1a IDs), commit validation, the pending commit journal, and restore.
//! See `docs/linux-port-plan.md` (rev 3).

pub mod apple_json;
pub mod commit;
pub mod geo;
pub mod group;
pub mod index;
pub mod meta;
pub mod model;
pub mod path_util;
pub mod quality;
pub mod store;
pub mod time;
pub mod trash;
pub mod watch;

pub use model::*;
