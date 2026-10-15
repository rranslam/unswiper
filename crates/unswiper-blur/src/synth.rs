// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Deterministic synthetic test scenes and blur kernels (plan §15.6 "xtask" fixtures).
//! Used by the tests and the accuracy gate; no photos are checked in.

use image::{DynamicImage, GrayImage, Luma};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// A textured "photo": gradient sky, rectangles, discs and fine stripes, plus mild noise.
pub fn scene(seed: u64, w: u32, h: u32) -> DynamicImage {
    let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut img = GrayImage::from_fn(w, h, |_, y| {
        Luma([(90.0 + 80.0 * y as f32 / h as f32) as u8])
    });
    let s = w.min(h) as f32;
    for _ in 0..40 {
        let cx = r.unit() * w as f32;
        let cy = r.unit() * h as f32;
        let size = (0.03 + r.unit() * 0.15) * s;
        let v = (r.unit() * 255.0) as u8;
        let disc = r.unit() < 0.5;
        let stripes = r.unit() < 0.3;
        for y in (cy - size).max(0.0) as u32..((cy + size) as u32).min(h) {
            for x in (cx - size).max(0.0) as u32..((cx + size) as u32).min(w) {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                if disc && dx * dx + dy * dy > size * size {
                    continue;
                }
                let v = if stripes && (x / 2 + y / 3) % 2 == 0 {
                    255 - v
                } else {
                    v
                };
                img.put_pixel(x, y, Luma([v]));
            }
        }
    }
    for p in img.pixels_mut() {
        let n = (r.unit() - 0.5) * 6.0;
        p.0[0] = (p.0[0] as f32 + n).clamp(0.0, 255.0) as u8;
    }
    DynamicImage::ImageLuma8(img)
}

/// Out-of-focus (Gaussian σ in pixels of the given image).
pub fn defocus(img: &DynamicImage, sigma: f32) -> DynamicImage {
    img.blur(sigma)
}

/// Horizontal camera shake: box kernel of `len` pixels.
pub fn motion(img: &DynamicImage, len: u32) -> DynamicImage {
    let g = img.to_luma8();
    let (w, h) = g.dimensions();
    let half = (len / 2) as i64;
    DynamicImage::ImageLuma8(GrayImage::from_fn(w, h, |x, y| {
        let mut sum = 0u32;
        for d in -half..=half {
            let xx = (x as i64 + d).clamp(0, w as i64 - 1) as u32;
            sum += g.get_pixel(xx, y).0[0] as u32;
        }
        Luma([(sum / (2 * half as u32 + 1)) as u8])
    }))
}

/// Shallow depth of field: a sharp subject over the middle quarter of the frame (four of the
/// sixteen tiles), heavily blurred elsewhere.
pub fn bokeh(img: &DynamicImage) -> DynamicImage {
    let back = img.blur(8.0).to_luma8();
    let sharp = img.to_luma8();
    let (w, h) = sharp.dimensions();
    DynamicImage::ImageLuma8(GrayImage::from_fn(w, h, |x, y| {
        let inside = x > w / 4 && x < w * 3 / 4 && y > h / 4 && y < h * 3 / 4;
        if inside {
            *sharp.get_pixel(x, y)
        } else {
            *back.get_pixel(x, y)
        }
    }))
}

/// Scales brightness (night shots).
pub fn darken(img: &DynamicImage, f: f32) -> DynamicImage {
    let mut g = img.to_luma8();
    g.pixels_mut()
        .for_each(|p| p.0[0] = (p.0[0] as f32 * f) as u8);
    DynamicImage::ImageLuma8(g)
}

/// Squeezes contrast around mid-grey by `f` (fog, pale text on a pale background).
pub fn faint(img: &DynamicImage, f: f32) -> DynamicImage {
    let mut g = img.to_luma8();
    g.pixels_mut()
        .for_each(|p| p.0[0] = (180.0 + (p.0[0] as f32 - 128.0) * f).clamp(0.0, 255.0) as u8);
    DynamicImage::ImageLuma8(g)
}

/// Mostly plain sky with one small sharp object.
pub fn low_texture(seed: u64, w: u32, h: u32) -> DynamicImage {
    let detail = scene(seed, w, h).to_luma8();
    DynamicImage::ImageLuma8(GrayImage::from_fn(w, h, |x, y| {
        if x < w / 3 && y > h * 2 / 3 {
            *detail.get_pixel(x, y)
        } else {
            Luma([(150.0 + 40.0 * y as f32 / h as f32) as u8])
        }
    }))
}
