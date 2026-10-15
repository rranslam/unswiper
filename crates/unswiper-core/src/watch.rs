// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Live folder watching (plan Phase 1b, §3.0, §10.4a).
//!
//! * [`FolderWatcher`] wraps `notify` (inotify on Linux, FSEvents on macOS) and falls back to
//!   polling when native watching can't start (e.g. inotify watch limit / `ENOSPC`, FUSE).
//! * [`Debouncer`] batches raw events: a batch is emitted after `quiet` with no new events, or
//!   at the latest after `max_wait`, so a burst of thousands of files becomes one re-index.
//!   Queue overflows and watcher errors turn the batch into a full [`WatchBatch::Rescan`].
//! * [`LiveIndex`] applies a batch incrementally: only the affected pairing sets (same folder +
//!   base name, so a Live Photo's HEIC and MOV are re-paired together) and folders are re-read;
//!   decisions on unchanged and renamed-back items survive; groups are recomputed.
//!   Paths the app moves itself (commit/restore) can be suppressed for a while.

use crate::group;
use crate::index::{
    self, entry_for, into_sets, is_excluded, items_for_set, set_key, sort_items, walk, DateReader,
    IndexError,
};
use crate::model::{Decision, MediaItem, VisitGroup};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What the index should do after a quiet period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchBatch {
    /// These paths (files or folders) were created, changed, removed or renamed.
    Paths(Vec<PathBuf>),
    /// Events were lost (overflow / watcher error): re-index everything.
    Rescan,
}

/// Collects raw events into batches. Pure logic; time is passed in.
#[derive(Debug)]
pub struct Debouncer {
    quiet: Duration,
    max_wait: Duration,
    paths: BTreeSet<PathBuf>,
    rescan: bool,
    first: Option<Instant>,
    last: Option<Instant>,
}

impl Debouncer {
    pub fn new(quiet: Duration, max_wait: Duration) -> Self {
        Debouncer {
            quiet,
            max_wait,
            paths: BTreeSet::new(),
            rescan: false,
            first: None,
            last: None,
        }
    }

    fn touch(&mut self, now: Instant) {
        self.first.get_or_insert(now);
        self.last = Some(now);
    }

    /// Feeds one raw `notify` result.
    pub fn push(&mut self, ev: notify::Result<Event>, now: Instant) {
        match ev {
            Err(_) => {
                self.rescan = true;
                self.touch(now);
            }
            Ok(ev) => {
                if ev.need_rescan() {
                    self.rescan = true;
                    self.touch(now);
                    return;
                }
                if matches!(ev.kind, EventKind::Access(_)) || ev.paths.is_empty() {
                    return;
                }
                self.paths.extend(ev.paths);
                self.touch(now);
            }
        }
    }

    /// When the pending batch becomes ready, if anything is pending.
    pub fn deadline(&self) -> Option<Instant> {
        let (first, last) = (self.first?, self.last?);
        Some((last + self.quiet).min(first + self.max_wait))
    }

    /// Takes the batch if its deadline has passed.
    pub fn take_ready(&mut self, now: Instant) -> Option<WatchBatch> {
        if now < self.deadline()? {
            return None;
        }
        self.first = None;
        self.last = None;
        let paths = std::mem::take(&mut self.paths);
        if std::mem::take(&mut self.rescan) {
            return Some(WatchBatch::Rescan);
        }
        Some(WatchBatch::Paths(paths.into_iter().collect()))
    }
}

/// Which back end a [`FolderWatcher`] ended up using.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Native,
    Polling,
}

/// Watches a library folder recursively and calls `on_batch` (on a background thread) with
/// debounced batches. Stops when dropped.
pub struct FolderWatcher {
    _watcher: Box<dyn Watcher + Send>,
    backend: Backend,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// Default debounce: 500 ms quiet, at most 5 s.
pub const DEFAULT_QUIET: Duration = Duration::from_millis(500);
pub const DEFAULT_MAX_WAIT: Duration = Duration::from_secs(5);

impl FolderWatcher {
    /// Native watcher, falling back to polling every 2 s if it can't start.
    pub fn start<F>(
        root: &Path,
        quiet: Duration,
        max_wait: Duration,
        on_batch: F,
    ) -> notify::Result<Self>
    where
        F: FnMut(WatchBatch) + Send + 'static,
    {
        if !root.is_dir() {
            return Err(notify::Error::path_not_found().add_path(root.to_path_buf()));
        }
        let (tx, rx) = mpsc::channel();
        let native = notify::recommended_watcher(tx.clone()).and_then(|mut w| {
            w.watch(root, RecursiveMode::Recursive)?;
            Ok(w)
        });
        match native {
            Ok(w) => Ok(Self::spawn(
                Box::new(w),
                Backend::Native,
                rx,
                quiet,
                max_wait,
                on_batch,
            )),
            Err(_) => Self::start_polling_with(
                root,
                Duration::from_secs(2),
                quiet,
                max_wait,
                tx,
                rx,
                on_batch,
            ),
        }
    }

