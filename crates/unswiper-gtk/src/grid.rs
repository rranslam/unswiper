// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! The photo grid of a visit: a `GtkGridView` that only builds cells for what is on screen
//! and recycles them while scrolling, so visits with thousands of photos open instantly.
//! Thumbnails are requested when a cell is bound and dropped from the queue when it is
//! unbound; [`Loader`] reads them on worker threads, newest request first.

use crate::thumbs::Thumb;
use relm4::gtk::prelude::*;
use relm4::gtk::{self, gdk, gio, glib};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};
use unswiper_core::MediaKind;

/// What a loaded thumbnail job produced.
#[derive(Debug)]
pub enum LoaderEvent {
    Thumb(String, Thumb),
    /// Video duration in milliseconds.
    Duration(String, u64),
}

/// Reads a thumbnail (and a video's duration).
pub type LoadFn = dyn Fn(&str, MediaKind) -> (Option<Thumb>, Option<u64>) + Send + Sync;

#[derive(Default)]
struct Queue {
    /// Requests in order; the newest is served first. Entries no longer wanted are skipped.
    order: Vec<(String, MediaKind)>,
    wanted: HashSet<String>,
    stop: bool,
}

/// Thumbnail reader threads with a cancellable request queue.
pub struct Loader {
    q: Arc<(Mutex<Queue>, Condvar)>,
}

impl Loader {
    pub fn start(
        threads: usize,
        load: Arc<LoadFn>,
        emit: Arc<dyn Fn(LoaderEvent) + Send + Sync>,
    ) -> Self {
        let q: Arc<(Mutex<Queue>, Condvar)> = Default::default();
        for _ in 0..threads.max(1) {
            let q = q.clone();
            let load = load.clone();
            let emit = emit.clone();
            std::thread::spawn(move || loop {
                let (path, kind) = {
                    let (m, cv) = &*q;
                    let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
                    loop {
                        if g.stop {
                            return;
                        }
                        match g.order.pop() {
                            Some((p, k)) if g.wanted.remove(&p) => break (p, k),
                            Some(_) => continue,
                            None => g = cv.wait(g).unwrap_or_else(|e| e.into_inner()),
                        }
                    }
                };
                let (thumb, dur) = load(&path, kind);
                if let Some(d) = dur {
                    emit(LoaderEvent::Duration(path.clone(), d));
                }
                if let Some(t) = thumb {
                    emit(LoaderEvent::Thumb(path, t));
                }
            });
        }
        Loader { q }
    }

    fn with<R>(&self, f: impl FnOnce(&mut Queue) -> R) -> R {
        let mut g = self.q.0.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut g)
    }

    /// Asks for `path` (no-op when it is already waiting).
    pub fn request(&self, path: &str, kind: MediaKind) {
        let new = self.with(|q| {
            let new = q.wanted.insert(path.to_string());
            if new {
                q.order.push((path.to_string(), kind));
            }
            new
        });
        if new {
            self.q.1.notify_one();
        }
    }

    /// Drops a waiting request (its cell scrolled away).
    pub fn cancel(&self, path: &str) {
        self.with(|q| q.wanted.remove(path));
    }

    /// Drops every waiting request (another visit or filter).
    pub fn clear(&self) {
        self.with(|q| {
            q.wanted.clear();
            q.order.clear();
        });
    }

    /// Requests still waiting.
    pub fn pending(&self) -> usize {
        self.with(|q| q.wanted.len())
    }
}

impl Drop for Loader {
    fn drop(&mut self) {
        self.with(|q| q.stop = true);
        self.q.1.notify_all();
    }
}

/// Badge of a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Badge {
    #[default]
    None,
    Keep,
    Blurry,
    Reject,
}

/// What a cell shows besides its photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Look {
    pub picked: bool,
    pub badge: Badge,
}

/// User actions on a cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GridEvent {
    /// Click (path, ctrl, shift); also Enter on the focused cell.
    Click(String, bool, bool),
    /// Double-click.
    Open(String),
    /// Right-click.
    Menu(String),
}

