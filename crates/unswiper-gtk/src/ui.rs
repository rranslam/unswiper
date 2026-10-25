// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! The relm4 layer: builds the GNOME window and turns signals into [`AppState`] calls.
//! Everything worth testing lives in [`crate::state`]; this module only wires widgets.

use crate::blur::{BlurEvent, BlurJob};
use crate::state::{AppState, Filter};
use crate::{help, thumbs, APP_ID, ISSUES, VERSION, WEBSITE};
use relm4::adw::prelude::*;
use relm4::gtk::{gdk, gio, glib};
use relm4::{adw, gtk, Component, ComponentParts, ComponentSender};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use unswiper_blur::decide::Thresholds;
use unswiper_core::index::index;
use unswiper_core::meta::info::{MediaInfo, Row};
use unswiper_core::quality::Sensitivity;
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
    /// Show a dialog a few seconds after start (screenshots): "about", or "commit" (rejects
    /// everything in the open visit, then opens Review & Commit; nothing is moved unless confirmed).
    pub present_dialog: Option<String>,
    /// Open the viewer on the first video after the first visit opens (smoke test) and print
    /// `viewer: prepared duration_ms=N` or `viewer: error …` to stdout.
    pub present_viewer: bool,
    /// Select the first item of the first visit, open the photo info inspector and the
    /// right-click popover, and print the rows (`info: <group> | <label> | <value>`) to stdout.
    pub present_info: bool,
    /// Run the Google Photos import assistant on its own with these archives and library folders,
    /// then index the library; prints `takeout: …` lines to stdout (smoke test).
    pub present_takeout: Option<(PathBuf, PathBuf)>,
}

pub fn run(opts: Options) {
    let app = relm4::RelmApp::new(APP_ID).with_args(vec![]);
    // Timed runs (--quit-after: tests, screenshots) are separate instances, so parallel tests and
    // an already-open window don't swallow them as a second launch of the same app.
    if opts.quit_after.is_some() {
        relm4::main_application().set_flags(gtk::gio::ApplicationFlags::NON_UNIQUE);
    }
    relm4::set_global_css(CSS);
    // Fallback icon path so the window and About dialog have the icon when uninstalled.
    if let (Some(display), Ok(dir)) = (
        gdk::Display::default(),
        crate::icons::install(&crate::icons::default_dir()),
    ) {
        gtk::IconTheme::for_display(&display).add_search_path(dir);
    }
    gtk::Window::set_default_icon_name(APP_ID);
    app.run::<App>(opts);
}

