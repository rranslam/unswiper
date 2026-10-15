// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Which scored photos to suggest as blurry (plan §15.1 "Decision"). Conservative: a false
//! "blurry" costs a good photo, so both stages are tuned for precision.

use crate::score::Score;
use unswiper_core::quality::Sensitivity;

/// Photos taken within this many seconds of each other form a burst.
pub const BURST_GAP_SECS: i64 = 3;
/// A burst needs at least this many photos.
pub const BURST_MIN: usize = 3;
/// Photos with lower confidence are never suggested.
pub const MIN_CONFIDENCE: f32 = 0.5;

/// Thresholds for one sensitivity level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thresholds {
    /// Absolute floor: sharpness below this is blurry anywhere.
    pub abs: f32,
    /// Relative: below this fraction of the burst's sharpest photo is blurry.
    pub rel: f32,
}

impl Thresholds {
    pub fn for_sensitivity(s: Sensitivity) -> Thresholds {
        match s {
            Sensitivity::Low => Thresholds {
                abs: 8.0,
                rel: 0.25,
            },
            Sensitivity::Normal => Thresholds {
                abs: 15.0,
                rel: 0.35,
            },
            Sensitivity::High => Thresholds {
                abs: 30.0,
                rel: 0.45,
            },
        }
    }

    /// Normal sensitivity with a custom absolute floor (CLI `--threshold`).
    pub fn with_abs(abs: f32) -> Thresholds {
        Thresholds {
            abs,
            ..Thresholds::for_sensitivity(Sensitivity::Normal)
        }
    }
}

/// One photo to judge.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub path: String,
    /// Capture time, Unix seconds. `None` for undated photos, which never form bursts.
    pub taken_at: Option<i64>,
    pub score: Score,
}

/// Why a photo was suggested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Reason {
    /// Below the absolute floor.
    Soft,
    /// Much softer than the sharpest photo of its burst.
    Burst,
    /// Streaked in one direction (camera shake) and fairly soft.
    Shake,
}

/// Gradient anisotropy at or above this counts as directional streaking.
pub const MOTION_ANISOTROPY: f32 = 0.5;
/// Shaky photos are flagged below this multiple of the absolute floor.
pub const MOTION_FLOOR_FACTOR: f32 = 4.0;

/// Returns, for each candidate (same order), `Some(reason)` when it should be suggested.
pub fn suggest(cands: &[Candidate], th: Thresholds) -> Vec<Option<Reason>> {
    // Bursts: runs of photos ≤ BURST_GAP_SECS apart, in time order.
    let mut order: Vec<usize> = (0..cands.len())
        .filter(|&i| cands[i].taken_at.is_some())
        .collect();
    order.sort_by_key(|&i| (cands[i].taken_at, i));
    let t = |i: usize| cands[i].taken_at.unwrap_or_default();
    let mut burst_max = vec![None; cands.len()];
    let mut start = 0;
    while start < order.len() {
        let mut end = start + 1;
        while end < order.len() && t(order[end]) - t(order[end - 1]) <= BURST_GAP_SECS {
            end += 1;
        }
        if end - start >= BURST_MIN {
            let max = order[start..end]
                .iter()
                .map(|&i| cands[i].score)
                .filter(|s| s.confidence >= MIN_CONFIDENCE)
                .map(|s| s.sharpness)
                .fold(0.0f32, f32::max);
            for &i in &order[start..end] {
                burst_max[i] = Some(max);
            }
        }
        start = end;
    }
    cands
        .iter()
        .zip(burst_max)
        .map(|(c, max)| {
            let s = c.score;
            if s.confidence < MIN_CONFIDENCE {
                None
            } else if s.sharpness < th.abs {
                Some(Reason::Soft)
            } else if s.motion >= MOTION_ANISOTROPY && s.sharpness < th.abs * MOTION_FLOOR_FACTOR {
                Some(Reason::Shake)
            } else if max.is_some_and(|m| s.sharpness < th.rel * m) {
                Some(Reason::Burst)
            } else {
                None
            }
        })
        .collect()
}

/// File types that come from cameras. Others (PNG screenshots, GIFs, BMPs) are never
/// suggested (plan §15.1 "never flag").
pub fn is_camera_format(path: &str) -> bool {
    let ext = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "heic" | "heif" | "tif" | "tiff" | "webp"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(path: &str, t: i64, sharp: f32, conf: f32) -> Candidate {
        Candidate {
            path: path.into(),
            taken_at: Some(t),
            score: Score {
                sharpness: sharp,
                global: sharp,
                motion: 0.0,
                mean_luma: 120.0,
                confidence: conf,
            },
        }
    }

    #[test]
    fn absolute_floor_and_confidence() {
        let t = Thresholds::for_sensitivity(Sensitivity::Normal);
        let r = suggest(
            &[
                c("a", 0, 5.0, 1.0),
                c("b", 100, 500.0, 1.0),
                c("c", 200, 1.0, 0.2),
            ],
            t,
        );
        assert_eq!(r, vec![Some(Reason::Soft), None, None]);
        let mut shaky = c("s", 0, 40.0, 1.0);
        shaky.score.motion = 0.7;
        let mut crisp = shaky.clone();
        crisp.score.sharpness = 400.0;
        assert_eq!(suggest(&[shaky, crisp], t), vec![Some(Reason::Shake), None]);
    }

    #[test]
    fn burst_relative_rule() {
        let t = Thresholds::for_sensitivity(Sensitivity::Normal);
        // Out of order on purpose; 3 photos within 2 s of each other.
        let cands = [
            c("b", 1002, 100.0, 1.0),
            c("a", 1000, 400.0, 1.0),
            c("c", 1004, 300.0, 1.0),
            c("far", 5000, 100.0, 1.0),
        ];
        let r = suggest(&cands, t);
        assert_eq!(r, vec![Some(Reason::Burst), None, None, None]);
        // Two photos are not a burst.
        let r = suggest(&cands[..2], t);
        assert_eq!(r, vec![None, None]);
        // Low-confidence frames don't set the burst max.
        let r = suggest(
            &[
                c("a", 0, 100.0, 1.0),
                c("b", 1, 100.0, 1.0),
                c("d", 2, 9000.0, 0.0),
            ],
            t,
        );
        assert_eq!(r, vec![None, None, None]);
        // Undated photos never form a burst.
        let mut und = vec![
            c("a", 0, 100.0, 1.0),
            c("b", 0, 100.0, 1.0),
            c("d", 0, 900.0, 1.0),
        ];
        und.iter_mut().for_each(|c| c.taken_at = None);
        assert_eq!(suggest(&und, t), vec![None, None, None]);
    }

    #[test]
    fn sensitivity_orders_thresholds() {
        let l = Thresholds::for_sensitivity(Sensitivity::Low);
        let n = Thresholds::for_sensitivity(Sensitivity::Normal);
        let h = Thresholds::for_sensitivity(Sensitivity::High);
        assert!(l.abs < n.abs && n.abs < h.abs);
        assert!(l.rel < n.rel && n.rel < h.rel);
        assert_eq!(Thresholds::with_abs(3.0).abs, 3.0);
        assert_eq!(Thresholds::with_abs(3.0).rel, n.rel);
    }

    #[test]
    fn camera_formats() {
        assert!(is_camera_format("/a/IMG_1.HEIC"));
        assert!(is_camera_format("x.jpeg"));
        assert!(!is_camera_format("Screenshot.png"));
        assert!(!is_camera_format("noext"));
    }
}
