// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! The sharpness score (plan §15.1).

use serde::{Deserialize, Serialize};

/// Images are scored at this long edge (smaller images are not upscaled).
pub const WORK_EDGE: u32 = 1024;
/// Tile grid is GRID × GRID.
pub const GRID: usize = 4;
/// Tiles whose intensity standard deviation is below this are "flat" and skipped.
pub const FLAT_STD: f32 = 6.0;
/// Below this mean luma (0–255) a photo is too dark to judge.
pub const DARK_LUMA: f32 = 40.0;

/// Grayscale working image, values 0–255.
#[derive(Debug, Clone, PartialEq)]
pub struct Gray {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl Gray {
    pub fn new(width: usize, height: usize, data: Vec<f32>) -> Gray {
        assert_eq!(width * height, data.len(), "buffer size");
        Gray {
            width,
            height,
            data,
        }
    }

    /// From any decoded image: grayscale, shrunk to [`WORK_EDGE`], Gaussian σ=1.
    pub fn prepare(img: &image::DynamicImage) -> Gray {
        let (w, h) = (img.width(), img.height());
        let long = w.max(h);
        let img = if long > WORK_EDGE {
            let s = WORK_EDGE as f64 / long as f64;
            let nw = ((w as f64 * s).round() as u32).max(1);
            let nh = ((h as f64 * s).round() as u32).max(1);
            img.resize_exact(nw, nh, image::imageops::FilterType::Triangle)
        } else {
            img.clone()
        };
        let l = img.to_luma8();
        let g = Gray::new(
            l.width() as usize,
            l.height() as usize,
            l.into_raw().into_iter().map(f32::from).collect(),
        );
        g.gaussian(1.0)
    }

    fn at(&self, x: isize, y: isize) -> f32 {
        let x = x.clamp(0, self.width as isize - 1) as usize;
        let y = y.clamp(0, self.height as isize - 1) as usize;
        self.data[y * self.width + x]
    }

    /// Separable Gaussian blur (edges clamped).
    pub fn gaussian(&self, sigma: f32) -> Gray {
        if sigma <= 0.0 || self.data.is_empty() {
            return self.clone();
        }
        let r = (sigma * 3.0).ceil() as isize;
        let mut k: Vec<f32> = (-r..=r)
            .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
            .collect();
        let sum: f32 = k.iter().sum();
        k.iter_mut().for_each(|v| *v /= sum);
        let (w, h) = (self.width, self.height);
        let mut tmp = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                tmp[y * w + x] = (-r..=r)
                    .map(|i| k[(i + r) as usize] * self.at(x as isize + i, y as isize))
                    .sum();
            }
        }
        let t = Gray::new(w, h, tmp);
        let mut out = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                out[y * w + x] = (-r..=r)
                    .map(|i| k[(i + r) as usize] * t.at(x as isize, y as isize + i))
                    .sum();
            }
        }
        Gray::new(w, h, out)
    }
}

/// One photo's quality numbers.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    /// p90 of per-tile Laplacian variance over textured tiles (higher = sharper).
    pub sharpness: f32,
    /// Laplacian variance of the whole frame.
    pub global: f32,
    /// Gradient-orientation anisotropy 0–1 (high = streaks in one direction, e.g. shake).
    pub motion: f32,
    pub mean_luma: f32,
    /// 0–1. Below 0.5 the photo is never suggested (too dark, almost no texture).
    pub confidence: f32,
}

fn variance(v: &[f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let n = v.len() as f64;
    let mean = v.iter().map(|&x| x as f64).sum::<f64>() / n;
    (v.iter().map(|&x| (x as f64 - mean).powi(2)).sum::<f64>() / n) as f32
}

/// p-th percentile (0–1) by nearest rank.
pub fn percentile(v: &[f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let idx = ((p * s.len() as f32).ceil() as usize).clamp(1, s.len()) - 1;
    s[idx]
}

/// Scores a prepared image.
pub fn score(g: &Gray) -> Score {
    let (w, h) = (g.width, g.height);
    if w < 3 || h < 3 {
        return Score {
            sharpness: 0.0,
            global: 0.0,
            motion: 0.0,
            mean_luma: g.data.iter().sum::<f32>() / g.data.len().max(1) as f32,
            confidence: 0.0,
        };
    }
    // Laplacian (4-neighbour) and gradients on the interior.
    let mut lap = vec![0.0f32; w * h];
    let (mut sxx, mut syy, mut sxy) = (0.0f64, 0.0f64, 0.0f64);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let c = g.data[y * w + x];
            let l = g.data[y * w + x - 1];
            let r = g.data[y * w + x + 1];
            let u = g.data[(y - 1) * w + x];
            let d = g.data[(y + 1) * w + x];
            lap[y * w + x] = l + r + u + d - 4.0 * c;
            let gx = ((r - l) / 2.0) as f64;
            let gy = ((d - u) / 2.0) as f64;
            sxx += gx * gx;
            syy += gy * gy;
            sxy += gx * gy;
        }
    }
    let interior = |x: usize, y: usize| x >= 1 && y >= 1 && x < w - 1 && y < h - 1;
    let mut tile_scores = vec![];
    for ty in 0..GRID {
        for tx in 0..GRID {
            let (x0, x1) = (tx * w / GRID, (tx + 1) * w / GRID);
            let (y0, y1) = (ty * h / GRID, (ty + 1) * h / GRID);
            let mut px = vec![];
            let mut lp = vec![];
            for y in y0..y1 {
                for x in x0..x1 {
                    px.push(g.data[y * w + x]);
                    if interior(x, y) {
                        lp.push(lap[y * w + x]);
                    }
                }
            }
            if variance(&px).sqrt() >= FLAT_STD && !lp.is_empty() {
                tile_scores.push(variance(&lp));
            }
        }
    }
    let all_lap: Vec<f32> = (1..h - 1)
        .flat_map(|y| (1..w - 1).map(move |x| (x, y)))
        .map(|(x, y)| lap[y * w + x])
        .collect();
    let mean_luma = g.data.iter().sum::<f32>() / g.data.len() as f32;
    let tr = sxx + syy;
    let motion = if tr > 0.0 {
        (((sxx - syy).powi(2) + 4.0 * sxy * sxy).sqrt() / tr) as f32
    } else {
        0.0
    };
    let mut confidence: f32 = 1.0;
    if tile_scores.len() < 2 {
        confidence = 0.0; // almost no texture: can't tell sharp from soft
    }
    if mean_luma < DARK_LUMA {
        confidence = confidence.min(0.2);
    }
    Score {
        sharpness: percentile(&tile_scores, 0.9),
        global: variance(&all_lap),
        motion,
        mean_luma,
        confidence,
    }
}

