// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

use super::*;
use exif::experimental::Writer;
use exif::{Field, Rational, SRational};
use proptest::prelude::*;
use std::io::Cursor;

fn f(tag: Tag, value: Value) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value,
    }
}
fn ascii(s: &str) -> Value {
    Value::Ascii(vec![s.as_bytes().to_vec()])
}
fn rat(n: u32, d: u32) -> Rational {
    Rational { num: n, denom: d }
}

/// TIFF bytes with these fields.
fn tiff(fields: &[Field]) -> Vec<u8> {
    let mut w = Writer::new();
    for x in fields {
        w.push_field(x);
    }
    let mut out = Cursor::new(vec![]);
    w.write(&mut out, false).unwrap();
    out.into_inner()
}

/// A minimal JPEG: SOI, APP1 Exif (optional), SOF0 with a size, EOI.
fn jpeg(fields: Option<&[Field]>, w: u16, h: u16) -> Vec<u8> {
    let mut j = vec![0xFF, 0xD8];
    if let Some(fields) = fields {
        let t = tiff(fields);
        j.extend_from_slice(&[0xFF, 0xE1]);
        j.extend_from_slice(&((t.len() + 8) as u16).to_be_bytes());
        j.extend_from_slice(b"Exif\0\0");
        j.extend_from_slice(&t);
    }
    j.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x04, 0x00, 0x00]); // a dummy DQT
    j.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 0x08]);
    j.extend_from_slice(&h.to_be_bytes());
    j.extend_from_slice(&w.to_be_bytes());
    j.extend_from_slice(&[0x01, 0x01, 0x11, 0x00]);
    j.extend_from_slice(&[0xFF, 0xD9]);
    j
}

fn write(name: &str, bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join(name);
    std::fs::write(&p, bytes).unwrap();
    (d, p)
}

fn iphone_fields() -> Vec<Field> {
    vec![
        f(Tag::Make, ascii("Apple")),
        f(Tag::Model, ascii("iPhone 15 Pro")),
        f(Tag::LensMake, ascii("Apple")),
        f(
            Tag::LensModel,
            ascii("iPhone 15 Pro back triple camera 6.765mm f/1.78"),
        ),
        f(Tag::Orientation, Value::Short(vec![6])),
        f(Tag::DateTimeOriginal, ascii("2026:03:14 10:11:12")),
        f(Tag::OffsetTimeOriginal, ascii("-07:00")),
        f(Tag::ExposureTime, Value::Rational(vec![rat(1, 120)])),
        f(Tag::FNumber, Value::Rational(vec![rat(178, 100)])),
        f(Tag::PhotographicSensitivity, Value::Short(vec![64])),
        f(Tag::FocalLength, Value::Rational(vec![rat(6765, 1000)])),
        f(Tag::FocalLengthIn35mmFilm, Value::Short(vec![24])),
        f(Tag::Flash, Value::Short(vec![0x10])),
        f(
            Tag::ExposureBiasValue,
            Value::SRational(vec![SRational { num: -1, denom: 3 }]),
        ),
        f(Tag::PixelXDimension, Value::Long(vec![4032])),
        f(Tag::PixelYDimension, Value::Long(vec![3024])),
        f(Tag::GPSLatitudeRef, ascii("S")),
        f(
            Tag::GPSLatitude,
            Value::Rational(vec![rat(33, 1), rat(52, 1), rat(780, 100)]),
        ),
        f(Tag::GPSLongitudeRef, ascii("E")),
        f(
            Tag::GPSLongitude,
            Value::Rational(vec![rat(151, 1), rat(12, 1), rat(3348, 100)]),
        ),
        f(Tag::GPSAltitudeRef, Value::Byte(vec![1])),
        f(Tag::GPSAltitude, Value::Rational(vec![rat(25, 2)])),
    ]
}

