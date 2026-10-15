// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Blur / out-of-focus detection for Unswiper (plan §15). Pure Rust, offline.
//!
//! * [`score`]: decode → 1024 px grayscale → Gaussian σ=1 → 4×4 tiles → Laplacian variance;
//!   `sharpness` is the 90th percentile over textured tiles, so a sharp subject anywhere
//!   (bokeh) passes.
//! * [`decide`]: conservative two-stage suggestion (absolute floor + relative to the
//!   sharpest photo of a burst), never flagging low-confidence photos.
//! * [`cache`]: `quality-cache.json`, keyed by size + mtime + algorithm version.
//! * [`scan`]: parallel, cancellable scoring with the cache.
//! * [`cli`]: the `unswiper-blur` command.
//!
//! Suggestions are written into the library through `unswiper_core::quality`; nothing here
//! moves or deletes files.

pub mod cache;
pub mod cli;
pub mod decide;
pub mod decode;
pub mod scan;
pub mod score;
#[doc(hidden)]
pub mod synth;

/// Bump when the score changes; cached scores from other versions are recomputed.
pub const ALGO_VERSION: u32 = 1;
