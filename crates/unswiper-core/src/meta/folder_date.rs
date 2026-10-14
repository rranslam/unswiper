// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Port of `FolderDate.swift`: dates from folder names (plan §7).
//!
//! Scanning from the deepest folder upward:
//! - consecutive components `YYYY/MM/DD`, `YYYY/MM`, `YYYY` (month/day may be 1 or 2 digits);
//! - one component `YYYY-MM-DD`, `YYYY_MM_DD`, `YYYY.MM.DD`, `YYYYMMDD` or `YYYY-MM`,
//!   optionally followed by whitespace and a name (`2025-07-04 Beach`, `1999 Photos`).
//!
//! Years 1980–2099; dates must exist. Only whole components or a leading token count.

use crate::time::{local_noon, Timestamp};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Precision {
    Day,
    Month,
    Year,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub precision: Precision,
}

impl Match {
    fn new(year: i32, month: u32, day: u32, precision: Precision) -> Self {
        Match {
            year,
            month,
            day,
            precision,
        }
    }

    /// Noon local time on the first day of the span.
    pub fn date(&self) -> Timestamp {
        local_noon(self.year, self.month, self.day).expect("validated date")
    }
}

/// `components` are folder names from the library root down to the file's folder.
pub fn parse_components<S: AsRef<str>>(components: &[S]) -> Option<Match> {
    let c: Vec<&str> = components.iter().map(|s| s.as_ref()).collect();
    for k in (0..c.len()).rev() {
        if k >= 2 {
            if let (Some(y), Some(m), Some(d)) =
                (year(c[k - 2]), month(c[k - 1]), number(c[k], 1, 2))
            {
                if valid(y, m, d) {
                    return Some(Match::new(y, m, d, Precision::Day));
                }
            }
        }
        if k >= 1 {
            if let (Some(y), Some(m)) = (year(c[k - 1]), month(c[k])) {
                return Some(Match::new(y, m, 1, Precision::Month));
            }
        }
        if let Some(m) = single(c[k]) {
            return Some(m);
        }
    }
    None
}

/// Parses a relative folder path such as `2026/01/01` or `Trips/2025-07-04 Beach`.
pub fn parse(relative_folder: &str) -> Option<Match> {
    let comps: Vec<&str> = relative_folder
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    parse_components(&comps)
}

/// One folder name: the whole component or its leading whitespace-separated token.
pub fn single(component: &str) -> Option<Match> {
    let trimmed = component.trim_matches(|c: char| c == ' ' || c == '\t');
    let token = trimmed.split([' ', '\t']).find(|s| !s.is_empty())?;

    if let Some(y) = year(token) {
        return Some(Match::new(y, 1, 1, Precision::Year));
    }
    if token.len() == 8 && token.bytes().all(|b| b.is_ascii_digit()) {
        let y: i32 = token[0..4].parse().ok()?;
        let m: u32 = token[4..6].parse().ok()?;
        let d: u32 = token[6..8].parse().ok()?;
        return valid(y, m, d).then(|| Match::new(y, m, d, Precision::Day));
    }
    for sep in ['-', '_', '.'] {
        let parts: Vec<&str> = token.split(sep).collect();
        if parts.len() == 3 {
            if let (Some(y), Some(m), Some(d)) =
                (year(parts[0]), month(parts[1]), number(parts[2], 1, 2))
            {
                if valid(y, m, d) {
                    return Some(Match::new(y, m, d, Precision::Day));
                }
            }
        }
        if sep == '-' && parts.len() == 2 {
            if let (Some(y), Some(m)) = (year(parts[0]), month(parts[1])) {
                return Some(Match::new(y, m, 1, Precision::Month));
            }
        }
    }
    None
}

