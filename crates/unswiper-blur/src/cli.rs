// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! `unswiper-blur scan <folder|library.json> [--json] [--threshold N] [--sensitivity S] [--write-cache [FILE]]`
//!
//! Scores photos and lists the ones that would be suggested as blurry. Read-only for
//! photos: it never moves, deletes or changes them, and never edits library.json. With
//! `--write-cache` it saves scores where the Linux app picks them up.

use crate::cache::QualityCache;
use crate::decide::{Reason, Thresholds};
use crate::scan;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use unswiper_core::quality::Sensitivity;
use unswiper_core::{index, LibrarySnapshot, MediaItem};

pub const USAGE: &str = "usage:
  unswiper-blur scan <folder|library.json> [options]   score photos, list likely blurry ones
      --json                 machine-readable output
      --sensitivity S        low | normal (default) | high
      --threshold N          custom absolute sharpness floor (overrides the sensitivity floor)
      --write-cache [FILE]   save scores (default: the app's quality-cache.json next to
                             library.json, or ~/.local/share/unswiper for a folder)
      --only FILE            score only the photos listed in FILE (one path per line, e.g.
                             the open visit); results still cover every photo with a score
      --progress             print `progress DONE TOTAL` lines on stderr while scoring (at
                             most 10 a second); with --write-cache, scores are also saved
                             every 30 s so a stopped scan resumes where it was
  unswiper-blur info [options] FILE...      photo info rows as JSON (see `info` below)
  unswiper-blur geo < request.json          place names: {\"cmd\":\"geo\",\"points\":[[lat,lon],...]}
  unswiper-blur group --library library.json [--mode M]   groups by place (JSON, read-only)
  unswiper-blur --version
Nothing is moved or deleted.";

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub target: PathBuf,
    pub json: bool,
    pub thresholds: Thresholds,
    pub write_cache: Option<Option<PathBuf>>,
    /// `--only`: a file listing the paths to score (others are only read from the cache).
    pub only: Option<PathBuf>,
    /// `--progress`: progress lines on stderr, periodic cache saves.
    pub progress: bool,
}

pub fn parse(args: &[String]) -> Result<Options, String> {
    if args.first().map(String::as_str) != Some("scan") {
        return Err(USAGE.into());
    }
    let mut target = None;
    let mut json = false;
    let mut sens = Sensitivity::Normal;
    let mut abs = None;
    let mut write_cache = None;
    let mut only = None;
    let mut progress = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => json = true,
            "--progress" => progress = true,
            "--sensitivity" => {
                sens = args
                    .get(i + 1)
                    .and_then(|s| Sensitivity::parse(s))
                    .ok_or("--sensitivity needs low, normal or high")?;
                i += 1;
            }
            "--threshold" => {
                abs = Some(
                    args.get(i + 1)
                        .and_then(|s| s.parse::<f32>().ok())
                        .filter(|v| *v >= 0.0)
                        .ok_or("--threshold needs a number")?,
                );
                i += 1;
            }
            "--write-cache" => {
                let file = args
                    .get(i + 1)
                    .filter(|a| !a.starts_with("--"))
                    .map(PathBuf::from);
                if file.is_some() {
                    i += 1;
                }
                write_cache = Some(file);
            }
            "--only" => {
                only = Some(
                    args.get(i + 1)
                        .filter(|a| !a.starts_with("--"))
                        .map(PathBuf::from)
                        .ok_or("--only needs a file listing paths")?,
                );
                i += 1;
            }
            a if a.starts_with("--") => return Err(format!("unknown option {a}\n{USAGE}")),
            a if target.is_none() => target = Some(PathBuf::from(a)),
            a => return Err(format!("unexpected argument {a}\n{USAGE}")),
        }
        i += 1;
    }
    let mut thresholds = Thresholds::for_sensitivity(sens);
    if let Some(a) = abs {
        thresholds.abs = a;
    }
    Ok(Options {
        target: target.ok_or(USAGE)?,
        json,
        thresholds,
        write_cache,
        only,
        progress,
    })
}

/// Photos to scan and the default cache location for a target.
fn load_target(target: &Path) -> Result<(Vec<MediaItem>, PathBuf), String> {
    if target.is_dir() {
        let (items, _) = index::index(
            target,
            36.0,
            &Default::default(),
            &index::MetadataDates,
            |_| {},
        )
        .map_err(|e| format!("{}: {e}", target.display()))?;
        let dir = unswiper_core::store::default_support_dir();
        Ok((items, QualityCache::path_in(&dir)))
    } else {
        let data = std::fs::read(target).map_err(|e| format!("{}: {e}", target.display()))?;
        let snap: LibrarySnapshot =
            serde_json::from_slice(&data).map_err(|e| format!("{}: {e}", target.display()))?;
        let dir = target.parent().unwrap_or(Path::new("."));
        Ok((snap.items, QualityCache::path_in(dir)))
    }
}