    /// Polling watcher (FUSE/network mounts, or when inotify watches are exhausted).
    pub fn start_polling<F>(
        root: &Path,
        interval: Duration,
        quiet: Duration,
        max_wait: Duration,
        on_batch: F,
    ) -> notify::Result<Self>
    where
        F: FnMut(WatchBatch) + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        Self::start_polling_with(root, interval, quiet, max_wait, tx, rx, on_batch)
    }

    fn start_polling_with<F>(
        root: &Path,
        interval: Duration,
        quiet: Duration,
        max_wait: Duration,
        tx: mpsc::Sender<notify::Result<Event>>,
        rx: mpsc::Receiver<notify::Result<Event>>,
        on_batch: F,
    ) -> notify::Result<Self>
    where
        F: FnMut(WatchBatch) + Send + 'static,
    {
        let config = notify::Config::default()
            .with_poll_interval(interval)
            .with_compare_contents(false);
        let mut w = notify::PollWatcher::new(tx, config)?;
        w.watch(root, RecursiveMode::Recursive)?;
        Ok(Self::spawn(
            Box::new(w),
            Backend::Polling,
            rx,
            quiet,
            max_wait,
            on_batch,
        ))
    }

    fn spawn<F>(
        watcher: Box<dyn Watcher + Send>,
        backend: Backend,
        rx: mpsc::Receiver<notify::Result<Event>>,
        quiet: Duration,
        max_wait: Duration,
        mut on_batch: F,
    ) -> Self
    where
        F: FnMut(WatchBatch) + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut deb = Debouncer::new(quiet, max_wait);
            while !stop2.load(Ordering::Relaxed) {
                let wait = deb
                    .deadline()
                    .map(|d| d.saturating_duration_since(Instant::now()))
                    .unwrap_or(Duration::from_millis(100))
                    .min(Duration::from_millis(100));
                match rx.recv_timeout(wait) {
                    Ok(ev) => deb.push(ev, Instant::now()),
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                if let Some(b) = deb.take_ready(Instant::now()) {
                    on_batch(b);
                }
            }
        });
        FolderWatcher {
            _watcher: watcher,
            backend,
            stop,
            thread: Some(thread),
        }
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }
}

impl Drop for FolderWatcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// What changed in the index after applying a batch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Delta {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub updated: Vec<String>,
    /// True when the batch caused a full re-index.
    pub rescanned: bool,
}

impl Delta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.updated.is_empty()
    }
}

/// An index that can be updated incrementally from watch batches.
pub struct LiveIndex {
    root: PathBuf,
    gap_hours: f64,
    items: BTreeMap<String, MediaItem>,
    groups: Vec<VisitGroup>,
    /// Decisions of items that disappeared, so a file moved away and back keeps its decision.
    remembered: HashMap<String, Decision>,
    suppressed: HashMap<PathBuf, Instant>,
}

fn same_content(a: &MediaItem, b: &MediaItem) -> bool {
    let mut b = b.clone();
    b.decision = a.decision;
    *a == b
}

impl LiveIndex {
    /// Full index of `root`.
    pub fn new(
        root: &Path,
        gap_hours: f64,
        decisions: &HashMap<String, Decision>,
        dates: &dyn DateReader,
    ) -> Result<Self, IndexError> {
        let (items, groups) = index::index(root, gap_hours, decisions, dates, |_| {})?;
        Ok(LiveIndex {
            root: root.to_path_buf(),
            gap_hours,
            items: items.into_iter().map(|i| (i.path.clone(), i)).collect(),
            groups,
            remembered: decisions.clone(),
            suppressed: HashMap::new(),
        })
    }

