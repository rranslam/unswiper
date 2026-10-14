// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! The relm4 layer: builds the GNOME window and turns signals into [`AppState`] calls.
//! Everything worth testing lives in [`crate::state`]; this module only wires widgets.

use crate::state::{AppState, Filter};
use crate::{help, thumbs, APP_ID, ISSUES, VERSION, WEBSITE};
use relm4::adw::prelude::*;
use relm4::gtk::{gdk, gio, glib};
use relm4::{adw, gtk, Component, ComponentParts, ComponentSender};
use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use unswiper_core::index::{index, MetadataDates};
use unswiper_core::store::{default_support_dir, LibraryStore};
use unswiper_core::trash::FreedesktopTrash;
use unswiper_core::watch::{FolderWatcher, WatchBatch, DEFAULT_MAX_WAIT, DEFAULT_QUIET};
use unswiper_core::{CommitMode, Decision, MediaItem, VisitGroup};

const THUMB: u32 = 220;

/// Launch options (from the command line).
#[derive(Debug, Default, Clone)]
pub struct Options {
    /// Support folder override (tests, screenshots). Default `$XDG_DATA_HOME/unswiper`.
    pub support_dir: Option<PathBuf>,
    /// Pick this folder and index it right away.
    pub open: Option<PathBuf>,
    /// Open the first visit after indexing (screenshots).
    pub open_first_visit: bool,
    /// Quit after this many seconds (headless smoke test).
    pub quit_after: Option<u32>,
}

pub fn run(opts: Options) {
    let app = relm4::RelmApp::new(APP_ID).with_args(vec![]);
    relm4::set_global_css(CSS);
    app.run::<App>(opts);
}

const CSS: &str = "
.thumb { border-radius: 8px; padding: 3px; border: 3px solid transparent; }
.thumb.picked { border-color: @accent_bg_color; }
.badge { border-radius: 99px; padding: 2px 8px; margin: 6px; font-weight: bold; }
.badge.keep { background: @success_bg_color; color: @success_fg_color; }
.badge.reject { background: @error_bg_color; color: @error_fg_color; }
.thumb.rejected picture { opacity: 0.45; }
";

pub struct ThumbMsg(pub thumbs::Thumb);
impl std::fmt::Debug for ThumbMsg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Thumb({}x{})", self.0.width, self.0.height)
    }
}

type IndexResult = Result<(Vec<MediaItem>, Vec<VisitGroup>), String>;

#[derive(Debug)]
pub enum Msg {
    ChooseFolder,
    FolderChosen(PathBuf),
    Reindex,
    IndexProgress(usize, usize),
    Indexed(IndexResult),
    OpenVisit(i32),
    Click(String, bool, bool),
    SelectAll,
    Decide(Decision),
    SetFilter(Filter),
    SetKeepTarget(i64),
    ReviewCommit,
    ConfirmCommit,
    RestoreLast,
    ResolvePending(bool),
    SetMode(CommitMode),
    Watch(WatchBatch),
    Thumb(u64, String, ThumbMsg),
    Save,
    Help,
    About,
    Quit,
}

pub struct App {
    state: AppState,
    opts: Options,
    watcher: Option<FolderWatcher>,
    thumb_gen: u64,
    save_scheduled: bool,
    indexing: bool,
}

pub struct Widgets {
    toasts: adw::ToastOverlay,
    stack: gtk::Stack,
    progress: adw::StatusPage,
    visits: gtk::ListBox,
    visit_title: gtk::Label,
    keep_spin: gtk::SpinButton,
    keep_status: gtk::Label,
    grid: gtk::FlowBox,
    cells: HashMap<String, (gtk::Box, gtk::Picture, gtk::Label)>,
    selection_label: gtk::Label,
    commit_button: gtk::Button,
    filter_buttons: Vec<(Filter, gtk::ToggleButton)>,
    restore_action: gio::SimpleAction,
    mode_action: gio::SimpleAction,
    syncing: Rc<Cell<bool>>,
}

