// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Decoding photos for scoring: the `image` crate for JPEG/PNG/TIFF/WebP/…, libheif for
//! HEIC/HEIF (feature `heif`, same as the GTK app's thumbnails). The Mac helper is built without
//! libheif (nothing to bundle) and converts HEIC with macOS's own `sips` instead.

use image::DynamicImage;
use std::path::Path;

pub fn decode(path: &Path) -> Option<DynamicImage> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if matches!(ext.as_str(), "heic" | "heif") {
        return heif(path);
    }
    image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()
}

#[cfg(feature = "heif")]
fn heif(path: &Path) -> Option<DynamicImage> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
    let lib = LibHeif::new();
    let ctx = HeifContext::read_from_file(path.to_str()?).ok()?;
    let handle = ctx.primary_image_handle().ok()?;
    // Let libheif shrink big images first: scoring only needs 1024 px.
    let img = lib
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgb), None)
        .ok()?;
    let p = img.planes().interleaved?;
    let (w, h, stride) = (p.width, p.height, p.stride);
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for row in 0..h as usize {
        rgb.extend_from_slice(&p.data[row * stride..row * stride + w as usize * 3]);
    }
    image::RgbImage::from_raw(w, h, rgb).map(DynamicImage::ImageRgb8)
}

#[cfg(all(not(feature = "heif"), target_os = "macos"))]
fn heif(path: &Path) -> Option<DynamicImage> {
    // `sips` ships with macOS (ImageIO, hardware HEVC). Shrinking to the 1024 px working size
    // here keeps it fast; the temporary JPEG is removed straight away.
    let tmp = std::env::temp_dir().join(format!(
        "unswiper-blur-{}-{}.jpg",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let ok = std::process::Command::new("/usr/bin/sips")
        .args(["-s", "format", "jpeg", "-Z"])
        .arg(crate::score::WORK_EDGE.to_string())
        .arg(path)
        .arg("--out")
        .arg(&tmp)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    let img = ok.then(|| image::open(&tmp).ok()).flatten();
    let _ = std::fs::remove_file(&tmp);
    img
}

#[cfg(all(not(feature = "heif"), not(target_os = "macos")))]
fn heif(_path: &Path) -> Option<DynamicImage> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_rejects() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.jpg");
        image::RgbImage::from_pixel(8, 6, image::Rgb([1, 2, 3]))
            .save(&p)
            .unwrap();
        let img = decode(&p).unwrap();
        assert_eq!((img.width(), img.height()), (8, 6));
        let bad = d.path().join("b.jpg");
        std::fs::write(&bad, b"nope").unwrap();
        assert!(decode(&bad).is_none());
        let h = d.path().join("c.HEIC");
        std::fs::write(&h, b"nope").unwrap();
        assert!(decode(&h).is_none());
        assert!(decode(&d.path().join("missing.jpg")).is_none());
    }
}
