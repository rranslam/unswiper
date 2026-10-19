// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! The "Import from Google Photos" assistant window (plan §19.4): eight steps from opening Google
//! Takeout to opening the unpacked library. Texts come from the shared guide
//! ([`unswiper_takeout::guide`]); figures and words from [`crate::takeout`].

use crate::help;
use crate::takeout::{progress_text, report_lines, step_label, Setup};
use relm4::adw::prelude::*;
use relm4::gtk::{gio, glib};
use relm4::{adw, gtk};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Instant;
use unswiper_takeout::cli::plan_lines;
use unswiper_takeout::guide::{guide, Guide};
use unswiper_takeout::{
    find_archives, plan, unpack_cancellable, Plan, UnpackError, UnpackProgress, UnpackReport,
};

/// Drive steps 6–8 on their own (smoke test): set both folders, check, unpack, open the library,
/// and print what happens to stdout (`takeout: …`).
#[derive(Debug, Clone)]
pub struct Auto {
    pub archives: PathBuf,
    pub library: PathBuf,
}

#[derive(Default)]
enum Run {
    #[default]
    Idle,
    Checking,
    Unpacking(Arc<AtomicBool>),
}

struct Model {
    guide: Guide,
    support: PathBuf,
    setup: Setup,
    plan: Option<Result<Plan, String>>,
    run: Run,
    /// Unpack finished in this session (Next allowed on step 7).
    unpacked: Option<UnpackReport>,
    unpack_problem: Option<String>,
    auto: Option<Auto>,
}

struct Ui {
    window: adw::Window,
    model: RefCell<Model>,
    step_label: gtk::Label,
    title: gtk::Label,
    body: gtk::Label,
    takeout_box: gtk::Box,
    folders_box: gtk::Box,
    archives_label: gtk::Label,
    library_label: gtk::Label,
    options_box: gtk::Box,
    delete_check: gtk::CheckButton,
    dup_check: gtk::CheckButton,
    figures: gtk::Label,
    check_button: gtk::Button,
    unpack_box: gtk::Box,
    progress: gtk::ProgressBar,
    unpack_status: gtk::Label,
    unpack_button: gtk::Button,
    stop_button: gtk::Button,
    index_box: gtk::Box,
    back: gtk::Button,
    next: gtk::Button,
    open_library: Box<dyn Fn(PathBuf)>,
}

fn wrap_label() -> gtk::Label {
    gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .selectable(true)
        .build()
}

fn folder_row(title: &str, value: &gtk::Label, button: &gtk::Button) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let t = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .width_chars(16)
        .css_classes(["heading"])
        .build();
    value.set_hexpand(true);
    value.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    value.set_xalign(0.0);
    b.append(&t);
    b.append(value);
    b.append(button);
    b
}

fn shown(p: &Option<PathBuf>) -> String {
    p.as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "Not chosen yet".into())
}

