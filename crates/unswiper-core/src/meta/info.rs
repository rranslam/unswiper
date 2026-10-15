// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Photo info (plan §17): camera, exposure, date, size and location of one photo or video, plus a
//! shared formatter that turns it into labelled rows. Linux uses it directly; the Mac app gets the
//! same rows from the bundled helper as JSON, so both apps show identical wording.
//!
//! Privacy: nothing here writes anywhere. GPS is read and returned only to the caller, which shows
//! it locally (it is never stored in library.json or the quality cache).

use crate::meta::quicktime;
use chrono::{DateTime, FixedOffset, NaiveDateTime};
use exif::{Exif, In, Tag, Value};
use serde::{Deserialize, Serialize};
use std::io::{BufReader, Read};
use std::path::Path;

/// A location.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Gps {
    pub lat: f64,
    pub lon: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt_m: Option<f64>,
}

/// Capture date as the camera wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Taken {
    /// Local wall-clock time.
    pub local: NaiveDateTime,
    /// Offset from UTC in minutes, when the file records it (EXIF `OffsetTimeOriginal`,
    /// QuickTime creation date). `None` = time zone unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset_min: Option<i32>,
}

/// Everything the info view knows about one file. All fields are optional: missing tags stay `None`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaInfo {
    pub is_video: bool,
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    pub focal_mm: Option<f64>,
    pub focal_35mm: Option<u32>,
    pub f_number: Option<f64>,
    pub exposure_s: Option<f64>,
    pub iso: Option<u32>,
    pub flash: Option<bool>,
    pub exposure_bias_ev: Option<f64>,
    pub taken: Option<Taken>,
    /// Display size: after the EXIF orientation / video rotation.
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub orientation: Option<u16>,
    pub duration_s: Option<f64>,
    pub codec: Option<String>,
    pub gps: Option<Gps>,
}

/// File-level facts the caller knows (from the index).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileFacts {
    pub name: String,
    pub size: Option<u64>,
    pub folder: Option<String>,
    /// Live Photo motion / RAW+JPEG partners: (file name, size).
    pub sidecars: Vec<(String, u64)>,
}

impl FileFacts {
    /// Name, size and folder of `path` from the file system.
    pub fn of(path: &Path) -> FileFacts {
        FileFacts {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            size: std::fs::metadata(path).ok().map(|m| m.len()),
            folder: path.parent().map(|p| p.display().to_string()),
            sidecars: vec![],
        }
    }
}

/// The blur check result in words (Rob, 15 Oct 2026: words, not numbers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sharpness {
    Sharp,
    Soft,
    Blurry,
    /// Video, screenshot, too dark or not scored yet.
    NotChecked,
}

impl Sharpness {
    pub fn label(self) -> &'static str {
        match self {
            Sharpness::Sharp => "Sharp",
            Sharpness::Soft => "Soft",
            Sharpness::Blurry => "Blurry",
            Sharpness::NotChecked => "Not checked",
        }
    }
}

/// Row groups, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    Camera,
    Exposure,
    Date,
    Image,
    File,
    Location,
    Unswiper,
}

impl Group {
    pub fn title(self) -> &'static str {
        match self {
            Group::Camera => "Camera",
            Group::Exposure => "Exposure",
            Group::Date => "Date",
            Group::Image => "Image",
            Group::File => "File",
            Group::Location => "Location",
            Group::Unswiper => "Unswiper",
        }
    }
}

/// One formatted row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub group: Group,
    pub label: String,
    pub value: String,
    /// Shown in the compact right-click popover too (the inspector shows every row).
    pub short: bool,
}

/// Formatter options.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatOptions {
    /// Setting "Show Location in Photo Info" (default on).
    pub show_location: bool,
    pub sharpness: Option<Sharpness>,
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions {
            show_location: true,
            sharpness: None,
        }
    }
}

// ---------------------------------------------------------------- reading

const SNIFF: usize = 64 * 1024;