const CSS: &str = "
.thumb { border-radius: 8px; padding: 3px; border: 3px solid transparent; }
.thumb.picked { border-color: @accent_bg_color; }
.badge { border-radius: 99px; padding: 2px 8px; margin: 6px; font-weight: bold; }
.badge.keep { background: @success_bg_color; color: @success_fg_color; }
.badge.reject { background: @error_bg_color; color: @error_fg_color; }
.badge.blurry { background: #b5560b; color: white; }
.thumb.rejected picture { opacity: 0.45; }
.thumb.rejected .duration { opacity: 0.6; }
.duration { background: alpha(black, 0.65); color: white; border-radius: 6px; padding: 1px 6px;
  margin: 6px; font-size: 0.85em; font-weight: bold; }
.viewer-media { background: black; }
/* Dialogs (AdwAlertDialog, AdwAboutDialog) float over a busy photo grid: give them a visible edge
   and a stronger shadow, and dim the window behind them. currentColor-based, so it works in light
   and dark mode. */
floating-sheet > sheet, bottom-sheet > sheet {
  box-shadow: 0 0 0 1px alpha(currentColor, 0.30), 0 0 0 2px alpha(black, 0.12),
              0 10px 36px 6px alpha(black, 0.45), 0 3px 8px alpha(black, 0.30);
}
floating-sheet > dimming, bottom-sheet > dimming { background-color: alpha(black, 0.38); }
/* Toasts float over the bottom bar: always an opaque dark pill with light text, so they stay
   readable even if the libadwaita stylesheet is overridden (e.g. by a forced GTK_THEME). */
toast {
  background-color: #303030; color: #f6f6f6; border-radius: 99px;
  border: 1px solid alpha(white, 0.10);
  box-shadow: 0 2px 6px alpha(black, 0.35), 0 6px 18px alpha(black, 0.30);
  padding: 4px 6px 4px 14px; margin: 12px; font-weight: normal;
}
toast label { color: #f6f6f6; }
/* Visits sidebar: roomy rows with a hairline between them, title and subtitle clearly apart
   (does not rely on libadwaita row styling, which a forced GTK_THEME drops). */
.visits { padding: 6px; }
.visits > row { padding: 8px 6px; margin: 0 0 3px 0; border-radius: 8px;
  border-bottom: 1px solid alpha(currentColor, 0.08); }
.visits > row:selected { border-bottom-color: transparent; }
.visits > row .title { font-weight: 600; }
.visits > row .subtitle { margin-top: 3px; font-size: 0.85em; opacity: 0.65; }
.visits > row .title, .visits > row .subtitle { padding: 0; }
toast button { color: #f6f6f6; }
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
    /// Open the "Import from Google Photos" assistant.
    ImportGooglePhotos,
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
    BlurProgress(usize, usize),
    BlurResult(HashSet<String>, HashSet<String>, bool),
    BlurSetEnabled(bool),
    BlurSetSensitivity(Sensitivity),
    BlurRescan,
    BlurClearVisit,
    BlurConfirmVisit,
    OpenViewer(String),
    ToggleViewer,
    CloseViewer,
    ViewerStep(i32),
    ViewerImage(String, ThumbMsg),
    ViewerMuted(bool),
    ViewerOpenExternal,
    SetLiveMotion(bool),
    Duration(u64, String, u64),
    ToggleInfo,
    SetShowLocation(bool),
    InfoRead(String, Box<MediaInfo>),
    InfoPopover(String),
    OpenMaps(String),
}

pub struct App {
    state: AppState,
    opts: Options,
    watcher: Option<FolderWatcher>,
    thumb_gen: u64,
    save_scheduled: bool,
    indexing: bool,
    blur_cancel: Option<Arc<AtomicBool>>,
    /// Viewer already reported to stdout (--present-viewer).
    viewer_reported: Rc<Cell<bool>>,
    viewer_presented: bool,
    /// Photo info read so far (memory only, plan §17.1 privacy).
    info_cache: crate::info::InfoCache,
    info_pending: HashSet<String>,
    /// Blur scores for the info "Sharpness" row, reloaded after each blur scan.
    quality: unswiper_blur::cache::QualityCache,
    /// --present-info: 0 = not yet, 1 = waiting for the rows, 2 = reported.
    info_report: u8,
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
    durations: HashMap<String, gtk::Label>,
    content: gtk::Stack,
    viewer: ViewerWidgets,
    selection_label: gtk::Label,
    commit_button: gtk::Button,
    filter_buttons: Vec<(Filter, gtk::ToggleButton)>,
    restore_action: gio::SimpleAction,
    mode_action: gio::SimpleAction,
    blur_action: gio::SimpleAction,
    blur_sens_action: gio::SimpleAction,
    blur_status: gtk::Label,
    live_action: gio::SimpleAction,
    syncing: Rc<Cell<bool>>,
    info: InfoWidgets,
}

/// Photo info inspector (right-hand sidebar) and the Compare strip (plan §17.2).
pub struct InfoWidgets {
    split: adw::OverlaySplitView,
    title: gtk::Label,
    body: gtk::Box,
    strip: gtk::Label,
    action: gio::SimpleAction,
    location_action: gio::SimpleAction,
    /// What the panel shows now: (path, rows), so unchanged state doesn't rebuild it.
    shown: Option<(String, Vec<Row>, String)>,
}

/// The full-size viewer page (plan §16.1).
pub struct ViewerWidgets {
    title: gtk::Label,
    position: gtk::Label,
    badge: gtk::Label,
    media: gtk::Stack,
    picture: gtk::Picture,
    video: gtk::Video,
    error: adw::StatusPage,
    prev: gtk::Button,
    next: gtk::Button,
    /// Path the current picture/stream belongs to.
    shown: Option<String>,
}

/// Viewer keys (Space, arrows) run in the capture phase so a focused button doesn't eat
/// Space; skip them while a dialog or a text/spin field has focus.
fn viewer_key(
    ctl: &gtk::ShortcutController,
    trigger: &str,
    sender: &ComponentSender<App>,
    msg: fn() -> Msg,
) {
    let s = sender.clone();
    let act = gtk::CallbackAction::new(move |w, _| {
        let focus = w.root().and_then(|r| r.focus());
        let busy = focus.is_some_and(|f| {
            f.ancestor(adw::Dialog::static_type()).is_some()
                || f.is::<gtk::Text>()
                || f.ancestor(gtk::SpinButton::static_type()).is_some()
        });
        if busy {
            return glib::Propagation::Proceed;
        }
        s.input(msg());
        glib::Propagation::Stop
    });
    ctl.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string(trigger),
        Some(act),
    ));
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
            .icon_name(APP_ID)
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
        lib.append(
            Some("Import from Google Photos…"),
            Some("win.import-google-photos"),
        );
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
        let blur = gio::Menu::new();
        blur.append(Some("Detect Blurry Photos"), Some("win.blur"));
        let sens = gio::Menu::new();
        for s in Sensitivity::ALL {
            sens.append(
                Some(s.label()),
                Some(&format!("win.blur-sensitivity::{}", s.as_str())),
            );
        }
        blur.append_submenu(Some("Blur Sensitivity"), &sens);
        blur.append(Some("Check for Blurry Photos Now"), Some("win.blur-rescan"));
        blur.append(
            Some("Not Blurry: Clear Suggestions in This Visit"),
            Some("win.blur-clear"),
        );
        blur.append(
            Some("Reject All Blurry Suggestions in This Visit"),
            Some("win.blur-confirm"),
        );
        menu.append_section(Some("Blurry Photos"), &blur);
        let view = gio::Menu::new();
        view.append(Some("Open Viewer (Space)"), Some("win.viewer"));
        view.append(Some("Play Live Photo Motion"), Some("win.live-motion"));
        menu.append_section(Some("Viewer"), &view);
        let info_menu = gio::Menu::new();
        info_menu.append(Some("Show Photo Info (I)"), Some("win.info"));
        info_menu.append(
            Some("Show Location in Photo Info"),
            Some("win.info-location"),
        );
        menu.append_section(Some("Photo Info"), &info_menu);
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
        action(&root, "import-google-photos", &sender, || {
            Msg::ImportGooglePhotos
        });
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
        let blur_action =
            gio::SimpleAction::new_stateful("blur", None, &state.blur_enabled().to_variant());
        {
            let s = sender.clone();
            blur_action.connect_activate(move |a, _| {
                let on = !a.state().and_then(|v| v.get::<bool>()).unwrap_or(true);
                a.set_state(&on.to_variant());
                s.input(Msg::BlurSetEnabled(on));
            });
        }
        root.add_action(&blur_action);
        let blur_sens_action = gio::SimpleAction::new_stateful(
            "blur-sensitivity",
            Some(glib::VariantTy::STRING),
            &state.blur_sensitivity().as_str().to_variant(),
        );
        {
            let s = sender.clone();
            blur_sens_action.connect_activate(move |a, v| {
                let Some(sens) = v
                    .and_then(|v| v.get::<String>())
                    .and_then(|n| Sensitivity::parse(&n))
                else {
                    return;
                };
                a.set_state(&sens.as_str().to_variant());
                s.input(Msg::BlurSetSensitivity(sens));
            });
        }
        root.add_action(&blur_sens_action);
        action(&root, "blur-rescan", &sender, || Msg::BlurRescan);
        action(&root, "blur-clear", &sender, || Msg::BlurClearVisit);
        action(&root, "blur-confirm", &sender, || Msg::BlurConfirmVisit);
        action(&root, "viewer", &sender, || Msg::ToggleViewer);
        let live_action =
            gio::SimpleAction::new_stateful("live-motion", None, &state.live_motion().to_variant());
        {
            let s = sender.clone();
            live_action.connect_activate(move |a, _| {
                let on = !a.state().and_then(|v| v.get::<bool>()).unwrap_or(false);
                a.set_state(&on.to_variant());
                s.input(Msg::SetLiveMotion(on));
            });
        }
        root.add_action(&live_action);
        let info_action = gio::SimpleAction::new_stateful("info", None, &false.to_variant());
        {
            let s = sender.clone();
            info_action.connect_activate(move |_, _| s.input(Msg::ToggleInfo));
        }
        root.add_action(&info_action);
        let info_location_action = gio::SimpleAction::new_stateful(
            "info-location",
            None,
            &state.show_location().to_variant(),
        );
        {
            let s = sender.clone();
            info_location_action.connect_activate(move |a, _| {
                let on = !a.state().and_then(|v| v.get::<bool>()).unwrap_or(true);
                a.set_state(&on.to_variant());
                s.input(Msg::SetShowLocation(on));
            });
        }
        root.add_action(&info_location_action);
        let blur_status = gtk::Label::builder()
            .css_classes(["dim-label", "caption"])
            .visible(false)
            .build();
        header.pack_start(&blur_status);

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
        shortcut(&ctl, "Escape", &sender, || Msg::CloseViewer);
        shortcut(&ctl, "i", &sender, || Msg::ToggleInfo);
        shortcut(&ctl, "<Control>i", &sender, || Msg::ToggleInfo);
        root.add_controller(ctl);
        let vctl = gtk::ShortcutController::new();
        vctl.set_scope(gtk::ShortcutScope::Managed);
        vctl.set_propagation_phase(gtk::PropagationPhase::Capture);
        viewer_key(&vctl, "space", &sender, || Msg::ToggleViewer);
        viewer_key(&vctl, "Left", &sender, || Msg::ViewerStep(-1));
        viewer_key(&vctl, "Right", &sender, || Msg::ViewerStep(1));
        root.add_controller(vctl);

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
        let import_gp = gtk::Button::builder()
            .label("Import from Google Photos…")
            .halign(gtk::Align::Center)
            .css_classes(["pill"])
            .build();
        {
            let s = sender.clone();
            import_gp.connect_clicked(move |_| s.input(Msg::ImportGooglePhotos));
        }
        let setup_buttons = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .halign(gtk::Align::Center)
            .build();
        setup_buttons.append(&choose);
        setup_buttons.append(&import_gp);
        let setup = adw::StatusPage::builder()
            .icon_name("folder-pictures-symbolic")
            .title("Choose Your Photo Library")
            .description("Pick the folder with your photos and videos. Unswiper groups them into visits so you can keep the best and set aside the rest. Nothing moves until you commit.\n\nYour photos are in Google Photos? Import from Google Photos guides you step by step.")
            .child(&setup_buttons)
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
            .css_classes(["navigation-sidebar", "visits"])
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
            (Filter::Blurry, "Blurry"),
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
        // Viewer page: back, title, position, decision badge, open externally; media in the middle
        // with previous/next on the sides. Video uses GTK's built-in controls (play, scrub, volume).
        let v_back = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .label("Grid")
            .tooltip_text("Back to the grid (Esc or Space)")
            .build();
        {
            let s = sender.clone();
            v_back.connect_clicked(move |_| s.input(Msg::CloseViewer));
        }
        let v_title = gtk::Label::builder()
            .css_classes(["heading"])
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .hexpand(true)
            .xalign(0.0)
            .build();
        let v_pos = gtk::Label::builder().css_classes(["dim-label"]).build();
        let v_badge = gtk::Label::builder()
            .css_classes(["badge"])
            .visible(false)
            .build();
        let v_ext = gtk::Button::builder()
            .label("Open in Default App")
            .tooltip_text("Open this file in your usual photo or video app")
            .build();
        {
            let s = sender.clone();
            v_ext.connect_clicked(move |_| s.input(Msg::ViewerOpenExternal));
        }
        let v_top = gtk::Box::builder()
            .spacing(12)
            .margin_start(12)
            .margin_end(12)
            .margin_top(8)
            .margin_bottom(8)
            .build();
        v_top.append(&v_back);
        v_top.append(&v_title);
        v_top.append(&v_badge);
        v_top.append(&v_pos);
        v_top.append(&v_ext);
        let v_picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Contain)
            .hexpand(true)
            .vexpand(true)
            .build();
        let v_video = gtk::Video::builder()
            .autoplay(true)
            .hexpand(true)
            .vexpand(true)
            .build();
        let v_error = adw::StatusPage::builder()
            .icon_name("video-x-generic-symbolic")
            .title("This Video Can't Be Played Here")
            .description("Unswiper doesn't have the codec for this file. Click Open in Default App to watch it in another app. Your decisions still work as usual.")
            .build();
        let v_media = gtk::Stack::builder()
            .css_classes(["viewer-media"])
            .hexpand(true)
            .vexpand(true)
            .build();
        v_media.add_named(&v_picture, Some("picture"));
        v_media.add_named(&v_video, Some("video"));
        v_media.add_named(&v_error, Some("error"));
        let nav_button = |icon: &str, tip: &str, step: i32| {
            let b = gtk::Button::builder()
                .icon_name(icon)
                .tooltip_text(tip)
                .valign(gtk::Align::Center)
                .css_classes(["circular", "osd"])
                .margin_start(8)
                .margin_end(8)
                .build();
            let s = sender.clone();
            b.connect_clicked(move |_| s.input(Msg::ViewerStep(step)));
            b
        };
        let v_prev = nav_button("go-previous-symbolic", "Previous (Left arrow)", -1);
        let v_next = nav_button("go-next-symbolic", "Next (Right arrow)", 1);
        let v_mid = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        v_mid.append(&v_prev);
        v_mid.append(&v_media);
        v_mid.append(&v_next);
        let v_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        v_page.append(&v_top);
        v_page.append(&v_mid);

        let content = gtk::Stack::new();
        content.add_named(&empty, Some("empty"));
        content.add_named(&grid_scroll, Some("grid"));
        content.add_named(&v_page, Some("viewer"));

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

        // Photo info inspector: a sidebar on the right of the grid/viewer, closed at start.
        let info_title = gtk::Label::builder()
            .css_classes(["heading"])
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .build();
        let info_body = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .build();
        let info_close = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text("Hide Photo Info (I)")
            .css_classes(["flat", "circular"])
            .valign(gtk::Align::Start)
            .build();
        {
            let s = sender.clone();
            info_close.connect_clicked(move |_| s.input(Msg::ToggleInfo));
        }
        let info_head = gtk::Box::builder().spacing(6).build();
        info_title.set_hexpand(true);
        info_head.append(&info_title);
        info_head.append(&info_close);
        let info_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_start(12)
            .margin_end(12)
            .margin_top(12)
            .margin_bottom(12)
            .build();
        info_box.append(&info_head);
        info_box.append(&info_body);
        let info_scroll = gtk::ScrolledWindow::builder()
            .child(&info_box)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let split = adw::OverlaySplitView::builder()
            .content(&content)
            .sidebar(&info_scroll)
            .sidebar_position(gtk::PackType::End)
            .show_sidebar(false)
            .min_sidebar_width(260.0)
            .max_sidebar_width(360.0)
            .vexpand(true)
            .build();
        // Narrow windows: the inspector floats over the grid instead of squeezing it.
        if let Ok(cond) = adw::BreakpointCondition::parse("max-width: 1100sp") {
            let bp = adw::Breakpoint::new(cond);
            bp.add_setter(&split, "collapsed", Some(&true.to_value()));
            root.add_breakpoint(bp);
        }
        // Compare strip: one line of settings for the selected item only.
        let strip = gtk::Label::builder()
            .css_classes(["caption", "dim-label"])
            .xalign(0.0)
            .margin_start(18)
            .margin_end(18)
            .margin_top(4)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .selectable(true)
            .visible(false)
            .build();

        let right = gtk::Box::new(gtk::Orientation::Vertical, 0);
        right.append(&top);
        right.append(&split);
        right.append(&strip);
        right.append(&bar);
        let paned = gtk::Paned::builder()
            .start_child(&side)
            .end_child(&right)
            .position(320)
            .shrink_start_child(false)
            .build();

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
            blur_cancel: None,
            viewer_reported: Rc::new(Cell::new(false)),
            viewer_presented: false,
            info_cache: Default::default(),
            info_pending: HashSet::new(),
            quality: Default::default(),
            info_report: 0,
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
            durations: HashMap::new(),
            content,
            viewer: ViewerWidgets {
                title: v_title,
                position: v_pos,
                badge: v_badge,
                media: v_media,
                picture: v_picture,
                video: v_video,
                error: v_error,
                prev: v_prev,
                next: v_next,
                shown: None,
            },
            selection_label,
            commit_button,
            filter_buttons,
            restore_action,
            mode_action,
            blur_action,
            blur_sens_action,
            blur_status,
            live_action,
            syncing,
            info: InfoWidgets {
                split,
                title: info_title,
                body: info_body,
                strip,
                action: info_action,
                location_action: info_location_action,
                shown: None,
            },
        };
        model.reload_quality();

        if let Some(p) = model.state.load_problem.take() {
            widgets.toasts.add_toast(adw::Toast::new(&p));
        }
        if let Some(dir) = opts.open.clone() {
            sender.input(Msg::FolderChosen(dir));
        } else if model.state.has_library() {
            model.start_watcher(&sender);
            model.start_blur(&sender);
        }
        model.refresh(&mut widgets);
        model.render_info(&mut widgets, &sender, &root);
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
        if let Some((archives, library)) = opts.present_takeout.clone() {
            let s = sender.clone();
            let w = root.clone();
            let support = model.state.store.support_dir().to_path_buf();
            glib::idle_add_local_once(move || {
                let s2 = s.clone();
                crate::takeout_ui::open(
                    &w,
                    support,
                    move |lib| s2.input(Msg::FolderChosen(lib)),
                    Some(crate::takeout_ui::Auto { archives, library }),
                )
                .present();
            });
        }
        if let Some(which) = opts.present_dialog.clone() {
            let s = sender.clone();
            glib::timeout_add_seconds_local_once(3, move || match which.as_str() {
                "commit" => {
                    s.input(Msg::SelectAll);
                    s.input(Msg::Decide(Decision::Reject));
                    s.input(Msg::ReviewCommit);
                }
                "takeout" => s.input(Msg::ImportGooglePhotos),
                _ => s.input(Msg::About),
            });
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
            Msg::ImportGooglePhotos => {
                changed = false;
                let s = sender.clone();
                crate::takeout_ui::open(
                    root,
                    self.state.store.support_dir().to_path_buf(),
                    move |lib| s.input(Msg::FolderChosen(lib)),
                    None,
                )
                .present();
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
                        if self.opts.present_takeout.is_some() {
                            println!("takeout: indexed items={n} visits={}", groups.len());
                        }
                        self.state.apply_index(items, groups, chrono::Utc::now());
                        self.start_watcher(&sender);
                        self.start_blur(&sender);
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
                    if self.opts.present_viewer && !self.viewer_presented {
                        self.viewer_presented = true;
                        let first_video = self
                            .state
                            .visible_items()
                            .iter()
                            .find(|i| i.kind == unswiper_core::MediaKind::Video)
                            .map(|i| i.path.clone());
                        match first_video {
                            Some(p) => sender.input(Msg::OpenViewer(p)),
                            None => println!("viewer: error no video in the first visit"),
                        }
                    }
                    if self.opts.present_info && self.info_report == 0 {
                        self.info_report = 1;
                        if let Some(p) = self.state.visible_items().first().map(|i| i.path.clone())
                        {
                            self.state.click(&p, false, false);
                            // Like a user pressing I once the window is shown.
                            let s = sender.clone();
                            glib::timeout_add_local_once(
                                std::time::Duration::from_millis(800),
                                move || s.input(Msg::ToggleInfo),
                            );
                        } else {
                            println!("info: error empty visit");
                        }
                    }
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
                    let dates = crate::takeout::dates_for(&self.state.root().unwrap_or_default());
                    let d = self.state.apply_watch(&b, dates.as_ref());
                    changed = !d.is_empty();
                    if changed {
                        self.rebuild_grid(w, &sender);
                        if !d.added.is_empty() {
                            self.start_blur(&sender);
                        }
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
            Msg::BlurProgress(done, total) => {
                changed = false;
                w.blur_status
                    .set_label(&format!("Checking sharpness {done}/{total}"));
                w.blur_status.set_visible(done < total);
            }
            Msg::BlurResult(flagged, scored, last) => {
                if last {
                    w.blur_status.set_visible(false);
                    self.blur_cancel = None;
                    self.reload_quality();
                    w.info.shown = None;
                }
                let r = self.state.apply_blur(&flagged, &scored);
                changed = r.suggested + r.withdrawn > 0;
                if changed && self.state.filter == Filter::Blurry {
                    self.rebuild_grid(w, &sender);
                }
                if last && r.suggested > 0 {
                    w.toasts.add_toast(adw::Toast::new(&match r.suggested {
                        1 => "1 photo looks blurry and was marked Reject. Nothing moves until you commit.".to_string(),
                        n => format!("{n} photos look blurry and were marked Reject. Nothing moves until you commit."),
                    }));
                }
            }
            Msg::BlurSetEnabled(on) => {
                let n = self.state.set_blur_enabled(on);
                if on {
                    self.start_blur(&sender);
                } else {
                    self.stop_blur(w);
                    if n > 0 {
                        w.toasts.add_toast(adw::Toast::new(&format!(
                            "Blur detection is off. {n} blurry suggestion{} cleared.",
                            if n == 1 { "" } else { "s" }
                        )));
                    }
                }
                self.rebuild_grid(w, &sender);
            }
            Msg::BlurSetSensitivity(sens) => {
                self.state.set_blur_sensitivity(sens);
                self.start_blur(&sender);
            }
            Msg::BlurRescan => {
                changed = false;
                if !self.state.blur_enabled() {
                    w.toasts.add_toast(adw::Toast::new(
                        "Turn on Detect Blurry Photos in the menu first.",
                    ));
                } else {
                    self.start_blur(&sender);
                }
            }
            Msg::BlurClearVisit => {
                let n = self.state.dismiss_suggested_in_visit();
                w.toasts.add_toast(adw::Toast::new(&format!(
                    "Cleared {n} blurry suggestion{} in this visit.",
                    if n == 1 { "" } else { "s" }
                )));
                self.rebuild_grid(w, &sender);
            }
            Msg::BlurConfirmVisit => {
                let n = self.state.confirm_suggested_in_visit();
                w.toasts.add_toast(adw::Toast::new(&format!(
                    "{n} blurry photo{} now rejected by you. They move only when you commit.",
                    if n == 1 { " is" } else { "s are" }
                )));
            }
            Msg::OpenViewer(p) => {
                if self.state.open_viewer(&p) {
                    self.show_viewer(w, &sender);
                }
            }
            Msg::ToggleViewer => {
                if self.state.viewer.is_some() {
                    self.state.close_viewer();
                } else if self.state.open_viewer_on_selection() {
                    self.show_viewer(w, &sender);
                }
            }
            Msg::CloseViewer => {
                changed = self.state.viewer.is_some();
                self.state.close_viewer();
            }
            Msg::ViewerStep(d) => {
                changed = self.state.viewer_step(d);
                if changed {
                    self.show_viewer(w, &sender);
                }
            }
            Msg::ViewerImage(path, t) => {
                changed = false;
                if w.viewer.shown.as_deref() == Some(path.as_str()) {
                    let t = t.0;
                    let bytes = glib::Bytes::from_owned(t.rgba);
                    let tex = gdk::MemoryTexture::new(
                        t.width as i32,
                        t.height as i32,
                        gdk::MemoryFormat::R8g8b8a8,
                        &bytes,
                        t.width as usize * 4,
                    );
                    w.viewer.picture.set_paintable(Some(&tex));
                }
            }
            Msg::ViewerMuted(m) => {
                changed = false;
                self.state.muted = m;
            }
            Msg::ViewerOpenExternal => {
                changed = false;
                if let Some(it) = self.state.viewer_item() {
                    gtk::FileLauncher::new(Some(&gio::File::for_path(&it.path))).launch(
                        Some(root),
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                }
            }
            Msg::SetLiveMotion(on) => {
                self.state.set_live_motion(on);
                if self.state.viewer.is_some() {
                    self.show_viewer(w, &sender);
                }
            }
            Msg::Duration(gen, path, ms) => {
                changed = false;
                if gen == self.thumb_gen {
                    if let Some(l) = w.durations.get(&path) {
                        l.set_label(&format!(
                            "▶ {}",
                            crate::player::format_duration(std::time::Duration::from_millis(ms))
                        ));
                    }
                }
            }
            Msg::ToggleInfo => {
                self.state.info_open = !self.state.info_open;
            }
            Msg::SetShowLocation(on) => {
                self.state.set_show_location(on);
            }
            Msg::InfoRead(path, info) => {
                changed = false;
                self.info_pending.remove(&path);
                self.info_cache.insert(path, *info);
            }
            Msg::InfoPopover(path) => {
                changed = false;
                self.show_info_popover(w, &sender, &path);
            }
            Msg::OpenMaps(url) => {
                changed = false;
                // Only ever on the user's click: this is the one action that sends a location out.
                gtk::UriLauncher::new(&url).launch(Some(root), None::<&gio::Cancellable>, |_| {});
            }
            Msg::Quit => {
                changed = false;
                if let Some(c) = &self.blur_cancel {
                    c.store(true, Ordering::Relaxed);
                }
                if self.state.needs_save() {
                    let _ = self.state.save();
                }
                self.watcher = None;
                relm4::main_application().quit();
            }
        }
        if changed {
            self.refresh(w);
            w.info.action.set_state(&self.state.info_open.to_variant());
            w.info
                .location_action
                .set_state(&self.state.show_location().to_variant());
            if self.state.needs_save() && !self.save_scheduled {
                self.save_scheduled = true;
                let s = sender.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(800), move || {
                    s.input(Msg::Save)
                });
            }
        }
        self.render_info(w, &sender, root);
    }
}