#[test]
fn reads_every_field_from_a_jpeg() {
    let (_d, p) = write("IMG_0001.JPG", &jpeg(Some(&iphone_fields()), 4032, 3024));
    let i = read(&p);
    assert!(!i.is_video);
    assert_eq!(i.make.as_deref(), Some("Apple"));
    assert_eq!(i.model.as_deref(), Some("iPhone 15 Pro"));
    // LensModel already starts with the lens make: not repeated
    assert_eq!(
        i.lens.as_deref(),
        Some("iPhone 15 Pro back triple camera 6.765mm f/1.78")
    );
    assert_eq!(i.exposure_s, Some(1.0 / 120.0));
    assert_eq!(i.f_number, Some(1.78));
    assert_eq!(i.iso, Some(64));
    assert_eq!(i.focal_mm, Some(6.765));
    assert_eq!(i.focal_35mm, Some(24));
    assert_eq!(i.flash, Some(false));
    assert!((i.exposure_bias_ev.unwrap() + 1.0 / 3.0).abs() < 1e-9);
    // orientation 6 swaps to portrait
    assert_eq!(
        (i.width, i.height, i.orientation),
        (Some(3024), Some(4032), Some(6))
    );
    let t = i.taken.unwrap();
    assert_eq!(t.local.to_string(), "2026-03-14 10:11:12");
    assert_eq!(t.offset_min, Some(-420));
    let g = i.gps.unwrap();
    assert!((g.lat - -33.8688333).abs() < 1e-6, "{}", g.lat);
    assert!((g.lon - 151.2093).abs() < 1e-6, "{}", g.lon);
    assert_eq!(g.alt_m, Some(-12.5));
}

#[test]
fn lens_make_prefix_flash_fired_and_datetime_fallback() {
    let fields = vec![
        f(Tag::Make, ascii("Canon")),
        f(Tag::Model, ascii("Canon EOS R6")),
        f(Tag::LensMake, ascii("Sigma")),
        f(Tag::LensModel, ascii("35mm F1.4 DG DN | A")),
        f(Tag::DateTime, ascii("2025:12:31 23:59:00")),
        f(Tag::Flash, Value::Short(vec![0x19])),
        f(Tag::GPSLatitudeRef, ascii("N")),
        f(
            Tag::GPSLatitude,
            Value::Rational(vec![rat(48, 1), rat(51, 1), rat(0, 1)]),
        ),
        f(Tag::GPSLongitudeRef, ascii("W")),
        f(
            Tag::GPSLongitude,
            Value::Rational(vec![rat(2, 1), rat(30, 1), rat(0, 1)]),
        ),
    ];
    let (_d, p) = write("x.tif", &tiff(&fields));
    let i = photo_info(&p);
    assert_eq!(i.lens.as_deref(), Some("Sigma 35mm F1.4 DG DN | A"));
    assert_eq!(i.flash, Some(true));
    assert_eq!(i.taken.unwrap().offset_min, None);
    assert_eq!(i.taken.unwrap().local.to_string(), "2025-12-31 23:59:00");
    let g = i.gps.unwrap();
    assert_eq!((g.lat, g.lon, g.alt_m), (48.85, -2.5, None));
    let rows = format(&i, &FileFacts::default(), &FormatOptions::default());
    assert_eq!(rows[0].value, "Canon EOS R6"); // model already names the maker
}

#[test]
fn odd_rationals_are_missing_not_nan() {
    let fields = vec![
        f(Tag::ExposureTime, Value::Rational(vec![rat(1, 0)])),
        f(Tag::FNumber, Value::Rational(vec![rat(0, 0)])),
        f(Tag::FocalLength, Value::Rational(vec![rat(0, 1)])),
        f(
            Tag::ExposureBiasValue,
            Value::SRational(vec![SRational { num: 1, denom: 0 }]),
        ),
        f(Tag::GPSLatitude, Value::Rational(vec![rat(1, 0)])),
        f(Tag::GPSLongitude, Value::Rational(vec![rat(2, 1)])),
        f(Tag::PixelXDimension, Value::Long(vec![0])),
        f(Tag::PixelYDimension, Value::Long(vec![10])),
    ];
    let (_d, p) = write("x.jpg", &jpeg(Some(&fields), 640, 480));
    let i = read(&p);
    assert_eq!(i.exposure_s, None);
    assert_eq!(i.f_number, None);
    assert_eq!(i.focal_mm, None);
    assert_eq!(i.exposure_bias_ev, None);
    assert_eq!(i.gps, None);
    // EXIF size 0 is ignored; the JPEG header gives the size
    assert_eq!((i.width, i.height), (Some(640), Some(480)));
}