fn mode_name(m: CommitMode) -> &'static str {
    match m {
        CommitMode::Trash => "trash",
        CommitMode::Quarantine => "quarantine",
        CommitMode::DryRun => "dry-run",
    }
}

fn action(
    win: &adw::ApplicationWindow,
    name: &str,
    sender: &ComponentSender<App>,
    msg: fn() -> Msg,
) -> gio::SimpleAction {
    let a = gio::SimpleAction::new(name, None);
    let s = sender.clone();
    a.connect_activate(move |_, _| s.input(msg()));
    win.add_action(&a);
    a
}

fn shortcut(
    ctl: &gtk::ShortcutController,
    trigger: &str,
    sender: &ComponentSender<App>,
    msg: fn() -> Msg,
) {
    let s = sender.clone();
    let act = gtk::CallbackAction::new(move |_, _| {
        s.input(msg());
        glib::Propagation::Stop
    });
    ctl.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string(trigger),
        Some(act),
    ));
}

impl Component for App {
    type CommandOutput = ();
    type Input = Msg;
    type Output = ();
    type Init = Options;
    type Root = adw::ApplicationWindow;
    type Widgets = Widgets;

    fn init_root() -> Self::Root {
        adw::ApplicationWindow::builder()
            .title("Unswiper")
            .default_width(1180)
            .default_height(760)
            .width_request(720)
            .height_request(480)
            .build()
    }