/// One grid entry.
#[derive(Debug, Clone)]
pub struct GridItem {
    pub path: String,
    pub kind: MediaKind,
}

#[derive(Clone)]
struct Cell {
    root: gtk::Box,
    pic: gtk::Picture,
    badge: gtk::Label,
    dur: gtk::Label,
    caption: gtk::Label,
}

impl Cell {
    /// The widgets of a cell built by the factory's setup.
    fn of(root: &gtk::Widget) -> Option<Cell> {
        let root = root.clone().downcast::<gtk::Box>().ok()?;
        let overlay = root.first_child()?.downcast::<gtk::Overlay>().ok()?;
        let caption = root.last_child()?.downcast::<gtk::Label>().ok()?;
        let pic = overlay.child()?.downcast::<gtk::Picture>().ok()?;
        let labels: Vec<gtk::Label> =
            std::iter::successors(overlay.first_child(), |w| w.next_sibling())
                .filter_map(|w| w.downcast::<gtk::Label>().ok())
                .collect();
        let badge = labels.iter().find(|l| l.has_css_class("badge"))?.clone();
        let dur = labels.iter().find(|l| l.has_css_class("duration"))?.clone();
        Some(Cell {
            root,
            pic,
            badge,
            dur,
            caption,
        })
    }

    fn path(&self) -> String {
        self.root.widget_name().to_string()
    }
}

const BLURRY_TIP: &str = "Unswiper thinks this photo is blurry and marked it Reject. Press K to keep it or U to clear. Nothing moves until you commit.";

/// Sets a cell's look, touching only what changed (`old` is what it shows now).
fn apply_look(c: &Cell, old: Option<Look>, new: Look) {
    if old.map(|o| o.picked) != Some(new.picked) {
        if new.picked {
            c.root.add_css_class("picked");
        } else {
            c.root.remove_css_class("picked");
        }
    }
    if old.map(|o| o.badge) == Some(new.badge) {
        return;
    }
    for k in ["keep", "reject", "blurry"] {
        c.badge.remove_css_class(k);
    }
    c.root.remove_css_class("rejected");
    let (label, class) = match new.badge {
        Badge::None => {
            c.badge.set_visible(false);
            return;
        }
        Badge::Keep => ("Keep", "keep"),
        Badge::Blurry => ("Blurry?", "blurry"),
        Badge::Reject => ("Reject", "reject"),
    };
    c.badge.set_label(label);
    c.badge.add_css_class(class);
    if new.badge != Badge::Keep {
        c.root.add_css_class("rejected");
    }
    c.badge.set_visible(true);
}

/// Thumbnails kept in memory (about 150 KB each), besides the PNG cache on disk.
const TEXTURES: usize = 600;

struct Shared {
    looks: HashMap<String, Look>,
    kinds: HashMap<String, MediaKind>,
    textures: HashMap<String, gdk::Texture>,
    texture_order: VecDeque<String>,
    durations: HashMap<String, u64>,
    /// Bound cells by path, with the look they show.
    bound: HashMap<String, (Cell, Look)>,
    loader: Loader,
}

/// The grid widget and its model.
pub struct Grid {
    pub view: gtk::GridView,
    store: RefCell<gio::ListStore>,
    /// Bumped by every `set_items`, so a deferred model from an older call is dropped.
    gen: Rc<std::cell::Cell<u64>>,
    shared: Rc<RefCell<Shared>>,
}