/// Scores `todo` in batches, printing `progress DONE TOTAL` on stderr (at most 10 a second,
/// and once at the end) and saving the cache every 30 s when `save` is set.
fn score_with_progress(todo: &[String], cache: &mut QualityCache, save: Option<&Path>, now: i64) {
    use std::io::Write;
    use std::time::{Duration, Instant};
    let todo: Vec<String> = todo
        .iter()
        .filter(|p| {
            crate::cache::Stamp::of(Path::new(p)).is_some_and(|st| cache.get(p, st).is_none())
        })
        .cloned()
        .collect();
    let total = todo.len();
    let last = std::sync::Mutex::new(Instant::now() - Duration::from_secs(1));
    let say = |done: usize| {
        let mut l = last.lock().unwrap_or_else(|e| e.into_inner());
        if done == total || l.elapsed() >= Duration::from_millis(100) {
            *l = Instant::now();
            let mut e = std::io::stderr().lock();
            let _ = writeln!(e, "progress {done} {total}");
            let _ = e.flush();
        }
    };
    say(0);
    let mut done = 0;
    let mut saved = Instant::now();
    for batch in todo.chunks(128) {
        let base = done;
        scan::update_cache(
            batch,
            cache,
            scan::default_threads(),
            &AtomicBool::new(false),
            now,
            &|d, _| say(base + d),
        );
        done += batch.len();
        if let Some(p) = save {
            if saved.elapsed() >= Duration::from_secs(30) {
                let _ = cache.save(p);
                saved = Instant::now();
            }
        }
    }
    say(total);
}

/// Runs the command; returns the text to print.
pub fn run(o: &Options, now: i64) -> Result<String, String> {
    let (items, default_cache) = load_target(&o.target)?;
    let cache_path = match &o.write_cache {
        Some(Some(p)) => p.clone(),
        _ => default_cache,
    };
    let mut cache = QualityCache::load(&cache_path);
    let paths = scan::scorable(&items);
    let todo: Vec<String> = match &o.only {
        Some(f) => {
            let list = std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?;
            let wanted: std::collections::HashSet<&str> = list.lines().map(str::trim).collect();
            paths
                .iter()
                .filter(|p| wanted.contains(p.as_str()))
                .cloned()
                .collect()
        }
        None => paths.clone(),
    };
    if o.progress {
        score_with_progress(
            &todo,
            &mut cache,
            o.write_cache.is_some().then_some(&cache_path),
            now,
        );
    } else {
        scan::update_cache(
            &todo,
            &mut cache,
            scan::default_threads(),
            &AtomicBool::new(false),
            now,
            &|_, _| {},
        );
    }
    let s = scan::suggestions(&items, &cache, o.thresholds);
    if o.write_cache.is_some() {
        cache
            .save(&cache_path)
            .map_err(|e| format!("{}: {e}", cache_path.display()))?;
    }
    let mut rows: Vec<(&String, f32, Option<Reason>)> = paths
        .iter()
        .filter_map(|p| {
            let e = cache.entries.get(p)?;
            s.scored
                .contains(p)
                .then(|| (p, e.score.sharpness, s.flagged.get(p).copied()))
        })
        .collect();
    rows.sort_by(|a, b| a.1.total_cmp(&b.1));
    if o.json {
        let v: Vec<serde_json::Value> = rows
            .iter()
            .map(|(p, _, r)| {
                let e = &cache.entries[*p];
                serde_json::json!({
                    "path": p, "score": e.score, "blurry": r.is_some(), "reason": r,
                })
            })
            .collect();
        return Ok(serde_json::to_string_pretty(&v).unwrap_or_default());
    }
    let mut out = String::new();
    for (p, sharp, r) in &rows {
        let tag = match r {
            Some(Reason::Soft) => "BLURRY",
            Some(Reason::Burst) => "BLURRY (burst)",
            Some(Reason::Shake) => "BLURRY (shake)",
            None => "ok",
        };
        out.push_str(&format!("{sharp:>10.1}  {tag:<14}  {p}\n"));
    }
    out.push_str(&format!(
        "{} photos scored, {} likely blurry, {} skipped (videos, screenshots, unreadable). Nothing was moved.\n",
        rows.len(),
        s.flagged.len(),
        items.len() - rows.len()
    ));
    if o.write_cache.is_some() {
        out.push_str(&format!("Scores saved to {}\n", cache_path.display()));
    }
    Ok(out)
}

