// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Dates from camera file names (plan §20.14, M3): the fallback for photos and videos without
//! a capture date or a GPS timestamp, before the folder name.
//!
//! Recognised, anywhere in the file name (without extension):
//! - `YYYYMMDD` followed by `_`/`-` and `HHMMSS` (`IMG_20240612_133241`, `VID_…`, `PANO_…`,
//!   `PXL_20240612_133241123`, `20240612_133241`, `IMG-20240612-WA0001` without a time);
//! - a 14-or-more digit run `YYYYMMDDHHMMSS…` (`…BURST20240612133241123_COVER`);
//! - `YYYY-MM-DD` with an optional `[ _-]HH[-.:]MM[-.:]SS` (`Screenshot_2024-06-12-13-32-41`,
//!   `2024-06-12 13.32.41`).
//!
//! Years 1990–2099; the date (and time) must exist. A date without a time is local noon. Pixel
//! (`PXL_`) names are in UTC, every other name in local time, like the camera clock.

use crate::time::{local_to_utc, Timestamp};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};

fn digits_at(b: &[u8], i: usize) -> usize {
    b[i..].iter().take_while(|c| c.is_ascii_digit()).count()
}

fn num(b: &[u8], i: usize, n: usize) -> Option<u32> {
    std::str::from_utf8(b.get(i..i + n)?).ok()?.parse().ok()
}

fn date(y: u32, m: u32, d: u32) -> Option<NaiveDate> {
    if !(1990..=2099).contains(&y) {
        return None;
    }
    NaiveDate::from_ymd_opt(y as i32, m, d)
}

fn time(h: u32, m: u32, s: u32) -> Option<NaiveTime> {
    NaiveTime::from_hms_opt(h, m, s)
}

/// The date in a file name (`stem` = name without extension), as a naive wall-clock time.
pub fn parse_naive(stem: &str) -> Option<NaiveDateTime> {
    let b = stem.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() || (i > 0 && b[i - 1].is_ascii_digit()) {
            i += 1;
            continue;
        }
        let run = digits_at(b, i);
        let found = if run == 8 || run >= 14 {
            date(num(b, i, 4)?, num(b, i + 4, 2)?, num(b, i + 6, 2)?).and_then(|d| {
                let t = if run >= 14 {
                    time(num(b, i + 8, 2)?, num(b, i + 10, 2)?, num(b, i + 12, 2)?)
                } else {
                    let j = i + 8;
                    let sep = b.get(j).is_some_and(|c| *c == b'_' || *c == b'-');
                    if sep && b.len() > j + 1 && digits_at(b, j + 1) >= 6 {
                        time(num(b, j + 1, 2)?, num(b, j + 3, 2)?, num(b, j + 5, 2)?)
                    } else {
                        None
                    }
                };
                Some(d.and_time(t.unwrap_or(NaiveTime::from_hms_opt(12, 0, 0)?)))
            })
        } else if run == 4 {
            dashed(b, i)
        } else {
            None
        };
        if found.is_some() {
            return found;
        }
        i += run;
    }
    None
}

/// `YYYY-MM-DD` (separators `-`, `_` or `.`) with an optional time.
fn dashed(b: &[u8], i: usize) -> Option<NaiveDateTime> {
    let sep = |k: usize| b.get(k).is_some_and(|c| matches!(c, b'-' | b'_' | b'.'));
    if !(sep(i + 4) && digits_at(b, i + 5) == 2 && sep(i + 7) && digits_at(b, i + 8) == 2) {
        return None;
    }
    let d = date(num(b, i, 4)?, num(b, i + 5, 2)?, num(b, i + 8, 2)?)?;
    let tsep = |k: usize| {
        b.get(k)
            .is_some_and(|c| matches!(c, b'-' | b'_' | b'.' | b':' | b' '))
    };
    let two = |k: usize| {
        (b.len() >= k + 2 && digits_at(b, k) == 2)
            .then(|| num(b, k, 2))
            .flatten()
    };
    let t = (tsep(i + 10) && tsep(i + 13) && tsep(i + 16))
        .then(|| time(two(i + 11)?, two(i + 14)?, two(i + 17)?))
        .flatten();
    Some(d.and_time(t.unwrap_or(NaiveTime::from_hms_opt(12, 0, 0)?)))
}

/// The capture time a camera wrote into the file name, or `None`.
pub fn parse(file_name: &str) -> Option<Timestamp> {
    let stem = file_name.split('.').next().unwrap_or(file_name);
    let n = parse_naive(stem)?;
    if stem.starts_with("PXL_") {
        Some(Utc.from_utc_datetime(&n))
    } else {
        local_to_utc(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> Option<String> {
        parse_naive(s).map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
    }

    #[test]
    fn camera_patterns() {
        assert_eq!(
            n("IMG_20240612_133241").as_deref(),
            Some("2024-06-12 13:32:41")
        );
        assert_eq!(
            n("IMG_20240612_133241-EFFECTS").as_deref(),
            Some("2024-06-12 13:32:41")
        );
        assert_eq!(
            n("PXL_20240612_133241123").as_deref(),
            Some("2024-06-12 13:32:41")
        );
        assert_eq!(
            n("PANO_20240612_133241").as_deref(),
            Some("2024-06-12 13:32:41")
        );
        assert_eq!(n("20240612_133241").as_deref(), Some("2024-06-12 13:32:41"));
        assert_eq!(
            n("IMG-20240612-WA0001").as_deref(),
            Some("2024-06-12 12:00:00")
        );
        assert_eq!(
            n("00000IMG_00000_BURST20240612133241123_COVER").as_deref(),
            Some("2024-06-12 13:32:41")
        );
        assert_eq!(
            n("Screenshot_2024-06-12-13-32-41").as_deref(),
            Some("2024-06-12 13:32:41")
        );
        assert_eq!(
            n("2024-06-12 13.32.41").as_deref(),
            Some("2024-06-12 13:32:41")
        );
        assert_eq!(n("Trip 2024-06-12").as_deref(), Some("2024-06-12 12:00:00"));
    }

    #[test]
    fn not_dates() {
        for s in [
            "IMG_1234",
            "DSCF0001",
            "P1000123",
            "IMG_20241332_120000",
            "IMG_19800101_120000",
            "123456789",
            "IMG_2024",
            "abc",
            "",
            "IMG_20240612_996000x",
        ] {
            let got = n(s);
            assert!(
                got.is_none() || s == "IMG_20240612_996000x",
                "{s:?} -> {got:?}"
            );
        }
        // A bad time keeps the day (noon).
        assert_eq!(
            n("IMG_20240612_996000").as_deref(),
            Some("2024-06-12 12:00:00")
        );
    }

    #[test]
    fn pixel_names_are_utc() {
        let t = parse("PXL_20240612_133241123.jpg").unwrap();
        assert_eq!(crate::time::format_iso(&t), "2024-06-12T13:32:41Z");
        assert!(parse("IMG_20240612_133241.jpg").is_some());
        assert!(parse("IMG_1234.JPG").is_none());
    }
}
