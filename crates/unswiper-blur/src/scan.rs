// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Scoring many photos: parallel (N−1 threads), cancellable, cache-aware, and turning
//! cached scores into suggestions for `unswiper_core::quality::apply_suggestions`.

use crate::cache::{QualityCache, Stamp};
use crate::decide::{self, Candidate, Reason, Thresholds};
use crate::score::{score_image, Score};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use unswiper_core::{MediaItem, MediaKind};

/// Worker threads: all cores but one, at least one.
pub fn default_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().saturating_sub(1).max(1))
        .unwrap_or(1)
}

/// Photos worth scoring: camera formats only (videos, screenshots and GIFs are skipped).
pub fn scorable(items: &[MediaItem]) -> Vec<String> {
    items
        .iter()
        .filter(|i| i.kind == MediaKind::Photo && decide::is_camera_format(&i.path))
        .map(|i| i.path.clone())
        .collect()
}

/// Scores `paths` that aren't already cached, and stores the results in `cache`.
/// Unreadable files are skipped. `on_progress(done, total)` is called from worker threads.
/// Returns how many photos were (re)scored. Stops early when `cancel` is set.
pub fn update_cache(
    paths: &[String],
    cache: &mut QualityCache,
    threads: usize,
    cancel: &AtomicBool,
    now: i64,
    on_progress: &(dyn Fn(usize, usize) + Sync),
) -> usize {
    let todo: Vec<(String, Stamp)> = paths
        .iter()
        .filter_map(|p| Stamp::of(std::path::Path::new(p)).map(|s| (p.clone(), s)))
        .filter(|(p, s)| cache.get(p, *s).is_none())
        .collect();
    let total = todo.len();
    let done = AtomicUsize::new(0);
    let work = || {
        todo.par_iter()
            .filter_map(|(p, st)| {
                if cancel.load(Ordering::Relaxed) {
                    return None;
                }
                let s = crate::decode::decode(std::path::Path::new(p)).map(|img| score_image(&img));
                on_progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
                s.map(|s| (p.clone(), *st, s))
            })
            .collect::<Vec<_>>()
    };
    let results = match rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        .build()
    {
        Ok(pool) => pool.install(work),
        Err(_) => work(),
    };
    let n = results.len();
    for (p, st, s) in results {
        cache.put(&p, st, s, now);
    }
    n
}

/// Suggestions from cached scores.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Suggestions {
    /// Paths the detector calls blurry, with the reason.
    pub flagged: HashMap<String, Reason>,
    /// Paths that have a valid score (used to withdraw stale suggestions only where known).
    pub scored: HashSet<String>,
}

impl Suggestions {
    pub fn flagged_paths(&self) -> HashSet<String> {
        self.flagged.keys().cloned().collect()
    }
}

/// Applies the decision rules to every scorable item with a valid cached score.
pub fn suggestions(items: &[MediaItem], cache: &QualityCache, t: Thresholds) -> Suggestions {
    let wanted: HashSet<String> = scorable(items).into_iter().collect();
    let cands: Vec<Candidate> = items
        .iter()
        .filter(|i| wanted.contains(&i.path))
        .filter_map(|i| {
            let st = Stamp::of(std::path::Path::new(&i.path))?;
            let score: Score = cache.get(&i.path, st)?;
            Some(Candidate {
                path: i.path.clone(),
                taken_at: (!i.is_undated && i.taken_at > unswiper_core::time::distant_past())
                    .then(|| i.taken_at.timestamp()),
                score,
            })
        })
        .collect();
    let reasons = decide::suggest(&cands, t);
    let mut out = Suggestions::default();
    for (c, r) in cands.iter().zip(reasons) {
        out.scored.insert(c.path.clone());
        if let Some(r) = r {
            out.flagged.insert(c.path.clone(), r);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth;
    use unswiper_core::quality::Sensitivity;
    use unswiper_core::time::parse_iso;
    use unswiper_core::Decision;

    pub(crate) fn item(path: &std::path::Path, t: &str, kind: MediaKind) -> MediaItem {
        MediaItem {
            path: path.to_string_lossy().into_owned(),
            kind,
            size_bytes: 1,
            taken_at: parse_iso(t).unwrap(),
            relative_folder: String::new(),
            decision: Decision::Undecided,
            modified_at: None,
            sidecars: vec![],
            is_undated: false,
            date_source: None,
            folder_path: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn scan_scores_caches_and_suggests() {
        let d = tempfile::tempdir().unwrap();
        let sharp = synth::scene(1, 600, 400);
        let a = d.path().join("sharp.jpg");
        let b = d.path().join("blurry.jpg");
        let png = d.path().join("shot.png");
        let vid = d.path().join("clip.mov");
        sharp.to_rgb8().save(&a).unwrap();
        synth::defocus(&sharp, 6.0).to_rgb8().save(&b).unwrap();
        synth::defocus(&sharp, 6.0).to_rgb8().save(&png).unwrap();
        std::fs::write(&vid, b"x").unwrap();
        let broken = d.path().join("broken.jpg");
        std::fs::write(&broken, b"x").unwrap();
        let items = vec![
            item(&a, "2026-01-01T10:00:00Z", MediaKind::Photo),
            item(&b, "2026-01-01T12:00:00Z", MediaKind::Photo),
            item(&png, "2026-01-01T13:00:00Z", MediaKind::Photo),
            item(&vid, "2026-01-01T14:00:00Z", MediaKind::Video),
            item(&broken, "2026-01-01T15:00:00Z", MediaKind::Photo),
            item(
                &d.path().join("gone.jpg"),
                "2026-01-01T16:00:00Z",
                MediaKind::Photo,
            ),
        ];
        let paths = scorable(&items);
        assert_eq!(paths.len(), 4, "png and video skipped");
        let mut cache = QualityCache::default();
        let seen = AtomicUsize::new(0);
        let n = update_cache(
            &paths,
            &mut cache,
            2,
            &AtomicBool::new(false),
            5,
            &|_, _| {
                seen.fetch_add(1, Ordering::Relaxed);
            },
        );
        assert_eq!(n, 2);
        assert_eq!(
            seen.load(Ordering::Relaxed),
            3,
            "broken file counted as done"
        );
        // Second pass: everything readable is cached.
        let n = update_cache(
            &paths,
            &mut cache,
            1,
            &AtomicBool::new(false),
            6,
            &|_, _| {},
        );
        assert_eq!(n, 0);
        let s = suggestions(
            &items,
            &cache,
            Thresholds::for_sensitivity(Sensitivity::Normal),
        );
        assert_eq!(s.scored.len(), 2);
        assert_eq!(
            s.flagged_paths(),
            HashSet::from([b.to_string_lossy().into_owned()])
        );
        assert_eq!(s.flagged.values().next(), Some(&Reason::Soft));
    }

    #[test]
    fn cancel_stops_work() {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("a.jpg");
        synth::scene(2, 64, 64).to_rgb8().save(&a).unwrap();
        let mut cache = QualityCache::default();
        let paths = vec![a.to_string_lossy().into_owned()];
        assert_eq!(
            update_cache(&paths, &mut cache, 1, &AtomicBool::new(true), 0, &|_, _| {}),
            0
        );
        assert!(default_threads() >= 1);
    }
}
