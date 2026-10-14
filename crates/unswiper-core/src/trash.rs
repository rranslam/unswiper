// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Trash back ends. The macOS app keeps using `FileManager.trashItem` until Phase 6; the core
//! ships a freedesktop.org home-trash implementation (Trash spec 1.0) for Linux.
//! Other-drive `$topdir/.Trash-$uid` support lands in Phase 2 (plan §10.4).

use std::io::Write;
use std::path::{Path, PathBuf};

pub trait Trasher {
    /// Moves `path` to the trash. Returns where it ended up, when known.
    fn trash(&self, path: &Path) -> std::io::Result<Option<PathBuf>>;
}

/// `$XDG_DATA_HOME/Trash` (default `~/.local/share/Trash`).
pub struct FreedesktopTrash {
    pub trash_dir: PathBuf,
}

impl FreedesktopTrash {
    pub fn home() -> Self {
        let base = std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
            });
        FreedesktopTrash {
            trash_dir: base.join("Trash"),
        }
    }

    /// Path to the `.trashinfo` for a file in `files/`, if `p` is inside this trash.
    pub fn info_for(&self, p: &Path) -> Option<PathBuf> {
        let name = p.file_name()?;
        (p.parent()? == self.trash_dir.join("files")).then(|| {
            let mut n = name.to_owned();
            n.push(".trashinfo");
            self.trash_dir.join("info").join(n)
        })
    }
}

/// Percent-encodes a path for `Path=` in `.trashinfo` (RFC 2396 unreserved + `/` kept).
pub fn url_encode_path(p: &str) -> String {
    let mut s = String::new();
    for b in p.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.!~*'()".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

impl Trasher for FreedesktopTrash {
    fn trash(&self, path: &Path) -> std::io::Result<Option<PathBuf>> {
        let files = self.trash_dir.join("files");
        let info = self.trash_dir.join("info");
        std::fs::create_dir_all(&files)?;
        std::fs::create_dir_all(&info)?;
        let name = path
            .file_name()
            .ok_or_else(|| std::io::Error::other("no file name"))?
            .to_string_lossy()
            .into_owned();
        let (stem, ext) = split_name(&name);
        // Reserve a unique name by creating the .trashinfo exclusively: a.jpg, a.1.jpg, a.2.jpg …
        let mut n = 0;
        let (target, info_path) = loop {
            let candidate = if n == 0 {
                name.clone()
            } else {
                format!("{stem}.{n}{ext}")
            };
            let ip = info.join(format!("{candidate}.trashinfo"));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&ip)
            {
                Ok(mut f) if !files.join(&candidate).exists() => {
                    let abs = crate::path_util::canonical_path(path);
                    let date = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
                    write!(
                        f,
                        "[Trash Info]\nPath={}\nDeletionDate={date}\n",
                        url_encode_path(&abs.to_string_lossy())
                    )?;
                    f.sync_all()?;
                    break (files.join(&candidate), ip);
                }
                Ok(_) => {
                    let _ = std::fs::remove_file(&ip);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
            n += 1;
        };
        if let Err(e) = std::fs::rename(path, &target) {
            let _ = std::fs::remove_file(&info_path);
            if e.raw_os_error() == Some(18) {
                return Err(std::io::Error::other(
                    "This file is on a different drive than your Trash. Use Quarantine mode for this library.",
                ));
            }
            return Err(e);
        }
        Ok(Some(target))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode() {
        assert_eq!(url_encode_path("/a b/ç%.jpg"), "/a%20b/%C3%A7%25.jpg");
        assert_eq!(split_name(".hidden"), (".hidden", ""));
        assert_eq!(split_name("a.b.jpg"), ("a.b", ".jpg"));
        assert_eq!(split_name("noext"), ("noext", ""));
    }

    #[test]
    fn trash_layout_and_collisions() {
        let d = tempfile::tempdir().unwrap();
        let t = FreedesktopTrash {
            trash_dir: d.path().join("Trash"),
        };
        let src = d.path().join("lib");
        std::fs::create_dir(&src).unwrap();
        let mut results = vec![];
        for _ in 0..3 {
            std::fs::write(src.join("a b.jpg"), b"x").unwrap();
            results.push(t.trash(&src.join("a b.jpg")).unwrap().unwrap());
        }
        let names: Vec<_> = results
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a b.jpg", "a b.1.jpg", "a b.2.jpg"]);
        let info = std::fs::read_to_string(t.info_for(&results[1]).unwrap()).unwrap();
        assert!(info.starts_with("[Trash Info]\nPath=/"));
        assert!(info.contains("a%20b.jpg\nDeletionDate="));
        assert!(t.info_for(Path::new("/elsewhere/x")).is_none());
        // Missing source: error and the reserved .trashinfo is removed.
        assert!(t.trash(&src.join("missing.jpg")).is_err());
        assert!(!t.trash_dir.join("info/missing.jpg.trashinfo").exists());
        assert!(t.trash(Path::new("/")).is_err());
    }

    #[test]
    fn stale_file_without_info_is_skipped() {
        let d = tempfile::tempdir().unwrap();
        let t = FreedesktopTrash {
            trash_dir: d.path().join("Trash"),
        };
        std::fs::create_dir_all(t.trash_dir.join("files")).unwrap();
        std::fs::write(t.trash_dir.join("files/a.jpg"), b"old").unwrap();
        std::fs::write(d.path().join("a.jpg"), b"new").unwrap();
        let r = t.trash(&d.path().join("a.jpg")).unwrap().unwrap();
        assert_eq!(r.file_name().unwrap(), "a.1.jpg");
        assert!(!t.trash_dir.join("info/a.jpg.trashinfo").exists());
    }

    #[test]
    fn home_uses_xdg() {
        let t = FreedesktopTrash::home();
        assert!(t.trash_dir.ends_with("Trash"));
    }
}
