// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! The `takeout` command line, shared by `unswiper-cli takeout …` (Linux, scripting) and the Mac
//! app's bundled helper `unswiper-blur takeout …`.
//!
//! ```text
//! takeout import <folder|archive>... --to <library> [--dry-run] [--exact] [--delete-archives]
//!                [--keep-duplicates] [--no-space-check] [--json | --progress-json]
//! takeout dates <library>          Google's dates and places as JSON (for the Mac indexer)
//! ```
//!
//! Machine-readable output (one JSON object per line, for the Mac app):
//! - `--dry-run --json`: one `{"event":"plan",…}` line.
//! - `--progress-json`: a `plan` line, then `progress` lines, then one `done` line, or an
//!   `error` line (with a `kind` such as `not_enough_space` or `cancelled`) and exit code 1.
//!
//! ```text
//! {"event":"plan","plan":{…Plan…},"fits":true,"needed":123,"lines":["…in words…"]}
//! {"event":"progress","archive":1,"archives":3,"files":25,"bytes":1000,"bytes_total":5000}
//! {"event":"done","report":{…UnpackReport…},"lines":["…in words…"]}
//! {"event":"error","kind":"not_enough_space","message":"…"}
//! ```

use crate::{
    find_archives, find_parts, library_dates, plan, unpack_with, Plan, UnpackError, UnpackOptions,
    UnpackReport,
};
use serde_json::json;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub const USAGE: &str = "  unswiper-cli takeout import <folder|archive>... --to <library> [options]
                                                          unpack Google Takeout .zip/.tgz archives
                                                          (a folder means every archive in it; an
                                                          archive brings its other parts along)
      --dry-run          only report sizes, item counts, space and time needed
      --exact            measure .tgz archives exactly in a dry run (reads them completely)
      --delete-archives  delete each archive after it unpacked without conflicts
      --keep-duplicates  also unpack album copies of photos already imported elsewhere
      --no-space-check   start even if the free space looks too small
      --json             dry run as JSON;  --progress-json  progress as JSON lines
  unswiper-cli takeout dates <library>                    Google's dates and places as JSON
";

