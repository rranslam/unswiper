// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! The app's view-model: everything the windows show and every user action, without GTK.
//! Mirrors `AppState.swift` (selection, decisions, keep targets, commit, restore, pending
//! commits) and adds live folder watching through [`LiveIndex`].

use crate::format;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use unswiper_core::commit::{self, CommitResult};
use unswiper_core::index::DateReader;
use unswiper_core::store::{read_manifest, LibraryStore, StoreError};
use unswiper_core::time::Timestamp;
use unswiper_core::trash::Trasher;
use unswiper_core::watch::{Delta, LiveIndex, WatchBatch};
use unswiper_core::{CommitMode, Decision, LibrarySnapshot, MediaItem, MediaKind, VisitGroup};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    Photos,
    Videos,
}

/// One row of the visits list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisitRow {
    pub id: String,
    pub title: String,
    pub dates: String,
    pub counts: String,
    pub progress: String,
    /// Every item decided.
    pub done: bool,
}

/// What a commit would do, for the confirmation dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitPlan {
    pub items: usize,
    pub files: usize,
    pub bytes: i64,
    pub mode: CommitMode,
}

impl CommitPlan {
    pub fn is_empty(&self) -> bool {
        self.items == 0
    }

    pub fn heading(&self) -> String {
        match self.mode {
            CommitMode::DryRun => "Dry Run".into(),
            _ if self.items == 1 => "Move 1 Rejected Item?".into(),
            _ => format!("Move {} Rejected Items?", self.items),
        }
    }

    pub fn body(&self) -> String {
        let what = format!("{} files, {}", self.files, format::bytes(self.bytes));
        match self.mode {
            CommitMode::Trash => format!("{what} will be moved to the Trash. You can restore them until the Trash is emptied."),
            CommitMode::Quarantine => format!("{what} will be moved to the _Rejected/Unswiper folder inside your library. Nothing is deleted."),
            CommitMode::DryRun => format!("Nothing will move. This only reports what would be moved ({what})."),
        }
    }

    /// The (destructive) confirm button. Enter never activates it (Cancel is the default).
    pub fn confirm_label(&self) -> &'static str {
        match self.mode {
            CommitMode::Trash => "Move to Trash",
            CommitMode::Quarantine => "Move to Quarantine",
            CommitMode::DryRun => "Run Dry Run",
        }
    }
}

pub struct AppState {
    pub store: LibraryStore,
    pub snap: LibrarySnapshot,
    live: Option<LiveIndex>,
    pub visit: Option<String>,
    pub selection: BTreeSet<String>,
    anchor: Option<String>,
    pub filter: Filter,
    /// Shown once after launch when library.json couldn't be read.
    pub load_problem: Option<String>,
    dirty: bool,
}

impl AppState {
    /// Loads library.json from `store` (backing up a damaged file, like the Mac app).
    pub fn new(mut store: LibraryStore) -> Self {
        let r = store.load();
        let mut s = AppState {
            store,
            snap: r.snapshot,
            live: None,
            visit: None,
            selection: BTreeSet::new(),
            anchor: None,
            filter: Filter::All,
            load_problem: r.problem,
            dirty: false,
        };
        if let Some(root) = s.root() {
            if root.is_dir() {
                s.live = Some(LiveIndex::from_items(&root, s.snap.config.visit_gap_hours, s.snap.items.clone()));
            }
        }
        s
    }

    pub fn root(&self) -> Option<PathBuf> {
        self.snap.config.root_path.as_ref().map(PathBuf::from)
    }

    pub fn has_library(&self) -> bool {
        self.root().is_some() && self.snap.last_indexed_at.is_some()
    }

    /// A new library folder: forget the old index (decisions are per path anyway).
    pub fn set_root(&mut self, root: &Path) {
        if self.root().as_deref() != Some(root) {
            self.snap.items.clear();
            self.snap.groups.clear();
            self.snap.last_indexed_at = None;
            self.close_visit();
            self.live = None;
        }
        self.snap.config.root_path = Some(root.to_string_lossy().into_owned());
        self.dirty = true;
    }

