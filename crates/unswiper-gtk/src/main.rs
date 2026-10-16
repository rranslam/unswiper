// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! `unswiper` — the Linux app.

use std::path::PathBuf;
use unswiper_gtk::ui::{self, Options};

const USAGE: &str =
    "Usage: unswiper [--library-dir DIR] [--open FOLDER] [--open-first-visit] [--quit-after SECS]
       [--present-dialog about|commit] [--present-viewer] [--present-info]
       unswiper --version | --help-selftest";

fn main() {
    let mut opts = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--version" => {
                println!("unswiper {}", unswiper_gtk::VERSION);
                return;
            }
            "--help-selftest" => match unswiper_gtk::help::selftest() {
                Ok(n) => {
                    println!("help: {n} pages ok");
                    return;
                }
                Err(e) => {
                    eprintln!("help: {e}");
                    std::process::exit(1);
                }
            },
            "--library-dir" => opts.support_dir = args.next().map(PathBuf::from),
            "--open" => opts.open = args.next().map(PathBuf::from),
            "--open-first-visit" => opts.open_first_visit = true,
            "--quit-after" => opts.quit_after = args.next().and_then(|s| s.parse().ok()),
            "--present-dialog" => opts.present_dialog = args.next(),
            "--present-viewer" => opts.present_viewer = true,
            "--present-info" => opts.present_info = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            _ => {
                eprintln!("unknown argument: {a}\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    ui::run(opts);
}
