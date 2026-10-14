// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Capture date of a QuickTime/MP4 video, without ffmpeg:
//! 1. `com.apple.quicktime.creationdate` in `moov/meta` (`keys` + `ilst`), as iPhones write it;
//! 2. `moov/mvhd` creation time (seconds since 1904-01-01 UTC).
//!
//! Dates at or before 1980 are rejected (see [`crate::meta::usable`]).

use crate::meta::usable;
use crate::time::Timestamp;
use chrono::{DateTime, TimeZone, Utc};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Seconds between 1904-01-01 and 1970-01-01.
pub const MAC_EPOCH_OFFSET: i64 = 2_082_844_800;
const MAX_MOOV: u64 = 64 * 1024 * 1024;

/// Reads the capture date of a video file, or `None`.
pub fn video_date(path: &Path) -> Option<Timestamp> {
    let moov = read_moov(path)?;
    usable(apple_creation_date(&moov)).or_else(|| usable(mvhd_date(&moov)))
}

/// Iterates the child boxes of `data`: (type, payload).
fn boxes(data: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = vec![];
    let mut i = 0usize;
    while i + 8 <= data.len() {
        let size32 = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as u64;
        let typ: [u8; 4] = data[i + 4..i + 8].try_into().unwrap();
        let (header, size) = match size32 {
            1 => {
                if i + 16 > data.len() {
                    break;
                }
                (
                    16u64,
                    u64::from_be_bytes(data[i + 8..i + 16].try_into().unwrap()),
                )
            }
            0 => (8, (data.len() - i) as u64),
            n => (8, n),
        };
        if size < header || i as u64 + size > data.len() as u64 {
            break;
        }
        out.push((typ, &data[i + header as usize..i + size as usize]));
        i += size as usize;
    }
    out
}

/// Finds the top-level `moov` box by seeking past everything else (so `mdat` is never read).
fn read_moov(path: &Path) -> Option<Vec<u8>> {
    let mut f = File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut pos = 0u64;
    while pos + 8 <= len {
        f.seek(SeekFrom::Start(pos)).ok()?;
        let mut h = [0u8; 8];
        f.read_exact(&mut h).ok()?;
        let size32 = u32::from_be_bytes(h[0..4].try_into().unwrap()) as u64;
        let (header, size) = match size32 {
            1 => {
                let mut b = [0u8; 8];
                f.read_exact(&mut b).ok()?;
                (16, u64::from_be_bytes(b))
            }
            0 => (8, len - pos),
            n => (8, n),
        };
        if size < header {
            return None;
        }
        if &h[4..8] == b"moov" {
            let body = size - header;
            if body > MAX_MOOV || pos + size > len {
                return None;
            }
            let mut buf = vec![0u8; body as usize];
            f.read_exact(&mut buf).ok()?;
            return Some(buf);
        }
        pos += size;
    }
    None
}

/// `mvhd` creation time.
pub fn mvhd_date(moov: &[u8]) -> Option<Timestamp> {
    let (_, mvhd) = boxes(moov).into_iter().find(|(t, _)| t == b"mvhd")?;
    let version = *mvhd.first()?;
    let secs = if version == 1 {
        u64::from_be_bytes(mvhd.get(4..12)?.try_into().ok()?) as i64
    } else {
        u32::from_be_bytes(mvhd.get(4..8)?.try_into().ok()?) as i64
    };
    if secs == 0 {
        return None;
    }
    Utc.timestamp_opt(secs - MAC_EPOCH_OFFSET, 0).single()
}