impl Grid {
    pub fn new(thumb: i32, loader: Loader, on_event: Rc<dyn Fn(GridEvent)>) -> Grid {
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let shared = Rc::new(RefCell::new(Shared {
            looks: HashMap::new(),
            kinds: HashMap::new(),
            textures: HashMap::new(),
            texture_order: VecDeque::new(),
            durations: HashMap::new(),
            bound: HashMap::new(),
            loader,
        }));
        let f = gtk::SignalListItemFactory::new();
        {
            let on_event = on_event.clone();
            f.connect_setup(move |_, li| {
                let Some(li) = li.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                li.set_child(Some(&build_cell(thumb, on_event.clone())));
            });
        }
        {
            let shared = shared.clone();
            f.connect_bind(move |_, li| {
                let Some(li) = li.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                let (Some(obj), Some(cell)) = (
                    li.item().and_downcast::<glib::BoxedAnyObject>(),
                    li.child().as_ref().and_then(Cell::of),
                ) else {
                    return;
                };
                let it = obj.borrow::<GridItem>();
                bind(&shared, &cell, &it);
            });
        }
        {
            let shared = shared.clone();
            f.connect_unbind(move |_, li| {
                let Some(cell) = li
                    .downcast_ref::<gtk::ListItem>()
                    .and_then(|li| li.child())
                    .as_ref()
                    .and_then(Cell::of)
                else {
                    return;
                };
                let path = cell.path();
                let mut sh = shared.borrow_mut();
                if sh
                    .bound
                    .get(&path)
                    .is_some_and(|(c, _)| c.root == cell.root)
                {
                    sh.bound.remove(&path);
                }
                sh.loader.cancel(&path);
                drop(sh);
                cell.pic.set_paintable(None::<&gdk::Paintable>);
            });
        }
        let view = gtk::GridView::builder()
            .model(&gtk::NoSelection::new(Some(store.clone())))
            .factory(&f)
            .min_columns(2)
            .max_columns(12)
            .single_click_activate(false)
            .margin_start(12)
            .margin_end(12)
            .css_classes(["photo-grid"])
            .build();
        {
            // Enter on the focused cell selects it, like a click.
            view.connect_activate(move |v, pos| {
                let item = v.model().and_then(|m| m.item(pos));
                if let Some(obj) = item.and_downcast::<glib::BoxedAnyObject>() {
                    let p = obj.borrow::<GridItem>().path.clone();
                    on_event(GridEvent::Click(p, false, false));
                }
            });
        }
        Grid {
            view,
            store: RefCell::new(store),
            gen: Default::default(),
            shared,
        }
    }

    /// Shows `items` with their looks (another visit or filter). Pending thumbnails of the
    /// previous items are dropped.
    pub fn set_items(&self, items: &[GridItem], looks: HashMap<String, Look>) {
        {
            let mut sh = self.shared.borrow_mut();
            sh.loader.clear();
            sh.looks = looks;
            sh.kinds = items.iter().map(|i| (i.path.clone(), i.kind)).collect();
        }
        let objs: Vec<glib::BoxedAnyObject> = items
            .iter()
            .map(|i| glib::BoxedAnyObject::new(i.clone()))
            .collect();
        // A new model rather than a splice: GTK drops the old items at once, which is several
        // times faster for thousands of items. Bind/unbind run in here; nothing is borrowed.
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        store.extend_from_slice(&objs);
        *self.store.borrow_mut() = store.clone();
        let gen = self.gen.get() + 1;
        self.gen.set(gen);
        let model = gtk::NoSelection::new(Some(store));
        if self.view.width() > 0 && self.view.is_mapped() {
            self.view.set_model(Some(&model));
        } else {
            // Not laid out yet (just shown): GTK would bind cells for 12 columns × 30 rows.
            // Wait for the first layout, when it knows how many fit.
            self.view.set_model(None::<&gtk::NoSelection>);
            let view = self.view.clone();
            let cur = self.gen.clone();
            glib::idle_add_local_once(move || {
                if cur.get() == gen {
                    view.set_model(Some(&model));
                }
            });
        }
        if let Some(a) = self.view.vadjustment() {
            a.set_value(0.0);
        }
    }

    /// New looks (selection, decisions, blur marks); only on-screen cells are touched.
    pub fn set_looks(&self, looks: HashMap<String, Look>) {
        let mut sh = self.shared.borrow_mut();
        sh.looks = looks;
        let Shared { looks, bound, .. } = &mut *sh;
        for (path, (cell, shown)) in bound.iter_mut() {
            let new = looks.get(path).copied().unwrap_or_default();
            if new != *shown {
                apply_look(cell, Some(*shown), new);
                *shown = new;
            }
        }
    }