    /// Wraps an index that was already built (e.g. loaded from library.json or just indexed),
    /// so the app doesn't have to walk the tree twice.
    pub fn from_items(root: &Path, gap_hours: f64, items: Vec<MediaItem>) -> Self {
        let mut li = LiveIndex {
            root: root.to_path_buf(),
            gap_hours,
            remembered: HashMap::new(),
            items: items.into_iter().map(|i| (i.path.clone(), i)).collect(),
            groups: vec![],
            suppressed: HashMap::new(),
        };
        li.regroup();
        li
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Items sorted by date, then path.
    pub fn items(&self) -> Vec<MediaItem> {
        let mut v: Vec<MediaItem> = self.items.values().cloned().collect();
        sort_items(&mut v);
        v
    }

    pub fn groups(&self) -> &[VisitGroup] {
        &self.groups
    }

    pub fn item(&self, path: &str) -> Option<&MediaItem> {
        self.items.get(path)
    }

    pub fn set_decision(&mut self, path: &str, d: Decision) -> bool {
        match self.items.get_mut(path) {
            Some(i) => {
                i.decision = d;
                true
            }
            None => false,
        }
    }

    /// Copies an item's decision and extra fields (e.g. blur suggestion state) from the
    /// app's copy. Returns false for unknown paths.
    pub fn sync_item(&mut self, src: &MediaItem) -> bool {
        match self.items.get_mut(&src.path) {
            Some(i) => {
                i.decision = src.decision;
                i.extra = src.extra.clone();
                true
            }
            None => false,
        }
    }

    /// Ignore events for these paths (e.g. files the app is about to move) for `ttl`.
    pub fn suppress<I: IntoIterator<Item = PathBuf>>(&mut self, paths: I, ttl: Duration) {
        let until = Instant::now() + ttl;
        for p in paths {
            self.suppressed.insert(p, until);
        }
    }

    /// Drops items the app moved itself (after a commit), without touching the disk.
    pub fn remove_paths(&mut self, paths: &[String]) -> Delta {
        let mut d = Delta::default();
        for p in paths {
            if self.items.remove(p).is_some() {
                d.removed.push(p.clone());
            }
        }
        self.regroup();
        d
    }

    fn regroup(&mut self) {
        self.groups = group::group(&self.items(), self.gap_hours);
    }

    fn full_rescan(&mut self, dates: &dyn DateReader) -> Delta {
        let mut decisions: HashMap<String, Decision> = self.remembered.clone();
        decisions.extend(self.items.values().map(|i| (i.path.clone(), i.decision)));
        let new = match index::index(&self.root, self.gap_hours, &decisions, dates, |_| {}) {
            Ok((items, _)) => items,
            Err(_) => vec![], // root gone: everything is removed
        };
        let new: BTreeMap<String, MediaItem> =
            new.into_iter().map(|i| (i.path.clone(), i)).collect();
        let old = std::mem::replace(&mut self.items, new);
        let mut d = self.diff(&old);
        d.rescanned = true;
        self.remember(&old);
        self.regroup();
        d
    }

    fn remember(&mut self, old: &BTreeMap<String, MediaItem>) {
        for (p, i) in old {
            if !self.items.contains_key(p) && i.decision != Decision::Undecided {
                self.remembered.insert(p.clone(), i.decision);
            }
        }
    }

    fn diff(&self, old: &BTreeMap<String, MediaItem>) -> Delta {
        let mut d = Delta::default();
        for (p, i) in &self.items {
            match old.get(p) {
                None => d.added.push(p.clone()),
                Some(o) if !same_content(o, i) => d.updated.push(p.clone()),
                _ => {}
            }
        }
        d.removed = old
            .keys()
            .filter(|p| !self.items.contains_key(*p))
            .cloned()
            .collect();
        d
    }

    /// Applies one debounced batch.
    pub fn apply(&mut self, batch: &WatchBatch, dates: &dyn DateReader) -> Delta {
        let now = Instant::now();
        self.suppressed.retain(|_, t| *t > now);
        let paths = match batch {
            WatchBatch::Rescan => return self.full_rescan(dates),
            WatchBatch::Paths(p) => p,
        };

        let mut dirty: BTreeMap<String, PathBuf> = BTreeMap::new(); // set key -> folder
        let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
        for p in paths {
            if self.suppressed.contains_key(p) {
                continue;
            }
            if p == &self.root {
                if !p.exists() {
                    return self.full_rescan(dates);
                }
                continue;
            }
            if !p.starts_with(&self.root) || is_excluded(p, &self.root) {
                continue;
            }
            let is_dir = std::fs::symlink_metadata(p)
                .map(|m| m.is_dir())
                .unwrap_or(false);
            let prefix = format!("{}/", p.display());
            let had_children = self.items.values().any(|i| {
                i.path.starts_with(&prefix)
                    || i.sidecars.iter().any(|s| s.path.starts_with(&prefix))
            });
            if is_dir || had_children {
                dirs.insert(p.clone());
            }
            if !is_dir {
                if let Some(parent) = p.parent() {
                    dirty.insert(set_key(p), parent.to_path_buf());
                }
            }
        }
        if dirty.is_empty() && dirs.is_empty() {
            return Delta::default();
        }

        let dir_prefixes: Vec<String> = dirs.iter().map(|d| format!("{}/", d.display())).collect();
        let affected = |path: &str| -> bool {
            dirty.contains_key(&set_key(Path::new(path)))
                || dir_prefixes
                    .iter()
                    .any(|pre| path.starts_with(pre.as_str()))
        };
        let old = self.items.clone();
        let decisions: HashMap<String, Decision> = old
            .values()
            .map(|i| (i.path.clone(), i.decision))
            .chain(self.remembered.iter().map(|(k, v)| (k.clone(), *v)))
            .collect();
        self.items
            .retain(|p, i| !(affected(p) || i.sidecars.iter().any(|s| affected(&s.path))));

        // Re-read affected folders and sets.
        let mut entries = vec![];
        let mut seen = 0usize;
        for d in &dirs {
            if d.is_dir() {
                let _ = walk(d, &self.root, &mut seen, &mut entries, &mut |_| {});
            }
        }
        let mut folders: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
        for (key, folder) in &dirty {
            folders
                .entry(folder.clone())
                .or_default()
                .insert(key.clone());
        }
        for (folder, keys) in folders {
            if dir_prefixes
                .iter()
                .any(|pre| format!("{}/", folder.display()).starts_with(pre.as_str()))
            {
                continue; // already walked
            }
            let Ok(rd) = std::fs::read_dir(&folder) else {
                continue;
            };
            for child in rd.flatten() {
                let p = child.path();
                if keys.contains(&set_key(&p)) {
                    if let Some(e) = entry_for(&p, &self.root) {
                        entries.push(e);
                    }
                }
            }
        }
        let lookup = |p: &str| decisions.get(p).copied();
        for set in into_sets(entries).values() {
            for item in items_for_set(set, &self.root, &lookup, dates) {
                self.items.insert(item.path.clone(), item);
            }
        }
        let d = self.diff(&old);
        self.remember(&old);
        if !d.is_empty() {
            self.regroup();
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{AccessKind, CreateKind, Flag};

    fn ev(kind: EventKind, p: &str) -> notify::Result<Event> {
        Ok(Event::new(kind).add_path(PathBuf::from(p)))
    }

    #[test]
    fn debouncer_quiet_and_max_wait() {
        let t0 = Instant::now();
        let mut d = Debouncer::new(Duration::from_millis(100), Duration::from_millis(1000));
        assert_eq!(d.deadline(), None);
        assert_eq!(d.take_ready(t0), None);
        d.push(ev(EventKind::Create(CreateKind::File), "/r/a"), t0);
        d.push(ev(EventKind::Access(AccessKind::Any), "/r/ignored"), t0);
        d.push(Ok(Event::new(EventKind::Any)), t0);
        assert_eq!(d.take_ready(t0 + Duration::from_millis(50)), None);
        // Keeps extending while events arrive…
        for i in 1..20 {
            d.push(
                ev(EventKind::Create(CreateKind::File), "/r/b"),
                t0 + Duration::from_millis(i * 60),
            );
        }
        // …but not past max_wait.
        let b = d.take_ready(t0 + Duration::from_millis(1000)).unwrap();
        assert_eq!(b, WatchBatch::Paths(vec!["/r/a".into(), "/r/b".into()]));
        assert_eq!(d.take_ready(t0 + Duration::from_secs(5)), None);
    }

    #[test]
    fn debouncer_overflow_and_errors_rescan() {
        let t0 = Instant::now();
        let mut d = Debouncer::new(Duration::from_millis(10), Duration::from_secs(1));
        d.push(ev(EventKind::Create(CreateKind::File), "/r/a"), t0);
        d.push(Ok(Event::new(EventKind::Other).set_flag(Flag::Rescan)), t0);
        assert_eq!(
            d.take_ready(t0 + Duration::from_millis(20)),
            Some(WatchBatch::Rescan)
        );
        d.push(Err(notify::Error::generic("queue overflow")), t0);
        assert_eq!(
            d.take_ready(t0 + Duration::from_millis(20)),
            Some(WatchBatch::Rescan)
        );
    }

    #[test]
    fn delta_empty() {
        assert!(Delta::default().is_empty());
        let d = Delta {
            added: vec!["x".into()],
            ..Default::default()
        };
        assert!(!d.is_empty());
    }
}