/// Reads the info of a photo or video. Unreadable or untagged files give an (almost) empty
/// [`MediaInfo`], never an error.
pub fn read(path: &Path) -> MediaInfo {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(ext.as_str(), "mov" | "mp4" | "m4v" | "3gp") {
        return video_info(path);
    }
    let mut info = photo_info(path);
    if info.width.is_none() {
        if let Some((w, h)) = header_size(path) {
            let (w, h) = oriented(w, h, info.orientation);
            info.width = Some(w);
            info.height = Some(h);
        }
    }
    info
}

/// Reads EXIF from a JPEG, TIFF/DNG, HEIF/HEIC, PNG or WebP file.
pub fn photo_info(path: &Path) -> MediaInfo {
    let Ok(file) = std::fs::File::open(path) else {
        return MediaInfo::default();
    };
    match exif::Reader::new().read_from_container(&mut BufReader::new(file)) {
        Ok(e) => from_exif(&e),
        Err(_) => MediaInfo::default(),
    }
}

/// Reads a QuickTime/MP4 video.
pub fn video_info(path: &Path) -> MediaInfo {
    let f = quicktime::video_facts(path).unwrap_or_default();
    from_video(&f)
}

/// [`MediaInfo`] from [`quicktime::VideoFacts`].
pub fn from_video(f: &quicktime::VideoFacts) -> MediaInfo {
    MediaInfo {
        is_video: true,
        make: f.make.clone(),
        model: f.model.clone(),
        duration_s: f.duration_s,
        width: f.width,
        height: f.height,
        codec: f.codec.clone(),
        taken: f.created.map(|d: DateTime<FixedOffset>| Taken {
            local: d.naive_local(),
            offset_min: Some(d.offset().local_minus_utc() / 60),
        }),
        gps: f
            .location
            .as_deref()
            .and_then(quicktime::parse_iso6709)
            .map(|(lat, lon, alt_m)| Gps { lat, lon, alt_m }),
        ..Default::default()
    }
}

fn text(e: &Exif, tag: Tag) -> Option<String> {
    match &e.get_field(tag, In::PRIMARY)?.value {
        Value::Ascii(v) => {
            let s = v
                .iter()
                .map(|b| {
                    String::from_utf8_lossy(b)
                        .trim_end_matches(['\0', ' '])
                        .trim()
                        .to_string()
                })
                .find(|s| !s.is_empty())?;
            Some(s)
        }
        _ => None,
    }
}

/// First value as f64 (rationals, integers). Non-finite results (x/0) are `None`.
fn num(e: &Exif, tag: Tag) -> Option<f64> {
    num_at(e, tag, 0)
}

fn num_at(e: &Exif, tag: Tag, i: usize) -> Option<f64> {
    let v = match &e.get_field(tag, In::PRIMARY)?.value {
        Value::Rational(r) => r.get(i).map(|r| r.to_f64()),
        Value::SRational(r) => r.get(i).map(|r| r.to_f64()),
        Value::Float(f) => f.get(i).map(|&f| f as f64),
        Value::Double(f) => f.get(i).copied(),
        other => other.get_uint(i).map(f64::from),
    }?;
    v.is_finite().then_some(v)
}

fn uint(e: &Exif, tag: Tag) -> Option<u32> {
    e.get_field(tag, In::PRIMARY)?.value.get_uint(0)
}

/// Swaps width/height for EXIF orientations 5–8 (rotated 90°).
fn oriented(w: u32, h: u32, orientation: Option<u16>) -> (u32, u32) {
    if matches!(orientation, Some(5..=8)) {
        (h, w)
    } else {
        (w, h)
    }
}