    /// Decisions to carry into a (re-)index.
    pub fn decisions(&self) -> HashMap<String, Decision> {
        self.snap.items.iter().map(|i| (i.path.clone(), i.decision)).collect()
    }

    /// Takes the result of an index run.
    pub fn apply_index(&mut self, items: Vec<MediaItem>, groups: Vec<VisitGroup>, now: Timestamp) {
        if let Some(root) = self.root() {
            self.live = Some(LiveIndex::from_items(&root, self.snap.config.visit_gap_hours, items.clone()));
        }
        self.snap.items = items;
        self.snap.groups = groups;
        self.snap.last_indexed_at = Some(now);
        self.after_items_changed();
    }

    /// Applies a debounced folder-watch batch.
    pub fn apply_watch(&mut self, batch: &WatchBatch, dates: &dyn DateReader) -> Delta {
        let Some(live) = self.live.as_mut() else { return Delta::default() };
        let d = live.apply(batch, dates);
        if !d.is_empty() {
            self.snap.items = live.items();
            self.snap.groups = live.groups().to_vec();
            self.after_items_changed();
        }
        d
    }

    fn after_items_changed(&mut self) {
        if let Some(v) = &self.visit {
            if !self.snap.groups.iter().any(|g| &g.id == v) {
                self.close_visit();
            }
        }
        let paths: BTreeSet<&str> = self.snap.items.iter().map(|i| i.path.as_str()).collect();
        self.selection.retain(|p| paths.contains(p.as_str()));
        self.dirty = true;
    }

    pub fn visits(&self) -> Vec<VisitRow> {
        self.snap
            .groups
            .iter()
            .map(|g| {
                let p = g.progress(&self.snap.items);
                VisitRow {
                    id: g.id.clone(),
                    title: g.title.clone(),
                    dates: format::date_range(&g.start_date, &g.end_date),
                    counts: format::counts(g.photo_count, g.video_count),
                    progress: format::progress(p.kept, p.rejected, p.left),
                    done: p.left == 0 && g.item_count() > 0,
                }
            })
            .collect()
    }

    pub fn open_visit(&mut self, id: &str) -> bool {
        if !self.snap.groups.iter().any(|g| g.id == id) {
            return false;
        }
        self.visit = Some(id.to_string());
        self.selection.clear();
        self.anchor = None;
        true
    }

    pub fn close_visit(&mut self) {
        self.visit = None;
        self.selection.clear();
        self.anchor = None;
    }

    pub fn current_group(&self) -> Option<&VisitGroup> {
        let id = self.visit.as_ref()?;
        self.snap.groups.iter().find(|g| &g.id == id)
    }

    pub fn item(&self, path: &str) -> Option<&MediaItem> {
        self.snap.items.iter().find(|i| i.path == path)
    }

    /// Items of the open visit in visit order, after the filter.
    pub fn visible_items(&self) -> Vec<&MediaItem> {
        let Some(g) = self.current_group() else { return vec![] };
        let by_path: HashMap<&str, &MediaItem> = self.snap.items.iter().map(|i| (i.path.as_str(), i)).collect();
        g.item_paths
            .iter()
            .filter_map(|p| by_path.get(p.as_str()).copied())
            .filter(|i| match self.filter {
                Filter::All => true,
                Filter::Photos => i.kind == MediaKind::Photo,
                Filter::Videos => i.kind == MediaKind::Video,
            })
            .collect()
    }

    pub fn set_filter(&mut self, f: Filter) {
        self.filter = f;
        let visible: BTreeSet<String> = self.visible_items().iter().map(|i| i.path.clone()).collect();
        self.selection.retain(|p| visible.contains(p));
    }

