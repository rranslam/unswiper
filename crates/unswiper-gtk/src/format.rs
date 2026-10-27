// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Text formatting shared by the views (mirrors the macOS app's strings).

use chrono::Local;
use unswiper_core::time::Timestamp;

/// File-size string like Foundation's `ByteCountFormatter` (`.file` style, decimal units).
pub fn bytes(n: i64) -> String {
    if n.abs() < 1000 {
        return if n == 1 {
            "1 byte".into()
        } else {
            format!("{n} bytes")
        };
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

/// `1,234` (thousands separators, for large counts in progress text).
pub fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Time left: `less than a minute left`, `about 8 min left`, `about 2 h 5 min left`.
pub fn time_left(d: std::time::Duration) -> String {
    let m = (d.as_secs() + 30) / 60;
    match m {
        0 => "less than a minute left".into(),
        1..=59 => format!("about {m} min left"),
        _ => format!("about {} h {} min left", m / 60, m % 60),
    }
}

/// Estimates time left from the rate since the last reset (start, resume, new pass).
#[derive(Debug, Clone, Default)]
pub struct Eta {
    base: Option<(std::time::Instant, usize)>,
    total: usize,
}

impl Eta {
    /// Starts measuring again from `done` (after a pause, or for a new pass).
    pub fn reset(&mut self) {
        self.base = None;
    }

    /// Time left, once there is enough to go on (2 s and a few items since the reset).
    pub fn update(
        &mut self,
        done: usize,
        total: usize,
        now: std::time::Instant,
    ) -> Option<std::time::Duration> {
        if total != self.total || self.base.is_some_and(|(_, d0)| done < d0) {
            self.total = total;
            self.base = None;
        }
        let (t0, d0) = *self.base.get_or_insert((now, done));
        let n = done.saturating_sub(d0);
        let el = now.saturating_duration_since(t0);
        if n < 3 || el < std::time::Duration::from_secs(2) || done >= total {
            return None;
        }
        Some(el.mul_f64((total - done) as f64 / n as f64))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn thousands_time_left_and_eta() {
        use super::*;
        use std::time::{Duration, Instant};
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(78_123), "78,123");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(
            time_left(Duration::from_secs(20)),
            "less than a minute left"
        );
        assert_eq!(time_left(Duration::from_secs(8 * 60)), "about 8 min left");
        assert_eq!(
            time_left(Duration::from_secs(125 * 60)),
            "about 2 h 5 min left"
        );
        let t = Instant::now();
        let mut e = Eta::default();
        assert_eq!(e.update(0, 100, t), None);
        assert_eq!(
            e.update(2, 100, t + Duration::from_secs(10)),
            None,
            "too few"
        );
        assert_eq!(
            e.update(10, 100, t + Duration::from_secs(1)),
            None,
            "too soon"
        );
        assert_eq!(
            e.update(10, 100, t + Duration::from_secs(10)),
            Some(Duration::from_secs(90))
        );
        assert_eq!(e.update(100, 100, t + Duration::from_secs(20)), None);
        // A new pass (different total) starts over.
        assert_eq!(e.update(5, 50, t + Duration::from_secs(30)), None);
    }

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
        let noon = |y, m, d| {
            Local
                .with_ymd_and_hms(y, m, d, 12, 0, 0)
                .unwrap()
                .with_timezone(&Utc)
        };
        assert_eq!(
            date_range(&noon(2026, 1, 5), &noon(2026, 1, 5)),
            "Jan 5, 2026"
        );
        assert_eq!(
            date_range(&noon(2026, 1, 5), &noon(2026, 1, 7)),
            "Jan 5 – Jan 7, 2026"
        );
        assert_eq!(
            date_range(&noon(2025, 12, 30), &noon(2026, 1, 2)),
            "Dec 30, 2025 – Jan 2, 2026"
        );
        let past = unswiper_core::time::distant_past();
        assert_eq!(date_range(&past, &past), "");
    }
}
