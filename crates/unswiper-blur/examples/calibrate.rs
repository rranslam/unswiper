//! Calibration on a folder of real photos (never committed; see docs/blur-evaluation.md).
//!
//! `cargo run --release -p unswiper-blur --example calibrate -- <folder> [blurry-list.txt]`
//!
//! Every photo not named in the optional list (one file name per line; `?name` marks a photo
//! left out of the counts) is treated as sharp. Prints, per sensitivity, false positives on the
//! sharp photos, recall on the listed blurry ones, and recall on synthetic blur added to the
//! sharp photos (Gaussian defocus and horizontal/vertical shake at the 1024 px working size).
//! Bursts are ignored (each photo scored on its own) so the numbers show the absolute rules.
use std::collections::HashMap;
use std::path::Path;
use unswiper_blur::decide::{suggest, Candidate, Thresholds};
use unswiper_core::quality::Sensitivity;
use unswiper_blur::{decode::decode, score::score_image, synth};

fn flagged(scores: &[(String, unswiper_blur::score::Score)], s: Sensitivity) -> Vec<bool> {
    let cands: Vec<Candidate> = scores
        .iter()
        .map(|(p, sc)| Candidate {
            path: p.clone(),
            taken_at: None,
            score: *sc,
        })
        .collect();
    suggest(&cands, Thresholds::for_sensitivity(s))
        .iter()
        .map(Option::is_some)
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = Path::new(args.get(1).expect("usage: calibrate <folder> [blurry-list.txt]"));
    let mut label: HashMap<String, char> = HashMap::new();
    if let Some(list) = args.get(2) {
        for l in std::fs::read_to_string(list).unwrap().lines() {
            let l = l.trim();
            if let Some(n) = l.strip_prefix('?') {
                label.insert(n.to_string(), '?');
            } else if !l.is_empty() && !l.starts_with('#') {
                label.insert(l.to_string(), 'B');
            }
        }
    }
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg")))
        .collect();
    files.sort();
    let (mut real, mut synth_sets): (Vec<_>, Vec<Vec<_>>) = (vec![], vec![vec![]; 4]);
    let names = ["defocus σ2", "defocus σ4", "shake 15 px", "shake 31 px"];
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let Some(img) = decode(f) else { continue };
        let sc = score_image(&img);
        let l = *label.get(&name).unwrap_or(&'S');
        real.push((name.clone(), sc, l));
        if l == 'S' {
            let small = img.resize(1024, 1024, image::imageops::FilterType::Triangle);
            let vertical = |len| synth::motion(&small.rotate90(), len).rotate270();
            let blurred = [
                synth::defocus(&small, 2.0),
                synth::defocus(&small, 4.0),
                if name.len() % 2 == 0 { synth::motion(&small, 15) } else { vertical(15) },
                if name.len() % 2 == 0 { vertical(31) } else { synth::motion(&small, 31) },
            ];
            for (k, b) in blurred.iter().enumerate() {
                synth_sets[k].push((name.clone(), score_image(b)));
            }
        }
    }
    let sharp = real.iter().filter(|r| r.2 == 'S').count();
    let blurry = real.iter().filter(|r| r.2 == 'B').count();
    println!("{} photos: {sharp} sharp, {blurry} blurry, {} left out", real.len(), real.len() - sharp - blurry);
    for s in [Sensitivity::Low, Sensitivity::Normal, Sensitivity::High] {
        let scores: Vec<_> = real.iter().map(|r| (r.0.clone(), r.1)).collect();
        let fl = flagged(&scores, s);
        let fp: Vec<_> = real.iter().zip(&fl).filter(|(r, f)| **f && r.2 == 'S').map(|(r, _)| r.0.as_str()).collect();
        let tp = real.iter().zip(&fl).filter(|(r, f)| **f && r.2 == 'B').count();
        print!(
            "{s:?}: FP {}/{sharp} ({:.1}%), real recall {tp}/{blurry}",
            fp.len(),
            100.0 * fp.len() as f64 / sharp.max(1) as f64
        );
        for (k, set) in synth_sets.iter().enumerate() {
            let n = flagged(set, s).iter().filter(|f| **f).count();
            print!(", {} {n}/{}", names[k], set.len());
        }
        println!("\n  FP: {fp:?}");
    }
    if std::env::var_os("CALIB_DUMP").is_some() {
        for r in &real {
            println!("dump real {} {} {:.2} {:.2} {:.3}", r.2, r.0, r.1.sharpness, r.1.motion, r.1.confidence);
        }
        for (k, set) in synth_sets.iter().enumerate() {
            for (n, sc) in set {
                println!("dump synth{k} {n} {:.2} {:.2} {:.3}", sc.sharpness, sc.motion, sc.confidence);
            }
        }
    }
    if std::env::var_os("CALIB_DETAIL").is_some() {
        for (k, set) in synth_sets.iter().enumerate() {
            let fl = flagged(set, Sensitivity::Normal);
            for ((n, sc), f) in set.iter().zip(fl) {
                if !f {
                    println!("miss {} {n} sharp={:.1} motion={:.2} conf={:.2}", names[k], sc.sharpness, sc.motion, sc.confidence);
                }
            }
        }
    }
}