impl App {
    fn reload_quality(&mut self) {
        self.quality = unswiper_blur::cache::QualityCache::load(
            &unswiper_blur::cache::QualityCache::path_in(self.state.store.support_dir()),
        );
    }

    /// Rows for `item` if its info has been read; otherwise starts reading it in the background.
    fn info_rows(&mut self, item: &MediaItem, sender: &ComponentSender<Self>) -> Option<Vec<Row>> {
        if let Some(media) = self.info_cache.get(&item.path) {
            let sharp = crate::info::sharpness(
                item,
                &self.quality,
                self.state.blur_sensitivity(),
                self.state.blur_enabled(),
            );
            return Some(crate::info::rows(
                item,
                media,
                self.state.show_location(),
                sharp,
            ));
        }
        if self.info_pending.insert(item.path.clone()) {
            let tx = sender.input_sender().clone();
            let path = item.path.clone();
            std::thread::spawn(move || {
                let info = crate::info::read(&path);
                tx.emit(Msg::InfoRead(path, Box::new(info)));
            });
        }
        None
    }

    /// Syncs the inspector panel and the Compare strip with the selection / viewer item.
    fn render_info(
        &mut self,
        w: &mut Widgets,
        sender: &ComponentSender<Self>,
        _root: &adw::ApplicationWindow,
    ) {
        let open = self.state.info_open && self.state.visit.is_some();
        w.info.split.set_show_sidebar(open);
        w.info.action.set_state(&self.state.info_open.to_variant());
        let target = self
            .state
            .info_target()
            .map(|(it, others)| (it.clone(), others));
        let Some((item, others)) = target else {
            w.info.strip.set_visible(false);
            if w.info.shown.take().is_some() || w.info.title.label().is_empty() {
                w.info.title.set_label("Photo Info");
                clear_box(&w.info.body);
                w.info.body.append(
                    &gtk::Label::builder()
                        .label("Select a photo or video to see its camera, settings, date, size and location.")
                        .wrap(true)
                        .xalign(0.0)
                        .css_classes(["dim-label"])
                        .build(),
                );
            }
            return;
        };
        let heading = crate::info::heading(&item, others);
        let rows = self.info_rows(&item, sender);
        let strip = match (&rows, self.info_cache.get(&item.path)) {
            (Some(_), Some(media)) => {
                let sharp = crate::info::sharpness(
                    &item,
                    &self.quality,
                    self.state.blur_sensitivity(),
                    self.state.blur_enabled(),
                );
                unswiper_core::meta::info::strip(media, sharp)
            }
            _ => String::new(),
        };
        w.info.strip.set_label(&strip);
        w.info
            .strip
            .set_visible(!strip.is_empty() && self.state.viewer.is_none());
        let Some(rows) = rows else {
            w.info.title.set_label(&heading);
            clear_box(&w.info.body);
            w.info.body.append(
                &gtk::Label::builder()
                    .label("Reading…")
                    .xalign(0.0)
                    .css_classes(["dim-label"])
                    .build(),
            );
            w.info.shown = None;
            return;
        };
        if self.info_report == 1 && open {
            self.info_report = 2;
            print!("{}", crate::info::report(&rows));
            println!("info: panel open");
            let path = item.path.clone();
            let s = sender.clone();
            glib::idle_add_local_once(move || s.input(Msg::InfoPopover(path)));
        }
        let key = (item.path.clone(), rows.clone(), heading.clone());
        if w.info.shown.as_ref() == Some(&key) {
            return;
        }
        w.info.title.set_label(&heading);
        clear_box(&w.info.body);
        for (title, group) in crate::info::grouped(&rows, false) {
            let g = adw::PreferencesGroup::builder().title(title).build();
            for r in group {
                let row = adw::ActionRow::builder()
                    .title(glib::markup_escape_text(&r.label))
                    .subtitle(glib::markup_escape_text(&r.value))
                    .subtitle_selectable(true)
                    .css_classes(["property"])
                    .build();
                if r.label == "Coordinates" {
                    if let Some(g) = self.info_cache.get(&item.path).and_then(|m| m.gps) {
                        let b = gtk::Button::builder()
                            .icon_name("mark-location-symbolic")
                            .tooltip_text("Open in Maps (opens OpenStreetMap in your browser)")
                            .valign(gtk::Align::Center)
                            .css_classes(["flat"])
                            .build();
                        let s = sender.clone();
                        let url = unswiper_core::meta::info::maps_url(&g);
                        b.connect_clicked(move |_| s.input(Msg::OpenMaps(url.clone())));
                        row.add_suffix(&b);
                    }
                }
                g.add(&row);
            }
            if title == "Location" {
                g.set_description(Some(
                    "Shown only on this computer. Turn off in the menu: Show Location in Photo Info.",
                ));
            }
            w.info.body.append(&g);
        }
        w.info.shown = Some(key);
    }

