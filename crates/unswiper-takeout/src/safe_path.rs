// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Archive entry names → safe relative paths (no zip-slip), and the Google Photos part of a
//! Takeout export.

/// Why an entry name was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unsafe {
    Absolute,
    ParentDir,
    DriveOrDevice,
    NulByte,
    Empty,
}

/// Splits an archive entry name into plain path components, refusing anything that could escape
/// the destination folder: absolute paths, `..`, Windows drive letters and device-like prefixes,
/// and NUL bytes. Backslashes count as separators (Windows-made zips). `.` and empty components are
/// dropped.
pub fn components(name: &str) -> Result<Vec<String>, Unsafe> {
    if name.contains('\0') {
        return Err(Unsafe::NulByte);
    }
    let name = name.replace('\\', "/");
    if name.starts_with('/') {
        return Err(Unsafe::Absolute);
    }
    let mut out = vec![];
    for (i, c) in name.split('/').enumerate() {
        match c {
            "" | "." => continue,
            ".." => return Err(Unsafe::ParentDir),
            _ => {}
        }
        if i == 0 && c.len() >= 2 && c.as_bytes()[1] == b':' {
            return Err(Unsafe::DriveOrDevice);
        }
        out.push(c.to_string());
    }
    if out.is_empty() {
        return Err(Unsafe::Empty);
    }
    Ok(out)
}

/// Names Google uses for the Google Photos folder inside `Takeout/` (it is localised).
pub const PHOTOS_DIRS: &[&str] = &[
    "Google Photos",
    "Google Fotos",
    "Google Foto",
    "Google Фото",
    "Google フォト",
    "Google 포토",
    "Google 相册",
    "Google 相簿",
];

/// Where an entry belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// Inside Google Photos: the path below the Google Photos folder (album or year folders).
    Photos(Vec<String>),
    /// The Google Photos folder itself, or a folder above it.
    Folder,
    /// Another Google product (`Takeout/Mail/...`), named for the report.
    OtherProduct(String),
    /// Anything else: not under `Takeout/`, or a file directly in it (`archive_browser.html`).
    Other,
}

/// Classifies an entry's components. `Takeout/Google Photos/2019/IMG.jpg` → `Photos(["2019",
/// "IMG.jpg"])`. The Google Photos folder name is matched case-insensitively.
pub fn place(comps: &[String]) -> Place {
    let is_photos = |s: &str| PHOTOS_DIRS.iter().any(|p| p.eq_ignore_ascii_case(s));
    match comps {
        [t, ..] if !t.eq_ignore_ascii_case("Takeout") => Place::Other,
        [_] => Place::Folder,
        [_, p] if is_photos(p) => Place::Folder,
        [_, p, rest @ ..] if is_photos(p) => Place::Photos(rest.to_vec()),
        // A file directly in Takeout/ (archive_browser.html) or an empty product folder.
        [_, _] => Place::Other,
        [_, p, ..] => Place::OtherProduct(p.clone()),
        [] => Place::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<String> {
        components(s).unwrap()
    }

    #[test]
    fn plain_names() {
        assert_eq!(
            c("Takeout/Google Photos/a b.jpg"),
            ["Takeout", "Google Photos", "a b.jpg"]
        );
        assert_eq!(c("./Takeout//x/./y.jpg"), ["Takeout", "x", "y.jpg"]);
        assert_eq!(
            c("Takeout\\Google Photos\\y.jpg"),
            ["Takeout", "Google Photos", "y.jpg"]
        );
        assert_eq!(c("dir/"), ["dir"]);
        // Dots inside names are fine; only a whole ".." component is refused.
        assert_eq!(c("a/..b/c..jpg"), ["a", "..b", "c..jpg"]);
    }

    #[test]
    fn zip_slip_is_refused() {
        assert_eq!(components("../evil.jpg"), Err(Unsafe::ParentDir));
        assert_eq!(
            components("Takeout/Google Photos/../../../etc/x"),
            Err(Unsafe::ParentDir)
        );
        assert_eq!(components("Takeout\\..\\..\\x"), Err(Unsafe::ParentDir));
        assert_eq!(components("/etc/passwd"), Err(Unsafe::Absolute));
        assert_eq!(components("\\\\server\\share\\x"), Err(Unsafe::Absolute));
        assert_eq!(components("C:/Windows/x"), Err(Unsafe::DriveOrDevice));
        assert_eq!(components("C:x"), Err(Unsafe::DriveOrDevice));
        assert_eq!(components("a\0b"), Err(Unsafe::NulByte));
        assert_eq!(components("./"), Err(Unsafe::Empty));
    }

    #[test]
    fn places() {
        assert_eq!(
            place(&c("Takeout/Google Photos/Photos from 2019/IMG.jpg")),
            Place::Photos(vec!["Photos from 2019".into(), "IMG.jpg".into()])
        );
        assert_eq!(
            place(&c("takeout/google photos/x.jpg")),
            Place::Photos(vec!["x.jpg".into()])
        );
        assert_eq!(
            place(&c("Takeout/Google Fotos/x.jpg")),
            Place::Photos(vec!["x.jpg".into()])
        );
        assert_eq!(place(&c("Takeout/Google Photos")), Place::Folder);
        assert_eq!(place(&c("Takeout")), Place::Folder);
        assert_eq!(
            place(&c("Takeout/Mail/All.mbox")),
            Place::OtherProduct("Mail".into())
        );
        assert_eq!(place(&c("Takeout/archive_browser.html")), Place::Other);
        assert_eq!(place(&c("readme.txt")), Place::Other);
    }
}
