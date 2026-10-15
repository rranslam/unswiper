// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! Video thumbnails through GStreamer: `uridecodebin ! videoconvert ! videoscale ! appsink`,
//! seek to 1 s (or 10% of shorter clips), grab one RGBA frame. Which codecs work depends on
//! the installed plugins (H.264/HEVC need gst-libav or a hardware decoder); anything that
//! cannot be decoded returns `None` and the UI shows the video placeholder.

use crate::thumbs::Thumb;
use std::path::Path;

#[cfg(feature = "gstreamer")]
mod imp {
    use super::*;
    use gst::prelude::*;
    use gst_video::prelude::*;
    use std::sync::OnceLock;
    use std::time::Duration;

    const TIMEOUT: Duration = Duration::from_secs(8);

    pub fn available() -> bool {
        static OK: OnceLock<bool> = OnceLock::new();
        *OK.get_or_init(|| {
            gst::init().is_ok() && gst::ElementFactory::find("uridecodebin").is_some()
        })
    }

    /// Seek target: 1 s in (skips black lead-in frames), or 10% of clips shorter than 10 s.
    pub fn seek_target(duration: Option<Duration>) -> Duration {
        match duration {
            Some(d) if d < Duration::from_secs(10) => d / 10,
            _ => Duration::from_secs(1),
        }
    }

    pub fn frame(path: &Path, max: u32) -> Option<Thumb> {
        if !available() {
            return None;
        }
        let uri = gst::glib::filename_to_uri(path.canonicalize().ok()?, None).ok()?;
        let pipeline = gst::parse::launch(&format!(
            "uridecodebin uri=\"{uri}\" ! videoconvert ! videoscale ! video/x-raw,format=RGBA,pixel-aspect-ratio=1/1 ! appsink name=sink sync=false max-buffers=1 drop=true"
        ))
        .ok()?
        .downcast::<gst::Pipeline>()
        .ok()?;
        let r = grab(&pipeline, max);
        let _ = pipeline.set_state(gst::State::Null);
        r
    }

    /// The clip's length, or `None` when it can't be opened or decoded.
    pub fn duration(path: &Path) -> Option<Duration> {
        if !available() {
            return None;
        }
        let uri = gst::glib::filename_to_uri(path.canonicalize().ok()?, None).ok()?;
        let p = gst::parse::launch(&format!("uridecodebin uri=\"{uri}\" ! fakesink sync=false"))
            .ok()?
            .downcast::<gst::Pipeline>()
            .ok()?;
        let r = wait_paused(&p).and_then(|_| p.query_duration::<gst::ClockTime>());
        let _ = p.set_state(gst::State::Null);
        r.map(|d| Duration::from_nanos(d.nseconds()))
            .filter(|d| !d.is_zero())
    }

    /// Test helper: encodes a 2 s colour-bar clip (H.264/MP4 when an encoder is installed,
    /// else WebM or Ogg). `None` when GStreamer has no usable encoder.
    #[doc(hidden)]
    pub fn make_test_clip(dir: &Path) -> Option<std::path::PathBuf> {
        if !available() {
            return None;
        }
        for (enc, mux, ext) in [
            ("x264enc", "mp4mux", "mp4"),
            ("openh264enc ! h264parse", "mp4mux", "mp4"),
            ("vp8enc deadline=1", "webmmux", "webm"),
            ("theoraenc", "oggmux", "ogv"),
        ] {
            let name = enc.split(' ').next().unwrap();
            if gst::ElementFactory::find(name).is_none() || gst::ElementFactory::find(mux).is_none()
            {
                continue;
            }
            let out = dir.join(format!("clip.{ext}"));
            let p = gst::parse::launch(&format!(
                "videotestsrc num-buffers=40 pattern=smpte ! video/x-raw,width=320,height=240,framerate=20/1 ! videoconvert ! {enc} ! {mux} ! filesink location=\"{}\"",
                out.display()
            ))
            .ok()?;
            p.set_state(gst::State::Playing).ok()?;
            let bus = p.bus()?;
            let done = bus.timed_pop_filtered(
                gst::ClockTime::from_seconds(30),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            );
            let _ = p.set_state(gst::State::Null);
            if matches!(done.map(|m| m.type_()), Some(gst::MessageType::Eos)) {
                return Some(out);
            }
        }
        None
    }

