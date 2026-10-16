// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Safe, resumable unpacking of Google Takeout archives (`.zip`, `.tgz`/`.tar.gz`, any number of
//! parts) into a folder the user chose.
//!
//! - Only the Google Photos part is unpacked: `Takeout/Google Photos/<folder>/<file>` becomes
//!   `<dest>/<folder>/<file>`. Other products are listed in the report and skipped.
//! - Google's file names are kept exactly, so JSON sidecars still match their photos.
//! - Unsafe entries (absolute paths, `..`, drive letters), symlinks and hard links are refused and
//!   listed; nothing is ever written outside `dest`, and existing symlinks inside `dest` are not
//!   followed.
//! - Each file is written to `<name>.unswiper-part` and renamed when complete, so a finished file
//!   is always whole. Running again skips files that are already there with the right size, and
//!   archives already finished are skipped via [`STATE_FILE`].
//! - An existing file with the same name but different content is never overwritten (reported as a
//!   conflict).
//! - **Incremental re-import:** every imported file is recorded (path and size) in [`STATE_FILE`].
//!   A later Takeout of the same library only adds new items: a file imported before is not
//!   written again, even if it was deleted or moved since (rejects stay gone), and with
//!   [`UnpackOptions::skip_duplicates`] a photo with the same file name and size as one already
//!   imported in another folder (the album copy of a year-folder photo) is skipped too.
//! - [`plan`] is the dry run: total size, item counts, how much is new, the space needed (with and
//!   without deleting archives) and the free space. [`unpack_with`] runs the same check first
//!   and refuses to start if the destination is too small.
//! - [`UnpackOptions::delete_archives`] deletes each archive once it has been unpacked without
//!   conflicts, so the peak extra space is about the new data plus one archive instead of twice.

