// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Accuracy gate (plan §15.6) on a labeled synthetic set generated from deterministic
//! scenes: sharp, defocus, motion, bokeh-sharp, night-sharp, low-texture-sharp, faint (low-contrast) sharp and bursts.
//! Gate: precision ≥ 0.95, recall ≥ 0.70, zero flags on bokeh-sharp / night-sharp /
//! low-texture-sharp. Calibration on Rob's real photos is a follow-up (see blur docs).

use unswiper_blur::decide::{suggest, Candidate, Thresholds};
use unswiper_blur::score::score_image;
use unswiper_blur::synth;
use unswiper_core::quality::Sensitivity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Label {
    Sharp,
    Defocus,
    Motion,
    BokehSharp,
    NightSharp,
    NightBlurry,
    LowTextureSharp,
    FaintSharp,
}

impl Label {
    fn blurry(self) -> bool {
        matches!(self, Label::Defocus | Label::Motion | Label::NightBlurry)
    }
    fn must_not_flag(self) -> bool {
        matches!(
            self,
            Label::BokehSharp | Label::NightSharp | Label::LowTextureSharp | Label::FaintSharp
        )
    }
}

fn fixture_set() -> Vec<(String, i64, Label, image::DynamicImage)> {
    let (w, h) = (384, 288);
    let mut v = vec![];
    let mut t = 0i64;
    let mut push = |name: String, label: Label, img: image::DynamicImage, gap: i64| {
        t += gap;
        v.push((name, t, label, img));
    };
    for seed in 0..12u64 {
        let s = synth::scene(seed, w, h);
        push(format!("sharp-{seed}"), Label::Sharp, s.clone(), 3600);
        push(
            format!("defocus3-{seed}"),
            Label::Defocus,
            synth::defocus(&s, 3.0),
            3600,
        );
        push(
            format!("defocus5-{seed}"),
            Label::Defocus,
            synth::defocus(&s, 5.0),
            3600,
        );
        push(
            format!("motion-{seed}"),
            Label::Motion,
            synth::motion(&s, 25),
            3600,
        );
        push(
            format!("bokeh-{seed}"),
            Label::BokehSharp,
            synth::bokeh(&s),
            3600,
        );
        push(
            format!("night-{seed}"),
            Label::NightSharp,
            synth::darken(&s, 0.18),
            3600,
        );
        push(
            format!("nightblur-{seed}"),
            Label::NightBlurry,
            synth::darken(&synth::defocus(&s, 4.0), 0.18),
            3600,
        );
        push(
            format!("faint-{seed}"),
            Label::FaintSharp,
            synth::faint(&s, 0.15),
            3600,
        );
        push(
            format!("lowtex-{seed}"),
            Label::LowTextureSharp,
            synth::low_texture(seed, w, h),
            3600,
        );
    }
    // Bursts: 3 frames 1 s apart; the middle one slightly soft (relative rule).
    for seed in 100..106u64 {
        let s = synth::scene(seed, w, h);
        push(format!("burst-{seed}-a"), Label::Sharp, s.clone(), 3600);
        push(
            format!("burst-{seed}-b"),
            Label::Defocus,
            synth::defocus(&s, 1.6),
            1,
        );
        push(format!("burst-{seed}-c"), Label::Sharp, s.clone(), 1);
    }
    v
}

#[test]
fn accuracy_gate() {
    let set = fixture_set();
    let cands: Vec<Candidate> = set
        .iter()
        .map(|(n, t, _, img)| Candidate {
            path: n.clone(),
            taken_at: Some(*t),
            score: score_image(img),
        })
        .collect();
    let flags = suggest(&cands, Thresholds::for_sensitivity(Sensitivity::Normal));
    let (mut tp, mut fp, mut fn_) = (0, 0, 0);
    let mut report = String::new();
    let mut bad: Vec<String> = vec![];
    for ((c, (_, _, label, _)), f) in cands.iter().zip(&set).zip(&flags) {
        let flagged = f.is_some();
        // Night-blurry is unjudgeable by design (dark → low confidence): not counted as a miss.
        let counted_blurry = label.blurry() && *label != Label::NightBlurry;
        match (flagged, counted_blurry) {
            (true, true) => tp += 1,
            (true, false) => fp += 1,
            (false, true) => fn_ += 1,
            _ => {}
        }
        if flagged && label.must_not_flag() {
            bad.push(c.path.clone());
        }
        report.push_str(&format!(
            "{:<18} {:?} sharp={:.1} motion={:.2} conf={} -> {:?}\n",
            c.path, label, c.score.sharpness, c.score.motion, c.score.confidence, f
        ));
    }
    let precision = tp as f64 / (tp + fp).max(1) as f64;
    let recall = tp as f64 / (tp + fn_).max(1) as f64;
    eprintln!("{report}precision {precision:.3} recall {recall:.3} (tp {tp} fp {fp} fn {fn_})");
    assert!(bad.is_empty(), "must-not-flag photos flagged: {bad:?}");
    assert!(precision >= 0.95, "precision {precision}");
    assert!(recall >= 0.70, "recall {recall}");
}