    fn init(
        opts: Options,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let support = opts.support_dir.clone().unwrap_or_else(default_support_dir);
        let state = AppState::new(LibraryStore::new(support));

        // Header + menu.
        let header = adw::HeaderBar::new();
        let menu = gio::Menu::new();
        let lib = gio::Menu::new();
        lib.append(Some("Choose Folder…"), Some("win.choose"));
        lib.append(Some("Re-index"), Some("win.reindex"));
        lib.append(Some("Restore Last Commit"), Some("win.restore"));
        menu.append_section(None, &lib);
        let modes = gio::Menu::new();
        modes.append(Some("Move to Trash"), Some("win.mode::trash"));
        modes.append(
            Some("Move to Quarantine Folder"),
            Some("win.mode::quarantine"),
        );
        modes.append(Some("Dry Run"), Some("win.mode::dry-run"));
        menu.append_section(Some("Commit Mode"), &modes);
        let app_sec = gio::Menu::new();
        app_sec.append(Some("Keyboard Shortcuts & Help"), Some("win.help"));
        app_sec.append(Some("About Unswiper"), Some("win.about"));
        menu.append_section(None, &app_sec);
        header.pack_end(
            &gtk::MenuButton::builder()
                .icon_name("open-menu-symbolic")
                .menu_model(&menu)
                .tooltip_text("Main Menu")
                .primary(true)
                .build(),
        );

        action(&root, "choose", &sender, || Msg::ChooseFolder);
        action(&root, "reindex", &sender, || Msg::Reindex);
        let restore_action = action(&root, "restore", &sender, || Msg::RestoreLast);
        action(&root, "help", &sender, || Msg::Help);
        action(&root, "about", &sender, || Msg::About);
        action(&root, "quit", &sender, || Msg::Quit);
        let mode_action = gio::SimpleAction::new_stateful(
            "mode",
            Some(glib::VariantTy::STRING),
            &mode_name(state.snap.config.commit_mode).to_variant(),
        );
        {
            let s = sender.clone();
            mode_action.connect_activate(move |a, v| {
                let Some(name) = v.and_then(|v| v.get::<String>()) else {
                    return;
                };
                a.set_state(&name.to_variant());
                let m = match name.as_str() {
                    "trash" => CommitMode::Trash,
                    "quarantine" => CommitMode::Quarantine,
                    _ => CommitMode::DryRun,
                };
                s.input(Msg::SetMode(m));
            });
        }
        root.add_action(&mode_action);

        // Shortcuts. Bubble phase, so a focused spin button keeps its own keys.
        let ctl = gtk::ShortcutController::new();
        ctl.set_scope(gtk::ShortcutScope::Managed);
        shortcut(&ctl, "k", &sender, || Msg::Decide(Decision::Keep));
        shortcut(&ctl, "r", &sender, || Msg::Decide(Decision::Reject));
        shortcut(&ctl, "u", &sender, || Msg::Decide(Decision::Undecided));
        shortcut(&ctl, "<Control>a", &sender, || Msg::SelectAll);
        shortcut(&ctl, "<Control>Return", &sender, || Msg::ReviewCommit);
        shortcut(&ctl, "<Control><Shift>r", &sender, || Msg::RestoreLast);
        shortcut(&ctl, "<Control>o", &sender, || Msg::ChooseFolder);
        shortcut(&ctl, "F5", &sender, || Msg::Reindex);
        shortcut(&ctl, "F1", &sender, || Msg::Help);
        shortcut(&ctl, "<Control>w", &sender, || Msg::Quit);
        shortcut(&ctl, "<Control>q", &sender, || Msg::Quit);
        root.add_controller(ctl);

        // Setup page.
        let choose = gtk::Button::builder()
            .label("Choose Folder…")
            .halign(gtk::Align::Center)
            .css_classes(["pill", "suggested-action"])
            .build();
        {
            let s = sender.clone();
            choose.connect_clicked(move |_| s.input(Msg::ChooseFolder));
        }
        let setup = adw::StatusPage::builder()
            .icon_name("folder-pictures-symbolic")
            .title("Choose Your Photo Library")
            .description("Pick the folder with your photos and videos. Unswiper groups them into visits so you can keep the best and set aside the rest. Nothing moves until you commit.")
            .child(&choose)
            .build();

        // Indexing page.
        let spinner = gtk::Spinner::builder()
            .spinning(true)
            .width_request(48)
            .height_request(48)
            .build();
        let progress = adw::StatusPage::builder()
            .title("Indexing…")
            .description("Looking for photos and videos")
            .child(&spinner)
            .build();

        // Main page: visits on the left, grid on the right.
        let visits = gtk::ListBox::builder()
            .css_classes(["navigation-sidebar"])
            .build();
        {
            let s = sender.clone();
            visits.connect_row_activated(move |_, row| s.input(Msg::OpenVisit(row.index())));
        }
        let side = gtk::ScrolledWindow::builder()
            .child(&visits)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .width_request(300)
            .build();

        let visit_title = gtk::Label::builder()
            .css_classes(["title-3"])
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let keep_spin = gtk::SpinButton::with_range(0.0, 999.0, 1.0);
        keep_spin.set_tooltip_text(Some("Keep target for this visit"));
        let syncing = Rc::new(Cell::new(false));
        {
            let s = sender.clone();
            let sy = syncing.clone();
            keep_spin.connect_value_changed(move |b| {
                if !sy.get() {
                    s.input(Msg::SetKeepTarget(b.value() as i64));
                }
            });
        }
        let keep_status = gtk::Label::builder().css_classes(["dim-label"]).build();
        let filter_box = gtk::Box::builder().css_classes(["linked"]).build();
        let mut filter_buttons = vec![];
        let mut first: Option<gtk::ToggleButton> = None;
        for (f, label) in [
            (Filter::All, "All"),
            (Filter::Photos, "Photos"),
            (Filter::Videos, "Videos"),
        ] {
            let b = gtk::ToggleButton::with_label(label);
            if let Some(g) = &first {
                b.set_group(Some(g));
            } else {
                b.set_active(true);
                first = Some(b.clone());
            }
            let s = sender.clone();
            let sy = syncing.clone();
            b.connect_toggled(move |b| {
                if b.is_active() && !sy.get() {
                    s.input(Msg::SetFilter(f));
                }
            });
            filter_box.append(&b);
            filter_buttons.push((f, b));
        }
        let top = gtk::Box::builder()
            .spacing(12)
            .margin_start(18)
            .margin_end(18)
            .margin_top(12)
            .margin_bottom(6)
            .build();
        top.append(&visit_title);
        top.append(&gtk::Label::new(Some("Keep target")));
        top.append(&keep_spin);
        top.append(&keep_status);
        top.append(&filter_box);

        let grid = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .homogeneous(true)
            .min_children_per_line(2)
            .max_children_per_line(12)
            .row_spacing(6)
            .column_spacing(6)
            .margin_start(12)
            .margin_end(12)
            .valign(gtk::Align::Start)
            .build();
        let grid_scroll = gtk::ScrolledWindow::builder()
            .child(&grid)
            .vexpand(true)
            .build();
        let empty = adw::StatusPage::builder()
            .icon_name("view-grid-symbolic")
            .title("Pick a Visit")
            .description("Choose a visit on the left to review its photos and videos.")
            .vexpand(true)
            .build();
        let content = gtk::Stack::new();
        content.add_named(&empty, Some("empty"));
        content.add_named(&grid_scroll, Some("grid"));

        let bar = gtk::ActionBar::new();
        let mk = |label: &str, tip: &str, d: Decision| {
            let b = gtk::Button::builder()
                .label(label)
                .tooltip_text(tip)
                .build();
            let s = sender.clone();
            b.connect_clicked(move |_| s.input(Msg::Decide(d)));
            b
        };
        let keep_b = mk("Keep", "Keep selected (K)", Decision::Keep);
        keep_b.add_css_class("success");
        let rej_b = mk("Reject", "Reject selected (R)", Decision::Reject);
        rej_b.add_css_class("error");
        bar.pack_start(&keep_b);
        bar.pack_start(&rej_b);
        bar.pack_start(&mk("Clear", "Clear decision (U)", Decision::Undecided));
        let selection_label = gtk::Label::builder().css_classes(["dim-label"]).build();
        bar.set_center_widget(Some(&selection_label));
        let commit_button = gtk::Button::builder()
            .label("Review & Commit…")
            .css_classes(["suggested-action"])
            .tooltip_text("Move this visit’s rejected items (Ctrl+Enter)")
            .build();
        {
            let s = sender.clone();
            commit_button.connect_clicked(move |_| s.input(Msg::ReviewCommit));
        }
        bar.pack_end(&commit_button);

        let right = gtk::Box::new(gtk::Orientation::Vertical, 0);
        right.append(&top);
        right.append(&content);
        right.append(&bar);
        let paned = gtk::Paned::builder()
            .start_child(&side)
            .end_child(&right)
            .position(320)
            .shrink_start_child(false)
            .build();
        {
            // The "empty" placeholder flips to the grid once a visit opens.
            let c = content.clone();
            visit_title.connect_label_notify(move |l| {
                c.set_visible_child_name(if l.label().is_empty() {
                    "empty"
                } else {
                    "grid"
                })
            });
        }

        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        stack.add_named(&setup, Some("setup"));
        stack.add_named(&progress, Some("indexing"));
        stack.add_named(&paned, Some("main"));

        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&stack));
        let tv = adw::ToolbarView::new();
        tv.add_top_bar(&header);
        tv.set_content(Some(&toasts));
        root.set_content(Some(&tv));
        {
            let s = sender.clone();
            root.connect_close_request(move |_| {
                s.input(Msg::Save);
                glib::Propagation::Proceed
            });
        }

        let mut model = App {
            state,
            opts: opts.clone(),
            watcher: None,
            thumb_gen: 0,
            save_scheduled: false,
            indexing: false,
        };
        let mut widgets = Widgets {
            toasts,
            stack,
            progress,
            visits,
            visit_title,
            keep_spin,
            keep_status,
            grid,
            cells: HashMap::new(),
            selection_label,
            commit_button,
            filter_buttons,
            restore_action,
            mode_action,
            syncing,
        };

        if let Some(p) = model.state.load_problem.take() {
            widgets.toasts.add_toast(adw::Toast::new(&p));
        }
        if let Some(dir) = opts.open.clone() {
            sender.input(Msg::FolderChosen(dir));
        } else if model.state.has_library() {
            model.start_watcher(&sender);
        }
        model.refresh(&mut widgets);
        if !model.state.pending_commits().is_empty() {
            let d = adw::AlertDialog::new(
                Some("Unfinished Commit"),
                Some("Unswiper was closed while moving files. Put the moved files back where they were, or leave them where they are now?"),
            );
            d.add_response("leave", "Leave As Is");
            d.add_response("restore", "Restore Files");
            d.set_response_appearance("restore", adw::ResponseAppearance::Suggested);
            d.set_default_response(Some("restore"));
            d.set_close_response("leave");
            let s = sender.clone();
            d.connect_response(None, move |_, r| {
                s.input(Msg::ResolvePending(r == "restore"))
            });
            let w = root.clone();
            glib::idle_add_local_once(move || d.present(Some(&w)));
        }
        if let Some(secs) = opts.quit_after {
            let s = sender.clone();
            glib::timeout_add_seconds_local_once(secs, move || s.input(Msg::Quit));
        }
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        w: &mut Widgets,
        msg: Msg,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        let mut changed = true;
        match msg {
            Msg::ChooseFolder => {
                changed = false;
                let dlg = gtk::FileDialog::builder()
                    .title("Choose Your Photo Library")
                    .modal(true)
                    .build();
                if let Some(r) = self.state.root() {
                    dlg.set_initial_folder(Some(&gio::File::for_path(r)));
                }
                let s = sender.clone();
                dlg.select_folder(Some(root), None::<&gio::Cancellable>, move |res| {
                    if let Some(p) = res.ok().and_then(|f| f.path()) {
                        s.input(Msg::FolderChosen(p));
                    }
                });
            }
            Msg::FolderChosen(p) => {
                self.state.set_root(&p);
                self.start_index(&sender, w);
            }
            Msg::Reindex => {
                if self.state.root().is_some() {
                    self.start_index(&sender, w);
                }
            }
            Msg::IndexProgress(seen, found) => {
                changed = false;
                w.progress.set_description(Some(&format!(
                    "{seen} files checked · {found} photos and videos found"
                )));
            }
            Msg::Indexed(r) => {
                self.indexing = false;
                match r {
                    Ok((items, groups)) => {
                        let n = items.len();
                        self.state.apply_index(items, groups, chrono::Utc::now());
                        self.start_watcher(&sender);
                        w.toasts.add_toast(adw::Toast::new(&format!(
                            "Indexed {n} items in {} visits.",
                            self.state.snap.groups.len()
                        )));
                        if self.opts.open_first_visit && !self.state.snap.groups.is_empty() {
                            self.opts.open_first_visit = false;
                            sender.input(Msg::OpenVisit(0));
                        }
                    }
                    Err(e) => w.toasts.add_toast(adw::Toast::new(&e)),
                }
                self.rebuild_grid(w, &sender);
            }
            Msg::OpenVisit(i) => {
                let id = usize::try_from(i)
                    .ok()
                    .and_then(|i| self.state.snap.groups.get(i))
                    .map(|g| g.id.clone());
                if let Some(id) = id {
                    self.state.open_visit(&id);
                    self.rebuild_grid(w, &sender);
                }
            }
            Msg::Click(p, ctrl, shift) => self.state.click(&p, ctrl, shift),
            Msg::SelectAll => self.state.select_all(),
            Msg::Decide(d) => {
                self.state.decide_selected(d);
            }
            Msg::SetFilter(f) => {
                self.state.set_filter(f);
                self.rebuild_grid(w, &sender);
            }
            Msg::SetKeepTarget(n) => {
                if let Some(id) = self.state.visit.clone() {
                    self.state.set_keep_target(&id, n);
                }
            }
            Msg::ReviewCommit => {
                changed = false;
                let plan = self.state.commit_plan();
                if plan.is_empty() {
                    w.toasts
                        .add_toast(adw::Toast::new(if self.state.visit.is_none() {
                            "Open a visit to commit its rejected items."
                        } else {
                            "Nothing to commit in this visit. Reject some items first."
                        }));
                } else {
                    let d = adw::AlertDialog::new(Some(&plan.heading()), Some(&plan.body()));
                    d.add_response("cancel", "Cancel");
                    d.add_response("confirm", plan.confirm_label());
                    if plan.mode != CommitMode::DryRun {
                        d.set_response_appearance("confirm", adw::ResponseAppearance::Destructive);
                    }
                    // Enter must never commit: Cancel is both the default and the close response.
                    d.set_default_response(Some("cancel"));
                    d.set_close_response("cancel");
                    let s = sender.clone();
                    d.connect_response(Some("confirm"), move |_, _| s.input(Msg::ConfirmCommit));
                    d.present(Some(root));
                }
            }
            Msg::ConfirmCommit => {
                let msg = self
                    .state
                    .commit(&FreedesktopTrash::home())
                    .unwrap_or_else(|e| e);
                w.toasts.add_toast(adw::Toast::new(&msg));
                self.rebuild_grid(w, &sender);
            }
            Msg::RestoreLast => {
                let (msg, reindex) = self.state.restore_last();
                w.toasts.add_toast(adw::Toast::new(&msg));
                if reindex {
                    self.start_index(&sender, w);
                }
            }
            Msg::ResolvePending(restore) => {
                if let Some((msg, reindex)) = self.state.resolve_pending(restore) {
                    w.toasts.add_toast(adw::Toast::new(&msg));
                    if reindex && self.state.root().is_some() {
                        self.start_index(&sender, w);
                    }
                }
            }
            Msg::SetMode(m) => self.state.set_commit_mode(m),
            Msg::Watch(b) => {
                if self.indexing {
                    changed = false;
                } else {
                    let d = self.state.apply_watch(&b, &MetadataDates);
                    changed = !d.is_empty();
                    if changed {
                        self.rebuild_grid(w, &sender);
                    }
                }
            }
            Msg::Thumb(gen, path, t) => {
                changed = false;
                if gen == self.thumb_gen {
                    if let Some((_, pic, _)) = w.cells.get(&path) {
                        let t = t.0;
                        let bytes = glib::Bytes::from_owned(t.rgba);
                        let tex = gdk::MemoryTexture::new(
                            t.width as i32,
                            t.height as i32,
                            gdk::MemoryFormat::R8g8b8a8,
                            &bytes,
                            t.width as usize * 4,
                        );
                        pic.set_paintable(Some(&tex));
                    }
                }
            }
            Msg::Save => {
                changed = false;
                self.save_scheduled = false;
                if self.state.needs_save() {
                    if let Err(e) = self.state.save() {
                        w.toasts
                            .add_toast(adw::Toast::new(&format!("Couldn't save: {e}")));
                    }
                }
            }
            Msg::Help => {
                changed = false;
                help_window(root).present();
            }
            Msg::About => {
                changed = false;
                about_dialog().present(Some(root));
            }
            Msg::Quit => {
                changed = false;
                if self.state.needs_save() {
                    let _ = self.state.save();
                }
                self.watcher = None;
                relm4::main_application().quit();
            }
        }
        if changed {
            self.refresh(w);
            if self.state.needs_save() && !self.save_scheduled {
                self.save_scheduled = true;
                let s = sender.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(800), move || {
                    s.input(Msg::Save)
                });
            }
        }
    }
}

