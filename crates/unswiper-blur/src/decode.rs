// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Decoding photos for scoring: the `image` crate for JPEG/PNG/TIFF/WebP/…, libheif for
//! HEIC/HEIF (feature `heif`, same as the GTK app's thumbnails).

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

#[cfg(not(feature = "heif"))]
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
