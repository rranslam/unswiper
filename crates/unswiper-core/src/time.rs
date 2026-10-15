// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Timestamps as stored in `library.json`: ISO 8601, whole seconds, UTC (`Z`), exactly what
//! Swift's `JSONEncoder.DateEncodingStrategy.iso8601` writes and reads.

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Deserializer, Serializer};

/// A point in time (UTC, second precision when persisted).
pub type Timestamp = DateTime<Utc>;

/// Swift's `Date.distantPast` (0001-01-01T00:00:00Z), used as the "no date" placeholder.
pub fn distant_past() -> Timestamp {
    Utc.with_ymd_and_hms(1, 1, 1, 0, 0, 0).unwrap()
}

/// Formats like Swift's `ISO8601DateFormatter` default: `2026-10-13T17:47:00Z`.
pub fn format_iso(t: &Timestamp) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Parses an ISO 8601 internet date-time (with `Z` or an offset, no fractional seconds required).
pub fn parse_iso(s: &str) -> Option<Timestamp> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Commit stamp used for manifest names and quarantine folders: `2026-10-13T17-47-00Z`.
pub fn stamp(t: &Timestamp) -> String {
    format_iso(t).replace(':', "-")
}

/// Interprets a naive wall-clock time in the local zone (like Foundation's `DateFormatter`).
/// For a time skipped by DST the later mapping is used; for an ambiguous one, the earlier.
pub fn local_to_utc(n: NaiveDateTime) -> Option<Timestamp> {
    match Local.from_local_datetime(&n) {
        chrono::LocalResult::Single(t) => Some(t.with_timezone(&Utc)),
        chrono::LocalResult::Ambiguous(a, _) => Some(a.with_timezone(&Utc)),
        chrono::LocalResult::None => Local
            .from_local_datetime(&(n + chrono::Duration::hours(1)))
            .earliest()
            .map(|t| t.with_timezone(&Utc)),
    }
}

/// Noon local time on the given day.
pub fn local_noon(y: i32, m: u32, d: u32) -> Option<Timestamp> {
    let n = NaiveDate::from_ymd_opt(y, m, d)?.and_hms_opt(12, 0, 0)?;
    local_to_utc(n)
}

pub(crate) mod serde_ts {
    use super::*;
    pub fn serialize<S: Serializer>(t: &Timestamp, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format_iso(t))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Timestamp, D::Error> {
        let s = String::deserialize(d)?;
        parse_iso(&s).ok_or_else(|| serde::de::Error::custom(format!("bad ISO 8601 date: {s}")))
    }
}

pub(crate) mod serde_opt_ts {
    use super::*;
    pub fn serialize<S: Serializer>(t: &Option<Timestamp>, s: S) -> Result<S::Ok, S::Error> {
        match t {
            Some(t) => s.serialize_str(&format_iso(t)),
            None => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Timestamp>, D::Error> {
        let s = Option::<String>::deserialize(d)?;
        match s {
            None => Ok(None),
            Some(s) => parse_iso(&s)
                .map(Some)
                .ok_or_else(|| serde::de::Error::custom(format!("bad ISO 8601 date: {s}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_round_trip_and_stamp() {
        let t = Utc.with_ymd_and_hms(2026, 10, 13, 17, 47, 5).unwrap();
        assert_eq!(format_iso(&t), "2026-10-13T17:47:05Z");
        assert_eq!(parse_iso("2026-10-13T17:47:05Z"), Some(t));
        assert_eq!(parse_iso("2026-10-13T10:47:05-07:00"), Some(t));
        assert_eq!(stamp(&t), "2026-10-13T17-47-05Z");
        assert_eq!(parse_iso("nope"), None);
        assert_eq!(format_iso(&distant_past()), "0001-01-01T00:00:00Z");
        assert_eq!(parse_iso("0001-01-01T00:00:00Z"), Some(distant_past()));
    }

    #[test]
    fn local_noon_valid_and_invalid() {
        assert!(local_noon(2026, 1, 1).is_some());
        assert!(local_noon(2026, 2, 30).is_none());
    }

    #[test]
    fn local_to_utc_handles_gap() {
        // Whatever the zone, a valid naive time always maps somewhere.
        let n = NaiveDate::from_ymd_opt(2026, 3, 8)
            .unwrap()
            .and_hms_opt(2, 30, 0)
            .unwrap();
        assert!(local_to_utc(n).is_some());
    }
}
