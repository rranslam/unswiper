// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! The app's view-model: everything the windows show and every user action, without GTK.
//! Mirrors `AppState.swift` (selection, decisions, keep targets, commit, restore, pending
//! commits) and adds live folder watching through [`LiveIndex`].

use crate::format;
use std::collections::HashSet;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use unswiper_core::commit::{self, CommitResult};
use unswiper_core::geo::{group_with, GroupSettings};
use unswiper_core::index::DateReader;
use unswiper_core::quality::{self, ApplyReport, Sensitivity};
use unswiper_core::store::{read_manifest, LibraryStore, StoreError};
use unswiper_core::time::Timestamp;
use unswiper_core::trash::Trasher;
use unswiper_core::watch::{Delta, LiveIndex, WatchBatch};
use unswiper_core::{
    CommitMode, Decision, GroupMode, LibrarySnapshot, MediaItem, MediaKind, VisitGroup,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    Photos,
    Videos,
    /// Only photos suggested as blurry (plan §15.2 "Show blurry suggestions").
    Blurry,
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
    /// The group is at a known place (plan §20.6: pin icon in the list).
    pub has_place: bool,
}

/// What a commit would do, for the confirmation dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitPlan {
    /// The open visit's title (commits act on one visit, like the Mac app).
    pub visit: String,
    pub items: usize,
    pub keeps: usize,
    pub undecided: usize,
    pub files: usize,
    pub bytes: i64,
    pub mode: CommitMode,
    /// How many of the rejects are blur suggestions the user hasn't confirmed.
    pub suggested_blurry: usize,
}

impl CommitPlan {
    pub fn is_empty(&self) -> bool {
        self.items == 0
    }

    pub fn heading(&self) -> String {
        match self.mode {
            CommitMode::DryRun => "Dry Run".into(),
            _ if self.items == 1 => format!("Move 1 Rejected Item from “{}”?", self.visit),
            _ => format!("Move {} Rejected Items from “{}”?", self.items, self.visit),
        }
    }