    /// Right-click → Show Info: a compact popover next to the tile.
    fn show_info_popover(&mut self, w: &mut Widgets, sender: &ComponentSender<Self>, path: &str) {
        let Some(item) = self.state.item(path).cloned() else {
            return;
        };
        let Some((cell, _, _)) = w.cells.get(path) else {
            return;
        };
        if self.info_cache.get(path).is_none() {
            // A single EXIF/moov read is fast; read it right away for the popover.
            self.info_cache
                .insert(path.to_string(), crate::info::read(path));
        }
        let Some(rows) = self.info_rows(&item, sender) else {
            return;
        };
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_start(6)
            .margin_end(6)
            .margin_top(6)
            .margin_bottom(6)
            .build();
        content.append(
            &gtk::Label::builder()
                .label(item.file_name())
                .css_classes(["heading"])
                .xalign(0.0)
                .build(),
        );
        let grid = gtk::Grid::builder()
            .row_spacing(4)
            .column_spacing(12)
            .build();
        let short: Vec<&Row> = crate::info::grouped(&rows, true)
            .into_iter()
            .flat_map(|(_, v)| v)
            .collect();
        for (i, r) in short.iter().enumerate() {
            grid.attach(
                &gtk::Label::builder()
                    .label(&r.label)
                    .xalign(1.0)
                    .css_classes(["dim-label"])
                    .build(),
                0,
                i as i32,
                1,
                1,
            );
            grid.attach(
                &gtk::Label::builder()
                    .label(&r.value)
                    .xalign(0.0)
                    .max_width_chars(40)
                    .wrap(true)
                    .build(),
                1,
                i as i32,
                1,
                1,
            );
        }
        content.append(&grid);
        let more = gtk::Button::builder()
            .label("Show All Info (I)")
            .css_classes(["flat"])
            .halign(gtk::Align::End)
            .build();
        let pop = gtk::Popover::builder()
            .child(&content)
            .position(gtk::PositionType::Right)
            .build();
        pop.set_parent(cell);
        {
            let s = sender.clone();
            let p = path.to_string();
            let pp = pop.clone();
            more.connect_clicked(move |_| {
                pp.popdown();
                s.input(Msg::Click(p.clone(), false, false));
                s.input(Msg::ToggleInfo);
            });
        }
        content.append(&more);
        pop.connect_closed(|p| {
            let p = p.clone();
            glib::idle_add_local_once(move || p.unparent());
        });
        pop.popup();
        if self.info_report == 2 {
            self.info_report = 3;
            println!("info: popover rows={}", short.len());
        }
    }

