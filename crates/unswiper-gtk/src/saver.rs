// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Writes library.json on a background thread so large libraries don't freeze the window.
//! Saves are applied in order and only the newest pending one is written.

use std::sync::mpsc::{channel, Receiver, Sender};
use unswiper_core::store::LibraryStore;
use unswiper_core::LibrarySnapshot;

enum Job {
    Save(Box<(LibraryStore, LibrarySnapshot)>),
    Flush(Sender<()>),
}

pub struct Saver {
    tx: Sender<Job>,
}

impl Saver {
    /// Starts the writer. `on_error` runs on the writer thread with the error text.
    pub fn start(on_error: impl Fn(String) + Send + 'static) -> Self {
        let (tx, rx) = channel::<Job>();
        std::thread::spawn(move || run(rx, on_error));
        Saver { tx }
    }

    /// Queues a save.
    pub fn save(&self, store: LibraryStore, snap: LibrarySnapshot) {
        let _ = self.tx.send(Job::Save(Box::new((store, snap))));
    }

    /// Waits until everything queued so far is written.
    pub fn flush(&self) {
        let (tx, rx) = channel();
        if self.tx.send(Job::Flush(tx)).is_ok() {
            let _ = rx.recv();
        }
    }
}

fn run(rx: Receiver<Job>, on_error: impl Fn(String)) {
    while let Ok(mut job) = rx.recv() {
        // Skip to the newest save; answer flushes after the save before them is written.
        let mut flushes = vec![];
        loop {
            match rx.try_recv() {
                Ok(Job::Flush(f)) => flushes.push(f),
                Ok(next @ Job::Save(_)) => {
                    if let Job::Flush(f) = std::mem::replace(&mut job, next) {
                        flushes.push(f);
                    }
                }
                Err(_) => break,
            }
        }
        match job {
            Job::Save(b) => {
                let (store, snap) = *b;
                if let Err(e) = store.save(&snap) {
                    on_error(e.to_string());
                }
            }
            Job::Flush(f) => flushes.push(f),
        }
        for f in flushes {
            let _ = f.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_newest_and_flushes() {
        let d = tempfile::tempdir().unwrap();
        let store = LibraryStore::new(d.path().to_path_buf());
        let saver = Saver::start(|e| panic!("{e}"));
        for n in 0..5 {
            let mut snap = LibrarySnapshot::empty();
            snap.config.visit_gap_hours = n as f64;
            saver.save(store.clone(), snap);
        }
        saver.flush();
        let mut store = store;
        assert_eq!(store.load().snapshot.config.visit_gap_hours, 4.0);
    }
}