fn number(s: &str, min: usize, max: usize) -> Option<u32> {
    let n = s.chars().count();
    if n < min || n > max || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn year(s: &str) -> Option<i32> {
    let y = number(s, 4, 4)? as i32;
    (1980..=2099).contains(&y).then_some(y)
}

fn month(s: &str) -> Option<u32> {
    let m = number(s, 1, 2)?;
    (1..=12).contains(&m).then_some(m)
}

/// Year 1980–2099 and a day that exists in that month.
pub fn valid(y: i32, m: u32, d: u32) -> bool {
    if !(1980..=2099).contains(&y) || !(1..=12).contains(&m) || d < 1 {
        return false;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    d <= days[(m - 1) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use Precision::*;

    fn m(y: i32, mo: u32, d: u32, p: Precision) -> Option<Match> {
        Some(Match::new(y, mo, d, p))
    }

    /// Parser table (plan §7: 60+ cases).
    #[test]
    fn parser_table() {
        let cases: Vec<(&str, Option<Match>)> = vec![
            ("2026/01/01", m(2026, 1, 1, Day)),
            ("2026/1/5", m(2026, 1, 5, Day)),
            ("2026/12/31", m(2026, 12, 31, Day)),
            ("2026/01", m(2026, 1, 1, Month)),
            ("2026/7", m(2026, 7, 1, Month)),
            ("2026", m(2026, 1, 1, Year)),
            ("1980", m(1980, 1, 1, Year)),
            ("2099", m(2099, 1, 1, Year)),
            ("1979", None),
            ("2100", None),
            ("0000", None),
            ("1999", m(1999, 1, 1, Year)),
            ("1999 Photos", m(1999, 1, 1, Year)),
            ("2026/01/01/extra", m(2026, 1, 1, Day)),
            ("Photos/2026/02/03", m(2026, 2, 3, Day)),
            ("2024/02/29", m(2024, 2, 29, Day)),
            ("2023/02/29", m(2023, 2, 1, Month)),
            ("2000/02/29", m(2000, 2, 29, Day)),
            ("2026/04/31", m(2026, 4, 1, Month)),
            ("2026/13/01", m(2026, 1, 1, Year)),
            ("2026/00", m(2026, 1, 1, Year)),
            ("2026-01-01", m(2026, 1, 1, Day)),
            ("2026_01_01", m(2026, 1, 1, Day)),
            ("2026.01.01", m(2026, 1, 1, Day)),
            ("20260101", m(2026, 1, 1, Day)),
            ("2026-01", m(2026, 1, 1, Month)),
            ("2026-1-5", m(2026, 1, 5, Day)),
            ("2025-07-04 Beach", m(2025, 7, 4, Day)),
            ("Trips/2025-07-04 Beach", m(2025, 7, 4, Day)),
            ("2025-07-04\tBeach", m(2025, 7, 4, Day)),
            ("  2025-07-04 Beach", m(2025, 7, 4, Day)),
            ("2026-01-01 Lisbon/raw", m(2026, 1, 1, Day)),
            ("Trip 2026-01-01", None),
            ("IMG_20260101", None),
            ("IMG_2026", None),
            ("v2026", None),
            ("12345", None),
            ("2026-13-40", None),
            ("2026-02-30", None),
            ("2024-02-29", m(2024, 2, 29, Day)),
            ("2023-02-29", None),
            ("20230229", None),
            ("20261301", None),
            ("202601011", None),
            ("2026010", None),
            ("2026_01", None),
            ("2026.01", None),
            ("2026-01-01-02", None),
            ("2026-01_01", None),
            ("2026-001-01", None),
            ("2026-01-001", None),
            ("19790101", None),
            ("21000101", None),
            ("Lisbon", None),
            ("", None),
            ("Ünïcødé/Été", None),
            ("Été/2026-08-01 Côte", m(2026, 8, 1, Day)),
            ("２０２６", None),
            ("2026/Lisbon", m(2026, 1, 1, Year)),
            ("Lisbon/2026", m(2026, 1, 1, Year)),
            ("2025/Trip/2026-03-04", m(2026, 3, 4, Day)),
            ("2026/03/Trip", m(2026, 3, 1, Month)),
            ("2026/3/x/y", m(2026, 3, 1, Month)),
            ("a/b/c", None),
            ("1999 Photos/Lisbon", m(1999, 1, 1, Year)),
            ("2026-1", m(2026, 1, 1, Month)),
            ("2026-12 December", m(2026, 12, 1, Month)),
        ];
        assert!(cases.len() >= 60);
        for (input, want) in cases {
            assert_eq!(parse(input), want, "input {input:?}");
        }
    }

    #[test]
    fn components_api_and_empty() {
        assert_eq!(parse_components::<&str>(&[]), None);
        assert_eq!(parse_components(&["2026", "01", "02"]), m(2026, 1, 2, Day));
        assert_eq!(single("   "), None);
    }

    #[test]
    fn noon_local() {
        let d = Match::new(2026, 1, 1, Day).date();
        let local = d.with_timezone(&chrono::Local);
        assert_eq!(
            local.format("%Y-%m-%d %H:%M").to_string(),
            "2026-01-01 12:00"
        );
    }

    #[test]
    fn validity() {
        assert!(valid(2024, 2, 29));
        assert!(!valid(1900, 2, 29));
        assert!(!valid(2026, 0, 1));
        assert!(!valid(2026, 1, 0));
        assert!(!valid(2026, 1, 32));
    }

    mod props {
        use super::super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn valid_dates_round_trip(y in 1980i32..=2099, m in 1u32..=12, d in 1u32..=31) {
                prop_assume!(valid(y, m, d));
                let want = Some(Match::new(y, m, d, Precision::Day));
                prop_assert_eq!(parse(&format!("{y:04}-{m:02}-{d:02}")), want);
                prop_assert_eq!(parse(&format!("{y:04}_{m:02}_{d:02}")), want);
                prop_assert_eq!(parse(&format!("{y:04}.{m:02}.{d:02}")), want);
                prop_assert_eq!(parse(&format!("{y:04}{m:02}{d:02}")), want);
                prop_assert_eq!(parse(&format!("{y:04}/{m:02}/{d:02}")), want);
                prop_assert_eq!(parse(&format!("{y:04}/{m}/{d}")), want);
                prop_assert_eq!(parse(&format!("{y:04}-{m:02}-{d:02} Some Name")), want);
            }

            #[test]
            fn letters_never_parse(s in "[A-Za-z _]{0,20}") {
                prop_assert_eq!(parse(&s), None);
            }

            #[test]
            fn prefixed_never_parse(p in "[A-Za-z]{1,5}", y in 1980i32..=2099) {
                prop_assert_eq!(parse(&format!("{p}{y}")), None);
                prop_assert_eq!(parse(&format!("{p}_{y}0101")), None);
            }
        }
    }
}