use crate::safe_path::{components, place, Place};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Progress record in the destination folder (Unswiper's own file; not media, so not indexed).
pub const STATE_FILE: &str = "unswiper-takeout-state.json";
/// Suffix of files still being written.
pub const PART_SUFFIX: &str = ".unswiper-part";

#[derive(Debug, thiserror::Error)]
pub enum UnpackError {
    #[error("{0}: not a Google Takeout archive (.zip or .tgz)")]
    UnknownFormat(PathBuf),
    #[error("{path}: {source}")]
    Archive {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not write to {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("no Google Photos folder found in these archives (found: {0})")]
    NoPhotos(String),
    #[error("not enough free space in {path}: needs {needed} bytes, {free} free")]
    NotEnoughSpace {
        path: PathBuf,
        needed: u64,
        free: u64,
    },
    #[error("no .zip or .tgz archives in {0}")]
    NoArchives(PathBuf),
}

/// How to unpack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnpackOptions {
    /// Delete each archive after it unpacked without conflicts (cuts peak space to about the new
    /// data plus one archive when the archives are on the same disk as the destination).
    pub delete_archives: bool,
    /// Skip a media file with the same name and size as one already imported in another folder.
    pub skip_duplicates: bool,
    /// Check the free space before starting.
    pub check_space: bool,
    /// Space to leave free on the destination disk.
    pub reserve_bytes: u64,
}

impl Default for UnpackOptions {
    fn default() -> Self {
        Self {
            delete_archives: false,
            skip_duplicates: true,
            check_space: true,
            reserve_bytes: 1 << 30,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UnpackProgress {
    /// 1-based number of the archive being unpacked.
    pub archive: usize,
    pub archives: usize,
    /// Files unpacked or skipped so far in all archives.
    pub files: usize,
    /// Bytes written so far.
    pub bytes: u64,
    /// Estimated bytes to write in total (0 if unknown); for a percentage and time left.
    pub bytes_total: u64,
}

/// What happened.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnpackReport {
    /// Files written in this run.
    pub extracted: usize,
    /// Files already present from an earlier (interrupted) run.
    pub already_there: usize,
    /// Archives skipped because an earlier run finished them.
    pub archives_skipped: usize,
    pub bytes_written: u64,
    /// Refused entry names (unsafe paths, symlinks, hard links).
    pub refused: Vec<String>,
    /// Entries not written because a different file with that name exists.
    pub conflicts: Vec<String>,
    /// Other Google products in the archives (skipped).
    pub other_products: BTreeSet<String>,
    /// Files skipped because an earlier import already brought them in (deleted or moved since).
    pub previously_imported: usize,
    /// Media files skipped as copies (same name and size) of a file imported in another folder.
    pub duplicates_skipped: usize,
    /// Archives deleted after unpacking.
    pub archives_deleted: Vec<PathBuf>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    version: u32,
    /// Finished archives: file name → size in bytes.
    done: BTreeMap<String, u64>,
    /// Finished archives that had conflicts (never deleted automatically).
    #[serde(default)]
    with_conflicts: BTreeSet<String>,
    /// Finished archives that had no Google Photos files (never deleted automatically).
    #[serde(default)]
    without_photos: BTreeSet<String>,
    /// Every file imported so far: path below the destination → size.
    #[serde(default)]
    imported: BTreeMap<String, u64>,
    /// (lower-case file name, size) of imported media files, derived from `imported`.
    #[serde(skip)]
    name_size: HashSet<(String, u64)>,
}

fn is_sidecar(name: &str) -> bool {
    name.to_lowercase().ends_with(".json")
}

fn name_key(rel: &str, size: u64) -> (String, u64) {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    (name.to_lowercase(), size)
}

/// What to do with one file of an archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decision {
    Write,
    AlreadyThere,
    Conflict,
    PreviouslyImported,
    Duplicate,
}

impl State {
    fn record(&mut self, rel: &str, size: u64) {
        self.imported.insert(rel.to_string(), size);
        if !is_sidecar(rel) {
            self.name_size.insert(name_key(rel, size));
        }
    }

    fn decide(&self, target: &Path, rel: &str, size: u64, skip_duplicates: bool) -> Decision {
        match fs::symlink_metadata(target) {
            Ok(m) if m.is_file() && m.len() == size => Decision::AlreadyThere,
            Ok(_) => Decision::Conflict,
            Err(_) if self.imported.get(rel) == Some(&size) => Decision::PreviouslyImported,
            Err(_)
                if skip_duplicates
                    && !is_sidecar(rel)
                    && self.name_size.contains(&name_key(rel, size)) =>
            {
                Decision::Duplicate
            }
            Err(_) => Decision::Write,
        }
    }
}

fn load_state(dest: &Path) -> State {
    let mut s: State = fs::read(dest.join(STATE_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    s.name_size = s
        .imported
        .iter()
        .filter(|(rel, _)| !is_sidecar(rel))
        .map(|(rel, &size)| name_key(rel, size))
        .collect();
    s
}

fn save_state(dest: &Path, s: &State) -> io::Result<()> {
    let tmp = dest.join(format!("{STATE_FILE}{PART_SUFFIX}"));
    fs::write(&tmp, serde_json::to_vec_pretty(s).unwrap_or_default())?;
    fs::rename(tmp, dest.join(STATE_FILE))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Zip,
    Tgz,
}

fn format_of(p: &Path) -> Option<Format> {
    let n = p.file_name()?.to_string_lossy().to_lowercase();
    if n.ends_with(".zip") {
        Some(Format::Zip)
    } else if n.ends_with(".tgz") || n.ends_with(".tar.gz") {
        Some(Format::Tgz)
    } else {
        None
    }
}

/// `takeout-20261015T120000Z-001.zip` → (`takeout-20261015T120000Z-`, `.zip`).
fn part_pattern(name: &str) -> Option<(String, String)> {
    let lower = name.to_lowercase();
    let ext = [".tar.gz", ".tgz", ".zip"]
        .into_iter()
        .find(|e| lower.ends_with(e))?;
    let stem = &name[..name.len() - ext.len()];
    let digits = stem.chars().rev().take_while(char::is_ascii_digit).count();
    if digits == 0 || !stem[..stem.len() - digits].ends_with('-') {
        return None;
    }
    Some((
        stem[..stem.len() - digits].to_string(),
        name[name.len() - ext.len()..].to_string(),
    ))
}

/// All parts of the export `archive` belongs to (`…-001.zip`, `…-002.zip`, …), sorted; just
/// `archive` if its name has no part number.
pub fn find_parts(archive: &Path) -> Vec<PathBuf> {
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (Some((prefix, ext)), Some(dir)) = (part_pattern(&name), archive.parent()) else {
        return vec![archive.to_path_buf()];
    };
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    let mut parts: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| part_pattern(n).is_some_and(|(p, e)| p == prefix && e == ext))
        .map(|n| archive.with_file_name(n))
        .collect();
    parts.sort();
    if parts.is_empty() {
        parts.push(archive.to_path_buf());
    }
    parts
}

/// All Takeout archives (`.zip`, `.tgz`, `.tar.gz`) directly in `dir`, sorted, so every part of
/// every export in the folder is imported.
pub fn find_archives(dir: &Path) -> Result<Vec<PathBuf>, UnpackError> {
    let rd = fs::read_dir(dir).map_err(|e| UnpackError::Archive {
        path: dir.to_path_buf(),
        source: e,
    })?;
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| format_of(p).is_some())
        .collect();
    v.sort();
    if v.is_empty() {
        return Err(UnpackError::NoArchives(dir.to_path_buf()));
    }
    Ok(v)
}

/// Creates `dest/<rel dirs>` one level at a time, refusing to go through a symlink.
fn safe_dirs(dest: &Path, rel: &[String]) -> io::Result<PathBuf> {
    let mut p = dest.to_path_buf();
    for c in rel {
        p.push(c);
        match fs::symlink_metadata(&p) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{} exists and is not a folder", p.display()),
                ))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => fs::create_dir(&p)?,
            Err(e) => return Err(e),
        }
    }
    Ok(p)
}