    /// (Re)starts the background blur scan if the setting is on.
    fn start_blur(&mut self, sender: &ComponentSender<Self>) {
        if let Some(c) = self.blur_cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        if !self.state.blur_enabled() || !self.state.has_library() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.blur_cancel = Some(cancel.clone());
        let first = self.state.suggested_scan_first();
        let job = BlurJob {
            items: self.state.blur_scan_order(),
            first,
            cache_path: unswiper_blur::cache::QualityCache::path_in(self.state.store.support_dir()),
            thresholds: Thresholds::for_sensitivity(self.state.blur_sensitivity()),
            cancel,
        };
        let tx = sender.input_sender().clone();
        std::thread::spawn(move || {
            let last = std::sync::Mutex::new(std::time::Instant::now());
            crate::blur::run(job, &|e| match e {
                BlurEvent::Progress(d, t) => {
                    let mut l = last.lock().unwrap();
                    if d == t || l.elapsed().as_millis() > 150 {
                        *l = std::time::Instant::now();
                        tx.emit(Msg::BlurProgress(d, t));
                    }
                }
                BlurEvent::Result {
                    flagged,
                    scored,
                    last,
                } => tx.emit(Msg::BlurResult(flagged, scored, last)),
            });
        });
    }

    /// Loads the viewed item into the viewer page (still, video or Live Photo motion).
    fn show_viewer(&mut self, w: &mut Widgets, sender: &ComponentSender<Self>) {
        let Some(m) = self.state.viewer_media() else {
            return;
        };
        let v = &mut w.viewer;
        v.shown = Some(m.path.clone());
        v.video.set_media_stream(None::<&gtk::MediaStream>);
        v.picture.set_paintable(None::<&gdk::Paintable>);
        match &m.play {
            Some(file) => {
                let stream = gtk::MediaFile::for_filename(file);
                stream.set_muted(self.state.muted);
                let is_motion = m.is_motion;
                let report = self.opts.present_viewer;
                let reported = self.viewer_reported.clone();
                stream.connect_prepared_notify(move |st| {
                    if !st.is_prepared() {
                        return;
                    }
                    let d = (st.duration() > 0)
                        .then(|| std::time::Duration::from_micros(st.duration() as u64));
                    st.set_loop(crate::player::should_loop(d, is_motion));
                    if report && !reported.replace(true) {
                        println!(
                            "viewer: prepared duration_ms={}",
                            d.map(|d| d.as_millis()).unwrap_or(0)
                        );
                    }
                });
                let media = v.media.clone();
                let reported = self.viewer_reported.clone();
                stream.connect_error_notify(move |st| {
                    if let Some(e) = st.error() {
                        media.set_visible_child_name("error");
                        if report && !reported.replace(true) {
                            println!("viewer: error {e}");
                        }
                    }
                });
                let s = sender.clone();
                stream.connect_muted_notify(move |st| s.input(Msg::ViewerMuted(st.is_muted())));
                v.video.set_loop(is_motion);
                v.video.set_media_stream(Some(&stream));
                v.media.set_visible_child_name("video");
            }
            None => {
                v.media.set_visible_child_name("picture");
                let tx = sender.input_sender().clone();
                let path = m.path.clone();
                std::thread::spawn(move || {
                    let p = std::path::Path::new(&path);
                    if let Some(t) = thumbs::load(p, unswiper_core::MediaKind::Photo, 2048, None) {
                        tx.emit(Msg::ViewerImage(path, ThumbMsg(t)));
                    }
                });
            }
        }
    }