    /// Click on a thumbnail: plain = select only this, Ctrl = toggle, Shift = range from anchor.
    pub fn click(&mut self, path: &str, ctrl: bool, shift: bool) {
        let order: Vec<String> = self.visible_items().iter().map(|i| i.path.clone()).collect();
        if !order.iter().any(|p| p == path) {
            return;
        }
        if shift {
            if let Some(a) = self.anchor.as_ref().and_then(|a| order.iter().position(|p| p == a)) {
                let b = order.iter().position(|p| p == path).unwrap();
                let (lo, hi) = (a.min(b), a.max(b));
                if !ctrl {
                    self.selection.clear();
                }
                self.selection.extend(order[lo..=hi].iter().cloned());
                return;
            }
        }
        if ctrl {
            if !self.selection.remove(path) {
                self.selection.insert(path.to_string());
            }
        } else {
            self.selection.clear();
            self.selection.insert(path.to_string());
        }
        self.anchor = Some(path.to_string());
    }

    pub fn select_all(&mut self) {
        self.selection = self.visible_items().iter().map(|i| i.path.clone()).collect();
    }

    /// Applies `d` to the selection; returns how many items changed.
    pub fn decide_selected(&mut self, d: Decision) -> usize {
        let paths: Vec<String> = self.selection.iter().cloned().collect();
        self.decide(&paths, d)
    }

    pub fn decide(&mut self, paths: &[String], d: Decision) -> usize {
        let set: BTreeSet<&str> = paths.iter().map(String::as_str).collect();
        let mut n = 0;
        for i in self.snap.items.iter_mut().filter(|i| set.contains(i.path.as_str())) {
            if i.decision != d {
                i.decision = d;
                n += 1;
            }
            if let Some(live) = self.live.as_mut() {
                live.set_decision(&i.path, d);
            }
        }
        if n > 0 {
            self.dirty = true;
        }
        n
    }

    pub fn keep_target(&self, id: &str) -> i64 {
        self.snap
            .config
            .keep_targets
            .as_ref()
            .and_then(|m| m.get(id).copied())
            .unwrap_or(self.snap.config.keep_target_default)
    }

    pub fn set_keep_target(&mut self, id: &str, n: i64) {
        self.snap.config.keep_targets.get_or_insert_with(Default::default).insert(id.to_string(), n.max(0));
        self.dirty = true;
    }

    /// `Keep target reached` or `4 of 10 kept`.
    pub fn keep_status(&self, id: &str) -> String {
        let Some(g) = self.snap.groups.iter().find(|g| g.id == id) else { return String::new() };
        let kept = g.progress(&self.snap.items).kept as i64;
        let target = self.keep_target(id);
        if kept >= target && target > 0 {
            "Keep target reached".into()
        } else {
            format!("{kept} of {target} kept")
        }
    }

    pub fn set_commit_mode(&mut self, m: CommitMode) {
        self.snap.config.commit_mode = m;
        self.dirty = true;
    }

    /// Every rejected item in the library (like the Mac app, commit covers all visits).
    pub fn commit_plan(&self) -> CommitPlan {
        let rejects: Vec<&MediaItem> = self.snap.items.iter().filter(|i| i.decision == Decision::Reject).collect();
        CommitPlan {
            items: rejects.len(),
            files: rejects.iter().map(|i| i.file_count()).sum(),
            bytes: rejects.iter().map(|i| i.total_bytes()).sum(),
            mode: self.snap.config.commit_mode,
        }
    }