impl App {
    fn start_index(&mut self, sender: &ComponentSender<Self>, w: &mut Widgets) {
        let Some(root) = self.state.root() else {
            return;
        };
        self.watcher = None;
        self.indexing = true;
        w.progress
            .set_description(Some("Looking for photos and videos"));
        w.stack.set_visible_child_name("indexing");
        let gap = self.state.snap.config.visit_gap_hours;
        let decisions = self.state.decisions();
        let tx = sender.input_sender().clone();
        std::thread::spawn(move || {
            let mut last = std::time::Instant::now();
            let r = index(&root, gap, &decisions, &MetadataDates, |p| {
                if last.elapsed().as_millis() > 100 {
                    last = std::time::Instant::now();
                    tx.emit(Msg::IndexProgress(p.files_seen, p.media_found));
                }
            });
            tx.emit(Msg::Indexed(r.map_err(|e| e.to_string())));
        });
    }

    fn start_watcher(&mut self, sender: &ComponentSender<Self>) {
        let Some(root) = self.state.root() else {
            return;
        };
        let tx = sender.input_sender().clone();
        self.watcher = FolderWatcher::start(&root, DEFAULT_QUIET, DEFAULT_MAX_WAIT, move |b| {
            tx.emit(Msg::Watch(b))
        })
        .ok();
    }