#[test]
fn files_without_exif() {
    let (_d, p) = write("shot.jpg", &jpeg(None, 1170, 2532));
    let i = read(&p);
    assert_eq!((i.width, i.height), (Some(1170), Some(2532)));
    assert_eq!(i.make, None);
    // PNG screenshot
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&1290u32.to_be_bytes());
    png.extend_from_slice(&2796u32.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    let (_d2, p2) = write("Screenshot.png", &png);
    assert_eq!(
        (read(&p2).width, read(&p2).height),
        (Some(1290), Some(2796))
    );
    // garbage and missing files
    let (_d3, p3) = write("x.jpg", b"not a jpeg");
    assert_eq!(read(&p3), MediaInfo::default());
    assert_eq!(read(Path::new("/nonexistent/x.heic")), MediaInfo::default());
    assert_eq!(header_size_bytes(&[0xFF, 0xD8, 0x00]), None);
    assert_eq!(
        header_size_bytes(&[0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x00]),
        None
    );
    assert_eq!(
        header_size_bytes(&[0xFF, 0xD8, 0xFF, 0xFF, 0xFF, 0xD9]),
        None
    );
}

#[test]
fn reads_videos_through_quicktime() {
    use crate::meta::quicktime::build::*;
    use crate::meta::quicktime::MAC_EPOCH_OFFSET;
    let mac = (1_767_225_600i64 + MAC_EPOCH_OFFSET) as u32;
    let bytes = file(
        &[
            mvhd_dur(mac, 600, 4500),
            video_trak(1920, 1080, true, b"hvc1", b"vide"),
            apple_meta_keys(&[
                ("com.apple.quicktime.make", "Apple"),
                ("com.apple.quicktime.model", "iPhone 15 Pro"),
                (
                    "com.apple.quicktime.creationdate",
                    "2026-03-04T10:00:00+0100",
                ),
                (
                    "com.apple.quicktime.location.ISO6709",
                    "+51.5007-000.1246+020.000/",
                ),
            ]),
        ],
        false,
    );
    let (_d, p) = write("IMG_0002.MOV", &bytes);
    let i = read(&p);
    assert!(i.is_video);
    assert_eq!(i.model.as_deref(), Some("iPhone 15 Pro"));
    assert_eq!((i.width, i.height), (Some(1080), Some(1920)));
    assert_eq!(i.duration_s, Some(7.5));
    assert_eq!(i.codec.as_deref(), Some("hvc1"));
    assert_eq!(i.taken.unwrap().offset_min, Some(60));
    assert_eq!(i.taken.unwrap().local.to_string(), "2026-03-04 10:00:00");
    assert_eq!(i.gps.unwrap().alt_m, Some(20.0));
    let rows = format(&i, &FileFacts::of(&p), &FormatOptions::default());
    let labels: Vec<_> = rows.iter().map(|r| r.label.as_str()).collect();
    assert!(!labels.contains(&"Shutter"), "no Exposure group for videos");
    assert_eq!(rows.last().unwrap().value, "Not checked");
    assert!(rows.iter().any(|r| r.value == "HEVC (hvc1)"));
    // an unreadable video is still a video
    let (_d2, p2) = write("x.mp4", b"junk");
    assert!(read(&p2).is_video);
}