/// `com.apple.quicktime.creationdate` from the QuickTime `meta` box.
pub fn apple_creation_date(moov: &[u8]) -> Option<Timestamp> {
    let (_, meta) = boxes(moov).into_iter().find(|(t, _)| t == b"meta")?;
    // QuickTime `meta` has no version/flags; ISO `meta` does. Detect by the first child.
    let meta =
        if meta.len() >= 8 && &meta[4..8] == b"hdlr" || meta.len() >= 8 && &meta[4..8] == b"keys" {
            meta
        } else {
            meta.get(4..)?
        };
    let children = boxes(meta);
    let keys = children.iter().find(|(t, _)| t == b"keys")?.1;
    let ilst = children.iter().find(|(t, _)| t == b"ilst")?.1;

    // keys: version/flags (4), count (4), then [size(4) namespace(4) name]
    let count = u32::from_be_bytes(keys.get(4..8)?.try_into().ok()?);
    let mut idx = None;
    let mut i = 8usize;
    for n in 1..=count {
        let size = u32::from_be_bytes(keys.get(i..i + 4)?.try_into().ok()?) as usize;
        if size < 8 {
            return None;
        }
        let name = keys.get(i + 8..i + size)?;
        if name == b"com.apple.quicktime.creationdate" {
            idx = Some(n);
            break;
        }
        i += size;
    }
    let idx = idx?;
    for (typ, item) in boxes(ilst) {
        if u32::from_be_bytes(typ) != idx {
            continue;
        }
        for (t, data) in boxes(item) {
            if &t == b"data" && data.len() > 8 {
                let s = std::str::from_utf8(&data[8..]).ok()?;
                return parse_apple_date(s.trim_end_matches('\0'));
            }
        }
    }
    None
}