/// Parses `+09:00`, `-07:00`, `Z`-less EXIF offsets into minutes.
pub fn parse_offset(s: &str) -> Option<i32> {
    let s = s.trim_end_matches(['\0', ' ']).trim();
    let (sign, rest) = match s.as_bytes().first()? {
        b'+' => (1, &s[1..]),
        b'-' => (-1, &s[1..]),
        _ => return None,
    };
    let (h, m) = rest.split_once(':')?;
    let (h, m): (i32, i32) = (h.parse().ok()?, m.parse().ok()?);
    (h <= 14 && m < 60).then_some(sign * (h * 60 + m))
}

/// [`MediaInfo`] from parsed EXIF.
pub fn from_exif(e: &Exif) -> MediaInfo {
    let orientation = uint(e, Tag::Orientation).and_then(|o| u16::try_from(o).ok());
    let size = uint(e, Tag::PixelXDimension)
        .zip(uint(e, Tag::PixelYDimension))
        .or_else(|| uint(e, Tag::ImageWidth).zip(uint(e, Tag::ImageLength)))
        .filter(|&(w, h)| w > 0 && h > 0)
        .map(|(w, h)| oriented(w, h, orientation));
    // The lens maker is added only for third-party lenses (a Sigma lens on a Canon body), not
    // when it repeats the camera maker (every iPhone lens is "Apple") or the model already says it.
    let make = text(e, Tag::Make);
    let lens = text(e, Tag::LensModel).map(|model| match text(e, Tag::LensMake) {
        Some(mk)
            if !model.to_lowercase().starts_with(&mk.to_lowercase())
                && make.as_deref().is_none_or(|m| !m.eq_ignore_ascii_case(&mk)) =>
        {
            format!("{mk} {model}")
        }
        _ => model,
    });
    let taken = [
        (Tag::DateTimeOriginal, Tag::OffsetTimeOriginal),
        (Tag::DateTime, Tag::OffsetTime),
    ]
    .into_iter()
    .find_map(|(d, o)| {
        let s = text(e, d)?;
        let local = NaiveDateTime::parse_from_str(&s, "%Y:%m:%d %H:%M:%S").ok()?;
        Some(Taken {
            local,
            offset_min: text(e, o).as_deref().and_then(parse_offset),
        })
    });
    MediaInfo {
        is_video: false,
        make,
        model: text(e, Tag::Model),
        lens,
        focal_mm: num(e, Tag::FocalLength).filter(|&f| f > 0.0),
        focal_35mm: uint(e, Tag::FocalLengthIn35mmFilm).filter(|&f| f > 0),
        f_number: num(e, Tag::FNumber).filter(|&f| f > 0.0),
        exposure_s: num(e, Tag::ExposureTime).filter(|&t| t > 0.0),
        iso: uint(e, Tag::PhotographicSensitivity).filter(|&i| i > 0),
        flash: uint(e, Tag::Flash).map(|f| f & 1 == 1),
        exposure_bias_ev: num(e, Tag::ExposureBiasValue),
        taken,
        width: size.map(|s| s.0),
        height: size.map(|s| s.1),
        orientation,
        duration_s: None,
        codec: None,
        gps: gps(e),
    }
}

fn gps(e: &Exif) -> Option<Gps> {
    let dms = |tag: Tag| -> Option<f64> {
        let d = num_at(e, tag, 0)?;
        let m = num_at(e, tag, 1).unwrap_or(0.0);
        let s = num_at(e, tag, 2).unwrap_or(0.0);
        Some(d + m / 60.0 + s / 3600.0)
    };
    let sign = |tag: Tag, neg: &str| {
        if text(e, tag).is_some_and(|r| r.eq_ignore_ascii_case(neg)) {
            -1.0
        } else {
            1.0
        }
    };
    let lat = dms(Tag::GPSLatitude)? * sign(Tag::GPSLatitudeRef, "S");
    let lon = dms(Tag::GPSLongitude)? * sign(Tag::GPSLongitudeRef, "W");
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    let below = uint(e, Tag::GPSAltitudeRef) == Some(1);
    let alt_m = num(e, Tag::GPSAltitude).map(|a| if below { -a } else { a });
    Some(Gps { lat, lon, alt_m })
}

