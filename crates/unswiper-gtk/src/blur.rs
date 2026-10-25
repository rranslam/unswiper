// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Background blur scan for the app (plan §15.1): scores the open visit first, then the
//! rest of the library, on all cores but one, using `quality-cache.json` next to
//! library.json. It only reports results; [`crate::state::AppState::apply_blur`] turns them
//! into suggestions, and nothing is ever moved here.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use unswiper_blur::cache::QualityCache;
use unswiper_blur::decide::Thresholds;
use unswiper_blur::scan;
use unswiper_core::MediaItem;

#[derive(Debug, Clone, PartialEq)]
pub enum BlurEvent {
    /// `done` of `total` photos scored in the current pass.
    Progress(usize, usize),
    /// Suggestions from everything scored so far. `last` is set on the final pass.
    Result {
        flagged: HashSet<String>,
        scored: HashSet<String>,
        last: bool,
    },
}

pub struct BlurJob {
    /// All items, in priority order (see `AppState::blur_scan_order`).
    pub items: Vec<MediaItem>,
    /// How many leading items belong to the open visit (scored first).
    pub first: usize,
    pub cache_path: PathBuf,
    pub thresholds: Thresholds,
    pub cancel: Arc<AtomicBool>,
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Runs the scan on the calling thread, reporting through `emit`.
pub fn run(job: BlurJob, emit: &(dyn Fn(BlurEvent) + Sync)) {
    let mut cache = QualityCache::load(&job.cache_path);
    let threads = scan::default_threads();
    let first = job.first.min(job.items.len());
    let passes: Vec<&[MediaItem]> = if first > 0 && first < job.items.len() {
        vec![&job.items[..first], &job.items[..]]
    } else {
        vec![&job.items[..]]
    };
    let n = passes.len();
    for (k, pass) in passes.into_iter().enumerate() {
        if job.cancel.load(Ordering::Relaxed) {
            return;
        }
        let paths = scan::scorable(pass);
        let scored_now =
            scan::update_cache(&paths, &mut cache, threads, &job.cancel, now(), &|d, t| {
                emit(BlurEvent::Progress(d, t))
            });
        if job.cancel.load(Ordering::Relaxed) {
            return;
        }
        if scored_now > 0 {
            let all: HashSet<String> = job.items.iter().map(|i| i.path.clone()).collect();
            cache.retain_paths(&all);
            let _ = cache.save(&job.cache_path);
        }
        let s = scan::suggestions(&job.items, &cache, job.thresholds);
        emit(BlurEvent::Result {
            flagged: s.flagged_paths(),
            scored: s.scored,
            last: k + 1 == n,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use unswiper_blur::synth;
    use unswiper_core::quality::Sensitivity;
    use unswiper_core::time::parse_iso;
    use unswiper_core::{Decision, MediaKind};

    fn item(p: &std::path::Path, hour: u32) -> MediaItem {
        MediaItem {
            path: p.to_string_lossy().into_owned(),
            kind: MediaKind::Photo,
            size_bytes: 1,
            taken_at: parse_iso(&format!("2026-01-01T{hour:02}:00:00Z")).unwrap(),
            relative_folder: String::new(),
            decision: Decision::Undecided,
            modified_at: None,
            sidecars: vec![],
            is_undated: false,
            date_source: None,
            folder_path: None,
            location: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn two_passes_then_cached() {
        let d = tempfile::tempdir().unwrap();
        let s = synth::scene(9, 400, 300);
        let a = d.path().join("a.jpg");
        let b = d.path().join("b.jpg");
        s.to_rgb8().save(&a).unwrap();
        synth::defocus(&s, 6.0).to_rgb8().save(&b).unwrap();
        let items = vec![item(&b, 1), item(&a, 2)];
        let cache_path = d.path().join("support/quality-cache.json");
        let job = || BlurJob {
            items: items.clone(),
            first: 1,
            cache_path: cache_path.clone(),
            thresholds: Thresholds::for_sensitivity(Sensitivity::Normal),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let events = Mutex::new(vec![]);
        run(job(), &|e| events.lock().unwrap().push(e));
        let ev = events.into_inner().unwrap();
        let results: Vec<_> = ev
            .iter()
            .filter_map(|e| match e {
                BlurEvent::Result {
                    flagged,
                    scored,
                    last,
                } => Some((flagged.len(), scored.len(), *last)),
                _ => None,
            })
            .collect();
        assert_eq!(results, vec![(1, 1, false), (1, 2, true)]);
        assert!(ev.contains(&BlurEvent::Progress(1, 1)));
        assert_eq!(QualityCache::load(&cache_path).entries.len(), 2);
        // Second run: all cached, a single pass when everything is "first".
        let events = Mutex::new(vec![]);
        let mut j = job();
        j.first = 2;
        run(j, &|e| events.lock().unwrap().push(e));
        let ev = events.into_inner().unwrap();
        assert_eq!(ev.len(), 1, "no progress when cached: {ev:?}");
        // Cancelled before start: nothing reported.
        let j = job();
        j.cancel.store(true, Ordering::Relaxed);
        let events = Mutex::new(vec![]);
        run(j, &|e| events.lock().unwrap().push(e));
        assert!(events.into_inner().unwrap().is_empty());
    }
}
