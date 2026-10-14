// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! The app icon, embedded so the window and About dialog show it even when Unswiper
//! runs uninstalled (cargo run, a bare binary). Installed copies (`data/icons`, the
//! AppImage) are found by the icon theme as usual; this only adds a fallback search path.

use std::path::{Path, PathBuf};

/// (path under the theme root, bytes)
pub const FILES: &[(&str, &[u8])] = &[
    (
        "hicolor/scalable/apps/io.github.rranslam.Unswiper.svg",
        include_bytes!("../data/icons/hicolor/scalable/apps/io.github.rranslam.Unswiper.svg"),
    ),
    (
        "hicolor/symbolic/apps/io.github.rranslam.Unswiper-symbolic.svg",
        include_bytes!(
            "../data/icons/hicolor/symbolic/apps/io.github.rranslam.Unswiper-symbolic.svg"
        ),
    ),
    (
        "hicolor/16x16/apps/io.github.rranslam.Unswiper.png",
        include_bytes!("../data/icons/hicolor/16x16/apps/io.github.rranslam.Unswiper.png"),
    ),
    (
        "hicolor/32x32/apps/io.github.rranslam.Unswiper.png",
        include_bytes!("../data/icons/hicolor/32x32/apps/io.github.rranslam.Unswiper.png"),
    ),
    (
        "hicolor/48x48/apps/io.github.rranslam.Unswiper.png",
        include_bytes!("../data/icons/hicolor/48x48/apps/io.github.rranslam.Unswiper.png"),
    ),
    (
        "hicolor/64x64/apps/io.github.rranslam.Unswiper.png",
        include_bytes!("../data/icons/hicolor/64x64/apps/io.github.rranslam.Unswiper.png"),
    ),
    (
        "hicolor/128x128/apps/io.github.rranslam.Unswiper.png",
        include_bytes!("../data/icons/hicolor/128x128/apps/io.github.rranslam.Unswiper.png"),
    ),
    (
        "hicolor/256x256/apps/io.github.rranslam.Unswiper.png",
        include_bytes!("../data/icons/hicolor/256x256/apps/io.github.rranslam.Unswiper.png"),
    ),
    (
        "hicolor/512x512/apps/io.github.rranslam.Unswiper.png",
        include_bytes!("../data/icons/hicolor/512x512/apps/io.github.rranslam.Unswiper.png"),
    ),
];

/// `~/.cache/unswiper/icons`
pub fn default_dir() -> PathBuf {
    crate::thumbs::default_cache_dir()
        .parent()
        .map(|p| p.join("icons"))
        .unwrap_or_else(|| PathBuf::from("icons"))
}

/// Writes the embedded icons under `dir` (only files that are missing or differ).
/// Returns the theme root to add as an icon search path.
pub fn install(dir: &Path) -> std::io::Result<PathBuf> {
    for (rel, bytes) in FILES {
        let p = dir.join(rel);
        if std::fs::read(&p).ok().as_deref() != Some(*bytes) {
            std::fs::create_dir_all(p.parent().unwrap())?;
            unswiper_core::store::write_atomic(&p, bytes)?;
        }
    }
    Ok(dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_every_size_once() {
        let d = tempfile::tempdir().unwrap();
        let root = install(d.path()).unwrap();
        for (rel, bytes) in FILES {
            assert_eq!(std::fs::read(root.join(rel)).unwrap(), *bytes, "{rel}");
        }
        let svg = root.join(FILES[0].0);
        let t = std::fs::metadata(&svg).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        install(d.path()).unwrap();
        assert_eq!(std::fs::metadata(&svg).unwrap().modified().unwrap(), t);
        assert!(default_dir().ends_with("unswiper/icons"));
    }

    #[test]
    fn pngs_are_square_and_sized() {
        for (rel, bytes) in FILES.iter().filter(|(r, _)| r.ends_with(".png")) {
            let n: u32 = rel
                .split('/')
                .nth(1)
                .unwrap()
                .split('x')
                .next()
                .unwrap()
                .parse()
                .unwrap();
            let img = image::load_from_memory(bytes).unwrap();
            assert_eq!((img.width(), img.height()), (n, n), "{rel}");
        }
        let svg = std::str::from_utf8(FILES[0].1).unwrap();
        assert!(svg.contains("SPDX-License-Identifier: MIT"));
    }
}
