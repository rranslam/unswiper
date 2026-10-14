// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! `unswiper-cli`: index | groups | commit --dry-run | restore (plan §5).

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;
use unswiper_core::{commit, group, index, store, CommitMode, LibrarySnapshot};

const USAGE: &str = "usage:
  unswiper-cli index <folder> [--gap-hours N] [--json]   index a folder and list visits
  unswiper-cli groups <library.json>                      list visits in a saved library
  unswiper-cli commit --dry-run <library.json>            what a commit would move (never moves)
  unswiper-cli restore <manifest.json>                    move files from a commit back
  unswiper-cli --version";

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
                    other => return Err(format!("unknown option {other}\n{USAGE}")),
                }
                i += 1;
            }
            let (items, groups) = index::index(
                Path::new(root),
                gap,
                &HashMap::new(),
                &index::MetadataDates,
                |_| {},
            )
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
            let snap = load(args.get(2).ok_or(USAGE)?)?;
            let st = store::LibraryStore::new(std::env::temp_dir());
            let root = snap.config.root_path.as_deref().map(Path::new);
            let trash = unswiper_core::trash::FreedesktopTrash::home();
            let r = commit::commit_rejects(&snap.items, CommitMode::DryRun, root, &st, &trash)
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
        _ => Err(USAGE.into()),
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