    pub fn set_thumb(&self, path: String, t: Thumb) {
        let bytes = glib::Bytes::from_owned(t.rgba);
        let tex: gdk::Texture = gdk::MemoryTexture::new(
            t.width as i32,
            t.height as i32,
            gdk::MemoryFormat::R8g8b8a8,
            &bytes,
            t.width as usize * 4,
        )
        .upcast();
        let mut sh = self.shared.borrow_mut();
        if let Some((c, _)) = sh.bound.get(&path) {
            c.pic.set_paintable(Some(&tex));
        }
        if sh.textures.insert(path.clone(), tex).is_none() {
            sh.texture_order.push_back(path);
        }
        while sh.texture_order.len() > TEXTURES {
            if let Some(old) = sh.texture_order.pop_front() {
                sh.textures.remove(&old);
            }
        }
    }

    pub fn set_duration(&self, path: String, ms: u64) {
        let mut sh = self.shared.borrow_mut();
        if let Some((c, _)) = sh.bound.get(&path) {
            c.dur.set_label(&duration_label(ms));
        }
        sh.durations.insert(path, ms);
    }

    /// Forgets loaded thumbnails (files changed on disk).
    pub fn forget_thumbnails(&self) {
        let mut sh = self.shared.borrow_mut();
        sh.textures.clear();
        sh.texture_order.clear();
        sh.durations.clear();
    }

    /// The on-screen cell of `path`, to anchor a popover.
    pub fn cell_widget(&self, path: &str) -> Option<gtk::Widget> {
        self.shared
            .borrow()
            .bound
            .get(path)
            .map(|(c, _)| c.root.clone().upcast())
    }