/// Parses `2026-01-02T10:11:12-0700` / `…-07:00` / `…Z`.
pub fn parse_apple_date(s: &str) -> Option<Timestamp> {
    DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%z")
        .or_else(|_| DateTime::parse_from_rfc3339(s))
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

#[cfg(test)]
pub(crate) mod build {
    //! Tiny MP4/MOV builders used by tests.

    pub fn bx(typ: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut v = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(typ);
        v.extend_from_slice(payload);
        v
    }

    pub fn mvhd(mac_secs: u32) -> Vec<u8> {
        let mut p = vec![0, 0, 0, 0];
        p.extend_from_slice(&mac_secs.to_be_bytes());
        p.extend_from_slice(&[0u8; 92]);
        bx(b"mvhd", &p)
    }

    pub fn apple_meta(date: &str, iso_fullbox: bool) -> Vec<u8> {
        let key = b"com.apple.quicktime.creationdate";
        let mut keys = vec![0, 0, 0, 0, 0, 0, 0, 2];
        let other = b"com.apple.quicktime.make";
        keys.extend_from_slice(&((other.len() + 8) as u32).to_be_bytes());
        keys.extend_from_slice(b"mdta");
        keys.extend_from_slice(other);
        keys.extend_from_slice(&((key.len() + 8) as u32).to_be_bytes());
        keys.extend_from_slice(b"mdta");
        keys.extend_from_slice(key);
        let mut data = vec![0, 0, 0, 1, 0, 0, 0, 0];
        data.extend_from_slice(date.as_bytes());
        let item1 = bx(
            &1u32.to_be_bytes(),
            &bx(b"data", &[0, 0, 0, 1, 0, 0, 0, 0, b'A']),
        );
        let item2 = bx(&2u32.to_be_bytes(), &bx(b"data", &data));
        let mut ilst = item1;
        ilst.extend(item2);
        let mut body = if iso_fullbox {
            vec![0, 0, 0, 0]
        } else {
            vec![]
        };
        body.extend(bx(b"hdlr", &[0u8; 24]));
        body.extend(bx(b"keys", &keys));
        body.extend(bx(b"ilst", &ilst));
        bx(b"meta", &body)
    }

    pub fn file(moov_children: &[Vec<u8>], mdat_first: bool) -> Vec<u8> {
        let moov = bx(b"moov", &moov_children.concat());
        let ftyp = bx(b"ftyp", b"qt  \0\0\0\0qt  ");
        let mdat = bx(b"mdat", &[0u8; 64]);
        if mdat_first {
            [ftyp, mdat, moov].concat()
        } else {
            [ftyp, moov, mdat].concat()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build::*;
    use super::*;

    fn write(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("v.mov");
        std::fs::write(&p, bytes).unwrap();
        (d, p)
    }

    #[test]
    fn mvhd_epoch_conversion() {
        // 2026-01-01T00:00:00Z = 1767225600 unix.
        let mac = (1_767_225_600i64 + MAC_EPOCH_OFFSET) as u32;
        let (_d, p) = write(&file(&[mvhd(mac)], true));
        assert_eq!(video_date(&p).unwrap().timestamp(), 1_767_225_600);
    }

    #[test]
    fn mvhd_v1_and_zero() {
        let mut p = vec![1, 0, 0, 0];
        p.extend_from_slice(&((1_767_225_600i64 + MAC_EPOCH_OFFSET) as u64).to_be_bytes());
        p.extend_from_slice(&[0u8; 100]);
        assert_eq!(
            mvhd_date(&bx(b"mvhd", &p)).unwrap().timestamp(),
            1_767_225_600
        );
        assert_eq!(mvhd_date(&mvhd(0)), None);
        assert_eq!(mvhd_date(&bx(b"mvhd", &[0])), None);
        assert_eq!(mvhd_date(&[]), None);
    }

    #[test]
    fn apple_date_wins_over_mvhd() {
        for fullbox in [false, true] {
            let mac = (1_767_225_600i64 + MAC_EPOCH_OFFSET) as u32;
            let bytes = file(
                &[mvhd(mac), apple_meta("2026-03-04T10:00:00-0700", fullbox)],
                false,
            );
            let (_d, p) = write(&bytes);
            assert_eq!(
                video_date(&p).unwrap(),
                Utc.with_ymd_and_hms(2026, 3, 4, 17, 0, 0).unwrap()
            );
        }
    }

    #[test]
    fn apple_date_formats() {
        assert!(parse_apple_date("2026-03-04T10:00:00-07:00").is_some());
        assert!(parse_apple_date("2026-03-04T10:00:00Z").is_some());
        assert!(parse_apple_date("garbage").is_none());
    }

    #[test]
    fn old_dates_rejected_and_bad_files() {
        let (_d, p) = write(&file(&[mvhd(1000)], true));
        assert_eq!(video_date(&p), None);
        let (_d2, p2) = write(b"tiny");
        assert_eq!(video_date(&p2), None);
        let (_d3, p3) = write(&bx(b"ftyp", b"isom"));
        assert_eq!(video_date(&p3), None);
        assert_eq!(video_date(Path::new("/nonexistent.mov")), None);
        // size smaller than header
        let (_d4, p4) = write(&[0, 0, 0, 4, b'm', b'o', b'o', b'v']);
        assert_eq!(video_date(&p4), None);
        // moov claims more than the file
        let (_d5, p5) = write(&[0, 0, 1, 0, b'm', b'o', b'o', b'v', 0, 0]);
        assert_eq!(video_date(&p5), None);
    }

    #[test]
    fn largesize_and_size_zero_boxes() {
        let mac = (1_767_225_600i64 + MAC_EPOCH_OFFSET) as u32;
        // 64-bit size moov
        let inner = mvhd(mac);
        let mut moov = 1u32.to_be_bytes().to_vec();
        moov.extend_from_slice(b"moov");
        moov.extend_from_slice(&((inner.len() + 16) as u64).to_be_bytes());
        moov.extend_from_slice(&inner);
        let (_d, p) = write(&moov);
        assert!(video_date(&p).is_some());
        // size 0 = to end of file
        let mut moov0 = 0u32.to_be_bytes().to_vec();
        moov0.extend_from_slice(b"moov");
        moov0.extend_from_slice(&inner);
        let (_d2, p2) = write(&moov0);
        assert!(video_date(&p2).is_some());
        // child-box iterator with largesize and truncated
        let mut big = 1u32.to_be_bytes().to_vec();
        big.extend_from_slice(b"mvhd");
        big.extend_from_slice(&24u64.to_be_bytes());
        big.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(boxes(&big).len(), 1);
        assert_eq!(boxes(&big[..12]).len(), 0);
        assert_eq!(boxes(&[0, 0, 0, 0, b'a', b'b', b'c', b'd']).len(), 1);
    }

    #[test]
    fn meta_without_key_or_bad_keys() {
        assert_eq!(apple_creation_date(&[]), None);
        let meta = bx(
            b"meta",
            &[
                bx(b"hdlr", &[0u8; 24]),
                bx(b"keys", &[0, 0, 0, 0, 0, 0, 0, 0]),
                bx(b"ilst", &[]),
            ]
            .concat(),
        );
        assert_eq!(apple_creation_date(&meta), None);
        let bad = bx(
            b"meta",
            &[
                bx(b"keys", &[0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2]),
                bx(b"ilst", &[]),
            ]
            .concat(),
        );
        assert_eq!(apple_creation_date(&bad), None);
    }
}