/// Convenience: prepare + score.
pub fn score_image(img: &image::DynamicImage) -> Score {
    score(&Gray::prepare(img))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth;

    #[test]
    fn percentile_edges() {
        assert_eq!(percentile(&[], 0.9), 0.0);
        assert_eq!(percentile(&[3.0], 0.9), 3.0);
        let v: Vec<f32> = (1..=10).map(|x| x as f32).collect();
        assert_eq!(percentile(&v, 0.9), 9.0);
        assert_eq!(percentile(&v, 0.0), 1.0);
        assert_eq!(percentile(&v, 1.0), 10.0);
    }

    #[test]
    fn flat_and_tiny_images_have_no_confidence() {
        let flat = Gray::new(64, 64, vec![128.0; 64 * 64]);
        let s = score(&flat);
        assert_eq!(s.sharpness, 0.0);
        assert_eq!(s.confidence, 0.0);
        assert_eq!(s.motion, 0.0);
        let tiny = Gray::new(2, 2, vec![10.0; 4]);
        assert_eq!(score(&tiny).confidence, 0.0);
        assert_eq!(flat.gaussian(0.0), flat);
    }

    #[test]
    #[should_panic(expected = "buffer size")]
    fn gray_checks_size() {
        Gray::new(2, 2, vec![0.0; 3]);
    }

    #[test]
    fn blur_lowers_score() {
        let sharp = synth::scene(7, 512, 384);
        let s0 = score_image(&sharp);
        let s1 = score_image(&synth::defocus(&sharp, 2.0));
        let s2 = score_image(&synth::defocus(&sharp, 4.0));
        assert!(s0.sharpness > s1.sharpness && s1.sharpness > s2.sharpness);
        assert!(s0.global > s2.global);
        assert_eq!(s0.confidence, 1.0);
    }

    #[test]
    fn motion_blur_is_anisotropic() {
        let sharp = synth::scene(3, 512, 384);
        let m = score_image(&synth::motion(&sharp, 21));
        let s = score_image(&sharp);
        assert!(
            m.motion > s.motion + 0.2,
            "motion {} vs {}",
            m.motion,
            s.motion
        );
    }

    #[test]
    fn dark_photos_are_low_confidence() {
        let dark = synth::darken(&synth::scene(5, 400, 300), 0.15);
        let s = score_image(&dark);
        assert!(s.mean_luma < DARK_LUMA);
        assert!(s.confidence < 0.5);
    }

    #[test]
    fn resolution_invariance() {
        let big = synth::scene(11, 2400, 1600);
        let small = big.resize_exact(1600, 1067, image::imageops::FilterType::Triangle);
        let (a, b) = (score_image(&big).sharpness, score_image(&small).sharpness);
        assert!((a - b).abs() / a.max(b) < 0.10, "{a} vs {b}");
    }

    mod prop {
        use super::*;
        use proptest::prelude::*;
        proptest! {
            #![proptest_config(ProptestConfig::with_cases(12))]
            #[test]
            fn more_blur_never_scores_higher(seed in 0u64..1000, s1 in 0.5f32..2.0, extra in 0.5f32..3.0) {
                let img = synth::scene(seed, 256, 192);
                let a = score_image(&synth::defocus(&img, s1)).sharpness;
                let b = score_image(&synth::defocus(&img, s1 + extra)).sharpness;
                prop_assert!(b <= a * 1.001, "{} then {}", a, b);
            }
        }
    }
}
