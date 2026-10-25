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
    let raw = apple_key(moov, "com.apple.quicktime.creationdate")?;
    parse_apple_date(&raw)
}

/// All string values of the QuickTime `moov/meta` (`keys` + `ilst`) as (key, value), in key order.
/// iPhones write `com.apple.quicktime.make`, `.model`, `.creationdate`, `.location.ISO6709` here.
pub fn apple_keys(moov: &[u8]) -> Vec<(String, String)> {
    apple_keys_opt(moov).unwrap_or_default()
}

fn apple_key(moov: &[u8], key: &str) -> Option<String> {
    apple_keys(moov)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

fn apple_keys_opt(moov: &[u8]) -> Option<Vec<(String, String)>> {
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
    let mut names = vec![];
    let mut i = 8usize;
    for _ in 1..=count {
        let size = u32::from_be_bytes(keys.get(i..i + 4)?.try_into().ok()?) as usize;
        if size < 8 {
            break;
        }
        let Some(name) = keys.get(i + 8..i + size) else {
            break;
        };
        names.push(String::from_utf8_lossy(name).into_owned());
        i += size;
    }
    let mut out = vec![];
    for (typ, item) in boxes(ilst) {
        let idx = u32::from_be_bytes(typ) as usize;
        let Some(name) = idx.checked_sub(1).and_then(|n| names.get(n)) else {
            continue;
        };
        for (t, data) in boxes(item) {
            // data: type indicator (4; 1 = UTF-8), locale (4), value
            if &t == b"data" && data.len() > 8 && data[3] == 1 {
                if let Ok(s) = std::str::from_utf8(&data[8..]) {
                    out.push((name.clone(), s.trim_end_matches('\0').to_string()));
                }
            }
        }
    }
    Some(out)
}

/// The ISO 6709 location in `udta`: QuickTime `udta/©xyz` (length, language, text), or the
/// MP4 / iTunes-style `udta/meta/ilst/©xyz/data` that Android phones and ffmpeg write.
fn udta_xyz(udta: &[u8]) -> Option<String> {
    let children = boxes(udta);
    let plain = children
        .iter()
        .find(|(t, _)| t == b"\xA9xyz")
        .and_then(|(_, p)| {
            let n = u16::from_be_bytes(p.get(0..2)?.try_into().ok()?) as usize;
            std::str::from_utf8(p.get(4..4 + n)?)
                .ok()
                .map(str::to_string)
        });
    plain.or_else(|| {
        let (_, meta) = children.iter().find(|(t, _)| t == b"meta")?;
        // ISO `meta` has version/flags before its children; QuickTime `meta` doesn't.
        let meta = if meta.get(4..8) == Some(b"hdlr") {
            &meta[..]
        } else {
            meta.get(4..)?
        };
        let ilst = boxes(meta).into_iter().find(|(t, _)| t == b"ilst")?.1;
        let item = boxes(ilst).into_iter().find(|(t, _)| t == b"\xA9xyz")?.1;
        let data = boxes(item).into_iter().find(|(t, _)| t == b"data")?.1;
        // data: type indicator (4), locale (4), UTF-8 text
        std::str::from_utf8(data.get(8..)?)
            .ok()
            .map(|s| s.trim_end_matches('\0').to_string())
    })
}

/// Technical facts of a video for the photo info view (plan §17.5). Every field is optional.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VideoFacts {
    pub duration_s: Option<f64>,
    /// Display size (after the track's rotation).
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Sample-entry FourCC of the first video track, e.g. `hvc1`, `avc1`.
    pub codec: Option<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    /// Capture date as written, with its offset when the file has one.
    pub created: Option<chrono::DateTime<chrono::FixedOffset>>,
    /// `true` when `created` came from `mvhd` (UTC, no local offset known).
    pub created_is_utc: bool,
    /// ISO 6709 location string, as stored (parse with [`parse_iso6709`]).
    pub location: Option<String>,
}

/// Reads [`VideoFacts`] from a QuickTime/MP4 file, or `None` when it has no `moov`.
pub fn video_facts(path: &Path) -> Option<VideoFacts> {
    read_moov(path).map(|m| facts_from_moov(&m))
}

