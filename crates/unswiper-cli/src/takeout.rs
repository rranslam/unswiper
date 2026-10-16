// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! `unswiper-cli takeout import`: unpack Google Takeout archives into a folder (plan §19).

use std::path::{Path, PathBuf};
use std::time::Instant;
use unswiper_takeout::{find_archives, find_parts, plan, unpack_with, Plan, UnpackOptions};

pub const USAGE: &str = "  unswiper-cli takeout import <folder|archive>... --to <dest> [options]
                                                          unpack Google Takeout .zip/.tgz archives
                                                          (a folder means every archive in it; an
                                                          archive brings its other parts along)
      --dry-run          only report sizes, item counts and the space needed
      --exact            measure .tgz archives exactly in a dry run (reads them completely)
      --delete-archives  delete each archive after it unpacked without conflicts
      --keep-duplicates  also unpack album copies of photos already imported elsewhere
      --no-space-check   start even if the free space looks too small";

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

/// Archives named on the command line: folders expand to every archive in them, archives to all
/// parts of their export. Sorted, without repeats.
fn collect(inputs: &[String]) -> Result<Vec<PathBuf>, String> {
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

fn print_plan(p: &Plan, opts: &UnpackOptions) {
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
        println!("{}  {}  {what}", a.path.display(), human(a.archive_bytes));
    }
    println!(
        "total: {} archives, {} in archives, {} photos and videos ({}), {} new ({})",
        p.archives.len(),
        human(p.archive_bytes),
        p.items,
        human(p.photos_bytes),
        p.new_items,
        human(p.new_bytes)
    );
    println!(
        "space needed: {} keeping the archives, {} deleting each after unpacking{}",
        human(p.needed_keep),
        human(p.needed_delete),
        if p.same_disk {
            ""
        } else {
            " (archives are on another disk, so deleting them frees no space here)"
        }
    );
    match p.free_bytes {
        Some(f) => println!(
            "free: {}; with the options given this {} (needs {} incl. reserve)",
            human(f),
            if p.fits(opts) { "fits" } else { "does NOT fit" },
            human(p.needed(opts))
        ),
        None => println!("free space unknown"),
    }
}

pub fn run(args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) != Some("import") {
        return Err(format!("usage:\n{USAGE}"));
    }
    let mut opts = UnpackOptions::default();
    let (mut dry, mut exact, mut dest, mut inputs) = (false, false, None, vec![]);
    let mut i = 1;
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
            o if o.starts_with("--") => return Err(format!("unknown option {o}\n{USAGE}")),
            _ => inputs.push(args[i].clone()),
        }
        i += 1;
    }
    let dest = dest.ok_or(format!("--to <dest> is required\n{USAGE}"))?;
    if inputs.is_empty() {
        return Err(format!("no archives given\n{USAGE}"));
    }
    let archives = collect(&inputs)?;
    if dry {
        let p = plan(&archives, &dest, &opts, exact).map_err(|e| e.to_string())?;
        print_plan(&p, &opts);
        return Ok(());
    }
    let start = Instant::now();
    let r = unpack_with(&archives, &dest, &opts, |t| {
        let secs = start.elapsed().as_secs_f64().max(0.001);
        let rate = t.bytes as f64 / secs;
        let left = if t.bytes_total > t.bytes && rate > 0.0 {
            format!(
                ", about {:.0} min left",
                (t.bytes_total - t.bytes) as f64 / rate / 60.0
            )
        } else {
            String::new()
        };
        eprintln!(
            "archive {}/{}: {} files, {} of ~{} ({}/s{left})",
            t.archive,
            t.archives,
            t.files,
            human(t.bytes),
            human(t.bytes_total),
            human(rate as u64)
        );
    })
    .map_err(|e| e.to_string())?;
    println!(
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
        println!("deleted {}", a.display());
    }
    if !r.other_products.is_empty() {
        let v: Vec<_> = r.other_products.iter().cloned().collect();
        println!("skipped other Google products: {}", v.join(", "));
    }
    for c in &r.refused {
        println!("refused unsafe entry: {c}");
    }
    for c in &r.conflicts {
        println!("not overwritten (a different file has this name): {c}");
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