/// Pixel size from the JPEG SOF or PNG IHDR header (screenshots and edited files often have no
/// EXIF size tags).
pub fn header_size(path: &Path) -> Option<(u32, u32)> {
    let f = std::fs::File::open(path).ok()?;
    let mut buf = vec![];
    f.take(SNIFF as u64).read_to_end(&mut buf).ok()?;
    header_size_bytes(&buf)
}

fn header_size_bytes(b: &[u8]) -> Option<(u32, u32)> {
    if b.starts_with(b"\x89PNG\r\n\x1a\n") && b.get(12..16) == Some(b"IHDR") {
        let w = u32::from_be_bytes(b.get(16..20)?.try_into().ok()?);
        let h = u32::from_be_bytes(b.get(20..24)?.try_into().ok()?);
        return (w > 0 && h > 0).then_some((w, h));
    }
    if !b.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut i = 2;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            return None;
        }
        let marker = b[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        // SOF0..SOF15 except DHT (C4), JPG (C8), DAC (CC)
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let h = u16::from_be_bytes([*b.get(i + 5)?, *b.get(i + 6)?]) as u32;
            let w = u16::from_be_bytes([*b.get(i + 7)?, *b.get(i + 8)?]) as u32;
            return (w > 0 && h > 0).then_some((w, h));
        }
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

// ---------------------------------------------------------------- formatting

fn trim_num(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// `1/120 s`, `1/3 s`, `0.6 s`, `2 s`, `30 s`.
pub fn shutter(t: f64) -> Option<String> {
    if !t.is_finite() || t <= 0.0 {
        return None;
    }
    if t >= 1.0 {
        return Some(format!("{} s", trim_num(t, 1)));
    }
    let inv = 1.0 / t;
    if inv >= 1.95 && (inv - inv.round()).abs() < 0.05 * inv {
        Some(format!("1/{} s", inv.round() as u64))
    } else {
        Some(format!("{} s", trim_num(t, 1)))
    }
}

/// `f/1.8`, `f/8`.
pub fn aperture(f: f64) -> Option<String> {
    (f.is_finite() && f > 0.0).then(|| format!("f/{}", trim_num(f, 1)))
}

/// `26 mm`, `6.9 mm (24 mm equiv.)`.
pub fn focal(mm: Option<f64>, equiv: Option<u32>) -> Option<String> {
    let mm = mm.filter(|m| m.is_finite() && *m > 0.0);
    match (mm, equiv) {
        (Some(m), Some(e)) if (m.round() as i64 - e as i64).abs() >= 1 => {
            Some(format!("{} mm ({e} mm equiv.)", trim_num(m, 1)))
        }
        (Some(m), _) => Some(format!("{} mm", trim_num(m, 1))),
        (None, Some(e)) => Some(format!("{e} mm equiv.")),
        (None, None) => None,
    }
}

/// `+0.7 EV`, `-1.3 EV`, `0 EV`. Values beyond ±20 EV are corrupt tags (`None`).
pub fn exposure_bias(ev: f64) -> Option<String> {
    if !ev.is_finite() || ev.abs() > 20.0 {
        return None;
    }
    let r = (ev * 10.0).round() / 10.0;
    Some(if r == 0.0 {
        "0 EV".to_string()
    } else if r > 0.0 {
        format!("+{} EV", trim_num(r, 1))
    } else {
        format!("-{} EV", trim_num(-r, 1))
    })
}

/// `1.2 MB`, `840 KB`, `512 bytes` (decimal units, as Finder and Files show).
pub fn file_size(b: u64) -> String {
    match b {
        0..=999 => format!("{b} bytes"),
        1_000..=999_999 => format!("{} KB", (b as f64 / 1e3).round()),
        1_000_000..=999_999_999 => format!("{} MB", trim_num(b as f64 / 1e6, 1)),
        _ => format!("{} GB", trim_num(b as f64 / 1e9, 2)),
    }
}

/// `0:07`, `12:30`, `1:02:03`.
pub fn duration(s: f64) -> Option<String> {
    if !s.is_finite() || s < 0.0 {
        return None;
    }
    let t = s.round() as u64;
    let (h, m, s) = (t / 3600, t / 60 % 60, t % 60);
    Some(if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    })
}

