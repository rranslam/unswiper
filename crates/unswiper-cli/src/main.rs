// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! `unswiper-cli`: index | groups | commit --dry-run | restore (plan §5) | takeout import (§19).

mod takeout;

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;
use unswiper_core::{commit, group, index, store, CommitMode, LibrarySnapshot};

const USAGE: &str = "usage:
  unswiper-cli index <folder> [--gap-hours N] [--takeout] [--json]
                                                          index a folder and list visits; --takeout
                                                          also reads Google's JSON sidecars
  unswiper-cli groups <library.json>                      list visits in a saved library
  unswiper-cli commit --dry-run [--visit ID] <library.json>  what a commit would move (never moves);
                                                          --visit limits it to one visit, like the apps
  unswiper-cli restore <manifest.json>                    move files from a commit back
  unswiper-cli --version
";

fn print_groups(groups: &[unswiper_core::VisitGroup]) {
    for g in groups {
        println!(
            "{:<24} {:>5} items {:>12} bytes  {}",
            g.id,
            g.item_count(),
            g.total_bytes,
            g.title
        );
    }
}

fn load(path: &str) -> Result<LibrarySnapshot, String> {
    let data = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    serde_json::from_slice(&data).map_err(|e| format!("{path}: {e}"))
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("--version") => {
            println!("unswiper-cli {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("index") => {
            let root = args.get(1).ok_or(USAGE)?;
            let mut gap = 36.0;
            let mut json = false;
            let mut takeout = false;
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--gap-hours" => {
                        gap = args
                            .get(i + 1)
                            .and_then(|s| s.parse().ok())
                            .ok_or("--gap-hours needs a number")?;
                        i += 1;
                    }
                    "--json" => json = true,
                    "--takeout" => takeout = true,
                    other => return Err(format!("unknown option {other}\n{USAGE}")),
                }
                i += 1;
            }
            let takeout_dates = unswiper_takeout::TakeoutDates::new(index::MetadataDates);
            let dates: &dyn index::DateReader = if takeout {
                &takeout_dates
            } else {
                &index::MetadataDates
            };
            let (items, groups) =
                index::index(Path::new(root), gap, &HashMap::new(), dates, |_| {})
                    .map_err(|e| e.to_string())?;
            if json {
                let mut snap = LibrarySnapshot::empty();
                snap.config.root_path = Some(root.clone());
                snap.config.visit_gap_hours = gap;
                snap.items = items;
                snap.groups = groups;
                println!("{}", snap.to_json());
            } else {
                println!("{} items in {} visits", items.len(), groups.len());
                print_groups(&groups);
            }
            Ok(())
        }
        Some("groups") => {
            let snap = load(args.get(1).ok_or(USAGE)?)?;
            let groups = group::group(&snap.items, snap.config.visit_gap_hours);
            print_groups(&groups);
            Ok(())
        }
        Some("commit") => {
            if args.get(1).map(String::as_str) != Some("--dry-run") {
                return Err("only `commit --dry-run` is supported by the CLI".into());
            }
            let (visit, lib) = match args.get(2).map(String::as_str) {
                Some("--visit") => (Some(args.get(3).ok_or(USAGE)?), args.get(4)),
                _ => (None, args.get(2)),
            };
            let snap = load(lib.ok_or(USAGE)?)?;
            let items = match visit {
                None => snap.items.clone(),
                Some(id) => {
                    let groups = group::group(&snap.items, snap.config.visit_gap_hours);
                    let g = groups
                        .iter()
                        .find(|g| &g.id == id)
                        .ok_or_else(|| format!("no visit with id {id} (see `groups`)"))?;
                    commit::visit_members(&snap.items, g)
                }
            };
            let st = store::LibraryStore::new(std::env::temp_dir());
            let root = snap.config.root_path.as_deref().map(Path::new);
            let trash = unswiper_core::trash::FreedesktopTrash::home();
            let r = commit::commit_rejects(&items, CommitMode::DryRun, root, &st, &trash)
                .map_err(|e| e.to_string())?;
            println!("would move {} items ({} bytes)", r.would_move, r.bytes);
            Ok(())
        }
        Some("restore") => {
            let path = args.get(1).ok_or(USAGE)?;
            let m = store::read_manifest(Path::new(path)).map_err(|e| e.to_string())?;
            let r = commit::restore(&m);
            println!(
                "restored {}, already in place {}, failed {}",
                r.restored,
                r.already_in_place,
                r.failed.len()
            );
            for (p, e) in &r.failed {
                eprintln!("  {p}: {e}");
            }
            if r.failed.is_empty() {
                Ok(())
            } else {
                Err("some files were not restored".into())
            }
        }
        Some("takeout") => takeout::run(&args[1..]),
        _ => Err(format!("{USAGE}{}", takeout::USAGE)),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
