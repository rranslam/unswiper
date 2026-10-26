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
use std::time::{Duration, Instant};
use unswiper_blur::cache::{QualityCache, Stamp};
use unswiper_blur::decide::Thresholds;
use unswiper_blur::scan;
use unswiper_core::MediaItem;

#[derive(Debug, Clone, PartialEq)]
pub enum BlurEvent {
    /// `done` of `total` photos scored in the current pass (cached photos are not counted).
    Progress(usize, usize),
    /// Suggestions from everything scored so far. Sent every few seconds during a pass so
    /// marks fill in while the scan runs. `last` is set on the final result, which also
    /// carries the cache (for the info panel's Sharpness row).
    Result {
        flagged: HashSet<String>,
        scored: HashSet<String>,
        last: bool,
        cache: Option<Box<QualityCache>>,
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
    /// While set, the scan waits between batches.
    pub pause: Arc<AtomicBool>,
}

/// Photos scored per batch: the unit of pausing, cancelling and progress.
const BATCH: usize = 128;
/// How often partial results are sent and the cache is written during a pass.
const RESULT_EVERY: Duration = Duration::from_secs(2);
const SAVE_EVERY: Duration = Duration::from_secs(30);

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Runs the scan on the calling thread, reporting through `emit`. Scores are written to the
/// cache as it goes (also when cancelled), so a later run skips photos already done.
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
    let cancelled = || job.cancel.load(Ordering::Relaxed);
    let mut unsaved = false;
    let mut last_save = Instant::now();
    let result = |cache: &QualityCache, last: bool| {
        let s = scan::suggestions(&job.items, cache, job.thresholds);
        emit(BlurEvent::Result {
            flagged: s.flagged_paths(),
            scored: s.scored,
            last,
            cache: last.then(|| Box::new(cache.clone())),
        });
    };
    for (k, pass) in passes.into_iter().enumerate() {
        if cancelled() {
            break;
        }
        let todo: Vec<String> = scan::scorable(pass)
            .into_iter()
            .filter(|p| {
                Stamp::of(std::path::Path::new(p)).is_some_and(|st| cache.get(p, st).is_none())
            })
            .collect();
        let total = todo.len();
        let mut done = 0;
        let mut last_result = Instant::now();
        for batch in todo.chunks(BATCH) {
            while job.pause.load(Ordering::Relaxed) && !cancelled() {
                std::thread::sleep(Duration::from_millis(100));
            }
            if cancelled() {
                break;
            }
            let base = done;
            let scored =
                scan::update_cache(batch, &mut cache, threads, &job.cancel, now(), &|d, _| {
                    emit(BlurEvent::Progress(base + d, total))
                });
            done += batch.len();
            unsaved |= scored > 0;
            if unsaved && last_save.elapsed() >= SAVE_EVERY {
                let _ = cache.save(&job.cache_path);
                unsaved = false;
                last_save = Instant::now();
            }
            if done < total && last_result.elapsed() >= RESULT_EVERY {
                result(&cache, false);
                last_result = Instant::now();
            }
        }
        if cancelled() {
            break;
        }
        if total > 0 {
            emit(BlurEvent::Progress(total, total));
        }
        if k + 1 == n {
            let all: HashSet<String> = job.items.iter().map(|i| i.path.clone()).collect();
            let before = cache.entries.len();
            cache.retain_paths(&all);
            unsaved |= cache.entries.len() != before;
        }
        if unsaved {
            let _ = cache.save(&job.cache_path);
            unsaved = false;
            last_save = Instant::now();
        }
        result(&cache, k + 1 == n);
    }
    if unsaved {
        // Cancelled: keep what was scored so the next run skips it.
        let _ = cache.save(&job.cache_path);
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
            pause: Arc::new(AtomicBool::new(false)),
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
                    cache,
                } => {
                    assert_eq!(cache.is_some(), *last);
                    Some((flagged.len(), scored.len(), *last))
                }
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

    #[test]
    fn cancel_keeps_scores_and_pause_waits() {
        let d = tempfile::tempdir().unwrap();
        let s = synth::scene(3, 200, 150);
        let mut items = vec![];
        for k in 0..(BATCH + 5) {
            let p = d.path().join(format!("p{k:03}.jpg"));
            s.to_rgb8().save(&p).unwrap();
            items.push(item(&p, 1));
        }
        let cache_path = d.path().join("support/quality-cache.json");
        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(true));
        let job = BlurJob {
            items: items.clone(),
            first: 0,
            cache_path: cache_path.clone(),
            thresholds: Thresholds::for_sensitivity(Sensitivity::Normal),
            cancel: cancel.clone(),
            pause: pause.clone(),
        };
        let events = Arc::new(Mutex::new(vec![]));
        let ev = events.clone();
        let h = std::thread::spawn(move || run(job, &move |e| ev.lock().unwrap().push(e)));
        std::thread::sleep(Duration::from_millis(300));
        assert!(events.lock().unwrap().is_empty(), "paused: nothing scored");
        // Unpause; cancel once the first batch is in.
        pause.store(false, Ordering::Relaxed);
        let t0 = Instant::now();
        while !events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, BlurEvent::Progress(d, _) if *d >= BATCH))
            && t0.elapsed() < Duration::from_secs(60)
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        cancel.store(true, Ordering::Relaxed);
        h.join().unwrap();
        let saved = QualityCache::load(&cache_path).entries.len();
        assert!(saved >= BATCH, "scores kept after cancel: {saved}");
    }
}
