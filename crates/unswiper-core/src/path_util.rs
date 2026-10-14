// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Port of `PathUtil.swift`.

use std::path::{Component, Path, PathBuf};

/// Removes `.` and `..` without touching the filesystem (like `URL.standardizedFileURL`).
pub fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Absolute path with `.`/`..` removed and symlinks resolved. For a path that doesn't exist,
/// the deepest existing parent is resolved and the rest appended unchanged.
pub fn canonical_path(p: &Path) -> PathBuf {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    };
    let norm = lexical_normalize(&abs);
    if let Ok(real) = std::fs::canonicalize(&norm) {
        return real;
    }
    match (norm.parent(), norm.file_name()) {
        (Some(parent), Some(name)) if parent != norm => canonical_path(parent).join(name),
        _ => norm,
    }
}

/// `true` when `path` is strictly inside `root` (component-wise, so `/photos2` is not in `/photos`).
pub fn is_inside(path: &Path, root: &Path) -> bool {
    path != root && path.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        assert_eq!(
            lexical_normalize(Path::new("/a/./b/../c")),
            PathBuf::from("/a/c")
        );
        assert_eq!(lexical_normalize(Path::new("/../a")), PathBuf::from("/a"));
    }

    #[test]
    fn canonical_existing_missing_and_symlink() {
        let d = tempfile::tempdir().unwrap();
        let real = std::fs::canonicalize(d.path()).unwrap();
        assert_eq!(canonical_path(d.path()), real);
        assert_eq!(
            canonical_path(&d.path().join("x/y/../z.jpg")),
            real.join("x").join("z.jpg")
        );
        std::fs::create_dir(real.join("target")).unwrap();
        std::os::unix::fs::symlink(real.join("target"), real.join("link")).unwrap();
        assert_eq!(canonical_path(&real.join("link/f")), real.join("target/f"));
        let rel = canonical_path(Path::new("relative-nonexistent-xyz"));
        assert!(rel.is_absolute());
        assert_eq!(canonical_path(Path::new("/")), PathBuf::from("/"));
    }

    #[test]
    fn inside() {
        assert!(is_inside(Path::new("/photos/a.jpg"), Path::new("/photos")));
        assert!(!is_inside(
            Path::new("/photos2/a.jpg"),
            Path::new("/photos")
        ));
        assert!(!is_inside(Path::new("/photos"), Path::new("/photos")));
    }
}