    /// Rebuilds the thumbnail grid for the open visit and starts loading thumbnails.
    fn rebuild_grid(&mut self, w: &mut Widgets, sender: &ComponentSender<Self>) {
        self.thumb_gen += 1;
        w.grid.remove_all();
        w.cells.clear();
        let items: Vec<MediaItem> = self.state.visible_items().into_iter().cloned().collect();
        for it in &items {
            let pic = gtk::Picture::builder()
                .content_fit(gtk::ContentFit::Cover)
                .width_request(THUMB as i32 * 3 / 4)
                .height_request(THUMB as i32 * 3 / 4)
                .build();
            let badge = gtk::Label::builder()
                .halign(gtk::Align::End)
                .valign(gtk::Align::Start)
                .css_classes(["badge"])
                .visible(false)
                .build();
            let overlay = gtk::Overlay::builder().child(&pic).build();
            overlay.add_overlay(&badge);
            if it.kind == unswiper_core::MediaKind::Video {
                let icon = gtk::Image::builder()
                    .icon_name("media-playback-start-symbolic")
                    .halign(gtk::Align::Start)
                    .valign(gtk::Align::End)
                    .margin_start(6)
                    .margin_bottom(6)
                    .pixel_size(20)
                    .build();
                overlay.add_overlay(&icon);
            }
            let name = it.path.rsplit('/').next().unwrap_or(&it.path).to_string();
            let cell = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .css_classes(["thumb"])
                .tooltip_text(&name)
                .build();
            cell.append(&overlay);
            cell.append(
                &gtk::Label::builder()
                    .label(&name)
                    .ellipsize(gtk::pango::EllipsizeMode::Middle)
                    .css_classes(["caption"])
                    .build(),
            );
            let click = gtk::GestureClick::new();
            let s = sender.clone();
            let p = it.path.clone();
            click.connect_pressed(move |g, _, _, _| {
                let m = g.current_event_state();
                s.input(Msg::Click(
                    p.clone(),
                    m.contains(gdk::ModifierType::CONTROL_MASK),
                    m.contains(gdk::ModifierType::SHIFT_MASK),
                ));
            });
            cell.add_controller(click);
            w.grid.append(&cell);
            w.cells.insert(it.path.clone(), (cell, pic, badge));
        }
        let gen = self.thumb_gen;
        let tx = sender.input_sender().clone();
        let jobs: Vec<(String, unswiper_core::MediaKind)> =
            items.iter().map(|i| (i.path.clone(), i.kind)).collect();
        std::thread::spawn(move || {
            let cache = thumbs::default_cache_dir();
            for (p, k) in jobs {
                if let Some(t) = thumbs::load(std::path::Path::new(&p), k, THUMB, Some(&cache)) {
                    if tx.send(Msg::Thumb(gen, p, ThumbMsg(t))).is_err() {
                        break;
                    }
                }
            }
        });
    }

