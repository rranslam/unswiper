// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Metadata: dates (EXIF/TIFF for stills, QuickTime/MP4 for videos, folder names; plan §7) and the
//! photo info view (plan §17).

pub mod exif;
pub mod folder_date;
pub mod info;
pub mod quicktime;

use crate::time::Timestamp;

/// Dates at or before 1980-01-01T00:00:00Z (zeroed/epoch timestamps) are treated as missing.
pub fn usable(t: Option<Timestamp>) -> Option<Timestamp> {
    t.filter(|t| t.timestamp() > 315_532_800)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn usable_cutoff() {
        assert_eq!(usable(None), None);
        assert_eq!(
            usable(Some(Utc.timestamp_opt(315_532_800, 0).unwrap())),
            None
        );
        let ok = Utc.timestamp_opt(315_532_801, 0).unwrap();
        assert_eq!(usable(Some(ok)), Some(ok));
    }
}