    pub fn body(&self) -> String {
        let what = format!(
            "{} rejected item{} ({} file{}, {})",
            self.items,
            if self.items == 1 { "" } else { "s" },
            self.files,
            if self.files == 1 { "" } else { "s" },
            format::bytes(self.bytes)
        );
        let action = match self.mode {
            CommitMode::Trash => format!("{what} will be moved to the Trash. You can restore them until the Trash is emptied."),
            CommitMode::Quarantine => format!("{what} will be moved to the _Rejected/Unswiper folder inside your library. Nothing is deleted."),
            CommitMode::DryRun => format!("Nothing will move. This only reports what would be moved: {what}."),
        };
        let blurry = match self.suggested_blurry {
            0 => String::new(),
            1 => "\n\n1 of them was suggested as blurry by Unswiper. Press Cancel to look at it first.".into(),
            n => format!("\n\n{n} of them were suggested as blurry by Unswiper. Press Cancel to look at them first."),
        };
        format!(
            "Visit “{}”: {action}{blurry}\n\n{} kept and {} undecided item{} stay where they are. Other visits are not touched.",
            self.visit,
            self.keeps,
            self.undecided,
            if self.undecided == 1 { "" } else { "s" }
        )
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

/// Config key for the Live Photo motion option (absent = off).
pub const KEY_LIVE_MOTION: &str = "livePhotoMotion";
/// Config key for "Show Location in Photo Info" (absent = on; stored only when turned off).
pub const KEY_INFO_LOCATION: &str = "infoShowLocation";

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
    /// The item open in the media viewer (plan §16), if any.
    pub viewer: Option<String>,
    /// Viewer sound: starts muted, remembered for this session only.
    pub muted: bool,
    /// Photo info inspector open (plan §17.2): remembered for this session only, starts closed.
    pub info_open: bool,
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
            viewer: None,
            muted: true,
            info_open: false,
        };
        if let Some(root) = s.root() {
            if root.is_dir() {
                s.live = Some(LiveIndex::from_items(
                    &root,
                    s.snap.config.visit_gap_hours,
                    s.snap.items.clone(),
                ));
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
        self.snap
            .items
            .iter()
            .map(|i| (i.path.clone(), i.decision))
            .collect()
    }

    /// Takes the result of an index run.
    pub fn apply_index(
        &mut self,
        mut items: Vec<MediaItem>,
        groups: Vec<VisitGroup>,
        now: Timestamp,
    ) {
        quality::carry_over(&self.snap.items, &mut items);
        if let Some(root) = self.root() {
            self.live = Some(LiveIndex::from_items(
                &root,
                self.snap.config.visit_gap_hours,
                items.clone(),
            ));
        }
        self.snap.items = items;
        self.snap.groups = groups;
        self.snap.last_indexed_at = Some(now);
        self.after_items_changed();
    }

    /// Applies a debounced folder-watch batch.
    pub fn apply_watch(&mut self, batch: &WatchBatch, dates: &dyn DateReader) -> Delta {
        let Some(live) = self.live.as_mut() else {
            return Delta::default();
        };
        let d = live.apply(batch, dates);
        if !d.is_empty() {
            let mut items = live.items();
            quality::carry_over(&self.snap.items, &mut items);
            self.snap.items = items;
            self.snap.groups = live.groups().to_vec();
            self.sync_live_decisions();
            self.after_items_changed();
        }
        d
    }

    fn after_items_changed(&mut self) {
        // The live index and the indexer group by visit; the other modes regroup here (§20.6).
        if self.group_mode() != GroupMode::Visit {
            self.regroup();
        }
        if let Some(v) = &self.visit {
            if !self.snap.groups.iter().any(|g| &g.id == v) {
                self.close_visit();
            }
        }
        let paths: BTreeSet<&str> = self.snap.items.iter().map(|i| i.path.as_str()).collect();
        self.selection.retain(|p| paths.contains(p.as_str()));
        self.check_viewer();
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
                    has_place: g.place.is_some(),
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
        self.viewer = None;
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
        let Some(g) = self.current_group() else {
            return vec![];
        };
        let by_path: HashMap<&str, &MediaItem> = self
            .snap
            .items
            .iter()
            .map(|i| (i.path.as_str(), i))
            .collect();
        g.item_paths
            .iter()
            .filter_map(|p| by_path.get(p.as_str()).copied())
            .filter(|i| match self.filter {
                Filter::All => true,
                Filter::Photos => i.kind == MediaKind::Photo,
                Filter::Videos => i.kind == MediaKind::Video,
                Filter::Blurry => quality::is_suggested(i),
            })
            .collect()
    }

    pub fn set_filter(&mut self, f: Filter) {
        self.filter = f;
        let visible: BTreeSet<String> = self
            .visible_items()
            .iter()
            .map(|i| i.path.clone())
            .collect();
        self.selection.retain(|p| visible.contains(p));
        self.check_viewer();
    }

    /// Click on a thumbnail: plain = select only this, Ctrl = toggle, Shift = range from anchor.
    pub fn click(&mut self, path: &str, ctrl: bool, shift: bool) {
        let order: Vec<String> = self
            .visible_items()
            .iter()
            .map(|i| i.path.clone())
            .collect();
        if !order.iter().any(|p| p == path) {
            return;
        }
        if shift {
            if let Some(a) = self
                .anchor
                .as_ref()
                .and_then(|a| order.iter().position(|p| p == a))
            {
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
        self.selection = self
            .visible_items()
            .iter()
            .map(|i| i.path.clone())
            .collect();
    }

    /// Applies `d` to the selection; returns how many items changed.
    pub fn decide_selected(&mut self, d: Decision) -> usize {
        let paths: Vec<String> = self.selection.iter().cloned().collect();
        self.decide(&paths, d)
    }

    pub fn decide(&mut self, paths: &[String], d: Decision) -> usize {
        let set: BTreeSet<&str> = paths.iter().map(String::as_str).collect();
        let mut n = 0;
        for i in self
            .snap
            .items
            .iter_mut()
            .filter(|i| set.contains(i.path.as_str()))
        {
            if quality::set_manual_decision(i, d) {
                n += 1;
            }
            if let Some(live) = self.live.as_mut() {
                live.sync_item(i);
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
        self.snap
            .config
            .keep_targets
            .get_or_insert_with(Default::default)
            .insert(id.to_string(), n.max(0));
        self.dirty = true;
    }

    /// `Keep target reached` or `4 of 10 kept`.
    pub fn keep_status(&self, id: &str) -> String {
        let Some(g) = self.snap.groups.iter().find(|g| g.id == id) else {
            return String::new();
        };
        let kept = g.progress(&self.snap.items).kept as i64;
        let target = self.keep_target(id);
        if kept >= target && target > 0 {
            "Keep target reached".into()
        } else {
            format!("{kept} of {target} kept")
        }
    }

    /// The saved grouping mode (missing or unknown = visit).
    pub fn group_mode(&self) -> GroupMode {
        self.snap.config.group_mode()
    }

    /// Changes the grouping mode, saves it in `AppConfig.groupMode` and regroups the library.
    pub fn set_group_mode(&mut self, m: GroupMode) {
        if self.snap.config.group_mode.as_deref() == Some(m.as_str()) {
            return;
        }
        self.snap.config.group_mode = Some(m.as_str().to_string());
        self.regroup();
        self.after_items_changed();
    }

    /// Recomputes the groups from the items with the saved grouping settings.
    fn regroup(&mut self) {
        let settings = GroupSettings::from_config(&self.snap.config);
        self.snap.groups = group_with(&self.snap.items, &settings, &unswiper_geo::Geocoder);
    }

    pub fn set_commit_mode(&mut self, m: CommitMode) {
        self.snap.config.commit_mode = m;
        self.dirty = true;
    }

    /// Members of the open visit (all of them, ignoring the filter).
    fn visit_members(&self) -> Vec<MediaItem> {
        self.current_group()
            .map(|g| commit::visit_members(&self.snap.items, g))
            .unwrap_or_default()
    }

    /// The rejected items of the open visit. Empty when no visit is open.
    pub fn commit_plan(&self) -> CommitPlan {
        let members = self.visit_members();
        let count = |d: Decision| members.iter().filter(|i| i.decision == d).count();
        let rejects: Vec<&MediaItem> = members
            .iter()
            .filter(|i| i.decision == Decision::Reject)
            .collect();
        CommitPlan {
            visit: self
                .current_group()
                .map(|g| g.title.clone())
                .unwrap_or_default(),
            items: rejects.len(),
            keeps: count(Decision::Keep),
            undecided: count(Decision::Undecided),
            files: rejects.iter().map(|i| i.file_count()).sum(),
            bytes: rejects.iter().map(|i| i.total_bytes()).sum(),
            mode: self.snap.config.commit_mode,
            suggested_blurry: rejects.iter().filter(|i| quality::is_suggested(i)).count(),
        }
    }

    /// Whether Review & Commit is available: a visit is open and it has rejects.
    pub fn can_commit(&self) -> bool {
        !self.commit_plan().is_empty()
    }

    /// Runs the commit and drops moved items from the index. Returns the message to show.
    pub fn commit(&mut self, trasher: &dyn Trasher) -> Result<String, String> {
        let mode = self.snap.config.commit_mode;
        let root = self.root();
        let members = self.visit_members();
        if !members.iter().any(|i| i.decision == Decision::Reject) {
            return Err("Open a visit with rejected items first.".into());
        }
        let member_paths: BTreeSet<&str> = members.iter().map(|i| i.path.as_str()).collect();
        // Ignore our own moves in the watcher for a while.
        if let Some(live) = self.live.as_mut() {
            let ours = self
                .snap
                .items
                .iter()
                .filter(|i| {
                    i.decision == Decision::Reject && member_paths.contains(i.path.as_str())
                })
                .flat_map(|i| {
                    std::iter::once(i.path.clone()).chain(i.sidecars.iter().map(|s| s.path.clone()))
                })
                .map(PathBuf::from);
            live.suppress(ours, std::time::Duration::from_secs(10));
        }
        let r = commit::commit_rejects(&members, mode, root.as_deref(), &self.store, trasher)
            .map_err(|e| e.to_string())?;
        if !r.dry_run {
            let gone: Vec<String> = self
                .snap
                .items
                .iter()
                .filter(|i| {
                    i.decision == Decision::Reject
                        && member_paths.contains(i.path.as_str())
                        && !Path::new(&i.path).exists()
                })
                .map(|i| i.path.clone())
                .collect();
            if !gone.is_empty() {
                if let Some(live) = self.live.as_mut() {
                    live.remove_paths(&gone);
                    let mut items = live.items();
                    quality::carry_over(&self.snap.items, &mut items);
                    self.snap.items = items;
                    self.snap.groups = live.groups().to_vec();
                } else {
                    self.snap.items.retain(|i| !gone.contains(&i.path));
                    self.snap.groups = unswiper_core::group::group(
                        &self.snap.items,
                        self.snap.config.visit_gap_hours,
                    );
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
        let mut msg = format!(
            "Restored {} file{}.",
            r.restored,
            if r.restored == 1 { "" } else { "s" }
        );
        if r.already_in_place > 0 {
            msg.push_str(&format!(" {} were already in place.", r.already_in_place));
        }
        if let Some((p, e)) = r.failed.first() {
            msg.push_str(&format!(
                " {} couldn't be restored (first: {}: {e})",
                r.failed.len(),
                short(p)
            ));
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

    // ---- Blur suggestions (plan §15) ----

    pub fn blur_enabled(&self) -> bool {
        quality::enabled(&self.snap.config)
    }

    pub fn blur_sensitivity(&self) -> Sensitivity {
        quality::sensitivity(&self.snap.config)
    }

    /// Turning detection off withdraws every suggestion (scores stay cached).
    pub fn set_blur_enabled(&mut self, on: bool) -> usize {
        quality::set_enabled(&mut self.snap.config, on);
        self.dirty = true;
        if on {
            return 0;
        }
        let n = quality::withdraw_all(&mut self.snap.items);
        self.sync_live_decisions();
        n
    }

    pub fn set_blur_sensitivity(&mut self, s: Sensitivity) {
        quality::set_sensitivity(&mut self.snap.config, s);
        self.dirty = true;
    }

    /// Applies a scan's verdict (see `unswiper_blur::scan::suggestions`).
    pub fn apply_blur(
        &mut self,
        flagged: &HashSet<String>,
        scored: &HashSet<String>,
    ) -> ApplyReport {
        let on = self.blur_enabled();
        let r = quality::apply_suggestions(&mut self.snap.items, flagged, scored, on);
        if r != ApplyReport::default() {
            self.dirty = true;
            self.sync_live_decisions();
        }
        r
    }

    pub fn is_blur_suggested(&self, path: &str) -> bool {
        self.item(path).is_some_and(quality::is_suggested)
    }

    /// Blur suggestions in the open visit.
    pub fn suggested_in_visit(&self) -> Vec<String> {
        self.visit_members()
            .iter()
            .filter(|i| quality::is_suggested(i))
            .map(|i| i.path.clone())
            .collect()
    }

    /// "Keep all suggested": clears every suggestion in the open visit (they become
    /// Undecided and are never suggested again). Returns how many.
    pub fn dismiss_suggested_in_visit(&mut self) -> usize {
        let paths = self.suggested_in_visit();
        self.decide(&paths, Decision::Undecided)
    }

    /// "Reject all suggested": confirms them as the user's own rejects.
    pub fn confirm_suggested_in_visit(&mut self) -> usize {
        let paths = self.suggested_in_visit();
        let n = paths.len();
        self.decide(&paths, Decision::Reject);
        n
    }

    /// Photos the blur scanner should look at: the open visit first, then the rest.
    pub fn blur_scan_order(&self) -> Vec<MediaItem> {
        let first: HashSet<String> = self.visit_members().into_iter().map(|i| i.path).collect();
        let mut v: Vec<MediaItem> = self.snap.items.clone();
        v.sort_by_key(|i| !first.contains(&i.path));
        v
    }

    /// How many leading items of [`Self::blur_scan_order`] belong to the open visit.
    pub fn suggested_scan_first(&self) -> usize {
        self.visit_members().len()
    }

    fn sync_live_decisions(&mut self) {
        if let Some(live) = self.live.as_mut() {
            for i in &self.snap.items {
                live.sync_item(i);
            }
        }
    }

    // ---- Media viewer (plan §16) ----

    /// Opens the viewer on `path` (must be visible in the open visit) and selects it alone,
    /// so K / R / U act on the shown item.
    pub fn open_viewer(&mut self, path: &str) -> bool {
        if !self.visible_items().iter().any(|i| i.path == path) {
            return false;
        }
        self.viewer = Some(path.to_string());
        self.selection = BTreeSet::from([path.to_string()]);
        self.anchor = Some(path.to_string());
        true
    }

    /// Space: opens the first selected item (in grid order), or the first item.
    pub fn open_viewer_on_selection(&mut self) -> bool {
        let pick = {
            let vis = self.visible_items();
            vis.iter()
                .find(|i| self.selection.contains(&i.path))
                .or(vis.first())
                .map(|i| i.path.clone())
        };
        pick.is_some_and(|p| self.open_viewer(&p))
    }

    pub fn close_viewer(&mut self) {
        self.viewer = None;
    }

    /// Left/Right: the previous/next item in grid order (no wrap). Returns whether it moved.
    pub fn viewer_step(&mut self, delta: i32) -> bool {
        let Some((idx, total)) = self.viewer_position() else {
            return false;
        };
        let next = idx as i64 + delta as i64;
        if next < 0 || next >= total as i64 {
            return false;
        }
        let p = self.visible_items()[next as usize].path.clone();
        self.open_viewer(&p)
    }

    /// (0-based index, count) of the viewed item among the visible items.
    pub fn viewer_position(&self) -> Option<(usize, usize)> {
        let v = self.viewer.as_ref()?;
        let vis = self.visible_items();
        vis.iter()
            .position(|i| &i.path == v)
            .map(|i| (i, vis.len()))
    }

    pub fn viewer_item(&self) -> Option<&MediaItem> {
        self.item(self.viewer.as_ref()?)
    }

    /// What the viewer should show and play.
    pub fn viewer_media(&self) -> Option<crate::player::ViewerMedia> {
        let it = self.viewer_item()?;
        Some(crate::player::media_for(it, self.live_motion()))
    }

    /// "Play Live Photo motion" option (default off, stored in library.json config).
    pub fn live_motion(&self) -> bool {
        self.snap
            .config
            .extra
            .get(KEY_LIVE_MOTION)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    }

    pub fn set_live_motion(&mut self, on: bool) {
        if on {
            self.snap
                .config
                .extra
                .insert(KEY_LIVE_MOTION.into(), serde_json::Value::Bool(true));
        } else {
            self.snap.config.extra.remove(KEY_LIVE_MOTION);
        }
        self.dirty = true;
    }

    /// "Show Location in Photo Info" (default on). Location is only ever shown on this computer.
    pub fn show_location(&self) -> bool {
        self.snap
            .config
            .extra
            .get(KEY_INFO_LOCATION)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true)
    }

    pub fn set_show_location(&mut self, on: bool) {
        if on {
            self.snap.config.extra.remove(KEY_INFO_LOCATION);
        } else {
            self.snap
                .config
                .extra
                .insert(KEY_INFO_LOCATION.into(), serde_json::Value::Bool(false));
        }
        self.dirty = true;
    }

    /// The item the photo info shows: the one in the viewer, else the first selected one in
    /// grid order, plus how many others are selected (plan §17.2).
    pub fn info_target(&self) -> Option<(&MediaItem, usize)> {
        if let Some(it) = self.viewer_item() {
            return Some((it, 0));
        }
        let vis = self.visible_items();
        let selected: Vec<&MediaItem> = vis
            .into_iter()
            .filter(|i| self.selection.contains(&i.path))
            .collect();
        let first = *selected.first()?;
        Some((first, selected.len() - 1))
    }

    /// Closes the viewer when its item is gone or filtered out.
    fn check_viewer(&mut self) {
        if self.viewer.is_some() && self.viewer_position().is_none() {
            self.viewer = None;
        }
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

/// True for folders handed over by the document portal (`/run/user/<uid>/doc/…` or
/// `/run/flatpak/doc/…`). That FUSE mount does not deliver inotify events for changes made
/// outside the app, so the folder watcher must poll there.
pub fn is_portal_path(p: &Path) -> bool {
    let s = p.to_string_lossy();
    s.starts_with("/run/flatpak/doc/")
        || (s.starts_with("/run/user/") && s.split('/').nth(4) == Some("doc"))
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
    let place = if mode == CommitMode::Trash {
        "to the Trash"
    } else {
        "to the quarantine folder"
    };
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
        s.apply_index(items, groups, chrono::Utc::now());
        (T { _d: d, root, base }, s)
    }

    fn open_first(s: &mut AppState) {
        let id = s.visits()[0].id.clone();
        assert!(s.open_visit(&id));
    }

    #[test]
    fn portal_paths() {
        assert!(is_portal_path(Path::new(
            "/run/user/1000/doc/ab12cd/Photos"
        )));
        assert!(is_portal_path(Path::new("/run/flatpak/doc/ab12cd/Photos")));
        assert!(!is_portal_path(Path::new("/run/user/1000/gvfs/x")));
        assert!(!is_portal_path(Path::new("/home/r/Pictures")));
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
        assert_eq!(
            p.heading(),
            format!("Move 1 Rejected Item from “{}”?", p.visit)
        );
        assert!(p.body().contains("Trash"));
        assert!(
            p.body()
                .starts_with(&format!("Visit “{}”: 1 rejected item (1 file,", p.visit)),
            "{}",
            p.body()
        );
        assert!(p.body().contains("0 kept and 0 undecided items stay"));
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
        let trash = FreedesktopTrash {
            trash_dir: t.base.join("Trash"),
        };
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
        let trash = FreedesktopTrash {
            trash_dir: t.base.join("Trash"),
        };
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

    fn two_visits() -> (T, AppState, Vec<String>) {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let root = base.join("lib");
        for (dir, n) in [
            ("2026-01-01 Lisbon", "a.jpg"),
            ("2026-01-01 Lisbon", "b.jpg"),
            ("2026-03-01 Porto", "c.jpg"),
        ] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            std::fs::write(root.join(dir).join(n), b"xy").unwrap();
        }
        let mut s = AppState::new(LibraryStore::new(base.join("support")));
        s.set_root(&root);
        let (items, groups) = index(&root, 36.0, &s.decisions(), &MetadataDates, |_| {}).unwrap();
        s.apply_index(items, groups, chrono::Utc::now());
        let all: Vec<String> = s.snap.items.iter().map(|i| i.path.clone()).collect();
        s.decide(&all, Decision::Reject);
        let ids = s.visits().iter().map(|v| v.id.clone()).collect();
        (T { _d: d, root, base }, s, ids)
    }

    #[test]
    fn commit_acts_only_on_open_visit() {
        let (t, mut s, ids) = two_visits();
        assert_eq!(ids.len(), 2);
        let trash = FreedesktopTrash {
            trash_dir: t.base.join("Trash"),
        };
        // No visit open: nothing to commit.
        assert!(!s.can_commit());
        assert!(s.commit_plan().is_empty());
        assert!(s.commit(&trash).is_err());
        let lisbon = s
            .snap
            .groups
            .iter()
            .find(|g| g.title.contains("Lisbon"))
            .unwrap()
            .id
            .clone();
        s.open_visit(&lisbon);
        let p = s.commit_plan();
        assert!(s.can_commit());
        assert_eq!((p.items, p.files, p.bytes), (2, 2, 4));
        assert!(p.visit.contains("Lisbon"));
        assert!(p.heading().contains("Lisbon") && p.heading().starts_with("Move 2 Rejected Items"));
        s.set_commit_mode(CommitMode::Quarantine);
        let msg = s.commit(&trash).unwrap();
        assert!(msg.starts_with("Moved 2 items"), "{msg}");
        // Porto's reject is untouched, on disk and in the index.
        assert!(t.root.join("2026-03-01 Porto/c.jpg").exists());
        assert!(!t.root.join("2026-01-01 Lisbon/a.jpg").exists());
        assert_eq!(s.snap.items.len(), 1);
        assert_eq!(s.snap.items[0].decision, Decision::Reject);
        assert!(!s.can_commit());
        // Restore brings back exactly the Lisbon files.
        let (msg, _) = s.restore_last();
        assert!(msg.starts_with("Restored 2 files"), "{msg}");
        assert!(t.root.join("2026-01-01 Lisbon/a.jpg").exists());
    }

    #[test]
    fn visit_without_rejects_cannot_commit() {
        let (_t, mut s, _) = two_visits();
        let all: Vec<String> = s.snap.items.iter().map(|i| i.path.clone()).collect();
        s.decide(&all, Decision::Keep);
        let id = s.visits()[0].id.clone();
        s.open_visit(&id);
        assert!(!s.can_commit());
        assert_eq!(s.commit_plan().keeps, s.visible_items().len());
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
        let d = s.apply_watch(
            &WatchBatch::Paths([b.clone()].into_iter().collect()),
            &MetadataDates,
        );
        assert_eq!(d.added.len(), 1);
        assert_eq!(s.snap.items.len(), 2);
        std::fs::remove_file(&b).unwrap();
        let d = s.apply_watch(
            &WatchBatch::Paths([b].into_iter().collect()),
            &MetadataDates,
        );
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

    fn paths(s: &AppState) -> Vec<String> {
        s.snap.items.iter().map(|i| i.path.clone()).collect()
    }

    fn hs(v: &[String]) -> HashSet<String> {
        v.iter().cloned().collect()
    }

    #[test]
    fn blur_suggestions_flow() {
        let (t, mut s) = setup(&["a.jpg", "b.jpg", "c.jpg"]);
        open_first(&mut s);
        let all = paths(&s);
        assert!(s.blur_enabled());
        // Manual keep on c wins over the detector.
        s.decide(std::slice::from_ref(&all[2]), Decision::Keep);
        let r = s.apply_blur(&hs(&all), &hs(&all));
        assert_eq!(r.suggested, 2);
        assert!(s.is_blur_suggested(&all[0]) && !s.is_blur_suggested(&all[2]));
        assert_eq!(s.suggested_in_visit().len(), 2);
        s.set_filter(Filter::Blurry);
        assert_eq!(s.visible_items().len(), 2);
        let p = s.commit_plan();
        assert_eq!((p.items, p.suggested_blurry), (2, 2));
        assert!(
            p.body().contains("2 of them were suggested as blurry"),
            "{}",
            p.body()
        );
        // U on one: dismissed, never re-flagged.
        s.decide(std::slice::from_ref(&all[0]), Decision::Undecided);
        s.apply_blur(&hs(&all), &hs(&all));
        assert!(!s.is_blur_suggested(&all[0]));
        assert!(s.commit_plan().body().contains("1 of them was suggested"));
        // Survives save + reload and a re-index.
        s.save().unwrap();
        let mut s2 = AppState::new(LibraryStore::new(t.base.join("support")));
        assert!(s2.is_blur_suggested(&all[1]));
        let (items, groups) =
            index(&t.root, 36.0, &s2.decisions(), &MetadataDates, |_| {}).unwrap();
        s2.apply_index(items, groups, chrono::Utc::now());
        assert!(s2.is_blur_suggested(&all[1]));
        assert!(unswiper_core::quality::is_dismissed(
            s2.item(&all[0]).unwrap()
        ));
        // Commit counts it like any reject; only the open visit, only on commit.
        open_first(&mut s2);
        s2.set_commit_mode(CommitMode::Quarantine);
        assert!(Path::new(&all[1]).exists(), "nothing moves before commit");
        s2.commit(&FreedesktopTrash {
            trash_dir: t.base.join("Trash"),
        })
        .unwrap();
        assert!(!Path::new(&all[1]).exists());
    }

    #[test]
    fn blur_bulk_actions_and_setting() {
        let (_t, mut s) = setup(&["a.jpg", "b.jpg", "c.jpg"]);
        open_first(&mut s);
        let all = paths(&s);
        s.apply_blur(&hs(&all), &hs(&all));
        assert_eq!(s.dismiss_suggested_in_visit(), 3);
        assert!(s.suggested_in_visit().is_empty());
        let (_t, mut s) = setup(&["a.jpg", "b.jpg"]);
        open_first(&mut s);
        let all = paths(&s);
        s.apply_blur(&hs(&all), &hs(&all));
        assert_eq!(s.confirm_suggested_in_visit(), 2);
        assert!(s.suggested_in_visit().is_empty());
        assert_eq!(s.commit_plan().suggested_blurry, 0);
        assert_eq!(s.commit_plan().items, 2);
        // Setting off withdraws suggestions and blocks new ones; sensitivity round-trips.
        let (_t, mut s) = setup(&["a.jpg", "b.jpg"]);
        open_first(&mut s);
        let all = paths(&s);
        assert_eq!(s.blur_scan_order().len(), 2);
        assert_eq!(s.suggested_scan_first(), 2);
        s.apply_blur(&hs(&all), &hs(&all));
        assert_eq!(s.set_blur_enabled(false), 2);
        assert!(!s.blur_enabled());
        assert_eq!(s.apply_blur(&hs(&all), &hs(&all)), ApplyReport::default());
        assert_eq!(s.set_blur_enabled(true), 0);
        s.set_blur_sensitivity(Sensitivity::High);
        assert_eq!(s.blur_sensitivity(), Sensitivity::High);
    }

    #[test]
    fn blur_state_survives_watch() {
        let (t, mut s) = setup(&["a.jpg"]);
        let all = paths(&s);
        s.apply_blur(&hs(&all), &hs(&all));
        let b = t.root.join("b.jpg");
        std::fs::write(&b, b"x").unwrap();
        s.apply_watch(
            &WatchBatch::Paths([b].into_iter().collect()),
            &MetadataDates,
        );
        assert!(s.is_blur_suggested(&all[0]));
    }

    #[test]
    fn viewer_navigation_and_decisions() {
        let (_t, mut s) = setup(&["a.jpg", "b.jpg", "c.mp4"]);
        assert!(!s.open_viewer_on_selection(), "no visit open");
        open_first(&mut s);
        let order: Vec<String> = s.visible_items().iter().map(|i| i.path.clone()).collect();
        // Space with nothing selected opens the first item.
        assert!(s.open_viewer_on_selection());
        assert_eq!(s.viewer_position(), Some((0, 3)));
        assert!(!s.viewer_step(-1), "no wrap at the start");
        assert!(s.viewer_step(1));
        assert_eq!(s.viewer.as_deref(), Some(order[1].as_str()));
        assert_eq!(s.selection, BTreeSet::from([order[1].clone()]));
        // K / R / U act on the viewed item.
        assert_eq!(s.decide_selected(Decision::Reject), 1);
        assert_eq!(s.viewer_item().unwrap().decision, Decision::Reject);
        assert!(s.viewer_step(1));
        assert!(!s.viewer_step(1), "no wrap at the end");
        let m = s.viewer_media().unwrap();
        assert_eq!(m.play.as_deref(), Some(order[2].as_str()));
        // Selection decides where Space opens.
        s.close_viewer();
        s.click(&order[1], false, false);
        assert!(s.open_viewer_on_selection());
        assert_eq!(s.viewer_position(), Some((1, 3)));
        assert!(!s.open_viewer("/not/here"));
        assert!(s.muted, "starts muted");
    }

    #[test]
    fn viewer_follows_filter_and_removals() {
        let (t, mut s) = setup(&["a.jpg", "c.mp4"]);
        open_first(&mut s);
        let video = s
            .visible_items()
            .iter()
            .find(|i| i.kind == MediaKind::Video)
            .unwrap()
            .path
            .clone();
        assert!(s.open_viewer(&video));
        s.set_filter(Filter::Videos);
        assert_eq!(s.viewer_position(), Some((0, 1)), "still visible");
        s.set_filter(Filter::Photos);
        assert!(s.viewer.is_none(), "filtered out: viewer closes");
        s.set_filter(Filter::All);
        assert!(s.open_viewer(&video));
        std::fs::remove_file(&video).unwrap();
        s.apply_watch(
            &WatchBatch::Paths([PathBuf::from(&video)].into_iter().collect()),
            &MetadataDates,
        );
        assert!(s.viewer.is_none(), "removed: viewer closes");
        let a = s.visible_items()[0].path.clone();
        assert!(s.open_viewer(&a));
        s.close_visit();
        assert!(s.viewer.is_none());
        let _ = t;
    }

    #[test]
    fn info_target_follows_viewer_and_selection() {
        let (_t, mut s) = setup(&["a.jpg", "b.jpg", "c.jpg"]);
        assert!(s.info_target().is_none(), "no visit");
        open_first(&mut s);
        let p = paths(&s);
        assert!(s.info_target().is_none(), "nothing selected");
        s.click(&p[2], false, false);
        s.click(&p[1], true, false);
        let (it, others) = s.info_target().unwrap();
        assert_eq!(
            (it.path.as_str(), others),
            (p[1].as_str(), 1),
            "first in grid order"
        );
        assert!(s.open_viewer(&p[0]));
        assert_eq!(s.info_target().unwrap(), (s.item(&p[0]).unwrap(), 0));
        assert!(s.viewer_step(1));
        assert_eq!(s.info_target().unwrap().0.path, p[1]);
        s.close_viewer();
        s.set_filter(Filter::Videos);
        assert!(s.info_target().is_none(), "selection filtered out");
    }

    #[test]
    fn show_location_setting_round_trips_and_defaults_on() {
        let (t, mut s) = setup(&["a.jpg"]);
        assert!(s.show_location());
        s.set_show_location(false);
        assert!(!s.show_location());
        s.save().unwrap();
        let s2 = AppState::new(LibraryStore::new(t.base.join("support")));
        assert!(!s2.show_location());
        s.set_show_location(true);
        assert!(s.show_location());
        assert!(!s.snap.config.extra.contains_key(KEY_INFO_LOCATION));
    }

    #[test]
    fn live_motion_setting() {
        let (t, mut s) = setup(&["IMG_1.HEIC", "IMG_1.MOV"]);
        open_first(&mut s);
        assert!(!s.live_motion());
        let p = s.visible_items()[0].path.clone();
        assert!(s.open_viewer(&p));
        assert_eq!(s.viewer_media().unwrap().play, None);
        s.set_live_motion(true);
        let m = s.viewer_media().unwrap();
        assert!(m.is_motion);
        assert!(m.play.unwrap().ends_with("IMG_1.MOV"));
        s.save().unwrap();
        let s2 = AppState::new(LibraryStore::new(t.base.join("support")));
        assert!(s2.live_motion());
        s.set_live_motion(false);
        assert!(!s.live_motion());
    }

    #[test]
    fn group_mode_picker_regroups_and_persists() {
        let (t, mut s) = setup(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
        assert_eq!(s.group_mode(), GroupMode::Visit);
        let visit_groups = s.snap.groups.clone();
        // Made-up coordinates (synthetic, plan §20.8).
        for it in s.snap.items.iter_mut().take(3) {
            it.location = unswiper_core::model::Location::new(
                10.0,
                20.0,
                unswiper_core::model::LocationSource::Exif,
            );
        }
        s.set_group_mode(GroupMode::Geo);
        assert_eq!(s.snap.config.group_mode.as_deref(), Some("geo"));
        assert!(s.needs_save());
        assert!(s.snap.groups.iter().any(|g| g.parent_visit_id.is_some()));
        let n: usize = s.snap.groups.iter().map(|g| g.item_count()).sum();
        assert_eq!(n, 4);
        s.save().unwrap();
        let s2 = AppState::new(LibraryStore::new(t.base.join("support")));
        assert_eq!(s2.group_mode(), GroupMode::Geo);
        assert_eq!(s2.snap.groups, s.snap.groups);
        s.set_group_mode(GroupMode::Visit);
        assert_eq!(s.group_mode(), GroupMode::Visit);
        assert_eq!(s.snap.groups, visit_groups);
    }
}
