// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Text formatting shared by the views (mirrors the macOS app's strings).

use chrono::Local;
use unswiper_core::time::Timestamp;

/// File-size string like Foundation's `ByteCountFormatter` (`.file` style, decimal units).
pub fn bytes(n: i64) -> String {
    if n.abs() < 1000 {
        return if n == 1 { "1 byte".into() } else { format!("{n} bytes") };
    }
    let units = ["KB", "MB", "GB", "TB", "PB"];
    let mut v = n as f64 / 1000.0;
    let mut i = 0;
    while v.abs() >= 999.95 && i + 1 < units.len() {
        v /= 1000.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", v.round() as i64, units[i])
    } else {
        format!("{v:.1} {}", units[i])
    }
}

/// `3 kept · 12 reject · 35 left`
pub fn progress(kept: usize, rejected: usize, left: usize) -> String {
    format!("{kept} kept · {rejected} reject · {left} left")
}

/// `12 photos · 3 videos` (singular/plural, zero parts omitted).
pub fn counts(photos: i64, videos: i64) -> String {
    let p = |n: i64, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    match (photos, videos) {
        (0, 0) => "Empty".into(),
        (p_, 0) => p(p_, "photo", "photos"),
        (0, v) => p(v, "video", "videos"),
        (p_, v) => format!("{} · {}", p(p_, "photo", "photos"), p(v, "video", "videos")),
    }
}

/// Visit date range in local time: `Jan 5, 2026` or `Jan 5 – Jan 7, 2026`; empty for undated.
pub fn date_range(start: &Timestamp, end: &Timestamp) -> String {
    if start.timestamp() < 0 {
        return String::new();
    }
    let s = start.with_timezone(&Local);
    let e = end.with_timezone(&Local);
    if s.date_naive() == e.date_naive() {
        s.format("%b %-d, %Y").to_string()
    } else if s.format("%Y").to_string() == e.format("%Y").to_string() {
        format!("{} – {}", s.format("%b %-d"), e.format("%b %-d, %Y"))
    } else {
        format!("{} – {}", s.format("%b %-d, %Y"), e.format("%b %-d, %Y"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn byte_strings() {
        assert_eq!(bytes(0), "0 bytes");
        assert_eq!(bytes(1), "1 byte");
        assert_eq!(bytes(999), "999 bytes");
        assert_eq!(bytes(1000), "1 KB");
        assert_eq!(bytes(15_300), "15 KB");
        assert_eq!(bytes(2_500_000), "2.5 MB");
        assert_eq!(bytes(999_999_999), "1.0 GB");
        assert_eq!(bytes(3_000_000_000_000_000_000), "3000.0 PB");
    }

    #[test]
    fn progress_and_counts() {
        assert_eq!(progress(3, 12, 35), "3 kept · 12 reject · 35 left");
        assert_eq!(counts(0, 0), "Empty");
        assert_eq!(counts(1, 0), "1 photo");
        assert_eq!(counts(0, 2), "2 videos");
        assert_eq!(counts(2, 1), "2 photos · 1 video");
    }

    #[test]
    fn ranges() {
        let noon = |y, m, d| Local.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap().with_timezone(&Utc);
        assert_eq!(date_range(&noon(2026, 1, 5), &noon(2026, 1, 5)), "Jan 5, 2026");
        assert_eq!(date_range(&noon(2026, 1, 5), &noon(2026, 1, 7)), "Jan 5 – Jan 7, 2026");
        assert_eq!(date_range(&noon(2025, 12, 30), &noon(2026, 1, 2)), "Dec 30, 2025 – Jan 2, 2026");
        let past = unswiper_core::time::distant_past();
        assert_eq!(date_range(&past, &past), "");
    }
}