/// Entry point used by the binary. Returns (exit code, stdout, stderr).
pub fn main_with(args: &[String], now: i64) -> (u8, String, String) {
    if args.first().map(String::as_str) == Some("--version") {
        return (
            0,
            format!("unswiper-blur {}\n", env!("CARGO_PKG_VERSION")),
            String::new(),
        );
    }
    if args.first().map(String::as_str) == Some("info") {
        return match crate::info::parse(args).and_then(|o| crate::info::run(&o)) {
            Ok(s) => (0, s, String::new()),
            Err(e) => (2, String::new(), format!("{e}\n{}\n", crate::info::USAGE)),
        };
    }
    match parse(args).and_then(|o| run(&o, now)) {
        Ok(s) => (0, s, String::new()),
        Err(e) => (2, String::new(), format!("{e}\n")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_options() {
        let o = parse(&a(&[
            "scan",
            "/x",
            "--json",
            "--sensitivity",
            "high",
            "--threshold",
            "4",
            "--write-cache",
        ]))
        .unwrap();
        assert!(o.json);
        assert_eq!(o.thresholds.abs, 4.0);
        assert_eq!(
            o.thresholds.rel,
            Thresholds::for_sensitivity(Sensitivity::High).rel
        );
        assert_eq!(o.write_cache, Some(None));
        let o = parse(&a(&["scan", "--write-cache", "/c.json", "/x"])).unwrap();
        assert_eq!(o.write_cache, Some(Some(PathBuf::from("/c.json"))));
        assert_eq!(o.only, None);
        assert!(!o.progress);
        assert!(parse(&a(&["scan", "/x", "--progress"])).unwrap().progress);
        let o = parse(&a(&["scan", "/x", "--only", "/list.txt"])).unwrap();
        assert_eq!(o.only, Some(PathBuf::from("/list.txt")));
        assert_eq!(o.target, PathBuf::from("/x"));
        for bad in [
            &["nope"][..],
            &["scan"],
            &["scan", "/x", "--sensitivity", "max"],
            &["scan", "/x", "--threshold", "-1"],
            &["scan", "/x", "--bogus"],
            &["scan", "/x", "/y"],
            &["scan", "/x", "--only"],
            &["scan", "/x", "--only", "--json"],
        ] {
            assert!(parse(&a(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn scan_folder_and_library() {
        let d = tempfile::tempdir().unwrap();
        let photos = d.path().join("photos");
        std::fs::create_dir(&photos).unwrap();
        let s = synth::scene(4, 500, 400);
        s.to_rgb8().save(photos.join("IMG_1.jpg")).unwrap();
        synth::defocus(&s, 6.0)
            .to_rgb8()
            .save(photos.join("IMG_2.jpg"))
            .unwrap();
        let cache = d.path().join("c.json");
        let (code, out, err) = main_with(
            &a(&[
                "scan",
                photos.to_str().unwrap(),
                "--write-cache",
                cache.to_str().unwrap(),
            ]),
            1,
        );
        assert_eq!(code, 0, "{err}");
        assert!(out.contains("2 photos scored, 1 likely blurry"), "{out}");
        assert!(out.contains("BLURRY") && out.contains("Scores saved"));
        assert_eq!(QualityCache::load(&cache).entries.len(), 2);
        // Library mode, JSON, default cache next to library.json.
        let (items, _) = index::index(
            &photos,
            36.0,
            &Default::default(),
            &index::MetadataDates,
            |_| {},
        )
        .unwrap();
        let mut snap = LibrarySnapshot::empty();
        snap.items = items;
        let lib = d.path().join("library.json");
        std::fs::write(&lib, snap.to_json()).unwrap();
        let (code, out, _) = main_with(
            &a(&["scan", lib.to_str().unwrap(), "--json", "--write-cache"]),
            2,
        );
        assert_eq!(code, 0);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 2);
        assert_eq!(v[0]["blurry"], true);
        assert_eq!(v[0]["reason"], "soft");
        assert!(d.path().join("quality-cache.json").exists());
        // --only scores just the listed photos (fresh cache); unlisted ones get no row.
        let only = d.path().join("only.txt");
        let first = v
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["blurry"] == false)
            .unwrap()["path"]
            .as_str()
            .unwrap()
            .to_string();
        std::fs::write(&only, format!("{first}\n")).unwrap();
        let fresh = d.path().join("fresh.json");
        let (code, out, err) = main_with(
            &a(&[
                "scan",
                lib.to_str().unwrap(),
                "--json",
                "--only",
                only.to_str().unwrap(),
                "--write-cache",
                fresh.to_str().unwrap(),
            ]),
            3,
        );
        assert_eq!(code, 0, "{err}");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 1);
        assert_eq!(v[0]["path"], first.as_str());
        assert_eq!(QualityCache::load(&fresh).entries.len(), 1);
        let (code, _, err) = main_with(
            &a(&["scan", lib.to_str().unwrap(), "--only", "/no/such/list"]),
            3,
        );
        assert_eq!(code, 2);
        assert!(err.contains("/no/such/list"));
        // Library unchanged.
        assert_eq!(std::fs::read_to_string(&lib).unwrap(), snap.to_json());
        // Errors.
        let (code, _, err) = main_with(&a(&["scan", "/definitely/missing.json"]), 0);
        assert_eq!(code, 2);
        assert!(err.contains("missing.json"));
        std::fs::write(d.path().join("bad.json"), b"{").unwrap();
        assert_eq!(
            main_with(
                &a(&["scan", d.path().join("bad.json").to_str().unwrap()]),
                0
            )
            .0,
            2
        );
        let (code, out, _) = main_with(&a(&["--version"]), 0);
        assert_eq!(code, 0);
        assert!(out.starts_with("unswiper-blur "));
    }
}