    /// Runs the commit and drops moved items from the index. Returns the message to show.
    pub fn commit(&mut self, trasher: &dyn Trasher) -> Result<String, String> {
        let mode = self.snap.config.commit_mode;
        let root = self.root();
        // Ignore our own moves in the watcher for a while.
        if let Some(live) = self.live.as_mut() {
            let ours = self
                .snap
                .items
                .iter()
                .filter(|i| i.decision == Decision::Reject)
                .flat_map(|i| std::iter::once(i.path.clone()).chain(i.sidecars.iter().map(|s| s.path.clone())))
                .map(PathBuf::from);
            live.suppress(ours, std::time::Duration::from_secs(10));
        }
        let r = commit::commit_rejects(&self.snap.items, mode, root.as_deref(), &self.store, trasher).map_err(|e| e.to_string())?;
        if !r.dry_run {
            let gone: Vec<String> = self
                .snap
                .items
                .iter()
                .filter(|i| i.decision == Decision::Reject && !Path::new(&i.path).exists())
                .map(|i| i.path.clone())
                .collect();
            if !gone.is_empty() {
                if let Some(live) = self.live.as_mut() {
                    live.remove_paths(&gone);
                    self.snap.items = live.items();
                    self.snap.groups = live.groups().to_vec();
                } else {
                    self.snap.items.retain(|i| !gone.contains(&i.path));
                    self.snap.groups = unswiper_core::group::group(&self.snap.items, self.snap.config.visit_gap_hours);
                }
                self.after_items_changed();
            }
        }
        Ok(commit_message(&r, mode))
    }

    pub fn can_restore(&self) -> bool {
        self.store.latest_manifest_path().is_some()
    }

    /// Restores the latest commit. Returns (message, re-index needed).
    pub fn restore_last(&mut self) -> (String, bool) {
        let Some(path) = self.store.latest_manifest_path() else {
            return ("There is no commit to restore.".into(), false);
        };
        self.restore_manifest(&path)
    }

    fn restore_manifest(&mut self, path: &Path) -> (String, bool) {
        let m = match read_manifest(path) {
            Ok(m) => m,
            Err(e) => return (format!("Couldn't read the commit record: {e}"), false),
        };
        let r = commit::restore(&m);
        if r.failed.is_empty() {
            let _ = self.store.mark_restored(path);
        }
        let mut msg = format!("Restored {} file{}.", r.restored, if r.restored == 1 { "" } else { "s" });
        if r.already_in_place > 0 {
            msg.push_str(&format!(" {} were already in place.", r.already_in_place));
        }
        if let Some((p, e)) = r.failed.first() {
            msg.push_str(&format!(" {} couldn't be restored (first: {}: {e})", r.failed.len(), short(p)));
        }
        (msg, r.restored > 0)
    }

    /// Commits interrupted by a crash (shown at launch).
    pub fn pending_commits(&self) -> Vec<PathBuf> {
        self.store.pending_manifest_paths()
    }

    /// "Restore Files" (true) or "Leave As Is" (false) for every interrupted commit.
    pub fn resolve_pending(&mut self, restore: bool) -> Option<(String, bool)> {
        let pending = self.pending_commits();
        if !restore || pending.is_empty() {
            return None;
        }
        let mut msgs = vec![];
        let mut reindex = false;
        for p in pending {
            let (m, r) = self.restore_manifest(&p);
            msgs.push(m);
            reindex |= r;
        }
        Some((msgs.join(" "), reindex))
    }

    pub fn needs_save(&self) -> bool {
        self.dirty
    }

    pub fn save(&mut self) -> Result<(), StoreError> {
        self.store.save(&self.snap)?;
        self.dirty = false;
        Ok(())
    }
}

