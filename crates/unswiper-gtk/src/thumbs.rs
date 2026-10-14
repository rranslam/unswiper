// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Thumbnails without a display: stills through the `image` crate, HEIC/HEIF through libheif
//! (required), video frames through `ffmpeg` when it is installed (optional; otherwise the UI
//! shows a film icon). Results are cached as PNG in `$XDG_CACHE_HOME/unswiper/thumbs`.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use unswiper_core::group::fnv1a;
use unswiper_core::MediaKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumb {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA8 (stride = width * 4).
    pub rgba: Vec<u8>,
}

impl Thumb {
    fn from_image(img: image::DynamicImage, max: u32) -> Thumb {
        let t = img.thumbnail(max, max).to_rgba8();
        Thumb {
            width: t.width(),
            height: t.height(),
            rgba: t.into_raw(),
        }
    }

    pub fn to_png(&self) -> Option<Vec<u8>> {
        let img = image::RgbaImage::from_raw(self.width, self.height, self.rgba.clone())?;
        let mut out = Cursor::new(vec![]);
        img.write_to(&mut out, image::ImageFormat::Png).ok()?;
        Some(out.into_inner())
    }
}

/// `$XDG_CACHE_HOME/unswiper/thumbs` (default `~/.cache/unswiper/thumbs`).
pub fn default_cache_dir() -> PathBuf {
    match std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache"),
    }
    .join("unswiper/thumbs")
}

/// Whether `ffmpeg` can be run (checked once).
pub fn ffmpeg_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("ffmpeg")
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

/// Whether HEIC/HEIF decoding was compiled in.
pub fn heif_supported() -> bool {
    cfg!(feature = "heif")
}

fn cache_key(path: &Path, max: u32) -> Option<String> {
    let m = std::fs::metadata(path).ok()?;
    let mtime = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(fnv1a(&format!(
        "{}|{}|{mtime}|{max}",
        path.display(),
        m.len()
    )))
}

fn ext(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Loads (or builds and caches) a thumbnail that fits in `max`×`max`.
pub fn load(path: &Path, kind: MediaKind, max: u32, cache: Option<&Path>) -> Option<Thumb> {
    let cached = cache
        .zip(cache_key(path, max))
        .map(|(d, k)| d.join(format!("{k}.png")));
    if let Some(c) = &cached {
        if let Ok(img) = image::open(c) {
            return Some(Thumb::from_image(img, max));
        }
    }
    let thumb = match kind {
        MediaKind::Photo => photo(path, max),
        MediaKind::Video => video(path, max),
    }?;
    if let (Some(c), Some(png)) = (&cached, thumb.to_png()) {
        if let Some(dir) = c.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(c, png);
    }
    Some(thumb)
}

fn photo(path: &Path, max: u32) -> Option<Thumb> {
    if matches!(ext(path).as_str(), "heic" | "heif") {
        return heif(path, max);
    }
    let img = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    Some(Thumb::from_image(img, max))
}

#[cfg(feature = "heif")]
fn heif(path: &Path, max: u32) -> Option<Thumb> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
    let lib = LibHeif::new();
    let ctx = HeifContext::read_from_file(path.to_str()?).ok()?;
    let handle = ctx.primary_image_handle().ok()?;
    let img = lib
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
        .ok()?;
    let planes = img.planes();
    let p = planes.interleaved?;
    let (w, h, stride) = (p.width, p.height, p.stride);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h as usize {
        rgba.extend_from_slice(&p.data[row * stride..row * stride + w as usize * 4]);
    }
    let img = image::RgbaImage::from_raw(w, h, rgba)?;
    Some(Thumb::from_image(image::DynamicImage::ImageRgba8(img), max))
}

#[cfg(not(feature = "heif"))]
fn heif(_path: &Path, _max: u32) -> Option<Thumb> {
    None
}

fn video(path: &Path, max: u32) -> Option<Thumb> {
    if !ffmpeg_available() {
        return None;
    }
    // Try 1 s in (skips black first frames), then the very first frame for short clips.
    for seek in ["1", "0"] {
        let out = Command::new("ffmpeg")
            .args(["-v", "error", "-ss", seek, "-i"])
            .arg(path)
            .args([
                "-frames:v",
                "1",
                "-vf",
                &format!("scale={max}:{max}:force_original_aspect_ratio=decrease"),
                "-f",
                "image2pipe",
                "-vcodec",
                "png",
                "-",
            ])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if out.status.success() && !out.stdout.is_empty() {
            if let Ok(img) = image::load_from_memory(&out.stdout) {
                return Some(Thumb::from_image(img, max));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
        let p = dir.join(name);
        image::RgbImage::from_pixel(w, h, image::Rgb([10, 120, 200]))
            .save(&p)
            .unwrap();
        p
    }

    #[test]
    fn still_thumbnail_and_cache() {
        let d = tempfile::tempdir().unwrap();
        let p = jpeg(d.path(), "a.jpg", 400, 200);
        let cache = d.path().join("cache");
        let t = load(&p, MediaKind::Photo, 128, Some(&cache)).unwrap();
        assert_eq!((t.width, t.height), (128, 64));
        assert_eq!(t.rgba.len(), 128 * 64 * 4);
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 1);
        // Second load comes from the cache.
        assert_eq!(load(&p, MediaKind::Photo, 128, Some(&cache)).unwrap(), t);
        // Wrong extension still decodes (format sniffed).
        let q = d.path().join("b.heic.jpg");
        std::fs::copy(&p, &q).unwrap();
        assert!(load(&q, MediaKind::Photo, 64, None).is_some());
    }

    #[test]
    fn broken_and_missing_files_fall_back() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.jpg");
        std::fs::write(&p, b"not an image").unwrap();
        assert!(load(&p, MediaKind::Photo, 64, Some(d.path())).is_none());
        assert!(load(&d.path().join("missing.png"), MediaKind::Photo, 64, None).is_none());
        let h = d.path().join("x.HEIC");
        std::fs::write(&h, b"not heif").unwrap();
        assert!(load(&h, MediaKind::Photo, 64, None).is_none());
        let v = d.path().join("x.mov");
        std::fs::write(&v, b"not a video").unwrap();
        assert!(load(&v, MediaKind::Video, 64, None).is_none());
        assert!(heif_supported() || !cfg!(feature = "heif"));
    }

    #[test]
    fn video_frame_with_ffmpeg() {
        if !ffmpeg_available() {
            eprintln!("ffmpeg not installed; skipping");
            return;
        }
        let d = tempfile::tempdir().unwrap();
        let v = d.path().join("clip.mp4");
        let ok = Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=2:size=320x240:rate=10",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&v)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let t = load(&v, MediaKind::Video, 100, None).unwrap();
        assert_eq!((t.width, t.height), (100, 75));
    }

    #[test]
    fn cache_dir_shape() {
        assert!(default_cache_dir().ends_with("unswiper/thumbs"));
    }
}