/// `Sat 14 Mar 2026, 10:11 (UTC-07:00)`; `(time zone unknown)` without an offset.
pub fn date(t: &Taken) -> String {
    let zone = match t.offset_min {
        Some(0) => "UTC".to_string(),
        Some(m) => {
            let sign = if m < 0 { '-' } else { '+' };
            format!("UTC{sign}{:02}:{:02}", m.abs() / 60, m.abs() % 60)
        }
        None => "time zone unknown".to_string(),
    };
    format!("{} ({zone})", t.local.format("%a %-d %b %Y, %H:%M:%S"))
}

/// `12.2 MP · 4032 × 3024`.
pub fn dimensions(w: u32, h: u32) -> String {
    let mp = w as f64 * h as f64 / 1e6;
    format!("{} MP · {w} × {h}", trim_num(mp.max(0.1), 1))
}

/// `37.77493, -122.41942`.
pub fn coordinates(g: &Gps) -> String {
    format!("{:.5}, {:.5}", g.lat, g.lon)
}

/// OpenStreetMap link for "Open in Maps" (only opened when the user clicks).
pub fn maps_url(g: &Gps) -> String {
    format!(
        "https://www.openstreetmap.org/?mlat={:.5}&mlon={:.5}#map=16/{:.5}/{:.5}",
        g.lat, g.lon, g.lat, g.lon
    )
}

/// `HEIC`, `JPEG`, `MOV`… from the file name.
pub fn format_name(name: &str) -> Option<String> {
    let ext = Path::new(name)
        .extension()?
        .to_string_lossy()
        .to_ascii_uppercase();
    Some(match ext.as_str() {
        "JPG" | "JPE" | "JPEG" => "JPEG".to_string(),
        "TIF" | "TIFF" => "TIFF".to_string(),
        "HEIF" | "HEIC" => "HEIC".to_string(),
        _ => ext,
    })
}

const MISSING: &str = "—";