    fn wait_paused(p: &gst::Pipeline) -> Option<()> {
        p.set_state(gst::State::Paused).ok()?;
        match p.state(gst::ClockTime::from_mseconds(TIMEOUT.as_millis() as u64)) {
            (Ok(_), gst::State::Paused, _) => Some(()),
            _ => None,
        }
    }

    fn grab(p: &gst::Pipeline, max: u32) -> Option<Thumb> {
        let sink = p.by_name("sink")?.downcast::<gst_app::AppSink>().ok()?;
        wait_paused(p)?;
        let dur = p
            .query_duration::<gst::ClockTime>()
            .map(|d| Duration::from_nanos(d.nseconds()));
        let target = seek_target(dur);
        if !target.is_zero()
            && p.seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::ClockTime::from_nseconds(target.as_nanos() as u64),
            )
            .is_ok()
        {
            let _ = p.state(gst::ClockTime::from_mseconds(TIMEOUT.as_millis() as u64));
        }
        let sample =
            sink.try_pull_preroll(gst::ClockTime::from_mseconds(TIMEOUT.as_millis() as u64))?;
        let info = gst_video::VideoInfo::from_caps(sample.caps()?).ok()?;
        let buf = sample.buffer()?;
        let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buf, &info).ok()?;
        let (w, h) = (info.width(), info.height());
        let stride = frame.plane_stride()[0] as usize;
        let data = frame.plane_data(0).ok()?;
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for row in 0..h as usize {
            rgba.extend_from_slice(&data[row * stride..row * stride + w as usize * 4]);
        }
        let img = image::RgbaImage::from_raw(w, h, rgba)?;
        Some(Thumb::from_image(image::DynamicImage::ImageRgba8(img), max))
    }
}

#[cfg(feature = "gstreamer")]
pub use imp::{available, duration, frame, make_test_clip, seek_target};

#[cfg(not(feature = "gstreamer"))]
pub fn available() -> bool {
    false
}

#[cfg(not(feature = "gstreamer"))]
pub fn frame(_path: &Path, _max: u32) -> Option<Thumb> {
    None
}

#[cfg(not(feature = "gstreamer"))]
pub fn duration(_path: &Path) -> Option<std::time::Duration> {
    None
}

#[cfg(not(feature = "gstreamer"))]
#[doc(hidden)]
pub fn make_test_clip(_dir: &Path) -> Option<std::path::PathBuf> {
    None
}

#[cfg(all(test, feature = "gstreamer"))]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn frame_from_generated_clip() {
        let d = tempfile::tempdir().unwrap();
        let Some(clip) = make_test_clip(d.path()) else {
            eprintln!("no GStreamer encoder/muxer available; skipping");
            return;
        };
        let t = frame(&clip, 100).expect("decoded a frame");
        assert_eq!((t.width, t.height), (100, 75));
        assert_eq!(t.rgba.len(), 100 * 75 * 4);
        // Not all black: videotestsrc's colour bars.
        assert!(t
            .rgba
            .chunks(4)
            .any(|p| p[0] > 100 || p[1] > 100 || p[2] > 100));
        let len = duration(&clip).expect("duration");
        assert!(
            len > Duration::from_millis(1500) && len < Duration::from_millis(2500),
            "{len:?}"
        );
        // Through the public loader too (cached).
        let cache = d.path().join("cache");
        assert!(
            crate::thumbs::load(&clip, unswiper_core::MediaKind::Video, 64, Some(&cache)).is_some()
        );
    }

    #[test]
    fn undecodable_files_give_none() {
        let d = tempfile::tempdir().unwrap();
        let bad = d.path().join("x.mov");
        std::fs::write(&bad, b"not a video at all").unwrap();
        assert!(frame(&bad, 64).is_none());
        assert!(frame(&d.path().join("missing.mp4"), 64).is_none());
        assert!(duration(&bad).is_none());
        assert!(duration(&d.path().join("missing.mp4")).is_none());
    }

    #[test]
    fn seek_targets() {
        assert_eq!(seek_target(None), Duration::from_secs(1));
        assert_eq!(
            seek_target(Some(Duration::from_secs(60))),
            Duration::from_secs(1)
        );
        assert_eq!(
            seek_target(Some(Duration::from_secs(2))),
            Duration::from_millis(200)
        );
    }
}
