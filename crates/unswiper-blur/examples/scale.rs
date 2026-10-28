// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Prints the sharpness score of synthetic photos (no real photos): sharp, defocused and
//! shaken scenes, and the same scene at different sizes, so the numbers on the blur scale
//! can be read in context. `cargo run --release -p unswiper-blur --example scale`

use unswiper_blur::score::score_image;
use unswiper_blur::synth;

fn median(mut v: Vec<f32>) -> f32 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn main() {
    let seeds = 1..=12u64;
    let row = |name: &str, f: &dyn Fn(u64) -> f32| {
        let v: Vec<f32> = seeds.clone().map(f).collect();
        let (lo, hi) = (
            v.iter().cloned().fold(f32::MAX, f32::min),
            v.iter().cloned().fold(0.0, f32::max),
        );
        println!(
            "{name:<34} median {:>8.1}   range {lo:>7.1} – {hi:>7.1}",
            median(v)
        );
    };
    println!("Scenes at 1024 × 768 (the working size), 12 seeds:");
    row("sharp", &|s| {
        score_image(&synth::scene(s, 1024, 768)).sharpness
    });
    for sigma in [1.0, 1.5, 2.0, 3.0, 4.0] {
        row(&format!("defocus σ {sigma}"), &|s| {
            score_image(&synth::defocus(&synth::scene(s, 1024, 768), sigma)).sharpness
        });
    }
    for len in [9, 15, 31] {
        row(&format!("shake {len} px"), &|s| {
            score_image(&synth::motion(&synth::scene(s, 1024, 768), len)).sharpness
        });
    }
    println!("\nSize: the same blur (σ 2 at the size shown), scored after the shrink to 1024:");
    for (w, h) in [(640, 480), (1024, 768), (2048, 1536), (4000, 3000)] {
        row(&format!("{w} × {h} sharp"), &|s| {
            score_image(&synth::scene(s, w, h)).sharpness
        });
        row(&format!("{w} × {h} defocus σ 2"), &|s| {
            score_image(&synth::defocus(&synth::scene(s, w, h), 2.0)).sharpness
        });
    }
}