struct Sink<'a> {
    dest: &'a Path,
    state: &'a mut State,
    skip_duplicates: bool,
    last_tick_bytes: u64,
    report: &'a mut UnpackReport,
    progress: &'a mut dyn FnMut(UnpackProgress),
    tick: UnpackProgress,
    photos_seen: bool,
}

enum Kind {
    File {
        size: u64,
        mtime: Option<SystemTime>,
    },
    Dir,
    Link,
    Other,
}

impl Sink<'_> {
    fn entry(
        &mut self,
        raw_name: &str,
        kind: Kind,
        data: &mut dyn Read,
    ) -> Result<(), UnpackError> {
        let comps = match components(raw_name) {
            Ok(c) => c,
            Err(_) => {
                self.report.refused.push(raw_name.to_string());
                return Ok(());
            }
        };
        let rel = match place(&comps) {
            Place::Photos(rel) => rel,
            Place::Folder => {
                self.photos_seen |= comps.len() == 2;
                return Ok(());
            }
            Place::OtherProduct(p) => {
                self.report.other_products.insert(p);
                return Ok(());
            }
            Place::Other => return Ok(()),
        };
        self.photos_seen = true;
        let write_err = |path: &Path, source| UnpackError::Write {
            path: path.to_path_buf(),
            source,
        };
        match kind {
            Kind::Dir => {
                safe_dirs(self.dest, &rel).map_err(|e| write_err(self.dest, e))?;
            }
            Kind::Link => self.report.refused.push(raw_name.to_string()),
            Kind::Other => {}
            Kind::File { size, mtime } => {
                let (name, dirs) = rel.split_last().expect("components are never empty");
                if name.ends_with(PART_SUFFIX) || (dirs.is_empty() && name == STATE_FILE) {
                    self.report.refused.push(raw_name.to_string());
                    return Ok(());
                }
                let dir = safe_dirs(self.dest, dirs).map_err(|e| write_err(self.dest, e))?;
                let target = dir.join(name);
                let key = rel.join("/");
                match self.state.decide(&target, &key, size, self.skip_duplicates) {
                    Decision::AlreadyThere => {
                        self.report.already_there += 1;
                        self.state.record(&key, size);
                    }
                    Decision::Conflict => self.report.conflicts.push(key),
                    Decision::PreviouslyImported => self.report.previously_imported += 1,
                    Decision::Duplicate => self.report.duplicates_skipped += 1,
                    Decision::Write => {
                        let part = dir.join(format!("{name}{PART_SUFFIX}"));
                        let n = (|| -> io::Result<u64> {
                            let _ = fs::remove_file(&part); // never write through a symlink
                            let mut f = File::options().write(true).create_new(true).open(&part)?;
                            let n = io::copy(data, &mut f)?;
                            if n != size {
                                return Err(io::Error::new(
                                    io::ErrorKind::UnexpectedEof,
                                    format!("expected {size} bytes, got {n}"),
                                ));
                            }
                            if let Some(t) = mtime {
                                let _ = f.set_modified(t);
                            }
                            f.sync_all()?;
                            drop(f);
                            fs::rename(&part, &target)?;
                            Ok(n)
                        })()
                        .map_err(|e| {
                            let _ = fs::remove_file(&part);
                            write_err(&target, e)
                        })?;
                        self.state.record(&key, size);
                        self.report.extracted += 1;
                        self.report.bytes_written += n;
                        self.tick.bytes += n;
                    }
                }
                self.tick.files += 1;
                if self.tick.files.is_multiple_of(25)
                    || self.tick.bytes - self.last_tick_bytes >= 64 << 20
                {
                    self.last_tick_bytes = self.tick.bytes;
                    (self.progress)(self.tick);
                }
            }
        }
        Ok(())
    }
}