#[test]
fn value_formatters() {
    assert_eq!(shutter(1.0 / 120.0).unwrap(), "1/120 s");
    assert_eq!(shutter(1.0 / 3.0).unwrap(), "1/3 s");
    assert_eq!(shutter(0.6).unwrap(), "0.6 s");
    assert_eq!(shutter(0.5).unwrap(), "1/2 s");
    assert_eq!(shutter(2.0).unwrap(), "2 s");
    assert_eq!(shutter(2.5).unwrap(), "2.5 s");
    assert_eq!(shutter(1.0 / 8000.0).unwrap(), "1/8000 s");
    assert_eq!(shutter(0.0), None);
    assert_eq!(shutter(f64::NAN), None);
    assert_eq!(aperture(1.78).unwrap(), "f/1.8");
    assert_eq!(aperture(8.0).unwrap(), "f/8");
    assert_eq!(aperture(-1.0), None);
    assert_eq!(
        focal(Some(6.765), Some(24)).unwrap(),
        "6.8 mm (24 mm equiv.)"
    );
    assert_eq!(focal(Some(50.0), Some(50)).unwrap(), "50 mm");
    assert_eq!(focal(Some(35.0), None).unwrap(), "35 mm");
    assert_eq!(focal(None, Some(26)).unwrap(), "26 mm equiv.");
    assert_eq!(focal(None, None), None);
    assert_eq!(exposure_bias(0.7).unwrap(), "+0.7 EV");
    assert_eq!(exposure_bias(-1.0 / 3.0).unwrap(), "-0.3 EV");
    assert_eq!(exposure_bias(0.01).unwrap(), "0 EV");
    assert_eq!(exposure_bias(f64::INFINITY), None);
    assert_eq!(exposure_bias(-1e300), None);
    assert_eq!(file_size(512), "512 bytes");
    assert_eq!(file_size(840_400), "840 KB");
    assert_eq!(file_size(2_140_000), "2.1 MB");
    assert_eq!(file_size(3_000_000), "3 MB");
    assert_eq!(file_size(1_234_000_000), "1.23 GB");
    assert_eq!(duration(7.4).unwrap(), "0:07");
    assert_eq!(duration(750.0).unwrap(), "12:30");
    assert_eq!(duration(3723.0).unwrap(), "1:02:03");
    assert_eq!(duration(-1.0), None);
    assert_eq!(dimensions(4032, 3024), "12.2 MP · 4032 × 3024");
    assert_eq!(dimensions(16, 16), "0.1 MP · 16 × 16");
    assert_eq!(format_name("a.jpg").as_deref(), Some("JPEG"));
    assert_eq!(format_name("a.HEIC").as_deref(), Some("HEIC"));
    assert_eq!(format_name("a.tif").as_deref(), Some("TIFF"));
    assert_eq!(format_name("a.mov").as_deref(), Some("MOV"));
    assert_eq!(format_name("README"), None);
    assert_eq!(codec_name("avc1"), "H.264 (avc1)");
    assert_eq!(codec_name("xyz1"), "xyz1");
    let g = Gps {
        lat: 37.774929,
        lon: -122.419416,
        alt_m: None,
    };
    assert_eq!(coordinates(&g), "37.77493, -122.41942");
    assert_eq!(
        maps_url(&g),
        "https://www.openstreetmap.org/?mlat=37.77493&mlon=-122.41942#map=16/37.77493/-122.41942"
    );
}

#[test]
fn offsets_and_dates() {
    assert_eq!(parse_offset("+09:00"), Some(540));
    assert_eq!(parse_offset("-03:30\0"), Some(-210));
    assert_eq!(parse_offset("+00:00"), Some(0));
    assert_eq!(parse_offset("   :  "), None);
    assert_eq!(parse_offset("09:00"), None);
    assert_eq!(parse_offset("+25:00"), None);
    assert_eq!(parse_offset(""), None);
    let local = NaiveDateTime::parse_from_str("2026-03-14 10:11:12", "%Y-%m-%d %H:%M:%S").unwrap();
    let t = |offset_min| date(&Taken { local, offset_min });
    assert_eq!(t(Some(-420)), "Sat 14 Mar 2026, 10:11:12 (UTC-07:00)");
    assert_eq!(t(Some(330)), "Sat 14 Mar 2026, 10:11:12 (UTC+05:30)");
    assert_eq!(t(Some(0)), "Sat 14 Mar 2026, 10:11:12 (UTC)");
    assert_eq!(t(None), "Sat 14 Mar 2026, 10:11:12 (time zone unknown)");
}

#[test]
fn location_hidden_by_setting_and_never_serialized_into_rows_when_off() {
    let i = MediaInfo {
        gps: Some(Gps {
            lat: 1.0,
            lon: 2.0,
            alt_m: Some(3.0),
        }),
        ..Default::default()
    };
    let on = format(&i, &FileFacts::default(), &FormatOptions::default());
    assert!(on.iter().any(|r| r.group == Group::Location));
    let off = format(
        &i,
        &FileFacts::default(),
        &FormatOptions {
            show_location: false,
            sharpness: None,
        },
    );
    assert!(off.iter().all(|r| r.group != Group::Location));
    assert!(!serde_json::to_string(&off).unwrap().contains("1.00000"));
}