/// 1.5 GB style sizes.
pub fn human(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1e9 {
        format!("{:.1} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else {
        format!("{bytes} bytes")
    }
}

/// "1 file", "3 files".
pub fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// "about 3 min", "about 2 h 10 min".
pub fn duration(secs: u64) -> String {
    if secs < 60 {
        "under a minute".into()
    } else if secs < 3600 {
        format!("about {} min", secs.div_ceil(60))
    } else {
        let m = secs.div_ceil(60);
        format!("about {} h {} min", m / 60, m % 60)
    }
}

/// Archives named on the command line: folders expand to every archive in them, archives to all
/// parts of their export. Sorted, without repeats.
pub fn collect(inputs: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut out = vec![];
    for i in inputs {
        let p = Path::new(i);
        if p.is_dir() {
            out.extend(find_archives(p).map_err(|e| e.to_string())?);
        } else if p.is_file() {
            out.extend(find_parts(p));
        } else {
            return Err(format!("{i}: no such file or folder"));
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// The dry-run figures in plain words (the apps show the same lines).
pub fn plan_lines(p: &Plan, opts: &UnpackOptions) -> Vec<String> {
    let mut v = vec![format!(
        "{} {} ({}): {} photos and videos, {} new ({}).",
        p.archives.len(),
        if p.archives.len() == 1 {
            "archive"
        } else {
            "archives"
        },
        human(p.archive_bytes),
        p.items,
        p.new_items,
        human(p.new_bytes)
    )];
    if p.archives.iter().any(|a| !a.exact) {
        v.push(
            ".tgz archives are estimated from their size; the counts cover the .zip archives only."
                .into(),
        );
    }
    let done = p.archives.iter().filter(|a| a.done_earlier).count();
    if done > 0 {
        v.push(format!(
            "{done} of them were imported earlier and are skipped."
        ));
    }
    v.push(format!(
        "Space needed: {} keeping the archives, {} deleting each one after unpacking.{}",
        human(p.needed_keep),
        human(p.needed_delete),
        if p.same_disk || p.archives.is_empty() {
            ""
        } else {
            " (The archives are on another disk, so deleting them frees no space here.)"
        }
    ));
    match p.free_bytes {
        Some(f) if p.fits(opts) => v.push(format!(
            "Free on the library disk: {}. It fits (with a {} reserve).",
            human(f),
            human(opts.reserve_bytes)
        )),
        Some(f) => v.push(format!(
            "Free on the library disk: {}. NOT enough: {} more is needed.",
            human(f),
            human(p.needed(opts).saturating_sub(f))
        )),
        None => v.push("Free space on the library disk is unknown.".into()),
    }
    v.push(if p.seconds_max == 0 {
        "Time: nothing to unpack.".into()
    } else if p.seconds_max < 60 {
        "Time: under a minute.".into()
    } else {
        format!(
            "Time: {} on a fast disk (SSD), up to {} on a hard disk.",
            duration(p.seconds_min),
            duration(p.seconds_max)
        )
    });
    v
}

/// What the unpack did, in words.
pub fn report_lines(r: &UnpackReport) -> Vec<String> {
    let mut v = vec![format!(
        "Unpacked {} ({}).",
        count(r.extracted, "file", "files"),
        human(r.bytes_written)
    )];
    let skipped = r.already_there + r.previously_imported;
    if skipped > 0 {
        v.push(format!(
            "{} already imported before.",
            count(skipped, "file was", "files were")
        ));
    }
    if r.duplicates_skipped > 0 {
        v.push(format!(
            "{} skipped (the same photo is in its year folder).",
            count(r.duplicates_skipped, "album copy was", "album copies were")
        ));
    }
    if r.archives_skipped > 0 {
        v.push(format!(
            "{} finished earlier and skipped.",
            count(r.archives_skipped, "archive was", "archives were")
        ));
    }
    if !r.archives_deleted.is_empty() {
        v.push(format!(
            "Deleted {}.",
            count(r.archives_deleted.len(), "archive", "archives")
        ));
    }
    if !r.other_products.is_empty() {
        let p: Vec<_> = r.other_products.iter().cloned().collect();
        v.push(format!("Skipped other Google products: {}.", p.join(", ")));
    }
    if !r.conflicts.is_empty() {
        v.push(format!(
            "{} files were not unpacked because a different file with the same name is already in the library (those archives were kept): {}",
            r.conflicts.len(),
            r.conflicts.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    if !r.refused.is_empty() {
        v.push(format!(
            "{} unsafe entries in the archives were refused.",
            r.refused.len()
        ));
    }
    v
}

fn plan_event(p: &Plan, opts: &UnpackOptions) -> serde_json::Value {
    json!({
        "event": "plan",
        "plan": p,
        "fits": p.fits(opts),
        "needed": p.needed(opts),
        "lines": plan_lines(p, opts),
    })
}

fn line(out: &mut dyn Write, v: &serde_json::Value) {
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

/// Runs `takeout <args>`; human output on `out`, progress on `err` (JSON modes: all on `out`).
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("import") => import(&args[1..], out, err),
        Some("dates") => {
            let lib = args.get(1).ok_or(format!("usage:\n{USAGE}"))?;
            let v = library_dates(Path::new(lib));
            let _ = writeln!(out, "{}", serde_json::to_string(&v).unwrap_or_default());
            Ok(())
        }
        _ => Err(format!("usage:\n{USAGE}")),
    }
}

fn import(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> Result<(), String> {
    let mut opts = UnpackOptions::default();
    let (mut dry, mut exact, mut json, mut progress_json) = (false, false, false, false);
    let (mut dest, mut inputs) = (None, vec![]);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--to" => {
                dest = Some(PathBuf::from(args.get(i + 1).ok_or("--to needs a folder")?));
                i += 1;
            }
            "--dry-run" => dry = true,
            "--exact" => exact = true,
            "--delete-archives" => opts.delete_archives = true,
            "--keep-duplicates" => opts.skip_duplicates = false,
            "--no-space-check" => opts.check_space = false,
            "--json" => json = true,
            "--progress-json" => progress_json = true,
            o if o.starts_with("--") => return Err(format!("unknown option {o}\n{USAGE}")),
            _ => inputs.push(args[i].clone()),
        }
        i += 1;
    }
    let fail = |out: &mut dyn Write, kind: &str, msg: String| -> Result<(), String> {
        if progress_json || json {
            line(
                out,
                &json!({"event": "error", "kind": kind, "message": msg}),
            );
        }
        Err(msg)
    };
    let Some(dest) = dest else {
        return fail(out, "usage", format!("--to <library> is required\n{USAGE}"));
    };
    if inputs.is_empty() {
        return fail(out, "usage", format!("no archives given\n{USAGE}"));
    }
    let archives = match collect(&inputs) {
        Ok(a) => a,
        Err(e) => return fail(out, "no_archives", e),
    };
    let p = match plan(&archives, &dest, &opts, exact && dry) {
        Ok(p) => p,
        Err(e) => return fail(out, e.kind(), e.to_string()),
    };
    if dry {
        if json {
            line(out, &plan_event(&p, &opts));
        } else {
            for a in &p.archives {
                let what = if a.done_earlier {
                    "imported earlier".to_string()
                } else if a.exact {
                    format!("{} items, {} new", a.items, human(a.new_bytes))
                } else {
                    format!(
                        "about {} new (estimated; --exact to count)",
                        human(a.new_bytes)
                    )
                };
                let _ = writeln!(
                    out,
                    "{}  {}  {what}",
                    a.path.display(),
                    human(a.archive_bytes)
                );
            }
            for l in plan_lines(&p, &opts) {
                let _ = writeln!(out, "{l}");
            }
        }
        return Ok(());
    }
    if progress_json {
        line(out, &plan_event(&p, &opts));
    }
    let start = Instant::now();
    let result = {
        let mut on_progress = |t: crate::UnpackProgress| {
            if progress_json {
                let mut v = serde_json::to_value(t).unwrap_or_default();
                v["event"] = "progress".into();
                line(out, &v);
            } else {
                let secs = start.elapsed().as_secs_f64().max(0.001);
                let rate = t.bytes as f64 / secs;
                let left = if t.bytes_total > t.bytes && rate > 0.0 {
                    format!(
                        ", {} left",
                        duration(((t.bytes_total - t.bytes) as f64 / rate) as u64)
                    )
                } else {
                    String::new()
                };
                let _ = writeln!(
                    err,
                    "archive {}/{}: {} files, {} of ~{} ({}/s{left})",
                    t.archive,
                    t.archives,
                    t.files,
                    human(t.bytes),
                    human(t.bytes_total),
                    human(rate as u64)
                );
            }
        };
        unpack_with(&archives, &dest, &opts, &mut on_progress)
    };
    let r = match result {
        Ok(r) => r,
        Err(e @ UnpackError::NotEnoughSpace { .. }) => {
            let msg = format!("{e}\n{}", plan_lines(&p, &opts).join("\n"));
            return fail(out, e.kind(), msg);
        }
        Err(e) => return fail(out, e.kind(), e.to_string()),
    };
    if progress_json {
        line(
            out,
            &json!({"event": "done", "report": r, "lines": report_lines(&r)}),
        );
        return Ok(());
    }
    let _ = writeln!(
        out,
        "unpacked {} files ({}) into {} in {:.0} s; {} already there, {} imported before, {} duplicates skipped, {} archives finished earlier",
        r.extracted,
        human(r.bytes_written),
        dest.display(),
        start.elapsed().as_secs_f64(),
        r.already_there,
        r.previously_imported,
        r.duplicates_skipped,
        r.archives_skipped
    );
    for a in &r.archives_deleted {
        let _ = writeln!(out, "deleted {}", a.display());
    }
    if !r.other_products.is_empty() {
        let v: Vec<_> = r.other_products.iter().cloned().collect();
        let _ = writeln!(out, "skipped other Google products: {}", v.join(", "));
    }
    for c in &r.refused {
        let _ = writeln!(out, "refused unsafe entry: {c}");
    }
    for c in &r.conflicts {
        let _ = writeln!(out, "not overwritten (a different file has this name): {c}");
    }
    if r.conflicts.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} conflicts; those archives were kept",
            r.conflicts.len()
        ))
    }
}