/// Opens the assistant. `open_library` is called with the library folder at the last step.
pub fn open(
    parent: &adw::ApplicationWindow,
    support: PathBuf,
    open_library: impl Fn(PathBuf) + 'static,
    auto: Option<Auto>,
) -> adw::Window {
    let guide = guide();
    let mut setup = Setup::load(&support);
    setup.step = setup.step.min(guide.steps.len() - 1);
    if let Some(a) = &auto {
        setup.archives_dir = Some(a.archives.clone());
        setup.library_dir = Some(a.library.clone());
        setup.step = 5;
    }

    let step_label_w = gtk::Label::builder()
        .xalign(0.0)
        .css_classes(["dim-label", "caption-heading"])
        .build();
    let title = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .css_classes(["title-2"])
        .build();
    let body = wrap_label();
    body.set_use_markup(true);

    // Step 1: open Takeout.
    let takeout_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let open_takeout = gtk::Button::builder()
        .label("Open Google Takeout")
        .css_classes(["pill", "suggested-action"])
        .build();
    let open_plain = gtk::Button::builder()
        .label("Open takeout.google.com")
        .css_classes(["flat"])
        .build();
    takeout_box.append(&open_takeout);
    takeout_box.append(&open_plain);

    // Step 6: folders and figures.
    let archives_label = gtk::Label::new(None);
    let library_label = gtk::Label::new(None);
    let choose_archives = gtk::Button::with_label("Choose…");
    let choose_library = gtk::Button::with_label("Choose…");
    let folders_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
    folders_box.append(&folder_row(
        "Archives folder",
        &archives_label,
        &choose_archives,
    ));
    folders_box.append(&folder_row(
        "Library folder",
        &library_label,
        &choose_library,
    ));

    // Steps 6 and 7: options.
    let delete_check = gtk::CheckButton::with_label("Delete each archive after unpacking");
    let dup_check = gtk::CheckButton::with_label(
        "Also unpack album copies (complete album folders, needs more space)",
    );
    let options_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    options_box.append(&delete_check);
    options_box.append(&dup_check);
    let figures = wrap_label();
    let check_button = gtk::Button::builder()
        .label("Check Again")
        .halign(gtk::Align::Start)
        .build();
    folders_box.append(&figures);
    folders_box.append(&check_button);

    // Step 7: unpack.
    let unpack_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let progress = gtk::ProgressBar::builder().show_text(false).build();
    let unpack_status = wrap_label();
    let unpack_button = gtk::Button::builder()
        .label("Unpack")
        .css_classes(["pill", "suggested-action"])
        .build();
    let stop_button = gtk::Button::builder()
        .label("Stop")
        .css_classes(["pill"])
        .build();
    let ub = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    ub.append(&unpack_button);
    ub.append(&stop_button);
    unpack_box.append(&progress);
    unpack_box.append(&unpack_status);
    unpack_box.append(&ub);

    // Step 8: open the library.
    let index_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let open_lib = gtk::Button::builder()
        .label("Open Library")
        .css_classes(["pill", "suggested-action"])
        .build();
    index_box.append(&open_lib);

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .margin_start(28)
        .margin_end(28)
        .margin_top(20)
        .margin_bottom(20)
        .build();
    for w in [
        step_label_w.upcast_ref::<gtk::Widget>(),
        title.upcast_ref(),
        body.upcast_ref(),
        takeout_box.upcast_ref(),
        folders_box.upcast_ref(),
        options_box.upcast_ref(),
        unpack_box.upcast_ref(),
        index_box.upcast_ref(),
    ] {
        content.append(w);
    }
    let back = gtk::Button::with_label("Back");
    let next = gtk::Button::builder()
        .label("Next")
        .css_classes(["suggested-action"])
        .build();
    let nav = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .margin_start(28)
        .margin_end(28)
        .margin_top(8)
        .margin_bottom(16)
        .build();
    let spacer = gtk::Box::builder().hexpand(true).build();
    nav.append(&back);
    nav.append(&spacer);
    nav.append(&next);

    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.append(
        &gtk::ScrolledWindow::builder()
            .child(&content)
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build(),
    );
    page.append(&nav);
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());
    tv.set_content(Some(&page));
    let window = adw::Window::builder()
        .title(&guide.title)
        .default_width(720)
        .default_height(640)
        .transient_for(parent)
        .content(&tv)
        .build();

    let ui = Rc::new(Ui {
        window: window.clone(),
        model: RefCell::new(Model {
            guide,
            support,
            setup,
            plan: None,
            run: Run::Idle,
            unpacked: None,
            unpack_problem: None,
            auto,
        }),
        step_label: step_label_w,
        title,
        body,
        takeout_box,
        folders_box,
        archives_label,
        library_label,
        options_box,
        delete_check,
        dup_check,
        figures,
        check_button,
        unpack_box,
        progress,
        unpack_status,
        unpack_button,
        stop_button,
        index_box,
        back,
        next,
        open_library: Box::new(open_library),
    });

    {
        let (s, w) = (ui.clone(), window.clone());
        open_takeout.connect_clicked(move |_| {
            let url = s.model.borrow().guide.takeout_url.clone();
            gtk::UriLauncher::new(&url).launch(Some(&w), None::<&gio::Cancellable>, |_| {});
        });
    }
    {
        let (s, w) = (ui.clone(), window.clone());
        open_plain.connect_clicked(move |_| {
            let url = s.model.borrow().guide.fallback_url.clone();
            gtk::UriLauncher::new(&url).launch(Some(&w), None::<&gio::Cancellable>, |_| {});
        });
    }
    for (button, archives) in [(choose_archives, true), (choose_library, false)] {
        let s = ui.clone();
        button.connect_clicked(move |_| choose_folder(&s, archives));
    }
    {
        let s = ui.clone();
        ui.delete_check.connect_toggled(move |c| {
            {
                let mut m = s.model.borrow_mut();
                if m.setup.delete_archives == c.is_active() {
                    return;
                }
                m.setup.delete_archives = c.is_active();
                m.plan = None;
            }
            changed_options(&s);
        });
    }
    {
        let s = ui.clone();
        ui.dup_check.connect_toggled(move |c| {
            {
                let mut m = s.model.borrow_mut();
                if m.setup.keep_duplicates == c.is_active() {
                    return;
                }
                m.setup.keep_duplicates = c.is_active();
                m.plan = None;
            }
            changed_options(&s);
        });
    }
    {
        let s = ui.clone();
        ui.check_button.connect_clicked(move |_| start_check(&s));
    }
    {
        let s = ui.clone();
        ui.unpack_button.connect_clicked(move |_| start_unpack(&s));
    }
    {
        let s = ui.clone();
        ui.stop_button.connect_clicked(move |_| {
            if let Run::Unpacking(c) = &s.model.borrow().run {
                c.store(true, Ordering::Relaxed);
            }
        });
    }
    {
        let s = ui.clone();
        open_lib.connect_clicked(move |_| finish(&s));
    }
    {
        let s = ui.clone();
        ui.back.connect_clicked(move |_| go(&s, -1));
    }
    {
        let s = ui.clone();
        ui.next.connect_clicked(move |_| go(&s, 1));
    }
    {
        // Closing stops a running unpack (it continues next time).
        let s = ui.clone();
        window.connect_close_request(move |_| {
            if let Run::Unpacking(c) = &s.model.borrow().run {
                c.store(true, Ordering::Relaxed);
            }
            glib::Propagation::Proceed
        });
    }
    refresh(&ui);
    let step = ui.model.borrow().setup.step;
    if step == 5 {
        start_check(&ui);
    }
    window
}