fn short(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// Result text, matching the Mac app's wording.
pub fn commit_message(r: &CommitResult, mode: CommitMode) -> String {
    if r.dry_run {
        return format!(
            "Dry run: {} item{} ({}) would be moved. Nothing was moved.",
            r.would_move,
            if r.would_move == 1 { "" } else { "s" },
            format::bytes(r.bytes)
        );
    }
    let place = if mode == CommitMode::Trash { "to the Trash" } else { "to the quarantine folder" };
    let mut s = format!(
        "Moved {} item{} ({}) {place}.",
        r.moved,
        if r.moved == 1 { "" } else { "s" },
        format::bytes(r.bytes)
    );
    if !r.failed.is_empty() {
        let (p, e) = &r.failed[0];
        s.push_str(&format!(" {} skipped — {}: {e}", r.failed.len(), short(p)));
    }
    s
}


#[cfg(test)]
mod tests {
    use super::*;
    use unswiper_core::index::{index, MetadataDates};
    use unswiper_core::trash::FreedesktopTrash;

    struct T {
        _d: tempfile::TempDir,
        root: PathBuf,
        base: PathBuf,
    }

    fn setup(names: &[&str]) -> (T, AppState) {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let root = base.join("lib");
        std::fs::create_dir(&root).unwrap();
        for n in names {
            std::fs::write(root.join(n), b"x").unwrap();
        }
        let mut s = AppState::new(LibraryStore::new(base.join("support")));
        s.set_root(&root);
        let (items, groups) = index(&root, 36.0, &s.decisions(), &MetadataDates, |_| {}).unwrap();
        s.apply_index(items, groups, chrono::Utc::now().into());
        (T { _d: d, root, base }, s)
    }

    fn open_first(s: &mut AppState) {
        let id = s.visits()[0].id.clone();
        assert!(s.open_visit(&id));
    }

    #[test]
    fn index_and_visits() {
        let (_t, s) = setup(&["a.jpg", "b.jpg", "c.mov"]);
        assert!(s.has_library());
        let v = s.visits();
        assert_eq!(v.len(), 1);
        assert!(!v[0].done);
        assert!(s.needs_save());
    }

    #[test]
    fn click_ctrl_shift_selection() {
        let (_t, mut s) = setup(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
        open_first(&mut s);
        let p: Vec<String> = s.visible_items().iter().map(|i| i.path.clone()).collect();
        s.click(&p[0], false, false);
        s.click(&p[2], false, true);
        assert_eq!(s.selection.len(), 3);
        s.click(&p[1], true, false);
        assert_eq!(s.selection.len(), 2);
        s.click(&p[3], false, false);
        assert_eq!(s.selection.len(), 1);
        s.click("nope", false, false);
        assert_eq!(s.selection.len(), 1);
        s.select_all();
        assert_eq!(s.selection.len(), 4);
    }

    #[test]
    fn decide_and_progress() {
        let (_t, mut s) = setup(&["a.jpg", "b.jpg"]);
        open_first(&mut s);
        s.select_all();
        assert_eq!(s.decide_selected(Decision::Keep), 2);
        assert_eq!(s.decide_selected(Decision::Keep), 0);
        assert!(s.visits()[0].done);
        let id = s.visits()[0].id.clone();
        s.set_keep_target(&id, 2);
        assert_eq!(s.keep_status(&id), "Keep target reached");
        s.set_keep_target(&id, 5);
        assert_eq!(s.keep_status(&id), "2 of 5 kept");
        assert_eq!(s.keep_target(&id), 5);
    }

    #[test]
    fn filter_drops_hidden_selection() {
        let (_t, mut s) = setup(&["a.jpg", "b.mov"]);
        open_first(&mut s);
        s.select_all();
        s.set_filter(Filter::Videos);
        assert_eq!(s.visible_items().len(), 1);
        assert_eq!(s.selection.len(), 1);
        s.set_filter(Filter::Photos);
        assert_eq!(s.visible_items()[0].kind, MediaKind::Photo);
        assert!(s.selection.is_empty());
    }

    #[test]
    fn plan_wording() {
        let (_t, mut s) = setup(&["a.jpg"]);
        assert!(s.commit_plan().is_empty());
        open_first(&mut s);
        s.select_all();
        s.decide_selected(Decision::Reject);
        s.set_commit_mode(CommitMode::Trash);
        let p = s.commit_plan();
        assert_eq!(p.heading(), "Move 1 Rejected Item?");
        assert!(p.body().contains("Trash"));
        assert_eq!(p.confirm_label(), "Move to Trash");
        s.set_commit_mode(CommitMode::Quarantine);
        assert_eq!(s.commit_plan().confirm_label(), "Move to Quarantine");
        s.set_commit_mode(CommitMode::DryRun);
        assert_eq!(s.commit_plan().heading(), "Dry Run");
        assert!(s.commit_plan().body().contains("Nothing will move"));
    }

    #[test]
    fn commit_quarantine_then_restore() {
        let (t, mut s) = setup(&["a.jpg", "b.jpg"]);
        open_first(&mut s);
        let a = s.visible_items()[0].path.clone();
        s.decide(std::slice::from_ref(&a), Decision::Reject);
        s.set_commit_mode(CommitMode::Quarantine);
        let trash = FreedesktopTrash { trash_dir: t.base.join("Trash") };
        let msg = s.commit(&trash).unwrap();
        assert!(msg.starts_with("Moved 1 item"), "{msg}");
        assert!(!Path::new(&a).exists());
        assert_eq!(s.snap.items.len(), 1);
        assert!(s.can_restore());
        let (msg, reindex) = s.restore_last();
        assert!(msg.starts_with("Restored 1 file"), "{msg}");
        assert!(reindex);
        assert!(Path::new(&a).exists());
        assert!(s.pending_commits().is_empty());
        assert!(s.resolve_pending(true).is_none());
        let _ = t.root;
    }

    #[test]
    fn commit_trash_and_dry_run() {
        let (t, mut s) = setup(&["a.jpg"]);
        open_first(&mut s);
        s.select_all();
        s.decide_selected(Decision::Reject);
        let trash = FreedesktopTrash { trash_dir: t.base.join("Trash") };
        s.set_commit_mode(CommitMode::DryRun);
        let msg = s.commit(&trash).unwrap();
        assert!(msg.starts_with("Dry run: 1 item"), "{msg}");
        assert_eq!(s.snap.items.len(), 1);
        s.set_commit_mode(CommitMode::Trash);
        let msg = s.commit(&trash).unwrap();
        assert!(msg.contains("to the Trash"), "{msg}");
        assert!(s.snap.items.is_empty());
        assert!(s.visit.is_none());
    }

    #[test]
    fn restore_without_commit() {
        let (_t, mut s) = setup(&["a.jpg"]);
        assert!(!s.can_restore());
        assert_eq!(s.restore_last().0, "There is no commit to restore.");
    }

    #[test]
    fn watch_adds_and_removes() {
        let (t, mut s) = setup(&["a.jpg"]);
        let b = t.root.join("b.jpg");
        std::fs::write(&b, b"x").unwrap();
        let d = s.apply_watch(&WatchBatch::Paths([b.clone()].into_iter().collect()), &MetadataDates);
        assert_eq!(d.added.len(), 1);
        assert_eq!(s.snap.items.len(), 2);
        std::fs::remove_file(&b).unwrap();
        let d = s.apply_watch(&WatchBatch::Paths([b].into_iter().collect()), &MetadataDates);
        assert_eq!(d.removed.len(), 1);
        assert_eq!(s.snap.items.len(), 1);
    }

    #[test]
    fn save_and_reload_keeps_decisions() {
        let (t, mut s) = setup(&["a.jpg"]);
        open_first(&mut s);
        s.select_all();
        s.decide_selected(Decision::Keep);
        s.save().unwrap();
        assert!(!s.needs_save());
        let s2 = AppState::new(LibraryStore::new(t.base.join("support")));
        assert!(s2.has_library());
        assert_eq!(s2.snap.items[0].decision, Decision::Keep);
        assert!(s2.load_problem.is_none());
    }

    #[test]
    fn set_root_resets_index() {
        let (t, mut s) = setup(&["a.jpg"]);
        open_first(&mut s);
        s.set_root(&t.base);
        assert!(!s.has_library());
        assert!(s.snap.items.is_empty());
        assert!(s.visit.is_none());
        assert!(!s.open_visit("x"));
    }
}