    /// Syncs every cheap widget with the state.
    fn refresh(&self, w: &mut Widgets) {
        w.syncing.set(true);
        let st = &self.state;
        if !self.indexing {
            w.stack
                .set_visible_child_name(if st.has_library() { "main" } else { "setup" });
        }
        w.restore_action.set_enabled(st.can_restore());
        w.mode_action
            .set_state(&mode_name(st.snap.config.commit_mode).to_variant());

        // Visits list (rebuilt; it is small) with the open one selected.
        w.visits.remove_all();
        for (i, v) in st.visits().iter().enumerate() {
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&v.title))
                .subtitle(if v.dates.is_empty() {
                    v.counts.clone()
                } else {
                    format!("{} · {}", v.dates, v.counts)
                })
                .activatable(true)
                .build();
            let prog = gtk::Label::builder()
                .label(&v.progress)
                .css_classes(["caption", "dim-label"])
                .build();
            row.add_suffix(&prog);
            if v.done {
                row.add_suffix(&gtk::Image::from_icon_name("emblem-ok-symbolic"));
            }
            w.visits.append(&row);
            if st.visit.as_deref() == Some(v.id.as_str()) {
                w.visits
                    .select_row(w.visits.row_at_index(i as i32).as_ref());
            }
        }

        match st.current_group() {
            Some(g) => {
                w.visit_title.set_label(&g.title);
                w.keep_spin.set_sensitive(true);
                w.keep_spin.set_value(st.keep_target(&g.id) as f64);
                w.keep_status.set_label(&st.keep_status(&g.id));
            }
            None => {
                w.visit_title.set_label("");
                w.keep_spin.set_sensitive(false);
                w.keep_status.set_label("");
            }
        }
        for (f, b) in &w.filter_buttons {
            b.set_active(*f == st.filter);
        }
        for (path, (cell, _, badge)) in &w.cells {
            let Some(it) = st.item(path) else { continue };
            if st.selection.contains(path) {
                cell.add_css_class("picked");
            } else {
                cell.remove_css_class("picked");
            }
            badge.remove_css_class("keep");
            badge.remove_css_class("reject");
            cell.remove_css_class("rejected");
            match it.decision {
                Decision::Keep => {
                    badge.set_label("Keep");
                    badge.add_css_class("keep");
                    badge.set_visible(true);
                }
                Decision::Reject => {
                    badge.set_label("Reject");
                    badge.add_css_class("reject");
                    cell.add_css_class("rejected");
                    badge.set_visible(true);
                }
                Decision::Undecided => badge.set_visible(false),
            }
        }
        let n = st.selection.len();
        w.selection_label.set_label(&match n {
            0 => "Click to select · Shift/Ctrl to extend".to_string(),
            1 => "1 selected".to_string(),
            n => format!("{n} selected"),
        });
        let plan = st.commit_plan();
        w.commit_button.set_sensitive(st.can_commit());
        w.commit_button.set_label(&if plan.is_empty() {
            "Review & Commit…".to_string()
        } else {
            format!("Review & Commit {}…", plan.items)
        });
        w.syncing.set(false);
    }
}