fn unpack_zip(path: &Path, sink: &mut Sink) -> Result<(), UnpackError> {
    let arch_err = |source| UnpackError::Archive {
        path: path.to_path_buf(),
        source,
    };
    let file = File::open(path).map_err(arch_err)?;
    let mut zip = zip::ZipArchive::new(io::BufReader::new(file))
        .map_err(|e| arch_err(io::Error::new(io::ErrorKind::InvalidData, e)))?;
    for i in 0..zip.len() {
        let mut f = zip
            .by_index(i)
            .map_err(|e| arch_err(io::Error::new(io::ErrorKind::InvalidData, e)))?;
        let name = match f.name() {
            Ok(n) => n.into_owned(),
            Err(_) => String::from_utf8_lossy(f.name_raw()).into_owned(),
        };
        let mode = f.unix_mode().unwrap_or(0);
        let kind = if f.is_dir() {
            Kind::Dir
        } else if mode & 0o170000 == 0o120000 {
            Kind::Link
        } else {
            Kind::File {
                size: f.size(),
                mtime: None,
            }
        };
        sink.entry(&name, kind, &mut f)?;
    }
    Ok(())
}

fn unpack_tgz(path: &Path, sink: &mut Sink) -> Result<(), UnpackError> {
    let arch_err = |source| UnpackError::Archive {
        path: path.to_path_buf(),
        source,
    };
    let file = File::open(path).map_err(arch_err)?;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(io::BufReader::new(file)));
    for e in tar.entries().map_err(arch_err)? {
        let mut e = e.map_err(arch_err)?;
        let name = String::from_utf8_lossy(&e.path_bytes()).into_owned();
        let h = e.header();
        let kind = match h.entry_type() {
            tar::EntryType::Regular | tar::EntryType::Continuous => Kind::File {
                size: h.size().map_err(arch_err)?,
                mtime: h.mtime().ok().map(|s| UNIX_EPOCH + Duration::from_secs(s)),
            },
            tar::EntryType::Directory => Kind::Dir,
            tar::EntryType::Symlink | tar::EntryType::Link => Kind::Link,
            _ => Kind::Other,
        };
        sink.entry(&name, kind, &mut e)?;
    }
    Ok(())
}