fn save(ui: &Ui) {
    let m = ui.model.borrow();
    let _ = m.setup.save(&m.support);
}

fn go(ui: &Rc<Ui>, delta: isize) {
    {
        let mut m = ui.model.borrow_mut();
        let n = m.guide.steps.len() as isize;
        m.setup.step = (m.setup.step as isize + delta).clamp(0, n - 1) as usize;
    }
    save(ui);
    refresh(ui);
    let step = ui.model.borrow().setup.step;
    if step == 5 && ui.model.borrow().plan.is_none() {
        start_check(ui);
    }
}

fn changed_options(ui: &Rc<Ui>) {
    save(ui);
    let busy = !matches!(ui.model.borrow().run, Run::Idle);
    if !busy && ui.model.borrow().setup.folder_problem().is_none() {
        start_check(ui);
    } else {
        refresh(ui);
    }
}

fn choose_folder(ui: &Rc<Ui>, archives: bool) {
    let dlg = gtk::FileDialog::builder()
        .title(if archives {
            "Choose the Folder with the Downloaded Archives"
        } else {
            "Choose the Library Folder"
        })
        .modal(true)
        .build();
    let current = {
        let m = ui.model.borrow();
        if archives {
            m.setup.archives_dir.clone()
        } else {
            m.setup.library_dir.clone()
        }
    };
    if let Some(c) = current.filter(|c| c.is_dir()) {
        dlg.set_initial_folder(Some(&gio::File::for_path(c)));
    }
    let s = ui.clone();
    dlg.select_folder(Some(&ui.window), None::<&gio::Cancellable>, move |res| {
        if let Some(p) = res.ok().and_then(|f| f.path()) {
            {
                let mut m = s.model.borrow_mut();
                if archives {
                    m.setup.archives_dir = Some(p);
                } else {
                    m.setup.library_dir = Some(p);
                }
                m.plan = None;
            }
            changed_options(&s);
        }
    });
}