#[test]
fn sharpness_words() {
    for (s, w) in [
        (Sharpness::Sharp, "Sharp"),
        (Sharpness::Soft, "Soft"),
        (Sharpness::Blurry, "Blurry"),
        (Sharpness::NotChecked, "Not checked"),
    ] {
        assert_eq!(s.label(), w);
        let rows = format(
            &MediaInfo::default(),
            &FileFacts::default(),
            &FormatOptions {
                show_location: true,
                sharpness: Some(s),
            },
        );
        assert_eq!(rows.last().unwrap().value, w);
    }
    assert_eq!(
        serde_json::to_string(&Sharpness::NotChecked).unwrap(),
        "\"notchecked\""
    );
    assert_eq!(Group::Unswiper.title(), "Unswiper");
}

/// One case of the golden table shared with the Mac tests (testdata/info-format-golden.json).
#[derive(Deserialize)]
struct Golden {
    name: String,
    info: MediaInfo,
    file: FileFacts,
    options: FormatOptions,
    rows: Vec<Row>,
}

#[test]
fn golden_table() {
    let raw = include_str!("../../../testdata/info-format-golden.json");
    let cases: Vec<Golden> = serde_json::from_str(raw).unwrap();
    assert!(cases.len() >= 4);
    for c in cases {
        assert_eq!(
            format(&c.info, &c.file, &c.options),
            c.rows,
            "case {}",
            c.name
        );
        // MediaInfo round-trips through the helper JSON
        let back: MediaInfo =
            serde_json::from_str(&serde_json::to_string(&c.info).unwrap()).unwrap();
        assert_eq!(back, c.info, "case {}", c.name);
    }
}

#[test]
fn file_facts_of_a_path() {
    let (d, p) = write("IMG_9.HEIC", b"12345");
    let ff = FileFacts::of(&p);
    assert_eq!(ff.name, "IMG_9.HEIC");
    assert_eq!(ff.size, Some(5));
    assert_eq!(ff.folder.as_deref(), Some(d.path().to_str().unwrap()));
    assert_eq!(FileFacts::of(Path::new("/")).name, "");
}

proptest! {
    /// The formatter never panics and never prints NaN/inf, whatever the numbers.
    #[test]
    fn formatter_never_prints_nan(
        t in proptest::num::f64::ANY,
        fnum in proptest::num::f64::ANY,
        ev in proptest::num::f64::ANY,
        mm in proptest::num::f64::ANY,
        dur in proptest::num::f64::ANY,
        lat in -90.0f64..90.0,
        alt in proptest::num::f64::ANY,
        w in 0u32..100_000,
        h in 0u32..100_000,
        size in proptest::num::u64::ANY,
        e35 in proptest::option::of(0u32..2000),
        off in proptest::option::of(-840i32..840),
        video in proptest::bool::ANY,
    ) {
        let i = MediaInfo {
            is_video: video,
            exposure_s: Some(t), f_number: Some(fnum), exposure_bias_ev: Some(ev),
            focal_mm: Some(mm), focal_35mm: e35, duration_s: Some(dur),
            width: Some(w), height: Some(h),
            taken: Some(Taken { local: NaiveDateTime::default(), offset_min: off }),
            gps: Some(Gps { lat, lon: 0.0, alt_m: Some(alt) }),
            ..Default::default()
        };
        let ff = FileFacts { name: "a.jpg".into(), size: Some(size), ..Default::default() };
        for r in format(&i, &ff, &FormatOptions::default()) {
            let v = r.value.to_lowercase();
            prop_assert!(!v.contains("nan") && !v.contains("inf"), "{:?}", r);
        }
    }
}

#[test]
fn compare_strip() {
    let photo = MediaInfo {
        focal_mm: Some(6.765),
        focal_35mm: Some(24),
        f_number: Some(1.78),
        exposure_s: Some(1.0 / 120.0),
        iso: Some(64),
        ..Default::default()
    };
    assert_eq!(
        strip(&photo, Some(Sharpness::Sharp)),
        "6.8 mm (24 mm equiv.) · f/1.8 · 1/120 s · ISO 64 · Sharp"
    );
    assert_eq!(strip(&MediaInfo::default(), None), "");
    assert_eq!(
        strip(&MediaInfo::default(), Some(Sharpness::NotChecked)),
        ""
    );
    let video = MediaInfo {
        is_video: true,
        duration_s: Some(7.4),
        width: Some(1080),
        height: Some(1920),
        ..Default::default()
    };
    assert_eq!(strip(&video, Some(Sharpness::Blurry)), "0:07 · 1080 × 1920");
}
