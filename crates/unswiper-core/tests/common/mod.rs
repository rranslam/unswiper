// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// Minimal JPEG with an APP1 EXIF block. `dto` → ExifIFD DateTimeOriginal, `dt` → IFD0 DateTime.
pub fn jpeg_with_exif(dto: Option<&str>, dt: Option<&str>) -> Vec<u8> {
    fn ascii20(s: &str) -> Vec<u8> {
        let mut v = s.as_bytes().to_vec();
        v.push(0);
        v.resize(20, 0);
        v
    }
    let mut ifd0: Vec<(u16, u16, u32, Vec<u8>)> = vec![];
    if let Some(dt) = dt {
        ifd0.push((0x0132, 2, 20, ascii20(dt)));
    }
    let mut tiff = b"II*\0".to_vec();
    tiff.extend_from_slice(&8u32.to_le_bytes());
    let n0 = ifd0.len() + 1;
    let ifd0_size = 2 + n0 * 12 + 4;
    let data0_off = 8 + ifd0_size;
    let exif_ifd_off = data0_off + ifd0.iter().map(|e| e.3.len()).sum::<usize>();
    let mut entries = vec![];
    let mut data = vec![];
    let mut off = data0_off;
    for (tag, typ, count, val) in &ifd0 {
        entries.push((*tag, *typ, *count, off as u32));
        data.extend_from_slice(val);
        off += val.len();
    }
    entries.push((0x8769, 4, 1, exif_ifd_off as u32));
    entries.sort_by_key(|e| e.0);
    tiff.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    for (tag, typ, count, v) in entries {
        tiff.extend_from_slice(&tag.to_le_bytes());
        tiff.extend_from_slice(&typ.to_le_bytes());
        tiff.extend_from_slice(&count.to_le_bytes());
        tiff.extend_from_slice(&v.to_le_bytes());
    }
    tiff.extend_from_slice(&0u32.to_le_bytes());
    tiff.extend_from_slice(&data);
    // Exif IFD
    let n1: u16 = if dto.is_some() { 1 } else { 0 };
    tiff.extend_from_slice(&n1.to_le_bytes());
    let val_off = exif_ifd_off + 2 + n1 as usize * 12 + 4;
    if dto.is_some() {
        tiff.extend_from_slice(&0x9003u16.to_le_bytes());
        tiff.extend_from_slice(&2u16.to_le_bytes());
        tiff.extend_from_slice(&20u32.to_le_bytes());
        tiff.extend_from_slice(&(val_off as u32).to_le_bytes());
    }
    tiff.extend_from_slice(&0u32.to_le_bytes());
    if let Some(dto) = dto {
        tiff.extend_from_slice(&ascii20(dto));
    }
    let mut app1 = b"Exif\0\0".to_vec();
    app1.extend_from_slice(&tiff);
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(&app1);
    out.extend_from_slice(&[0xFF, 0xD9]);
    out
}

fn bx(typ: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut v = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
    v.extend_from_slice(typ);
    v.extend_from_slice(payload);
    v
}

/// Minimal MP4 with only `moov/mvhd` at the given unix time.
pub fn mp4_with_mvhd(unix: i64) -> Vec<u8> {
    let mut p = vec![0, 0, 0, 0];
    p.extend_from_slice(&((unix + 2_082_844_800) as u32).to_be_bytes());
    p.extend_from_slice(&[0u8; 92]);
    [
        bx(b"ftyp", b"isom\0\0\0\0isom"),
        bx(b"moov", &bx(b"mvhd", &p)),
    ]
    .concat()
}

pub fn write(root: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, bytes).unwrap();
    p
}

pub fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap()
}
