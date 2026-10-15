// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Capture date of a still image: EXIF `DateTimeOriginal`, then TIFF `DateTime`
//! (same precedence as the Swift `exifDate(for:)`), interpreted in the local time zone.
//! Supports JPEG, TIFF/DNG, HEIF/HEIC, PNG and WebP containers.

use crate::time::{local_to_utc, Timestamp};
use chrono::NaiveDateTime;
use exif::{In, Tag, Value};
use std::io::BufReader;
use std::path::Path;

/// Parses an EXIF date string `yyyy:MM:dd HH:mm:ss` (trailing NULs/spaces ignored) as local time.
pub fn parse_exif_datetime(raw: &str) -> Option<Timestamp> {
    let s = raw.trim_end_matches(['\0', ' ']);
    let n = NaiveDateTime::parse_from_str(s, "%Y:%m:%d %H:%M:%S").ok()?;
    local_to_utc(n)
}

fn ascii(field: &exif::Field) -> Option<String> {
    match &field.value {
        Value::Ascii(v) => v.first().map(|b| String::from_utf8_lossy(b).into_owned()),
        _ => None,
    }
}

/// Reads the capture date of an image file, or `None`.
pub fn exif_date(path: &Path) -> Option<Timestamp> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let data = exif::Reader::new().read_from_container(&mut reader).ok()?;
    for tag in [Tag::DateTimeOriginal, Tag::DateTime] {
        if let Some(d) = data
            .get_field(tag, In::PRIMARY)
            .and_then(ascii)
            .and_then(|s| parse_exif_datetime(&s))
        {
            return Some(d);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_formats() {
        assert!(parse_exif_datetime("2026:01:02 10:11:12").is_some());
        assert!(parse_exif_datetime("2026:01:02 10:11:12\0").is_some());
        assert!(parse_exif_datetime("0000:00:00 00:00:00").is_none());
        assert!(parse_exif_datetime("2026-01-02 10:11:12").is_none());
        assert!(parse_exif_datetime("").is_none());
    }

    #[test]
    fn missing_or_non_image() {
        assert_eq!(exif_date(Path::new("/nonexistent/x.jpg")), None);
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.jpg");
        std::fs::write(&p, b"not a jpeg").unwrap();
        assert_eq!(exif_date(&p), None);
    }
}
