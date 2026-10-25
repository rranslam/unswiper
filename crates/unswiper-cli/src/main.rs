// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! `unswiper-cli`: index | groups | commit --dry-run | restore (plan §5) | takeout import (§19) |
//! grouping modes, `config set group-mode` and `geo` (§20.7).

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;
use unswiper_core::geo::{group_with, GroupSettings};
use unswiper_core::{commit, index, store, AppConfig, CommitMode, GroupMode, LibrarySnapshot};
use unswiper_geo::Geocoder;

const USAGE: &str = "usage:
  unswiper-cli index <folder> [--gap-hours N] [--takeout] [--json] [grouping options]
                                                          index a folder and list visits; --takeout
                                                          also reads Google's JSON sidecars
  unswiper-cli groups <library.json> [--json] [grouping options]
                                                          list visits in a saved library (its saved
                                                          grouping mode unless overridden)
  grouping options (override the saved setting for this run only; never saved):
      --group-mode visit|geo|visit-geo   visit (default): time visits as in v0.5; geo: by place;
                                         visit-geo: each visit split by place
      --place-radius-km N                place radius (default 1 for geo, 2 for visit-geo)
      --min-place-items N                items that make a place (default 3)
      --join-same-place                  visit-geo: join visits at one place under 36 h apart
  unswiper-cli config set group-mode visit|geo|visit-geo <library.json>
                                                          save the grouping mode in a library
  unswiper-cli config get group-mode <library.json>       print the saved grouping mode
  unswiper-cli geo <lat> <lon>                            offline place name for a point (debug)
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

/// Grouping flags given on the command line (all optional).
#[derive(Default)]
struct GroupFlags {
    mode: Option<GroupMode>,
    radius_km: Option<f64>,
    min_items: Option<usize>,
    join_same_place: bool,
}

impl GroupFlags {
    /// Consumes a grouping flag at `args[*i]`; `Ok(false)` if it isn't one.
    fn take(&mut self, args: &[String], i: &mut usize) -> Result<bool, String> {
        let val = |i: usize| args.get(i + 1).map(String::as_str);
        match args[*i].as_str() {
            "--group-mode" => {
                self.mode = Some(
                    val(*i)
                        .and_then(GroupMode::parse)
                        .ok_or("--group-mode needs visit, geo or visit-geo")?,
                );
                *i += 1;
            }
            "--place-radius-km" => {
                self.radius_km = Some(
                    val(*i)
                        .and_then(|s| s.parse::<f64>().ok())
                        .filter(|r| r.is_finite() && *r > 0.0)
                        .ok_or("--place-radius-km needs a number above 0")?,
                );
                *i += 1;
            }
            "--min-place-items" => {
                self.min_items = Some(
                    val(*i)
                        .and_then(|s| s.parse::<usize>().ok())
                        .filter(|n| *n >= 1)
                        .ok_or("--min-place-items needs a whole number of at least 1")?,
                );
                *i += 1;
            }
            "--join-same-place" => self.join_same_place = true,
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// The saved settings with these flags on top (for this run only).
    fn settings(&self, saved: &AppConfig) -> GroupSettings {
        let mut s = GroupSettings::from_config(saved);
        if let Some(m) = self.mode {
            if m != s.mode && saved.place_radius_km.is_none() {
                s.radius_km = m.default_radius_km();
            }
            s.mode = m;
        }
        if s.mode == GroupMode::Visit
            && (self.radius_km.is_some() || self.min_items.is_some() || self.join_same_place)
        {
            eprintln!("note: place options are ignored in visit mode");
        } else if self.join_same_place && s.mode != GroupMode::VisitGeo {
            eprintln!("note: --join-same-place only applies to visit-geo");
        }
        if let Some(r) = self.radius_km {
            s.radius_km = r;
        }
        if let Some(n) = self.min_items {
            s.min_items = n;
        }
        if self.join_same_place {
            s.join_same_place = true;
        }
        s
    }
}

fn groups_json(groups: &[unswiper_core::VisitGroup]) -> String {
    unswiper_core::apple_json::to_string(&serde_json::to_value(groups).expect("groups serialise"))
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
            let mut flags = GroupFlags::default();
            let mut i = 2;
            while i < args.len() {
                if flags.take(args, &mut i)? {
                    i += 1;
                    continue;
                }
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
            let (items, _) = index::index(Path::new(root), gap, &HashMap::new(), dates, |_| {})
                .map_err(|e| e.to_string())?;
            let cfg = AppConfig {
                visit_gap_hours: gap,
                ..Default::default()
            };
            let groups = group_with(&items, &flags.settings(&cfg), &Geocoder);
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
            let mut flags = GroupFlags::default();
            let mut json = false;
            let mut i = 2;
            while i < args.len() {
                if !flags.take(args, &mut i)? {
                    match args[i].as_str() {
                        "--json" => json = true,
                        other => return Err(format!("unknown option {other}\n{USAGE}")),
                    }
                }
                i += 1;
            }
            let groups = group_with(&snap.items, &flags.settings(&snap.config), &Geocoder);
            if json {
                println!("{}", groups_json(&groups));
            } else {
                print_groups(&groups);
            }
            Ok(())
        }
        Some("config") => {
            let (verb, key) = (
                args.get(1).map(String::as_str),
                args.get(2).map(String::as_str),
            );
            match (verb, key) {
                (Some("get"), Some("group-mode")) => {
                    let snap = load(args.get(3).ok_or(USAGE)?)?;
                    println!("{}", snap.config.group_mode().as_str());
                    Ok(())
                }
                (Some("set"), Some("group-mode")) => {
                    let mode = args
                        .get(3)
                        .and_then(|m| GroupMode::parse(m))
                        .ok_or("group-mode must be visit, geo or visit-geo")?;
                    let path = args.get(4).ok_or(USAGE)?;
                    let mut snap = load(path)?;
                    snap.config.group_mode = Some(mode.as_str().into());
                    snap.schema_version = Some(LibrarySnapshot::CURRENT_SCHEMA_VERSION);
                    store::write_atomic(Path::new(path), snap.to_json().as_bytes())
                        .map_err(|e| format!("{path}: {e}"))?;
                    println!("group-mode = {}", mode.as_str());
                    Ok(())
                }
                _ => Err(USAGE.into()),
            }
        }
        Some("geo") => {
            let num = |k: usize| args.get(k).and_then(|s| s.parse::<f64>().ok());
            let (Some(lat), Some(lon)) = (num(1), num(2)) else {
                return Err(format!("geo needs <lat> <lon>\n{USAGE}"));
            };
            match Geocoder.lookup(lat, lon) {
                Some(p) => println!("{}", p.label()),
                None => println!("no place within {} km", unswiper_geo::MAX_DISTANCE_KM),
            }
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
                    let settings = GroupSettings::from_config(&snap.config);
                    let groups = group_with(&snap.items, &settings, &Geocoder);
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
        Some("takeout") => {
            let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
            unswiper_takeout::cli::run(&args[1..], &mut out, &mut err)
        }
        _ => Err(format!("{USAGE}{}", unswiper_takeout::cli::USAGE)),
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