fn remove_parts(dir: &Path) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => remove_parts(&p),
            Ok(t) if t.is_file() && p.to_string_lossy().ends_with(PART_SUFFIX) => {
                let _ = fs::remove_file(&p);
            }
            _ => {}
        }
    }
}

/// Dry-run figures for one archive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArchivePlan {
    pub path: PathBuf,
    /// Size of the archive file.
    pub archive_bytes: u64,
    /// Media files (photos and videos) in the Google Photos part.
    pub items: usize,
    /// JSON sidecars and album metadata files.
    pub sidecars: usize,
    /// Uncompressed size of the Google Photos part.
    pub photos_bytes: u64,
    /// Bytes this import would write (new files only).
    pub new_bytes: u64,
    /// Media files that would be written.
    pub new_items: usize,
    /// Media files skipped (already there, imported before, or duplicates).
    pub skipped_items: usize,
    /// False for a `.tgz` estimated from its file size (exact figures need reading it all).
    pub exact: bool,
    /// Finished by an earlier import (not read again; counts are 0).
    pub done_earlier: bool,
}

/// Dry-run result: what an import would do and whether it fits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub archives: Vec<ArchivePlan>,
    pub archive_bytes: u64,
    pub items: usize,
    pub photos_bytes: u64,
    pub new_items: usize,
    pub new_bytes: u64,
    /// Free bytes on the destination disk (None if unknown).
    pub free_bytes: Option<u64>,
    /// Peak extra space needed when the archives are kept (the new data).
    pub needed_keep: u64,
    /// Peak extra space needed when each archive is deleted after unpacking.
    pub needed_delete: u64,
    /// True if the archives are on the same disk as the destination (deleting them frees space
    /// there).
    pub same_disk: bool,
}

impl Plan {
    /// Space needed for this choice, including the reserve.
    pub fn needed(&self, opts: &UnpackOptions) -> u64 {
        let n = if opts.delete_archives {
            self.needed_delete
        } else {
            self.needed_keep
        };
        n + opts.reserve_bytes
    }

    pub fn fits(&self, opts: &UnpackOptions) -> bool {
        self.free_bytes.is_none_or(|f| f >= self.needed(opts))
    }
}

/// Nearest existing folder at or above `p`.
fn existing_ancestor(p: &Path) -> PathBuf {
    let mut p = p.to_path_buf();
    while !p.exists() {
        if !p.pop() || p.as_os_str().is_empty() {
            return PathBuf::from(".");
        }
    }
    p
}

/// Free bytes for unprivileged users on the disk holding `p`.
pub fn free_space(p: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        let s = rustix::fs::statvfs(existing_ancestor(p)).ok()?;
        Some(s.f_bavail.saturating_mul(s.f_frsize))
    }
    #[cfg(not(unix))]
    {
        let _ = p;
        None
    }
}