    /// Number of items.
    pub fn len(&self) -> u32 {
        self.store.borrow().n_items()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Cells built right now (for tests: stays near what fits on screen).
    pub fn bound_cells(&self) -> usize {
        self.shared.borrow().bound.len()
    }

    /// Thumbnail requests still waiting.
    pub fn pending_thumbnails(&self) -> usize {
        self.shared.borrow().loader.pending()
    }
}

fn duration_label(ms: u64) -> String {
    format!(
        "▶ {}",
        crate::player::format_duration(std::time::Duration::from_millis(ms))
    )
}

fn bind(shared: &Rc<RefCell<Shared>>, cell: &Cell, it: &GridItem) {
    let mut sh = shared.borrow_mut();
    let name = it.path.rsplit('/').next().unwrap_or(&it.path).to_string();
    // The tooltips are answered on demand (query-tooltip): setting tooltip text is costly
    // in GTK and cells are bound often while scrolling.
    cell.root.set_widget_name(&it.path);
    cell.caption.set_label(&name);
    let video = it.kind == MediaKind::Video;
    cell.dur.set_visible(video);
    if video {
        cell.dur.set_label(
            &sh.durations
                .get(&it.path)
                .map(|&ms| duration_label(ms))
                .unwrap_or_else(|| "▶".into()),
        );
    }
    let look = sh.looks.get(&it.path).copied().unwrap_or_default();
    // A recycled cell may show anything: set the whole look.
    apply_look(cell, None, look);
    match sh.textures.get(&it.path) {
        Some(t) => cell.pic.set_paintable(Some(t)),
        None => {
            cell.pic.set_paintable(None::<&gdk::Paintable>);
            sh.loader.request(&it.path, it.kind);
        }
    }
    sh.bound.insert(it.path.clone(), (cell.clone(), look));
}

fn build_cell(thumb: i32, on_event: Rc<dyn Fn(GridEvent)>) -> gtk::Box {
    let pic = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Cover)
        .width_request(thumb)
        .height_request(thumb)
        .build();
    let badge = gtk::Label::builder()
        .halign(gtk::Align::End)
        .valign(gtk::Align::Start)
        .css_classes(["badge"])
        .visible(false)
        .build();
    let dur = gtk::Label::builder()
        .label("▶")
        .halign(gtk::Align::Start)
        .valign(gtk::Align::End)
        .css_classes(["duration"])
        .visible(false)
        .build();
    badge.set_has_tooltip(true);
    badge.connect_query_tooltip(|b, _, _, _, tip| {
        if !b.has_css_class("blurry") {
            return false;
        }
        tip.set_text(Some(BLURRY_TIP));
        true
    });
    let overlay = gtk::Overlay::builder().child(&pic).build();
    overlay.add_overlay(&badge);
    overlay.add_overlay(&dur);
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .css_classes(["thumb"])
        .build();
    root.set_has_tooltip(true);
    root.connect_query_tooltip(|r, _, _, _, tip| {
        let p = r.widget_name();
        tip.set_text(Some(p.rsplit('/').next().unwrap_or(&p)));
        true
    });
    root.append(&overlay);
    root.append(
        &gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .css_classes(["caption"])
            .build(),
    );
    // Left button: select on the press, open the viewer on the second press of a double-click.
    // (Switching to the viewer page while the button is down is fine: GTK moves the press to
    // the page that stays visible when the cell is unmapped.)
    let click = gtk::GestureClick::new();
    {
        let on_event = on_event.clone();
        let r = root.clone();
        click.connect_pressed(move |g, n, _, _| {
            on_event(press_event(
                n,
                g.current_event_state(),
                r.widget_name().as_str(),
            ));
        });
    }
    root.add_controller(click);
    let menu = gtk::GestureClick::builder()
        .button(gdk::BUTTON_SECONDARY)
        .build();
    {
        let r = root.clone();
        // Right button: the info popover opens on the RELEASE, not the press. Opened on the
        // press, the popover appears under the pointer while the button is still down; GTK has
        // marked the cell and its parents (GtkBox, ...) "active" for that press, and the
        // release then ends up on the popover's widgets, so GTK un-marks widgets it never
        // marked: "Broken accounting of active state for widget (GtkBox)" (GTK issue 7932,
        // fixed in GTK 4.21.6; the AppImage ships the distribution's older GTK).
        menu.connect_released(move |_, _, _, _| {
            on_event(GridEvent::Menu(r.widget_name().to_string()))
        });
    }
    root.add_controller(menu);
    root
}