/// Polls `rx` on the main loop until `on` returns false.
fn poll<T: 'static>(rx: mpsc::Receiver<T>, mut on: impl FnMut(T) -> bool + 'static) {
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || loop {
        match rx.try_recv() {
            Ok(v) => {
                if !on(v) {
                    return glib::ControlFlow::Break;
                }
            }
            Err(mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => return glib::ControlFlow::Break,
        }
    });
}

fn start_check(ui: &Rc<Ui>) {
    let (archives_dir, library, opts) = {
        let mut m = ui.model.borrow_mut();
        if !matches!(m.run, Run::Idle) || m.setup.folder_problem().is_some() {
            drop(m);
            refresh(ui);
            return;
        }
        m.run = Run::Checking;
        (
            m.setup.archives_dir.clone().unwrap_or_default(),
            m.setup.library_dir.clone().unwrap_or_default(),
            m.setup.options(),
        )
    };
    refresh(ui);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let r = find_archives(&archives_dir)
            .and_then(|a| plan(&a, &library, &opts, false))
            .map_err(|e| match e {
                UnpackError::NoArchives(_) => "No .zip or .tgz files in the archives folder. Download the parts from Google's email into this folder (step 5), or choose the folder you saved them in.".to_string(),
                e => e.to_string(),
            });
        let _ = tx.send(r);
    });
    let s = ui.clone();
    poll(rx, move |r: Result<Plan, String>| {
        let fits = {
            let mut m = s.model.borrow_mut();
            m.run = Run::Idle;
            let opts = m.setup.options();
            let fits = matches!(&r, Ok(p) if p.fits(&opts));
            if m.auto.is_some() {
                match &r {
                    Ok(p) => println!(
                        "takeout: plan archives={} items={} new_items={} fits={fits}",
                        p.archives.len(),
                        p.items,
                        p.new_items
                    ),
                    Err(e) => println!("takeout: plan error {e}"),
                }
            }
            m.plan = Some(r);
            fits
        };
        refresh(&s);
        if fits && s.model.borrow().auto.is_some() {
            go(&s, 1);
            start_unpack(&s);
        }
        false
    });
}

enum Event {
    Progress(UnpackProgress),
    Done(Result<UnpackReport, UnpackError>),
}

fn start_unpack(ui: &Rc<Ui>) {
    let cancel = Arc::new(AtomicBool::new(false));
    let (archives_dir, library, opts) = {
        let mut m = ui.model.borrow_mut();
        if !matches!(m.run, Run::Idle) || m.setup.folder_problem().is_some() {
            return;
        }
        m.run = Run::Unpacking(cancel.clone());
        m.unpack_problem = None;
        m.unpacked = None;
        (
            m.setup.archives_dir.clone().unwrap_or_default(),
            m.setup.library_dir.clone().unwrap_or_default(),
            m.setup.options(),
        )
    };
    ui.unpack_status.set_text("Starting…");
    ui.progress.set_fraction(0.0);
    refresh(ui);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let r = find_archives(&archives_dir).and_then(|a| {
            let ptx = tx.clone();
            unpack_cancellable(&a, &library, &opts, &cancel, move |p| {
                let _ = ptx.send(Event::Progress(p));
            })
        });
        let _ = tx.send(Event::Done(r));
    });
    let started = Instant::now();
    let s = ui.clone();
    poll(rx, move |ev| match ev {
        Event::Progress(p) => {
            let (f, t) = progress_text(&p, started.elapsed());
            s.progress.set_fraction(f);
            s.unpack_status.set_text(&t);
            true
        }
        Event::Done(r) => {
            let auto = {
                let mut m = s.model.borrow_mut();
                m.run = Run::Idle;
                match r {
                    Ok(rep) => {
                        if m.auto.is_some() {
                            println!(
                                "takeout: unpacked extracted={} duplicates_skipped={} deleted={}",
                                rep.extracted,
                                rep.duplicates_skipped,
                                rep.archives_deleted.len()
                            );
                        }
                        m.unpacked = Some(rep);
                    }
                    Err(UnpackError::Cancelled) => {
                        m.unpack_problem =
                            Some("Stopped. Click Unpack to continue where it stopped.".into())
                    }
                    Err(e) => m.unpack_problem = Some(e.to_string()),
                }
                m.plan = None; // figures changed
                m.auto.is_some() && m.unpacked.is_some()
            };
            if s.model.borrow().unpacked.is_some() {
                s.progress.set_fraction(1.0);
            }
            refresh(&s);
            if auto {
                go(&s, 1);
                finish(&s);
            }
            false
        }
    });
}