    fn stop_blur(&mut self, w: &mut Widgets) {
        if let Some(c) = self.blur_cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        w.blur_status.set_visible(false);
    }

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
            // Google Photos (Takeout) libraries also use Google's JSON sidecars (plan §19.5).
            let dates = crate::takeout::dates_for(&root);
            let r = index(&root, gap, &decisions, dates.as_ref(), |p| {
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
        let on_batch = move |b| tx.emit(Msg::Watch(b));
        self.watcher = if crate::state::is_portal_path(&root) {
            // Document-portal folders (Flatpak) get no inotify events: poll every 5 s.
            FolderWatcher::start_polling(
                &root,
                std::time::Duration::from_secs(5),
                DEFAULT_QUIET,
                DEFAULT_MAX_WAIT,
                on_batch,
            )
        } else {
            FolderWatcher::start(&root, DEFAULT_QUIET, DEFAULT_MAX_WAIT, on_batch)
        }
        .ok();
    }

    /// Rebuilds the thumbnail grid for the open visit and starts loading thumbnails.
    fn rebuild_grid(&mut self, w: &mut Widgets, sender: &ComponentSender<Self>) {
        self.thumb_gen += 1;
        w.grid.remove_all();
        w.cells.clear();
        w.durations.clear();
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
                // Play badge; the duration is filled in by the thumbnail thread.
                let dur = gtk::Label::builder()
                    .label("▶")
                    .halign(gtk::Align::Start)
                    .valign(gtk::Align::End)
                    .css_classes(["duration"])
                    .build();
                overlay.add_overlay(&dur);
                w.durations.insert(it.path.clone(), dur);
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
            click.connect_pressed(move |g, n, _, _| {
                if n == 2 {
                    s.input(Msg::OpenViewer(p.clone()));
                    return;
                }
                let m = g.current_event_state();
                s.input(Msg::Click(
                    p.clone(),
                    m.contains(gdk::ModifierType::CONTROL_MASK),
                    m.contains(gdk::ModifierType::SHIFT_MASK),
                ));
            });
            cell.add_controller(click);
            let menu_click = gtk::GestureClick::builder()
                .button(gdk::BUTTON_SECONDARY)
                .build();
            let s = sender.clone();
            let p = it.path.clone();
            menu_click.connect_pressed(move |_, _, _, _| s.input(Msg::InfoPopover(p.clone())));
            cell.add_controller(menu_click);
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
                if k == unswiper_core::MediaKind::Video {
                    if let Some(d) = crate::video::duration(std::path::Path::new(&p)) {
                        if tx
                            .send(Msg::Duration(gen, p.clone(), d.as_millis() as u64))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
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
        w.blur_action.set_state(&st.blur_enabled().to_variant());
        w.blur_sens_action
            .set_state(&st.blur_sensitivity().as_str().to_variant());

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

        w.live_action.set_state(&st.live_motion().to_variant());
        w.content.set_visible_child_name(if st.viewer.is_some() {
            "viewer"
        } else if st.visit.is_some() {
            "grid"
        } else {
            "empty"
        });
        if st.viewer.is_none() && w.viewer.shown.take().is_some() {
            // Stop playback when leaving the viewer.
            w.viewer.video.set_media_stream(None::<&gtk::MediaStream>);
        }
        if let (Some(it), Some((i, n))) = (st.viewer_item(), st.viewer_position()) {
            let v = &w.viewer;
            v.title.set_label(it.file_name());
            v.position.set_label(&format!("{} of {n}", i + 1));
            v.prev.set_sensitive(i > 0);
            v.next.set_sensitive(i + 1 < n);
            v.badge.remove_css_class("keep");
            v.badge.remove_css_class("reject");
            v.badge.remove_css_class("blurry");
            let (label, class) = match it.decision {
                Decision::Keep => ("Keep", "keep"),
                Decision::Reject if unswiper_core::quality::is_suggested(it) => {
                    ("Blurry?", "blurry")
                }
                Decision::Reject => ("Reject", "reject"),
                Decision::Undecided => ("", ""),
            };
            v.badge.set_label(label);
            v.badge.set_visible(!label.is_empty());
            if !class.is_empty() {
                v.badge.add_css_class(class);
            }
            let _ = &v.error;
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
            badge.remove_css_class("blurry");
            badge.set_tooltip_text(None);
            cell.remove_css_class("rejected");
            match it.decision {
                Decision::Keep => {
                    badge.set_label("Keep");
                    badge.add_css_class("keep");
                    badge.set_visible(true);
                }
                Decision::Reject if unswiper_core::quality::is_suggested(it) => {
                    badge.set_label("Blurry?");
                    badge.add_css_class("blurry");
                    badge.set_tooltip_text(Some("Unswiper thinks this photo is blurry and marked it Reject. Press K to keep it or U to clear. Nothing moves until you commit."));
                    cell.add_css_class("rejected");
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

fn clear_box(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
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