/// Turns info into rows, in the shared order (plan §17.1). Missing values get no row, except in
/// the Exposure group of camera photos, where "—" shows the camera didn't record the value.
pub fn format(info: &MediaInfo, file: &FileFacts, opts: &FormatOptions) -> Vec<Row> {
    let mut rows = vec![];
    let mut push = |group, label: &str, value: Option<String>, short| {
        if let Some(value) = value {
            rows.push(Row {
                group,
                label: label.to_string(),
                value,
                short,
            });
        }
    };

    // Camera
    let camera = match (&info.make, &info.model) {
        (Some(mk), Some(md)) if md.to_lowercase().starts_with(&mk.to_lowercase()) => {
            Some(md.clone())
        }
        (Some(mk), Some(md)) => Some(format!("{mk} {md}")),
        (mk, md) => mk.clone().or_else(|| md.clone()),
    };
    push(Group::Camera, "Camera", camera, true);
    push(Group::Camera, "Lens", info.lens.clone(), true);

    // Exposure
    let has_exposure = info.exposure_s.is_some()
        || info.f_number.is_some()
        || info.iso.is_some()
        || info.focal_mm.is_some()
        || info.focal_35mm.is_some();
    // Photos with no camera data at all (screenshots, exports) get no Exposure group.
    if !info.is_video && (has_exposure || info.make.is_some() || info.model.is_some()) {
        let or_missing = |v: Option<String>| Some(v.unwrap_or_else(|| MISSING.to_string()));
        push(
            Group::Exposure,
            "Shutter",
            or_missing(info.exposure_s.and_then(shutter)),
            true,
        );
        push(
            Group::Exposure,
            "Aperture",
            or_missing(info.f_number.and_then(aperture)),
            true,
        );
        push(
            Group::Exposure,
            "ISO",
            or_missing(info.iso.map(|i| format!("ISO {i}"))),
            true,
        );
        push(
            Group::Exposure,
            "Focal length",
            or_missing(focal(info.focal_mm, info.focal_35mm)),
            true,
        );
        push(
            Group::Exposure,
            "Flash",
            or_missing(
                info.flash
                    .map(|f| if f { "Fired" } else { "Off" }.to_string()),
            ),
            false,
        );
        push(
            Group::Exposure,
            "Exposure compensation",
            or_missing(info.exposure_bias_ev.and_then(exposure_bias)),
            false,
        );
    }

    // Date
    push(Group::Date, "Taken", info.taken.as_ref().map(date), true);

    // Image
    let dims = info.width.zip(info.height).map(|(w, h)| dimensions(w, h));
    push(Group::Image, "Dimensions", dims, true);
    push(
        Group::Image,
        "Duration",
        info.duration_s.and_then(duration),
        true,
    );
    push(
        Group::Image,
        "Codec",
        info.codec.as_deref().map(codec_name),
        false,
    );

    // File
    push(
        Group::File,
        "Name",
        Some(file.name.clone()).filter(|n| !n.is_empty()),
        false,
    );
    let size = file.size.map(|s| {
        let mut v = file_size(s);
        for (n, sz) in &file.sidecars {
            v.push_str(&format!(" + {n}, {}", file_size(*sz)));
        }
        v
    });
    push(Group::File, "Size", size, true);
    push(Group::File, "Format", format_name(&file.name), true);
    push(Group::File, "Folder", file.folder.clone(), false);

    // Location (local only)
    if opts.show_location {
        if let Some(g) = &info.gps {
            push(Group::Location, "Coordinates", Some(coordinates(g)), false);
            push(
                Group::Location,
                "Altitude",
                g.alt_m
                    .filter(|a| a.is_finite())
                    .map(|a| format!("{} m", a.round() as i64)),
                false,
            );
        }
    }

    // Unswiper
    let sharp = if info.is_video {
        Some(Sharpness::NotChecked)
    } else {
        opts.sharpness
    };
    push(
        Group::Unswiper,
        "Sharpness",
        sharp.map(|s| s.label().to_string()),
        true,
    );
    rows
}

/// One-line summary for the Compare strip (selected item only): exposure for photos,
/// duration and size for videos, then the blur word, e.g.
/// `6.8 mm (24 mm equiv.) · f/1.8 · 1/120 s · ISO 64 · Sharp`. Empty when nothing is known.
pub fn strip(info: &MediaInfo, sharpness: Option<Sharpness>) -> String {
    let mut parts: Vec<String> = vec![];
    if info.is_video {
        parts.extend(info.duration_s.and_then(duration));
        parts.extend(
            info.width
                .zip(info.height)
                .map(|(w, h)| format!("{w} × {h}")),
        );
    } else {
        parts.extend(focal(info.focal_mm, info.focal_35mm));
        parts.extend(info.f_number.and_then(aperture));
        parts.extend(info.exposure_s.and_then(shutter));
        parts.extend(info.iso.map(|i| format!("ISO {i}")));
        parts.extend(sharpness.map(|s| s.label().to_string()));
    }
    parts.join(" · ")
}

/// `HEVC (hvc1)`, `H.264 (avc1)`, or the FourCC.
pub fn codec_name(fourcc: &str) -> String {
    let name = match fourcc {
        "hvc1" | "hev1" => "HEVC",
        "avc1" | "avc3" => "H.264",
        "ap4h" | "apch" | "apcn" | "apcs" | "apco" => "ProRes",
        "av01" => "AV1",
        "vp09" => "VP9",
        _ => return fourcc.to_string(),
    };
    format!("{name} ({fourcc})")
}

#[cfg(test)]
mod tests;