fn finish(ui: &Rc<Ui>) {
    let lib = {
        let mut m = ui.model.borrow_mut();
        // Next time the assistant opens on "Choose the folders" (for the next export).
        m.setup.step = 5;
        m.setup.library_dir.clone()
    };
    save(ui);
    if let Some(lib) = lib {
        (ui.open_library)(lib);
    }
    ui.window.close();
}

fn refresh(ui: &Ui) {
    let m = ui.model.borrow();
    let n = m.guide.steps.len();
    let step = &m.guide.steps[m.setup.step];
    ui.step_label.set_text(&step_label(m.setup.step, n));
    ui.title.set_text(&step.title);
    ui.body.set_markup(&help::to_pango(&step.body));
    let action = step.action.as_str();
    ui.takeout_box.set_visible(action == "open_takeout");
    ui.folders_box.set_visible(action == "choose_folders");
    ui.options_box
        .set_visible(action == "choose_folders" || action == "unpack");
    ui.unpack_box.set_visible(action == "unpack");
    ui.index_box.set_visible(action == "index");

    ui.archives_label.set_text(&shown(&m.setup.archives_dir));
    ui.library_label.set_text(&shown(&m.setup.library_dir));
    let (delete, keep_dups) = (m.setup.delete_archives, m.setup.keep_duplicates);
    let idle = matches!(m.run, Run::Idle);
    let unpacking = matches!(m.run, Run::Unpacking(_));
    ui.delete_check.set_sensitive(!unpacking);
    ui.dup_check.set_sensitive(!unpacking);
    let opts = m.setup.options();
    let (text, fits) = match (&m.run, m.setup.folder_problem(), &m.plan) {
        (Run::Checking, _, _) => ("Checking the archives…".to_string(), false),
        (_, Some(p), _) => (p.to_string(), false),
        (_, None, Some(Ok(p))) => (plan_lines(p, &opts).join("\n"), p.fits(&opts)),
        (_, None, Some(Err(e))) => (e.clone(), false),
        (_, None, None) => (String::new(), false),
    };
    ui.figures.set_text(&text);
    ui.check_button
        .set_sensitive(idle && m.setup.folder_problem().is_none());

    // The space check is mandatory: no Unpack without a dry run that fits (the core checks too).
    ui.unpack_button
        .set_sensitive(idle && m.setup.folder_problem().is_none() && fits);
    ui.unpack_button.set_label(if m.unpack_problem.is_some() {
        "Continue Unpacking"
    } else {
        "Unpack"
    });
    ui.stop_button.set_visible(unpacking);
    if idle {
        if let Some(r) = &m.unpacked {
            ui.unpack_status.set_text(&report_lines(r).join("\n"));
        } else if let Some(p) = &m.unpack_problem {
            ui.unpack_status.set_text(p);
        } else if ui.unpack_status.text() == "Starting…" {
            ui.unpack_status.set_text("");
        }
    }

    ui.back.set_sensitive(m.setup.step > 0 && !unpacking);
    ui.next.set_visible(m.setup.step + 1 < n);
    ui.next.set_sensitive(match action {
        "choose_folders" => idle && fits,
        "unpack" => idle && m.unpacked.is_some(),
        _ => true,
    });
    drop(m);
    // After the borrow ends: setting a check button runs its toggled handler.
    if ui.delete_check.is_active() != delete {
        ui.delete_check.set_active(delete);
    }
    if ui.dup_check.is_active() != keep_dups {
        ui.dup_check.set_active(keep_dups);
    }
}