fn same_disk(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (fs::metadata(a), fs::metadata(existing_ancestor(b))) {
            (Ok(x), Ok(y)) => x.dev() == y.dev(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (a, b);
        false
    }
}

/// Files (path, size) of one archive, from the zip directory, or from reading a whole `.tgz`.
fn list_entries(path: &Path) -> Result<Vec<(String, u64)>, UnpackError> {
    let arch_err = |source| UnpackError::Archive {
        path: path.to_path_buf(),
        source,
    };
    let invalid =
        |e: zip::result::ZipError| arch_err(io::Error::new(io::ErrorKind::InvalidData, e));
    let mut out = vec![];
    let file = File::open(path).map_err(arch_err)?;
    match format_of(path) {
        Some(Format::Zip) => {
            let mut zip = zip::ZipArchive::new(io::BufReader::new(file)).map_err(invalid)?;
            for i in 0..zip.len() {
                let f = zip.by_index_raw(i).map_err(invalid)?;
                let symlink = f.unix_mode().unwrap_or(0) & 0o170000 == 0o120000;
                if !f.is_dir() && !symlink {
                    let name = match f.name() {
                        Ok(n) => n.into_owned(),
                        Err(_) => String::from_utf8_lossy(f.name_raw()).into_owned(),
                    };
                    out.push((name, f.size()));
                }
            }
        }
        Some(Format::Tgz) => {
            let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(io::BufReader::new(file)));
            for e in tar.entries().map_err(arch_err)? {
                let e = e.map_err(arch_err)?;
                let h = e.header();
                if matches!(
                    h.entry_type(),
                    tar::EntryType::Regular | tar::EntryType::Continuous
                ) {
                    let name = String::from_utf8_lossy(&e.path_bytes()).into_owned();
                    out.push((name, h.size().map_err(arch_err)?));
                }
            }
        }
        None => return Err(UnpackError::UnknownFormat(path.to_path_buf())),
    }
    Ok(out)
}

/// Dry run: sizes, item counts, what is new, and the space needed, without writing anything.
/// Zip archives are measured exactly from their directory (fast). A `.tgz` has no directory:
/// with `exact` it is read completely (as slow as unpacking it, minus the writing), otherwise its
/// file size is used as the estimate (photos and videos barely compress, so this errs high).
pub fn plan(
    archives: &[PathBuf],
    dest: &Path,
    opts: &UnpackOptions,
    exact: bool,
) -> Result<Plan, UnpackError> {
    let mut state = load_state(dest);
    let mut plan = Plan {
        free_bytes: free_space(dest),
        same_disk: !archives.is_empty() && archives.iter().all(|a| same_disk(a, dest)),
        ..Default::default()
    };
    let (mut written, mut freed) = (0u64, 0u64);
    for a in archives {
        let archive_bytes = fs::metadata(a)
            .map_err(|e| UnpackError::Archive {
                path: a.clone(),
                source: e,
            })?
            .len();
        let key = a
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut ap = ArchivePlan {
            path: a.clone(),
            archive_bytes,
            exact: true,
            ..Default::default()
        };
        if state.done.get(&key) == Some(&archive_bytes) {
            ap.done_earlier = true;
        } else if format_of(a) == Some(Format::Tgz) && !exact {
            ap.exact = false;
            ap.photos_bytes = archive_bytes;
            ap.new_bytes = archive_bytes;
        } else {
            for (raw, size) in list_entries(a)? {
                let Ok(comps) = components(&raw) else {
                    continue;
                };
                let Place::Photos(rel) = place(&comps) else {
                    continue;
                };
                let rel = rel.join("/");
                let name = rel.rsplit('/').next().unwrap_or(&rel);
                if name.ends_with(PART_SUFFIX) || rel == STATE_FILE {
                    continue;
                }
                let media = !is_sidecar(&rel);
                if media {
                    ap.items += 1;
                } else {
                    ap.sidecars += 1;
                }
                ap.photos_bytes += size;
                match state.decide(&dest.join(&rel), &rel, size, opts.skip_duplicates) {
                    Decision::Write => {
                        ap.new_bytes += size;
                        ap.new_items += usize::from(media);
                        state.record(&rel, size);
                    }
                    Decision::AlreadyThere => {
                        ap.skipped_items += usize::from(media);
                        state.record(&rel, size);
                    }
                    _ => ap.skipped_items += usize::from(media),
                }
            }
        }
        written += ap.new_bytes;
        plan.needed_delete = plan.needed_delete.max(written.saturating_sub(freed));
        if plan.same_disk {
            freed += archive_bytes;
        }
        plan.archive_bytes += archive_bytes;
        plan.items += ap.items;
        plan.photos_bytes += ap.photos_bytes;
        plan.new_items += ap.new_items;
        plan.new_bytes += ap.new_bytes;
        plan.archives.push(ap);
    }
    plan.needed_keep = written;
    Ok(plan)
}

