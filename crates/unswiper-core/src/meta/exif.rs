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

/// The GPS timestamp of an image (`GPSDateStamp` + `GPSTimeStamp`, always UTC), or `None`.
/// The fallback for photos without `DateTimeOriginal`/`DateTime` (plan §20.14).
pub fn gps_date(path: &Path) -> Option<Timestamp> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let data = exif::Reader::new().read_from_container(&mut reader).ok()?;
    let day = data
        .get_field(Tag::GPSDateStamp, In::PRIMARY)
        .and_then(ascii)?;
    let hms = match data
        .get_field(Tag::GPSTimeStamp, In::PRIMARY)
        .map(|f| &f.value)
    {
        Some(Value::Rational(v)) if v.len() == 3 => [v[0].to_f64(), v[1].to_f64(), v[2].to_f64()],
        _ => return None,
    };
    parse_gps_datetime(&day, hms)
}

/// `"YYYY:MM:DD"` plus `[h, m, s]` (UTC) → timestamp. Fractions of a second are dropped.
pub fn parse_gps_datetime(day: &str, hms: [f64; 3]) -> Option<Timestamp> {
    use chrono::{NaiveDate, TimeZone, Utc};
    let day = day.trim_end_matches(['\0', ' ']);
    let d = NaiveDate::parse_from_str(day, "%Y:%m:%d").ok()?;
    if hms.iter().any(|x| !x.is_finite() || *x < 0.0) {
        return None;
    }
    let t = d.and_hms_opt(hms[0] as u32, hms[1] as u32, hms[2] as u32)?;
    crate::meta::usable(Some(Utc.from_utc_datetime(&t)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gps_formats() {
        let t = parse_gps_datetime("2024:06:12", [13.0, 32.0, 41.5]).unwrap();
        assert_eq!(crate::time::format_iso(&t), "2024-06-12T13:32:41Z");
        assert!(parse_gps_datetime("2024:06:12\0", [0.0, 0.0, 0.0]).is_some());
        assert!(parse_gps_datetime("0000:00:00", [0.0, 0.0, 0.0]).is_none());
        assert!(parse_gps_datetime("2024:06:12", [25.0, 0.0, 0.0]).is_none());
        assert!(parse_gps_datetime("2024:06:12", [f64::NAN, 0.0, 0.0]).is_none());
        assert!(parse_gps_datetime("1970:01:01", [0.0, 0.0, 0.0]).is_none());
    }

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