/// What a primary-button press on a cell does: a double-click opens the viewer, any other
/// press selects (Ctrl/Shift extend).
fn press_event(n_press: i32, m: gdk::ModifierType, path: &str) -> GridEvent {
    if n_press == 2 {
        GridEvent::Open(path.to_string())
    } else {
        GridEvent::Click(
            path.to_string(),
            m.contains(gdk::ModifierType::CONTROL_MASK),
            m.contains(gdk::ModifierType::SHIFT_MASK),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    fn wait(f: impl Fn() -> bool) {
        let t = Instant::now();
        while !f() && t.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn press_selects_and_double_click_opens() {
        let none = gdk::ModifierType::empty();
        assert_eq!(
            press_event(1, gdk::ModifierType::CONTROL_MASK, "/a.jpg"),
            GridEvent::Click("/a.jpg".into(), true, false)
        );
        assert_eq!(
            press_event(1, gdk::ModifierType::SHIFT_MASK, "/a.jpg"),
            GridEvent::Click("/a.jpg".into(), false, true)
        );
        assert_eq!(
            press_event(2, none, "/a.jpg"),
            GridEvent::Open("/a.jpg".into())
        );
    }

    #[test]
    fn loader_serves_newest_first_and_skips_cancelled() {
        let gate = Arc::new(Mutex::new(()));
        let held = gate.lock().unwrap();
        let loads = Arc::new(Mutex::new(vec![]));
        let started = Arc::new(AtomicUsize::new(0));
        let (g, l, s) = (gate.clone(), loads.clone(), started.clone());
        let load: Arc<LoadFn> = Arc::new(move |p: &str, k: MediaKind| {
            s.fetch_add(1, Ordering::SeqCst);
            drop(g.lock().unwrap());
            l.lock().unwrap().push(p.to_string());
            let t = Thumb {
                width: 1,
                height: 1,
                rgba: vec![0; 4],
            };
            (Some(t), (k == MediaKind::Video).then_some(1500))
        });
        let events = Arc::new(Mutex::new(vec![]));
        let ev = events.clone();
        let loader = Loader::start(1, load, Arc::new(move |e| ev.lock().unwrap().push(e)));
        // The one worker blocks on "a" while the rest queue up.
        loader.request("a", MediaKind::Photo);
        wait(|| started.load(Ordering::SeqCst) == 1);
        for p in ["b", "c", "d"] {
            loader.request(p, MediaKind::Photo);
        }
        loader.request("v", MediaKind::Video);
        loader.request("c", MediaKind::Photo); // already waiting: no duplicate
        loader.cancel("d");
        assert_eq!(loader.pending(), 3);
        drop(held);
        wait(|| loads.lock().unwrap().len() == 4);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(*loads.lock().unwrap(), vec!["a", "v", "c", "b"]);
        let ev = events.lock().unwrap();
        assert!(ev
            .iter()
            .any(|e| matches!(e, LoaderEvent::Duration(p, 1500) if p == "v")));
        assert_eq!(
            ev.iter()
                .filter(|e| matches!(e, LoaderEvent::Thumb(..)))
                .count(),
            4
        );
        drop(ev);
        loader.request("e", MediaKind::Photo);
        loader.clear();
        assert_eq!(loader.pending(), 0);
    }

    /// A 6,000-item grid in a window builds only a few hundred cells, requests thumbnails
    /// only for those, and keeps looks across rebinding. Skipped without a display.
    #[test]
    fn large_grid_builds_only_visible_cells() {
        if gtk::init().is_err() {
            eprintln!("no display: skipped");
            return;
        }
        let load: Arc<LoadFn> = Arc::new(|_: &str, _| (None, None));
        let loader = Loader::start(1, load, Arc::new(|_| {}));
        let events = Rc::new(RefCell::new(vec![]));
        let ev = events.clone();
        let grid = Grid::new(120, loader, Rc::new(move |e| ev.borrow_mut().push(e)));
        let scroll = gtk::ScrolledWindow::builder().child(&grid.view).build();
        let win = gtk::Window::builder()
            .default_width(800)
            .default_height(600)
            .child(&scroll)
            .build();
        win.present();
        let items: Vec<GridItem> = (0..6000)
            .map(|i| GridItem {
                path: format!("/x/p{i:04}.jpg"),
                kind: MediaKind::Photo,
            })
            .collect();
        let looks: HashMap<String, Look> = [(
            "/x/p0000.jpg".to_string(),
            Look {
                picked: true,
                badge: Badge::Blurry,
            },
        )]
        .into();
        let spin = |ms: u64| {
            let t = Instant::now();
            while t.elapsed() < Duration::from_millis(ms) {
                glib::MainContext::default().iteration(false);
            }
        };
        spin(300);
        grid.set_items(&items, looks);
        spin(500);
        assert_eq!(grid.len(), 6000);
        let bound = grid.bound_cells();
        assert!(bound > 0 && bound < 1000, "bound {bound}");
        assert!(grid.pending_thumbnails() <= bound);
        let first = grid
            .cell_widget("/x/p0000.jpg")
            .expect("first cell on screen");
        assert!(first.has_css_class("picked") && first.has_css_class("rejected"));
        grid.set_looks(HashMap::new());
        assert!(!first.has_css_class("picked") && !first.has_css_class("rejected"));
        // Another visit: old requests are dropped.
        grid.set_items(&items[..3], HashMap::new());
        spin(300);
        assert_eq!(grid.len(), 3);
        assert_eq!(grid.bound_cells(), 3);
        assert!(grid.cell_widget("/x/p0100.jpg").is_none());
        win.close();
    }
}