/// [`unpack_with`] with the default options.
pub fn unpack(
    archives: &[PathBuf],
    dest: &Path,
    progress: impl FnMut(UnpackProgress),
) -> Result<UnpackReport, UnpackError> {
    unpack_with(archives, dest, &UnpackOptions::default(), progress)
}

/// Unpacks the Google Photos part of `archives` into `dest` (created if missing). Safe to run
/// again after an interruption or with a newer export: finished work and files imported before
/// are skipped.
pub fn unpack_with(
    archives: &[PathBuf],
    dest: &Path,
    opts: &UnpackOptions,
    mut progress: impl FnMut(UnpackProgress),
) -> Result<UnpackReport, UnpackError> {
    for a in archives {
        format_of(a).ok_or_else(|| UnpackError::UnknownFormat(a.clone()))?;
    }
    let estimate = plan(archives, dest, opts, false)?;
    if opts.check_space && !estimate.fits(opts) {
        return Err(UnpackError::NotEnoughSpace {
            path: dest.to_path_buf(),
            needed: estimate.needed(opts),
            free: estimate.free_bytes.unwrap_or(0),
        });
    }
    fs::create_dir_all(dest).map_err(|e| UnpackError::Write {
        path: dest.to_path_buf(),
        source: e,
    })?;
    let mut state = load_state(dest);
    state.version = 2;
    let mut report = UnpackReport::default();
    let mut tick = UnpackProgress {
        archives: archives.len(),
        bytes_total: estimate.new_bytes,
        ..Default::default()
    };
    let mut photos_seen = false;
    for (i, a) in archives.iter().enumerate() {
        tick.archive = i + 1;
        progress(tick);
        let key = a
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let size = fs::metadata(a)
            .map_err(|e| UnpackError::Archive {
                path: a.clone(),
                source: e,
            })?
            .len();
        if state.done.get(&key) == Some(&size) {
            report.archives_skipped += 1;
            photos_seen |= !state.without_photos.contains(&key);
        } else {
            let conflicts_before = report.conflicts.len();
            let mut sink = Sink {
                dest,
                state: &mut state,
                skip_duplicates: opts.skip_duplicates,
                last_tick_bytes: tick.bytes,
                report: &mut report,
                progress: &mut progress,
                tick,
                photos_seen: false,
            };
            match format_of(a) {
                Some(Format::Zip) => unpack_zip(a, &mut sink)?,
                Some(Format::Tgz) => unpack_tgz(a, &mut sink)?,
                None => unreachable!("checked above"),
            }
            tick = sink.tick;
            photos_seen |= sink.photos_seen;
            if !sink.photos_seen {
                state.without_photos.insert(key.clone());
            }
            state.done.insert(key.clone(), size);
            if report.conflicts.len() > conflicts_before {
                state.with_conflicts.insert(key.clone());
            }
            save_state(dest, &state).map_err(|e| UnpackError::Write {
                path: dest.join(STATE_FILE),
                source: e,
            })?;
        }
        let deletable = state.done.get(&key) == Some(&size)
            && !state.with_conflicts.contains(&key)
            && !state.without_photos.contains(&key);
        if opts.delete_archives && deletable {
            fs::remove_file(a).map_err(|e| UnpackError::Write {
                path: a.clone(),
                source: e,
            })?;
            report.archives_deleted.push(a.clone());
        }
    }
    if !photos_seen {
        let found: Vec<_> = report.other_products.iter().cloned().collect();
        return Err(UnpackError::NoPhotos(if found.is_empty() {
            "nothing".into()
        } else {
            found.join(", ")
        }));
    }
    remove_parts(dest);
    progress(tick);
    Ok(report)
}