/// [`VideoFacts`] from the payload of a `moov` box.
pub fn facts_from_moov(moov: &[u8]) -> VideoFacts {
    let keys = apple_keys(moov);
    let key = |k: &str| {
        keys.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let mut f = VideoFacts {
        make: key("com.apple.quicktime.make"),
        model: key("com.apple.quicktime.model"),
        location: key("com.apple.quicktime.location.ISO6709"),
        ..Default::default()
    };
    let top = boxes(moov);
    if let Some((_, mvhd)) = top.iter().find(|(t, _)| t == b"mvhd") {
        f.duration_s = mvhd_duration(mvhd);
    }
    let dated = key("com.apple.quicktime.creationdate").and_then(|s| {
        DateTime::parse_from_str(&s, "%Y-%m-%dT%H:%M:%S%z")
            .or_else(|_| DateTime::parse_from_rfc3339(&s))
            .ok()
    });
    if let Some(d) = dated.filter(|d| usable(Some(d.with_timezone(&Utc))).is_some()) {
        f.created = Some(d);
    } else if let Some(d) = usable(mvhd_date(moov)) {
        f.created = Some(d.fixed_offset());
        f.created_is_utc = true;
    }
    if f.location.is_none() {
        f.location = top
            .iter()
            .find(|(t, _)| t == b"udta")
            .and_then(|(_, u)| udta_xyz(u));
    }
    for (_, trak) in top.iter().filter(|(t, _)| t == b"trak") {
        let tb = boxes(trak);
        let Some((_, mdia)) = tb.iter().find(|(t, _)| t == b"mdia") else {
            continue;
        };
        let mb = boxes(mdia);
        let is_video = mb
            .iter()
            .find(|(t, _)| t == b"hdlr")
            .is_some_and(|(_, h)| h.get(8..12) == Some(b"vide"));
        if !is_video {
            continue;
        }
        if let Some((_, tkhd)) = tb.iter().find(|(t, _)| t == b"tkhd") {
            if let Some((w, h)) = tkhd_size(tkhd) {
                f.width = Some(w);
                f.height = Some(h);
            }
        }
        f.codec = mb
            .iter()
            .find(|(t, _)| t == b"minf")
            .and_then(|(_, m)| boxes(m).into_iter().find(|(t, _)| t == b"stbl"))
            .and_then(|(_, s)| boxes(s).into_iter().find(|(t, _)| t == b"stsd"))
            .and_then(|(_, s)| s.get(12..16))
            .map(|c| String::from_utf8_lossy(c).trim().to_string())
            .filter(|c| !c.is_empty());
        break;
    }
    f
}

/// Duration from an `mvhd` payload.
fn mvhd_duration(mvhd: &[u8]) -> Option<f64> {
    let (scale, dur) = if *mvhd.first()? == 1 {
        (
            u32::from_be_bytes(mvhd.get(20..24)?.try_into().ok()?),
            u64::from_be_bytes(mvhd.get(24..32)?.try_into().ok()?),
        )
    } else {
        (
            u32::from_be_bytes(mvhd.get(12..16)?.try_into().ok()?),
            u32::from_be_bytes(mvhd.get(16..20)?.try_into().ok()?) as u64,
        )
    };
    (scale > 0 && dur > 0 && dur != u32::MAX as u64 && dur != u64::MAX)
        .then(|| dur as f64 / scale as f64)
}

/// Display width/height from a `tkhd` payload (16.16 fixed point at the end), swapped when the
/// matrix rotates by 90° (portrait iPhone videos).
fn tkhd_size(tkhd: &[u8]) -> Option<(u32, u32)> {
    let n = tkhd.len();
    if n < 8 {
        return None;
    }
    let w = u32::from_be_bytes(tkhd[n - 8..n - 4].try_into().ok()?) >> 16;
    let h = u32::from_be_bytes(tkhd[n - 4..n].try_into().ok()?) >> 16;
    if w == 0 || h == 0 {
        return None;
    }
    let m = if *tkhd.first()? == 1 { 52 } else { 40 };
    let a = i32::from_be_bytes(tkhd.get(m..m + 4)?.try_into().ok()?);
    let b = i32::from_be_bytes(tkhd.get(m + 4..m + 8)?.try_into().ok()?);
    Some(if a == 0 && b != 0 { (h, w) } else { (w, h) })
}

/// Parses an ISO 6709 string in decimal degrees, e.g. `+37.7749-122.4194+012.345/`, into
/// (latitude, longitude, altitude in metres). Degree-minute forms are not supported (`None`).
pub fn parse_iso6709(s: &str) -> Option<(f64, f64, Option<f64>)> {
    let s = s.trim().trim_end_matches('/');
    let mut parts = vec![];
    let mut cur = String::new();
    for c in s.chars() {
        if (c == '+' || c == '-') && !cur.is_empty() {
            parts.push(std::mem::take(&mut cur));
        }
        if !(c == '+' || c == '-' || c == '.' || c.is_ascii_digit()) {
            break; // e.g. "CRSWGS_84"
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    if parts.len() < 2 || !parts.iter().all(|p| p.starts_with(['+', '-'])) {
        return None;
    }
    let int_digits = |p: &str| p[1..].split('.').next().map_or(0, str::len);
    if int_digits(&parts[0]) != 2 || int_digits(&parts[1]) != 3 {
        return None;
    }
    let lat: f64 = parts[0].parse().ok()?;
    let lon: f64 = parts[1].parse().ok()?;
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    let alt = parts.get(2).and_then(|a| a.parse::<f64>().ok());
    Some((lat, lon, alt))
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

    /// QuickTime `meta` with arbitrary UTF-8 keys.
    pub fn apple_meta_keys(kv: &[(&str, &str)]) -> Vec<u8> {
        let mut keys = vec![0, 0, 0, 0];
        keys.extend_from_slice(&(kv.len() as u32).to_be_bytes());
        let mut ilst = vec![];
        for (n, (k, v)) in kv.iter().enumerate() {
            keys.extend_from_slice(&((k.len() + 8) as u32).to_be_bytes());
            keys.extend_from_slice(b"mdta");
            keys.extend_from_slice(k.as_bytes());
            let mut data = vec![0, 0, 0, 1, 0, 0, 0, 0];
            data.extend_from_slice(v.as_bytes());
            ilst.extend(bx(&(n as u32 + 1).to_be_bytes(), &bx(b"data", &data)));
        }
        let mut body = bx(b"hdlr", &[0u8; 24]);
        body.extend(bx(b"keys", &keys));
        body.extend(bx(b"ilst", &ilst));
        bx(b"meta", &body)
    }

    /// `mvhd` v0 with a timescale and duration.
    pub fn mvhd_dur(mac_secs: u32, scale: u32, dur: u32) -> Vec<u8> {
        let mut p = vec![0, 0, 0, 0];
        p.extend_from_slice(&mac_secs.to_be_bytes());
        p.extend_from_slice(&mac_secs.to_be_bytes());
        p.extend_from_slice(&scale.to_be_bytes());
        p.extend_from_slice(&dur.to_be_bytes());
        p.extend_from_slice(&[0u8; 80]);
        bx(b"mvhd", &p)
    }

    /// A video `trak` with a tkhd size, optional 90° rotation, and a sample entry FourCC.
    pub fn video_trak(
        w: u32,
        h: u32,
        rotated: bool,
        fourcc: &[u8; 4],
        handler: &[u8; 4],
    ) -> Vec<u8> {
        let mut t = vec![0u8; 84];
        let (a, b): (i32, i32) = if rotated { (0, 0x10000) } else { (0x10000, 0) };
        t[40..44].copy_from_slice(&a.to_be_bytes());
        t[44..48].copy_from_slice(&b.to_be_bytes());
        t[76..80].copy_from_slice(&(w << 16).to_be_bytes());
        t[80..84].copy_from_slice(&(h << 16).to_be_bytes());
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(handler);
        hdlr.extend_from_slice(&[0u8; 13]);
        let mut stsd = vec![0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 16];
        stsd.extend_from_slice(fourcc);
        stsd.extend_from_slice(&[0u8; 8]);
        let stbl = bx(b"stbl", &bx(b"stsd", &stsd));
        let minf = bx(b"minf", &stbl);
        let mdia = bx(b"mdia", &[bx(b"hdlr", &hdlr), minf].concat());
        bx(b"trak", &[bx(b"tkhd", &t), mdia].concat())
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

    #[test]
    fn facts_from_iphone_like_moov() {
        let mac = (1_767_225_600i64 + MAC_EPOCH_OFFSET) as u32;
        let (_d, p) = write(&file(
            &[
                mvhd_dur(mac, 600, 4500),
                bx(b"trak", &bx(b"tkhd", &[0u8; 84])), // no mdia: skipped
                video_trak(1920, 1080, false, b"soun", b"soun"),
                video_trak(1920, 1080, true, b"hvc1", b"vide"),
                apple_meta_keys(&[
                    ("com.apple.quicktime.make", "Apple"),
                    ("com.apple.quicktime.model", "iPhone 15 Pro"),
                    (
                        "com.apple.quicktime.location.ISO6709",
                        "+37.7749-122.4194+012.300/",
                    ),
                    (
                        "com.apple.quicktime.creationdate",
                        "2026-03-04T10:00:00-0700",
                    ),
                ]),
            ],
            true,
        ));
        let f = video_facts(&p).unwrap();
        assert_eq!(f.duration_s, Some(7.5));
        assert_eq!((f.width, f.height), (Some(1080), Some(1920)));
        assert_eq!(f.codec.as_deref(), Some("hvc1"));
        assert_eq!(f.make.as_deref(), Some("Apple"));
        assert_eq!(f.model.as_deref(), Some("iPhone 15 Pro"));
        assert_eq!(f.created.unwrap().offset().local_minus_utc(), -7 * 3600);
        assert!(!f.created_is_utc);
        let (lat, lon, alt) = parse_iso6709(f.location.as_deref().unwrap()).unwrap();
        assert_eq!((lat, lon, alt), (37.7749, -122.4194, Some(12.3)));
    }

    #[test]
    fn facts_fallbacks_mvhd_date_and_udta_xyz() {
        let mac = (1_767_225_600i64 + MAC_EPOCH_OFFSET) as u32;
        let mut xyz = vec![0, 18, 0x15, 0xc7];
        xyz.extend_from_slice(b"-33.8688+151.2093/");
        let udta = bx(b"udta", &bx(b"\xA9xyz", &xyz));
        let f = facts_from_moov(
            &[
                mvhd(mac),
                udta,
                video_trak(640, 480, false, b"avc1", b"vide"),
            ]
            .concat(),
        );
        assert!(f.created_is_utc);
        assert_eq!(f.created.unwrap().timestamp(), 1_767_225_600);
        assert_eq!(f.duration_s, None); // mvhd() has no timescale
        assert_eq!((f.width, f.height), (Some(640), Some(480)));
        assert_eq!(
            parse_iso6709(f.location.as_deref().unwrap()),
            Some((-33.8688, 151.2093, None))
        );
        assert_eq!(facts_from_moov(&[]), VideoFacts::default());
        assert_eq!(video_facts(Path::new("/nonexistent.mov")), None);
    }

    #[test]
    fn mp4_ilst_xyz_location() {
        // udta/meta (ISO, version+flags)/hdlr + ilst/©xyz/data, as Android and ffmpeg write it.
        let mut data = vec![0, 0, 0, 1, 0, 0, 0, 0];
        data.extend_from_slice(b"+12.3456-065.4321/");
        let ilst = bx(b"ilst", &bx(b"\xA9xyz", &bx(b"data", &data)));
        let mut meta = vec![0, 0, 0, 0];
        meta.extend(bx(b"hdlr", &[0u8; 25]));
        meta.extend(ilst);
        let udta = bx(b"udta", &bx(b"meta", &meta));
        let f = facts_from_moov(&udta);
        assert_eq!(
            parse_iso6709(f.location.as_deref().unwrap()),
            Some((12.3456, -65.4321, None))
        );
        // A udta without any location stays None.
        assert_eq!(facts_from_moov(&bx(b"udta", &[])).location, None);
    }

    #[test]
    fn mvhd_v1_duration_and_bad_values() {
        let mut p = vec![1, 0, 0, 0];
        p.extend_from_slice(&[0u8; 16]);
        p.extend_from_slice(&1000u32.to_be_bytes());
        p.extend_from_slice(&2500u64.to_be_bytes());
        assert_eq!(mvhd_duration(&p), Some(2.5));
        assert_eq!(mvhd_duration(&[0u8; 20]), None);
        assert_eq!(mvhd_duration(&[]), None);
        assert_eq!(tkhd_size(&[0u8; 84]), None);
        assert_eq!(tkhd_size(&[0u8; 4]), None);
    }

    #[test]
    fn iso6709_forms() {
        assert_eq!(
            parse_iso6709("+48.8577+002.2950/"),
            Some((48.8577, 2.295, None))
        );
        assert_eq!(
            parse_iso6709("-01.5+036.8+1700.5CRSWGS_84/"),
            Some((-1.5, 36.8, Some(1700.5)))
        );
        assert_eq!(parse_iso6709("+4851.46+00217.70/"), None); // degree-minute form
        assert_eq!(parse_iso6709("+95.0+010.0/"), None);
        assert_eq!(parse_iso6709("hello"), None);
        assert_eq!(parse_iso6709(""), None);
        assert_eq!(parse_iso6709("+48.8"), None);
    }

    #[test]
    fn apple_keys_skips_non_utf8_and_unknown_indexes() {
        let mut meta = apple_meta_keys(&[("a", "1")]);
        assert_eq!(apple_keys(&meta), vec![("a".to_string(), "1".to_string())]);
        // flip the data type indicator to 23 (float): ignored
        let pos = meta.windows(4).position(|w| w == b"data").unwrap();
        meta[pos + 7] = 23;
        assert!(apple_keys(&meta).is_empty());
    }
}