fn about_dialog() -> adw::AboutDialog {
    adw::AboutDialog::builder()
        .application_name("Unswiper")
        .application_icon(APP_ID)
        .version(VERSION)
        .developer_name("Rob Ranslam")
        .copyright("© 2026 Rob Ranslam")
        .license_type(gtk::License::MitX11)
        .website(WEBSITE)
        .issue_url(ISSUES)
        .comments("Sort trip photos into visits, keep the best, set aside the rest.")
        .build()
}

fn help_window(parent: &adw::ApplicationWindow) -> adw::Window {
    let topics = help::topics();
    let list = gtk::ListBox::builder()
        .css_classes(["navigation-sidebar"])
        .build();
    for t in &topics {
        list.append(
            &gtk::Label::builder()
                .label(&t.title)
                .xalign(0.0)
                .margin_top(6)
                .margin_bottom(6)
                .margin_start(6)
                .build(),
        );
    }
    let body = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .yalign(0.0)
        .selectable(true)
        .use_markup(true)
        .margin_start(24)
        .margin_end(24)
        .margin_top(18)
        .margin_bottom(24)
        .build();
    let md: Vec<&'static str> = topics.iter().map(|t| t.markdown).collect();
    {
        let b = body.clone();
        list.connect_row_selected(move |_, row| {
            if let Some(r) = row {
                b.set_markup(&help::to_pango(md[r.index() as usize]));
            }
        });
    }
    list.select_row(list.row_at_index(0).as_ref());
    let paned = gtk::Paned::builder()
        .start_child(
            &gtk::ScrolledWindow::builder()
                .child(&list)
                .width_request(230)
                .build(),
        )
        .end_child(
            &gtk::ScrolledWindow::builder()
                .child(&body)
                .hexpand(true)
                .build(),
        )
        .position(240)
        .build();
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());
    tv.set_content(Some(&paned));
    adw::Window::builder()
        .title("Unswiper Help")
        .default_width(860)
        .default_height(620)
        .transient_for(parent)
        .content(&tv)
        .build()
}
